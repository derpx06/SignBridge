package com.whispr.app.flow

import android.accessibilityservice.AccessibilityService
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import com.whispr.app.data.HistoryStore
import android.view.HapticFeedbackConstants
import android.view.WindowManager
import kotlin.math.abs
import android.view.accessibility.AccessibilityEvent
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.unit.dp

/**
 * The Wispr-Flow-style surface: while a text field is focused it shows a small orb over the app;
 * tap it to talk hands-free (Done inserts) or hold it to talk and let go to insert. The
 * accessibility service is what lets us see the focused field and insert text into it.
 */
class FlowAccessibilityService : AccessibilityService() {
    private val main = Handler(Looper.getMainLooper())
    private lateinit var settings: FlowSettings
    private lateinit var inserter: TextInserter
    private lateinit var controller: DictationController
    private var overlay: OverlayHost? = null
    private var state by mutableStateOf(FlowState())
    private val recheck = Runnable { refreshBubble() }
    private var visible by mutableStateOf(false)
    private var pendingRemove: Runnable? = null
    private var snap: android.animation.ValueAnimator? = null
    private var graceHide: Runnable? = null
    private var placeQueued = false
    private var placeAnimate = false
    private var dragging = false
    private var pressed = false
    private var pendingPress: Runnable? = null
    private var dragX0 = 0
    private var dragY0 = 0
    private var popupAbove by mutableStateOf(true)
    private var animX = 0
    private var animY = 0

    /** The window currently has the bar's shape (reported by Compose; it lags [state] while the bar folds away). */
    private var barLayout = false
    private var dockRight by mutableStateOf(true)
    private var barWidth by mutableStateOf(320.dp)
    private var pressFromOrb = false
    private var pressAt = 0L
    private var lastPkg: String? = null

    override fun onServiceConnected() {
        settings = FlowSettings(this)
        inserter = TextInserter(this)
        controller = DictationController(this, settings, insert = { text -> insertStyled(text) }, onState = ::onFlowState)
        controller.onUndo = { inserter.undo(it) }
        controller.onRaw = { raw -> onRawWords(raw) }
        instance = this
        PauseTileService.refresh(this)
        refreshBubble()
    }

    private fun onFlowState(next: FlowState) {
        val old = state
        // Audio meter ticks must not recompose or re-place anything unless the level moved visibly.
        if (next.level != old.level && kotlin.math.abs(next.level - old.level) < 0.04f && next.copy(level = old.level) == old) return
        state = next
        overlay?.let { h ->
            val api30 = android.os.Build.VERSION.SDK_INT >= 30
            when {
                next.phase == Phase.Warming && old.phase != Phase.Warming -> h.haptic(if (api30) HapticFeedbackConstants.GESTURE_START else HapticFeedbackConstants.VIRTUAL_KEY)
                next.phase == Phase.Done && old.phase != Phase.Done && next.canUndo -> h.haptic(if (api30) HapticFeedbackConstants.CONFIRM else HapticFeedbackConstants.VIRTUAL_KEY)
                next.notice != null && next.notice != old.notice && next.notice.kind != NoticeKind.Info -> h.haptic(reject())
                next.nearLimit && !old.nearLimit -> h.haptic(HapticFeedbackConstants.CLOCK_TICK)
            }
        }
        if (next.wantsBar() != old.wantsBar() || next.phase != old.phase) scheduleRecheck(300)
    }

    private fun scheduleRecheck(ms: Long) { main.removeCallbacks(recheck); main.postDelayed(recheck, ms) }

    private fun reject() = if (android.os.Build.VERSION.SDK_INT >= 30) HapticFeedbackConstants.REJECT else HapticFeedbackConstants.LONG_PRESS

    /** Style for this dictation: a spoken directive, else the user's per-app choice, else the app's kind (mail formal, chat casual). */
    @Volatile private var directive: com.whispr.core.dictation.Style? = null

