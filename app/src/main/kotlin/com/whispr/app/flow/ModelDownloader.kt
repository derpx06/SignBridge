package com.whispr.app.flow

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.StatFs
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import org.apache.commons.compress.archivers.tar.TarArchiveInputStream
import org.apache.commons.compress.compressors.bzip2.BZip2CompressorInputStream
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.SocketTimeoutException
import java.net.URL
import java.net.UnknownHostException
import java.security.MessageDigest

/** A failure whose [message] is already fit to show the user. */
class DownloadException(message: String) : Exception(message)

/** What must be sorted out before a download can start. */
enum class Preflight { Ok, Offline, Metered, NoSpace }

/**
 * Downloads the speech model the user asked for. Every file is checked against a SHA-256 that is
 * pinned here, and nothing is moved into place until all checks pass. This is the only network
 * code for the speech model; dictation itself never touches the network.
 *
 * The download is resumable (HTTP Range into a `.part` file that survives rotation, a closed app and
 * process death) and runs on a process-wide scope, so leaving a screen or rotating does not restart it.
 * Screens just read [phase], [progress] and [error].
 */
object ModelDownloader {
    /** Sizes shown to the user: what is fetched, and what the unpacked model takes on the phone. */
    const val DOWNLOAD_MB = 105
    const val INSTALLED_MB = 130
    /** Archive + unpacked copy exist together for a moment, plus headroom. */
    private const val NEEDED_BYTES = 320L * 1024 * 1024

    private const val ASR_URL =
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet_tdt_transducer_110m-en-36000-int8.tar.bz2"
    private const val ASR_SHA256 = "f628312e9fdf8686374cb01a69425c41732529d540860311f16f37cbc32cfe9b"
    private const val VAD_URL = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx"
    private const val VAD_SHA256 = "9e2449e1087496d8d4caba907f23e0bd3f78d91fa552479bb9c23ac09cbb1fd6"

    /** Files kept from the archive (its folder name is dropped) and their SHA-256. */
    private val FILES = mapOf(
        "encoder.int8.onnx" to "0f35509ddeb9b39002fb077d979a9fe74f06eb0bc4dd5c34f512f82e5111d657",
        "decoder.int8.onnx" to "f7c331c5504c2e593c76ed22b728e3f554af6c4a383dde862e719ced08b1da19",
        "joiner.int8.onnx" to "bf7dff69e9f2cdbe9943d70da358f38b361c115ba0105bae7e908e0d6ec782f6",
        "tokens.txt" to "450e56bd2f036fe5b6aa821865838cc5aa9d8b0106134ce9a9ba0664abe6cd10",
    )

    enum class Phase { Idle, Downloading, Unpacking, Done, Failed }

    // Compose-observable state; lives for the process so screens can come and go.
    var phase by mutableStateOf(Phase.Idle); private set
    var progress by mutableFloatStateOf(0f); private set
    var error by mutableStateOf<String?>(null); private set

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var job: Job? = null
    @Volatile private var cancelled = false

    val running: Boolean get() = job?.isActive == true

