//! Model bake-off: scores a cleanup model on `eval/fluency.tsv` (category, input, reference).
//! Pure scoring lives here so it can be unit-tested; `run` drives a real model.

use std::collections::BTreeMap;
use voicio_engine::cleanup::{negations, rule_clean};
use voicio_engine::word_errors;

pub struct Case {
    pub category: String,
    pub input: String,
    pub reference: String,
}

pub fn parse(tsv: &str) -> Vec<Case> {
    tsv.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let mut p = l.splitn(3, '\t');
            Some(Case { category: p.next()?.into(), input: p.next()?.into(), reference: p.next()?.into() })
        })
        .collect()
}

/// What one system produced for one case.
pub struct Outcome {
    pub text: String,
    /// The model's own answer before the guardrail (None when it was not asked, e.g. very short input).
    pub model_said: Option<String>,
    pub used_model: bool,
    pub millis: f64,
}

#[derive(Default, Clone)]
pub struct Tally {
    pub n: usize,
    pub exact: usize,
    pub word_errors: usize,
    pub ref_words: usize,
}

impl Tally {
    fn add(&mut self, reference: &str, got: &str) {
        let (errors, len) = word_errors(reference, got);
        self.n += 1;
        self.word_errors += errors;
        self.ref_words += len;
        self.exact += usize::from(got.trim() == reference.trim());
    }
    pub fn wer(&self) -> f64 {
        100.0 * self.word_errors as f64 / self.ref_words.max(1) as f64
    }
    pub fn exact_pct(&self) -> f64 {
        100.0 * self.exact as f64 / self.n.max(1) as f64
    }
}

/// A meaning change: a negation gained or lost, or a number that is not in the reference.
pub fn meaning_changed(reference: &str, got: &str) -> bool {
    let digits = |t: &str| -> Vec<String> { t.split(|c: char| !c.is_ascii_digit()).filter(|r| !r.is_empty()).map(String::from).collect() };
    let want = digits(reference);
    negations(reference) != negations(got) || digits(got).iter().any(|d| !want.contains(d))
}

#[derive(Default)]
pub struct Report {
    pub by_category: BTreeMap<String, Tally>,
    pub total: Tally,
    /// Already-correct sentences changed at all (over-editing).
    pub overedited: usize,
    pub correct_cases: usize,
    pub meaning_changes: usize,
    /// Model answers the guardrail refused.
    pub rejected: usize,
    pub asked: usize,
    pub millis: Vec<f64>,
}

pub fn score(cases: &[Case], outcomes: &[Outcome]) -> Report {
    let mut r = Report::default();
    for (case, out) in cases.iter().zip(outcomes) {
        r.by_category.entry(case.category.clone()).or_default().add(&case.reference, &out.text);
        r.total.add(&case.reference, &out.text);
        if case.category == "correct" {
            r.correct_cases += 1;
            r.overedited += usize::from(out.text.trim() != case.input.trim());
        }
        r.meaning_changes += usize::from(meaning_changed(&case.reference, &out.text));
        if let Some(said) = &out.model_said {
            r.asked += 1;
            r.rejected += usize::from(!out.used_model || !voicio_engine::cleanup::guardrail(&case.input, said));
        }
        r.millis.push(out.millis);
    }
    r
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() { return 0.0; }
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

pub fn print(name: &str, r: &Report) {
    println!("\n== {name}");
    println!("{:<12} {:>4} {:>8} {:>8}", "category", "n", "exact%", "WER%");
    for (cat, t) in &r.by_category {
        println!("{:<12} {:>4} {:>8.1} {:>8.1}", cat, t.n, t.exact_pct(), t.wer());
    }
    println!("{:<12} {:>4} {:>8.1} {:>8.1}", "ALL", r.total.n, r.total.exact_pct(), r.total.wer());
    let mut ms = r.millis.clone();
    ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "over-edited correct sentences: {}/{}   meaning changes: {}   guardrail rejected: {}/{}   latency p50 {:.0} ms p95 {:.0} ms",
        r.overedited, r.correct_cases, r.meaning_changes, r.rejected, r.asked, percentile(&ms, 0.5), percentile(&ms, 0.95)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(cat: &str, input: &str, reference: &str) -> Case {
        Case { category: cat.into(), input: input.into(), reference: reference.into() }
    }
    fn out(text: &str) -> Outcome {
        Outcome { text: text.into(), model_said: None, used_model: true, millis: 10.0 }
    }

    #[test]
    fn exact_match_and_word_errors_are_counted() {
        let cases = [case("grammar", "he go", "He goes."), case("grammar", "x", "Yes.")];
        let r = score(&cases, &[out("He goes."), out("No.")]);
        assert_eq!(r.total.exact, 1);
        assert_eq!(r.total.word_errors, 1);
        assert!((r.total.exact_pct() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn over_editing_a_correct_sentence_is_flagged() {
        let cases = [case("correct", "I will go.", "I will go."), case("correct", "We left.", "We left.")];
        let r = score(&cases, &[out("I will go."), out("We have left.")]);
        assert_eq!((r.overedited, r.correct_cases), (1, 2));
    }

    #[test]
    fn dropped_negations_and_invented_numbers_are_meaning_changes() {
        assert!(meaning_changed("I do not want it.", "I want it."));
        assert!(meaning_changed("We need fifteen.", "We need 15."));
        assert!(!meaning_changed("We need 15 licenses.", "We need 15 licenses."));
        assert!(!meaning_changed("It isn't ready.", "It isn't ready."));
    }

    #[test]
    fn tsv_parsing_skips_comments() {
        let cases = parse("# header\ngrammar\ta b c\tA b c.\n\nshort\tyes\tYes.\n");
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[1].category, "short");
    }
}

/// Runs the rules baseline and (if given) a model over the file and prints both reports.
pub fn run(file: &std::path::Path, cleaner: Option<(&str, &voicio_engine::cleanup::Cleaner)>, verbose: bool, style: voicio_engine::style::CleanupStyle) {
    let cases = parse(&std::fs::read_to_string(file).unwrap_or_else(|e| panic!("{}: {e}", file.display())));
    let baseline: Vec<Outcome> = cases.iter().map(|c| Outcome { text: rule_clean(&c.input), model_said: None, used_model: false, millis: 0.0 }).collect();
    print("rules only (no model)", &score(&cases, &baseline));
    let Some((name, cleaner)) = cleaner else { return };
    let mut outcomes = Vec::new();
    let mut raw_outcomes = Vec::new();
    for case in &cases {
        let t = std::time::Instant::now();
        let (cleaned, said) = cleaner.clean_traced(&case.input, &style.resolve(&case.input));
        let millis = t.elapsed().as_secs_f64() * 1000.0;
        if verbose && cleaned.text.trim() != case.reference.trim() {
            println!("[{}] {}\n   model: {:?}\n   got:   {}\n   want:  {}", case.category, case.input, said, cleaned.text, case.reference);
        }
        let raw_text = said.first().filter(|s| !s.trim().is_empty()).cloned().unwrap_or_else(|| cleaned.text.clone());
        raw_outcomes.push(Outcome { text: raw_text, model_said: None, used_model: true, millis });
        outcomes.push(Outcome { text: cleaned.text, model_said: said.first().cloned().or_else(|| (!cleaned.used_model && case.input.split_whitespace().count() >= 4).then(String::new)), used_model: cleaned.used_model, millis });
    }
    print(&format!("{name}: what the user gets (model + guardrail + rules fallback)"), &score(&cases, &outcomes));
    print(&format!("{name}: model answers with NO guardrail (shows model quality)"), &score(&cases, &raw_outcomes));
}
