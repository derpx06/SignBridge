//! Deterministic polish run after the model (and on rule-only dictations): fixes the verb agreement,
//! tense and contraction slips that a 0.6B model leaves in, and capitalises casual text. Every rule
//! is narrow on purpose: when unsure it changes nothing, because a wrong "fix" is worse than a miss.

const CONJ: &[&str] = &["and", "but", "because", "that", "when", "if", "so", "then", "or", "while", "since", "which", "where", "as", "after", "before", "once", "until", "how", "what", "though", "although"];

/// Verbs whose third-person form is regular enough to build with -s/-es (he tell -> he tells).
const BASE: &[&str] = &[
    "tell", "go", "come", "want", "need", "like", "know", "think", "say", "take", "get", "make", "give", "work", "drain", "overheat", "meet", "send", "show", "look", "seem", "keep", "start", "stop", "run", "help", "use", "try", "ask", "call", "write", "read", "leave", "feel", "talk", "move", "live", "love", "hate", "wait", "play", "crash", "fail", "happen", "change", "turn", "open", "close", "return", "send", "log", "load", "sound", "believe", "mean", "tend", "refuse", "forget", "decide", "prefer", "ask", "bring", "buy", "pay", "check", "update", "install",
];

/// Irregular verbs: base, past.
const PAST: &[(&str, &str)] = &[
    ("go", "went"), ("come", "came"), ("tell", "told"), ("take", "took"), ("buy", "bought"), ("send", "sent"), ("give", "gave"), ("meet", "met"), ("write", "wrote"), ("eat", "ate"), ("find", "found"), ("leave", "left"), ("make", "made"), ("get", "got"), ("bring", "brought"), ("see", "saw"),
];

const SUBJECTS: &[&str] = &["i", "you", "he", "she", "we", "they"];

fn core(w: &str) -> String {
    w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'').to_lowercase()
}

/// Replaces the word inside a token, keeping its punctuation and a leading capital.
fn swap(token: &str, new: &str) -> String {
    let start = token.find(|c: char| c.is_alphanumeric() || c == '\'').unwrap_or(0);
    let end = token.rfind(|c: char| c.is_alphanumeric() || c == '\'').map_or(token.len(), |i| i + token[i..].chars().next().map_or(1, char::len_utf8));
    let cap = token[start..].chars().next().is_some_and(char::is_uppercase);
    let mut word = new.to_string();
    if cap {
        word = word[..1].to_uppercase() + &word[1..];
    }
    format!("{}{}{}", &token[..start], word, &token[end..])
}

fn third(base: &str) -> String {
    if base == "go" || base == "do" {
        return format!("{base}es");
    }
    if base.ends_with("ch") || base.ends_with("sh") || base.ends_with('s') || base.ends_with('x') || base.ends_with('z') {
        return format!("{base}es");
    }
    if base.ends_with('y') && !base.ends_with("ay") && !base.ends_with("ey") && !base.ends_with("oy") {
        return format!("{}ies", &base[..base.len() - 1]);
    }
    format!("{base}s")
}

fn base_of_third(word: &str) -> Option<String> {
    BASE.iter().find(|b| third(b) == word).map(|b| (*b).to_string())
}

const CONTRACTIONS: &[(&str, &str)] = &[
    ("dont", "don't"), ("doesnt", "doesn't"), ("didnt", "didn't"), ("cant", "can't"), ("isnt", "isn't"), ("arent", "aren't"), ("wasnt", "wasn't"), ("werent", "weren't"), ("havent", "haven't"), ("hasnt", "hasn't"), ("hadnt", "hadn't"), ("shouldnt", "shouldn't"), ("couldnt", "couldn't"), ("wouldnt", "wouldn't"), ("im", "I'm"), ("ive", "I've"), ("youre", "you're"), ("theyre", "they're"), ("weve", "we've"), ("youve", "you've"), ("theyve", "they've"), ("thats", "that's"), ("whats", "what's"), ("lets", "let's"),
];

