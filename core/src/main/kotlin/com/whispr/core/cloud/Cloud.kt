package com.whispr.core.cloud

import com.whispr.core.cloud.Json.path
import java.io.ByteArrayOutputStream
import java.util.Base64

/**
 * Bring-your-own-key speech-to-text. The providers and their request shapes come from the shared registry
 * `providers/cloud-stt.json` (a core resource through Gradle `sourceSets`; the desktop app and Chrome read the
 * same file). This file implements the registry's five request styles, with the network injected.
 */
data class CloudProvider(
    val id: String,
    /** The model's name (model first: "Nova-3"); [maker] is the subtitle. */
    val name: String,
    val maker: String,
    val models: List<String>,
    val defaultModel: String,
    val priceNote: String,
    val speedNote: String,
    /** Where the audio goes (named in the consent text). Azure's contains `{region}`: use [hostFor]. */
    val host: String,
    val needsRegion: Boolean,
    /** Upload-then-poll providers: about 1-3 s slower. */
    val slow: Boolean,
    /** At least one request fact was not confirmed by the provider's docs (see [notes]). */
    val experimental: Boolean,
    val notes: String,
    val docs: String,
    val maxSeconds: Int,
    internal val spec: Map<String, Any?>,
) {
    /** [region]: an Azure region ("westeurope") or a full resource host. */
    /** [region]: an Azure region ("westeurope") or the resource's host or endpoint; an unusable value shows as typed. */
    fun hostFor(region: String): String = if (!needsRegion) host else CloudApi.azureHost(region, host) ?: host.replace("{region}", region.trim())

    @Suppress("UNCHECKED_CAST") private val live: Map<String, Any?>? get() = spec["live"] as Map<String, Any?>?
    /** Words can stream in while speaking ([CloudLive]); the UI shows a "Live" toggle only then. */
    val liveSupported: Boolean get() = live != null
    @Suppress("UNCHECKED_CAST") val liveModels: List<String> get() = live?.get("models") as List<String>? ?: emptyList()
    val liveDefaultModel: String get() = live?.get("default_model") as String? ?: ""
    /** The live model for the chosen (batch) model: its mapped counterpart, the same id when it is also a live model, else the live default. Same as desktop/Chrome. */
    @Suppress("UNCHECKED_CAST")
    fun liveModelFor(model: String): String = (live?.get("map") as Map<String, String>?)?.get(model) ?: model.takeIf { it in liveModels } ?: liveDefaultModel

    @Deprecated("Model-first naming: use name and maker") val label: String get() = name

    override fun toString() = "CloudProvider($id)"

    companion object {
        fun byId(id: String): CloudProvider? = CloudProviders.byId(id)
        @Deprecated("Use CloudProviders.all") val entries: List<CloudProvider> get() = CloudProviders.all
        @Deprecated("Use CloudProviders.byId(\"openai\")") val OpenAI: CloudProvider get() = CloudProviders.byId("openai")!!
    }
}

/** A provider left out on purpose, with the reason the UI shows. */
data class ExcludedProvider(val id: String, val name: String, val reason: String)

object CloudProviders {
    @Suppress("UNCHECKED_CAST")
    private val registry: Map<String, Any?> by lazy {
        val text = CloudProviders::class.java.getResource("/cloud-stt.json")?.readText() ?: error("cloud-stt.json missing from resources")
        Json.parse(text) as? Map<String, Any?> ?: error("cloud-stt.json is not valid JSON")
    }

    @Suppress("UNCHECKED_CAST")
    val all: List<CloudProvider> by lazy {
        (registry["providers"] as List<Map<String, Any?>>).map { p ->
            fun s(k: String) = p[k] as? String ?: ""
            CloudProvider(
                id = s("id"), name = s("name"), maker = s("maker"), models = (p["models"] as List<String>), defaultModel = s("default_model"),
                priceNote = s("price_note"), speedNote = s("speed_note"), host = s("host"), needsRegion = p["needs_region"] == true,
                slow = p["slow"] == true, experimental = p["experimental"] == true, notes = s("notes"), docs = s("docs"),
                maxSeconds = (p["max_seconds"] as Double).toInt(), spec = p,
            )
        }
    }

