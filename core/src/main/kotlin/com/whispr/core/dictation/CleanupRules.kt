package com.whispr.core.dictation

/**
 * The pure, deterministic part of Voicio's post-dictation cleanup (port of `cleanup.rs`; the
 * llama.cpp `Cleaner` is not ported). [ruleClean] is the fallback cleanup; [guardrail] decides
 * whether a model's rewrite may replace the spoken text. Accuracy beats polish: never invent words.
 */
object CleanupRules {
    private val FILLERS = setOf("um", "umm", "uh", "uhh", "uhm", "erm", "er", "hmm", "mm", "mhm", "ah")

    /** Words that are real when doubled ("had had", "very very", "bye bye"), so never stutters. */
    private val LEGIT_REPEATS = setOf("had", "that", "very", "bye", "no", "so", "well", "really", "many", "far")

    private val NUMBER_WORDS = setOf(
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
        "twelve", "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen",
        "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety", "hundred",
        "thousand", "million", "billion", "first", "second", "third", "half", "quarter", "dozen",
        "point", "percent",
    )

    private val NEGATIONS = setOf("not", "never", "cannot", "without", "nobody", "nothing")

    /**
     * Drops filler words and immediate stutters ("the the"), keeps everything else verbatim,
     * collapses whitespace to single spaces and upper-cases the first letter.
     */
    fun ruleClean(raw: String): String {
        val out = ArrayList<String>()
        for (word in words(raw)) {
            val n = normalize(word)
            if (n in FILLERS) continue
            val prev = out.lastOrNull()
            if (prev != null && n.isNotEmpty() && normalize(prev) == n && n !in LEGIT_REPEATS) continue
            out.add(word)
        }
        val text = out.joinToString(" ")
        if (text.isEmpty()) return text
        val first = text.codePointAt(0)
        if (!Character.isLowerCase(first)) return text
        val width = Character.charCount(first)
        return String(Character.toChars(first)).uppercase() + text.substring(width)
    }

    /**
     * Accepts [output] only if it does not add content that was never spoken in [input]. Deletions
     * are fine (fillers, self-corrections); formatting like "forty two" -> "42" is fine.
     * Rejected when: too short (< 50 % of the content words), negation count changed, spoken digits
     * lost or unspoken digits invented, or more than 2 (and > 10 %) invented words.
     */
    fun guardrail(input: String, output: String): Boolean {
        val spoken = words(input).map(::normalize).toSet()
        val content = words(ruleClean(input)).map(::normalize).filter { it.isNotEmpty() }
        val written = words(output).map(::normalize).filter { it.isNotEmpty() }
        if (content.isEmpty()) return written.isEmpty()
        // Writing numbers or an address ("john dot smith at example dot com" -> an address) or resolving a
        // self-correction legitimately shortens the text, so allow much shorter output when it is formatted.
        val formatted = output.any { it in '0'..'9' || it == '@' || it == '$' || it == '%' }
        if (written.size * 10 < content.size * (if (formatted) 1 else 5)) return false
        if (negations(input) != negations(output)) return false
        if (!numbersConsistent(input, output)) return false
        val invented = written.count { w ->
            w.codePointCount(0, w.length) >= 4 &&
                w.none { it in '0'..'9' } &&
                w !in spoken &&
                // Allow inflections/expansions formatting produces ("going" from "gonna").
                spoken.none { s -> commonPrefix(w, s) >= 3 }
        }
        return invented <= 2 && invented * 10 <= maxOf(written.size, 10)
    }

    /** Contractions with the apostrophe dropped (speech-to-text often does that). */
    private val NEGATED = setOf(
        "dont", "doesnt", "didnt", "cant", "wont", "isnt", "arent", "wasnt", "werent", "havent", "hasnt", "hadnt",
        "shouldnt", "couldnt", "wouldnt", "mustnt", "neednt", "aint",
    )

    private fun commonPrefix(a: String, b: String): Int {
        val x = a.codePoints().toArray()
        val y = b.codePoints().toArray()
        var n = 0
        while (n < x.size && n < y.size && x[n] == y[n]) n++
        return n
    }

    internal fun negations(text: String): Int =
        words(text).map(::normalize).count { w ->
            val bare = w.replace("'", "") // "dont" and "don't" are the same word
            w in NEGATIONS || bare in NEGATED || w.endsWith("n't")
        }

    /**
     * Digits in the output must have been spoken, either as digits or as number words; digits that
     * were spoken as digits must survive.
     */
    internal fun numbersConsistent(raw: String, cleaned: String): Boolean {
        val spoken = digitRuns(raw)
        val written = digitRuns(cleaned)
        if (spoken.any { it !in written }) return false
        val saidNumberWords = words(raw).map(::normalize).any { it in NUMBER_WORDS }
        return written.isEmpty() || spoken.isNotEmpty() || saidNumberWords
    }

    private fun digitRuns(text: String): List<String> = Regex("[0-9]+").findAll(text).map { it.value }.toList()

    /** Keeps letters, digits and apostrophes, lower-cased (Rust: `is_alphanumeric() || '\''`). */
    internal fun normalize(word: String): String {
        val sb = StringBuilder()
        word.codePoints().forEach { cp ->
            if (isAlphanumeric(cp) || cp == '\''.code) sb.appendCodePoint(cp)
        }
        return sb.toString().lowercase()
    }

    internal fun isAlphanumeric(cp: Int): Boolean =
        Character.isAlphabetic(cp) || when (Character.getType(cp).toByte()) {
            Character.DECIMAL_DIGIT_NUMBER, Character.LETTER_NUMBER, Character.OTHER_NUMBER -> true
            else -> false
        }

    /** Splits on Unicode White_Space like Rust's `split_whitespace`. */
    internal fun words(text: String): List<String> {
        val result = ArrayList<String>()
        var start = -1
        for (i in text.indices) {
            if (isWhite(text[i])) {
                if (start >= 0) { result.add(text.substring(start, i)); start = -1 }
            } else if (start < 0) {
                start = i
            }
        }
        if (start >= 0) result.add(text.substring(start))
        return result
    }

    internal fun isWhite(c: Char): Boolean =
        c in '\t'..'\r' || c == '\u0085' || Character.isSpaceChar(c)
}