/// Fixes one sentence's worth of words in place.
fn fix_sentence(words: &mut [String]) {
    // contractions the speech model spelled without the apostrophe
    for w in words.iter_mut() {
        let c = core(w);
        if let Some((_, to)) = CONTRACTIONS.iter().find(|(from, _)| *from == c) {
            // "lets"/"thats" are ambiguous in other readings; only fix them when clearly a sentence word
            if !matches!(c.as_str(), "thats" | "whats" | "lets") {
                *w = swap(w, to);
            }
        } else if c == "i" {
            *w = swap(w, "I");
        }
    }
    // "their going" -> "they're going" (only before an -ing verb / "not" / "always", where a possessive cannot fit)
    for i in 0..words.len().saturating_sub(1) {
        let (c, n) = (core(&words[i]), core(&words[i + 1]));
        if c == "their" && (n.ends_with("ing") && !matches!(n.as_str(), "building" | "meeting" | "feeling" | "wedding" | "training" | "booking" | "saving" | "savings" | "morning" | "evening" | "ceiling" | "clothing" | "string" | "thing" | "things" | "something" | "anything" | "everything" | "king" | "ring" | "spring" | "wing") || matches!(n.as_str(), "not" | "always" | "never" | "here")) {
            words[i] = swap(&words[i], "they're");
        }
    }
    // "me and him was late" -> "he and I were late" (subject position at a clause start, before a past/present verb)
    for i in 0..words.len().saturating_sub(3) {
        let starts = i == 0 || words[i - 1].ends_with(['.', ',', ';', '!', '?']) || CONJ.contains(&core(&words[i - 1]).as_str());
        if starts && core(&words[i]) == "me" && core(&words[i + 1]) == "and" && matches!(core(&words[i + 2]).as_str(), "him" | "her" | "them") && matches!(core(&words[i + 3]).as_str(), "was" | "were" | "went" | "are" | "is" | "have" | "has") {
            let who = match core(&words[i + 2]).as_str() { "him" => "he", "her" => "she", _ => "they" };
            words[i] = swap(&words[i], who);
            words[i + 2] = "I".to_string();
            let v = core(&words[i + 3]);
            if matches!(v.as_str(), "was" | "is" | "has") {
                words[i + 3] = swap(&words[i + 3], match v.as_str() { "was" => "were", "is" => "are", _ => "have" });
            }
        }
    }
    let past_marker = {
        let text: Vec<String> = words.iter().map(|w| core(w)).collect();
        let marker = text.iter().any(|w| w == "yesterday" || w == "ago") || text.windows(2).any(|p| p[0] == "last" && matches!(p[1].as_str(), "week" | "night" | "month" | "year" | "time" | "monday" | "tuesday" | "wednesday" | "thursday" | "friday" | "weekend"));
        let future = text.iter().any(|w| matches!(w.as_str(), "tomorrow" | "will" | "would" | "going" | "gonna" | "next" | "should" | "could" | "can" | "must"));
        marker && !future
    };
    let mut if_clause = false;
    let mut had_fix = false;
    for i in 0..words.len() {
        let cur = core(&words[i]);
        let prev = if i == 0 { String::new() } else { core(&words[i - 1]) };
        let subj_before = if i == 0 { None } else { Some(prev.as_str()) };
        // "their is" -> "there is"
        if prev == "their" && matches!(cur.as_str(), "is" | "are" | "was" | "were" | "has" | "have" | "had") && (i == 1 || at_clause_for(words, i - 1)) {
            words[i - 1] = swap(&words[i - 1], "there");
        }
        // "if I would have known" -> "if I had known"
        if cur == "if" {
            if_clause = true;
        }
        if if_clause && cur == "would" && i + 1 < words.len() && core(&words[i + 1]) == "have" && i > 0 && SUBJECTS.contains(&prev.as_str()) {
            words[i] = swap(&words[i], "had");
            words[i + 1] = String::new();
            had_fix = true;
        } else if had_fix && cur == "will" && i + 1 < words.len() && core(&words[i + 1]) == "have" {
            words[i] = swap(&words[i], "would");
        }
        // "discuss about the budget" -> "discuss the budget"; "please to check" -> "please check"
        if cur == "about" && matches!(prev.as_str(), "discuss" | "discussed" | "discussing" | "discusses") {
            words[i] = String::new();
            continue;
        }
        if cur == "to" && prev == "please" {
            words[i] = String::new();
            continue;
        }
        // "the customers is" -> "the customers are" (plural noun with a singular verb; only unambiguous plurals)
        if i >= 2 && matches!(cur.as_str(), "is" | "was") && core(&words[i - 2]) == "the" && is_plain_plural(&prev) && !words[i - 1].ends_with([',', ';']) {
            words[i] = swap(&words[i], if cur == "is" { "are" } else { "were" });
            continue;
        }
        let Some(subj) = subj_before else { continue };
        let plural = matches!(subj, "we" | "you" | "they");
        let third_p = matches!(subj, "he" | "she") || (subj == "it" && i >= 2 && (CONJ.contains(&core(&words[i - 2]).as_str()) || words[i - 2].ends_with([',', ';', ':'])));
        let gate = i < 2 || CONJ.contains(&core(&words[i - 2]).as_str()) || words[i - 2].ends_with([',', ';', ':', '.']);
        match cur.as_str() {
            "was" if plural => words[i] = swap(&words[i], "were"),
            "has" if plural || subj == "i" => words[i] = swap(&words[i], "have"),
            "is" if subj == "i" => words[i] = swap(&words[i], "am"),
            "is" if plural => words[i] = swap(&words[i], "are"),
            "are" if third_p || subj == "i" => words[i] = swap(&words[i], if subj == "i" { "am" } else { "is" }),
            "have" if third_p && gate => words[i] = swap(&words[i], "has"),
            "don't" if third_p => words[i] = swap(&words[i], "doesn't"),
            "seen" if SUBJECTS.contains(&subj) && !matches!(subj, "") => words[i] = swap(&words[i], "saw"),
            w if plural || subj == "i" => {
                if SUBJECTS.contains(&subj) {
                    if let Some(b) = base_of_third(w) {
                        // "we meets" -> "we meet"; not a noun reading: "we need updates" never follows a bare subject
                        words[i] = swap(&words[i], &b);
                    }
                }
            }
            _ => {}
        }
        // after the (possibly fixed) agreement: tense and 3rd-person -s
        let cur2 = core(&words[i]);
        if SUBJECTS.contains(&subj) && past_marker {
            if let Some((_, past)) = PAST.iter().find(|(b, _)| *b == cur2) {
                words[i] = swap(&words[i], past);
                continue;
            }
            if matches!(subj, "he" | "she") {
                if let Some(b) = base_of_third(&cur2) {
                    if let Some((_, past)) = PAST.iter().find(|(pb, _)| *pb == b) {
                        words[i] = swap(&words[i], past);
                        continue;
                    }
                }
            }
        }
        if third_p && gate && BASE.contains(&cur2.as_str()) {
            words[i] = swap(&words[i], &third(&cur2));
        }
    }
}