    @Suppress("UNCHECKED_CAST")
    val excluded: List<ExcludedProvider> by lazy {
        (registry["excluded"] as List<Map<String, String>>).map { ExcludedProvider(it.getValue("id"), it.getValue("name"), it.getValue("reason")) }
    }

    internal val prompt: String get() = registry["prompt"] as String
    @Suppress("UNCHECKED_CAST")
    internal val locales: Map<String, String> get() = registry["locales"] as Map<String, String>
    @Suppress("UNCHECKED_CAST")
    internal val languageNames: Map<String, String> get() = registry["language_names"] as Map<String, String>

    fun byId(id: String): CloudProvider? = all.firstOrNull { it.id == id }
}

/**
 * One HTTP request. [headers] holds the key: never log this object. [timeoutMs] is the whole exchange's deadline
 * (sized to the audio); [cancelled] lets the sender cut an in-flight upload when the user stops or cancels.
 */
class HttpReq(
    val method: String, val url: String, val headers: Map<String, String>, val contentType: String?, val body: ByteArray,
    val timeoutMs: Long = 30_000, val cancelled: () -> Boolean = { false },
) {
    override fun toString() = "HttpReq($method <redacted>)"
}

/** [retryAfter]: the Retry-After header, if any. */
class HttpResp(val status: Int, val body: String, val retryAfter: String? = null)

/** The same texts as the desktop app (`CloudError`) and Chrome (`CLOUD_ERROR_TEXT`). */
enum class CloudErrorKind(val message: String) {
    KeyRejected("The provider rejected this API key."),
    NoCredit("This account is out of credit or quota."),
    RateLimited("Rate limited by the provider; try again shortly."),
    TooLong("This recording is too long for the provider."),
    BadAudio("The provider could not process the audio."),
    Server("The provider is having problems."),
    Offline("Could not reach the provider (offline or timed out)."),
    Bad("The provider sent a reply Voicio could not read."),
    NoText("The provider returned no transcript."),
    Cancelled("Cancelled."),
    NeedsRegion("Enter the region (for example westeurope) or the resource host of your Azure Speech resource."),
}

/** [detail]: what the provider answered (status, body or close reason) for diagnostics and the probe; never holds a key. */
class CloudException(val kind: CloudErrorKind, val detail: String = "") : Exception(kind.message)

/**
 * The real network, with java.net only (so the JVM live test uses it too). Throws [java.net.SocketTimeoutException]
 * when the deadline passes, another IOException when the connection fails, [CloudException] Cancelled on cancel.
 */
object CloudHttp {
    fun send(req: HttpReq): HttpResp {
        val conn = java.net.URL(req.url).openConnection() as java.net.HttpURLConnection
        var why: String? = null
        val start = System.nanoTime()
        // read timeouts alone do not bound a slow upload, so a watchdog cuts the connection (deadline or cancel)
        val watchdog = java.util.Timer(true).also {
            it.schedule(object : java.util.TimerTask() {
                override fun run() {
                    val late = (System.nanoTime() - start) / 1_000_000 > req.timeoutMs
                    if (late || req.cancelled()) { why = if (late) "timeout" else "cancel"; cancel(); conn.disconnect() }
                }
            }, 100, 100)
        }
        try {
            conn.requestMethod = req.method
            conn.connectTimeout = 10_000
            conn.readTimeout = req.timeoutMs.toInt()
            req.contentType?.let { conn.setRequestProperty("Content-Type", it) }
            req.headers.forEach { (k, v) -> conn.setRequestProperty(k, v) }
            if (req.method == "POST") {
                conn.doOutput = true
                conn.setFixedLengthStreamingMode(req.body.size)
                conn.outputStream.use { it.write(req.body) }
            }
            val status = conn.responseCode
            val stream = if (status in 200..299) conn.inputStream else conn.errorStream
            return HttpResp(status, stream?.use { it.readBytes().decodeToString() }.orEmpty(), conn.getHeaderField("Retry-After"))
        } catch (e: java.io.IOException) {
            when (why) {
                "cancel" -> throw CloudException(CloudErrorKind.Cancelled)
                "timeout" -> throw java.net.SocketTimeoutException("deadline")
                else -> throw e
            }
        } finally {
            watchdog.cancel()
            conn.disconnect()
        }
    }
}

