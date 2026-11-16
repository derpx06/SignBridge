//! Post-dictation cleanup: "S1-mini" by "Superwhisper" (Qwen3-0.6B fine-tune, Apache-2.0 plus a
//! naming clause) on llama.cpp, behind a guardrail that falls back to rule cleanup whenever the
//! model output drifts from what was said. Accuracy beats polish: we never ship invented words.

use llama_cpp_2::context::params::{KvCacheType, LlamaContextParams};
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{LlamaChatMessage, LlamaChatTemplate};
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;
use std::collections::HashSet;
use std::num::NonZeroU32;
use std::path::Path;

use crate::style::CleanupStyle;

pub const MODEL_FILE: &str = "s1-mini/s1-mini-q4_k_m.gguf";
/// The same model without its duplicate output table (127 MB less memory). Made once from [`MODEL_FILE`].
pub const TIED_FILE: &str = "s1-mini/s1-mini-tied.gguf";
pub const LABEL: &str = "S1-mini by Superwhisper (Q4_K_M)";

// Exact training-time prompt; the model card warns that rewording it causes garbled output.
const SYSTEM: &str = "You are a text normalizer for speech-to-text transcripts. The input begins with a control line specifying the styling, structure, and context settings; clean the transcript to match those settings and output only the cleaned text.";
const CONTEXT_TOKENS: u32 = 1024;
const UBATCH: usize = 256; // prompt-processing batch; compute buffers scale with it
/// Shorter dictations are left to the rules: ASR output is already punctuated, and tiny inputs
/// are where a small LLM is most likely to reply instead of echo.
const MIN_WORDS_FOR_MODEL: usize = 4;
/// Unpunctuated runs longer than this are split so one chunk never exceeds the context.
const MAX_CHUNK_WORDS: usize = 150;
const MAX_INPUT_TOKENS: usize = 400; // input + output (<= ~2x input) must fit the context

/// How the model is prompted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// S1-mini's exact training prompt (ChatML plus a control line).
    S1,
    /// Any instruction-tuned model: its own chat template plus a strict fix-only instruction.
    Chat,
}

/// The instruction for general models: fix the language, change nothing else.
const FIX_ONLY: &str = "Fix the grammar, punctuation, capitalisation and fluency of this speech transcript. Remove filler words and stutters. Write numbers, times and emails the usual way. Keep every idea, name, number and negation exactly as said. Do not add, explain or reword anything else. Output only the corrected text.";

/// Worked examples for general models (none of them appear in the evaluation set).
const FEW_SHOT: &[(&str, &str)] = &[
    ("um so me and my friend was going to the the market yesterday", "So my friend and I were going to the market yesterday."),
    ("the report dont have the numbers i asked for", "The report doesn't have the numbers I asked for."),
    ("call me at five thirty p m or email jo at example dot com", "Call me at 5:30 p.m. or email jo@example.com."),
    ("we should not ship it on friday", "We should not ship it on Friday."),
];

/// Memory-mapped weights with no repacked copy (measured on a phone: 761 -> 391 MB, same output and
/// speed). `VOICIO_REPACK=1` restores the old behaviour (anonymous repacked weights) for comparison.
fn load_params() -> LlamaModelParams {
    if std::env::var_os("VOICIO_REPACK").is_some() {
        LlamaModelParams::default().with_use_mmap(false)
    } else {
        LlamaModelParams::default().with_use_mmap(true).with_use_extra_bufts(false)
    }
}

pub struct Cleaner {
    backend: LlamaBackend,
    model: LlamaModel,
    threads: i32,
    style: Style,
    template: Option<LlamaChatTemplate>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cleaned {
    pub text: String,
    /// False when the model output was rejected (or not attempted) and rule cleanup was used.
    pub used_model: bool,
}

impl Cleaner {
    pub fn load(models_dir: &Path, threads: i32) -> Result<Cleaner, String> {
        let original = models_dir.join(MODEL_FILE);
        let tied = models_dir.join(TIED_FILE);
        // Prefer the slimmed copy; make it once if only the downloaded file exists. If that fails
        // (disk full, odd file) the original still works, just with more memory.
        if !tied.is_file() && original.is_file() {
            let _ = crate::gguf::drop_tensor(&original, &tied, "output.weight");
        }
        Self::load_file(if tied.is_file() { &tied } else { &original }, threads, Style::S1)
    }

