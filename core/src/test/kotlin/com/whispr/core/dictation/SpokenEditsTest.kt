package com.whispr.core.dictation

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SpokenEditsTest {
    private val none = Dictionary(emptyList())
    private fun spoken(raw: String) = Pipeline.process(raw, none, Pipeline.Options(spoken = true)).text

    @Test fun backtrackReplacesTheRestatedPart() {
        assertEquals("coffee at 3", SpokenEdits.backtrack("coffee at 2 actually 3"))
        assertEquals("Coffee at 3.", SpokenEdits.backtrack("Coffee at 2, actually 3."))
        assertEquals("send it to Mary", SpokenEdits.backtrack("send it to John actually to Mary"))
        assertEquals("meet on Wednesday at one", SpokenEdits.backtrack("meet on Tuesday at noon actually Wednesday at one"))
        assertEquals("call me at six thirty", SpokenEdits.backtrack("call me at five no wait six thirty"))
        assertEquals("I love dogs", SpokenEdits.backtrack("I love cats actually dogs"))
    }

    @Test fun actuallyAsAnOrdinaryWordStays() {
        assertEquals("I actually think so", SpokenEdits.backtrack("I actually think so"))
        assertEquals("it's actually fine", SpokenEdits.backtrack("it's actually fine"))
        assertEquals("Actually, let's go", SpokenEdits.backtrack("Actually, let's go"))
        assertEquals("It went well. Actually it went great", SpokenEdits.backtrack("It went well. Actually it went great"))
    }

    @Test fun spokenPunctuationReplacesTheModelsGuess() {
        assertEquals("see you, tomorrow?", SpokenEdits.punctuation("see you comma tomorrow question mark"))
        assertEquals("done.", SpokenEdits.punctuation("done, full stop."))
        assertEquals("Are you coming?", spoken("are you coming question mark"))
    }

    @Test fun ordinalsBecomeAList() {
        assertEquals("I need:\n1. Milk\n2. Eggs\n3. Bread", spoken("I need first milk second eggs third bread"))
        assertEquals("The first time and the second time.", spoken("the first time and the second time"))
        assertEquals("First things first", spoken("first things first"))
    }

    @Test fun pipelineKeepsOldBehaviourWithoutSpoken() {
        assertEquals("Coffee at 2 actually 3.", Pipeline.process("coffee at 2 actually 3", none).text)
        assertEquals("Coffee at 3", spoken("um coffee at 2 actually 3")) // short lines get no final period (Formatter)
    }

    @Test fun commandsOnTheirOwnRunAnAction() {
        assertEquals(Commands.Action.Undo, Commands.parse("Undo.").action)
        assertEquals(Commands.Action.DeleteThat, Commands.parse("Delete that.").action)
        assertEquals(Commands.Action.DeleteLastWord, Commands.parse("delete last word").action)
        assertEquals(Commands.Action.Clear, Commands.parse("Clear all.").action)
        assertEquals(Commands.Action.Send, Commands.parse("Send.").action)
        assertEquals(Commands.Action.Next, Commands.parse("Next.").action)
        assertNull(Commands.parse("I will send it tomorrow.").action)
        assertEquals("I will send it tomorrow.", Commands.parse("I will send it tomorrow.").text)
    }

    @Test fun directivesAndSend() {
        Commands.parse("Make it formal, I don't know.").let { assertEquals(Style.Formal, it.style); assertEquals("I don't know.", it.text) }
        Commands.parse("Milk, eggs and bread. As a list.").let { assertEquals(Style.List, it.style); assertEquals("Milk, eggs and bread.", it.text) }
        Commands.parse("See you soon. Send.").let { assertTrue(it.sendAfter); assertEquals("See you soon.", it.text) }
        Commands.parse("Email mode hi John thanks").let { assertEquals(Style.Email, it.style) }
    }

    @Test fun styles() {
        assertEquals("I do not know, it is fine and I cannot go.", Style.Formal.apply("I don't know, it's fine and I can't go"))
        assertEquals("On my way", Style.Casual.apply("On my way."))
        assertEquals("Are you there?", Style.Casual.apply("Are you there?"))
        assertEquals("- Milk\n- Eggs\n- Bread", Style.List.apply("Milk, eggs and bread."))
        assertEquals("Hi John,\n\nThe report is ready.\n\nThanks,\nManas", Style.Email.apply("Hi John, the report is ready. Thanks, Manas"))
        assertEquals(Style.Formal, Style.forApp("com.google.android.gm"))
        assertEquals(Style.Casual, Style.forApp("com.whatsapp"))
        assertNull(Style.forApp("com.android.chrome"))
    }
}
