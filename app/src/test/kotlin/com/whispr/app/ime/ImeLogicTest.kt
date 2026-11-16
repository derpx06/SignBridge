package com.whispr.app.ime

import android.text.InputType
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ImeLogicTest {
    @Test fun passwordNumberAndPhoneFieldsGetNoVoice() {
        val text = InputType.TYPE_CLASS_TEXT
        assertEquals(FieldKind.Password, fieldKind(text or InputType.TYPE_TEXT_VARIATION_PASSWORD))
        assertEquals(FieldKind.Password, fieldKind(text or InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD))
        assertEquals(FieldKind.Password, fieldKind(text or InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD))
        assertEquals(FieldKind.Password, fieldKind(InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_VARIATION_PASSWORD))
        assertEquals(FieldKind.Number, fieldKind(InputType.TYPE_CLASS_NUMBER))
        assertEquals(FieldKind.Phone, fieldKind(InputType.TYPE_CLASS_PHONE))
        assertEquals(FieldKind.Email, fieldKind(text or InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS))
        assertEquals(FieldKind.Text, fieldKind(text or InputType.TYPE_TEXT_FLAG_MULTI_LINE))
        listOf(FieldKind.Password, FieldKind.Number, FieldKind.Phone).forEach { assertFalse(it.voice) }
        assertTrue(FieldKind.Text.voice)
    }

    @Test fun plainKeysInPasswordFields() {
        val rows = Layouts.board(Layer.Letters, FieldKind.Password, Shift.Off, numberRow = false, globe = true)
        assertTrue(rows.flatten().any { it.kind == K.Char && it.label == "q" })
        assertEquals(4, Layouts.board(Layer.Letters, FieldKind.Number, Shift.Off, numberRow = true, globe = true).size)
    }

    @Test fun suggestionsAndWordDeletion() {
        val lex = Lexicon().apply { rebuild(listOf("Kubernetes"), listOf("meeting tomorrow", "meeting at noon")) }
        assertEquals(listOf("meeting"), lex.suggest("mee"))
        assertEquals(listOf("Kubernetes"), lex.suggest("ku"))
        assertEquals("wor", Lexicon.currentWord("hello wor"))
        assertEquals(6, Lexicon.wordsBack("hello world ", 1))
        assertEquals(12, Lexicon.wordsBack("hello world ", 2))
    }

    @Test fun clipsExpireUnlessPinned() {
        val c = Clips(setOf("pinned"))
        c.add("fresh", 0L)
        assertEquals(listOf("pinned", "fresh"), c.visible(1000L).map { it.text })
        assertEquals("fresh", c.recent(1000L)?.text)
        assertEquals(listOf("pinned"), c.visible(Clips.TTL_MS + 1).map { it.text })
    }
}