object Wav {
    /** 16 kHz mono 16-bit PCM WAV. */
    fun encode(pcm: ShortArray, sampleRate: Int = 16_000): ByteArray {
        val dataLen = pcm.size * 2
        val out = ByteArrayOutputStream(44 + dataLen)
        fun le32(v: Int) { for (s in 0..24 step 8) out.write((v shr s) and 0xFF) }
        fun le16(v: Int) { out.write(v and 0xFF); out.write((v shr 8) and 0xFF) }
        out.write("RIFF".toByteArray()); le32(36 + dataLen)
        out.write("WAVEfmt ".toByteArray()); le32(16); le16(1); le16(1)
        le32(sampleRate); le32(sampleRate * 2); le16(2); le16(16)
        out.write("data".toByteArray()); le32(dataLen)
        for (s in pcm) le16(s.toInt())
        return out.toByteArray()
    }
}

@Suppress("UNCHECKED_CAST")
object CloudApi {
    private const val BOUNDARY = "----voicioBoundary7MA4YWxkTrZu0gW"
    private const val POLL_EVERY_MS = 400L
    /** A Retry-After longer than this is not waited for: the dictation keeps the on-device text. */
    private const val MAX_RETRY_AFTER_S = 10L

    /**
     * One transcription. [send] does the HTTP ([CloudHttp.send], or a fake in tests); [cancelled] stops it between
     * requests, polls and retry waits (and [CloudHttp] cuts an in-flight request). Longer than the provider's
     * per-request cap: split at quiet points, sent in order, texts joined. Throws [CloudException]; every failure
     * means "use the on-device text".
     */
    fun transcribe(
        p: CloudProvider, key: String, model: String, region: String, pcm: ShortArray, language: String?,
        send: (HttpReq) -> HttpResp, cancelled: () -> Boolean = { false }, sleep: (Long) -> Unit = { Thread.sleep(it) },
    ): String {
        val c = Call(p, key.trim(), model.ifBlank { p.defaultModel }, region.trim(), send, cancelled, sleep)
        val lang = c.lang(language)
        val text = split(pcm, p.maxSeconds * 16_000).map { r ->
            c.timeoutMs = timeoutFor(r.last - r.first + 1)
            c.run(Wav.encode(pcm.copyOfRange(r.first, r.last + 1)), lang).trim()
        }.filter { it.isNotEmpty() }.joinToString(" ")
        if (text.isEmpty()) throw CloudException(CloudErrorKind.NoText)
        return text
    }

    /** An HTTP failure to the error the user sees. Bodies are only searched, never shown or logged. Same rules as desktop/Chrome. */
    fun classify(status: Int, body: String = ""): CloudErrorKind {
        val b = body.lowercase()
        val credit = listOf("insufficient_quota", "insufficient_credit", "credit", "balance", "payment", "billing_hard_limit", "quota_exceeded").any { it in b }
        val badKey = listOf("api_key_invalid", "api key not valid", "invalid api key", "invalid_api_key", "incorrect api key", "invalid_auth", "unauthorized", "not authori", "not_authori").any { it in b }
        return when {
            status == 402 -> CloudErrorKind.NoCredit
            status == 401 || status == 403 -> if (credit) CloudErrorKind.NoCredit else CloudErrorKind.KeyRejected
            status == 429 -> if (credit) CloudErrorKind.NoCredit else CloudErrorKind.RateLimited
            status == 413 -> CloudErrorKind.TooLong
            status == 400 && badKey -> CloudErrorKind.KeyRejected // Gemini: 400 API_KEY_INVALID
            status == 408 -> CloudErrorKind.Offline
            status == 498 || status in 500..599 -> CloudErrorKind.Server // 498: Groq flex tier at capacity
            else -> CloudErrorKind.BadAudio
        }
    }

    @Deprecated("Use classify(status, body)") fun errorFor(status: Int) = classify(status)

