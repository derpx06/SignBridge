package com.whispr.core.dictation

import java.util.regex.Pattern

/**
 * The user's dictionary: whole-phrase, case-insensitive, longest-match-first replacements.
 * A port of Voicio's `replacements.rs`, itself derived from Hex (https://github.com/anomalyco/hex,
 * MIT, Copyright (c) 2026 Christopher Langton).
 *
 * - Phrases match only on word boundaries; CJK/Thai/Lao/Khmer/Myanmar/Tibetan/Hangul characters do not
 *   count as word characters, so `issue` still matches inside "这个issue很重要".
 * - The longest match wins; ties go to the earliest position, then the earlier rule. Outputs are never
 *   re-scanned (no recursion in one pass).
 * - An empty output deletes the phrase and tidies the spacing and clause punctuation around it.
 */
class Dictionary(entries: List<Entry>) {

    /** [phrase] is what the recogniser hears, [output] what to type (empty = delete). */
    data class Entry(val phrase: String, val output: String)

    private class Rule(val matcher: Pattern, val output: String)

    private class Candidate(val start: Int, val end: Int, val rule: Int, val byteLen: Int)

    private val rules: List<Rule> = entries.mapNotNull { e ->
        val phrase = e.phrase.trim()
        if (phrase.isEmpty()) null
        else Rule(Pattern.compile(Pattern.quote(phrase), Pattern.CASE_INSENSITIVE or Pattern.UNICODE_CASE), e.output)
    }

    fun replace(text: String): String {
        val candidates = rules.withIndex().flatMap { (i, r) ->
            phraseMatches(r.matcher, text).map { (s, e) ->
                // Rust measures match length in UTF-8 bytes; keep that ordering.
                Candidate(s, e, i, text.substring(s, e).toByteArray(Charsets.UTF_8).size)
            }
        }.sortedWith(
            compareByDescending<Candidate> { it.byteLen }.thenBy { it.start }.thenBy { it.rule },
        )
        val selected = ArrayList<Candidate>()
        for (c in candidates) {
            if (selected.any { c.start < it.end && it.start < c.end }) continue
            selected.add(c)
        }
        if (selected.isEmpty()) return text
        selected.sortBy { it.start }

        val output = StringBuilder(text.length)
        var cursor = 0
        val capitalizeNext = BooleanArray(1)
        for ((index, c) in selected.withIndex()) {
            if (cursor < c.start) pushSegment(output, text.substring(cursor, c.start), capitalizeNext)
            val replacement = rules[c.rule].output
            if (replacement.isNotEmpty()) {
                pushSegment(output, replacement, capitalizeNext)
                cursor = c.end
                continue
            }
            val nextLimit = selected.getOrNull(index + 1)?.start ?: text.length
            val between = text.substring(c.end, nextLimit)
            if (atSentenceStart(output)) {
                val firstLetter = text.substring(c.start, c.end).codePoints().toArray().firstOrNull { Character.isAlphabetic(it) }
                if (firstLetter != null && Character.isUpperCase(firstLetter)) capitalizeNext[0] = true
                val afterSpaces = between.trimStart(*CLAUSE_PUNCTUATION).trimStart(*HORIZONTAL_WHITESPACE)
                cursor = c.end + (between.length - afterSpaces.length)
                if ((afterSpaces.isEmpty() && nextLimit == text.length) || afterSpaces.startsWith('\n')) {
                    trimTrailingHorizontalSpaces(output)
                }
            } else {
                val remaining = text.substring(c.end).trimStart(*HORIZONTAL_WHITESPACE)
                if (between.firstOrNull()?.let { it in CLAUSE_PUNCTUATION } == true || remaining.isEmpty() || remaining.startsWith('\n')) {
                    trimTrailingHorizontalSpaces(output)
                    if (output.lastOrNull() == ',') output.setLength(output.length - 1)
                    cursor = if (remaining.isEmpty() || remaining.startsWith('\n')) {
                        c.end + (between.length - between.trimStart(*HORIZONTAL_WHITESPACE).length)
                    } else {
                        c.end
                    }
                } else if (output.lastOrNull()?.let { it in HORIZONTAL_WHITESPACE } == true) {
                    cursor = c.end + (between.length - between.trimStart(*HORIZONTAL_WHITESPACE).length)
                } else {
                    cursor = c.end
                }
            }
        }
        if (cursor < text.length) pushSegment(output, text.substring(cursor), capitalizeNext)
        return output.toString()
    }

