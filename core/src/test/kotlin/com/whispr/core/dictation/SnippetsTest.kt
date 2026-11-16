package com.whispr.core.dictation

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SnippetsTest {
    private val saved = mapOf("my address" to "12 Baker Street, London", "sign-off" to "Best,\nManas")

    @Test fun triggerAloneTypesTheSnippet() {
        assertEquals("12 Baker Street, London", Snippets.expand("My address.", saved))
        assertEquals("Best,\nManas", Snippets.expand("sign off", saved))
    }

    @Test fun insideASentenceItMustBeAskedFor() {
        assertEquals("Send it to 12 Baker Street, London please", Snippets.expand("Send it to insert my address please", saved))
        assertNull(Snippets.expand("Check my address book", saved))
        assertNull(Snippets.expand("Hello there", saved))
        assertNull(Snippets.expand("my address", emptyMap()))
    }
}