    /// Loads any GGUF. Used by the bake-off to compare small models on the same dictations.
    pub fn load_file(path: &Path, threads: i32, style: Style) -> Result<Cleaner, String> {
        let path = path.to_path_buf();
        if !path.is_file() {
            return Err(format!(
                "missing model file {} (run engine/scripts/fetch-models.sh)",
                path.display()
            ));
        }
        let mut backend = LlamaBackend::init().map_err(|e| e.to_string())?;
        backend.void_logs();
        // No mmap: llama.cpp repacks Q4_K weights into an AVX2/NEON-friendly layout in anonymous
        // memory; with mmap on, the file pages stay resident too (measured 738 MB vs 535 MB, same speed).
        let params = load_params();
        let model = LlamaModel::load_from_file(&backend, &path, &params)
            .map_err(|e| format!("cleanup model: {e}"))?;
        let template = if style == Style::Chat { Some(model.chat_template(None).map_err(|e| format!("no chat template: {e}"))?) } else { None };
        Ok(Cleaner {
            backend,
            model,
            threads,
            style,
            template,
        })
    }

    /// Cleans a whole dictation. Long input is cleaned sentence-chunk by chunk.
    pub fn clean(&self, raw: &str) -> Cleaned {
        self.clean_traced(raw, &CleanupStyle::default()).0
    }

    /// Cleans in the given register, structure and context.
    pub fn clean_with(&self, raw: &str, style: &CleanupStyle) -> Cleaned {
        self.clean_traced(raw, style).0
    }

    /// Like [`clean`](Self::clean), plus what the model itself said for each chunk before the
    /// guardrail judged it (empty when the model was not asked). For evaluation.
    pub fn clean_traced(&self, raw: &str, style: &CleanupStyle) -> (Cleaned, Vec<String>) {
        // "new paragraph" / "new line" are layout commands: clean each part alone and join them.
        let parts = if style.is_voice_on() { split_breaks(raw) } else { vec![(String::new(), raw.to_string())] };
        if parts.len() < 2 {
            return self.clean_one(raw, style);
        }
        let (mut text, mut said, mut used) = (String::new(), Vec::new(), true);
        for (sep, part) in parts {
            let (c, s) = self.clean_one(&part, style);
            if c.text.is_empty() {
                continue;
            }
            if !text.is_empty() {
                text.push_str(if sep.is_empty() { " " } else { &sep });
            }
            text.push_str(&c.text);
            used &= c.used_model || part.split_whitespace().count() < MIN_WORDS_FOR_MODEL;
            said.extend(s);
        }
        (Cleaned { text, used_model: used }, said)
    }

    fn clean_one(&self, raw: &str, style: &CleanupStyle) -> (Cleaned, Vec<String>) {
        let (c, said) = self.clean_model(raw, style);
        (Cleaned { text: finish(&c.text, style), used_model: c.used_model }, said)
    }

    fn clean_model(&self, raw: &str, style: &CleanupStyle) -> (Cleaned, Vec<String>) {
        let ruled = rule_clean(raw);
        if ruled.is_empty() || ruled.split_whitespace().count() < MIN_WORDS_FOR_MODEL {
            return (Cleaned { text: ruled, used_model: false }, Vec::new());
        }
        let mut pieces = Vec::new();
        let mut said = Vec::new();
        let mut used_model = true;
        for chunk in self.chunks(raw) {
            match self.generate(&chunk, style) {
                Ok(out) if guardrail(&chunk, &out) => {
                    said.push(out.clone());
                    pieces.push(out)
                }
                other => {
                    eprintln!("voicio-engine: cleanup guardrail kept the rule-based text ({})", match &other { Ok(_) => "model output rejected".to_string(), Err(e) => e.clone() });
                    if let Ok(out) = other {
                        said.push(out);
                    }
                    used_model = false;
                    pieces.push(rule_clean(&chunk));
                }
            }
        }
        let text = pieces.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ");
        (Cleaned { text, used_model }, said)
    }

