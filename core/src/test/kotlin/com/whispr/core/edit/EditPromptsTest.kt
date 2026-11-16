package com.whispr.core.edit

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class EditPromptsTest {
    @Test fun cleanPromptUsesTheTrainedControlLine() {
        val p = EditPrompts.build(EditRequest("um hello there"))
        assertTrue(p.contains("[Styling: semi-formal] [Structure: prose] [Context: general]\num hello there<|im_end|>"))
        assertTrue(p.endsWith("<|im_start|>assistant\n<think>\n\n</think>\n\n"))
    }

    @Test fun proofreadAndShortenUseTheirInstructions() {
        assertTrue(EditPrompts.build(EditRequest("x", EditMode.Proofread)).contains("Fix spelling, grammar and punctuation"))
        assertTrue(EditPrompts.build(EditRequest("x", EditMode.Rewrite)).contains("shorter and clearer"))
    }

    @Test fun answerLengthIsBounded() {
        assertEquals(32, EditPrompts.maxNewTokens("hi"))
        assertEquals(512, EditPrompts.maxNewTokens("a".repeat(5000)))
    }
}