/// A noun that is surely plural: ends in -s but not in -ss/-us/-is/-ics/-ous and is not a known singular -s word.
fn is_plain_plural(w: &str) -> bool {
    const SINGULAR: &[&str] = &["news", "series", "status", "bus", "business", "process", "access", "address", "class", "lens", "analysis", "focus", "progress", "bonus", "canvas", "gas", "this", "its", "his", "was", "has", "does", "always", "perhaps", "less", "plus", "chaos", "success", "campus", "virus", "census", "corpus", "atlas", "alias", "bias", "stats", "mathematics", "physics", "ios", "os", "pass", "kubernetes", "aws", "wi-fi"];
    w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") && !w.ends_with("us") && !w.ends_with("is") && !w.ends_with("ics") && !w.ends_with("ous") && !SINGULAR.contains(&w) && w.chars().all(|c| c.is_ascii_lowercase())
}

fn at_clause_for(words: &[String], i: usize) -> bool {
    i == 0 || CONJ.contains(&core(&words[i - 1]).as_str()) || words[i - 1].ends_with([',', ';', ':', '.'])
}

/// "10am" / "3:30pm" -> "10 a.m." / "3:30 p.m."
fn meridiem(token: &str) -> Option<String> {
    let lower = token.to_lowercase();
    let trimmed = lower.trim_end_matches(['.', ',', ';', '?', '!']);
    let tail = &lower[trimmed.len()..];
    for (suffix, dotted) in [("am", "a.m."), ("pm", "p.m.")] {
        if let Some(num) = trimmed.strip_suffix(suffix) {
            if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit() || c == ':') && num.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                // the sentence period is shared with the one in "p.m."
                let tail = if tail.starts_with('.') { &tail[1..] } else { tail };
                return Some(format!("{num} {dotted}{tail}"));
            }
        }
    }
    None
}

