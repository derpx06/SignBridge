package com.whispr.core.cloud

import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/**
 * Real endpoints, no account: every provider gets the bench clip with an invalid key through the phone's own code
 * (CloudApi + CloudHttp for batch, CloudLive for live). PASS = the provider rejected the KEY, so the URL, method,
 * auth header and request shape were accepted. Run with VOICIO_PROBE=1 (skipped otherwise):
 *
 *   VOICIO_PROBE=1 ./gradlew --no-daemon :core:test --tests '*CloudProbeTest*'
 */
class CloudProbeTest {
    companion object {
        const val PROBE_KEY = "voicio-probe-invalid"
        private val AUTH_WORDS = listOf("api key", "api_key", "apikey", "api-key", "unauthor", "authenticat", "invalid key", "invalid_key", "credential", "token", "permission", "forbidden", "subscription key", "access denied", "not valid", "invalid_auth", "incorrect", "jwt", "not authori", "not_authori")

        /** The same verdict as `voicio cloud-check --probe`. */
        fun verdict(status: Int, body: String): Pair<Boolean, String> {
            val aboutKey = AUTH_WORDS.any { it in body.lowercase() }
            return when {
                status == 401 || status == 403 -> true to "key rejected"
                (status == 400 || status == 422) && aboutKey -> true to "key rejected (4xx with an auth error)"
                status == 404 || status == 405 -> false to "wrong path or method"
                status == 400 || status == 415 || status == 422 -> false to "request shape rejected"
                status == 0 -> false to "no HTTP answer (host, DNS, TLS or timeout)"
                status in 200..299 -> false to "accepted an invalid key?"
                else -> false to "unexpected status"
            }
        }
    }

    @Test fun everyEndpointRejectsTheKeyNotTheRequest() {
        assumeTrue("set VOICIO_PROBE=1 to probe the real endpoints", System.getenv("VOICIO_PROBE") == "1")
        val bytes = File("../extension/apps/chrome/public/bench.wav").readBytes()
        val pcm = ShortArray((bytes.size - 44) / 2) { ((bytes[44 + 2 * it].toInt() and 0xFF) or (bytes[45 + 2 * it].toInt() shl 8)).toShort() }
        val rows = ArrayList<String>(); var failed = 0
        for (p in CloudProviders.all) {
            val region = if (p.needsRegion) "westeurope" else ""
            var last: Triple<String, String, HttpResp>? = null
            val send: (HttpReq) -> HttpResp = { r ->
                try { CloudHttp.send(r).also { last = Triple(r.method, r.url, it) } }
                catch (e: java.io.IOException) { last = Triple(r.method, r.url, HttpResp(0, e.toString())); throw e }
            }
            runCatching { CloudApi.transcribe(p, PROBE_KEY, "", region, pcm, "en", send, sleep = {}) }
            val row = last?.let { (method, url, resp) ->
                val (pass, why) = verdict(resp.status, resp.body)
                if (!pass) failed++
                "${if (pass) "PASS" else "FAIL"} ${resp.status} $method ${url.substringAfter("://")}  $why  ${resp.body.replace(PROBE_KEY, "<key>").replace("\n", " ").take(100)}"
            } ?: "FAIL no request sent".also { failed++ }
            rows += "%-12s batch %s".format(p.id, row)
            if (p.liveSupported) {
                val (pass, detail) = try {
                    val live = CloudLive(p, PROBE_KEY, "", region, "en", {})
                    live.feed(FloatArray(16_000) { pcm[it] / 32768f })
                    false to "accepted an invalid key? ${live.finish()}"
                } catch (e: CloudException) {
                    val d = e.detail.replace(PROBE_KEY, "<key>")
                    val http = Regex("^HTTP (\\d+) ?(.*)", RegexOption.DOT_MATCHES_ALL).find(d)
                    when {
                        http != null -> verdict(http.groupValues[1].toInt(), http.groupValues[2]).let { (ok, why) -> ok to "$why: $d" }
                        e.kind == CloudErrorKind.KeyRejected || AUTH_WORDS.any { it in d.lowercase() } -> true to "key rejected: $d"
                        else -> false to "${e.kind}: $d"
                    }
                }
                if (!pass) failed++
                rows += "%-12s live  %s %s".format(p.id, if (pass) "PASS" else "FAIL", detail.replace("\n", " ").take(160))
            }
        }
        rows.forEach(::println)
        println("$failed failed")
        assertTrue(rows.joinToString("\n"), failed == 0)
    }
}
