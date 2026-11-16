package com.whispr.core.dictation

import com.whispr.core.dictation.VoiceCommands.Command
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PipelineTest {
    private val dictionary = Dictionary(listOf(Dictionary.Entry("kube control", "kubectl"), Dictionary.Entry("so", "")))

    @Test fun commandsThenCleanupThenDictionary() {
        val result = Pipeline.process("um open kube control new line the the next thing press enter", dictionary)
        assertEquals("Open kubectl\nThe next thing", result.text)
        assertTrue(result.pressEnter)
    }

    @Test fun cleanupWorksOnEachLineSoBreaksSurvive() {
        assertEquals("One\n\nTwo", Pipeline.process("one new paragraph two", Dictionary(emptyList())).text)
    }

    @Test fun scratchThatSeesRawWordsNotCleanedOnes() {
        assertEquals("Keep this.", Pipeline.process("keep this. um wrong bit scratch that", Dictionary(emptyList())).text)
    }

    @Test fun dictionaryRunsAfterCleanupSoItsOutputIsNotAltered() {
        val d = Dictionary(listOf(Dictionary.Entry("ay", "the the")))
        assertEquals("the the now", Pipeline.process("ay now", d).text)
    }

    @Test fun optionsCanTurnCleanupAndCommandsOff() {
        val none = Dictionary(emptyList())
        val off = Pipeline.Options(commands = emptySet(), cleanup = false, format = false)
        assertEquals("um a new line b", Pipeline.process("um a new line b", none, off).text)
    }

    @Test fun stopListeningFlagIsReported() {
        assertTrue(Pipeline.process("bye stop listening", Dictionary(emptyList()), Pipeline.Options(setOf(Command.StopListening))).stop)
    }

    @Test fun smartFormattingRunsAfterCleanupAndBeforeTheDictionary() {
        val dict = Dictionary(listOf(Dictionary.Entry("acme", "ACME Corp")))
        val on = Pipeline.process("um so can you send acme the invoice by five thirty p m", dict).text
        assertEquals("So can you send ACME Corp the invoice by 5:30 PM?", on)
    }

    @Test fun smartFormattingCanBeSwitchedOff() {
        val off = Pipeline.process("can you send it by five thirty p m", Dictionary(emptyList()), Pipeline.Options(format = false)).text
        assertEquals("Can you send it by five thirty p m", off)
    }
}
