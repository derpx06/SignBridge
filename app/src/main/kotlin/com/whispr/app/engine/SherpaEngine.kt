package com.whispr.app.engine

import com.k2fsa.sherpa.onnx.OfflineModelConfig
import com.k2fsa.sherpa.onnx.OfflineRecognizer
import com.k2fsa.sherpa.onnx.OfflineRecognizerConfig
import com.k2fsa.sherpa.onnx.OfflineTransducerModelConfig
import com.k2fsa.sherpa.onnx.SileroVadModelConfig
import com.k2fsa.sherpa.onnx.Vad
import com.k2fsa.sherpa.onnx.VadModelConfig
import java.io.Closeable
import java.io.File
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicReference

/**
 * Silero VAD + a NeMo transducer (Parakeet TDT 110M int8), driven the same way as the desktop
 * worker (`engine/src/lib.rs`): the VAD cuts the stream into utterances, each final utterance is
 * decoded once, and while someone is still talking the current utterance is re-decoded every
 * [PARTIAL_EVERY_MS] for live words. Decoding runs on one thread; the audio thread never blocks on it.
 */
class SherpaEngine(modelDir: File, vadModel: File, threads: Int = 2) : Closeable {
    private val recognizer: OfflineRecognizer
    private val vad: Vad
    private val worker = Executors.newSingleThreadExecutor { Thread(it, "voicio-asr") }

    init {
        fun file(name: String): String =
            File(modelDir, name).also { require(it.isFile) { "model file missing: $it" } }.path
        recognizer = OfflineRecognizer(
            config = OfflineRecognizerConfig(
                modelConfig = OfflineModelConfig(
                    transducer = OfflineTransducerModelConfig(
                        encoder = file("encoder.int8.onnx"),
                        decoder = file("decoder.int8.onnx"),
                        joiner = file("joiner.int8.onnx"),
                    ),
                    tokens = file("tokens.txt"),
                    modelType = "nemo_transducer",
                    numThreads = threads,
                ),
            ),
        )
        require(vadModel.isFile) { "VAD model missing: $vadModel" }
        vad = Vad(
            config = VadModelConfig(
                sileroVadModelConfig = SileroVadModelConfig(
                    model = vadModel.path,
                    threshold = 0.5f,
                    minSilenceDuration = 0.8f, // fewer mid-sentence cuts than 0.5 s
                    minSpeechDuration = 0.12f, // one-word replies must survive
                    windowSize = VAD_WINDOW,
                    maxSpeechDuration = 30f,
                ),
                sampleRate = SAMPLE_RATE,
                numThreads = 1,
            ),
        )
    }

    private fun decode(samples: FloatArray): String {
        val stream = recognizer.createStream()
        try {
            stream.acceptWaveform(samples, SAMPLE_RATE)
            recognizer.decode(stream)
            return recognizer.getResult(stream).text.trim()
        } finally {
            stream.release()
        }
    }

    fun newSession(onLive: (String) -> Unit = {}): Session = Session(onLive)

    override fun close() {
        worker.shutdown()
        worker.awaitTermination(5, java.util.concurrent.TimeUnit.SECONDS)
        recognizer.release()
        vad.release()
    }