    /** Milliseconds to wait before the single retry, or null = do not retry. */
    fun retryAfterMs(kind: CloudErrorKind, header: String?, random: () -> Long = { kotlin.random.Random.nextLong(1000) }): Long? {
        if (kind != CloudErrorKind.RateLimited && kind != CloudErrorKind.Server && kind != CloudErrorKind.Offline) return null
        val s = header?.trim()?.toLongOrNull() ?: return 500 + random()
        return if (s <= MAX_RETRY_AFTER_S) s * 1000 else null
    }

    /** Per request: 30 s plus 2 s per second of audio (covers a slow upload of the WAV). */
    fun timeoutFor(samples: Int): Long = 30_000L + samples.toLong() * 2_000 / 16_000

    /** Azure: a region name, or a host under one of Azure's two Speech domains (scheme and "/" ignored); null otherwise. */
    fun azureHost(region: String, template: String): String? {
        val r = region.trim().lowercase().removePrefix("https://").trimEnd('/')
        fun label(s: String) = s.isNotEmpty() && s.length <= 63 && !s.startsWith('-') && !s.endsWith('-') && s.all { it in 'a'..'z' || it in '0'..'9' || it == '-' }
        if (label(r)) return template.replace("{region}", r)
        return listOf(".cognitiveservices.azure.com", ".api.cognitive.microsoft.com").firstOrNull { r.endsWith(it) && label(r.removeSuffix(it)) }?.let { r }
    }

    /** Splits audio longer than [max] samples at its quietest 100 ms in the second half of each stretch. Same as desktop/Chrome. */
    fun split(pcm: ShortArray, max: Int): List<IntRange> {
        val w = 1_600; val step = 800
        val out = ArrayList<IntRange>(); var start = 0
        while (max > w && pcm.size - start > max) {
            var best = start + max - w; var bestE = Double.MAX_VALUE
            var i = start + max / 2
            while (i <= start + max - w) {
                var e = 0.0
                for (k in i until i + w) { val x = pcm[k] / 32768.0; e += x * x }
                if (e <= bestE) { bestE = e; best = i } // ties: the latest, for fewer requests
                i += step
            }
            val cut = best + w / 2
            out += start until cut; start = cut
        }
        out += start until pcm.size
        return out
    }

    /** "Test key" outcome: a provider that accepts the key but not the silent test audio still means the key works. */
    fun keyWorks(e: CloudException?) = e == null || e.kind == CloudErrorKind.Bad || e.kind == CloudErrorKind.BadAudio || e.kind == CloudErrorKind.NoText

    /** LLM transcribers wrap text sometimes: drop a code fence, a "Here is the transcript:" preamble and quotes. Same rules as desktop/Chrome. */
    fun tidy(s: String): String {
        var t = s.trim()
        if (t.startsWith("```")) {
            t = t.removePrefix("```").substringAfter('\n', "").trimEnd().removeSuffix("```").trim()
        }
        val i = t.indexOf(':')
        if (i in 0 until 80) {
            val head = t.substring(0, i).lowercase()
            if ('\n' !in head && ("transcri" in head || head.startsWith("here is") || head.startsWith("here's"))) t = t.substring(i + 1).trim()
        }
        for ((a, b) in listOf("\"" to "\"", "“" to "”")) {
            if (t.length >= a.length + b.length && t.startsWith(a) && t.endsWith(b)) { t = t.substring(a.length, t.length - b.length).trim(); break }
        }
        return t
    }

    /** [tidy], then "" for an answer about the audio instead of a transcript ("I'm sorry, I can't transcribe this audio", "[silence]"). */
    fun transcriptOnly(s: String): String {
        val t = tidy(s); val l = t.lowercase()
        val marker = t.length < 40 && (t.startsWith('[') && t.endsWith(']') || t.startsWith('(') && t.endsWith(')') || t.startsWith('<') && t.endsWith('>'))
        val openers = listOf("i'm sorry", "i am sorry", "sorry", "i cannot", "i can't", "i can not", "i'm unable", "i am unable", "unfortunately", "as an ai", "there is no", "there's no", "no speech", "the audio", "this audio", "the recording", "this recording", "the provided audio")
        val aboutAudio = listOf("audio", "transcri", "speech", "recording").any { it in l }
        return if (marker || (aboutAudio && openers.any { l.startsWith(it) })) "" else t
    }

