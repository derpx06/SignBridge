package com.whispr.core.dictation

/**
 * Works out the new contents of a text field when dictated text is inserted at the caret.
 * Pure, so the awkward cases (placeholder text, selections, spacing) are unit-tested.
 */
object FieldInsert {
    data class Result(val text: String, val caret: Int, val inserted: String)

    /**
     * @param current what the field reports as its text. Many apps (WhatsApp, Gmail) report their
     *   placeholder ("Message") here when the field is empty, so it is ignored when [showingHint] is
     *   true or when it equals [hint].
     * @param selStart caret or selection start, -1 if unknown (then the end is used)
     */
    fun merge(
        current: CharSequence?,
        hint: CharSequence?,
        showingHint: Boolean,
        selStart: Int,
        selEnd: Int,
        dictated: String,
    ): Result {
        val raw = current?.toString().orEmpty()
        val isPlaceholder = showingHint || (hint != null && hint.isNotEmpty() && raw == hint.toString())
        val existing = if (isPlaceholder) "" else raw
        val a = (if (selStart < 0 || isPlaceholder) existing.length else selStart).coerceIn(0, existing.length)
        val b = (if (selEnd < 0 || isPlaceholder) existing.length else selEnd).coerceIn(0, existing.length)
        val from = minOf(a, b)
        val to = maxOf(a, b)
        val before = existing.substring(0, from)
        val after = existing.substring(to)
        val lead = if (before.isNotEmpty() && !before.last().isWhitespace() && before.last() !in OPENERS && !dictated.first().isNoSpaceBefore()) " " else ""
        val trail = if (after.isNotEmpty() && !after.first().isWhitespace() && after.first() !in CLOSERS_AND_PUNCT && !dictated.last().isWhitespace()) " " else ""
        val inserted = lead + dictated + trail
        return Result(before + inserted + after, before.length + lead.length + dictated.length, inserted)
    }

    /**
     * True when [current] is the field's placeholder rather than the user's text. Apps report the
     * placeholder as the text of an empty field ("Message", "Ask Meta AI or search"), sometimes with
     * the hint flag set and sometimes only as an equal [hint].
     */
    fun isPlaceholder(current: CharSequence?, hint: CharSequence?, showingHint: Boolean): Boolean =
        showingHint || (hint != null && hint.isNotEmpty() && current?.toString() == hint.toString()) || looksLikePlaceholder(current?.toString())

    /** Apps like WhatsApp report their placeholder as plain text with no hint flag, so recognise the usual wording. */
    fun looksLikePlaceholder(text: String?): Boolean {
        val t = text?.trim()?.lowercase() ?: return false
        if (t.isEmpty() || t.length > 48) return false
        return t.endsWith("…") || t.endsWith("...") || t in KNOWN ||
            (t.startsWith("ask ") && t.contains(" or search")) || t.startsWith("type a ") || t.startsWith("write a ") || t.startsWith("search ")
    }
    private val KNOWN = setOf("type a message", "write a message", "add a comment", "enter text", "type here", "say something")

    private fun Char.isNoSpaceBefore() = this in CLOSERS_AND_PUNCT
    private const val OPENERS = "([{\"'“‘@#/"
    private const val CLOSERS_AND_PUNCT = ".,;:!?)]}%\"'”’"
}