    fn token_count(&self, text: &str) -> usize {
        self.model
            .vocab()
            .tokenize(text.as_bytes(), false, false)
            .len()
    }

    /// Splits on sentence ends so each chunk stays under the input budget.
    fn chunks(&self, raw: &str) -> Vec<String> {
        if self.token_count(raw) <= MAX_INPUT_TOKENS {
            return vec![raw.to_string()];
        }
        let mut chunks = vec![String::new()];
        let sentences = raw.split_inclusive(['.', '?', '!']).flat_map(split_long);
        for sentence in sentences {
            let current = chunks.last_mut().expect("non-empty");
            if !current.is_empty()
                && self.token_count(current) + self.token_count(&sentence) > MAX_INPUT_TOKENS
            {
                chunks.push(sentence.trim_start().to_string());
            } else {
                current.push_str(&sentence);
            }
        }
        chunks
    }

    fn generate(&self, raw: &str, style: &CleanupStyle) -> Result<String, String> {
        let control = style.control_line();
        let prompt = match (&self.style, &self.template) {
            (Style::Chat, Some(template)) => {
                let mut messages = vec![LlamaChatMessage::new("system".into(), FIX_ONLY.into()).map_err(|e| e.to_string())?];
                if std::env::var_os("VOICIO_NO_FEWSHOT").is_none() {
                    for (said, fixed) in FEW_SHOT {
                        messages.push(LlamaChatMessage::new("user".into(), (*said).into()).map_err(|e| e.to_string())?);
                        messages.push(LlamaChatMessage::new("assistant".into(), (*fixed).into()).map_err(|e| e.to_string())?);
                    }
                }
                messages.push(LlamaChatMessage::new("user".into(), raw.into()).map_err(|e| e.to_string())?);
                let mut p = self.model.apply_chat_template(template, &messages, true).map_err(|e| e.to_string())?;
                // Models that think before answering (Qwen3): close the thinking block so they answer straight away.
                if template.to_str().is_ok_and(|t| t.contains("<think>")) && !p.trim_end().ends_with("</think>") {
                    p.push_str("<think>\n\n</think>\n\n");
                }
                p
            }
            _ => format!(
                "<|im_start|>system\n{SYSTEM}<|im_end|>\n<|im_start|>user\n{control}\n{raw}<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n"
            ),
        };
        #[cfg(debug_assertions)]
        if std::env::var_os("VOICIO_DEBUG_PROMPT").is_some() { eprintln!("PROMPT: {prompt:?}"); }
        let vocab = self.model.vocab();
        let tokens = vocab.tokenize(prompt.as_bytes(), self.style == Style::Chat, true); // general models expect their BOS token
        let max_new = (self.token_count(raw) * 2 + 16)
            .min(CONTEXT_TOKENS as usize - tokens.len().min(CONTEXT_TOKENS as usize));
        if max_new < 8 {
            return Err("input too long for the cleanup context".into());
        }
        // A fresh context per call, sized to this prompt + output: its q8_0 KV cache (~57 KB/token)
        // and compute buffers are freed as soon as we return.
        let n_ctx = (tokens.len() + max_new).next_multiple_of(64) as u32;
        let params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(n_ctx))
            .with_n_batch(UBATCH as u32)
            .with_n_ubatch(UBATCH as u32)
            .with_n_threads(self.threads)
            .with_n_threads_batch(self.threads)
            .with_flash_attention_policy(llama_cpp_sys_2::LLAMA_FLASH_ATTN_TYPE_ENABLED)
            .with_type_k(KvCacheType::Q8_0)
            .with_type_v(KvCacheType::Q8_0);
        let mut ctx = self
            .model
            .new_context(&self.backend, params)
            .map_err(|e| e.to_string())?;

