package com.whispr.core.cloud

import com.whispr.core.cloud.Json.path
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import okio.ByteString.Companion.toByteString
import java.util.Base64

/** A JSON pointer ("/a/0/b") into a parsed value; null when absent. */
internal fun pointer(v: Any?, p: String): Any? {
    if (p.isEmpty()) return v
    var cur = v
    for (k in p.trim('/').split('/')) cur = k.toIntOrNull()?.let { cur.path(it) } ?: cur.path(k)
    return cur
}

/**
 * What one streamed dictation has heard so far: the registry's `live.messages` rules (partial, delta, final,
 * tokens, result, error, done). The same rules as the desktop app (`LiveState` in cloud.rs) and Chrome.
 */
@Suppress("UNCHECKED_CAST")
internal class LiveState(private val live: Map<String, Any?>) {
    var finals = StringBuilder()
    var partial = ""
    var ready = live["ready"] == null
    var endSent = false
    var done = false
    var error: String? = null
    private val join = live["join"] as? String ?: ""

    private fun matches(v: Any?, whenMap: Map<String, Any?>) = whenMap.all { (p, want) ->
        val got = pointer(v, p)
        got != null && (want == "*" || got == want)
    }

    private fun append(t: String) {
        if (finals.isNotEmpty() && t.isNotEmpty()) finals.append(join)
        finals.append(t)
    }

    /** One server message; true = the text shown changed. */
    fun on(msg: String): Boolean {
        val v = Json.parse(msg) ?: return false
        val before = text()
        (live["ready"] as Map<String, Any?>?)?.let { if (matches(v, it)) ready = true }
        val rules = live["messages"] as List<Map<String, Any?>>
        // a "done" rule also applies when another rule took the message (Soniox: {"tokens":[...],"finished":true})
        if (rules.any { it["do"] == "done" && matches(v, it["match"] as Map<String, Any?>) }) done = true
        val rule = rules.firstOrNull { matches(v, it["match"] as Map<String, Any?>) } ?: return text() != before
        val path = rule["text"] as? String ?: ""
        fun text() = pointer(v, path).let { it as? String ?: it?.let(Json::write) ?: "" }
        when (rule["do"]) {
            "partial" -> partial = text()
            "delta" -> partial += text()
            "final" -> { append(text()); partial = ""; if (endSent && live["end_done"] == "final") done = true }
            "tokens" -> {
                val p = StringBuilder()
                for (t in pointer(v, path) as? List<Map<String, Any?>> ?: emptyList()) {
                    val s = t["text"] as? String ?: ""
                    if (s.startsWith('<') && s.endsWith('>')) continue // <fin>, <end> markers
                    if (t["is_final"] == true) finals.append(s) else p.append(s)
                }
                partial = p.toString()
            }
            "result" -> { finals = StringBuilder(text()); partial = ""; done = true }
            "done" -> done = true
            "error" -> error = if (path.isEmpty()) msg else text()
        }
        return text() != before
    }

    fun text(): String {
        val sep = if (finals.isEmpty() || partial.isEmpty()) "" else join
        return "$finals$sep$partial".split(Regex("\\s+")).filter { it.isNotEmpty() }.joinToString(" ")
    }
}

/**
 * A live (streaming) cloud dictation: [feed] 16 kHz audio while the user speaks, [onText] gets the words so far,
 * [finish] sends the end message and returns the final text (throws [CloudException]). The key goes only in the
 * WebSocket upgrade header; nothing here logs it. [base] (tests) replaces the URL's scheme and host.
 */
