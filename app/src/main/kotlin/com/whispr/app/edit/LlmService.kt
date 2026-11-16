package com.whispr.app.edit

import android.app.Service
import android.content.ComponentCallbacks2
import android.content.Intent
import android.os.Bundle
import android.os.Handler
import android.os.HandlerThread
import android.os.IBinder
import android.os.Message
import android.os.Messenger
import android.os.Process
import java.io.File

/**
 * Runs the on-device model in its own process (":llm"). The point of a separate process is memory:
 * ending it returns every byte (weights, buffers, native heap) to the system, which a phone needs.
 * It loads the model on the first request, and kills its own process after [IDLE_MS] without one or
 * as soon as Android reports memory pressure. Requests are answered one at a time.
 */
class LlmService : Service() {
    private lateinit var worker: HandlerThread
    private lateinit var handler: Handler
    private lateinit var messenger: Messenger
    private var handle = 0L
    private var initialised = false
    private val die = Runnable { shutdownProcess() }

    override fun onCreate() {
        worker = HandlerThread("voicio-llm", Process.THREAD_PRIORITY_BACKGROUND).also { it.start() }
        handler = Handler(worker.looper) { msg -> handle(msg); true }
        messenger = Messenger(handler)
    }

    override fun onBind(intent: Intent): IBinder = messenger.binder

    private fun handle(msg: Message) {
        handler.removeCallbacks(die)
        val reply = msg.replyTo
        val data = msg.data
        val result = Bundle()
        try {
            if (msg.what == MSG_EDIT) {
                val model = File(data.getString(KEY_MODEL).orEmpty())
                ensureLoaded(model)
                val answer = LlamaBridge.nativeGenerate(handle, data.getString(KEY_PROMPT).orEmpty(), data.getInt(KEY_MAX_NEW))
                if (answer == null) result.putString(KEY_ERROR, "The model could not finish.") else result.putString(KEY_TEXT, answer.trim())
            }
        } catch (t: Throwable) { // native load failure, out of memory...: report it, never crash the caller
            result.putString(KEY_ERROR, t.message ?: t.javaClass.simpleName)
            freeModel()
        }
        runCatching { reply?.send(Message.obtain(null, msg.what, msg.arg1, 0).also { it.data = result }) }
        handler.postDelayed(die, IDLE_MS)
    }

    private fun ensureLoaded(model: File) {
        if (handle != 0L) return
        require(model.isFile) { "Smart Edit model is not installed" }
        if (!initialised) { LlamaBridge.nativeInit(applicationInfo.nativeLibraryDir); initialised = true }
        handle = LlamaBridge.nativeLoad(model.path, THREADS)
        require(handle != 0L) { "Could not load the model" }
    }

    private fun freeModel() { if (handle != 0L) { LlamaBridge.nativeFree(handle); handle = 0L } }

    override fun onTrimMemory(level: Int) {
        // Any real pressure: give everything back by ending this process; the next request reloads it.
        if (level >= ComponentCallbacks2.TRIM_MEMORY_RUNNING_LOW) handler.post { shutdownProcess() }
    }

    private fun shutdownProcess() { freeModel(); Process.killProcess(Process.myPid()) }

    override fun onDestroy() { handler.removeCallbacks(die); freeModel(); worker.quit() }

    companion object {
        const val MSG_EDIT = 1
        const val KEY_MODEL = "model"
        const val KEY_PROMPT = "prompt"
        const val KEY_MAX_NEW = "max_new"
        const val KEY_TEXT = "text"
        const val KEY_ERROR = "error"
        /** Free the model 45 seconds after the last request. */
        const val IDLE_MS = 45_000L
        /** Measured on the target phone: 3 threads were as fast as 4 and kinder to the rest of the system. */
        const val THREADS = 3
    }
}