        let mut batch = LlamaBatch::new(UBATCH, 1);
        for (i, chunk) in tokens.chunks(UBATCH).enumerate() {
            batch.clear();
            let last_chunk = (i + 1) * UBATCH >= tokens.len();
            for (j, &token) in chunk.iter().enumerate() {
                let pos = (i * UBATCH + j) as i32;
                batch
                    .add(token, pos, &[0], last_chunk && j == chunk.len() - 1)
                    .map_err(|e| e.to_string())?;
            }
            ctx.decode(&mut batch).map_err(|e| e.to_string())?;
        }

        let mut sampler = LlamaSampler::greedy();
        let mut out = Vec::new();
        let mut pos = tokens.len() as i32;
        for _ in 0..max_new {
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            if vocab.is_eog(token) {
                let text = String::from_utf8_lossy(&out).trim().to_string();
                return Ok(text);
            }
            out.extend(vocab.token_to_piece(token, false, None));
            batch.clear();
            batch
                .add(token, pos, &[0], true)
                .map_err(|e| e.to_string())?;
            pos += 1;
            ctx.decode(&mut batch).map_err(|e| e.to_string())?;
        }
        Err("cleanup did not finish (possible loop)".into())
    }
}

/// Rules only (no model): fillers, stutters, grammar slips, layout commands. Also used when the model is unavailable.
pub fn rules(raw: &str, style: &CleanupStyle) -> String {
    let parts = if style.is_voice_on() { split_breaks(raw) } else { vec![(String::new(), raw.to_string())] };
    let mut text = String::new();
    for (sep, part) in parts {
        let c = finish(&rule_clean(&part), style);
        if c.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push_str(if sep.is_empty() { " " } else { &sep });
        }
        text.push_str(&c);
    }
    text
}

/// Deterministic polish after the model or the rules: grammar slips, and capital letters for "casual".
pub fn finish(text: &str, style: &CleanupStyle) -> String {
    let polished = crate::grammar::polish(text);
    if style.styling == crate::style::Styling::Casual { crate::grammar::capitalise(&polished) } else { polished }
}

/// Splits at spoken "new paragraph" / "new line": (separator before the part, the part).
fn split_breaks(raw: &str) -> Vec<(String, String)> {
    let words: Vec<&str> = raw.split_whitespace().collect();
    let key = |w: &str| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
    let mut parts = vec![(String::new(), Vec::new())];
    let mut i = 0;
    while i < words.len() {
        if i + 1 < words.len() && key(words[i]) == "new" && matches!(key(words[i + 1]).as_str(), "paragraph" | "line") {
            let sep = if key(words[i + 1]) == "paragraph" { "\n\n" } else { "\n" };
            parts.push((sep.to_string(), Vec::new()));
            i += 2;
        } else {
            parts.last_mut().expect("non-empty").1.push(words[i]);
            i += 1;
        }
    }
    parts.into_iter().filter(|(_, w)| !w.is_empty()).map(|(s, w)| (s, w.join(" "))).collect()
}

/// Cuts a run with no sentence punctuation into pieces of at most `MAX_CHUNK_WORDS` words.
fn split_long(sentence: &str) -> Vec<String> {
    let words: Vec<&str> = sentence.split_whitespace().collect();
    if words.len() <= MAX_CHUNK_WORDS {
        return vec![sentence.to_string()];
    }
    words.chunks(MAX_CHUNK_WORDS).map(|w| format!("{} ", w.join(" "))).collect()
}

const FILLERS: &[&str] = &[
    "um", "umm", "uh", "uhh", "uhm", "erm", "er", "hmm", "mm", "mhm", "ah",
];

/// Words that are real when doubled ("had had", "very very", "bye bye"), so never stutters.
const LEGIT_REPEATS: &[&str] = &["had", "that", "very", "bye", "no", "so", "well", "really", "many", "far"];

const NUMBER_WORDS: &[&str] = &[
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve",
    "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen", "twenty", "thirty",
    "forty", "fifty", "sixty", "seventy", "eighty", "ninety", "hundred", "thousand", "million", "billion",
    "first", "second", "third", "half", "quarter", "dozen", "point", "percent",
];

