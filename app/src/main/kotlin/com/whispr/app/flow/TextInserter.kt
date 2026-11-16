package com.whispr.app.flow

import android.accessibilityservice.AccessibilityService
import android.content.ClipData
import android.content.ClipboardManager
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.PersistableBundle
import android.view.accessibility.AccessibilityNodeInfo
import com.whispr.core.dictation.FieldInsert

/** What happened when we tried to put dictated text into the focused field. */
sealed interface InsertResult {
    /**
     * Typed into the field. [before] is the field's text before (for the SET_TEXT route), and
     * [pastedAt] is where the text went when it was pasted at the caret (-1 otherwise).
     */
    data class Inserted(val length: Int, val before: String, val inserted: String = "", val pastedAt: Int = -1) : InsertResult
    /** The field refused text, so it is on the clipboard for the user to paste. */
    data object Copied : InsertResult
    data object NoField : InsertResult
    data object Password : InsertResult
}

/**
 * Inserts text into the focused input field through the accessibility API (the same route
 * Wispr Flow uses on Android).
 *
 * The field's reported text cannot be trusted: an empty field often reports its placeholder
 * ("Message", "Ask Meta AI or search") as if it were text. So the preferred route does not read
 * the text at all: the dictation is pasted at the caret and the user's clipboard is put back
 * afterwards. If a field does not accept paste we fall back to setting its text, using only text
 * we are sure is real.
 */
class TextInserter(private val service: AccessibilityService) {
    private val main = Handler(Looper.getMainLooper())

    fun focusedField(): AccessibilityNodeInfo? {
        val root = service.rootInActiveWindow ?: return null
        root.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)?.takeIf { looksEditable(it) }?.let { return it }
        // Some notes/editor apps do not report input focus: look for a focused editable view in the tree.
        return findEditable(root, 0)
    }

    private fun looksEditable(n: AccessibilityNodeInfo) =
        n.isEditable || n.className?.toString()?.contains("EditText", true) == true ||
            n.actionList.any { it.id == AccessibilityNodeInfo.ACTION_SET_TEXT }

    private fun findEditable(n: AccessibilityNodeInfo, depth: Int): AccessibilityNodeInfo? {
        if (depth > 40) return null
        if ((n.isFocused || n.isAccessibilityFocused) && looksEditable(n)) return n
        for (i in 0 until n.childCount) {
            val hit = n.getChild(i)?.let { findEditable(it, depth + 1) }
            if (hit != null) return hit
        }
        return null
    }

    fun insert(text: String): InsertResult {
        val node = focusedField() ?: return clipboardOnly(text, InsertResult.NoField)
        if (node.isPassword) return InsertResult.Password
        val hint = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) node.hintText else null
        val reported = node.text
        val placeholder = FieldInsert.isPlaceholder(reported, hint, node.isShowingHintText)
        val real = if (placeholder) "" else reported?.toString().orEmpty()
        val start = if (placeholder) 0 else node.textSelectionStart.coerceAtLeast(0).coerceAtMost(real.length)
        val end = if (placeholder) 0 else node.textSelectionEnd.coerceAtLeast(0).coerceAtMost(real.length)
        val piece = FieldInsert.merge(real, null, false, start, end, text)

        run {
            // The paste action is only advertised once the clipboard holds something, so do not check the action list first.
            val clipboard = service.getSystemService(ClipboardManager::class.java)
            val saved = clipboard.primaryClip
            clipboard.setPrimaryClip(sensitiveClip(piece.inserted))
            val pasted = node.performAction(AccessibilityNodeInfo.ACTION_PASTE)
            restoreClipboardLater(clipboard, saved)
            if (pasted) return InsertResult.Inserted(piece.inserted.length, real, piece.inserted, pastedAt = 0)
        }

        // Fallback: set the whole text, only when the field is empty so no reported text has to be trusted.
        if (real.isNotEmpty()) return clipboardOnly(text, InsertResult.NoField)
        val set = node.performAction(
            AccessibilityNodeInfo.ACTION_SET_TEXT,
            Bundle().apply { putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, piece.text) },
        )
        if (set) {
            node.performAction(
                AccessibilityNodeInfo.ACTION_SET_SELECTION,
                Bundle().apply {
                    putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT, piece.caret)
                    putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT, piece.caret)
                },
            )
            return InsertResult.Inserted(piece.inserted.length, real, piece.inserted)
        }
        return clipboardOnly(text, InsertResult.NoField)
    }

    /** Undoes the last insert: removes exactly the text we added, and only if it is still there. */
    fun undo(result: InsertResult.Inserted): Boolean {
        val node = focusedField() ?: return false
        if (result.pastedAt >= 0 && result.inserted.isNotEmpty()) {
            node.refresh()
            val now = node.text?.toString().orEmpty()
            val from = now.lastIndexOf(result.inserted) // where it landed (the reported caret cannot be trusted)
            if (from < 0) return false // edited since: leave it alone
            val to = from + result.inserted.length
            node.performAction(
                AccessibilityNodeInfo.ACTION_SET_SELECTION,
                Bundle().apply {
                    putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT, from)
                    putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT, to)
                },
            )
            val clipboard = service.getSystemService(ClipboardManager::class.java)
            val saved = clipboard.primaryClip
            val cut = node.performAction(AccessibilityNodeInfo.ACTION_CUT)
            restoreClipboardLater(clipboard, saved)
            return cut
        }
        return node.performAction(
            AccessibilityNodeInfo.ACTION_SET_TEXT,
            Bundle().apply { putCharSequence(AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE, result.before) },
        )
    }

    /** The target app pastes asynchronously, so the user's clipboard is restored a moment later. */
    private fun restoreClipboardLater(clipboard: ClipboardManager, saved: ClipData?) {
        main.postDelayed({
            runCatching { if (saved != null) clipboard.setPrimaryClip(saved) else if (Build.VERSION.SDK_INT >= 28) clipboard.clearPrimaryClip() }
        }, 600)
    }

    /** Marks the clip sensitive so Android does not pop up a preview of the dictated text. */
    private fun sensitiveClip(text: String): ClipData = ClipData.newPlainText("Voicio", text).apply {
        description.extras = PersistableBundle().apply { putBoolean("android.content.extra.IS_SENSITIVE", true) }
    }

    private fun clipboardOnly(text: String, result: InsertResult): InsertResult {
        service.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Voicio", text))
        return if (result == InsertResult.NoField) InsertResult.Copied else result
    }
}