    private fun onRawWords(raw: String): String? {
        if (!settings.spokenCommands) return raw
        val parsed = com.whispr.core.dictation.Commands.parse(raw)
        when (parsed.action) {
            com.whispr.core.dictation.Commands.Action.Undo, com.whispr.core.dictation.Commands.Action.DeleteThat -> { main.post { controller.undo() }; return null }
            null -> Unit
            else -> return null // send / next / clear / stop need a keyboard: never type the command words
        }
        directive = parsed.style
        return parsed.text.ifBlank { null }
    }

    /** Text goes in only by paste / set-text on the focused field (TextInserter); the service never taps or swipes. */
    private fun insertStyled(text: String): InsertResult {
        val pkg = rootInActiveWindow?.packageName?.toString()
        val style = directive ?: pkg?.let { p -> settings.styleOverride(p)?.let { if (it == "None") null else com.whispr.core.dictation.Style.parse(it) } ?: com.whispr.core.dictation.Style.forApp(p) }
        directive = null
        val saved = com.whispr.app.data.SnippetStore.map(this)
        val whole = saved.entries.firstOrNull { com.whispr.core.dictation.Commands.norm(it.key) == com.whispr.core.dictation.Commands.norm(text) }?.value
        val styled = whole ?: (style?.apply(text) ?: text).let { com.whispr.core.dictation.Snippets.expand(it, saved) ?: it }
        // Never type into a view the app marked sensitive (Android 14+), even if it is not a password field.
        val field = inserter.focusedField()
        if (field != null && android.os.Build.VERSION.SDK_INT >= 34 && field.isAccessibilityDataSensitive) return InsertResult.Password
        return inserter.insert(styled).also { if (it is InsertResult.Inserted) HistoryStore.add(this, styled, foregroundApp(), settings.historyDays) }
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent) {
        if (event.packageName == packageName) return // our own bubble / app windows must not re-trigger placement
        when (event.eventType) {
            AccessibilityEvent.TYPE_VIEW_FOCUSED,
            AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED,
            AccessibilityEvent.TYPE_WINDOWS_CHANGED,
            -> {
                scheduleRecheck(120) // trailing debounce: focus and keyboard animations settle before we look
            }
        }
    }

    override fun onTrimMemory(level: Int) {
        super.onTrimMemory(level)
        if (level >= android.content.ComponentCallbacks2.TRIM_MEMORY_RUNNING_LOW) EngineHolder.trim()
    }

    override fun onInterrupt() {
        controller.cancel()
    }

    /** Shows the field while an editable field has focus (or always, if the user chose that). */
    private fun refreshBubble() = try { refreshBubbleUnsafe() } catch (e: Exception) { android.util.Log.e("Voicio", "bubble refresh failed", e) }

    private fun refreshBubbleUnsafe() {
        val pkg = rootInActiveWindow?.packageName?.toString()
        if (pkg == packageName) return // our own app (or the bubble's own window)
        val busy = state.wantsBar()
        // Another app: forget the last insert (it holds that field's earlier text).
        if (pkg != null && pkg != lastPkg) { if (lastPkg != null && !busy) controller.forget(); lastPkg = pkg }
        // The Voicio Keyboard is on screen (or the user chose keyboard only): one mic is enough.
        if (!busy && (com.whispr.app.ime.VoicioIme.showing || settings.inputMode == "keyboard")) { hideBubble(); return }
        val field = inserter.focusedField()
        // Never over password fields or views the app marked sensitive (Android 14+).
        val unsafe = field != null && (field.isPassword || (android.os.Build.VERSION.SDK_INT >= 34 && field.isAccessibilityDataSensitive))
        if (unsafe && !busy) { hideBubble(); return }
        val wanted = busy || settings.alwaysShowBubble || field != null
        if (wanted) {
            graceHide?.let { main.removeCallbacks(it); graceHide = null }
            if (!visible) showBubble() else place(animate = true) // keep clear of the field and keyboard
        } else if (graceHide == null && overlay != null) {
            // Focus often drops for a moment while the keyboard animates: only hide if it stays gone.
            graceHide = Runnable { graceHide = null; if (!state.wantsBar()) hideBubble() }.also { main.postDelayed(it, 300) }
        }
    }

