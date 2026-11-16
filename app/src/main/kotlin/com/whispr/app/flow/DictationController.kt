package com.whispr.app.flow

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import com.whispr.app.cloud.CloudClient
import com.whispr.core.cloud.CloudException
import com.whispr.core.cloud.CloudLive
import com.whispr.app.engine.AudioCapture
import com.whispr.app.engine.SherpaEngine
import com.whispr.app.edit.LlmTextEditor
import com.whispr.app.edit.SmartEditModel
import com.whispr.core.dictation.Dictionary
import com.whispr.core.dictation.Pipeline
import com.whispr.core.edit.EditRequest
import com.whispr.core.edit.EditResult
import com.whispr.core.edit.GuardedEditor
import com.whispr.core.edit.TextEditor
import com.whispr.core.edit.text
import com.whispr.core.dictation.Trigger
import java.io.File
import java.util.concurrent.Executors

enum class Phase { Idle, Warming, Listening, Working, Done }
enum class NoticeKind { Warn, Error, Info }
/** [retry]: the field offers a Retry button (the dictation can simply be tried again). */
data class Notice(val kind: NoticeKind, val text: String, val retry: Boolean = false)

/** Everything the bubble needs to draw. */
data class FlowState(
    val phase: Phase = Phase.Idle,
    val live: String = "",
    val notice: Notice? = null,
    val level: Float = 0f,
    val canUndo: Boolean = false,
    /** The finished utterance is being sent to the user's cloud provider. */
    val cloud: Boolean = false,
    /** Ten seconds or less until the time limit (one warning tick). */
    val nearLimit: Boolean = false,
)

/** Where the speech models live: the app's private storage (no storage permission, invisible to other apps). */
object ModelStore {
    fun root(context: Context): File = File(context.filesDir, "models")
    fun asr(context: Context) = File(root(context), "parakeet-110m")
    fun vad(context: Context) = File(root(context), "silero_vad.onnx")
    fun ready(context: Context): Boolean =
        listOf("encoder.int8.onnx", "decoder.int8.onnx", "joiner.int8.onnx", "tokens.txt").all { File(asr(context), it).isFile } &&
            vad(context).isFile
}

/**
 * One dictation from press to typed text. The bubble calls [press] / [release] on the main thread.
 * [Trigger] decides what a press means (hold, toggle, hold-or-tap), the microphone and the model run
 * off the main thread, and [insert] puts the finished text into the focused field.
 *
 * Audio spoken while the model is still loading is kept and replayed, so a cold start loses no words.
 */
