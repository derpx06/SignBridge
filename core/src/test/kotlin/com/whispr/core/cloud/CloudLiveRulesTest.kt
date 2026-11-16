package com.whispr.core.cloud

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.DataInputStream
import java.io.OutputStream
import java.net.ServerSocket
import java.security.MessageDigest
import java.util.Base64
import kotlin.concurrent.thread

/** Live (streaming) rules and transport, without network: the same cases as the desktop tests. */
@Suppress("UNCHECKED_CAST")
class CloudLiveRulesTest {
    private fun p(id: String) = CloudProviders.byId(id)!!
    private fun feed(id: String, vararg msgs: String) = LiveState(p(id).spec["live"] as Map<String, Any?>).apply { ready = true; msgs.forEach { on(it) } }

    @Test fun liveModelsMapFromTheBatchModel() {
        fun p(id: String) = CloudProviders.byId(id)!!
        org.junit.Assert.assertEquals("scribe_v3_live", p("elevenlabs").liveModelFor("scribe_v3"))
        org.junit.Assert.assertEquals("gpt-live-transcribe", p("openai").liveModelFor("gpt-transcribe"))
        org.junit.Assert.assertEquals("nova-3", p("deepgram").liveModelFor("nova-3"))
        org.junit.Assert.assertEquals("stt-rt-v6", p("soniox").liveModelFor("stt-async-v6"))
        for (pr in CloudProviders.all.filter { it.liveSupported }) for (m in pr.models + "") org.junit.Assert.assertTrue("${pr.id}: $m", pr.liveModelFor(m) in pr.liveModels)
    }

    @Test fun liveMessagesFollowEachProvidersRules() {
        assertEquals("yeah so my credit card", feed("deepgram", """{"type":"Results","is_final":true,"channel":{"alternatives":[{"transcript":"yeah so my"}]}}""", """{"type":"Results","is_final":false,"channel":{"alternatives":[{"transcript":"credit card"}]}}""").text())
        assertEquals("My number is 555-0134. thanks", feed("assemblyai", """{"type":"Turn","end_of_turn":false,"transcript":"my number is five"}""", """{"type":"Turn","end_of_turn":true,"transcript":"My number is 555-0134."}""", """{"type":"Turn","end_of_turn":false,"transcript":"thanks"}""").text())
        val so = feed("soniox", """{"tokens":[{"text":"How","is_final":true},{"text":"'re","is_final":false}]}""", """{"tokens":[{"text":"'re","is_final":true},{"text":" you","is_final":true},{"text":"<fin>","is_final":true}]}""", """{"tokens":[],"finished":true}""")
        assertEquals("How're you", so.text()); assertTrue(so.done)
        assertEquals("Hello there", feed("mistral", """{"type":"transcription.text.delta","text":"Hel"}""", """{"type":"transcription.text.delta","text":"lo there"}""").text())
        val mi = feed("mistral", """{"type":"transcription.done","text":"Hello there."}"""); assertEquals("Hello there.", mi.text()); assertTrue(mi.done)
        val oa = feed("openai", """{"type":"conversation.item.input_audio_transcription.delta","item_id":"item_003","delta":"Hello,"}""")
        assertEquals("Hello,", oa.text()); oa.endSent = true
        oa.on("""{"type":"conversation.item.input_audio_transcription.completed","item_id":"item_003","transcript":"Hello, how are you?"}""")
        assertEquals("Hello, how are you?", oa.text()); assertTrue(oa.done)
        val sm = feed("speechmatics", """{"message":"AddTranscript","metadata":{"transcript":"Hello world."}}""", """{"message":"AddPartialTranscript","metadata":{"transcript":"how"}}""", """{"message":"EndOfTranscript"}""")
        assertEquals("Hello world. how", sm.text()); assertTrue(sm.done)
        val ca = feed("cartesia", """{"type":"transcript","is_final":true,"text":"How are"}""", """{"type":"transcript","is_final":true,"text":" you doing today?"}""", """{"type":"done"}""")
        assertEquals("How are you doing today?", ca.text()); assertTrue(ca.done)
        assertEquals(CloudErrorKind.KeyRejected, CloudLive.classifyLive(feed("elevenlabs", """{"message_type":"auth_error","error":"Invalid API key"}""").error!!))
        assertEquals(CloudErrorKind.KeyRejected, CloudLive.classifyLive(feed("speechmatics", """{"message":"Error","type":"not_authorised","reason":"Not Authorized"}""").error!!))
        assertTrue(CloudProviders.all.count { it.liveSupported } >= 8)
        assertFalse(p("groq").liveSupported); assertEquals("nova-3", p("deepgram").liveDefaultModel)
    }

