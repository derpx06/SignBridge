//! How the cleanup model should write: the three control axes S1-mini was trained on, plus short
//! spoken directives ("email mode", "make it formal") that change them for one dictation.
//!
//! S1-mini does one job and does not follow free-form instructions, so "instructions" are mapped
//! onto its trained axes here. Anything outside those sets would make the model produce garbage.

/// Register: how much of the speaker's voice survives.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Styling {
    Casual,
    SemiCasual,
    #[default]
    SemiFormal,
    Formal,
}

/// Whether enumerations may become bullet lists.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Structure {
    /// Prose, unless the speaker enumerates (first/second/third, "one ... two ... three", "as a list")
    /// or dictates an email (greeting and sign-off): then the whole dictation is laid out as one.
    #[default]
    Auto,
    Prose,
    Lists,
}

/// Destination conventions.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Context {
    #[default]
    General,
    Email,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct CleanupStyle {
    pub styling: Styling,
    pub structure: Structure,
    pub context: Context,
    /// Spoken instructions ("make it formal", "as a list") are typed as words, not obeyed.
    pub ignore_voice: bool,
    /// The model is skipped for this dictation (low memory) but the rules still run: never type raw text.
    pub rules_only: bool,
}

impl Styling {
    pub fn as_str(self) -> &'static str {
        match self {
            Styling::Casual => "casual",
            Styling::SemiCasual => "semi-casual",
            Styling::SemiFormal => "semi-formal",
            Styling::Formal => "formal",
        }
    }
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().replace([' ', '_'], "-").as_str() {
            "casual" => Some(Styling::Casual),
            "semi-casual" => Some(Styling::SemiCasual),
            "semi-formal" => Some(Styling::SemiFormal),
            "formal" => Some(Styling::Formal),
            _ => None,
        }
    }
}

impl Structure {
    pub fn as_str(self) -> &'static str {
        match self {
            // the model was only trained on prose and lists: Auto is prose until `resolve` finds a list or an email
            Structure::Auto | Structure::Prose => "prose",
            Structure::Lists => "lists",
        }
    }
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Structure::Auto),
            "prose" => Some(Structure::Prose),
            "lists" | "list" => Some(Structure::Lists),
            _ => None,
        }
    }
}

impl Context {
    pub fn as_str(self) -> &'static str {
        match self {
            Context::General => "general",
            Context::Email => "email",
        }
    }
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "general" => Some(Context::General),
            "email" => Some(Context::Email),
            _ => None,
        }
    }
}

impl CleanupStyle {
    /// The exact control line the model was trained on.
    pub fn control_line(&self) -> String {
        format!("[Styling: {}] [Structure: {}] [Context: {}]", self.styling.as_str(), self.structure.as_str(), self.context.as_str())
    }

    /// `"semi-formal,prose,general"`: how the agent hands the style to the worker.
    pub fn encode(&self) -> String {
        let structure = if self.structure == Structure::Auto { "auto" } else { self.structure.as_str() };
        format!("{},{},{}", self.styling.as_str(), structure, self.context.as_str())
    }

    /// Tolerant inverse of [`encode`](Self::encode): anything unknown keeps its default.
    pub fn decode(text: &str) -> Self {
        let mut parts = text.split(',');
        Self {
            styling: parts.next().and_then(Styling::parse).unwrap_or_default(),
            structure: parts.next().and_then(Structure::parse).unwrap_or_default(),
            context: parts.next().and_then(Context::parse).unwrap_or_default(),
            ignore_voice: false,
            rules_only: false,
        }
        .with_flags(text)
    }

    fn with_flags(mut self, text: &str) -> Self {
        for flag in text.split(',').skip(3).map(str::trim) {
            self.ignore_voice |= flag == "novoice";
            self.rules_only |= flag == "rules";
        }
        self
    }

    /// Lists and emails are laid out over the whole dictation, so they cannot be cleaned one
    /// utterance at a time while the user is still speaking.
    pub fn needs_whole_text(&self) -> bool {
        self.structure == Structure::Lists || self.context == Context::Email
    }

    /// For `Structure::Auto`: looks at the whole dictation and switches to lists or email layout when
    /// the speaker clearly dictated one. Explicit choices are returned unchanged.
    pub fn resolve(mut self, raw: &str) -> Self {
        if self.structure != Structure::Auto {
            return self;
        }
        let words: Vec<String> = raw.split_whitespace().map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()).filter(|w| !w.is_empty()).collect();
        if self.context == Context::General && looks_like_email(&words) {
            self.context = Context::Email;
        }
        if looks_enumerated(&words) {
            self.structure = Structure::Lists;
        }
        self
    }

    pub fn is_voice_on(&self) -> bool {
        !self.ignore_voice
    }

    pub fn apply(mut self, d: &Directive) -> Self {
        self.styling = d.styling.unwrap_or(self.styling);
        self.structure = d.structure.unwrap_or(self.structure);
        self.context = d.context.unwrap_or(self.context);
        self
    }
}

