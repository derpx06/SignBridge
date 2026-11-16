package com.whispr.core.edit

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TextEditorTest {
    private val rules = RulesEditor()

    @Test fun rulesCleanAndFormat() =
        assertEquals("Can you send it by 5 PM?", rules.edit(EditRequest("um can you send it by five p m")).text)

    @Test fun guardRejectsFlippedNegation() =
        assertFalse(EditGuard.accept("I can come to the meeting tomorrow", "I cannot come to the meeting tomorrow", EditMode.Clean))

    @Test fun guardRejectsChangedNumbers() =
        assertFalse(EditGuard.accept("the invoice is for 500 dollars", "the invoice is for 5000 dollars", EditMode.Proofread))

    @Test fun guardRejectsInventedContent() =
        assertFalse(EditGuard.accept("send the report", "Sure! Here is a polished version of your report about quarterly revenue growth", EditMode.Clean))

    @Test fun guardAcceptsLightPolish() =
        assertTrue(EditGuard.accept("send me the the invoice friday", "Send me the invoice Friday.", EditMode.Clean))

    @Test fun rewriteMayChangeWordsButNotNegationsOrNumbers() {
        assertTrue(EditGuard.accept("we need this done by 5 today", "We would appreciate it if this could be completed by 5 today.", EditMode.Rewrite))
        assertFalse(EditGuard.accept("we do not need this today", "We need this today.", EditMode.Rewrite))
        assertFalse(EditGuard.accept("we need 5 chairs", "We need 6 chairs.", EditMode.Rewrite))
    }

    @Test fun guardedEditorFallsBackWhenModelDrifts() {
        val bad = TextEditor { EditResult.Edited("I will never send it.", true) }
        val r = GuardedEditor(bad, rules).edit(EditRequest("i will send it tomorrow morning"))
        assertTrue(r is EditResult.Kept)
        assertEquals("I will send it tomorrow morning.", r.text)
    }

    @Test fun guardedEditorUsesAGoodModelAnswer() {
        val good = TextEditor { EditResult.Edited("Send me the invoice Friday.", true) }
        val r = GuardedEditor(good, rules).edit(EditRequest("send me the the invoice friday"))
        assertEquals(EditResult.Edited("Send me the invoice Friday.", true), r)
    }

    @Test fun guardedEditorSurvivesAModelCrash() {
        val crash = TextEditor { error("native failure") }
        val r = GuardedEditor(crash, rules).edit(EditRequest("hello there my friend"))
        assertTrue(r is EditResult.Kept)
    }

    @Test fun noModelMeansRulesOnly() =
        assertEquals(EditResult.Edited("Hello there my friend.", false), GuardedEditor(null, rules).edit(EditRequest("hello there my friend")))
}