    private fun enc(v: String) = buildString { for (b in v.toByteArray()) { val c = b.toInt() and 0xFF; if (c.toChar().isLetterOrDigit() && c < 128 || c.toChar() in "-._~") append(c.toChar()) else append("%%%02X".format(c)) } }

    private fun multipart(fields: List<Pair<String, String>>, fileField: String, file: ByteArray): ByteArray {
        val out = ByteArrayOutputStream(file.size + 1024)
        fun w(s: String) = out.write(s.toByteArray())
        for ((k, v) in fields) w("--$BOUNDARY\r\nContent-Disposition: form-data; name=\"$k\"\r\n\r\n$v\r\n")
        w("--$BOUNDARY\r\nContent-Disposition: form-data; name=\"$fileField\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\n")
        out.write(file)
        w("\r\n--$BOUNDARY--\r\n")
        return out.toByteArray()
    }

    /** A JSON pointer ("/a/0/b") into a parsed value. */
    private fun at(v: Any?, pointer: String): String? {
        var cur = v
        for (k in pointer.trim('/').split('/')) cur = k.toIntOrNull()?.let { cur.path(it) } ?: cur.path(k)
        return cur as? String
    }

    private class Call(
        val p: CloudProvider, val key: String, val model: String, val region: String,
        val send: (HttpReq) -> HttpResp, val cancelled: () -> Boolean, val sleep: (Long) -> Unit,
    ) {
        val s = p.spec
        var timeoutMs = timeoutFor(0)
        fun str(k: String) = s[k] as? String ?: ""
        fun obj(k: String) = s[k] as? Map<String, Any?>
        val langSpec = obj("lang") ?: emptyMap()

        /** The language value to send, in the provider's format; null = leave it out. */
        fun lang(language: String?): String? {
            val v = language?.trim()?.takeIf { it.isNotEmpty() && it != "auto" && it.all { c -> c.isLetterOrDigit() && c.code < 128 || c == '-' } }
            val value = v?.let {
                val base = it.substringBefore('-').lowercase()
                when (langSpec["format"]) {
                    "locale" -> if ('-' in it) it else CloudProviders.locales[base]
                    "indic" -> if ('-' in it) it else "$base-IN"
                    "name" -> CloudProviders.languageNames[base]
                    else -> base // ISO 639-1: "en-US" -> "en"
                }
            }
            return value ?: langSpec["auto"] as? String
        }

        fun url(template: String, id: String = "", file: String = ""): String {
            var url = template.replace("{model}", model).replace("{id}", enc(id)).replace("{file}", enc(file))
            if (p.needsRegion) url = url.replace(p.host, azureHost(region, p.host) ?: throw CloudException(CloudErrorKind.NeedsRegion))
            return url
        }

        /** Waits [ms] in small steps; false = cancelled meanwhile. */
        fun pause(ms: Long): Boolean {
            var waited = 0L
            while (waited < ms) { if (cancelled()) return false; val d = minOf(20L, ms - waited); sleep(d); waited += d }
            return !cancelled()
        }

        /** One request, retried once on a rate limit, server error or dropped connection (not on a timeout or a rejected key). */
        fun call(method: String, url: String, contentType: String? = null, body: ByteArray = ByteArray(0)): Any? {
            val auth = s["auth"] as List<String>
            val headers = LinkedHashMap<String, String>()
            headers[auth[0]] = auth[1] + key
            (obj("headers") as Map<String, String>?)?.let { headers.putAll(it) }
            var retried = false
            while (true) {
                if (method != "DELETE" && cancelled()) throw CloudException(CloudErrorKind.Cancelled)
                val (kind, wait) = try {
                    val r = send(HttpReq(method, url, headers, contentType, body, timeoutMs, if (method == "DELETE") ({ false }) else cancelled))
                    if (r.status in 200..299) {
                        if (method == "DELETE" || r.body.isBlank()) return null
                        return Json.parse(r.body) ?: throw CloudException(CloudErrorKind.Bad)
                    }
                    val k = classify(r.status, r.body)
                    k to retryAfterMs(k, r.retryAfter)
                } catch (e: java.net.SocketTimeoutException) {
                    CloudErrorKind.Offline to null
                } catch (e: java.io.IOException) {
                    CloudErrorKind.Offline to retryAfterMs(CloudErrorKind.Offline, null)
                }
                if (wait == null || retried || method == "DELETE") throw CloudException(kind)
                retried = true
                if (!pause(wait)) throw CloudException(CloudErrorKind.Cancelled)
            }
        }

        fun text(v: Any?, pointer: String = str("text")) = at(v, pointer) ?: throw CloudException(CloudErrorKind.Bad)

        fun query(url: String, q: List<Pair<String, String>>) =
            q.foldIndexed(url) { i, u, (k, v) -> u + (if (i == 0 && '?' !in url) "?" else "&") + "$k=${enc(v)}" }

        fun run(wav: ByteArray, lang: String?): String = when (str("style")) {
            "multipart" -> {
                val fields = ArrayList<Pair<String, String>>()
                str("model_field").takeIf { it.isNotEmpty() }?.let { fields += it to model }
                (obj("fields") as Map<String, String>?)?.forEach { (k, v) -> fields += k to v }
                val langField = langSpec["field"] as? String ?: ""
                if (lang != null && langField.isNotEmpty()) fields += langField to lang
                obj("json_field")?.let { j ->
                    val value = if (lang == null && j["value_auto"] != null) j["value_auto"] as String else (j["value"] as String).replace("{lang}", lang ?: "")
                    fields += (j["name"] as String) to value.replace("{model}", model)
                }
                val q = (obj("query") as Map<String, String>?).orEmpty().toSortedMap().map { (k, v) -> k to v.replace("{model}", model) }
                text(call("POST", query(url(str("url")), q), "multipart/form-data; boundary=$BOUNDARY", multipart(fields, str("file_field"), wav))).trim()
            }
            "raw" -> {
                val q = (obj("query") as Map<String, String>?).orEmpty().toSortedMap().map { (k, v) -> k to v.replace("{model}", model) }.toMutableList()
                if (lang != null) q += (langSpec["field"] as String) to lang
                text(call("POST", query(url(str("url")), q), "audio/wav", wav)).trim()
            }
            "json_audio" -> {
                val data = Base64.getEncoder().encodeToString(wav)
                val prompt = CloudProviders.prompt + (lang?.let { " The speech is in the language with code $it." } ?: "")
                val body: Map<String, Any?> = if (str("body") == "gemini") mapOf(
                    "contents" to listOf(mapOf("role" to "user", "parts" to listOf(mapOf("text" to prompt), mapOf("inlineData" to mapOf("mimeType" to "audio/wav", "data" to data))))),
                    "generationConfig" to (obj("generation_config") ?: emptyMap<String, Any?>()),
                ) else mapOf(
                    "model" to model, "temperature" to 0,
                    "messages" to listOf(mapOf("role" to "user", "content" to listOf(mapOf("type" to "text", "text" to prompt), mapOf("type" to "input_audio", "input_audio" to mapOf("data" to data, "format" to "wav"))))),
                )
                transcriptOnly(text(call("POST", url(str("url")), "application/json", Json.write(body).toByteArray())))
            }
            "upload_poll" -> uploadPoll(wav, lang)
            else -> throw CloudException(CloudErrorKind.Bad)
        }

        fun uploadPoll(wav: ByteArray, lang: String?): String {
            val up = obj("upload")!!; val create = obj("create")!!; val status = obj("status")!!
            val uploaded = if (up["body"] == "raw") call("POST", url(up["url"] as String), "application/octet-stream", wav)
            else call("POST", url(up["url"] as String), "multipart/form-data; boundary=$BOUNDARY", multipart(emptyList(), "file", wav))
            val file = text(uploaded, up["id"] as String)
            var job = ""
            try {
                val models = listOfNotNull(model, (create["model_fallback"] as String?)?.takeIf { it != model })
                val body = linkedMapOf<String, Any?>(create["ref_field"] as String to file, create["model_field"] as String to if (create["model_list"] == true) models else model)
                if (lang != null) body[langSpec["field"] as String] = if (langSpec["list"] == true) listOf(lang) else lang
                job = text(call("POST", url(create["url"] as String), "application/json", Json.write(body).toByteArray()), create["id"] as String)
                val limit = timeoutMs
                val started = System.nanoTime()
                while (true) {
                    val v = call("GET", url(status["url"] as String, job, file))
                    val state = text(v, status["field"] as String)
                    if (state == status["done"]) {
                        val result = str("result")
                        return text(if (result.isEmpty()) v else call("GET", url(result, job, file))).trim()
                    }
                    if (state == status["failed"]) throw CloudException(CloudErrorKind.BadAudio)
                    if ((System.nanoTime() - started) / 1_000_000 > limit) throw CloudException(CloudErrorKind.Offline)
                    if (!pause(POLL_EVERY_MS)) throw CloudException(CloudErrorKind.Cancelled)
                }
            } finally {
                // Best effort: the provider keeps nothing once the text is back (or the job failed).
                timeoutMs = timeoutFor(0)
                (s["cleanup"] as List<String>?)?.filter { job.isNotEmpty() || "{id}" !in it }?.forEach { runCatching { call("DELETE", url(it, job, file)) } }
            }
        }
    }
}

