package com.whispr.app.storage

import java.io.IOException
import org.junit.Assert.assertNull
import org.junit.Test

class EncryptedTranscriptRecoveryTest {
    @Test
    fun `corrupted session reads are ignored instead of crashing startup`() {
        assertNull(readIgnoringCorruption { throw IOException("bad ciphertext") })
    }
}
