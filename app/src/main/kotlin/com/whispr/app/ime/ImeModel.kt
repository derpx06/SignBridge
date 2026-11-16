package com.whispr.app.ime

import com.whispr.app.flow.FlowState

/** What kind of field has focus. Password, number and phone fields get plain keys only: no voice, suggestions or clipboard. */
enum class FieldKind { Text, Email, Url, Password, Number, Phone;
    val voice: Boolean get() = this == Text || this == Email || this == Url
}

enum class Layer { Letters, Symbols, Symbols2, Emoji, Clipboard, Menu, History;
    /** Panels that take the keys' place (the voice panel never covers them). */
    val panel: Boolean get() = this == Emoji || this == Clipboard || this == Menu || this == History
}

/** On-device rewrites of the selection or the last dictation (long-press the style chip). */
enum class Rewrite(val needsModel: Boolean) { Formal(true), Casual(true), Shorter(true), Grammar(true), List(false), Email(false) }
enum class Shift { Off, Once, Lock }
enum class EnterKind { Return, Send, Search, Go, Next, Done }

/** One copied item. Unpinned items live only in memory and disappear after [Clips.TTL_MS]. */
data class Clip(val text: String, val at: Long, val pinned: Boolean = false)

/** Everything the keyboard draws. Immutable; the service swaps in a new copy on every change. */
data class ImeUi(
    val flow: FlowState = FlowState(),
    val field: FieldKind = FieldKind.Text,
    val enter: EnterKind = EnterKind.Return,
    val shift: Shift = Shift.Off,
    val layer: Layer = Layer.Letters,
    val suggestions: List<String> = emptyList(),
    /** A just-copied item offered in the bar (Gboard's paste chip). */
    val recentClip: Clip? = null,
    val clips: List<Clip> = emptyList(),
    val recentEmoji: List<String> = emptyList(),
    /** The style chip: the name shown, and whether it is the app's default (not the user's choice). */
    val style: String = "",
    val styleAuto: Boolean = true,
    val cloud: String? = null,
    val micGranted: Boolean = true,
    val oneHanded: Int = 0,
    val landscape: Boolean = false,
    val floating: Boolean = false,
    val floatX: Int = -1,
    val floatY: Int = -1,
    val numberRow: Boolean = true,
    val dark: Boolean = true,
    val height: Float = 1f,
    val continuous: Boolean = false,
    /** Text that could not be typed and is on the clipboard (the Copy chip). */
    val copied: String? = null,
    val globe: Boolean = true,
    /** Where audio goes: "On device" or "Cloud · Groq". */
    val privacy: String = "On device",
    /** IME_FLAG_NO_PERSONALIZED_LEARNING: nothing is stored or learned. */
    val incognito: Boolean = false,
    /** Why the style chip shows what it does ("Gmail · mail apps are formal"). */
    val styleWhy: String = "",
    val whisper: Boolean = false,
    val autoStopMs: Long = 0L,
    /** The on-device rewrite model is installed (Formal, Casual, Shorter, Fix grammar). */
    val rewriteModel: Boolean = false,
    /** A rewrite is running. */
    val rewriting: Boolean = false,
    /** Recent dictations to reinsert (swipe up on the bar). Empty in incognito and password fields. */
    val history: List<String> = emptyList(),
    /** Words from the last dictation offered as "+ word" (add to dictionary). */
    val newWords: List<String> = emptyList(),
)

/** Callbacks from the keyboard UI. All default to nothing so previews and tests can pass a bare object. */
interface ImeActions {
    fun micDown() {}
    fun micUp() {}
    fun text(s: String) {}
    fun backspace() {}
    fun deleteWord() {}
    /** Backspace swipe: select [n] words before the caret (0 = none). */
    fun selectWordsBack(n: Int) {}
    fun finishSelect(delete: Boolean) {}
    fun cursor(steps: Int) {}
    fun space() {}
    fun enter() {}
    fun shift() {}
    fun layer(l: Layer) {}
    fun globe() {}
    fun picker() {}
    fun suggestion(word: String) {}
    fun paste(clip: Clip) {}
    fun pin(clip: Clip) {}
    fun emoji(e: String) {}
    fun nextStyle() {}
    fun oneHanded(side: Int) {}
    fun floating(on: Boolean) {}
    fun floatMoved(x: Int, y: Int) {}
    fun floatBounds(left: Int, top: Int, right: Int, bottom: Int) {}
    fun done() {}
    fun undo() {}
    fun retry() {}
    fun copyAgain() {}
    fun openApp() {}
    fun feedback(kind: Feedback) {}
    /** The panel's X: cancel the dictation, or close a notice. */
    fun cancel() {}
    fun openMenu() {}
    fun rewrite(kind: Rewrite) {}
    fun toggleWhisper() {}
    fun cycleAutoStop() {}
    fun reinsert(text: String) {}
    fun addWord(word: String) {}
    fun theme() {}
}

