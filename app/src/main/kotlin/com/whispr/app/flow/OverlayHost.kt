package com.whispr.app.flow

import android.content.Context
import android.graphics.PixelFormat
import android.view.Gravity
import android.view.MotionEvent
import android.view.WindowManager
import android.widget.FrameLayout
import android.view.ViewConfiguration
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.ComposeView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.lifecycle.setViewTreeViewModelStoreOwner
import androidx.savedstate.SavedStateRegistry
import androidx.savedstate.SavedStateRegistryController
import androidx.savedstate.SavedStateRegistryOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner

/** Touch events on the grab area (the orb, or the bar's body), already separated from taps on its buttons. */
interface BubbleGestures {
    fun onDown()
    /** The finger moved past the slop. Return false to ignore movement for the rest of this touch. */
    fun onDragStart(): Boolean
    /** Finger travel in raw screen px since [onDragStart] (absolute, not per-event deltas, so nothing drifts). */
    fun onDrag(totalDx: Int, totalDy: Int)
    fun onUp(wasDrag: Boolean)
}

/**
 * Shows a Compose view in a system overlay window (above other apps, not focusable, so the
 * app underneath keeps its keyboard and caret). A service has no Activity, so this supplies the
 * lifecycle, saved-state and view-model owners Compose looks for in the view tree.
 */
class OverlayHost(
    private val context: Context,
    private val windowType: Int,
    private val gestures: BubbleGestures,
) {
    /** Where in the window a press grabs (orb, or the bar body between its buttons), in window px. Compose keeps it current. */
    val grab = android.graphics.Rect()

    /** System haptic (respects the user's "touch feedback" setting; needs no VIBRATE permission). */
    fun haptic(constant: Int) { view?.performHapticFeedback(constant) }

    private val windows = context.getSystemService(WindowManager::class.java)
    private var view: TouchLayer? = null
    private var params: WindowManager.LayoutParams? = null
    private val owner = Owner()

    val shown: Boolean get() = view != null

    /** Called when the content changes size (words arriving), so the owner can keep the window on screen. */
    var onResize: (() -> Unit)? = null

    /** The window's current size in px (0 until it has been laid out). */
    fun size(): Pair<Int, Int> = (view?.width ?: 0) to (view?.height ?: 0)

    fun show(x: Int, y: Int, content: @Composable () -> Unit) {
        if (view != null) return
        val lp = WindowManager.LayoutParams(
            WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.WRAP_CONTENT,
            windowType,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL or
                WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
            PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.TOP or Gravity.START
            this.x = x
            this.y = y
        }
        val compose = ComposeView(context).apply {
            setViewTreeLifecycleOwner(owner)
            setViewTreeViewModelStoreOwner(owner)
            setViewTreeSavedStateRegistryOwner(owner)
            setContent(content)
        }
        val v = TouchLayer(context).also {
            it.addView(compose)
            it.addOnLayoutChangeListener { _, l, t, r, b, ol, ot, or, ob -> if (r - l != or - ol || b - t != ob - ot) onResize?.invoke() }
        }
        v.setViewTreeLifecycleOwner(owner)
        v.setViewTreeViewModelStoreOwner(owner)
        v.setViewTreeSavedStateRegistryOwner(owner)
        owner.resume()
        windows.addView(v, lp)
        view = v
        params = lp
    }

    private val debug = (context.applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE) != 0

    /** [src] only labels the debug frame log (tag VoicioPlace): drag, snap, place. */
    fun moveTo(x: Int, y: Int, src: String = "place") {
        val v = view ?: return
        val lp = params ?: return
        if (lp.x == x && lp.y == y) return
        if (debug) android.util.Log.d("VoicioPlace", "t=${android.os.SystemClock.uptimeMillis()} src=$src x=$x y=$y dx=${x - lp.x} dy=${y - lp.y}")
        lp.x = x
        lp.y = y
        windows.updateViewLayout(v, lp)
    }

    fun position(): Pair<Int, Int> = (params?.x ?: 0) to (params?.y ?: 0)

    fun hide() {
        val v = view ?: return
        view = null
        params = null
        owner.pause()
        runCatching { windows.removeView(v) }
        (v.getChildAt(0) as? ComposeView)?.disposeComposition()
    }

    /**
     * Owns the touches that land on [grab]: press starts dictation, a drag moves the window, release
     * ends it. Touches elsewhere (Cancel, Done, Undo) go to Compose.
     */
    private inner class TouchLayer(context: Context) : FrameLayout(context) {
        private var grabbed = false
        private var dragging = false
        private var noDrag = false
        private var lastX = 0f
        private var lastY = 0f
        private var downX = 0f
        private var downY = 0f
        private val slop = ViewConfiguration.get(context).scaledTouchSlop * 2

        private fun onBubble(ev: MotionEvent) = grab.contains(ev.x.toInt(), ev.y.toInt())

        override fun onInterceptTouchEvent(ev: MotionEvent): Boolean {
            if (ev.actionMasked == MotionEvent.ACTION_DOWN && onBubble(ev)) {
                grabbed = true
                dragging = false
                noDrag = false
                downX = ev.rawX; downY = ev.rawY; lastX = ev.rawX; lastY = ev.rawY
                gestures.onDown()
                return true
            }
            return false
        }

        override fun onTouchEvent(ev: MotionEvent): Boolean {
            if (!grabbed) return super.onTouchEvent(ev)
            when (ev.actionMasked) {
                MotionEvent.ACTION_MOVE -> {
                    if (!dragging && !noDrag && (kotlin.math.abs(ev.rawX - downX) > slop || kotlin.math.abs(ev.rawY - downY) > slop)) {
                        if (gestures.onDragStart()) { dragging = true; lastX = ev.rawX; lastY = ev.rawY } else noDrag = true
                    }
                    if (dragging) gestures.onDrag(Math.round(ev.rawX - lastX), Math.round(ev.rawY - lastY))
                }
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                    grabbed = false
                    gestures.onUp(dragging)
                }
            }
            return true
        }
    }

    private class Owner : LifecycleOwner, SavedStateRegistryOwner, ViewModelStoreOwner {
        private val registry = LifecycleRegistry(this)
        private val saved = SavedStateRegistryController.create(this)
        override val viewModelStore = ViewModelStore()
        override val lifecycle: Lifecycle get() = registry
        override val savedStateRegistry: SavedStateRegistry get() = saved.savedStateRegistry

        init {
            saved.performAttach()
            saved.performRestore(null)
            registry.currentState = Lifecycle.State.CREATED
        }

        fun resume() { registry.currentState = Lifecycle.State.RESUMED }
        fun pause() { registry.currentState = Lifecycle.State.CREATED }
    }
}
