package com.whispr.app.edit

import android.content.Context
import android.os.StatFs
import com.whispr.core.edit.GgufTie
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest

/**
 * Downloads the Smart Edit model (S1-mini, Apache-2.0 plus a naming clause) with a pinned SHA-256,
 * then strips the duplicate output table on the device (see [GgufTie]) so it takes 127 MB less
 * storage and RAM. The download resumes if it was interrupted. This and the speech model download are
 * the only network use in the app.
 */
object SmartEditInstaller {
    private const val URL = "https://huggingface.co/superwhisper/s1-mini-GGUF/resolve/main/s1-mini-q4_k_m.gguf"
    private const val SHA256 = "3b41ebe2502cbd03e811d5d16b022f5ab551eda58d62597d152f89535003c634"
    private const val SIZE = 484_219_808L
    /** The original (484 MB) plus the stripped copy (357 MB) exist together for a moment. */
    private const val NEEDED_BYTES = 900L * 1024 * 1024

    fun hasSpace(context: Context): Boolean {
        val dir = SmartEditModel.dir(context).also { it.mkdirs() }
        return StatFs(dir.path).availableBytes >= NEEDED_BYTES
    }

    /** [onProgress] 0..1. Blocking; call from a background thread. Throws with a readable message. */
    fun install(context: Context, onProgress: (Float) -> Unit) {
        val dir = SmartEditModel.dir(context).also { it.mkdirs() }
        if (!hasSpace(context)) throw IllegalStateException("Not enough free storage (about 900 MB needed while installing)")
        val part = File(dir, "download.part")
        val full = File(dir, "s1-mini-q4_k_m.gguf")
        download(part) { onProgress(it * 0.9f) }
        check(part.renameTo(full)) { "Could not save the model" }
        try {
            check(GgufTie.dropTensor(full, SmartEditModel.file(context))) { "The model file has an unexpected layout" }
        } finally {
            full.delete()
        }
        onProgress(1f)
    }

    fun uninstall(context: Context) { SmartEditModel.dir(context).deleteRecursively() }

    private fun download(part: File, onProgress: (Float) -> Unit) {
        val digest = MessageDigest.getInstance("SHA-256")
        var have = if (part.exists()) part.length() else 0L
        if (have > SIZE) { part.delete(); have = 0 }
        if (have > 0) part.inputStream().use { input -> // re-hash what we already have so the final check covers every byte
            val buf = ByteArray(1 shl 16)
            while (true) { val n = input.read(buf); if (n < 0) break; digest.update(buf, 0, n) }
        }
        if (have < SIZE) {
            val c = (URL(URL).openConnection() as HttpURLConnection).apply {
                connectTimeout = 15_000; readTimeout = 30_000; instanceFollowRedirects = true
                if (have > 0) setRequestProperty("Range", "bytes=$have-")
            }
            try {
                val ok = c.responseCode == 200 || (c.responseCode == 206 && have > 0)
                check(ok) { "Download failed (HTTP ${c.responseCode})" }
                if (c.responseCode == 200 && have > 0) { part.delete(); digest.reset(); have = 0 } // server ignored Range
                java.io.FileOutputStream(part, have > 0).use { out ->
                    c.inputStream.use { input ->
                        val buf = ByteArray(1 shl 16)
                        var done = have
                        while (true) {
                            val n = input.read(buf); if (n < 0) break
                            out.write(buf, 0, n); digest.update(buf, 0, n); done += n
                            onProgress((done.toFloat() / SIZE).coerceIn(0f, 1f))
                        }
                    }
                }
            } finally { c.disconnect() }
        }
        val got = digest.digest().joinToString("") { "%02x".format(it) }
        if (got != SHA256) { part.delete(); throw IllegalStateException("The downloaded file is damaged (checksum mismatch). Try again.") }
    }
}
