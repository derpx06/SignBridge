package com.whispr.core.dictation

/**
 * Smart formatting for a dictated line: turns what the speech model wrote into what a person
 * would type. Pure and rule-based (no model, no network), so it is instant and testable.
 *
 * What it does, in order:
 * 1. e-mail and web addresses ("john dot smith at gmail dot com" -> john.smith@gmail.com)
 * 2. numbers ("twenty five" -> 25), phone-style digit runs, decimals, times ("five thirty p m" -> 5:30 PM),
 *    percent and money ("ten percent" -> 10%, "five dollars" -> $5)
 * 3. spacing around punctuation
 * 4. capitals: sentence starts, "I", weekdays and months
 * 5. a final "." or "?" when the line is a whole sentence without one
 *
 * Single digit words stay words unless a unit follows ("one of them" stays, "five minutes" becomes "5 minutes").
 */
object Formatter {
    data class Options(
        val numbers: Boolean = true,
        val addresses: Boolean = true,
        val punctuation: Boolean = true,
        val capitals: Boolean = true,
    )

    fun format(text: String, options: Options = Options()): String =
        text.split('\n').joinToString("\n") { line(it, options) }

    private fun line(input: String, o: Options): String {
        var s = input.trim()
        if (s.isEmpty()) return s
        if (o.addresses) s = addresses(s)
        if (o.numbers) s = numbers(s)
        if (o.punctuation) s = tidy(s)
        if (o.capitals) s = capitals(s)
        if (o.punctuation) s = terminal(s)
        return s
    }

    // ------------------------------------------------------------ addresses

    private val TLD = "com|org|net|edu|gov|io|co|in|dev|app|ai|me|info"
    private val EMAIL_SEPARATED = Regex("\\b([a-z0-9]+(?: (?:dot|underscore|dash) [a-z0-9]+)+) at ([a-z0-9]+(?: dot [a-z0-9]+)*) dot ($TLD)\\b", RegexOption.IGNORE_CASE)
    private val EMAIL_AFTER_CUE = Regex("(?<=\\b(?:email|e-mail|mail|address|id) (?:is )?)([a-z0-9]+) at ([a-z0-9]+(?: dot [a-z0-9]+)*) dot ($TLD)\\b", RegexOption.IGNORE_CASE)
    private val URL = Regex("\\b((?:www dot )?[a-z0-9]+(?: dot [a-z0-9]+)*) dot ($TLD)\\b", RegexOption.IGNORE_CASE)

    private fun joinSpoken(s: String) = s.replace(" dot ", ".", true).replace(" underscore ", "_", true).replace(" dash ", "-", true).replace(" ", "")

    private fun addresses(s: String): String {
        var r = EMAIL_SEPARATED.replace(s) { m -> joinSpoken(m.groupValues[1]) + "@" + joinSpoken(m.groupValues[2]) + "." + m.groupValues[3] }.lowercaseAddresses()
        r = EMAIL_AFTER_CUE.replace(r) { m -> m.groupValues[1] + "@" + joinSpoken(m.groupValues[2]) + "." + m.groupValues[3] }.lowercaseAddresses()
        return URL.replace(r) { m -> joinSpoken(m.groupValues[1]) + "." + m.groupValues[2] }
    }

    /** Addresses are case-insensitive and read better in lower case. */
    private fun String.lowercaseAddresses(): String =
        Regex("[A-Za-z0-9._-]+@[A-Za-z0-9.-]+").replace(this) { it.value.lowercase() }

    // ------------------------------------------------------------ numbers

    private val UNITS = mapOf(
        "zero" to 0, "one" to 1, "two" to 2, "three" to 3, "four" to 4, "five" to 5, "six" to 6, "seven" to 7, "eight" to 8, "nine" to 9,
        "ten" to 10, "eleven" to 11, "twelve" to 12, "thirteen" to 13, "fourteen" to 14, "fifteen" to 15, "sixteen" to 16,
        "seventeen" to 17, "eighteen" to 18, "nineteen" to 19,
    )
    private val TENS = mapOf("twenty" to 20, "thirty" to 30, "forty" to 40, "fifty" to 50, "sixty" to 60, "seventy" to 70, "eighty" to 80, "ninety" to 90)
    private val SCALES = mapOf("thousand" to 1_000L, "million" to 1_000_000L, "billion" to 1_000_000_000L)
    private val DIGIT_WORDS = (0..9).associateBy { w -> UNITS.entries.first { it.value == w }.key } + ("oh" to 0)
    private val UNIT_WORDS = setOf(
        "percent", "dollars", "rupees", "euros", "cents", "o'clock", "minutes", "hours", "seconds", "days", "weeks", "months", "years",
        "km", "kilometers", "kilometres", "meters", "metres", "miles", "kg", "kilograms", "grams", "degrees", "mb", "gb", "kb", "tb",
    )