/** What kind of field this is, from its EditorInfo input type. Password/number/phone: plain keys only. */
fun fieldKind(inputType: Int): FieldKind {
    val cls = inputType and android.text.InputType.TYPE_MASK_CLASS
    val variation = inputType and android.text.InputType.TYPE_MASK_VARIATION
    return when (cls) {
        android.text.InputType.TYPE_CLASS_NUMBER ->
            if (variation == android.text.InputType.TYPE_NUMBER_VARIATION_PASSWORD) FieldKind.Password else FieldKind.Number
        android.text.InputType.TYPE_CLASS_PHONE -> FieldKind.Phone
        android.text.InputType.TYPE_CLASS_DATETIME -> FieldKind.Number
        android.text.InputType.TYPE_CLASS_TEXT -> when (variation) {
            android.text.InputType.TYPE_TEXT_VARIATION_PASSWORD, android.text.InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD,
            android.text.InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD -> FieldKind.Password
            android.text.InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS, android.text.InputType.TYPE_TEXT_VARIATION_WEB_EMAIL_ADDRESS -> FieldKind.Email
            android.text.InputType.TYPE_TEXT_VARIATION_URI -> FieldKind.Url
            else -> FieldKind.Text
        }
        else -> FieldKind.Text // TYPE_NULL: a terminal or game; plain text keys work there
    }
}

enum class Feedback { Key, Delete, Space, Enter, Long, Tick }

/** A key on the board. [alts] are offered on long-press (drag sideways to choose). */
data class KeySpec(val kind: K, val label: String = "", val out: String = label, val alts: List<String> = emptyList(), val weight: Float = 1f)

enum class K { Char, Shift, Back, Space, Enter, ToSymbols, ToSymbols2, ToLetters, Globe, Emoji, Spacer }

object Layouts {
    private val ALTS = mapOf(
        'q' to "1%", 'w' to "2\\", 'e' to "3éèêëē", 'r' to "4₹", 't' to "5[", 'y' to "6]", 'u' to "7üúûù", 'i' to "8íïîì", 'o' to "9óöôòœ", 'p' to "0}",
        'a' to "@áàâäåæ", 's' to "#ß", 'd' to "$", 'f' to "_", 'g' to "&", 'h' to "-", 'j' to "+", 'k' to "(", 'l' to ")",
        'z' to "*", 'x' to "\"", 'c' to "'ç", 'v' to ":", 'b' to ";", 'n' to "!ñ", 'm' to "?",
    )

    private fun letters(row: String, shift: Shift) = row.map { c ->
        val up = shift != Shift.Off
        KeySpec(K.Char, if (up) c.uppercase() else c.toString(), alts = ALTS[c]?.map { a -> if (up) a.uppercase() else a.toString() }.orEmpty())
    }

    private fun chars(row: String) = row.map { KeySpec(K.Char, it.toString()) }

    private val NUMBER_ROW = "1234567890".map { KeySpec(K.Char, it.toString()) }

