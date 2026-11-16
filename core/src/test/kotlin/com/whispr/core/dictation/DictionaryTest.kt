package com.whispr.core.dictation

import org.junit.Assert.assertEquals
import org.junit.Test

/** Ported from the unit tests in `replacements.rs`. */
class DictionaryTest {
    private fun dict(vararg rules: Pair<String, String>) = Dictionary(rules.map { Dictionary.Entry(it.first, it.second) })

    @Test fun replacesGloballyOnUnicodePhraseBoundariesWithoutDisturbingPunctuation() {
        val d = dict("cafe au lait" to "café au lait", "open code" to "OpenCode", "über" to "Über")
        assertEquals(
            "OpenCode, OpenCode. café au lait; café au lait! Über.",
            d.replace("OPEN CODE, open code. café au lait; cafe au lait! ÜBER."),
        )
        assertEquals("reopen code, open codes, übercool", d.replace("reopen code, open codes, übercool"))
    }

    @Test fun replacesPhrasesAdjacentToUnspacedScripts() {
        val d = dict("issue" to "Issue", "open code" to "OpenCode", "严肃" to "严肃", "이슈" to "issue")
        assertEquals("这个Issue很重要", d.replace("这个issue很重要"))
        assertEquals("用OpenCode写", d.replace("用open code写"))
        assertEquals("ใช้Issueบ่อย", d.replace("ใช้issueบ่อย"))
        assertEquals("issue를 만들었다", d.replace("이슈를 만들었다"))
        assertEquals("Issue很重要", d.replace("issue很重要"))
        assertEquals("这个很重要Issue", d.replace("这个很重要issue"))
        assertEquals("issue_id", d.replace("issue_id"))
        assertEquals("issue2", d.replace("issue2"))
        assertEquals("reissues", d.replace("reissues"))
    }

    @Test fun longestMatchWinsAndReplacementsDoNotRecurse() {
        val d = dict("open" to "closed", "open code" to "OpenCode", "code then open" to "combined", "closed" to "shut")
        assertEquals("closed combined", d.replace("open code then open"))
    }

    @Test fun outputsDoNotTriggerOtherRulesDuringTheSamePass() {
        val d = dict("open code" to "OpenCode", "alpha" to "beta release", "beta" to "gamma")
        assertEquals("OpenCode beta release", d.replace("OPEN CODE alpha"))
        assertEquals("gamma release", d.replace("beta release"))
    }

    @Test fun aMatchRejectedInsideAWordDoesNotHideAnOverlappingPhrase() {
        val d = dict("I I" to "I", "the the" to "the")
        assertEquals("Hi I think so.", d.replace("Hi I I think so."))
        assertEquals("Breathe the air.", d.replace("Breathe the the air."))
    }

    @Test fun duplicatePhrasesUseSettingsOrderDeterministically() {
        assertEquals("first", dict("hex" to "first", "HEX" to "second").replace("hex"))
    }

    @Test fun emptyReplacementsDeletePhrasesAndCollapseSurroundingSpacingAndPunctuation() {
        val d = dict("um" to "", "uh" to "", "thank you" to "")
        assertEquals("I think so.", d.replace("I um think so."))
        assertEquals("I, think so.", d.replace("I, um, think so."))
        assertEquals("I, think so.", d.replace("I, um, uh, think so."))
        assertEquals("The build passed.", d.replace("Um, the build passed."))
        assertEquals("The build passed.", d.replace("Um, uh, the build passed."))
        assertEquals("the build passed.", d.replace("um, the build passed."))
        assertEquals("I think so.", d.replace("I think so um."))
        assertEquals("I think so.", d.replace("I think so, um."))
        assertEquals("I think so", d.replace("I think so, um"))
        assertEquals("", d.replace("Thank you."))
        assertEquals("Hello.", d.replace("Hello. Thank you."))
        assertEquals("Hello. The build passed.", d.replace("Hello. Thank you. The build passed."))
    }

    // ---- additions ----

    @Test fun blankPhrasesAreIgnoredAndAnEmptyDictionaryIsANoOp() {
        assertEquals("hello world", dict("  " to "x", "" to "y").replace("hello world"))
        assertEquals("hello world", Dictionary(emptyList()).replace("hello world"))
    }

    @Test fun phrasesAreTrimmedAndTreatedAsLiteralText() {
        val d = dict("  c++  " to "C plus plus", "a.b" to "dot")
        assertEquals("I like C plus plus", d.replace("I like c++"))
        assertEquals("axb dot", d.replace("axb a.b"))
    }

    @Test fun aDeletionAtTheStartOfAnUppercaseSentenceCapitalisesTheNextWord() {
        assertEquals("The end", dict("well" to "").replace("Well the end"))
    }
}
