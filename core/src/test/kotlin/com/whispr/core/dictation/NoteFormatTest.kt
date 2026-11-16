package com.whispr.core.dictation

import org.junit.Assert.assertEquals
import org.junit.Test

class NoteFormatTest {
    @Test fun listsBecomeBulletsWithATitle() {
        val n = NoteFormat.structure("Shopping list: milk, eggs and bread.")
        assertEquals("Shopping list", n.title)
        assertEquals("Shopping list\n• Milk\n• Eggs\n• Bread", n.body)
    }

    @Test fun sequencesBecomeNumberedSteps() {
        val n = NoteFormat.structure("First, open the app. Then tap the bubble. Finally speak.")
        assertEquals("1. Open the app.\n2. Tap the bubble.\n3. Speak.", n.body)
    }

    @Test fun longTextIsSplitIntoParagraphs() {
        val n = NoteFormat.structure("One is here. Two is here. Three is here. Four is here.")
        assertEquals("One is here. Two is here. Three is here.\n\nFour is here.", n.body)
        assertEquals("One is here", n.title)
    }

    @Test fun existingLineBreaksAreKept() {
        val n = NoteFormat.structure("Call mom.\nBuy gifts.")
        assertEquals("Call mom.\n\nBuy gifts.", n.body)
    }

    @Test fun longFirstSentenceGivesAShortTitle() {
        assertEquals("Meeting with the design team about the…", NoteFormat.structure("meeting with the design team about the launch plan tomorrow.").title)
    }

    @Test fun ordinaryColonSentencesAreNotLists() {
        assertEquals("Note: remember to send the invoice today after lunch.", NoteFormat.structure("Note: remember to send the invoice today after lunch.").body)
    }

    @Test fun emptyInput() = assertEquals(StructuredNote("", ""), NoteFormat.structure("  \n "))
}