@Suppress("UNCHECKED_CAST")
class CloudLive(
    val provider: CloudProvider, key: String, model: String, region: String, language: String?,
    private val onText: (String) -> Unit, base: String? = null, client: OkHttpClient = shared,
) {
    private val live = provider.spec["live"] as? Map<String, Any?> ?: throw CloudException(CloudErrorKind.Bad)
    private val st = LiveState(live)
    private val lock = Object()
    private val rate = (live["rate"] as? Double)?.toInt() ?: 16_000
    private val frame = ShortArray(rate / 10) // 100 ms
    private var n = 0
    private var seq = 0
    private val held = ArrayDeque<FloatArray>() // before `ready`, at most ~2 s
    private var heldSamples = 0
    @Volatile private var failure: CloudException? = null
    @Volatile private var closed = false
    @Volatile private var lastMessage = System.nanoTime()
    private val ws: WebSocket

    /** The stream already failed (the caller then shows the on-device words again). */
    val failed: Boolean get() = failure != null

    init {
        val k = key.trim()
        if (k.isEmpty()) throw CloudException(CloudErrorKind.KeyRejected)
        val m = provider.liveModelFor(model)
        val langSpec = live["lang"] as Map<String, Any?>?
        val chosen = language?.trim()?.takeIf { it.isNotEmpty() && it != "auto" && it.all { c -> c.isLetterOrDigit() && c.code < 128 || c == '-' } }
        val lang = langSpec?.let { chosen?.substringBefore('-')?.lowercase() ?: it["auto"] as String? }
        var url = (live["url"] as String).replace("{model}", m)
        val q = (live["query"] as Map<String, String>?).orEmpty().toSortedMap().map { (qk, v) -> qk to v.replace("{model}", m) }.toMutableList()
        val field = langSpec?.get("field") as String? ?: ""
        if (lang != null && field.isNotEmpty()) q += field to lang
        q.forEachIndexed { i, (qk, v) -> url += (if (i == 0 && '?' !in url) "?" else "&") + "$qk=" + java.net.URLEncoder.encode(v, "UTF-8") }
        if (provider.needsRegion) url = url.replace(provider.host, CloudApi.azureHost(region, provider.host) ?: throw CloudException(CloudErrorKind.NeedsRegion))
        if (base != null) url = base + "/" + url.split('/', limit = 4).getOrElse(3) { "" }
        val start = (if (lang == null && live["start_auto"] != null) live["start_auto"] as String else (live["start"] as String? ?: "").replace("{lang}", lang ?: "")).replace("{model}", m)
        val auth = live["auth"] as List<String>? ?: provider.spec["auth"] as List<String>
        val req = Request.Builder().url(url).header(auth[0], auth[1] + k).build()
        ws = client.newWebSocket(req, object : WebSocketListener() {
            override fun onMessage(webSocket: WebSocket, text: String) {
                lastMessage = System.nanoTime()
                synchronized(lock) {
                    val wasReady = st.ready
                    if (st.on(text)) onText(st.text())
                    st.error?.let { e -> fail(CloudException(classifyLive(e), e.replace(k, "<key>"))); webSocket.cancel(); return }
                    if (!wasReady && st.ready) while (held.isNotEmpty()) push(held.removeFirst())
                }
            }
            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                val body = runCatching { response?.body?.string() }.getOrNull().orEmpty().replace(k, "<key>")
                fail(if (response != null) CloudException(CloudApi.classify(response.code, body), "HTTP ${response.code} $body") else CloudException(CloudErrorKind.Offline, t.toString().replace(k, "<key>")))
                closed = true
            }
            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                if (!st.endSent) fail(CloudException(classifyLive(reason), "closed $code $reason"))
                closed = true
                webSocket.close(1000, null)
            }
            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) { closed = true }
        })
        if (start.isNotEmpty()) ws.send(start)
    }

    private fun fail(e: CloudException) { if (failure == null && !st.done) failure = e }

    /** 16 kHz mono audio from the microphone; frames go out as 100 ms pieces. Never blocks. */
    fun feed(pcm16k: FloatArray) = synchronized(lock) {
        if (failure != null || st.endSent) return@synchronized
        if (!st.ready) {
            held.addLast(pcm16k); heldSamples += pcm16k.size
            while (heldSamples > 32_000) heldSamples -= held.removeFirst().size // a server that never says "ready" must not grow memory
        } else push(pcm16k)
    }

    private fun push(pcm16k: FloatArray) {
        for (s in resample(pcm16k)) {
            frame[n++] = (s.coerceIn(-1f, 1f) * 32767).toInt().toShort()
            if (n == frame.size) sendFrame()
        }
    }

    private fun sendFrame() {
        if (n == 0) return
        val bytes = ByteArray(n * 2)
        for (i in 0 until n) { bytes[2 * i] = frame[i].toInt().toByte(); bytes[2 * i + 1] = (frame[i].toInt() shr 8).toByte() }
        n = 0
        val audio = live["audio"] as String? ?: ""
        if (audio.isEmpty()) ws.send(bytes.toByteString()) else ws.send(audio.replace("{audio}", Base64.getEncoder().encodeToString(bytes)))
        seq++
    }

    private fun resample(pcm: FloatArray): FloatArray {
        if (rate == 16_000 || pcm.isEmpty()) return pcm
        val out = FloatArray(pcm.size * rate / 16_000)
        for (i in out.indices) {
            val x = i * 16_000.0 / rate; val a = x.toInt(); val f = (x - a).toFloat()
            out[i] = pcm[a] * (1 - f) + pcm[minOf(a + 1, pcm.size - 1)] * f
        }
        return out
    }

    /**
     * Blocking: sends the rest of the audio and the end message, then waits until the provider says it is done,
     * goes quiet for 2 s, or 10 s pass. Returns the text; throws [CloudException] on any failure.
     */
    fun finish(cancelled: () -> Boolean = { false }): String {
        val readyBy = System.nanoTime() + 10_000_000_000
        while (!st.ready && failure == null && !closed && System.nanoTime() < readyBy) { // audio is held until then
            if (cancelled()) { ws.cancel(); throw CloudException(CloudErrorKind.Cancelled) }
            Thread.sleep(20)
        }
        val endAt = System.nanoTime()
        synchronized(lock) {
            if (failure == null && st.ready) {
                sendFrame()
                when (val end = live["end"] as String?) { null -> ws.close(1000, null); else -> ws.send(end.replace("{seq}", seq.toString())) }
            }
            st.endSent = true
        }
        while (true) {
            if (cancelled()) { ws.cancel(); throw CloudException(CloudErrorKind.Cancelled) }
            failure?.let { throw it }
            val sinceEnd = (System.nanoTime() - endAt) / 1_000_000
            val quiet = (System.nanoTime() - lastMessage) / 1_000_000
            if (st.done || closed || sinceEnd > 10_000 || (sinceEnd > 2_000 && quiet > 2_000)) break
            Thread.sleep(20)
        }
        ws.close(1000, null)
        failure?.let { throw it }
        return synchronized(lock) { st.text() }.ifEmpty { throw CloudException(CloudErrorKind.NoText) }
    }

    fun cancel() { ws.cancel() }

    override fun toString() = "CloudLive(${provider.id})"

    companion object {
        val shared: OkHttpClient by lazy { OkHttpClient() }

        /** An error message from a live stream to the error the user sees. Same as desktop/Chrome. */
        fun classifyLive(msg: String): CloudErrorKind {
            val l = msg.lowercase()
            return when {
                CloudApi.classify(400, msg) == CloudErrorKind.KeyRejected -> CloudErrorKind.KeyRejected
                listOf("quota", "credit", "balance", "insufficient", "payment").any { it in l } -> CloudErrorKind.NoCredit
                "rate" in l -> CloudErrorKind.RateLimited
                else -> CloudErrorKind.Bad
            }
        }
    }
}
