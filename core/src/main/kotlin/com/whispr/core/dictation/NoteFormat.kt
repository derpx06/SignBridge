package com.whispr.core.dictation

/** A dictated note laid out for reading: a short title and a body with paragraphs, bullets or numbered steps. */
data class StructuredNote(val title: String, val body: String)

/**
 * Turns cleaned dictation into a tidy note. No model: pure rules, so it is instant and works offline.
 *  - "Shopping list: milk, eggs and bread." becomes a heading with bullets.
 *  - Three or more sentences that start with first / then / finally become numbered steps.
 *  - Everything else is grouped into short paragraphs (three sentences at most).
 * Lines the user already broke (voice command "new line") stay broken.
 */
object NoteFormat {
    private val SENTENCE = Regex("(?<=[.!?])\\s+")
    private val LIST_HEAD = Regex("^([^:\\n]{3,60}):\\s+(.+)$")
    private val ORDINAL = Regex("^(first|second|third|fourth|next|then|after that|finally|lastly)\\b[,]?\\s+", RegexOption.IGNORE_CASE)
    private const val MAX_ITEM_WORDS = 5
    private const val PER_PARAGRAPH = 3

    fun structure(text: String): StructuredNote {
        val lines = text.lines().map { it.trim() }.filter { it.isNotEmpty() }
        if (lines.isEmpty()) return StructuredNote("", "")
        val blocks = ArrayList<String>()
        var title: String? = null
        for (line in lines) {
            if (line.startsWith("- ") || line.startsWith("• ") || line.matches(Regex("^\\d+[.)] .*"))) { blocks += line; continue }
            val sentences = line.split(SENTENCE).filter { it.isNotBlank() }
            val paragraph = ArrayList<String>()
            fun flush() { if (paragraph.isNotEmpty()) { blocks += paragraph.joinToString(" "); paragraph.clear() } }
            var i = 0
            while (i < sentences.size) {
                val s = sentences[i]
                val list = listOf(s)
                val asList = bullets(s)
                if (asList != null) {
                    flush(); blocks += asList.first + "\n" + asList.second.joinToString("\n") { "• $it" }
                    if (title == null) title = asList.first
                    i++; continue
                }
                if (ORDINAL.containsMatchIn(s)) {
                    var j = i
                    while (j < sentences.size && ORDINAL.containsMatchIn(sentences[j])) j++
                    if (j - i >= 3) {
                        flush()
                        blocks += (i until j).mapIndexed { n, k -> "${n + 1}. ${capitalize(sentences[k].replace(ORDINAL, "").trim())}" }.joinToString("\n")
                        i = j; continue
                    }
                }
                paragraph += list
                if (paragraph.size == PER_PARAGRAPH) flush()
                i++
            }
            flush()
        }
        val body = blocks.joinToString("\n\n")
        return StructuredNote(title ?: titleOf(lines.first()), body)
    }

    /** "Heading: a, b and c." -> heading plus items, or null when it does not look like a list. */
    private fun bullets(sentence: String): Pair<String, List<String>>? {
        val m = LIST_HEAD.matchEntire(sentence.trim()) ?: return null
        val tail = m.groupValues[2].trimEnd('.', '!', '?', ' ')
        if (tail.contains(". ")) return null
        val items = tail.split(Regex(",\\s*(?:and\\s+|or\\s+)?|\\s+and\\s+")).map { it.trim() }.filter { it.isNotEmpty() }
        if (items.size < 2 || items.any { it.split(' ').size > MAX_ITEM_WORDS }) return null
        return capitalize(m.groupValues[1].trim()) to items.map { capitalize(it) }
    }

    private fun titleOf(first: String): String {
        val sentence = first.split(SENTENCE).first().trimEnd('.', '!', '?', ',', ';', ':', ' ')
        val words = sentence.split(' ').filter { it.isNotEmpty() }
        val short = words.take(7).joinToString(" ")
        return capitalize(short) + if (words.size > 7) "…" else ""
    }

    private fun capitalize(s: String) = if (s.isEmpty()) s else s.substring(0, 1).uppercase() + s.substring(1)
}
