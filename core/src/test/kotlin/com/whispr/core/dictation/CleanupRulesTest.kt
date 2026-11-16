package com.whispr.core.dictation

import com.whispr.core.dictation.CleanupRules.guardrail
import com.whispr.core.dictation.CleanupRules.ruleClean
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Ported from the unit tests in `cleanup.rs`, plus edge cases. */
class CleanupRulesTest {

    @Test fun ruleCleanDropsFillersAndStutters() {
        assertEquals("So the report is ready", ruleClean("um so the the report is uh ready"))
        assertEquals("", ruleClean("uh um"))
    }

    @Test fun guardrailAcceptsCleanupAndRejectsInvention() {
        val raw = "so um i need to send the the report by uh friday no wait make that thursday"
        assertTrue(guardrail(raw, "I need to send the report by Thursday."))
        assertTrue(guardrail("i think the answer is forty two", "I think the answer is 42."))
        assertTrue(guardrail("we're gonna ship it", "We're going to ship it."))
        assertFalse(guardrail(raw, "Please find attached the quarterly financial summary for review."))
        assertFalse(guardrail("send the report to maria by friday and copy the whole team", "Send it."))
    }

    @Test fun guardrailRejectsFlippedMeaning() {
        assertFalse(guardrail("i do not want to ship it on friday", "I want to ship it on Friday."))
        assertFalse(guardrail("we should ship it on friday", "We should not ship it on Friday."))
        assertTrue(guardrail("i don't want to ship it on friday", "I do not want to ship it on Friday."))
        assertTrue(guardrail("i can't make it on friday", "I cannot make it on Friday."))
        assertTrue(guardrail("call me at 15 minutes past three", "Call me at 15 minutes past three."))
        assertFalse(guardrail("call me at 15 minutes past three", "Call me at 50 minutes past three."))
        assertFalse(guardrail("please call me tomorrow morning about it", "Please call me tomorrow at 9 about it."))
    }

    @Test fun shortPhrasesAndLegitRepeatsSurvive() {
        assertEquals("Yes", ruleClean("yes"))
        assertEquals("I had had enough", ruleClean("i had had enough"))
        assertEquals("It was very very good", ruleClean("it was very very good"))
        assertEquals("We should go", ruleClean("we we should go"))
    }

    // ---- additions ----

    @Test fun ruleCleanIgnoresPunctuationAndCaseWhenSpottingFillersAndStutters() {
        assertEquals("The build passed.", ruleClean("Um, the build passed."))
        assertEquals("Hello", ruleClean("Hello HELLO hello"))
    }

    @Test fun ruleCleanCollapsesWhitespaceIncludingNonBreakingSpace() {
        assertEquals("Leading and trailing", ruleClean("  leading \t and  trailing \n"))
        assertEquals("", ruleClean(""))
    }

    @Test fun ruleCleanCapitalisesWithFullUnicodeCaseMapping() {
        assertEquals("SStraße is long", ruleClean("ßtraße is long"))
        assertEquals("Élan vital", ruleClean("élan vital"))
    }

    @Test fun punctuationOnlyWordsAreNeverTreatedAsStutters() {
        assertEquals("... ... ...", ruleClean("... ... ..."))
    }

    @Test fun guardrailEmptyContentOnlyAcceptsEmptyOutput() {
        assertTrue(guardrail("uh um", ""))
        assertFalse(guardrail("uh um", "something"))
    }

    @Test fun guardrailRejectsOutputShorterThanHalfTheContent() {
        assertFalse(guardrail("one two three four five six", "one two"))
        assertTrue(guardrail("one two three four five six", "one two three"))
    }

    @Test fun guardrailNegationCountIsExactButNoIsExempt() {
        assertTrue(guardrail("no wait make that thursday", "Make that Thursday."))
        assertFalse(guardrail("it works nothing else", "It works."))
    }
}