    private class Tok(val raw: String) {
        val trail: String = raw.takeLastWhile { it in ".,;:!?" }
        val core: String = raw.dropLast(trail.length).lowercase()
        val lead: Boolean = raw.firstOrNull()?.let { !it.isLetterOrDigit() } ?: false
    }

    private class Run(val value: Long, val end: Int)

    private fun numbers(s: String): String {
        val toks = s.split(' ').filter { it.isNotEmpty() }.flatMap { splitHyphenNumber(it) }.map { Tok(it) }
        val out = ArrayList<String>()
        var i = 0
        while (i < toks.size) {
            val t = toks[i]
            if (t.lead) { out += t.raw; i++; continue }
            val seq = digitSequence(toks, i)
            if (seq != null) { out += seq.first + toks[seq.second - 1].trail; i = seq.second; continue }
            val clock = time(toks, i)
            if (clock != null) { out += clock.first; i = clock.second; continue }
            val run = run(toks, i)
            if (run == null) { out += t.raw; i++; continue }
            var end = run.end
            var text = group(run.value)
            var trail = toks[end - 1].trail
            var decimal = false
            if (trail.isEmpty() && end < toks.size && toks[end].core == "point") {
                var j = end + 1
                val sb = StringBuilder()
                while (j < toks.size && DIGIT_WORDS.containsKey(toks[j].core) && toks[j].core != "oh") { sb.append(DIGIT_WORDS.getValue(toks[j].core)); trail = toks[j].trail; j++; if (trail.isNotEmpty()) break }
                if (sb.isNotEmpty()) { text += "." + sb; end = j; decimal = true }
            }
            val nextUnit = end < toks.size && toks[end].core in UNIT_WORDS
            val convert = run.value >= 10 || decimal || nextUnit
            if (convert) { out += text + trail; i = end } else { for (k in i until end) out += toks[k].raw; i = end }
        }
        return money(out.joinToString(" "))
    }

    /** "twenty-five" -> "twenty", "five" so the number grammar sees two words. */
    private fun splitHyphenNumber(raw: String): List<String> {
        val parts = raw.split('-')
        if (parts.size == 2 && parts[0].lowercase() in TENS && parts[1].trimEnd('.', ',', ';', ':', '!', '?').lowercase() in UNITS) return parts
        return listOf(raw)
    }

    private fun group(v: Long): String = if (v >= 2100) "%,d".format(v) else v.toString() // years 1000-2099 stay unseparated

    private fun digitSequence(toks: List<Tok>, start: Int): Pair<String, Int>? {
        var j = start
        val sb = StringBuilder()
        while (j < toks.size && !toks[j].lead && DIGIT_WORDS.containsKey(toks[j].core)) {
            sb.append(DIGIT_WORDS.getValue(toks[j].core)); j++
            if (toks[j - 1].trail.isNotEmpty()) break
        }
        return if (j - start >= 4) sb.toString() to j else null
    }

    private fun run(toks: List<Tok>, start: Int): Run? {
        var i = start
        var total = 0L
        var cur = 0L
        var kind = 0 // 0 none, 1 unit, 2 teen, 3 tens, 4 hundred, 5 scale
        var any = false
        var hadScale = false
        while (i < toks.size) {
            val t = toks[i]
            if (t.lead) break
            val w = t.core
            val last = t.trail.isNotEmpty()
            val next = toks.getOrNull(i + 1)?.core
            var consumed = true
            when {
                w == "a" && i == start && next in setOf("hundred", "thousand", "million", "billion") -> { cur = 1; kind = 1; any = true; consumed = true; i++; continue }
                w == "and" && any && (hadScale || kind == 4) && next != null && (next in UNITS || next in TENS) -> { i++; continue }
                w in UNITS && UNITS.getValue(w) in 1..9 && kind in setOf(0, 3, 4, 5) -> { cur += UNITS.getValue(w); kind = 1 }
                w == "zero" && kind == 0 && !any -> { kind = 1 }
                w in UNITS && UNITS.getValue(w) in 10..19 && kind in setOf(0, 4, 5) -> { cur += UNITS.getValue(w); kind = 2 }
                w in TENS && kind in setOf(0, 4, 5) -> { cur += TENS.getValue(w); kind = 3 }
                w == "hundred" && kind == 1 && cur in 1..9 -> { cur *= 100; kind = 4 }
                w in SCALES && cur > 0 && kind in setOf(1, 2, 3, 4) -> { total += cur * SCALES.getValue(w); cur = 0; kind = 5; hadScale = true }
                else -> consumed = false
            }
            if (!consumed) break
            any = true
            i++
            if (last) break
        }
        if (!any || i == start) return null
        // a dangling "and" is not part of the number
        return Run(total + cur, i)
    }

    private val AMPM = Regex("^[ap]\\.?m\\.?$")