    private fun showBubble() {
        pendingRemove?.let { main.removeCallbacks(it); pendingRemove = null }
        if (overlay != null) { visible = true; return } // it was on its way out: spring back in
        // Always the accessibility overlay: banking screens' HIDE_OVERLAY_WINDOWS cannot hide it, it never marks their
        // touches as obscured, and it sits above the keyboard (docs/research/bank-safe-accessibility.md).
        val type = WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY
        val host = OverlayHost(this, type, object : BubbleGestures {
            override fun onDown() {
                pressFromOrb = !barLayout
                pressAt = android.os.SystemClock.elapsedRealtime()
                // Start dictating only if the finger stays put a moment: a drag that starts at once must not fold the orb
                // into the bar (a big window jump) and back.
                if (pressFromOrb) {
                    pressed = false
                    pendingPress = Runnable { pendingPress = null; pressed = true; controller.press() }.also { main.postDelayed(it, PRESS_DELAY_MS) }
                }
            }

            override fun onDragStart(): Boolean {
                pendingPress?.let { main.removeCallbacks(it); pendingPress = null }
                if (pressFromOrb && pressed) {
                    // Holding to talk for a while: a thumb wobble must not throw the dictation away.
                    val held = android.os.SystemClock.elapsedRealtime() - pressAt
                    if (held >= com.whispr.core.dictation.Trigger.HOLD_TO_TALK_MS && state.wantsBar()) return false
                    controller.abandonPress()
                }
                snap?.cancel()
                dragging = true
                overlay?.position()?.let { dragX0 = it.first; dragY0 = it.second }
                return true
            }

            override fun onDrag(dx: Int, dy: Int) {
                val host = overlay ?: return
                // Raw-pixel travel from where the drag began; no re-layout, no placement, no animation while the finger is down.
                val u = usable(withIme = true)
                val (w, _) = winSize()
                val nx = if (pressFromOrb) (dragX0 + dx).coerceIn(u.left, maxOf(u.right - w, u.left)) else host.position().first // the bar slides up and down only
                val lift = if (barLayout && popupAbove) popupH() else 0 // window top = pill top - popup
                val top = (dragY0 + lift + dy).coerceIn(u.top, maxOf(u.bottom - pillH(), u.top))
                host.moveTo(nx, top - lift, "drag")
            }

            override fun onUp(wasDrag: Boolean) {
                dragging = false
                when {
                    wasDrag -> endDrag()
                    pressFromOrb -> {
                        pendingPress?.let { main.removeCallbacks(it); pendingPress = null; pressed = true; controller.press() }
                        controller.release()
                    }
                    else -> Unit // a tap on the bar body does nothing
                }
            }
        })
        host.onResize = { place(animate = false) }
        overlay = host
        visible = true
        barLayout = state.wantsBar()
        dockRight = settings.bubbleX.let { it < 0 || it == Int.MAX_VALUE }
        val r = compute()
        popupAbove = r.popupAbove
        val (x, y) = r.x to r.y
        host.show(x, y) {
            FlowField(
                state, visible = visible, dockRight = dockRight, barWidth = barWidth, popupAbove = popupAbove,
                onBarLayout = { bar ->
                    if (bar != barLayout) {
                        barLayout = bar
                        place(animate = false)
                    }
                },
                onGrab = { r -> overlay?.grab?.set(r) },
                onOrbClick = { controller.startHandsFree() },
                onCancel = {
                    if (state.phase == Phase.Idle || state.phase == Phase.Done) controller.dismiss()
                    else { overlay?.haptic(reject()); controller.cancel() }
                },
                onDone = { controller.done() },
                onRetry = { controller.retry() },
                onUndo = { controller.undo() },
            )
        }
    }

    private fun px(dp: Float) = (dp * resources.displayMetrics.density).toInt()

    /** Top of the keyboard in screen px, or null when no keyboard is showing. The keyboard is its own accessibility window. */
    private fun imeTop(): Int? = runCatching {
        val ime = windows.firstOrNull { it.type == android.view.accessibility.AccessibilityWindowInfo.TYPE_INPUT_METHOD } ?: return@runCatching null
        val rb = android.graphics.Rect(); ime.getBoundsInScreen(rb)
        rb.top.takeIf { rb.height() > 0 }
    }.getOrNull()

