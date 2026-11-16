package com.whispr.app.flow

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import android.view.accessibility.AccessibilityNodeInfo

/**
 * Debug builds only: types text into the focused field through the running accessibility service.
 * `adb shell am broadcast -a com.whispr.app.DEBUG_INSERT -p io.github.derpx06.voicio.debug --es t "hello"`
 * then read `logcat -s VOICIO_TEST`.
 */
class DebugInsertReceiver : BroadcastReceiver() {
    override fun onReceive(c: Context, i: Intent) {
        val service = FlowAccessibilityService.instance
        if (service == null) { Log.i(TAG, "accessibility service is off"); return }
        val inserter = TextInserter(service)
        val n = inserter.focusedField()
        Log.i(TAG, "field text=${n?.text} hint=${n?.hintText} showingHint=${n?.isShowingHintText} sel=${n?.textSelectionStart}-${n?.textSelectionEnd} actions=${n?.actionList?.map { it.id }?.contains(AccessibilityNodeInfo.ACTION_PASTE)}")
        Log.i(TAG, "insert -> ${inserter.insert(i.getStringExtra("t") ?: "hello")}")
    }

    private companion object { const val TAG = "VOICIO_TEST" }
}