/** Just enough JSON to read a transcript and write a request: objects, arrays, strings, numbers, literals. */
internal object Json {
    fun parse(s: String): Any? = try {
        P(s).run { val v = value(); ws(); if (i == s.length) v else null }
    } catch (e: RuntimeException) { null }

    fun write(v: Any?): String = when (v) {
        null -> "null"
        is String -> buildString {
            append('"')
            for (c in v) when {
                c == '"' -> append("\\\""); c == '\\' -> append("\\\\"); c == '\n' -> append("\\n"); c == '\r' -> append("\\r"); c == '\t' -> append("\\t")
                c < ' ' -> append("\\u%04x".format(c.code))
                else -> append(c)
            }
            append('"')
        }
        is Map<*, *> -> v.entries.joinToString(",", "{", "}") { (k, x) -> write(k.toString()) + ":" + write(x) }
        is List<*> -> v.joinToString(",", "[", "]") { write(it) }
        is Number, is Boolean -> v.toString()
        else -> write(v.toString())
    }

    fun Any?.path(vararg keys: Any): Any? {
        var cur = this
        for (k in keys) cur = when {
            k is String && cur is Map<*, *> -> cur[k]
            k is Int && cur is List<*> -> cur.getOrNull(k)
            else -> return null
        }
        return cur
    }