    /** Screen area inside the system bars and cutouts; with [withIme] also above the keyboard (only used to clamp a live drag). */
    private fun usable(withIme: Boolean = false): android.graphics.Rect {
        val r: android.graphics.Rect
        if (android.os.Build.VERSION.SDK_INT >= 30) {
            val m = windowManager().currentWindowMetrics
            r = android.graphics.Rect(m.bounds)
            val i = m.windowInsets.getInsets(android.view.WindowInsets.Type.systemBars() or android.view.WindowInsets.Type.displayCutout())
            r.left += i.left; r.top += i.top; r.right -= i.right; r.bottom -= i.bottom
        } else {
            r = resources.displayMetrics.let { android.graphics.Rect(0, 0, it.widthPixels, it.heightPixels) }
        }
        if (withIme) imeTop()?.let { if (it > r.top) r.bottom = minOf(r.bottom, it) }
        return r
    }

    private fun box(r: android.graphics.Rect) = Box(r.left, r.top, r.right, r.bottom)
    private fun orbPx() = px(OrbSize.value + 2 * ShadowPad.value)
    private fun pillH() = if (barLayout) px(PillHeight.value + 2 * ShadowPad.value) else orbPx()
    private fun popupH() = if (barLayout) px(PopupReserve.value) else 0

    /** Window size from known constants; only the orb's width is measured (it grows for the Undo pill). */
    private fun winSize(): Pair<Int, Int> {
        if (barLayout) return (px(barWidth.value) + 2 * px(ShadowPad.value)) to (popupH() + pillH())
        val (mw, mh) = overlay?.size() ?: (0 to 0)
        return (if (mh == orbPx() && mw > 0) mw else orbPx()) to orbPx()
    }

    /** The one placement path: saved edge + fraction + screen + keyboard in, window position out. */
    private fun compute(): Placement.Result {
        val (w, _) = winSize()
        return Placement.place(Placement.Input(
            usable = box(usable()), imeTop = imeTop(), w = w, pillH = pillH(), popupH = popupH(),
            bar = barLayout, dockRight = dockRight, yFrac = settings.bubbleFrac.takeIf { it >= 0f },
            pad = px(ShadowPad.value), margin = 0, restLift = px(96f), imeGap = px(16f),
        ))
    }

    /** Coalesces every request in a frame into one placement pass (Choreographer). */
    private fun place(animate: Boolean) {
        if (rotating) return
        placeAnimate = placeAnimate || animate
        if (placeQueued) return
        placeQueued = true
        android.view.Choreographer.getInstance().postFrameCallback { placeQueued = false; val a = placeAnimate; placeAnimate = false; placeNow(a) }
    }

    private fun placeNow(animate: Boolean) {
        val host = overlay ?: return
        if (dragging) return // the finger owns the position
        val width = ((usable().width() / resources.displayMetrics.density) - 2 * BAR_MARGIN_DP).coerceIn(200f, BAR_MAX_DP).dp
        if (width != barWidth) barWidth = width
        val r = compute()
        if (r.popupAbove != popupAbove) popupAbove = r.popupAbove
        val (cx, cy) = host.position()
        if (abs(r.x - cx) <= 2 && abs(r.y - cy) <= 2) return // already there: never touch the window
        val (x, y) = r.x to r.y
        if (!animate) { snap?.cancel(); host.moveTo(x, y); return }
        if (snap?.isRunning == true && x == animX && y == animY) return // already gliding there
        animX = x; animY = y
        snap?.cancel()
        snap = android.animation.ValueAnimator.ofFloat(0f, 1f).apply {
            duration = 200
            interpolator = android.view.animation.PathInterpolator(0.4f, 0f, 0.2f, 1f)
            addUpdateListener { v -> val t = v.animatedValue as Float; overlay?.moveTo((cx + (x - cx) * t).toInt(), (cy + (y - cy) * t).toInt(), "snap") }
            start()
        }
    }