class DictationController(
    private val context: Context,
    private val settings: FlowSettings,
    private val insert: (String) -> InsertResult,
    private val onState: (FlowState) -> Unit,
    /** The keyboard always uses hold-or-tap; the bubble follows the user's setting. */
    private val modeOverride: Trigger.Mode? = null,
    /** Longest dictation. The keyboard allows 30 minutes (the engine's own cap). */
    private val maxHoldMs: Long = MAX_HOLD_MS,
) {
    private val main = Handler(Looper.getMainLooper())
    private val background = Executors.newSingleThreadExecutor { Thread(it, "voicio-flow") }
    private val trigger = Trigger(settings.shortcutMode)
    private var state = FlowState()
    private var capture: AudioCapture? = null
    @Volatile private var session: SherpaEngine.Session? = null
    /** Live cloud dictation (the provider's Live toggle): its words replace the on-device partials. */
    @Volatile private var cloudLive: CloudLive? = null
    private val backlog = ArrayList<FloatArray>() // audio that arrived before the model was ready
    private var generation = 0 // bumps on every start/cancel so late callbacks from an old dictation are ignored
    private var lastInsert: InsertResult.Inserted? = null
    private var peakDb = -120.0
    private var clipped = false
    private var cloudOnly: ArrayList<FloatArray>? = null // recording for a cloud-only dictation (no on-device model)
    private var noticeClear: Runnable? = null
    private val tick = Runnable { act(trigger.due(now())) }
    private val maxHold = Runnable { if (state.phase == Phase.Listening) { notice(NoticeKind.Warn, "Time limit reached"); finish() } }

    /**
     * Runs on the raw words of a finished dictation (background thread) before the pipeline: return the
     * text to process, or null when the words were a command the owner handled (nothing is typed).
     */
    @Volatile var onRaw: ((String) -> String?)? = null
    /** Microphone gain (whisper mode raises it). */
    @Volatile var gain = 1f
    /** Hands-free only: stop after this much silence once speech was heard (0 = never). */
    var autoStopMs = 0L
    /** Continuous mode (double-tap on the keyboard mic): no auto-stop. */
    var continuous = false
    private var pressed = false
    private var heardAt = 0L
    private val nearLimit = Runnable { if (state.phase == Phase.Listening) publish(state.copy(nearLimit = true)) }
    private val undoExpire = Runnable { if (state.phase == Phase.Idle && state.notice == null) publish(state.copy(canUndo = false)) }

    private fun now() = SystemClock.elapsedRealtime()

    fun press() {
        pressed = true
        trigger.setMode(modeOverride ?: settings.shortcutMode)
        val dictation = when (state.phase) {
            Phase.Idle, Phase.Done -> Trigger.Dictation.Idle
            Phase.Warming, Phase.Listening -> Trigger.Dictation.Listening
            Phase.Working -> Trigger.Dictation.Busy
        }
        act(trigger.press(now(), dictation))
    }

    fun release() {
        pressed = false
        trigger.release(now())
        trigger.deadlineMs()?.let { main.postDelayed(tick, maxOf(it - now(), 0)) }
    }

    /** The user dragged the bubble: this press was not a dictation. */
    fun abandonPress() {
        pressed = false
        trigger.release(now())
        main.removeCallbacks(tick)
        trigger.due(Long.MAX_VALUE)
        if (state.phase == Phase.Warming || state.phase == Phase.Listening) cancel()
    }

    fun undo() {
        val inserted = lastInsert ?: return
        lastInsert = null
        publish(state.copy(canUndo = false))
        // The service wires TextInserter.undo through `insert`'s owner; see FlowAccessibilityService.
        onUndo?.invoke(inserted)
    }

    var onUndo: ((InsertResult.Inserted) -> Unit)? = null

    /** The Done button: stop listening and insert (same as releasing a hold). */
    fun done() = finish()

    /** TalkBack's double-tap on the orb: always hands-free, whatever the press mode. */
    fun startHandsFree() {
        if (state.phase == Phase.Idle || state.phase == Phase.Done) { trigger.reset(); start() }
    }

    /** The Retry button on a notice: clear it and listen again, hands-free. */
    fun retry() {
        cancel()
        start()
    }

    /** A short info notice from the owner (the keyboard's rewrite results). */
    fun notify(text: String) = notice(NoticeKind.Info, text)

    /** Close a notice early (the X on an error). */
    fun dismiss() {
        noticeClear?.let(main::removeCallbacks)
        if (state.phase == Phase.Idle || state.phase == Phase.Done) publish(FlowState())
    }

    private fun act(action: Trigger.Action) {
        when (action) {
            Trigger.Action.None -> Unit
            Trigger.Action.Start -> start()
            Trigger.Action.Stop -> finish()
            Trigger.Action.Discard -> cancel() // Hold-only mode and a quick tap: quietly nothing, never an error
            Trigger.Action.Busy -> notice(NoticeKind.Info, "One moment…")
        }
    }

    private fun start() {
        if (!ModelStore.ready(context)) {
            if (CloudClient.configured(context, settings) != null) return startCloudOnly() // cloud covers the missing model
            trigger.reset()
            notice(NoticeKind.Error, "Download a speech model first")
            return
        }
        cloudOnly = null
        val id = ++generation
        peakDb = -120.0
        clipped = false
        backlog.clear()
        session = null
        publish(FlowState(phase = Phase.Warming))
        heardAt = 0L
        main.postDelayed(maxHold, maxHoldMs)
        main.postDelayed(nearLimit, maxHoldMs - 10_000)
        val mic = AudioCapture(
            onChunk = { samples, level -> main.post { if (id == generation) onAudio(samples, level) } },
            onFailure = { message -> main.post { if (id == generation) { cancel(); notice(NoticeKind.Error, message, retry = true) } } },
        )
        capture = mic
        mic.start()
        cloudLive = if (settings.cloudMode == "always") startLive(id) else null // "fallback": the on-device model is here
        background.execute {
            val engine = try {
                EngineHolder.get(context)
            } catch (e: Throwable) { // includes OutOfMemoryError: never take the whole service down
                EngineHolder.close()
                main.post { if (id == generation) { cancel(); notice(NoticeKind.Warn, "Not enough free memory. Close a few apps and try again.", retry = true) } }
                return@execute
            }
            val live = engine.newSession { text -> if (settings.liveText && cloudLive?.failed != false) main.post { if (id == generation) publish(state.copy(live = text)) } }
            main.post {
                if (id != generation) { live.cancel(); return@post }
                session = live
                backlog.forEach { live.feed(it) }
                backlog.clear()
                if (state.phase == Phase.Warming) publish(state.copy(phase = Phase.Listening))
            }
        }
    }

    /** No on-device model, cloud on: record only; the whole recording goes to the provider on release. */
    private fun startCloudOnly() {
        val id = ++generation
        peakDb = -120.0
        clipped = false
        backlog.clear()
        session = null
        cloudOnly = ArrayList()
        publish(FlowState(phase = Phase.Listening, cloud = true))
        heardAt = 0L
        main.postDelayed(maxHold, maxHoldMs)
        main.postDelayed(nearLimit, maxHoldMs - 10_000)
        capture = AudioCapture(
            onChunk = { samples, level -> main.post { if (id == generation) onAudio(samples, level) } },
            onFailure = { message -> main.post { if (id == generation) { cancel(); notice(NoticeKind.Error, message, retry = true) } } },
        ).also { it.start() }
        cloudLive = startLive(id)
    }

    /** The live cloud session for dictation [id], or null (Live off, unsupported, or it could not start). */
    private fun startLive(id: Int): CloudLive? = CloudClient.liveFor(context, settings) { text ->
        if (settings.liveText) main.post { if (id == generation) publish(state.copy(live = text)) }
    }

    private fun onAudio(input: FloatArray, inputLevel: com.whispr.app.engine.Level) {
        val g = gain
        val samples = if (g == 1f) input else FloatArray(input.size) { (input[it] * g).coerceIn(-1f, 1f) }
        val level = if (g == 1f) inputLevel else AudioCapture.levelOf(samples)
        peakDb = maxOf(peakDb, level.peakDb)
        if (level.rmsDb > SPEECH_RMS_DB) heardAt = now()
        else if (autoStopMs > 0 && !continuous && !pressed && heardAt > 0 && now() - heardAt > autoStopMs && state.phase == Phase.Listening) {
            finish()
            return
        }
        if (level.peakDb > -0.5) clipped = true
        cloudLive?.feed(samples)
        val s = session
        val recording = cloudOnly
        if (recording != null) {
            recording.add(samples)
            publish(state.copy(level = Math.pow(((level.rmsDb + 58) / 40).coerceIn(0.0, 1.0), 0.75).toFloat()))
            return
        }
        if (s == null) backlog.add(samples) else if (!s.feed(samples)) {
            notice(NoticeKind.Warn, "Time limit reached")
            finish()
            return
        }
        val meter = Math.pow(((level.rmsDb + 58) / 40).coerceIn(0.0, 1.0), 0.75).toFloat() // speech sits around -35..-15 dBFS
        publish(state.copy(level = meter))
    }

    private fun finish() {
        if (state.phase != Phase.Listening && state.phase != Phase.Warming) return
        main.removeCallbacks(maxHold)
        main.removeCallbacks(nearLimit)
        val id = generation
        val mic = capture
        capture = null
        publish(state.copy(phase = Phase.Working, level = 0f))
        val peak = peakDb
        val wasClipped = clipped
        val recorded = cloudOnly
        cloudOnly = null
        val streamed = cloudLive
        cloudLive = null
        background.execute {
            mic?.stop()
            if (recorded != null) return@execute finishCloudOnly(id, recorded, peak, wasClipped, streamed)
            // The model may still be loading: wait for the session the start() job creates.
            var live: SherpaEngine.Session? = null
            val deadline = SystemClock.elapsedRealtime() + 30_000
            while (SystemClock.elapsedRealtime() < deadline && id == generation) {
                live = session
                if (live != null) break
                Thread.sleep(20)
            }
            if (id != generation || live == null) { streamed?.cancel(); return@execute }
            var raw = try { live.stop() } catch (e: Throwable) {
                main.post { if (id == generation) { cancel(); notice(NoticeKind.Error, "Something went wrong. Please try again.", retry = true) } }
                return@execute
            }
            var cloudNote: String? = null
            if (streamed != null) {
                // live cloud: its text replaces the on-device one; any failure keeps the on-device text
                try {
                    val text = streamed.finish { id != generation }
                    if (text.isNotBlank()) raw = text
                } catch (e: CloudException) {
                    if (e.kind != com.whispr.core.cloud.CloudErrorKind.Cancelled) cloudNote = "Cloud: ${e.kind.message}. Used on-device."
                } catch (e: Throwable) {
                    cloudNote = "Cloud failed. Used on-device."
                }
            }
            val cloud = if (streamed == null && raw.isNotBlank() && settings.cloudMode == "always") CloudClient.configured(context, settings) else null
            if (cloud != null) {
                main.post { if (id == generation) publish(state.copy(cloud = true)) }
                try {
                    val text = CloudClient.transcribe(cloud, live.pcm, cancelled = { id != generation })
                    if (text.isNotBlank()) raw = text // blank: keep the on-device result
                } catch (e: CloudException) {
                    cloudNote = "Cloud: ${e.kind.message}. Used on-device."
                } catch (e: Throwable) { // any failure at all: the dictation must survive
                    cloudNote = "Cloud failed. Used on-device."
                }
            }
            process(id, raw, peak, wasClipped, cloudNote)
        }
    }

    /** Cloud-only dictation (no on-device model): the cloud text takes the same cleanup + voice-command path. */
    private fun finishCloudOnly(id: Int, chunks: List<FloatArray>, peak: Double, wasClipped: Boolean, streamed: CloudLive?) {
        // live cloud first; if the stream failed, the kept recording is sent whole
        if (streamed != null) {
            val text = runCatching { streamed.finish { id != generation } }.getOrNull()
            if (id != generation) return
            if (!text.isNullOrBlank()) return process(id, text, peak, wasClipped, null)
        }
        val setup = CloudClient.configured(context, settings)
        val pcm = ShortArray(chunks.sumOf { it.size }).also { out ->
            var i = 0
            for (c in chunks) for (v in c) out[i++] = (v.coerceIn(-1f, 1f) * 32767).toInt().toShort()
        }
        val raw = try {
            if (setup == null) "" else CloudClient.transcribe(setup, pcm, cancelled = { id != generation })
        } catch (e: Throwable) {
            val why = (e as? CloudException)?.kind?.message ?: "Cloud failed"
            main.post { if (id == generation) { cancel(); notice(NoticeKind.Error, "$why. No on-device model to fall back to.", retry = true) } }
            return
        }
        process(id, raw, peak, wasClipped, null)
    }

    private fun process(id: Int, heard: String, peak: Double, wasClipped: Boolean, cloudNote: String?) {
        val hook = onRaw
        val raw = if (hook == null || heard.isBlank()) heard else hook(heard) ?: run {
            main.post { if (id == generation) { trigger.reset(); session = null; publish(FlowState()); EngineHolder.touch() } }
            return
        }
        val dictionary = settings.dictionary()
        val options = Pipeline.Options(cleanup = settings.cleanup, format = settings.smartFormat, spoken = settings.spokenCommands)
        val useAi = settings.aiPolish && SmartEditModel.installed(context)
        // The AI sees the rule-cleaned text; the user's own words are applied after it so it cannot alter them.
        var processed = Pipeline.process(raw, if (useAi) Dictionary(emptyList()) else dictionary, options)
        if (useAi && processed.text.split(' ').size >= MIN_WORDS_FOR_AI) {
            val polished = try {
                GuardedEditor(LlmTextEditor(context), TextEditor { EditResult.Edited(it.text, false) })
                    .edit(EditRequest(processed.text)).text
            } catch (e: Exception) { processed.text } // no memory, no model, timeout: keep the rule result
            processed = processed.copy(text = dictionary.replace(polished))
        } else if (useAi) {
            processed = processed.copy(text = dictionary.replace(processed.text))
        }
        main.post {
            if (id != generation) return@post
            deliver(processed.text, raw.isBlank(), peak, wasClipped)
            cloudNote?.let { notice(NoticeKind.Info, it) }
        }
    }

    private fun deliver(text: String, nothingHeard: Boolean, peak: Double, wasClipped: Boolean) {
        trigger.reset()
        if (nothingHeard || text.isBlank()) {
            val (kind, message) = when {
                peak < QUIET_PEAK_DB -> NoticeKind.Warn to "Microphone is silent"
                else -> NoticeKind.Warn to "Didn't catch that"
            }
            session = null
            publish(FlowState())
            notice(kind, message, retry = true)
            return
        }
        when (val result = insert(text)) {
            is InsertResult.Inserted -> {
                lastInsert = result
                session = null
                publish(FlowState(phase = Phase.Done, live = text, canUndo = true))
                if (wasClipped) notice(NoticeKind.Warn, "Too loud: move the phone back") else scheduleIdle(DONE_MS)
            }
            InsertResult.Copied -> { session = null; publish(FlowState(phase = Phase.Done)); notice(NoticeKind.Info, "Copied: paste it") }
            InsertResult.NoField -> { session = null; publish(FlowState(phase = Phase.Done)); notice(NoticeKind.Info, "No text field: copied") }
            InsertResult.Password -> { session = null; publish(FlowState()); notice(NoticeKind.Warn, "Won't type into a password field") }
        }
        EngineHolder.touch()
    }

    fun cancel() {
        generation++
        main.removeCallbacks(maxHold)
        main.removeCallbacks(nearLimit)
        main.removeCallbacks(tick)
        capture?.stop()
        capture = null
        session?.cancel()
        session = null
        cloudLive?.cancel()
        cloudLive = null
        cloudOnly = null
        backlog.clear()
        trigger.reset()
        publish(FlowState())
    }

    private fun notice(kind: NoticeKind, text: String, retry: Boolean = false) {
        noticeClear?.let(main::removeCallbacks)
        publish(state.copy(notice = Notice(kind, text, retry)))
        scheduleIdle(if (kind == NoticeKind.Info) 2_500 else 4_000)
    }

    private fun scheduleIdle(afterMs: Long = 2_000) {
        noticeClear?.let(main::removeCallbacks)
        val clear = Runnable {
            if (state.phase == Phase.Done || state.phase == Phase.Idle) {
                publish(FlowState(canUndo = state.canUndo))
                main.removeCallbacks(undoExpire)
                if (state.canUndo) main.postDelayed(undoExpire, UNDO_MS)
            }
        }
        noticeClear = clear
        main.postDelayed(clear, afterMs)
    }

    private fun publish(next: FlowState) {
        state = next
        EngineHolder.busy = next.phase != Phase.Idle
        onState(next)
    }

    /** The user left the app: drop the last insert (it holds the field's earlier text) and any shown words. */
    fun forget() {
        lastInsert = null
        main.removeCallbacks(undoExpire)
        if (state.phase == Phase.Idle && (state.live.isNotEmpty() || state.canUndo)) publish(FlowState())
    }

    /** Loads the speech model in the background so the first words come back fast (the keyboard calls this when it opens). */
    fun warm() {
        if (!ModelStore.ready(context)) return
        background.execute { try { EngineHolder.get(context); EngineHolder.touch() } catch (e: Throwable) { EngineHolder.close() } }
    }

    /** [releaseEngine] false: another surface (bubble or keyboard) may still be using the shared model. */
    fun shutdown(releaseEngine: Boolean = true) {
        cancel()
        if (releaseEngine) EngineHolder.close() else EngineHolder.touch()
        background.shutdown()
    }

    companion object {
        private const val MAX_HOLD_MS = 120_000L
        /** The check mark shows this long, then the bar folds back into the orb with an Undo pill. */
        private const val DONE_MS = 450L
        private const val UNDO_MS = 4_000L
        private const val QUIET_PEAK_DB = -50.0
        /** Above this the chunk counts as speech for auto-stop (speech sits around -35..-15 dBFS). */
        private const val SPEECH_RMS_DB = -42.0
        /** Shorter dictations are left to the rules: tiny inputs are where a small model is most likely to reply instead of echo. */
        private const val MIN_WORDS_FOR_AI = 4
    }
}

/** Keeps one loaded engine for the whole process and frees it after five idle minutes. */
object EngineHolder {
    private var engine: SherpaEngine? = null
    private val main = Handler(Looper.getMainLooper())
    private val unload = Runnable { close() }

    @Synchronized
    fun get(context: Context): SherpaEngine {
        main.removeCallbacks(unload)
        return engine ?: SherpaEngine(ModelStore.asr(context), ModelStore.vad(context)).also { engine = it }
    }

    fun touch() {
        main.removeCallbacks(unload)
        main.postDelayed(unload, 5 * 60_000L)
    }

    /** The system is short of memory: free the model if nothing is being dictated. */
    fun trim() { if (!busy) close() }
    @Volatile var busy = false

    @Synchronized
    fun close() {
        engine?.close()
        engine = null
    }
}