    private class P(val s: String) {
        var i = 0
        fun ws() { while (i < s.length && s[i].isWhitespace()) i++ }
        fun value(): Any? {
            ws()
            return when (val c = s[i]) {
                '{' -> { i++; val m = LinkedHashMap<String, Any?>(); ws(); if (s[i] == '}') { i++; m } else { while (true) { ws(); val k = str(); ws(); expect(':'); m[k] = value(); ws(); if (s[i++] == '}') break }; m } }
                '[' -> { i++; val l = ArrayList<Any?>(); ws(); if (s[i] == ']') { i++; l } else { while (true) { l.add(value()); ws(); if (s[i++] == ']') break }; l } }
                '"' -> str()
                else -> { val st = i; while (i < s.length && s[i] !in ",}] \n\r\t") i++; val t = s.substring(st, i); when (t) { "true" -> true; "false" -> false; "null" -> null; else -> t.toDoubleOrNull() ?: error("bad token $c") } }
            }
        }
        fun expect(c: Char) { if (s[i++] != c) error("expected $c") }
        fun str(): String {
            expect('"')
            val sb = StringBuilder()
            while (true) {
                val c = s[i++]
                when (c) {
                    '"' -> return sb.toString()
                    '\\' -> when (val e = s[i++]) {
                        'n' -> sb.append('\n'); 't' -> sb.append('\t'); 'r' -> sb.append('\r'); 'b' -> sb.append('\b'); 'f' -> sb.append('\u000c')
                        'u' -> { sb.append(s.substring(i, i + 4).toInt(16).toChar()); i += 4 }
                        else -> sb.append(e)
                    }
                    else -> sb.append(c)
                }
            }
        }
    }
}