    /** A tiny WebSocket server (RFC 6455, enough for one test client). */
    private class FakeWs(val script: FakeWs.() -> Unit) {
        val server = ServerSocket(0)
        val base = "ws://127.0.0.1:${server.localPort}"
        var request = ""; var auth = ""
        lateinit var input: DataInputStream; lateinit var out: OutputStream
        val done = thread {
            val s = server.accept()
            input = DataInputStream(s.getInputStream()); out = s.getOutputStream()
            val lines = generateSequence { readLine().takeIf { it.isNotEmpty() } }.toList()
            request = lines[0]
            auth = lines.firstOrNull { it.startsWith("Authorization:", true) }.orEmpty()
            val key = lines.first { it.startsWith("Sec-WebSocket-Key:", true) }.substringAfter(':').trim()
            val accept = Base64.getEncoder().encodeToString(MessageDigest.getInstance("SHA-1").digest((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").toByteArray()))
            out.write("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: $accept\r\n\r\n".toByteArray()); out.flush()
            script()
            Thread.sleep(200); s.close(); server.close()
        }
        private fun readLine(): String { val b = StringBuilder(); while (true) { val c = input.read(); if (c < 0 || c == '\n'.code) return b.toString().trimEnd('\r'); b.append(c.toChar()) } }
        /** (opcode, payload) of the next client frame. */
        fun read(): Pair<Int, ByteArray> {
            val b0 = input.readUnsignedByte(); val b1 = input.readUnsignedByte()
            var len = (b1 and 0x7F).toLong()
            if (len == 126L) len = input.readUnsignedShort().toLong() else if (len == 127L) len = input.readLong()
            val mask = ByteArray(4).also { input.readFully(it) }
            val data = ByteArray(len.toInt()).also { input.readFully(it) }
            for (i in data.indices) data[i] = (data[i].toInt() xor mask[i % 4].toInt()).toByte()
            return (b0 and 0x0F) to data
        }
        fun text(t: String) { val d = t.toByteArray(); out.write(0x81); if (d.size < 126) out.write(d.size) else { out.write(126); out.write(d.size shr 8); out.write(d.size and 0xFF) }; out.write(d); out.flush() }
    }

    @Test fun liveStreamEndToEndAgainstAFakeServer() {
        var bytes = 0; var frames = 0; var end = ""
        val ws = FakeWs {
            text("""{"type":"Results","is_final":false,"channel":{"alternatives":[{"transcript":"hel"}]}}""")
            while (true) {
                val (op, d) = read()
                if (op == 2) { bytes += d.size; frames++ } else if (op == 1) { end = String(d); break }
            }
            text("""{"type":"Results","is_final":true,"channel":{"alternatives":[{"transcript":"hello world"}]}}""")
            text("""{"type":"Metadata"}""")
        }
        val shown = ArrayList<String>()
        val live = CloudLive(p("deepgram"), " SECRET ", "", "", "en", { shown += it }, ws.base)
        assertFalse("SECRET" in live.toString())
        repeat(10) { live.feed(FloatArray(1_600) { 0.1f }) }
        assertEquals("hello world", live.finish())
        ws.done.join(5_000)
        assertTrue(shown.toString(), "hel" in shown)
        assertTrue(ws.request, ws.request.startsWith("GET /v1/listen?") && "model=nova-3" in ws.request && "encoding=linear16" in ws.request && "sample_rate=16000" in ws.request && "language=en" in ws.request)
        assertFalse(ws.request.contains("SECRET")); assertEquals("Authorization: Token SECRET", ws.auth)
        assertEquals(32_000 to 10, bytes to frames); assertEquals("""{"type":"CloseStream"}""", end)
    }

    @Test fun liveWaitsForReadyAndCountsFrames() {
        var start = ""; var frames = 0; var end = ""
        val ws = FakeWs {
            start = String(read().second)
            Thread.sleep(150)
            text("""{"message":"RecognitionStarted"}""")
            while (true) { val (op, d) = read(); if (op == 2) frames++ else if (op == 1) { end = String(d); break } }
            text("""{"message":"AddTranscript","metadata":{"transcript":"Hello world."}}""")
            text("""{"message":"EndOfTranscript"}""")
        }
        val live = CloudLive(p("speechmatics"), "SECRET", "", "", null, {}, ws.base)
        live.feed(FloatArray(4_000) { 0.1f })
        assertEquals("Hello world.", live.finish())
        ws.done.join(5_000)
        assertTrue(start, "\"language\":\"en\"" in start && "\"model\":\"enhanced\"" in start)
        assertEquals(3, frames); assertEquals("""{"message":"EndOfStream","last_seq_no":3}""", end)
    }
}