    /** Finger lifted after a drag: save edge + height fraction, then glide to the edge. */
    private fun endDrag() {
        val host = overlay ?: return
        val (x, y) = host.position()
        val (w, _) = winSize()
        val u = box(usable())
        val pillTop = if (barLayout && popupAbove) y + popupH() else y
        settings.bubbleFrac = Placement.fracOf(u, pillH(), pillTop)
        if (pressFromOrb) {
            dockRight = Placement.dockRightFor(u, x + w / 2)
            settings.bubbleX = if (dockRight) Int.MAX_VALUE else 0
        }
        place(animate = true)
    }

    /** Rotation or a fold: the saved fraction maps onto the new screen; nothing is reset. */
    override fun onConfigurationChanged(newConfig: android.content.res.Configuration) {
        super.onConfigurationChanged(newConfig)
        // Insets and the keyboard window settle a moment after the config change: place once, after they have, not on stale numbers.
        rotating = true // ignore window-churn placements until the rotation has settled
        main.removeCallbacks(settlePlace)
        main.postDelayed(settlePlace, 700)
    }
    private var rotating = false
    private val settlePlace = Runnable { rotating = false; place(animate = false) }

    /** Spring out, then take the window away once the exit has played. */
    private fun hideBubble(now: Boolean = false) {
        if (now) { pendingRemove?.let { main.removeCallbacks(it) }; pendingRemove = null; snap?.cancel(); overlay?.hide(); overlay = null; visible = false; return }
        if (overlay == null || pendingRemove != null) return
        visible = false
        pendingRemove = Runnable { pendingRemove = null; overlay?.hide(); overlay = null }.also { main.postDelayed(it, 320) }
    }

    /** Name of the app the user is dictating into, for the history list. */
    private fun foregroundApp(): String = try {
        val pkg = rootInActiveWindow?.packageName?.toString().orEmpty()
        if (pkg.isEmpty()) "" else packageManager.getApplicationLabel(packageManager.getApplicationInfo(pkg, 0)).toString()
    } catch (e: Exception) { "" }

    private fun windowManager() = getSystemService(WindowManager::class.java)

    override fun onDestroy() {
        main.removeCallbacks(recheck)
        main.removeCallbacks(settlePlace)
        graceHide?.let { main.removeCallbacks(it) }
        hideBubble(now = true)
        if (::controller.isInitialized) controller.shutdown(releaseEngine = !com.whispr.app.ime.VoicioIme.showing) // the keyboard shares the model
        instance = null
        PauseTileService.refresh(this)
        super.onDestroy()
    }

    companion object {
        /** The running service, for the app's settings screens. Null when the user has it switched off. */
        @Volatile var instance: FlowAccessibilityService? = null
            private set

        private const val PRESS_DELAY_MS = 120L
        private const val BAR_MARGIN_DP = 12f
        private const val BAR_MAX_DP = 280f

        /**
         * Turns the accessibility service off for real (as if switched off in Settings), so payment apps stop
         * warning. Returns false when it was not on.
         */
        fun pause(): Boolean = instance?.let { it.disableSelf(); true } ?: false

        /**
         * Opens the Accessibility list with Voicio highlighted. (The per-service details page needs the
         * system-only OPEN_ACCESSIBILITY_DETAILS_SETTINGS permission, so apps must not launch it.)
         */
        fun turnBackOn(context: android.content.Context) {
            val plain = android.content.Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS).addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
            try { context.startActivity(turnBackOnIntent(context)) } catch (e: Exception) {
                runCatching { context.startActivity(plain) }
            }
        }

        fun turnBackOnIntent(context: android.content.Context): android.content.Intent {
            val component = android.content.ComponentName(context, FlowAccessibilityService::class.java).flattenToString()
            val args = android.os.Bundle().apply { putString(":settings:fragment_args_key", component) }
            return android.content.Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)
                .putExtra(":settings:fragment_args_key", component)
                .putExtra(":settings:show_fragment_args", args)
                .addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
        }
    }
}
