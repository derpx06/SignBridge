package com.whispr.app.engine

import android.annotation.SuppressLint
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.media.audiofx.AutomaticGainControl
import android.media.audiofx.NoiseSuppressor
import kotlin.math.abs
import kotlin.math.log10
import kotlin.math.max
import kotlin.math.sqrt

/** Level of one chunk of microphone audio, in dBFS (0 = full scale). */
data class Level(val peakDb: Double, val rmsDb: Double)

/**
 * 16 kHz mono microphone capture on its own thread. The caller must already hold RECORD_AUDIO.
 * [onChunk] receives about 50 ms of samples in -1..1 plus their [Level]; it runs on the capture
 * thread and must be quick. [onFailure] is called once if the microphone cannot be opened or dies.
 */
class AudioCapture(
    private val onChunk: (FloatArray, Level) -> Unit,
    private val onFailure: (String) -> Unit,
) {
    @Volatile private var running = false
    private var thread: Thread? = null

    @SuppressLint("MissingPermission")
    fun start() {
        if (running) return
        running = true
        thread = Thread({ run() }, "voicio-mic").also { it.start() }
    }

    /** Stops and waits for the capture thread so the microphone is free when this returns. */
    fun stop() {
        running = false
        thread?.join(1_000)
        thread = null
    }

    @SuppressLint("MissingPermission")
    private fun run() {
        val chunk = SAMPLE_RATE / 20
        val minBytes = AudioRecord.getMinBufferSize(SAMPLE_RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
        if (minBytes <= 0) return fail("This phone has no usable microphone input")
        val record = try {
            AudioRecord(
                MediaRecorder.AudioSource.VOICE_RECOGNITION,
                SAMPLE_RATE,
                AudioFormat.CHANNEL_IN_MONO,
                AudioFormat.ENCODING_PCM_16BIT,
                max(minBytes, chunk * 2 * 8),
            )
        } catch (e: Exception) {
            return fail("Could not open the microphone: ${e.message}")
        }
        if (record.state != AudioRecord.STATE_INITIALIZED) {
            record.release()
            return fail("The microphone is busy or not allowed")
        }
        // The phone's own noise suppression and gain control, where it has them.
        val effects = buildList {
            if (NoiseSuppressor.isAvailable()) add(NoiseSuppressor.create(record.audioSessionId))
            if (AutomaticGainControl.isAvailable()) add(AutomaticGainControl.create(record.audioSessionId))
        }
        try {
            record.startRecording()
            if (record.recordingState != AudioRecord.RECORDSTATE_RECORDING) {
                return fail("The microphone is in use by another app")
            }
            val shorts = ShortArray(chunk)
            while (running) {
                val n = record.read(shorts, 0, chunk)
                if (n < 0) return fail("The microphone stopped (error $n)")
                if (n == 0) continue
                val samples = FloatArray(n) { shorts[it] / 32768f }
                onChunk(samples, levelOf(samples))
            }
        } finally {
            effects.forEach { it?.release() }
            runCatching { record.stop() }
            record.release()
        }
    }

    private fun fail(message: String) {
        running = false
        onFailure(message)
    }

    companion object {
        const val SAMPLE_RATE = 16_000

        fun levelOf(samples: FloatArray): Level {
            var peak = 0f
            var sum = 0.0
            for (s in samples) {
                peak = max(peak, abs(s))
                sum += s * s
            }
            val rms = sqrt(sum / max(samples.size, 1))
            return Level(db(peak.toDouble()), db(rms))
        }

        private fun db(x: Double) = 20 * log10(max(x, 1e-9))
    }
}