/// "dear maria ... regards john": a greeting first and a sign-off last.
fn looks_like_email(w: &[String]) -> bool {
    if w.len() < 12 {
        return false;
    }
    let greeting = matches!(w[0].as_str(), "dear" | "hi" | "hello" | "hey") && w.len() > 2;
    let tail: Vec<&str> = w[w.len().saturating_sub(5)..].iter().map(String::as_str).collect();
    let signoff = tail.iter().any(|x| matches!(*x, "regards" | "sincerely" | "cheers" | "thanks" | "yours" | "faithfully")) || tail.windows(2).any(|p| p == ["thank", "you"] || p == ["best", "wishes"]);
    greeting && signoff
}

/// First/second/third, "one ... two ... three" after a colon-like lead, or a bulleted request.
fn looks_enumerated(w: &[String]) -> bool {
    // "the first was too expensive, the second ..." is prose; "first we fix ... second we update" is a list
    let has = |x: &str| w.iter().enumerate().any(|(i, v)| v == x && (i == 0 || !matches!(w[i - 1].as_str(), "the" | "a" | "our" | "my" | "that" | "this")));
    let ordinals = ["first", "second", "third", "fourth"].iter().filter(|o| has(o)).count();
    if has("first") && has("second") && ordinals >= 2 && (has("third") || has("finally") || has("lastly") || has("next")) {
        return true;
    }
    let pos = |x: &str| w.iter().position(|v| v == x);
    // "action items one update ... two email ... three book": numbers in order
    if let (Some(a), Some(b), Some(c)) = (pos("one"), pos("two"), pos("three")) {
        if a < b && b < c && b - a >= 2 && c - b >= 2 && w.iter().any(|v| matches!(v.as_str(), "items" | "steps" | "points" | "tasks" | "things" | "list" | "agenda" | "numbers")) {
            return true;
        }
    }
    w.windows(2).any(|p| p[0] == "bullet" && matches!(p[1].as_str(), "point" | "points" | "list"))
}

/// Settings found at the start of what was said.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Directive {
    pub styling: Option<Styling>,
    pub structure: Option<Structure>,
    pub context: Option<Context>,
}

impl Directive {
    pub fn is_empty(&self) -> bool {
        *self == Directive::default()
    }
}

enum Effect {
    Styling(Styling),
    Structure(Structure),
    Context(Context),
}

/// Spoken phrases and what they set. Matched only at the very start, whole words, longest first, so
/// ordinary sentences are left alone ("Email John about..." is not a directive).
const PHRASES: &[(&str, Effect)] = &[
    ("semi formal mode", Effect::Styling(Styling::SemiFormal)),
    ("semi formal tone", Effect::Styling(Styling::SemiFormal)),
    ("semi casual mode", Effect::Styling(Styling::SemiCasual)),
    ("semi casual tone", Effect::Styling(Styling::SemiCasual)),
    ("formal mode", Effect::Styling(Styling::Formal)),
    ("formal tone", Effect::Styling(Styling::Formal)),
    ("make it formal", Effect::Styling(Styling::Formal)),
    ("make this formal", Effect::Styling(Styling::Formal)),
    ("in a formal tone", Effect::Styling(Styling::Formal)),
    ("casual mode", Effect::Styling(Styling::Casual)),
    ("casual tone", Effect::Styling(Styling::Casual)),
    ("make it casual", Effect::Styling(Styling::Casual)),
    ("make this casual", Effect::Styling(Styling::Casual)),
    ("in a casual tone", Effect::Styling(Styling::Casual)),
    ("email mode", Effect::Context(Context::Email)),
    ("email format", Effect::Context(Context::Email)),
    ("format this as an email", Effect::Context(Context::Email)),
    ("format as an email", Effect::Context(Context::Email)),
    ("write this as an email", Effect::Context(Context::Email)),
    ("make this an email", Effect::Context(Context::Email)),
    ("make it an email", Effect::Context(Context::Email)),
    ("as an email", Effect::Context(Context::Email)),
    ("list mode", Effect::Structure(Structure::Lists)),
    ("bullet mode", Effect::Structure(Structure::Lists)),
    ("bullet point mode", Effect::Structure(Structure::Lists)),
    ("bullet points mode", Effect::Structure(Structure::Lists)),
    ("format this as a list", Effect::Structure(Structure::Lists)),
    ("format as a list", Effect::Structure(Structure::Lists)),
    ("make this a list", Effect::Structure(Structure::Lists)),
    ("make it a list", Effect::Structure(Structure::Lists)),
    ("use bullet points", Effect::Structure(Structure::Lists)),
    ("in bullet points", Effect::Structure(Structure::Lists)),
    ("as bullet points", Effect::Structure(Structure::Lists)),
    ("as a bulleted list", Effect::Structure(Structure::Lists)),
    ("as a list", Effect::Structure(Structure::Lists)),
    ("prose mode", Effect::Structure(Structure::Prose)),
    ("no bullet points", Effect::Structure(Structure::Prose)),
    ("no bullets", Effect::Structure(Structure::Prose)),
    ("as a paragraph", Effect::Structure(Structure::Prose)),
    ("as paragraphs", Effect::Structure(Structure::Prose)),
];

