package com.whispr.app.edit

import android.app.Activity
import android.os.Bundle
import android.os.SystemClock
import android.util.Log
import com.whispr.core.dictation.Pipeline
import com.whispr.core.dictation.Dictionary
import com.whispr.core.edit.EditMode
import com.whispr.core.edit.EditRequest
import com.whispr.core.edit.EditResult
import com.whispr.core.edit.GuardedEditor
import com.whispr.core.edit.TextEditor
import kotlin.concurrent.thread

/** `adb shell am start -n io.github.derpx06.voicio.debug/com.whispr.app.edit.SelfTestActivity` then read `logcat -s VOICIO_TEST`. */
class SelfTestActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        thread {
            val tag = "VOICIO_TEST"
            Log.i(tag, "installed=${SmartEditModel.installed(this)} enoughMemory=${LlmTextEditor.hasEnoughMemory(this)}")
            val rules = TextEditor { EditResult.Edited(Pipeline.process(it.text, Dictionary(emptyList())).text, false) }
            val ai = GuardedEditor(LlmTextEditor(this), rules)
            val cases = listOf(
                "um so like can you send me the the invoice before friday and also uh remind me about the meeting at five thirty p m tomorrow thanks" to EditMode.Clean,
                "i dont think we should of gone there yesterday its to late now and their not answering" to EditMode.Proofread,
                "so basically what i wanted to say is that the meeting that we had planned for tuesday is now going to be moved to wednesday because the room is not available on tuesday" to EditMode.Rewrite,
                "i can come to the meeting tomorrow but not on friday" to EditMode.Clean,
            )
            for ((text, mode) in cases) {
                val t0 = SystemClock.elapsedRealtime()
                val r = try { ai.edit(EditRequest(text, mode)) } catch (e: Exception) { EditResult.Kept("ERROR ${e.message}", "exception") }
                val ms = SystemClock.elapsedRealtime() - t0
                val kind = if (r is EditResult.Edited) "AI" else "KEPT"
                Log.i(tag, "mode=$mode ms=$ms result=$kind :: ${when (r) { is EditResult.Edited -> r.text; is EditResult.Kept -> r.text + " | " + r.reason }}")
            }
            Log.i(tag, "done; model process stays loaded for 45 s")
            Thread.sleep(8000)
            finish()
        }
    }
}
