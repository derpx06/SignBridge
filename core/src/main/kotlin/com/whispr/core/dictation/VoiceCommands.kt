package com.whispr.core.dictation

/**
 * Spoken editing commands (port of the command loop in the Voicio extension's `pipeline.ts`).
 *
 * "new line" and "new paragraph" become `\n` and `\n\n`; "scratch that" drops the words back to the
 * previous line break or sentence end (it does nothing right after a finished sentence, as in the TS
 * code); "press enter" and "stop listening" are reported as flags. Saying "literally" before a
 * command types its words instead of running it.
 */
object VoiceCommands {
    enum class Command(val phrase: List<String>) {
        NewLine(listOf("new", "line")),
        NewParagraph(listOf("new", "paragraph")),
        ScratchThat(listOf("scratch", "that")),
        PressEnter(listOf("press", "enter")),
        StopListening(listOf("stop", "listening")),
    }

    /** [text] may contain `\n`; [pressEnter] and [stop] mirror the matching commands. */
    data class Result(val text: String, val pressEnter: Boolean = false, val stop: Boolean = false)

    private const val BREAK = "\u0000" // internal line-break marker; never in user text

    /** Runs the commands in [enabled] over [raw]; words are otherwise passed through untouched. */
    fun apply(raw: String, enabled: Set<Command> = Command.entries.toSet()): Result {
        val words = CleanupRules.words(raw)
        val out = ArrayList<String>()
        var pressEnter = false
        var stop = false
        var i = 0
        while (i < words.size) {
            if (bare(words[i]) == "literally" && i + 1 < words.size) {
                val escaped = Command.entries.firstOrNull { matches(words, i + 1, it.phrase) }
                if (escaped != null) {
                    val n = escaped.phrase.size
                    out.addAll(words.subList(i + 1, i + 1 + n))
                    i += 1 + n
                    continue
                }
            }
            val hit = Command.entries.firstOrNull { it in enabled && matches(words, i, it.phrase) }
            if (hit == null) {
                out.add(words[i++])
                continue
            }
            i += hit.phrase.size
            when (hit) {
                Command.NewLine -> out.add(BREAK)
                Command.NewParagraph -> { out.add(BREAK); out.add(BREAK) }
                Command.ScratchThat ->
                    while (out.isNotEmpty() && out.last() != BREAK && !out.last().endsWithSentenceEnd()) out.removeAt(out.size - 1)
                Command.PressEnter -> pressEnter = true
                Command.StopListening -> { stop = true; i = words.size }
            }
        }
        return Result(join(out), pressEnter, stop)
    }

    private fun String.endsWithSentenceEnd() = isNotEmpty() && last() in ".!?"

    private fun matches(words: List<String>, at: Int, phrase: List<String>) =
        phrase.withIndex().all { (k, p) -> bare(words.getOrNull(at + k) ?: "") == p }

    private fun isWordChar(c: Char) = Character.isLetterOrDigit(c) || c == '\'' || Character.isSurrogate(c)

    /** Lower-case word with leading/trailing punctuation stripped ("New," -> "new"). */
    private fun bare(word: String) = word.lowercase().trim { !isWordChar(it) }

    /** Joins words with single spaces; a break trims the space before it and starts a new line. */
    private fun join(parts: List<String>): String {
        val text = StringBuilder()
        for (p in parts) {
            if (p == BREAK) {
                text.setLength(text.trimEnd(' ').length)
                text.append('\n')
            } else {
                if (text.isNotEmpty() && text.last() != '\n') text.append(' ')
                text.append(p)
            }
        }
        return if (text.isBlank()) "" else text.toString()
    }
}
