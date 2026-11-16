package com.whispr.core.cloud

import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/**
 * Real API calls through [CloudApi] + [CloudHttp] (the code the phone runs), one per provider whose key is in the
 * environment: VOICIO_KEY_GROQ, VOICIO_KEY_OPENAI, ... (the provider id upper-cased); Azure also needs
 * VOICIO_AZURE_REGION. Skipped when no key is set. Sends the bench clip and checks WER against providers/tests/bench.txt.
 * Never prints a key.
 *
 *   VOICIO_KEY_GROQ=... ./gradlew --no-daemon :core:test --tests '*CloudLiveTest*'
 */
class CloudLiveTest {
    private fun words(s: String) = s.lowercase().map { if (it.isLetterOrDigit() || it == '\'') it else ' ' }.joinToString("").split(' ').filter { it.isNotEmpty() }

    private fun wer(ref: String, hyp: String): Double {
        val r = words(ref); val h = words(hyp)
        var row = IntArray(h.size + 1) { it }
        for (i in r.indices) {
            val next = IntArray(h.size + 1); next[0] = i + 1
            for (j in h.indices) next[j + 1] = minOf(row[j] + if (r[i] == h[j]) 0 else 1, row[j + 1] + 1, next[j] + 1)
            row = next
        }
        return row[h.size].toDouble() / maxOf(1, r.size)
    }

    @Test fun realProvidersTranscribeTheBenchClip() {
        val keyed = CloudProviders.all.mapNotNull { p -> System.getenv("VOICIO_KEY_" + p.id.uppercase())?.trim()?.takeIf { it.isNotEmpty() }?.let { p to it } }
        assumeTrue("no VOICIO_KEY_* variables: live cloud check skipped", keyed.isNotEmpty())
        val bytes = File("../extension/apps/chrome/public/bench.wav").readBytes() // 16 kHz mono 16-bit, 44-byte header
        val pcm = ShortArray((bytes.size - 44) / 2) { ((bytes[44 + 2 * it].toInt() and 0xFF) or (bytes[45 + 2 * it].toInt() shl 8)).toShort() }
        val ref = javaClass.getResource("/bench.txt")!!.readText().trim()
        val lines = ArrayList<String>(); var failed = 0
        println("clip %.1f s; reference: %s".format(pcm.size / 16_000.0, ref))
        for (p in CloudProviders.all) {
            val key = keyed.firstOrNull { it.first == p }?.second
            if (key == null) { lines += "%-12s not called (no VOICIO_KEY_%s)".format(p.id, p.id.uppercase()); continue }
            val model = System.getenv("VOICIO_MODEL_" + p.id.uppercase())?.takeIf { it in p.models } ?: p.defaultModel
            val t0 = System.nanoTime()
            val line = try {
                val text = CloudApi.transcribe(p, key, model, System.getenv("VOICIO_AZURE_REGION").orEmpty(), pcm, "en", CloudHttp::send)
                val w = wer(ref, text); val pass = text.isNotBlank() && w < 0.20
                if (!pass) failed++
                "%-12s %-26s %s %6d ms  WER %5.1f %%  \"%s\"".format(p.id, model, if (pass) "PASS" else "FAIL", (System.nanoTime() - t0) / 1_000_000, w * 100, text)
            } catch (e: CloudException) {
                failed++
                "%-12s %-26s FAIL %6d ms  %s".format(p.id, model, (System.nanoTime() - t0) / 1_000_000, e.kind.message)
            }
            lines += line
        }
        // VOICIO_LIVE=1: also stream the clip through CloudLive (100 ms frames, as the microphone delivers them)
        if (System.getenv("VOICIO_LIVE") == "1") for ((p, key) in keyed.filter { it.first.liveSupported }) {
            val t0 = System.nanoTime()
            lines += try {
                val live = CloudLive(p, key, System.getenv("VOICIO_MODEL_" + p.id.uppercase()).orEmpty(), System.getenv("VOICIO_AZURE_REGION").orEmpty(), "en", {})
                for (i in pcm.indices step 1_600) { live.feed(FloatArray(minOf(1_600, pcm.size - i)) { pcm[i + it] / 32768f }); Thread.sleep(100) }
                val text = live.finish(); val w = wer(ref, text); val pass = text.isNotBlank() && w < 0.20
                if (!pass) failed++
                "%-12s live %-21s %s %6d ms  WER %5.1f %%  \"%s\"".format(p.id, p.liveDefaultModel, if (pass) "PASS" else "FAIL", (System.nanoTime() - t0) / 1_000_000, w * 100, text)
            } catch (e: CloudException) {
                failed++
                "%-12s live %-21s FAIL %6d ms  %s".format(p.id, p.liveDefaultModel, (System.nanoTime() - t0) / 1_000_000, e.kind.message)
            }
        }
        lines.forEach(::println)
        println("${keyed.size} provider(s) called, $failed failed")
        assertTrue(lines.joinToString("\n"), failed == 0)
    }
}
