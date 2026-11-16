package com.whispr.app.edit

import android.app.ActivityManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.os.Bundle
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.Message
import android.os.Messenger
import com.whispr.app.flow.ModelStore
import com.whispr.core.edit.EditMode
import com.whispr.core.edit.EditPrompts
import com.whispr.core.edit.EditRequest
import com.whispr.core.edit.EditResult
import com.whispr.core.edit.TextEditor
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

/** Where the optional AI model lives and whether it is installed. */
object SmartEditModel {
    const val NAME = "S1-mini by Superwhisper"
    fun dir(context: Context) = File(ModelStore.root(context), "smart-edit")
    fun file(context: Context) = File(dir(context), "s1-mini-tied.gguf")
    fun installed(context: Context) = file(context).isFile
}

/**
 * Talks to the model process. Blocking by design (it is called from background threads), with a hard
 * timeout, and it refuses to start when the phone is short of memory so Voicio never makes things worse.
 */
class LlmTextEditor(context: Context) : TextEditor {
    private val app = context.applicationContext
    private val main = Handler(Looper.getMainLooper())

    override fun edit(request: EditRequest): EditResult {
        if (!SmartEditModel.installed(app)) throw IllegalStateException("Smart Edit is not installed")
        if (!hasEnoughMemory(app)) throw IllegalStateException("Low memory")
        val prompt = EditPrompts.build(request)
        val max = EditPrompts.maxNewTokens(request.text)
        val answer = AtomicReference<String?>(null)
        val error = AtomicReference<String?>(null)
        val done = CountDownLatch(1)
        val replyTo = Messenger(Handler(Looper.getMainLooper()) { m ->
            m.data.getString(LlmService.KEY_TEXT)?.let(answer::set)
            m.data.getString(LlmService.KEY_ERROR)?.let(error::set)
            done.countDown(); true
        })
        val ready = CountDownLatch(1)
        var service: Messenger? = null
        val connection = object : ServiceConnection {
            override fun onServiceConnected(name: ComponentName, binder: IBinder) { service = Messenger(binder); ready.countDown() }
            override fun onServiceDisconnected(name: ComponentName) { error.compareAndSet(null, "The model stopped (low memory?)"); done.countDown() }
        }
        main.post { app.bindService(Intent(app, LlmService::class.java), connection, Context.BIND_AUTO_CREATE) }
        try {
            if (!ready.await(5, TimeUnit.SECONDS)) throw IllegalStateException("The model did not start")
            val msg = Message.obtain(null, LlmService.MSG_EDIT).apply {
                data = Bundle().apply {
                    putString(LlmService.KEY_MODEL, SmartEditModel.file(app).path)
                    putString(LlmService.KEY_PROMPT, prompt)
                    putInt(LlmService.KEY_MAX_NEW, max)
                }
            }
            msg.replyTo = replyTo
            service!!.send(msg)
            if (!done.await(TIMEOUT_S, TimeUnit.SECONDS)) throw IllegalStateException("The model took too long")
            error.get()?.let { throw IllegalStateException(it) }
            return EditResult.Edited(answer.get().orEmpty(), usedModel = true)
        } finally {
            main.post { runCatching { app.unbindService(connection) } }
        }
    }

    companion object {
        const val TIMEOUT_S = 20L
        /** Do not start the model when less than this is free: ~400 MB for the model plus headroom for the app. */
        const val MIN_AVAILABLE_MB = 700

        fun hasEnoughMemory(context: Context): Boolean {
            val info = ActivityManager.MemoryInfo()
            context.getSystemService(ActivityManager::class.java).getMemoryInfo(info)
            return !info.lowMemory && info.availMem / (1024 * 1024) >= MIN_AVAILABLE_MB
        }
    }
}