    /** Checks the phone can take the download now. Cheap; call from the UI thread. */
    fun preflight(context: Context): Preflight {
        val cm = context.getSystemService(ConnectivityManager::class.java)
        val caps = cm?.getNetworkCapabilities(cm.activeNetwork)
        if (caps == null || !caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)) return Preflight.Offline
        val dir = ModelStore.root(context).also { it.mkdirs() }
        val partial = File(dir, ".download/asr.part").takeIf { it.exists() }?.length() ?: 0L
        if (StatFs(dir.path).availableBytes + partial < NEEDED_BYTES) return Preflight.NoSpace
        if (cm.isActiveNetworkMetered) return Preflight.Metered
        return Preflight.Ok
    }

    /** True when a half-finished download is waiting to be resumed. */
    fun hasPartial(context: Context) = File(ModelStore.root(context), ".download/asr.part").length() > 0

    /** Starts (or continues) the download in the background. Does nothing if one is already running. */
    fun start(context: Context) {
        if (running) return
        val app = context.applicationContext
        cancelled = false
        error = null
        phase = Phase.Downloading
        job = scope.launch {
            try {
                download(app) { progress = it }
                phase = Phase.Done
            } catch (e: Exception) {
                if (cancelled) { phase = Phase.Idle } else { error = friendly(e); phase = Phase.Failed }
            }
        }
    }

    /** Stops the running download but keeps the partial file, so the next [start] resumes. */
    fun cancel() { cancelled = true }

    /** Forgets a finished or failed run so a screen opened later starts clean. */
    fun reset() { if (!running) { phase = Phase.Idle; error = null; progress = 0f } }

    /** Removes the installed model and any half-finished download. */
    fun delete(context: Context) {
        cancel()
        EngineHolder.close()
        ModelStore.root(context).deleteRecursively()
        reset()
    }

    /** [onProgress] gets 0..1. Blocks; call from a background thread. Throws [DownloadException] or an IOException. */
    fun download(context: Context, onProgress: (Float) -> Unit) {
        val root = ModelStore.root(context).also { it.mkdirs() }
        val work = File(root, ".download").also { it.mkdirs() }
        var ok = false
        try {
            phase = Phase.Downloading
            val archive = File(work, "asr.part")
            fetch(ASR_URL, archive, ASR_SHA256) { onProgress(it * 0.85f) }
            phase = Phase.Unpacking
            val staged = File(work, "parakeet-110m").also { it.deleteRecursively(); it.mkdirs() }
            extract(archive, staged) { onProgress(0.85f + it * 0.12f) }
            val vad = File(work, "silero_vad.onnx")
            fetch(VAD_URL, vad, VAD_SHA256) { }
            EngineHolder.close()
            val target = ModelStore.asr(context)
            target.deleteRecursively()
            if (!staged.renameTo(target)) throw DownloadException("Could not install the model. Free some storage and try again.")
            if (!vad.renameTo(ModelStore.vad(context))) throw DownloadException("Could not install the voice detector. Free some storage and try again.")
            onProgress(1f)
            ok = true
        } finally {
            // Keep the partial archive after a failure so the next try resumes; drop everything once installed.
            if (ok) work.deleteRecursively() else File(work, "parakeet-110m").deleteRecursively()
        }
    }

    /** Resumes into [to] with a Range request when a partial file exists. */
    private fun fetch(url: String, to: File, sha256: String, onProgress: (Float) -> Unit) {
        val digest = MessageDigest.getInstance("SHA-256")
        var have = if (to.exists()) to.length() else 0L
        if (have > 0) to.inputStream().buffered().use { input -> // re-hash what we have so the final check covers every byte
            val buf = ByteArray(1 shl 16)
            while (true) { val n = input.read(buf); if (n < 0) break; digest.update(buf, 0, n) }
        }
        val connection = (URL(url).openConnection() as HttpURLConnection).apply {
            connectTimeout = 15_000
            readTimeout = 30_000
            instanceFollowRedirects = true
            if (have > 0) setRequestProperty("Range", "bytes=$have-")
        }
        try {
            val code = connection.responseCode
            when {
                code == 416 -> Unit // the partial file is already everything; the checksum below decides
                code == 200 -> if (have > 0) { to.delete(); digest.reset(); have = 0 } // server ignored Range: start over
                code == 206 && have > 0 -> Unit
                else -> throw DownloadException("The download server answered with an error (HTTP $code). Try again in a moment.")
            }
            if (code != 416) {
                val total = (have + connection.contentLengthLong).coerceAtLeast(1)
                var done = have
                FileOutputStream(to, have > 0).use { out ->
                    connection.inputStream.use { input ->
                        val buffer = ByteArray(64 * 1024)
                        while (true) {
                            if (cancelled) throw IOException("cancelled")
                            val n = input.read(buffer)
                            if (n < 0) break
                            out.write(buffer, 0, n)
                            digest.update(buffer, 0, n)
                            done += n
                            onProgress((done.toFloat() / total).coerceIn(0f, 1f))
                        }
                    }
                }
                if (done < total && connection.contentLengthLong > 0) throw IOException("connection closed early")
            }
            if (digest.digest().hex() != sha256) {
                to.delete()
                throw DownloadException("The downloaded file was damaged, so it was discarded. Tap Try again to download it fresh.")
            }
        } finally {
            connection.disconnect()
        }
    }

    private fun extract(archive: File, into: File, onProgress: (Float) -> Unit) {
        val size = archive.length().coerceAtLeast(1)
        val counted = CountingStream(archive.inputStream().buffered())
        TarArchiveInputStream(BZip2CompressorInputStream(counted)).use { tar ->
            val found = HashSet<String>()
            while (true) {
                if (cancelled) throw IOException("cancelled")
                val entry = tar.nextEntry ?: break
                val name = entry.name.substringAfterLast('/')
                val expected = FILES[name] ?: continue
                if (entry.isDirectory) continue
                val out = File(into, name) // name has no separators, so it cannot leave `into`
                val digest = MessageDigest.getInstance("SHA-256")
                out.outputStream().use { sink -> copyHashing(tar, sink, digest) { onProgress((counted.count.toFloat() / size).coerceIn(0f, 1f)) } }
                if (digest.digest().hex() != expected) throw DownloadException("A model file ($name) was damaged. Tap Try again to download it fresh.")
                found += name
            }
            if (found.size != FILES.size) throw DownloadException("The model download was incomplete. Tap Try again.")
        }
    }

    private class CountingStream(private val inner: InputStream) : InputStream() {
        var count = 0L
        override fun read(): Int = inner.read().also { if (it >= 0) count++ }
        override fun read(b: ByteArray, off: Int, len: Int): Int = inner.read(b, off, len).also { if (it > 0) count += it }
        override fun close() = inner.close()
    }

    private fun copyHashing(input: InputStream, out: java.io.OutputStream, digest: MessageDigest, tick: () -> Unit) {
        val buffer = ByteArray(64 * 1024)
        while (true) {
            val n = input.read(buffer)
            if (n < 0) return
            digest.update(buffer, 0, n)
            out.write(buffer, 0, n)
            tick()
        }
    }

    /** Turns an exception into a sentence a person can act on. Never shows raw exception text. */
    internal fun friendly(e: Exception): String = when {
        e is DownloadException -> e.message.orEmpty()
        e is UnknownHostException -> "Cannot reach the download server. Check your internet connection. Your progress is kept."
        e is SocketTimeoutException -> "The connection timed out. Check your internet connection and try again. Your progress is kept."
        e is IOException && e.message?.contains("space left", true) == true -> "Your phone is out of storage. Free about ${INSTALLED_MB * 2} MB and try again."
        e is IOException -> "The download was interrupted. Your progress is kept, so Try again continues where it stopped."
        else -> "Something went wrong while downloading. Tap Try again."
    }

    private fun ByteArray.hex() = joinToString("") { "%02x".format(it) }
}
