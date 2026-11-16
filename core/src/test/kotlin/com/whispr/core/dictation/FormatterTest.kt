package com.whispr.core.dictation

import org.junit.Assert.assertEquals
import org.junit.Test

class FormatterTest {
    private fun f(s: String) = Formatter.format(s)
    private fun check(vararg rows: Pair<String, String>) = rows.forEach { (input, expected) -> assertEquals("input: $input", expected, f(input)) }

    @Test fun cardinals() = check(
        "I need twenty five chairs." to "I need 25 chairs.",
        "we have one hundred and twenty people" to "We have 120 people.",
        "it costs a hundred" to "It costs 100",
        "there were two thousand five hundred visitors today" to "There were 2,500 visitors today.",
        "fifteen" to "15",
        "she is twenty-five years old" to "She is 25 years old.",
        "three million people came" to "3,000,000 people came",
    )

    @Test fun smallNumbersStayWordsWithoutAUnit() = check(
        "one of them is ready today" to "One of them is ready today.",
        "I have two dogs and a cat" to "I have two dogs and a cat.",
        "he is the one for me" to "He is the one for me.",
    )

    @Test fun smallNumbersBecomeDigitsWithAUnit() = check(
        "wait five minutes please" to "Wait 5 minutes please.",
        "it takes three hours" to "It takes 3 hours.",
        "about two kilometers away" to "About 2 kilometers away.",
        "it is nine o'clock" to "It is 9 o'clock.",
    )

    @Test fun decimals() = check(
        "growth was three point five percent" to "Growth was 3.5%",
        "pi is three point one four" to "Pi is 3.14",
    )

    @Test fun digitRuns() = check(
        "call nine eight seven six five four three two one zero" to "Call 9876543210",
        "my pin is one two three four" to "My pin is 1234.",
    )

    @Test fun times() = check(
        "meet me at five thirty p m" to "Meet me at 5:30 PM.",
        "the call is at five p m" to "The call is at 5 PM.",
        "wake me at seven a m" to "Wake me at 7 AM.",
        "at nine oh five a m sharp" to "At 9:05 AM sharp.",
        "see you at five thirty p.m." to "See you at 5:30 PM.",
    )

    @Test fun percentAndMoney() = check(
        "ten percent off" to "10% off",
        "it costs five dollars" to "It costs $5",
        "pay fifty rupees" to "Pay ₹50",
        "that is twenty euros" to "That is €20",
        "one hundred percent sure" to "100% sure",
    )

    @Test fun emails() = check(
        "send it to john dot smith at gmail dot com" to "Send it to john.smith@gmail.com.",
        "my email is john at gmail dot com" to "My email is john@gmail.com.",
        "contact sales underscore team at example dot org" to "Contact sales_team@example.org",
    )

    @Test fun urls() = check(
        "visit voicio dot com for details" to "Visit voicio.com for details.",
        "go to www dot example dot org" to "Go to www.example.org",
        "look at google dot com" to "Look at google.com",
    )

    @Test fun plainWordAtIsNotAnEmail() = check(
        "meet me at home tomorrow morning please" to "Meet me at home tomorrow morning please.",
        "look at the dot on the map" to "Look at the dot on the map.",
    )

    @Test fun spacingAndPunctuation() = check(
        "hello ,how are you" to "Hello, how are you?",
        "wait , what" to "Wait, what",
        "this is great !!" to "This is great!!",
        "yes,, I will" to "Yes, I will",
    )

    @Test fun capitals() = check(
        "i think i'm ready and i'll go" to "I think I'm ready and I'll go.",
        "we meet on monday in october" to "We meet on Monday in October.",
        "it works. then we ship it." to "It works. Then we ship it.",
        "I may go in march" to "I may go in march.",
    )

    @Test fun addressesKeepTheirCaseAtASentenceStart() = check(
        "mail john dot smith at gmail dot com. thanks" to "Mail john.smith@gmail.com. Thanks",
    )

    @Test fun questions() = check(
        "can you send me the invoice before friday" to "Can you send me the invoice before Friday?",
        "what time is the meeting tomorrow" to "What time is the meeting tomorrow?",
        "Can you send me the invoice." to "Can you send me the invoice?",
        "how are you doing today" to "How are you doing today?",
    )

    @Test fun statementsGetAFullStop() = check(
        "i will send the report tomorrow morning" to "I will send the report tomorrow morning.",
        "thanks" to "Thanks",
        "ok sounds good" to "Ok sounds good",
    )

    @Test fun alreadyCleanTextIsLeftAlone() = check(
        "Hello, can you hear me?" to "Hello, can you hear me?",
        "The meeting is at 5:30 PM." to "The meeting is at 5:30 PM.",
        "We shipped it. Thanks!" to "We shipped it. Thanks!",
    )

    @Test fun formattingTwiceChangesNothing() {
        listOf(
            "i need twenty five chairs by five thirty p m", "can you email john dot smith at gmail dot com",
            "it costs five dollars and ten percent off", "hello ,how are you", "visit voicio dot com",
        ).forEach { s ->
            val once = f(s)
            assertEquals("not idempotent: $s", once, f(once))
        }
    }

    @Test fun multipleLinesAreFormattedSeparately() =
        assertEquals("Hello there\nWe meet at 5 PM.", f("hello there\nwe meet at five p m"))

    @Test fun optionsCanTurnStagesOff() {
        val off = Formatter.Options(numbers = false, addresses = false, punctuation = false, capitals = false)
        assertEquals("i need twenty five chairs", Formatter.format("i need twenty five chairs", off))
        assertEquals("I need twenty five chairs", Formatter.format("i need twenty five chairs", off.copy(capitals = true)))
    }

    @Test fun neverChangesTheWordsThemselves() {
        // Apart from number/address spans, the same words come out in the same order.
        val input = "please tell sam and priya that the report is ready and the team can start reviewing it today"
        val out = f(input)
        assertEquals(input.split(' '), out.trimEnd('.', '?').split(' ').map { it.lowercase() })
    }

    @Test fun isFast() {
        val line = "i need twenty five chairs by five thirty p m and also ten percent off for john dot smith at gmail dot com " .repeat(5)
        repeat(50) { f(line) } // warm up
        val t = System.nanoTime()
        repeat(200) { f(line) }
        val msPerLine = (System.nanoTime() - t) / 200 / 1e6
        assert(msPerLine < 5.0) { "formatter took $msPerLine ms per line" }
    }
}