    /** "five thirty p m" / "five p m" / "5 pm" -> "5:30 PM" / "5 PM". */
    private fun time(toks: List<Tok>, start: Int): Pair<String, Int>? {
        val hour = run(toks, start) ?: return null
        if (hour.value !in 1..12 || toks[hour.end - 1].trail.isNotEmpty()) return null
        var j = hour.end
        var minute: String? = null
        run(toks, j)?.let { m ->
            if (m.value in 10..59) { minute = "%02d".format(m.value); j = m.end }
        }
        if (minute == null && j + 1 < toks.size && toks[j].core == "oh") {
            val u = UNITS[toks[j + 1].core]
            if (u != null && u in 1..9) { minute = "%02d".format(u); j += 2 }
        }
        val (suffix, end) = ampm(toks, j) ?: return null
        val tail = toks[end - 1].trail.trimEnd('.')
        return (if (minute != null) "${hour.value}:$minute $suffix" else "${hour.value} $suffix") + tail to end
    }

    private fun ampm(toks: List<Tok>, j: Int): Pair<String, Int>? {
        val a = toks.getOrNull(j) ?: return null
        if (AMPM.matches(a.raw.trimEnd(',', ';', ':', '!', '?').lowercase())) return a.raw.first().uppercase() + "M" to j + 1
        val b = toks.getOrNull(j + 1) ?: return null
        if (a.core in setOf("a", "p") && b.core.trimEnd('.') == "m") return a.core.uppercase() + "M" to j + 2
        return null
    }

    private val PERCENT = Regex("(\\d[\\d,]*(?:\\.\\d+)?) percent\\b")
    private val DOLLARS = Regex("\\b(\\d[\\d,]*(?:\\.\\d+)?) dollars?\\b")
    private val RUPEES = Regex("\\b(\\d[\\d,]*(?:\\.\\d+)?) rupees?\\b")
    private val EUROS = Regex("\\b(\\d[\\d,]*(?:\\.\\d+)?) euros?\\b")

    private fun money(s: String): String =
        s.let { PERCENT.replace(it) { m -> m.groupValues[1] + "%" } }
            .let { DOLLARS.replace(it) { m -> "$" + m.groupValues[1] } }
            .let { RUPEES.replace(it) { m -> "₹" + m.groupValues[1] } }
            .let { EUROS.replace(it) { m -> "€" + m.groupValues[1] } }

    // ------------------------------------------------------------ punctuation

    private fun tidy(s: String): String =
        s.replace(Regex("\\s+"), " ")
            .replace(Regex("\\s+([,.;:!?])"), "$1")
            .replace(Regex("([,;])\\1+"), "$1")
            .replace(Regex("([,;!?])(?=[A-Za-z])"), "$1 ")
            .trim()

    private val STRONG_QUESTION = Regex("^(?:(?:so|and|ok|okay|well|hey|hi|hello|please),?\\s+)?(?:what|why|how|when|where|who|whom|whose|which|can you|could you|would you|will you|do you|did you|does it|are you|is it|is there|are we|shall we|should we|have you|how about)\\b", RegexOption.IGNORE_CASE)

    private fun terminal(s: String): String {
        if (s.isEmpty()) return s
        val words = s.split(' ').size
        val last = s.last()
        val oneSentence = s.dropLast(1).none { it in ".!?" }
        // "Hello, how are you" is a question: judge the clause after the last comma as well as the line start.
        val asks = STRONG_QUESTION.containsMatchIn(s) || STRONG_QUESTION.containsMatchIn(s.substringAfterLast(',').trim())
        if (last == '.' && oneSentence && words >= 3 && asks) return s.dropLast(1) + "?"
        if ((last.isLetterOrDigit() || last == '%' || last == ')') && words >= 4) {
            return s + if (asks && oneSentence) "?" else "."
        }
        return s
    }

    // ------------------------------------------------------------ capitals

    private val DAYS_MONTHS = Regex("(?<![\\w@./-])(monday|tuesday|wednesday|thursday|friday|saturday|sunday|january|february|april|june|july|august|september|october|november|december)(?![\\w@./-])")
    private val I_WORD = Regex("(?<![\\w@./'-])i(?![\\w@./-])")
    private val I_CONTRACTION = Regex("(?<![\\w@./-])i('(?:m|ll|ve|d))\\b")
    private val SENTENCE_START = Regex("(^|[.!?]\\s+)([a-z])")

    private fun looksLikeAddress(word: String) = '@' in word || Regex("[a-z0-9]\\.[a-z]").containsMatchIn(word)

    private fun capitals(s: String): String {
        var r = I_CONTRACTION.replace(s) { "I" + it.groupValues[1] }
        r = I_WORD.replace(r, "I")
        r = DAYS_MONTHS.replace(r) { it.value.replaceFirstChar(Char::uppercase) }
        return SENTENCE_START.replace(r) { m ->
            val after = r.substring(m.range.last).takeWhile { it != ' ' }
            if (looksLikeAddress(after)) m.value else m.groupValues[1] + m.groupValues[2].uppercase()
        }
    }
}