fn normalize(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric() || *c == '\'')
        .collect::<String>()
        .to_lowercase()
}

/// Deterministic cleanup used when the model is unavailable or its output is rejected:
/// drops filler words and immediate stutters ("the the"), keeps everything else verbatim.
pub fn rule_clean(raw: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for word in raw.split_whitespace() {
        let n = normalize(word);
        if FILLERS.contains(&n.as_str()) {
            continue;
        }
        if out
            .last()
            .is_some_and(|prev| normalize(prev) == n && !n.is_empty() && !LEGIT_REPEATS.contains(&n.as_str()))
        {
            continue;
        }
        out.push(word);
    }
    let mut text = out.join(" ");
    if let Some(first) = text.chars().next() {
        if first.is_lowercase() {
            text = first.to_uppercase().collect::<String>() + &text[first.len_utf8()..];
        }
    }
    text
}

/// Accepts model output only if it does not add content that was never spoken.
/// Deletions are fine (fillers, self-corrections); formatting like "forty two" -> "42" is fine.
pub fn guardrail(raw: &str, cleaned: &str) -> bool {
    let spoken: HashSet<String> = raw.split_whitespace().map(normalize).collect();
    let content: Vec<String> = rule_clean(raw)
        .split_whitespace()
        .map(normalize)
        .filter(|w| !w.is_empty())
        .collect();
    let words: Vec<String> = cleaned
        .split_whitespace()
        .map(normalize)
        .filter(|w| !w.is_empty())
        .collect();
    if content.is_empty() {
        return words.is_empty();
    }
    // Too short: the model dropped real speech rather than fillers. Writing "twenty three thousand"
    // as 23,000 or "john dot smith at example dot com" as an address legitimately collapses many
    // spoken words into one, and a self-correction ("forty two no sorry forty three") drops some,
    // so allow a much shorter answer when the output holds such formatting.
    let formatted = cleaned.chars().any(|c| c.is_ascii_digit() || matches!(c, '@' | '$' | '%'));
    let floor = if formatted { 1 } else { 5 };
    if words.len() * 10 < content.len() * floor {
        return false;
    }
    // "not"/"never"/"n't" flipping is the worst thing a cleanup can do: the counts must match.
    // ("no" is exempt: "no wait, make that Thursday" is a legitimate self-correction.)
    if negations(raw) != negations(cleaned) {
        return false;
    }
    if !numbers_consistent(raw, cleaned) {
        return false;
    }
    let invented = words
        .iter()
        .filter(|w| {
            w.chars().count() >= 4 && !w.chars().any(|c| c.is_ascii_digit()) && !spoken.contains(*w)
        })
        // Allow inflections/expansions formatting produces ("going" from "gonna", "cannot" from "can't").
        .filter(|w| {
            !spoken
                .iter()
                .any(|s| w.chars().zip(s.chars()).take_while(|(a, b)| a == b).count() >= 3)
        })
        .count();
    invented <= 2 && invented * 10 <= words.len().max(10)
}

pub fn negations(text: &str) -> usize {
    text.split_whitespace()
        .map(normalize)
        .filter(|w| {
            let bare = w.replace('\'', ""); // "dont" and "don't" are the same word
            matches!(bare.as_str(), "not" | "never" | "cannot" | "without" | "nobody" | "nothing") || NEGATED.contains(&bare.as_str()) || w.ends_with("n't")
        })
        .count()
}

/// Contractions with their apostrophe removed (speech-to-text often drops it).
const NEGATED: &[&str] = &[
    "dont", "doesnt", "didnt", "cant", "wont", "isnt", "arent", "wasnt", "werent", "havent", "hasnt", "hadnt",
    "shouldnt", "couldnt", "wouldnt", "mustnt", "neednt", "aint",
];

