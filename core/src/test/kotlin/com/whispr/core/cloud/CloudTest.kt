package com.whispr.core.cloud

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CloudTest {
    private fun sample(n: String) = javaClass.getResource("/cloud/$n.json")!!.readText()
    private fun kind(f: () -> Unit): CloudErrorKind? = try { f(); null } catch (e: CloudException) { e.kind }
    private fun p(id: String) = CloudProviders.byId(id)!!
    private val pcm = ShortArray(16_000)

    /** A fake network: answers with [replies] in order and records every request (the key must never reach a URL). */
    private class Fake(vararg replies: Pair<Int, String>) {
        val queue = ArrayDeque(replies.toList())
        val seen = ArrayList<HttpReq>()
        val send: (HttpReq) -> HttpResp = { r ->
            seen += r
            assertFalse("key in URL: ${r.url}", "SECRET" in r.url)
            assertFalse("SECRET" in r.toString())
            val (s, b) = queue.removeFirst(); HttpResp(s, b)
        }
        fun body(i: Int) = String(seen[i].body, Charsets.UTF_8)
    }

    private fun run(id: String, vararg replies: Pair<Int, String>, lang: String? = null, model: String = "", region: String = ""): Pair<String, Fake> {
        val f = Fake(*replies)
        return CloudApi.transcribe(p(id), " SECRET ", model, region, pcm, lang, f.send, sleep = {}) to f
    }

    @Test fun registryParsesAndEveryStyleIsImplemented() {
        assertEquals(16, CloudProviders.all.size) // 15 providers, Lemonfox twice (EU endpoint)
        assertEquals(6, CloudProviders.excluded.size)
        for (pr in CloudProviders.all) {
            assertTrue(pr.id, pr.defaultModel in pr.models)
            assertTrue(pr.id, pr.name.isNotEmpty() && pr.maker.isNotEmpty() && pr.priceNote.isNotEmpty() && pr.speedNote.isNotEmpty())
            assertTrue(pr.id, pr.spec["style"] in setOf("multipart", "raw", "json_audio", "upload_poll"))
        }
        assertTrue(p("assemblyai").slow && p("soniox").slow && p("azure").needsRegion && p("openrouter").experimental)
        assertEquals("westeurope.api.cognitive.microsoft.com", p("azure").hostFor("westeurope"))
        assertNull(CloudProviders.byId("nope"))
    }

    @Test fun wavHeader() {
        val w = Wav.encode(shortArrayOf(1, -1, 256))
        assertEquals("RIFF", String(w, 0, 4)); assertEquals("WAVE", String(w, 8, 4))
        assertEquals(44 + 6, w.size)
        fun le(o: Int, n: Int) = (0 until n).sumOf { (w[o + it].toInt() and 0xFF) shl (8 * it) }
        assertEquals(1, le(20, 2)); assertEquals(1, le(22, 2)); assertEquals(16_000, le(24, 4)); assertEquals(16, le(34, 2)); assertEquals(6, le(40, 4))
        assertEquals(0xFF, w[46].toInt() and 0xFF) // -1 little endian
    }

    @Test fun multipartOpenAiCompatible() {
        for (id in listOf("openai", "groq", "mistral", "together")) {
            val (text, f) = run(id, 200 to sample("openai"), lang = "hi")
            assertEquals(id, "Hello, world.", text)
            val r = f.seen.single()
            assertEquals("Bearer SECRET", r.headers["Authorization"])
            assertTrue(r.contentType!!.startsWith("multipart/form-data; boundary="))
            assertTrue(id, f.body(0).contains("name=\"file\"; filename=\"audio.wav\"") && f.body(0).contains("name=\"language\"\r\n\r\nhi\r\n"))
        }
        assertEquals("https://api.groq.com/openai/v1/audio/transcriptions", run("groq", 200 to sample("openai")).second.seen[0].url)
        assertTrue(run("together", 200 to sample("openai")).second.body(0).contains("name=\"language\"\r\n\r\nauto\r\n"))
        assertEquals("https://eu-api.lemonfox.ai/v1/audio/transcriptions", run("lemonfoxeu", 200 to sample("openai")).second.seen[0].url)
    }

    @Test fun multipartVariants() {
        val (t, e) = run("elevenlabs", 200 to sample("elevenlabs"))
        assertEquals("x", t); assertEquals("SECRET", e.seen[0].headers["xi-api-key"]); assertTrue(e.body(0).contains("name=\"model_id\"\r\n\r\n${p("elevenlabs").defaultModel}\r\n"))
        val (_, c) = run("cartesia", 200 to "{\"text\":\"ink\"}")
        assertEquals("2026-08-14", c.seen[0].headers["Cartesia-Version"]); assertEquals("https://api.cartesia.ai/stt", c.seen[0].url)
        val (st, s) = run("sarvam", 200 to sample("sarvam"), lang = "hi")
        assertEquals("नमस्ते नमस्ते", st); assertEquals("SECRET", s.seen[0].headers["api-subscription-key"]); assertTrue(s.body(0).contains("saaras:v4") && s.body(0).contains("\r\nhi-IN\r\n"))
        assertTrue(run("sarvam", 200 to sample("sarvam")).second.body(0).contains("\r\nunknown\r\n"))
        // 30 s is over Sarvam's 29 s cap: two requests, texts joined
        val long = Fake(200 to "{\"transcript\":\"one\"}", 200 to "{\"transcript\":\" two \"}")
        assertEquals("one two", CloudApi.transcribe(p("sarvam"), "k", "", "", ShortArray(30 * 16_000), null, long.send))
        assertEquals(2, long.seen.size)
        val (at, a) = run("azure", 200 to sample("azure"), lang = "de", region = "westeurope")
        assertEquals("Hallo Welt.", at)
        assertEquals("https://westeurope.api.cognitive.microsoft.com/speechtotext/transcriptions:transcribe?api-version=2025-10-15", a.seen[0].url)
        assertEquals("SECRET", a.seen[0].headers["Ocp-Apim-Subscription-Key"])
        assertTrue(a.body(0).contains("name=\"audio\"; filename=") && a.body(0).contains("{\"locales\":[\"de-DE\"]}"))
        assertEquals(CloudErrorKind.NeedsRegion, kind { run("azure", region = "") })
        assertEquals(CloudErrorKind.NeedsRegion, kind { run("azure", region = "evil.com/x?") })
        val (mt, m) = run("speechmatics", 201 to "{\"id\":\"j1\",\"status\":\"done\",\"txt\":\"from the job\\n\"}")
        assertEquals("from the job", mt)
        assertEquals("https://eu1.asr.api.speechmatics.com/v2/jobs?format=txt&wait=25", m.seen[0].url)
        assertTrue(m.body(0).contains("name=\"data_file\"") && m.body(0).contains("\"language\":\"auto\",\"operating_point\":\"enhanced\""))
        assertEquals(CloudErrorKind.Bad, kind { run("speechmatics", 201 to "{\"id\":\"j1\",\"status\":\"created\"}") })
    }

    @Test fun rawBodyDeepgram() {
        val (t, f) = run("deepgram", 200 to sample("deepgram"), lang = "en")
        assertEquals("send it to Ana", t)
        val r = f.seen[0]
        assertEquals("https://api.deepgram.com/v1/listen?model=nova-3&smart_format=true&language=en", r.url)
        assertEquals("Token SECRET", r.headers["Authorization"]); assertEquals("audio/wav", r.contentType); assertEquals(44 + 32_000, r.body.size)
        assertTrue(run("deepgram", 200 to sample("deepgram")).second.seen[0].url.endsWith("&language=multi"))
    }

    @Test fun jsonAudioGeminiAndOpenRouter() {
        val (t, f) = run("gemini", 200 to sample("gemini"), lang = "en", model = "gemini-3.8-flash")
        assertEquals("Call me at five.", t)
        assertEquals("https://generativelanguage.googleapis.com/v1beta/models/gemini-3.8-flash:generateContent", f.seen[0].url)
        assertEquals("SECRET", f.seen[0].headers["x-goog-api-key"])
        val body = Json.parse(f.body(0))
        with(Json) {
            assertEquals(0.0, body.path("generationConfig", "temperature"))
            assertEquals("audio/wav", body.path("contents", 0, "parts", 1, "inlineData", "mimeType"))
            assertTrue((body.path("contents", 0, "parts", 1, "inlineData", "data") as String).startsWith("UklGR"))
            assertTrue((body.path("contents", 0, "parts", 0, "text") as String).contains("verbatim"))
        }
        val (o, of) = run("openrouter", 200 to sample("openrouter"))
        assertEquals("see you soon", o)
        assertEquals("https://openrouter.ai/api/v1/chat/completions", of.seen[0].url); assertEquals("Bearer SECRET", of.seen[0].headers["Authorization"])
        with(Json) { assertEquals("wav", Json.parse(of.body(0)).path("messages", 0, "content", 1, "input_audio", "format")) }
    }

    @Test fun tidyStripsWrappersOnly() {
        assertEquals("Hello.", CloudApi.tidy("  \"Hello.\" "))
        assertEquals("hi there", CloudApi.tidy("Transcription: hi there"))
        assertEquals("Ok.", CloudApi.tidy("Sure, here's the transcript:\n“Ok.”"))
        assertEquals("abc", CloudApi.tidy("```text\nabc\n```"))
        assertEquals("Note: buy milk", CloudApi.tidy("Note: buy milk"))
        assertEquals("He said \"no\" twice", CloudApi.tidy("He said \"no\" twice"))
    }

    @Test fun uploadPollAssemblyAi() {
        val (t, f) = run("assemblyai", 200 to "{\"upload_url\":\"https://cdn.example/u1\"}", 200 to "{\"id\":\"t1\",\"status\":\"queued\"}",
            200 to "{\"id\":\"t1\",\"status\":\"processing\",\"text\":null}", 200 to sample("assemblyai-completed"), lang = "en")
        assertEquals("done text", t)
        assertEquals("https://api.assemblyai.com/v2/upload", f.seen[0].url); assertEquals("SECRET", f.seen[0].headers["authorization"])
        assertEquals("application/octet-stream", f.seen[0].contentType)
        assertEquals("{\"audio_url\":\"https://cdn.example/u1\",\"speech_models\":[\"${p("assemblyai").defaultModel}\",\"universal-2\"],\"language_code\":\"en\"}", f.body(1))
        assertEquals(listOf("GET", "GET"), f.seen.drop(2).map { it.method })
        assertEquals("https://api.assemblyai.com/v2/transcript/t1", f.seen[3].url)
        assertEquals(CloudErrorKind.BadAudio, kind { run("assemblyai", 200 to "{\"upload_url\":\"u\"}", 200 to "{\"id\":\"t1\"}", 200 to "{\"status\":\"error\"}") })
    }

    @Test fun uploadPollSonioxCleansUpAndCancels() {
        val (t, f) = run("soniox", 201 to "{\"id\":\"f-1\"}", 201 to "{\"id\":\"tr-1\",\"status\":\"queued\"}", 200 to "{\"status\":\"completed\"}",
            200 to sample("soniox-transcript"), 204 to "", 204 to "", lang = "fr")
        assertEquals("soniox text", t)
        assertTrue(f.body(0).contains("name=\"file\"; filename="))
        assertEquals("{\"file_id\":\"f-1\",\"model\":\"${p("soniox").defaultModel}\",\"language_hints\":[\"fr\"]}", f.body(1))
        assertEquals("https://api.soniox.com/v1/transcriptions/tr-1/transcript", f.seen[3].url)
        assertEquals(listOf("DELETE https://api.soniox.com/v1/transcriptions/tr-1", "DELETE https://api.soniox.com/v1/files/f-1"), f.seen.drop(4).map { "${it.method} ${it.url}" })
        // cancelled while polling: stops, still deletes what it uploaded
        var polls = 0
        val c = Fake(201 to "{\"id\":\"f-1\"}", 201 to "{\"id\":\"tr-1\"}", 200 to "{\"status\":\"processing\"}", 204 to "", 204 to "")
        assertEquals(CloudErrorKind.Cancelled, kind { CloudApi.transcribe(p("soniox"), "SECRET", "", "", pcm, null, c.send, cancelled = { c.seen.size >= 3 }, sleep = { polls++ }) })
        assertEquals(listOf("DELETE", "DELETE"), c.seen.drop(3).map { it.method })
    }

    @Test fun errorMappingAndRetries() {
        assertEquals(CloudErrorKind.KeyRejected, CloudApi.classify(401)); assertEquals(CloudErrorKind.KeyRejected, CloudApi.classify(403))
        assertEquals(CloudErrorKind.NoCredit, CloudApi.classify(402)); assertEquals(CloudErrorKind.RateLimited, CloudApi.classify(429, "{\"error\":{\"code\":\"rate_limit_exceeded\"}}"))
        assertEquals(CloudErrorKind.NoCredit, CloudApi.classify(429, "{\"error\":{\"type\":\"insufficient_quota\"}}"))
        assertEquals(CloudErrorKind.KeyRejected, CloudApi.classify(400, "{\"error\":{\"details\":[{\"reason\":\"API_KEY_INVALID\"}]}}"))
        assertEquals(CloudErrorKind.NoCredit, CloudApi.classify(401, "{\"detail\":{\"status\":\"quota_exceeded\"}}"))
        assertEquals(CloudErrorKind.TooLong, CloudApi.classify(413)); assertEquals(CloudErrorKind.Server, CloudApi.classify(503)); assertEquals(CloudErrorKind.Server, CloudApi.classify(498))
        assertEquals(CloudErrorKind.BadAudio, CloudApi.classify(422))
        assertEquals(3000L, CloudApi.retryAfterMs(CloudErrorKind.RateLimited, "3")); assertNull(CloudApi.retryAfterMs(CloudErrorKind.RateLimited, "60"))
        assertEquals(1499L, CloudApi.retryAfterMs(CloudErrorKind.Server, null) { 999 }); assertNull(CloudApi.retryAfterMs(CloudErrorKind.KeyRejected, "1"))
        assertEquals(150_000L, CloudApi.timeoutFor(60 * 16_000))
        // never retried: a rejected key; retried once: rate limit / server error / dropped connection
        for ((st, want) in listOf(401 to CloudErrorKind.KeyRejected, 402 to CloudErrorKind.NoCredit, 400 to CloudErrorKind.BadAudio)) {
            val f = Fake(st to "{}"); assertEquals(want, kind { CloudApi.transcribe(p("groq"), "k", "", "", pcm, null, f.send, sleep = {}) }); assertEquals(1, f.seen.size)
        }
        for ((st, want) in listOf(429 to CloudErrorKind.RateLimited, 503 to CloudErrorKind.Server)) {
            val f = Fake(st to "{}", st to "{}"); assertEquals(want, kind { CloudApi.transcribe(p("groq"), "k", "", "", pcm, null, f.send, sleep = {}) }); assertEquals(2, f.seen.size)
        }
        var waited = 0L; var calls = 0
        val flaky: (HttpReq) -> HttpResp = { r -> assertTrue(r.timeoutMs >= 30_000); if (calls++ == 0) HttpResp(429, "{}", "2") else HttpResp(200, "{\"text\":\"ok\"}") }
        assertEquals("ok", CloudApi.transcribe(p("groq"), "k", "", "", pcm, null, flaky, sleep = { waited += it })); assertEquals(2000L, waited)
        calls = 0
        val reset: (HttpReq) -> HttpResp = { if (calls++ == 0) throw java.net.SocketException("reset") else HttpResp(200, "{\"text\":\"ok\"}") }
        assertEquals("ok", CloudApi.transcribe(p("groq"), "k", "", "", pcm, null, reset, sleep = {}))
        calls = 0
        val slow: (HttpReq) -> HttpResp = { calls++; throw java.net.SocketTimeoutException() }
        assertEquals(CloudErrorKind.Offline, kind { CloudApi.transcribe(p("groq"), "k", "", "", pcm, null, slow, sleep = {}) }); assertEquals(1, calls)
        assertEquals(CloudErrorKind.Cancelled, kind { CloudApi.transcribe(p("groq"), "k", "", "", pcm, null, Fake().send, cancelled = { true }) })
        assertEquals(CloudErrorKind.KeyRejected, kind { run("groq", 401 to "{}") })
        assertEquals(CloudErrorKind.Bad, kind { run("groq", 200 to "<html>") })
        assertEquals(CloudErrorKind.NoText, kind { run("gemini", 200 to "{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"I'm sorry, I cannot transcribe this audio.\"}]}}]}") })
        assertTrue(CloudApi.keyWorks(null) && CloudApi.keyWorks(CloudException(CloudErrorKind.BadAudio)) && !CloudApi.keyWorks(CloudException(CloudErrorKind.KeyRejected)))
    }

    @Test fun sharedRules() {
        val t = "{region}.api.cognitive.microsoft.com"
        assertEquals("westeurope.api.cognitive.microsoft.com", CloudApi.azureHost(" WestEurope ", t))
        assertEquals("my-res.cognitiveservices.azure.com", CloudApi.azureHost("https://my-res.cognitiveservices.azure.com/", t))
        for (bad in listOf("", "evil.com", "x.cognitiveservices.azure.com.evil.com", "a/b", "me.cognitiveservices.azure.com:444", "-x")) assertNull(bad, CloudApi.azureHost(bad, t))
        assertEquals("", CloudApi.transcriptOnly("I'm sorry, but I can't transcribe this audio."))
        assertEquals("", CloudApi.transcriptOnly("[silence]"))
        assertEquals("", CloudApi.transcriptOnly("There is no speech in the audio."))
        assertEquals("I'm sorry I can't make it tonight.", CloudApi.transcriptOnly("I'm sorry I can't make it tonight."))
        assertEquals("Call me.", CloudApi.transcriptOnly("Here is the transcript: \"Call me.\""))
        // 50 s with a quiet gap at 22 s, 29 s cap -> cut inside the gap
        val pcm = ShortArray(50 * 16_000) { 9830 }
        for (i in 22 * 16_000 until 22 * 16_000 + 3_200) pcm[i] = 0
        val parts = CloudApi.split(pcm, 29 * 16_000)
        assertEquals(2, parts.size); assertTrue(parts[0].last + 1 in 22 * 16_000 until 22 * 16_000 + 3_200); assertEquals(pcm.size - 1, parts[1].last)
        assertEquals(4, CloudApi.split(ShortArray(100 * 16_000) { 3000 }, 29 * 16_000).size)
        assertEquals(listOf(0 until 16_000), CloudApi.split(ShortArray(16_000), 29 * 16_000))
    }

    /** providers/tests/contract.json: the docs' example bodies and the request facts, shared with desktop and Chrome. */
    @Suppress("UNCHECKED_CAST")
    @Test fun contractCasesFromTheDocs() {
        val doc = Json.parse(javaClass.getResource("/contract.json")!!.readText()) as Map<String, Any?>
        val cases = doc["cases"] as List<Map<String, Any?>>
        assertEquals(CloudProviders.all.size, cases.size)
        for (c in cases) {
            val id = c["provider"] as String
            val replies = (c["replies"] as List<List<Any?>>).map { (it[0] as Double).toInt() to ((it[1] as? String) ?: Json.write(it[1])) }
            val f = Fake(*replies.toTypedArray())
            val got = CloudApi.transcribe(p(id), "SECRET", "", c["region"] as String? ?: "", ShortArray(16_000) { 3000 }, c["lang"] as String?, f.send, sleep = {})
            assertEquals(id, c["text"], got)
            val want = c["requests"] as List<Map<String, Any?>>
            assertEquals("$id: request count", want.size, f.seen.size)
            for ((i, w) in want.withIndex()) {
                val r = f.seen[i]; val u = java.net.URI(r.url)
                assertEquals(id, w["method"], r.method); assertEquals(id, w["path"], u.rawPath)
                val (hk, hv) = w["header"] as List<String>
                assertEquals("$id: $hk", hv.replace("KEY", "SECRET"), r.headers.entries.firstOrNull { it.key.equals(hk, true) }?.value)
                val q = u.rawQuery.orEmpty().split('&').toSet()
                (w["query"] as Map<String, String>?)?.forEach { (k, v) -> assertTrue("$id: query $k", "$k=${java.net.URLEncoder.encode(v, "UTF-8")}" in q) }
                val body = String(r.body, Charsets.UTF_8)
                (w["form"] as Map<String, String?>?)?.forEach { (k, v) -> assertTrue("$id: form $k", if (v == null) "name=\"$k\"" in body else "name=\"$k\"\r\n\r\n$v\r\n" in body) }
                (w["json"] as Map<String, Any?>?)?.forEach { (k, v) -> assertEquals("$id: json $k", Json.write(v), Json.write((Json.parse(body) as Map<String, Any?>)[k])) }
                (w["raw"] as String?)?.let { assertEquals(id, it, r.contentType); assertEquals("RIFF", String(r.body, 0, 4)) }
            }
        }
    }
}
