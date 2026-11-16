package com.whispr.core.dictation

/**
 * Whole-utterance commands and style directives (Gboard-style), read from the raw words of one dictation.
 * "undo", "delete that", "clear all", "send"... said on their own run an [Action] and type nothing.
 * "as a list", "make it formal", "email mode" at the start (or as the last sentence) set the [Style] of
 * this dictation. A final sentence of just "send" / "press enter" types the rest and then sends.
 */
object Commands {
    enum class Action { Undo, DeleteThat, DeleteLastWord, Clear, Send, Next, Stop }

    data class Parsed(val text: String, val action: Action? = null, val style: Style? = null, val sendAfter: Boolean = false)

    private val ACTIONS = listOf(
        "undo" to Action.Undo, "undo that" to Action.Undo,
        "scratch that" to Action.DeleteThat, "delete that" to Action.DeleteThat,
        "delete last word" to Action.DeleteLastWord, "delete the last word" to Action.DeleteLastWord,
        "clear" to Action.Clear, "clear all" to Action.Clear, "clear everything" to Action.Clear,
        "send" to Action.Send, "send it" to Action.Send, "send message" to Action.Send, "send that" to Action.Send, "press enter" to Action.Send,
        "next" to Action.Next, "next field" to Action.Next, "go to next" to Action.Next,
        "stop" to Action.Stop, "stop listening" to Action.Stop,
    ).toMap()

    private val SEND_TAIL = setOf("send", "send it", "send message", "press enter")

    private val DIRECTIVES = listOf(
        "as a list" to Style.List, "make it a list" to Style.List, "make a list" to Style.List, "list mode" to Style.List,
        "make it formal" to Style.Formal, "formal mode" to Style.Formal, "be formal" to Style.Formal,
        "make it casual" to Style.Casual, "casual mode" to Style.Casual,
        "email mode" to Style.Email, "as an email" to Style.Email, "make it an email" to Style.Email,
    )

    fun parse(raw: String): Parsed {
        val whole = norm(raw)
        ACTIONS[whole]?.let { return Parsed("", action = it) }
        var text = raw.trim()
        var style: Style? = null
        // leading directive: "make it formal, I would like..." / "As a list. Milk, eggs."
        for ((phrase, s) in DIRECTIVES) {
            val n = phrase.split(' ').size
            val words = text.split(Regex("\\s+"))
            if (words.size >= n && norm(words.take(n).joinToString(" ")) == phrase) {
                style = s
                text = words.drop(n).joinToString(" ").trimStart(',', '.', ':', ' ', '-')
                break
            }
        }
        // last sentence alone: "... eggs. As a list." / "See you soon. Send."
        var sendAfter = false
        repeat(2) {
            val (head, last) = splitLastSentence(text)
            if (head.isEmpty()) return@repeat
            val n = norm(last)
            val directive = DIRECTIVES.firstOrNull { it.first == n }
            when {
                n in SEND_TAIL && !sendAfter -> { sendAfter = true; text = head }
                directive != null && style == null -> { style = directive.second; text = head }
            }
        }
        if (style == null && text.isEmpty()) return Parsed("")
        return Parsed(text.replaceFirstChar { it.uppercaseChar() }, style = style, sendAfter = sendAfter)
    }

    private fun splitLastSentence(text: String): Pair<String, String> {
        val t = text.trimEnd()
        val body = t.trimEnd('.', '!', '?')
        val cut = body.indexOfLast { it == '.' || it == '!' || it == '?' }
        return if (cut < 0) "" to t else t.substring(0, cut + 1).trim() to t.substring(cut + 1).trim()
    }

    /** Lower case, punctuation gone, single spaces ("Send it." -> "send it"). */
    fun norm(s: String) = s.lowercase().replace(Regex("[^\\p{L}\\p{N}' ]"), " ").trim().split(Regex("\\s+")).joinToString(" ")
}

/** Writing style for one dictation. Rule-based, so it is instant and never changes the meaning. */
enum class Style {
    Casual, Formal, List, Email;

    fun apply(text: String): String = when (this) {
        Casual -> casual(text)
        Formal -> formal(text)
        List -> list(text)
        Email -> email(text)
    }