fn words_of(text: &str) -> Vec<(String, usize)> {
    // (lowercase word without edge punctuation, byte offset where the word ends in `text`)
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices().chain(std::iter::once((text.len(), ' '))) {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                let word = text[s..i].trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase().replace('-', " ");
                out.push((word, i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    out
}

/// Takes spoken directives off the front of `raw`. Returns them and the text that remains.
pub fn extract_directive(raw: &str) -> (Directive, String) {
    let mut directive = Directive::default();
    let mut rest = raw.trim();
    'again: loop {
        let words = words_of(rest);
        for (phrase, effect) in PHRASES {
            let n = phrase.split(' ').count();
            if words.len() >= n && words[..n].iter().map(|(w, _)| w.as_str()).collect::<Vec<_>>().join(" ") == *phrase {
                match effect {
                    Effect::Styling(s) => directive.styling = Some(*s),
                    Effect::Structure(s) => directive.structure = Some(*s),
                    Effect::Context(c) => directive.context = Some(*c),
                }
                rest = rest[words[n - 1].1..].trim_start_matches(|c: char| !c.is_alphanumeric());
                continue 'again;
            }
        }
        break;
    }
    if directive.is_empty() {
        return (directive, raw.trim().to_string());
    }
    let mut text = rest.to_string();
    if let Some(first) = text.chars().next() {
        if first.is_lowercase() {
            text = first.to_uppercase().collect::<String>() + &text[first.len_utf8()..];
        }
    }
    (directive, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_commands_can_be_switched_off() {
        assert!(!CleanupStyle::decode("formal,prose,general,novoice").is_voice_on());
        assert!(CleanupStyle::decode("formal,prose,general").is_voice_on());
    }

    #[test]
    fn control_line_is_exactly_what_the_model_was_trained_on() {
        assert_eq!(CleanupStyle::default().control_line(), "[Styling: semi-formal] [Structure: prose] [Context: general]");
        let s = CleanupStyle { styling: Styling::Formal, structure: Structure::Lists, context: Context::Email, ..Default::default() };
        assert_eq!(s.control_line(), "[Styling: formal] [Structure: lists] [Context: email]");
    }

    #[test]
    fn encode_and_decode_round_trip_and_tolerate_garbage() {
        let s = CleanupStyle { styling: Styling::Casual, structure: Structure::Lists, context: Context::Email, ..Default::default() };
        assert_eq!(CleanupStyle::decode(&s.encode()), s);
        assert_eq!(CleanupStyle::decode(""), CleanupStyle::default());
        assert_eq!(CleanupStyle::decode("shouty,wild"), CleanupStyle::default());
        assert_eq!(Styling::parse("Semi Formal"), Some(Styling::SemiFormal));
    }

    #[test]
    fn directives_at_the_start_change_the_style_and_are_removed() {
        let (d, rest) = extract_directive("Email mode. hey sarah just following up on the proposal");
        assert_eq!(d.context, Some(Context::Email));
        assert_eq!(rest, "Hey sarah just following up on the proposal");
        let (d, rest) = extract_directive("make it formal, i cannot attend tomorrow");
        assert_eq!(d.styling, Some(Styling::Formal));
        assert_eq!(rest, "I cannot attend tomorrow");
        let (d, rest) = extract_directive("casual mode as a list. milk eggs and bread");
        assert_eq!((d.styling, d.structure), (Some(Styling::Casual), Some(Structure::Lists)));
        assert_eq!(rest, "Milk eggs and bread");
    }

    #[test]
    fn ordinary_sentences_are_left_alone() {
        for text in ["Email John about the meeting", "we should write this as an email later", "The list mode of the app is broken", "make it formal"] {
            let (d, rest) = extract_directive(text);
            if text == "make it formal" {
                assert_eq!((d.styling, rest.as_str()), (Some(Styling::Formal), ""));
            } else {
                assert!(d.is_empty(), "{text}");
                assert_eq!(rest, text);
            }
        }
    }

    #[test]
    fn lists_and_emails_need_the_whole_text() {
        assert!(!CleanupStyle::default().needs_whole_text());
        assert!(CleanupStyle { structure: Structure::Lists, ..Default::default() }.needs_whole_text());
        assert!(CleanupStyle { context: Context::Email, ..Default::default() }.needs_whole_text());
    }
}
