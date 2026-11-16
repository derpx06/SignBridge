package com.whispr.core.dictation

/**
 * How people actually talk while dictating, turned into what they meant (shared by the bubble and the keyboard):
 * - Backtrack: "coffee at 2 actually 3" -> "coffee at 3"; "no wait" works the same way.
 * - Spoken punctuation: "comma", "full stop", "question mark"... become the mark, replacing any the model guessed.
 * - Ordinals: "first milk second eggs third bread" -> a numbered list.
 * Pure and rule-based; [Pipeline] runs it when `Options.spoken` is on.
 */
object SpokenEdits {
    /** Raw words in, the restated part removed. Runs before the voice commands. */
    fun backtrack(raw: String): String {
        var words = CleanupRules.words(raw)
        var guard = 0
        while (guard++ < 8) {
            val hit = findCue(words) ?: break
            val (at, len) = hit
            val before = words.subList(0, at)
            val restEnd = sentenceEnd(words, at + len)
            val after = words.subList(at + len, restEnd)
            val cut = cutPoint(before, after)
            // Keep the punctuation that ended the dropped part ("at 2." -> "at 3.") on the replacement.
            words = before.subList(0, cut) + after + words.subList(restEnd, words.size)
        }
        return words.joinToString(" ")
    }

    private val CUES = listOf(listOf("no", "wait"), listOf("actually"))

    /** Words that make "actually" an ordinary adverb ("I actually think", "it's actually fine"). */
    private val ADVERB_BEFORE = setOf(
        "i", "you", "we", "they", "he", "she", "it", "it's", "that's", "there's", "this", "that", "is", "was", "are", "were",
        "am", "be", "been", "do", "does", "did", "should", "would", "could", "will", "can", "might", "must", "have", "has",
        "had", "not", "really", "i'm", "i'd", "i've", "i'll", "we're", "we'll", "they're", "you're", "don't", "didn't",
        "isn't", "wasn't", "and", "but", "so", "or", "if", "because", "who", "what", "which", "also", "just",
    )

    private fun findCue(words: List<String>): Pair<Int, Int>? {
        for (i in words.indices) for (cue in CUES) {
            if (!cue.indices.all { bare(words.getOrNull(i + it)) == cue[it] }) continue
            if (i == 0 || i + cue.size >= words.size) continue
            val prev = words[i - 1]
            if (prev.endsWith('.') || prev.endsWith('?') || prev.endsWith('!')) continue // starts a new sentence: leave it
            val restLen = sentenceEnd(words, i + cue.size) - (i + cue.size)
            if (restLen == 0) continue
            if (cue.size == 1) { // "actually" is usually just a word: only a short correction right after a noun/number counts
                if (bare(prev) in ADVERB_BEFORE || restLen > 4) continue
            } else if (restLen > 8) continue
            return i to cue.size
        }
        return null
    }

    /** Where the correction ends: the end of its sentence (inclusive), or of the text. */
    private fun sentenceEnd(words: List<String>, from: Int): Int {
        for (j in from until words.size) if (words[j].last() in ".!?") return j + 1
        return words.size
    }

    /** How much of [before] the correction [after] replaces: aligned on a shared word, else numbers, else word for word. */
    private fun cutPoint(before: List<String>, after: List<String>): Int {
        val tailFrom = maxOf(0, before.size - 6)
        for ((j, w) in after.withIndex()) {
            val b = bare(w)
            if (b.isEmpty()) continue
            val idx = (before.size - 1 downTo tailFrom).firstOrNull { bare(before[it]) == b } ?: continue
            if (idx - j >= 0) return idx - j
        }
        if (isNumber(after.first())) {
            var k = before.size
            while (k > 0 && isNumber(before[k - 1])) k--
            if (k < before.size) return k
        }
        return maxOf(0, before.size - minOf(after.size, 2).coerceAtMost(before.size))
    }

    private val NUMBER_WORDS = setOf(
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve", "fifteen",
        "twenty", "thirty", "forty", "fifty", "hundred", "thousand", "half", "quarter", "am", "pm", "a.m.", "p.m.", "o'clock",
    )

    private fun isNumber(w: String): Boolean { val b = bare(w); return b.isNotEmpty() && (b.all { it.isDigit() || it == ':' } || b in NUMBER_WORDS) }

    // ------------------------------------------------------------ spoken punctuation

    private val MARKS = listOf(
        listOf("question", "mark") to "?", listOf("exclamation", "mark") to "!", listOf("exclamation", "point") to "!",
        listOf("full", "stop") to ".", listOf("period") to ".", listOf("comma") to ",", listOf("semicolon") to ";",
        listOf("colon") to ":",
    )

    /** "see you comma tomorrow question mark" -> "see you, tomorrow?"; a spoken mark replaces the model's own. */
    fun punctuation(line: String): String {
        val words = line.split(' ').filter { it.isNotEmpty() }
        if (words.isEmpty()) return line
        val out = ArrayList<String>()
        var i = 0
        while (i < words.size) {
            val hit = if (out.isEmpty()) null else MARKS.firstOrNull { (p, _) -> p.indices.all { bare(words.getOrNull(i + it)) == p[it] } }
            if (hit == null) { out.add(words[i++]); continue }
            val last = out.removeAt(out.size - 1).trimEnd(',', '.', ';', ':', '!', '?')
            out.add(last + hit.second)
            i += hit.first.size
            // the model often punctuates the mark word itself ("comma,") - that came along with it and is dropped
        }
        return out.joinToString(" ")
    }

    // ------------------------------------------------------------ ordinal lists

    private val ORDINALS = listOf(setOf("first", "firstly"), setOf("second", "secondly"), setOf("third", "thirdly"), setOf("fourth", "fourthly"), setOf("fifth", "fifthly"))
    private val NOT_ORDINAL_AFTER = setOf("the", "a", "my", "his", "her", "our", "your", "their", "at", "its", "this", "that")

    /** "I need first milk second eggs" -> "I need:\n1. Milk\n2. Eggs". Needs at least "first" and "second", in order. */
    fun ordinalList(text: String): String {
        if (text.contains('\n')) return text
        val words = text.split(' ').filter { it.isNotEmpty() }
        val marks = ArrayList<Int>()
        var next = 0
        for ((i, w) in words.withIndex()) {
            if (next >= ORDINALS.size) break
            if (bare(w) in ORDINALS[next] && (i == 0 || bare(words[i - 1]) !in NOT_ORDINAL_AFTER)) { marks.add(i); next++ }
        }
        if (marks.size < 2) return text
        val items = marks.indices.map { k ->
            val end = marks.getOrNull(k + 1) ?: words.size
            words.subList(marks[k] + 1, end).joinToString(" ").trim().trimEnd(',', ';', '.').trimStart(',', ' ')
        }
        if (items.any { it.isEmpty() }) return text
        val lead = words.subList(0, marks[0]).joinToString(" ").trim().trimEnd(',', '.', ':', ';')
        val list = items.mapIndexed { k, s -> "${k + 1}. ${s.replaceFirstChar { it.uppercaseChar() }}" }.joinToString("\n")
        return if (lead.isEmpty()) list else "$lead:\n$list"
    }

    private fun bare(word: String?) = word.orEmpty().lowercase().trim { !(it.isLetterOrDigit() || it == '\'' || it == ':') }
}