    companion object {
        private val MAIL = listOf("com.google.android.gm", "com.microsoft.office.outlook", "com.yahoo.mobile.client.android.mail",
            "ch.protonmail.android", "me.proton.android.mail", "com.samsung.android.email.provider", "com.fsck.k9", "com.readdle.spark",
            "com.zoho.mail", "net.thunderbird.android", "com.android.email")
        private val CHAT = listOf("com.whatsapp", "com.whatsapp.w4b", "org.telegram", "org.thunderdog.challegram", "com.google.android.apps.messaging",
            "org.thoughtcrime.securesms", "com.instagram.android", "com.facebook.orca", "com.discord", "com.Slack", "com.snapchat.android",
            "com.samsung.android.messaging", "jp.naver.line.android", "com.viber.voip", "com.microsoft.teams")

        /** The default style for an app: mail is formal, chat is casual, everything else is left as said. */
        fun forApp(pkg: String?): Style? = when {
            pkg == null -> null
            MAIL.any { pkg == it || pkg.startsWith("$it.") } -> Formal
            CHAT.any { pkg == it || pkg.startsWith("$it.") } -> Casual
            else -> null
        }

        fun parse(name: String?): Style? = entries.firstOrNull { it.name.equals(name, ignoreCase = true) }

        private val CONTRACTIONS = listOf(
            "can't" to "cannot", "won't" to "will not", "shan't" to "shall not", "n't" to " not", "i'm" to "I am", "'re" to " are",
            "'ve" to " have", "'ll" to " will", "'d" to " would", "it's" to "it is", "that's" to "that is", "there's" to "there is",
            "what's" to "what is", "let's" to "let us", "gonna" to "going to", "wanna" to "want to", "gotta" to "have to",
            "kinda" to "kind of", "yeah" to "yes", "yep" to "yes", "nope" to "no", "ok" to "okay",
        )

        private fun formal(text: String): String {
            var s = text.replace('’', '\'')
            for ((from, to) in CONTRACTIONS) {
                val re = if (from.startsWith("'") || from == "n't") Regex("(?i)(?<=\\p{L})${Regex.escape(from)}\\b") else Regex("(?i)\\b${Regex.escape(from)}\\b")
                s = s.replace(re) { m -> if (m.value.first().isUpperCase() && to.first().isLowerCase()) to.replaceFirstChar { it.uppercaseChar() } else to }
            }
            return s.lines().joinToString("\n") { line -> val t = line.trimEnd(); if (t.isEmpty() || t.last() in ".!?:") t else "$t." }
        }

        /** A one-line chat message loses its final full stop ("On my way."-> "On my way"). Questions keep their mark. */
        private fun casual(text: String): String {
            val t = text.trim()
            val sentences = t.count { it == '.' || it == '!' || it == '?' }
            return if (!t.contains('\n') && sentences == 1 && t.endsWith('.') && !t.endsWith("...")) t.dropLast(1) else t
        }

        private fun list(text: String): String {
            if (text.lines().size > 1) return text
            val parts = text.trim().trimEnd('.').split(Regex("(?<=[.!?])\\s+|,\\s*(?:and\\s+)?|\\s+and\\s+"))
                .map { it.trim().trimEnd('.', ',') }.filter { it.isNotEmpty() }
            if (parts.size < 2) return text
            return parts.joinToString("\n") { "- " + it.replaceFirstChar { c -> c.uppercaseChar() } }
        }

        private val GREETING = Regex("^((?:hi|hello|hey|dear|good morning|good afternoon|good evening)\\b[^,.!?\\n]{0,40})[,.!]?\\s+", RegexOption.IGNORE_CASE)
        private val SIGN_OFF = Regex("\\s*\\b((?:thanks|thank you|many thanks|best regards|kind regards|regards|best|cheers|sincerely)(?: so much)?)[,.!]?(\\s+[\\p{L} ]{1,30})?[.!]?$", RegexOption.IGNORE_CASE)

        /** "Hi John, ... thanks, Manas" -> greeting line, body, sign-off on its own lines. */
        private fun email(text: String): String {
            var body = text.trim()
            var greeting = ""
            GREETING.find(body)?.let { m -> greeting = m.groupValues[1].trim() + ",\n\n"; body = body.substring(m.range.last + 1) }
            var sign = ""
            SIGN_OFF.find(body)?.takeIf { it.range.first > 0 }?.let { m ->
                val name = m.groupValues[2].trim()
                sign = "\n\n" + m.groupValues[1].replaceFirstChar { it.uppercaseChar() } + "," + if (name.isNotEmpty()) "\n" + name.replaceFirstChar { it.uppercaseChar() } else ""
                body = body.substring(0, m.range.first).trim()
            }
            body = body.replaceFirstChar { it.uppercaseChar() }
            if (body.isNotEmpty() && body.last() !in ".!?") body += "."
            return greeting + body + sign
        }
    }
}

/**
 * Voice snippets: saying a trigger on its own ("my address") types the saved text; inside a sentence it must be
 * asked for ("insert my address", "paste my email") so ordinary speech ("check my email") is never replaced.
 */
object Snippets {
    /** [snippets]: trigger -> text. Returns the raw words with snippets expanded, or null when nothing matched. */
    fun expand(raw: String, snippets: Map<String, String>): String? {
        if (snippets.isEmpty()) return null
        val byTrigger = snippets.entries.associate { Commands.norm(it.key) to it.value }.filterKeys { it.isNotEmpty() }
        byTrigger[Commands.norm(raw)]?.let { return it }
        var out = raw
        var hit = false
        for ((trigger, text) in byTrigger.entries.sortedByDescending { it.key.length }) {
            val words = trigger.split(' ').joinToString("[\\s,]+") { Regex.escape(it) }
            val re = Regex("(?i)\\b(?:insert|paste|type)\\s+$words\\b[.,!?]?")
            if (re.containsMatchIn(out)) { out = re.replace(out) { Regex.escapeReplacement(text) }; hit = true }
        }
        return if (hit) out else null
    }
}