/// Polishes text line by line (lists and paragraphs keep their layout).
pub fn polish(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            let (marker, body) = split_marker(line);
            let mut sentence: Vec<String> = Vec::new();
            let mut out: Vec<String> = Vec::new();
            for w in body.split_whitespace() {
                sentence.push(meridiem(w).unwrap_or_else(|| w.to_string()));
                if w.ends_with(['.', '?', '!']) && !w.to_lowercase().ends_with("p.m.") && !w.to_lowercase().ends_with("a.m.") {
                    fix_sentence(&mut sentence);
                    out.append(&mut sentence);
                }
            }
            fix_sentence(&mut sentence);
            out.append(&mut sentence);
            format!("{marker}{}", out.into_iter().filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" "))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Splits a list marker ("- ", "1. ") from the line.
fn split_marker(line: &str) -> (&str, &str) {
    for m in ["- ", "* "] {
        if let Some(rest) = line.strip_prefix(m) {
            return (m, rest);
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && line[digits..].starts_with(". ") {
        return (&line[..digits + 2], &line[digits + 2..]);
    }
    ("", line)
}

/// Capital letters at sentence starts and after list markers: the model writes "casual" text in lowercase.
pub fn capitalise(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            let (marker, body) = split_marker(line);
            let mut start = true;
            let words: Vec<String> = body
                .split_whitespace()
                .map(|w| {
                    let mut t = w.to_string();
                    if start {
                        if let Some(i) = t.find(char::is_alphabetic) {
                            let c = t[i..].chars().next().unwrap_or('a');
                            t.replace_range(i..i + c.len_utf8(), &c.to_uppercase().collect::<String>());
                        }
                    }
                    let l = w.to_lowercase();
                    start = w.ends_with(['.', '?', '!']) && !l.ends_with("a.m.") && !l.ends_with("p.m.") && !l.ends_with("e.g.") && !l.ends_with("i.e.");
                    t
                })
                .collect();
            format!("{marker}{}", words.join(" "))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixes_agreement_tense_and_contractions() {
        assert_eq!(polish("Tomorrow we meets at 10 in the room."), "Tomorrow we meet at 10 in the room.");
        assert_eq!(polish("Please let me know if you has any question."), "Please let me know if you have any question.");
        assert_eq!(polish("Yesterday, I go to the doctor and he tell me to rest."), "Yesterday, I went to the doctor and he told me to rest.");
        assert_eq!(polish("My battery drains fast and it overheat sometimes."), "My battery drains fast and it overheats sometimes.");
        assert_eq!(polish("He don't know. i dont care"), "He doesn't know. I don't care");
        assert_eq!(polish("Their is a problem."), "There is a problem.");
        assert_eq!(polish("If I would have known earlier, I will have told you."), "If I had known earlier, I would have told you.");
        assert_eq!(polish("The customers is upset and we discuss about it."), "The customers are upset and we discuss it.");
        assert_eq!(polish("The status is green, the news was good."), "The status is green, the news was good.");
        assert_eq!(polish("I seen the email."), "I saw the email.");
        assert_eq!(polish("The meeting is at 3:30pm or 10am."), "The meeting is at 3:30 p.m. or 10 a.m.");
        assert_eq!(polish("Meet at 5pm."), "Meet at 5 p.m.");
        assert_eq!(polish("Their going to the store, and me and him was late."), "They're going to the store, and he and I were late.");
        assert_eq!(polish("Their meeting is at noon."), "Their meeting is at noon.");
    }

    #[test]
    fn leaves_correct_text_alone() {
        for s in ["Did he have the file?", "Let it go.", "She said we need it, and it works.", "I think we should go.", "If I were you, I would go.", "Make it work.", "They have the keys.", "I will tell him tomorrow."] {
            assert_eq!(polish(s), s);
        }
    }

    #[test]
    fn capitalises_sentences_and_list_items() {
        assert_eq!(capitalise("hello there. i am here"), "Hello there. I am here");
        assert_eq!(capitalise("the list:\n- milk\n1. eggs"), "The list:\n- Milk\n1. Eggs");
    }
}