    private companion object {
        val HORIZONTAL_WHITESPACE = charArrayOf(' ', '\t')
        val CLAUSE_PUNCTUATION = charArrayOf('.', '!', '?', ',', ';', ':')
        val SENTENCE_ENDINGS = charArrayOf('.', '!', '?', '\n')

        fun atSentenceStart(output: CharSequence): Boolean {
            val trimmed = output.trimEnd(*HORIZONTAL_WHITESPACE)
            return trimmed.isEmpty() || trimmed.last() in SENTENCE_ENDINGS
        }

        fun trimTrailingHorizontalSpaces(output: StringBuilder) {
            var n = output.length
            while (n > 0 && output[n - 1] in HORIZONTAL_WHITESPACE) n--
            output.setLength(n)
        }

        fun pushSegment(output: StringBuilder, segment: String, capitalizeNext: BooleanArray) {
            if (!capitalizeNext[0] || segment.isEmpty()) {
                output.append(segment)
                if (segment.any { !CleanupRules.isWhite(it) }) capitalizeNext[0] = false
                return
            }
            val index = segment.indexOfFirst { !CleanupRules.isWhite(it) }
            if (index < 0) {
                output.append(segment)
                return
            }
            val cp = segment.codePointAt(index)
            output.append(segment, 0, index)
            if (Character.isLowerCase(cp) && atSentenceStart(output)) {
                output.append(String(Character.toChars(cp)).uppercase())
            } else {
                output.appendCodePoint(cp)
            }
            output.append(segment, index + Character.charCount(cp), segment.length)
            capitalizeNext[0] = false
        }

        /**
         * Whole-phrase matches. A match rejected for touching a word resumes the search one character
         * later, so an overlapping whole-phrase match is still found.
         */
        fun phraseMatches(matcher: Pattern, text: String): List<Pair<Int, Int>> {
            val found = ArrayList<Pair<Int, Int>>()
            val m = matcher.matcher(text)
            var from = 0
            while (from <= text.length && m.find(from)) {
                if (phraseBoundary(text, m.start(), m.end())) {
                    from = m.end()
                    found.add(m.start() to m.end())
                } else {
                    from = m.start() + Character.charCount(text.codePointAt(m.start()))
                }
            }
            return found
        }

        fun phraseBoundary(text: String, start: Int, end: Int): Boolean {
            val before = if (start > 0) text.codePointBefore(start) else null
            val after = if (end < text.length) text.codePointAt(end) else null
            return !(before != null && wordCharacter(before)) && !(after != null && wordCharacter(after))
        }

        fun wordCharacter(cp: Int): Boolean = spaceDelimitedScript(cp) && (CleanupRules.isAlphanumeric(cp) || cp == '_'.code)

        /** Scripts that do not put spaces between words: their characters never count as word characters. */
        fun spaceDelimitedScript(cp: Int): Boolean = when (Character.UnicodeScript.of(cp)) {
            Character.UnicodeScript.HAN, Character.UnicodeScript.HIRAGANA, Character.UnicodeScript.KATAKANA,
            Character.UnicodeScript.HANGUL, Character.UnicodeScript.THAI, Character.UnicodeScript.LAO,
            Character.UnicodeScript.KHMER, Character.UnicodeScript.MYANMAR, Character.UnicodeScript.TIBETAN,
            -> false
            else -> true
        }
    }
}