    fun board(layer: Layer, field: FieldKind, shift: Shift, numberRow: Boolean, globe: Boolean): List<List<KeySpec>> {
        if (field == FieldKind.Number) return numberPad(phone = false)
        if (field == FieldKind.Phone) return numberPad(phone = true)
        val comma = when (field) {
            FieldKind.Email -> KeySpec(K.Char, "@")
            FieldKind.Url -> KeySpec(K.Char, "/")
            else -> KeySpec(K.Char, ",", alts = listOf(";", ":", "!", "?"))
        }
        val switcher = if (globe) KeySpec(K.Globe, weight = 1f) else KeySpec(K.Emoji, weight = 1f)
        val period = KeySpec(K.Char, ".", alts = listOf("…", "?", "!", ",", "-", "'", "\""))
        val bottom = { toggle: KeySpec -> listOf(toggle, comma, switcher, KeySpec(K.Space, weight = 4.4f), period, KeySpec(K.Enter, weight = 1.5f)) }
        return when (layer) {
            Layer.Symbols -> listOf(
                chars("1234567890"),
                chars("@#\$_&-+()/"),
                listOf(KeySpec(K.ToSymbols2, "=\\<", weight = 1.5f)) + chars("*\"':;!?") + KeySpec(K.Back, weight = 1.5f),
                bottom(KeySpec(K.ToLetters, "ABC", weight = 1.5f)),
            )
            Layer.Symbols2 -> listOf(
                chars("~`|•√π÷×¶∆"),
                chars("£¢€¥^°={}\\"),
                listOf(KeySpec(K.ToSymbols, "?123", weight = 1.5f)) + chars("%©®™✓[]") + KeySpec(K.Back, weight = 1.5f),
                bottom(KeySpec(K.ToLetters, "ABC", weight = 1.5f)),
            )
            else -> listOfNotNull(
                if (numberRow) NUMBER_ROW else null,
                letters("qwertyuiop", shift),
                listOf(KeySpec(K.Spacer, weight = 0.5f)) + letters("asdfghjkl", shift) + KeySpec(K.Spacer, weight = 0.5f),
                listOf(KeySpec(K.Shift, weight = 1.5f)) + letters("zxcvbnm", shift) + KeySpec(K.Back, weight = 1.5f),
                bottom(KeySpec(K.ToSymbols, "?123", weight = 1.5f)),
            )
        }
    }

    private fun numberPad(phone: Boolean): List<List<KeySpec>> = listOf(
        chars("123") + KeySpec(K.Char, if (phone) "+" else "-"),
        chars("456") + KeySpec(K.Space),
        chars("789") + KeySpec(K.Back),
        if (phone) chars("*0#") + KeySpec(K.Enter) else chars(",0.") + KeySpec(K.Enter),
    )
}

/** Word completions from the user's dictionary and their own recent dictations. */
class Lexicon {
    private var words: List<Pair<String, Int>> = emptyList() // word, weight; sorted by weight

    fun rebuild(dictionary: List<String>, recent: List<String>) {
        val counts = HashMap<String, Int>()
        fun add(w: String, n: Int) {
            val t = w.trim { !it.isLetterOrDigit() }
            if (t.length >= 3 && t.any { it.isLetter() }) counts[t] = (counts[t] ?: 0) + n
        }
        dictionary.forEach { line -> line.split(' ').forEach { add(it, 50) } }
        recent.forEach { text -> text.split(Regex("\\s+")).forEach { add(it, 1) } }
        // one spelling per word: the most used capitalisation wins
        words = counts.entries.groupBy { it.key.lowercase() }
            .map { (_, forms) -> forms.maxBy { it.value }.key to forms.sumOf { it.value } }
            .sortedByDescending { it.second }
    }

    /** Up to [n] words starting with [prefix] (case-insensitive), keeping a typed capital. */
    fun suggest(prefix: String, n: Int = 3): List<String> {
        if (prefix.isEmpty()) return emptyList()
        val lower = prefix.lowercase()
        return words.asSequence().map { it.first }
            .filter { it.length > prefix.length && it.lowercase().startsWith(lower) }
            .take(n)
            .map { if (prefix.first().isUpperCase()) it.replaceFirstChar { c -> c.uppercaseChar() } else it }
            .toList()
    }

