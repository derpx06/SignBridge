package com.whispr.core.dictation

import com.whispr.core.dictation.VoiceCommands.Command
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class VoiceCommandsTest {
    private fun text(raw: String, enabled: Set<Command> = Command.entries.toSet()) = VoiceCommands.apply(raw, enabled).text

    @Test fun newLineAndNewParagraphInsertBreaks() {
        assertEquals("hello\nworld", text("hello new line world"))
        assertEquals("hello\n\nworld", text("hello new paragraph world"))
        assertEquals("hello\n", text("hello new line"))
    }

    @Test fun commandsIgnoreCaseAndSurroundingPunctuation() {
        assertEquals("hello,\nworld", text("hello, New Line, world"))
    }

    @Test fun scratchThatDropsTheUnfinishedSentence() {
        assertEquals("Hello there.", text("Hello there. I mean scratch that"))
        assertEquals("one\nfour", text("one new line two three scratch that four"))
        assertEquals("", text("never mind scratch that"))
    }

    @Test fun scratchThatRightAfterAFinishedSentenceDoesNothing() {
        assertEquals("Done. ok", text("Done. scratch that ok"))
        assertEquals("Done.", text("Done. scratch that"))
    }

    @Test fun literallyTypesTheWordsInsteadOfRunningTheCommand() {
        assertEquals("type new line please", text("type literally new line please"))
        assertEquals("a literally b", text("a literally b"))
        assertEquals("trailing literally", text("trailing literally"))
    }

    @Test fun pressEnterAndStopListeningSetFlags() {
        val enter = VoiceCommands.apply("send it press enter")
        assertEquals("send it", enter.text)
        assertTrue(enter.pressEnter)
        assertFalse(enter.stop)

        val stop = VoiceCommands.apply("done stop listening ignored words")
        assertEquals("done", stop.text)
        assertTrue(stop.stop)
    }

    @Test fun disabledCommandsAreTypedAsPlainWords() {
        assertEquals("a new line b", text("a new line b", enabled = emptySet()))
        assertEquals("a\nb", text("a new line b", enabled = setOf(Command.NewLine)))
    }

    @Test fun blankInputGivesEmptyText() {
        assertEquals("", text("   "))
        assertEquals("", text("new line"))
    }
}
