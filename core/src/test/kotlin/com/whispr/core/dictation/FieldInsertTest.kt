package com.whispr.core.dictation

import org.junit.Test
import org.junit.Assert.assertEquals

class FieldInsertTest {
    private fun merge(current: String?, hint: String? = null, showingHint: Boolean = false, s: Int = -1, e: Int = -1, text: String) =
        FieldInsert.merge(current, hint, showingHint, s, e, text)

    @Test fun emptyField() = assertEquals("Hello there.", merge("", text = "Hello there.").text)

    @Test fun whatsappPlaceholderIsNotText() {
        assertEquals("Hello", merge("Message", hint = "Message", text = "Hello").text)
        assertEquals("Hello", merge("Message", showingHint = true, text = "Hello").text)
    }

    @Test fun realTextEqualToHintIsOnlyIgnoredWhenItIsTheHint() =
        assertEquals("Message Hello", merge("Message", hint = null, text = "Hello").text)

    @Test fun appendsWithSpace() = assertEquals("Hi Bob", merge("Hi", s = 2, e = 2, text = "Bob").text)

    @Test fun noSpaceAfterNewlineOrOpener() {
        assertEquals("Hi\nBob", merge("Hi\n", s = 3, e = 3, text = "Bob").text)
        assertEquals("(Bob", merge("(", s = 1, e = 1, text = "Bob").text)
    }

    @Test fun noSpaceBeforePunctuation() = assertEquals("Hi.", merge("Hi", s = 2, e = 2, text = ".").text)

    @Test fun replacesSelection() = assertEquals("Hi there", merge("Hi Bob", s = 3, e = 6, text = "there").text)

    @Test fun insertsInMiddleAndSpacesBothSides() {
        val r = merge("Hi Bob", s = 2, e = 2, text = "there")
        assertEquals("Hi there Bob", r.text)
        assertEquals(8, r.caret) // right after "there", before the added trailing space
    }

    @Test fun reversedSelectionAndOutOfRange() = assertEquals("Hi there", merge("Hi Bob", s = 99, e = 3, text = "there").text)

    @Test fun metaAiSearchPlaceholderIsNotText() {
        val r = FieldInsert.merge("Ask Meta AI or search", "Ask Meta AI or search", false, 0, 0, "Sarvesh")
        assertEquals("Sarvesh", r.text)
        assertEquals(true, FieldInsert.isPlaceholder("Ask Meta AI or search", "Ask Meta AI or search", false))
        assertEquals(true, FieldInsert.isPlaceholder("anything", null, true))
        assertEquals(false, FieldInsert.isPlaceholder("Hello", null, false))
        assertEquals(false, FieldInsert.isPlaceholder("Message", null, false)) // real text that merely looks like a hint
    }
}

class PlaceholderHeuristicTest {
    @org.junit.Test fun placeholdersAreRecognised() {
        for (p in listOf("Search…", "Ask Meta AI or Search", "Type a message")) org.junit.Assert.assertTrue(p, FieldInsert.isPlaceholder(p, null, false))
        for (p in listOf("Hello there", "Meeting at 5", "")) org.junit.Assert.assertFalse(p, FieldInsert.isPlaceholder(p, null, false))
    }
}
