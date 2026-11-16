package com.whispr.app.engine

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import com.whispr.app.edit.LlmTextEditor
import com.whispr.app.edit.SmartEditModel
import com.whispr.app.flow.EngineHolder
import com.whispr.app.flow.FlowSettings
import com.whispr.core.dictation.Dictionary
import com.whispr.core.dictation.NoteFormat
import com.whispr.core.dictation.Pipeline
import com.whispr.core.edit.EditRequest
import com.whispr.core.edit.EditResult
import com.whispr.core.edit.GuardedEditor
import com.whispr.core.edit.TextEditor
import com.whispr.core.edit.text
import java.util.concurrent.Executors

/**
 * Records one voice note inside the app (no accessibility service involved): tap to start, tap to stop.
 * Same engine, VAD and text pipeline as the bubble. Everything runs on-device; call from the main thread.
 */
class NoteRecorder(private val context: Context) {
    private val main = Handler(Looper.getMainLooper())
    private val background = Executors.newSingleThreadExecutor()
    private var capture: AudioCapture? = null
    @Volatile private var session: SherpaEngine.Session? = null
    private val backlog = ArrayList<FloatArray>()
    private var generation = 0
    var active = false
        private set

    fun start(onLive: (String) -> Unit, onLevel: (Float) -> Unit, onError: (String) -> Unit) {
        if (active) return
        active = true
        EngineHolder.busy = true
        val id = ++generation
        backlog.clear(); session = null
        val mic = AudioCapture(
            onChunk = { samples, level ->
                main.post {
                    if (id == generation && active) {
                        val s = session
                        if (s == null) backlog.add(samples) else s.feed(samples)
                        onLevel(Math.pow(((level.rmsDb + 58) / 40).coerceIn(0.0, 1.0), 0.75).toFloat())
                    }
                }
            },
            onFailure = { message -> main.post { if (id == generation) { cancel(); onError(message) } } },
        )
        capture = mic
        mic.start()
        background.execute {
            val engine = try { EngineHolder.get(context) } catch (e: Throwable) {
                EngineHolder.close()
                main.post { if (id == generation) { cancel(); onError("Not enough free memory. Close a few apps and try again.") } }
                return@execute
            }
            val live = engine.newSession { text -> main.post { if (id == generation) onLive(text) } }
            main.post {
                if (id != generation) { live.cancel(); return@post }
                session = live
                backlog.forEach { live.feed(it) }
                backlog.clear()
            }
        }
    }

    /** Stops listening and returns the finished, formatted text through [onText] (empty when nothing was heard). */
    fun stop(settings: FlowSettings, polishWithAi: Boolean, onText: (String) -> Unit) {
        if (!active) return
        active = false
        val id = generation
        val mic = capture
        capture = null
        background.execute {
            mic?.stop()
            var live: SherpaEngine.Session? = null
            val deadline = SystemClock.elapsedRealtime() + 30_000
            while (SystemClock.elapsedRealtime() < deadline && id == generation) {
                live = session
                if (live != null) break
                Thread.sleep(20)
            }
            if (id != generation || live == null) return@execute
            val result = try { process(settings, live.stop(), polishWithAi) } catch (e: Throwable) { "" }
            main.post {
                if (id != generation) return@post
                session = null
                EngineHolder.busy = false
                EngineHolder.touch()
                onText(result)
            }
        }
    }

    fun cancel() {
        generation++
        active = false
        val mic = capture; capture = null
        val s = session; session = null
        background.execute { mic?.stop(); runCatching { s?.cancel() } }
        EngineHolder.busy = false
    }

    /** Raw transcript to a tidy note body: rules, optional AI polish (guarded), then paragraphs and lists. */
    private fun process(settings: FlowSettings, raw: String, polishWithAi: Boolean): String {
        if (raw.isBlank()) return ""
        val dictionary = settings.dictionary()
        val useAi = polishWithAi && SmartEditModel.installed(context)
        var text = Pipeline.process(raw, if (useAi) Dictionary(emptyList()) else dictionary, Pipeline.Options(cleanup = settings.cleanup, format = settings.smartFormat)).text
        if (useAi && text.split(' ').size >= 4) {
            text = dictionary.replace(try {
                GuardedEditor(LlmTextEditor(context), TextEditor { EditResult.Edited(it.text, false) }).edit(EditRequest(text)).text
            } catch (e: Exception) { text })
        } else if (useAi) text = dictionary.replace(text)
        return NoteFormat.structure(text).body
    }
}