    companion object {
        /** The word being typed: letters, digits and apostrophes right before the caret. */
        fun currentWord(before: CharSequence): String {
            var i = before.length
            while (i > 0 && (before[i - 1].isLetterOrDigit() || before[i - 1] == '\'')) i--
            return before.substring(i)
        }

        /** How many characters [words] words back from the end of [before] covers (trailing spaces count with the word). */
        fun wordsBack(before: CharSequence, words: Int): Int {
            var i = before.length
            repeat(words) {
                while (i > 0 && before[i - 1].isWhitespace()) i--
                if (i > 0 && !before[i - 1].isLetterOrDigit()) { i--; return@repeat } // a lone mark counts as a word
                while (i > 0 && !before[i - 1].isWhitespace() && (before[i - 1].isLetterOrDigit() || before[i - 1] == '\'')) i--
            }
            return before.length - i
        }
    }
}

/** Copied items: pinned ones stay, the rest expire after an hour and are never written to disk. */
class Clips(pinned: Set<String>) {
    private val items = ArrayList<Clip>(pinned.map { Clip(it, 0L, pinned = true) })

    fun add(text: String, now: Long) {
        if (text.isBlank()) return
        val pinnedAlready = items.any { it.pinned && it.text == text }
        items.removeAll { !it.pinned && it.text == text }
        if (!pinnedAlready) items.add(0, Clip(text, now))
        while (items.count { !it.pinned } > MAX) items.remove(items.last { !it.pinned })
    }

    fun visible(now: Long): List<Clip> {
        items.removeAll { !it.pinned && now - it.at > TTL_MS }
        return items.sortedWith(compareByDescending<Clip> { it.pinned }.thenByDescending { it.at })
    }

    /** Newest item copied in the last two minutes, for the paste chip. */
    fun recent(now: Long): Clip? = items.firstOrNull { !it.pinned && now - it.at < RECENT_MS }

    fun togglePin(clip: Clip): Set<String> {
        val i = items.indexOfFirst { it.text == clip.text }
        if (i >= 0) items[i] = items[i].copy(pinned = !items[i].pinned, at = if (items[i].pinned) System.currentTimeMillis() else items[i].at)
        return items.filter { it.pinned }.map { it.text }.toSet()
    }

    companion object {
        const val TTL_MS = 60 * 60_000L
        const val RECENT_MS = 2 * 60_000L
        private const val MAX = 30
    }
}

/** The system emoji set by category (filtered at run time to what this phone's font can draw). */
object EmojiSet {
    private fun range(a: Int, b: Int) = (a..b).toList()
    private fun cps(vararg x: Int) = x.toList()

    val categories: List<Pair<String, List<Int>>> = listOf(
        "😀" to range(0x1F600, 0x1F637) + range(0x1F641, 0x1F644) + range(0x1F910, 0x1F92F) + range(0x1F970, 0x1F976) + cps(0x1F97A, 0x1F978, 0x1F9D0),
        "👋" to range(0x1F44A, 0x1F450) + cps(0x1F44B, 0x270C, 0x270D, 0x1F590, 0x1F596) + range(0x1F64C, 0x1F64F) + range(0x1F918, 0x1F91F) + range(0x1F440, 0x1F445) + range(0x1F466, 0x1F469) + cps(0x1F4AA, 0x1F933),
        "❤" to cps(0x2764, 0x1F9E1, 0x1F49B, 0x1F49A, 0x1F499, 0x1F49C, 0x1F5A4, 0x1F90D, 0x1F90E) + range(0x1F493, 0x1F49F) + cps(0x1F48B, 0x1F4AF, 0x1F525, 0x2728, 0x2B50, 0x1F389, 0x1F38A, 0x2705, 0x274C, 0x2753, 0x2757),
        "🐻" to range(0x1F400, 0x1F43C) + range(0x1F980, 0x1F99F) + range(0x1F330, 0x1F343),
        "🍔" to range(0x1F345, 0x1F37F) + range(0x1F950, 0x1F96F),
        "⚽" to range(0x1F380, 0x1F393) + range(0x1F3A0, 0x1F3CA) + cps(0x26BD, 0x26BE, 0x1F3D0, 0x1F3D3),
        "🚗" to range(0x1F680, 0x1F6C5) + range(0x1F3D4, 0x1F3DF),
        "💡" to range(0x1F4A1, 0x1F4FC) + range(0x1F50A, 0x1F52E),
    )

    fun glyph(cp: Int): String = String(Character.toChars(cp)) + if (cp < 0x1F000) "️" else ""
}