    /** One dictation. [feed] from the audio thread, then [stop] from a background thread. */
    inner class Session(private val onLive: (String) -> Unit) {
        private var audio = ShortArray(SAMPLE_RATE * 30)
        private var size = 0
        private val pending = FloatArray(VAD_WINDOW)
        private var pendingSize = 0
        private var lastEnd = 0
        private var speechStart = -1
        private var lastPartialAt = 0L
        private val finals = ArrayList<String>() // touched only on `worker`
        private val partial = AtomicReference<Pair<Int, Int>?>(null) // (start, end) of the live utterance
        @Volatile private var cancelled = false
        private var stopped = false

        /** The whole utterance as 16 kHz PCM, valid after [stop] (for the optional cloud path). */
        var pcm: ShortArray = ShortArray(0)
            private set

        val seconds: Double get() = size.toDouble() / SAMPLE_RATE

        /** Returns false once the 30-minute cap is hit; the caller should then [stop]. */
        fun feed(samples: FloatArray): Boolean {
            // Audio already queued on the main thread can arrive just after stop(); drop it quietly.
            if (stopped || cancelled) return true
            val room = MAX_SESSION_SAMPLES - size
            val accepted = minOf(samples.size, room)
            if (size + accepted > audio.size) audio = audio.copyOf(maxOf(audio.size * 2, size + accepted))
            var i = 0
            while (i < accepted) {
                val s = samples[i].let { if (it.isFinite()) it.coerceIn(-1f, 1f) else 0f }
                audio[size++] = (s * Short.MAX_VALUE).toInt().toShort()
                pending[pendingSize++] = s
                if (pendingSize == VAD_WINDOW) {
                    runCatching { vad.acceptWaveform(pending.copyOf()) }
                    pendingSize = 0
                }
                i++
            }
            runCatching { popFinals() }
            if (runCatching { vad.isSpeechDetected() }.getOrDefault(false)) {
                val start = if (speechStart < 0) maxOf(size - LOOKBACK, 0).also { speechStart = it } else speechStart
                val now = System.currentTimeMillis()
                if (size - start >= MIN_PARTIAL && now - lastPartialAt >= PARTIAL_EVERY_MS) {
                    lastPartialAt = now
                    queuePartial(start, size)
                }
            } else {
                speechStart = -1
            }
            return accepted == samples.size
        }

        private fun popFinals() {
            while (!vad.empty()) {
                val segment = vad.front()
                val rawStart = maxOf(segment.start, 0)
                val start = maxOf(rawStart - PREROLL, lastEnd)
                val end = minOf(rawStart + segment.samples.size + POSTROLL, size)
                vad.pop()
                if (start >= end) continue
                lastEnd = end
                partial.set(null) // a final supersedes the live decode of the same utterance
                val chunk = toFloat(start, end)
                worker.execute {
                    if (cancelled) return@execute
                    val text = decode(chunk)
                    if (text.isNotEmpty()) {
                        finals.add(text)
                        onLive(finals.joinToString(" "))
                    }
                }
                speechStart = -1
            }
        }

        private fun queuePartial(start: Int, end: Int) {
            val wasEmpty = partial.getAndSet(start to end) == null
            if (!wasEmpty) return
            worker.execute {
                val range = partial.getAndSet(null) ?: return@execute
                if (cancelled) return@execute
                val text = decode(toFloat(range.first, range.second))
                if (text.isNotEmpty() && !cancelled) {
                    onLive((finals + text).joinToString(" "))
                }
            }
        }

        private fun toFloat(start: Int, end: Int) =
            FloatArray(end - start) { audio[start + it] / Short.MAX_VALUE.toFloat() }

        /** Flushes the VAD, waits for every decode, and returns the raw transcript. Blocks. */
        fun stop(): String {
            if (stopped) return ""
            stopped = true
            // The native VAD can throw if the engine was released or reset under us (e.g. a field switch);
            // a dictation must never take the app down, so keep whatever was recognised so far.
            runCatching {
                if (pendingSize > 0) vad.acceptWaveform(pending.copyOf(pendingSize))
                vad.flush()
                popFinals()
            }
            pendingSize = 0
            partial.set(null)
            val done = worker.submit<String> { finals.joinToString(" ") }
            val text = done.get()
            runCatching { vad.reset() }
            pcm = audio.copyOf(size)
            audio = ShortArray(0)
            return text
        }

        /** Drops everything; queued decodes are skipped. */
        fun cancel() {
            cancelled = true
            stopped = true
            partial.set(null)
            vad.clear()
            vad.reset()
            audio = ShortArray(0)
        }
    }

    companion object {
        const val SAMPLE_RATE = 16_000
        private const val VAD_WINDOW = 512
        private const val PREROLL = SAMPLE_RATE * 3 / 10 // 0.3 s kept before each VAD start so first phonemes survive
        private const val POSTROLL = SAMPLE_RATE * 15 / 100 // the VAD trims its silence window; keep the word's tail
        private const val LOOKBACK = SAMPLE_RATE / 2 // VAD reports speech ~0.25-0.5 s after it begins
        private const val MIN_PARTIAL = SAMPLE_RATE / 5
        private const val PARTIAL_EVERY_MS = 250L
        private const val MAX_SESSION_SAMPLES = SAMPLE_RATE * 60 * 30
    }
}
