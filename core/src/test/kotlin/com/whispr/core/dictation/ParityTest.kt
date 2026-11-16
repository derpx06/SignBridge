package com.whispr.core.dictation

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Golden rows produced by running the real Rust `rule_clean` / `guardrail` (copied verbatim from
 * `engine/src/cleanup.rs` into a scratch crate) over the same inputs.
 */
class ParityTest {
    private fun rows(name: String): List<List<String>> {
        val text = javaClass.getResourceAsStream("/parity/$name")!!.readBytes().toString(Charsets.UTF_8)
        return text.removeSuffix("\n").split("\n").map { it.split("\t") }
    }

    @Test fun ruleCleanMatchesRust() {
        val rows = rows("cleanup.tsv")
        assertTrue(rows.size >= 30)
        for ((input, expected) in rows) assertEquals("ruleClean(\"$input\")", expected, CleanupRules.ruleClean(input))
    }

    @Test fun guardrailMatchesRust() {
        val rows = rows("guardrail.tsv")
        assertTrue(rows.size >= 25)
        for ((input, output, expected) in rows) {
            assertEquals("guardrail(\"$input\", \"$output\")", expected.toBoolean(), CleanupRules.guardrail(input, output))
        }
    }
}