/// Digits in the output must have been spoken, either as digits or as number words; digits that
/// were spoken as digits must survive.
fn numbers_consistent(raw: &str, cleaned: &str) -> bool {
    let runs = |t: &str| -> Vec<String> {
        t.split(|c: char| !c.is_ascii_digit()).filter(|r| !r.is_empty()).map(String::from).collect()
    };
    let (spoken, written) = (runs(raw), runs(cleaned));
    if spoken.iter().any(|r| !written.contains(r)) {
        return false;
    }
    let said_number_words = raw.split_whitespace().map(normalize).any(|w| NUMBER_WORDS.contains(&w.as_str()));
    written.is_empty() || !spoken.is_empty() || said_number_words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_clean_drops_fillers_and_stutters() {
        assert_eq!(
            rule_clean("um so the the report is uh ready"),
            "So the report is ready"
        );
        assert_eq!(rule_clean("uh um"), "");
    }

    #[test]
    fn guardrail_accepts_cleanup_and_rejects_invention() {
        let raw = "so um i need to send the the report by uh friday no wait make that thursday";
        assert!(guardrail(raw, "I need to send the report by Thursday."));
        assert!(guardrail(
            "i think the answer is forty two",
            "I think the answer is 42."
        ));
        assert!(guardrail("we're gonna ship it", "We're going to ship it."));
        assert!(!guardrail(
            raw,
            "Please find attached the quarterly financial summary for review."
        ));
        assert!(!guardrail(
            "send the report to maria by friday and copy the whole team",
            "Send it."
        ));
    }

    #[test]
    fn guardrail_rejects_flipped_meaning() {
        // dropped or invented negation
        assert!(!guardrail("i do not want to ship it on friday", "I want to ship it on Friday."));
        assert!(!guardrail("we should ship it on friday", "We should not ship it on Friday."));
        assert!(guardrail("i don't want to ship it on friday", "I do not want to ship it on Friday."));
        assert!(guardrail("i can't make it on friday", "I cannot make it on Friday."));
        // numbers: changed, invented, or lost
        // ponytail: number WORDS are only checked for presence, not value (needs a number-word parser).
        assert!(guardrail("call me at 15 minutes past three", "Call me at 15 minutes past three."));
        assert!(!guardrail("call me at 15 minutes past three", "Call me at 50 minutes past three."));
        assert!(!guardrail("please call me tomorrow morning about it", "Please call me tomorrow at 9 about it."));
    }

    #[test]
    fn guardrail_accepts_legitimate_formatting_and_contractions() {
        // "dont" and "don't" are the same negation
        assert!(guardrail("honestly um i dont really care which one we pick", "Honestly, I don't really care which one we pick."));
        assert!(guardrail("i dont think we should of shipped it before testing", "I don't think we should have shipped it before testing."));
        assert!(!guardrail("i dont think we should ship it", "I think we should ship it."));
        // many spoken words collapse into one written form
        assert!(guardrail("the total came to twenty three thousand four hundred and fifty dollars", "The total came to $23,450."));
        assert!(guardrail("the answer is forty two no sorry forty three", "The answer is 43."));
        assert!(guardrail("send it to john dot smith at example dot com", "Send it to john.smith@example.com."));
        // but a plain answer that drops real speech is still refused
        assert!(!guardrail("send the report to maria by friday and copy the whole team", "Send it."));
    }

    #[test]
    fn short_phrases_skip_the_model_and_legit_repeats_survive() {
        assert_eq!(rule_clean("yes"), "Yes");
        assert_eq!(rule_clean("i had had enough"), "I had had enough");
        assert_eq!(rule_clean("it was very very good"), "It was very very good");
        assert_eq!(rule_clean("we we should go"), "We should go");
    }

    #[test]
    fn spoken_breaks_split_the_dictation() {
        let parts = split_breaks("first point. New paragraph, second point new line third");
        assert_eq!(parts, vec![("".into(), "first point.".into()), ("\n\n".into(), "second point".into()), ("\n".into(), "third".into())]);
    }

    #[test]
    fn long_unpunctuated_text_is_split() {
        let text = vec!["word"; 400].join(" ");
        let parts = split_long(&text);
        assert_eq!(parts.len(), 3);
        assert!(parts.iter().all(|p| p.split_whitespace().count() <= MAX_CHUNK_WORDS));
    }
}
