package com.whispr.app.ime

import android.Manifest
import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.inputmethodservice.InputMethodService
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.ToneGenerator
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.PersistableBundle
import android.os.SystemClock
import android.provider.Settings
import android.view.HapticFeedbackConstants
import android.view.KeyEvent
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.ViewCompositionStrategy
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
import com.whispr.app.R
import com.whispr.app.data.HistoryStore
import com.whispr.app.data.SnippetStore
import com.whispr.app.edit.LlmTextEditor
import com.whispr.app.edit.SmartEditModel
import com.whispr.app.flow.DictationController
import com.whispr.app.flow.EngineHolder
import com.whispr.app.flow.FlowAccessibilityService
import com.whispr.app.flow.FlowSettings
import com.whispr.app.flow.FlowState
import com.whispr.app.flow.InsertResult
import com.whispr.app.flow.Phase
import com.whispr.core.cloud.CloudProviders
import com.whispr.core.dictation.Commands
import com.whispr.core.dictation.FieldInsert
import com.whispr.core.dictation.Snippets
import com.whispr.core.dictation.Style
import com.whispr.core.dictation.Trigger
import com.whispr.core.edit.EditMode
import com.whispr.core.edit.EditRequest
import com.whispr.core.edit.EditResult
import com.whispr.core.edit.GuardedEditor
import com.whispr.core.edit.TextEditor
import com.whispr.core.edit.Tone
import com.whispr.core.edit.text
import java.util.concurrent.Executors

/**
 * Voicio Keyboard: a full keyboard with a voice-first bar. It needs no accessibility permission, so banking
 * apps have nothing to warn about. Dictation runs through the same [DictationController] as the bubble
 * (same on-device recogniser, clean-up, dictionary and optional cloud), and text goes in with commitText.
 *
 * Threads: audio, recognition and rewrites run on background threads; every InputConnection call happens
 * on the main thread.
 */
class VoicioIme : InputMethodService(), LifecycleOwner, SavedStateRegistryOwner, ViewModelStoreOwner, ImeActions {
    private val registry = LifecycleRegistry(this)
    private val saved = SavedStateRegistryController.create(this)
    override val lifecycle: Lifecycle get() = registry
    override val savedStateRegistry: SavedStateRegistry get() = saved.savedStateRegistry
    override val viewModelStore = ViewModelStore()

    private val main = Handler(Looper.getMainLooper())
    private val work = Executors.newSingleThreadExecutor { Thread(it, "voicio-ime") }
    private lateinit var settings: FlowSettings
    private lateinit var controller: DictationController
    private lateinit var clips: Clips
    private val lexicon = Lexicon()
    private var view: View? = null

    var ui by mutableStateOf(ImeUi())
        private set

    private var selStart = 0
    private var selEnd = 0
    private var pkg: String? = null
    private var lastSpaceAt = 0L
    private var lastShiftAt = 0L
    private var micDownAt = 0L
    private var lastTapUpAt = 0L
    private var ignoreUp = false
    private var swipeAnchor = -1
    private var swipeLen = 0
    private var lastDictation: String? = null
    @Volatile private var pendingStyle: Style? = null
    @Volatile private var pendingSend = false
    private var focus: AudioFocusRequest? = null
    private var tones: ToneGenerator? = null

    private val clipListener = ClipboardManager.OnPrimaryClipChangedListener { readClipboard() }

    override fun onCreate() {
        super.onCreate()
        saved.performAttach()
        saved.performRestore(null)
        registry.currentState = Lifecycle.State.CREATED
        settings = FlowSettings(this)
        clips = Clips(settings.pinnedClips)
        controller = DictationController(this, settings, insert = ::commitDictation, onState = ::onFlow,
            modeOverride = Trigger.Mode.HoldOrTap, maxHoldMs = 30 * 60_000L)
        controller.onUndo = { r -> undoInsert(r) }
        controller.onRaw = { raw -> onRawWords(raw) }
        getSystemService(ClipboardManager::class.java).addPrimaryClipChangedListener(clipListener)
    }

    override fun onCreateInputView(): View {
        window.window?.decorView?.let {
            it.setViewTreeLifecycleOwner(this)
            it.setViewTreeSavedStateRegistryOwner(this)
            it.setViewTreeViewModelStoreOwner(this)
        }
        return ComposeView(this).apply {
            setViewTreeLifecycleOwner(this@VoicioIme)
            setViewTreeSavedStateRegistryOwner(this@VoicioIme)
            setViewTreeViewModelStoreOwner(this@VoicioIme)
            setViewCompositionStrategy(ViewCompositionStrategy.DisposeOnDetachedFromWindowOrReleasedFromPool)
            setContent { KeyboardView(ui, this@VoicioIme) }
        }.also { view = it }
    }

    override fun onEvaluateFullscreenMode() = false

    private fun landscape() = resources.configuration.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE

    /** Rotation: shorter keys and no number row in landscape so the app keeps room above the keyboard. */
    override fun onConfigurationChanged(newConfig: android.content.res.Configuration) {
        super.onConfigurationChanged(newConfig)
        val l = newConfig.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
        ui = ui.copy(landscape = l, height = settings.keyboardHeight * if (l) LANDSCAPE_HEIGHT else 1f, numberRow = settings.numberRow && !l)
    }

    private val floatRect = android.graphics.Rect()

    /** Floating: the window is a transparent full-height canvas; only the card takes touches and the app is not resized. */
    override fun onComputeInsets(outInsets: Insets) {
        super.onComputeInsets(outInsets)
        if (!ui.floating || floatRect.isEmpty) return
        val h = window.window?.decorView?.height ?: return
        outInsets.contentTopInsets = h
        outInsets.visibleTopInsets = h
        outInsets.touchableInsets = Insets.TOUCHABLE_INSETS_REGION
        outInsets.touchableRegion.set(floatRect)
    }

    override fun floating(on: Boolean) {
        settings.floating = on
        floatRect.setEmpty()
        ui = ui.copy(floating = on, layer = if (ui.layer == Layer.Menu) Layer.Letters else ui.layer)
    }

    override fun floatMoved(x: Int, y: Int) {
        settings.floatX = x
        settings.floatY = y
        ui = ui.copy(floatX = x, floatY = y)
    }

    override fun floatBounds(left: Int, top: Int, right: Int, bottom: Int) {
        if (floatRect.left == left && floatRect.top == top && floatRect.right == right && floatRect.bottom == bottom) return
        floatRect.set(left, top, right, bottom)
        view?.requestLayout() // recompute the touchable region
    }

    /** A voice keyboard stays useful with a hardware keyboard attached (Bluetooth, Chromebook): always show it. */
    override fun onEvaluateInputViewShown(): Boolean {
        super.onEvaluateInputViewShown()
        return true
    }

    override fun onStartInputView(info: EditorInfo, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        showing = true
        registry.currentState = Lifecycle.State.RESUMED
        if (!restarting && ui.flow.phase != Phase.Idle) controller.cancel() // a new field mid-dictation
        pkg = info.packageName
        selStart = info.initialSelStart.coerceAtLeast(0)
        selEnd = info.initialSelEnd.coerceAtLeast(0)
        val field = fieldKind(info.inputType)
        val incognito = info.imeOptions and EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING != 0
        val (style, auto, why) = styleFor(info.packageName)
        val cloud = if (field.voice && settings.cloudMode != "off" && settings.hasCloudConsent(settings.cloudProvider)) CloudProviders.byId(settings.cloudProvider)?.name else null
        if (field.voice && !incognito) {
            HistoryStore.ensure(this, settings.historyDays)
            lexicon.rebuild(settings.dictionaryText.lines().map { l -> l.substringAfter("=>").trim() }, HistoryStore.items.take(200).map { it.text })
        }
        controller.gain = if (settings.whisperMode) WHISPER_GAIN else 1f
        controller.autoStopMs = settings.autoStopMs
        controller.continuous = false
        ui = ImeUi(
            flow = if (restarting) ui.flow else FlowState(),
            field = field,
            enter = enterKind(info),
            layer = Layer.Letters,
            style = style, styleAuto = auto, styleWhy = why,
            cloud = cloud,
            privacy = if (cloud != null && settings.cloudMode == "always") "Cloud · $cloud" else getString(R.string.ime_on_device),
            incognito = incognito,
            micGranted = checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED,
            oneHanded = settings.oneHanded,
            landscape = landscape(),
            floating = settings.floating,
            floatX = settings.floatX, floatY = settings.floatY,
            numberRow = settings.numberRow && !landscape(),
            dark = when (settings.keyboardTheme) { "paper" -> false; "system" -> (resources.configuration.uiMode and android.content.res.Configuration.UI_MODE_NIGHT_MASK) == android.content.res.Configuration.UI_MODE_NIGHT_YES; else -> true },
            height = settings.keyboardHeight * if (landscape()) LANDSCAPE_HEIGHT else 1f,
            globe = shouldOfferSwitchingToNextInputMethod(),
            whisper = settings.whisperMode,
            autoStopMs = settings.autoStopMs,
            rewriteModel = SmartEditModel.installed(this),
            recentEmoji = settings.recentEmoji,
            history = if (field.voice && !incognito && settings.historyDays > 0) HistoryStore.items.take(20).map { it.text } else emptyList(),
        )
        refreshClips()
        afterEdit()
        // Load the model now so the first words come back fast.
        if (field.voice && ui.micGranted) controller.warm()
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        super.onFinishInputView(finishingInput)
        showing = false
        if (ui.flow.phase != Phase.Idle) controller.cancel() // never keep the mic open for a hidden keyboard
        controller.forget()
        lastDictation = null
        abandonFocus()
        // Keep the shared model loaded only while the bubble may need it.
        if (FlowAccessibilityService.instance == null) EngineHolder.trim()
        registry.currentState = Lifecycle.State.STARTED
    }

    override fun onUpdateSelection(oldSelStart: Int, oldSelEnd: Int, newSelStart: Int, newSelEnd: Int, candidatesStart: Int, candidatesEnd: Int) {
        super.onUpdateSelection(oldSelStart, oldSelEnd, newSelStart, newSelEnd, candidatesStart, candidatesEnd)
        selStart = newSelStart
        selEnd = newSelEnd
        if (swipeAnchor < 0) afterEdit()
    }

    override fun onDestroy() {
        showing = false
        getSystemService(ClipboardManager::class.java).removePrimaryClipChangedListener(clipListener)
        controller.shutdown(releaseEngine = FlowAccessibilityService.instance == null)
        work.shutdownNow()
        abandonFocus()
        tones?.release()
        registry.currentState = Lifecycle.State.DESTROYED
        viewModelStore.clear()
        view = null
        super.onDestroy()
    }

    // ------------------------------------------------------------ dictation

    private fun onFlow(next: FlowState) {
        val old = ui.flow
        val wasOn = old.phase == Phase.Warming || old.phase == Phase.Listening
        val isOn = next.phase == Phase.Warming || next.phase == Phase.Listening
        if (isOn && !wasOn) { requestFocus(); if (settings.voiceSounds) tone(ToneGenerator.TONE_PROP_ACK); haptic(HapticFeedbackConstants.VIRTUAL_KEY) }
        if (wasOn && !isOn) { abandonFocus(); if (settings.voiceSounds) tone(ToneGenerator.TONE_PROP_PROMPT) }
        if (next.phase == Phase.Idle && ui.continuous) controller.continuous = false
        // Continuous mode ends when you say "stop listening".
        if (ui.continuous && next.phase == Phase.Listening && Commands.norm(next.live).endsWith("stop listening")) main.post { controller.done() }
        ui = ui.copy(flow = next, continuous = ui.continuous && next.phase != Phase.Idle && next.phase != Phase.Done)
    }

    /** Background thread: commands, snippets and the style directive, from the raw words. */
    private fun onRawWords(raw: String): String? {
        if (!settings.spokenCommands) return raw
        if (Commands.norm(raw) in SnippetStore.map(this).keys.map(Commands::norm)) return raw // a snippet trigger, expanded at commit
        val parsed = Commands.parse(raw)
        parsed.action?.let { a -> main.post { runCommand(a) }; return null }
        pendingStyle = parsed.style
        pendingSend = parsed.sendAfter
        return parsed.text.ifBlank { null }
    }

    private fun runCommand(a: Commands.Action) {
        val ic = currentInputConnection ?: return
        when (a) {
            Commands.Action.Undo, Commands.Action.DeleteThat -> controller.undo()
            Commands.Action.DeleteLastWord -> deleteWord()
            Commands.Action.Clear -> { ic.performContextMenuAction(android.R.id.selectAll); ic.commitText("", 1) }
            Commands.Action.Send -> enter()
            Commands.Action.Next -> ic.performEditorAction(EditorInfo.IME_ACTION_NEXT)
            Commands.Action.Stop -> Unit
        }
    }

    /** Main thread: style, spacing, commit; on failure the text goes to the clipboard with a Copy chip. */
    private fun commitDictation(text: String): InsertResult {
        if (!ui.field.voice) return InsertResult.Password
        val style = pendingStyle ?: styleFor(pkg).let { (name, _, _) -> Style.parse(name) }
        pendingStyle = null
        // Snippets are expanded after clean-up so the saved text goes in exactly as saved.
        val saved = SnippetStore.map(this)
        val whole = saved.entries.firstOrNull { Commands.norm(it.key) == Commands.norm(text) }?.value
        val styled = whole ?: (style?.apply(text) ?: text).let { Snippets.expand(it, saved) ?: it }
        val ic = currentInputConnection ?: return copyFallback(styled)
        val before = ic.getTextBeforeCursor(64, 0)?.toString().orEmpty()
        val after = ic.getTextAfterCursor(1, 0)?.toString().orEmpty()
        val selected = !ic.getSelectedText(0).isNullOrEmpty()
        val piece = if (selected) FieldInsert.Result(styled, styled.length, styled) else FieldInsert.merge(before + after, null, false, before.length, before.length, styled)
        if (!ic.commitText(piece.inserted, 1)) return copyFallback(styled)
        lastDictation = piece.inserted
        if (!ui.incognito) {
            HistoryStore.add(this, styled, appLabel(), settings.historyDays)
            ui = ui.copy(history = if (settings.historyDays > 0) HistoryStore.items.take(20).map { it.text } else emptyList())
        }
        ui = ui.copy(newWords = if (ui.incognito) emptyList() else newWordsIn(styled), copied = null)
        if (pendingSend) { pendingSend = false; main.post { enter() } }
        return InsertResult.Inserted(piece.inserted.length, "", piece.inserted, pastedAt = 0)
    }

    private fun copyFallback(text: String): InsertResult {
        getSystemService(ClipboardManager::class.java).setPrimaryClip(sensitive(text))
        ui = ui.copy(copied = text)
        return InsertResult.Copied
    }

    private fun undoInsert(r: InsertResult.Inserted) {
        val ic = currentInputConnection ?: return
        if (ic.getTextBeforeCursor(r.inserted.length, 0)?.toString() == r.inserted) ic.deleteSurroundingText(r.inserted.length, 0)
        lastDictation = null
        ui = ui.copy(newWords = emptyList())
    }

    /** Capitalised or long words of the last dictation that are not yet in the dictionary ("+ Kubernetes"). */
    private fun newWordsIn(text: String): List<String> {
        val known = settings.dictionaryText.lowercase()
        return text.split(Regex("[^\\p{L}\\p{N}'-]+")).filter { it.length >= 4 && (it.first().isUpperCase() || it.length >= 9) && it.lowercase() !in known }
            .drop(1).distinct().take(2) // the first word is capitalised anyway
    }

    // ------------------------------------------------------------ mic

    override fun micDown() {
        if (!ui.field.voice) return
        if (!ui.micGranted) { openApp(); ignoreUp = true; return }
        val now = SystemClock.uptimeMillis()
        val on = ui.flow.phase == Phase.Warming || ui.flow.phase == Phase.Listening
        if (on && now - lastTapUpAt < DOUBLE_TAP_MS && !ui.continuous) { // double-tap: continuous
            controller.continuous = true
            ui = ui.copy(continuous = true)
            ignoreUp = true
            haptic(HapticFeedbackConstants.LONG_PRESS)
            return
        }
        micDownAt = now
        controller.press()
    }

    override fun micUp() {
        if (ignoreUp) { ignoreUp = false; return }
        controller.release()
        val now = SystemClock.uptimeMillis()
        lastTapUpAt = if (now - micDownAt < Trigger.HOLD_TO_TALK_MS) now else 0L
    }

    override fun done() = controller.done()
    override fun undo() = controller.undo()
    override fun retry() = controller.retry()
    override fun cancel() = if (ui.flow.phase == Phase.Idle || ui.flow.phase == Phase.Done) controller.dismiss() else controller.cancel()

    // ------------------------------------------------------------ keys

    override fun text(s: String) {
        val ic = currentInputConnection ?: return
        val out = if (ui.shift != Shift.Off && s.length == 1) s.uppercase() else s
        ic.commitText(out, 1)
        if (ui.shift == Shift.Once) ui = ui.copy(shift = Shift.Off)
        if (ui.layer == Layer.Symbols && s == "'") ui = ui.copy(layer = Layer.Letters)
    }

    override fun backspace() {
        val ic = currentInputConnection ?: return
        if (selEnd > selStart) ic.commitText("", 1) else sendDownUpKeyEvents(KeyEvent.KEYCODE_DEL)
    }

    override fun deleteWord() {
        val ic = currentInputConnection ?: return
        if (selEnd > selStart) { ic.commitText("", 1); return }
        val before = ic.getTextBeforeCursor(64, 0) ?: return
        val n = Lexicon.wordsBack(before, 1)
        if (n > 0) ic.deleteSurroundingText(n, 0)
    }

    override fun selectWordsBack(n: Int) {
        val ic = currentInputConnection ?: return
        if (swipeAnchor < 0) swipeAnchor = selEnd
        val before = ic.getTextBeforeCursor(400, 0) ?: return
        swipeLen = if (n == 0) 0 else Lexicon.wordsBack(before.subSequence(0, (before.length - (selEnd - swipeAnchor).coerceAtLeast(0)).coerceAtLeast(0)), n)
        ic.setSelection(swipeAnchor - swipeLen, swipeAnchor)
    }

    override fun finishSelect(delete: Boolean) {
        val ic = currentInputConnection
        if (ic != null && swipeAnchor >= 0) { if (delete && swipeLen > 0) ic.commitText("", 1) else ic.setSelection(swipeAnchor, swipeAnchor) }
        swipeAnchor = -1
        swipeLen = 0
        afterEdit()
    }

    override fun cursor(steps: Int) {
        val code = if (steps < 0) KeyEvent.KEYCODE_DPAD_LEFT else KeyEvent.KEYCODE_DPAD_RIGHT
        repeat(kotlin.math.abs(steps)) { sendDownUpKeyEvents(code) }
    }

    override fun space() {
        val ic = currentInputConnection ?: return
        val now = SystemClock.uptimeMillis()
        val before = ic.getTextBeforeCursor(2, 0)?.toString().orEmpty()
        // Double space after a word: ". "
        if (ui.field == FieldKind.Text && now - lastSpaceAt < 450 && before.length == 2 && before[1] == ' ' && before[0].isLetterOrDigit()) {
            ic.deleteSurroundingText(1, 0)
            ic.commitText(". ", 1)
            lastSpaceAt = 0
        } else {
            ic.commitText(" ", 1)
            lastSpaceAt = now
        }
    }

    override fun enter() {
        sendKeyChar('\n') // the field's action (Send, Search, Next) when it has one, else a new line
    }

    override fun shift() {
        val now = SystemClock.uptimeMillis()
        ui = ui.copy(shift = when {
            ui.shift == Shift.Lock -> Shift.Off
            ui.shift == Shift.Once && now - lastShiftAt < DOUBLE_TAP_MS -> Shift.Lock
            ui.shift == Shift.Once -> Shift.Off
            else -> Shift.Once
        })
        lastShiftAt = now
    }

    override fun layer(l: Layer) {
        if (!ui.field.voice && (l == Layer.Clipboard || l == Layer.History || l == Layer.Menu)) return // never in password fields
        if (l == Layer.Clipboard) refreshClips()
        ui = ui.copy(layer = l)
    }

    override fun globe() { if (!switchToNextInputMethod(false)) picker() }
    override fun picker() { getSystemService(InputMethodManager::class.java).showInputMethodPicker() }

    override fun suggestion(word: String) {
        val ic = currentInputConnection ?: return
        val typed = Lexicon.currentWord(ic.getTextBeforeCursor(48, 0) ?: "")
        ic.deleteSurroundingText(typed.length, 0)
        ic.commitText("$word ", 1)
    }

    override fun paste(clip: Clip) {
        currentInputConnection?.commitText(clip.text, 1)
        ui = ui.copy(recentClip = null, layer = if (ui.layer == Layer.Clipboard) Layer.Letters else ui.layer)
    }

    override fun pin(clip: Clip) {
        settings.pinnedClips = clips.togglePin(clip)
        refreshClips()
    }

    override fun emoji(e: String) {
        currentInputConnection?.commitText(e, 1)
        settings.recentEmoji = (listOf(e) + settings.recentEmoji.filter { it != e })
        ui = ui.copy(recentEmoji = settings.recentEmoji)
    }

    override fun nextStyle() {
        val p = pkg ?: return
        val order = listOf("None", "Casual", "Formal", "List", "Email")
        val current = settings.styleOverride(p) ?: (Style.forApp(p)?.name ?: "None")
        settings.setStyleOverride(p, order[(order.indexOf(current) + 1) % order.size])
        val (style, auto, why) = styleFor(p)
        ui = ui.copy(style = style, styleAuto = auto, styleWhy = why)
    }

    override fun oneHanded(side: Int) {
        settings.oneHanded = side
        ui = ui.copy(oneHanded = side, layer = if (ui.layer == Layer.Menu) Layer.Letters else ui.layer)
    }

    override fun copyAgain() {
        ui.copied?.let { getSystemService(ClipboardManager::class.java).setPrimaryClip(sensitive(it)) }
        ui = ui.copy(copied = null)
    }

    override fun openApp() {
        packageManager.getLaunchIntentForPackage(packageName)?.let { runCatching { startActivity(it.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) } }
    }

    override fun openMenu() { if (ui.field.voice) ui = ui.copy(layer = Layer.Menu) }

    override fun toggleWhisper() {
        settings.whisperMode = !settings.whisperMode
        controller.gain = if (settings.whisperMode) WHISPER_GAIN else 1f
        ui = ui.copy(whisper = settings.whisperMode)
    }

    override fun cycleAutoStop() {
        settings.autoStopMs = when (settings.autoStopMs) { 0L -> 1500L; 1500L -> 3000L; else -> 0L }
        controller.autoStopMs = settings.autoStopMs
        ui = ui.copy(autoStopMs = settings.autoStopMs)
    }

    override fun theme() {
        settings.keyboardTheme = if (ui.dark) "paper" else "ink"
        ui = ui.copy(dark = !ui.dark)
    }

    override fun reinsert(text: String) {
        if (!ui.field.voice) return
        val ic = currentInputConnection ?: return
        val before = ic.getTextBeforeCursor(64, 0)?.toString().orEmpty()
        ic.commitText(FieldInsert.merge(before, null, false, before.length, before.length, text).inserted, 1)
        ui = ui.copy(layer = Layer.Letters)
    }

    override fun addWord(word: String) {
        val lines = settings.dictionaryText.lines().filter { it.isNotBlank() }
        if (lines.none { it.substringBefore("=>").trim().equals(word, true) }) settings.dictionaryText = (lines + "${word.lowercase()} => $word").joinToString("\n")
        ui = ui.copy(newWords = ui.newWords - word)
        haptic(HapticFeedbackConstants.CONFIRM.takeIf { Build.VERSION.SDK_INT >= 30 } ?: HapticFeedbackConstants.VIRTUAL_KEY)
    }

    /** Rewrites the selection (or the last dictation, if it is still right before the caret) on this phone. */
    override fun rewrite(kind: Rewrite) {
        val ic = currentInputConnection ?: return
        val selected = ic.getSelectedText(0)?.toString().orEmpty()
        val last = lastDictation?.takeIf { ic.getTextBeforeCursor(it.length, 0)?.toString() == it }
        val source = selected.ifEmpty { last?.trim().orEmpty() }
        if (source.isEmpty()) { controller.notify(getString(R.string.ime_nothing_to_rewrite)); return }
        ui = ui.copy(rewriting = true)
        val context = applicationContext
        work.execute {
            val out = try {
                when (kind) {
                    Rewrite.List -> EditResult.Edited(Style.List.apply(source), false)
                    Rewrite.Email -> EditResult.Edited(Style.Email.apply(source), false)
                    else -> {
                        val (mode, tone) = when (kind) {
                            Rewrite.Formal -> EditMode.Rewrite to Tone.Formal
                            Rewrite.Casual -> EditMode.Rewrite to Tone.Casual
                            Rewrite.Shorter -> EditMode.Rewrite to Tone.Concise
                            else -> EditMode.Proofread to null
                        }
                        GuardedEditor(LlmTextEditor(context), TextEditor { EditResult.Edited(it.text, false) }).edit(EditRequest(source, mode, tone))
                    }
                }
            } catch (e: Throwable) { EditResult.Kept(source, "") }
            main.post {
                ui = ui.copy(rewriting = false, layer = Layer.Letters)
                val conn = currentInputConnection ?: return@post
                if (out is EditResult.Kept || out.text == source) { controller.notify(getString(R.string.ime_rewrite_kept)); return@post }
                if (selected.isNotEmpty()) conn.commitText(out.text, 1)
                else if (last != null && conn.getTextBeforeCursor(last.length, 0)?.toString() == last) {
                    val lead = last.takeWhile { it == ' ' }
                    conn.deleteSurroundingText(last.length, 0)
                    conn.commitText(lead + out.text, 1)
                    lastDictation = lead + out.text
                }
            }
        }
    }

    // ------------------------------------------------------------ feedback

    override fun feedback(kind: Feedback) {
        val strength = settings.keyHaptics
        if (strength > 0) haptic(when (kind) {
            Feedback.Long -> HapticFeedbackConstants.LONG_PRESS
            Feedback.Tick -> HapticFeedbackConstants.CLOCK_TICK
            else -> when (strength) { 1 -> HapticFeedbackConstants.CLOCK_TICK; 2 -> HapticFeedbackConstants.KEYBOARD_TAP; else -> HapticFeedbackConstants.VIRTUAL_KEY }
        })
        val volume = settings.keySound
        if (volume > 0f && kind != Feedback.Tick && kind != Feedback.Long) {
            getSystemService(AudioManager::class.java).playSoundEffect(when (kind) {
                Feedback.Delete -> AudioManager.FX_KEYPRESS_DELETE
                Feedback.Space -> AudioManager.FX_KEYPRESS_SPACEBAR
                Feedback.Enter -> AudioManager.FX_KEYPRESS_RETURN
                else -> AudioManager.FX_KEYPRESS_STANDARD
            }, volume)
        }
    }

    private fun haptic(constant: Int) { view?.performHapticFeedback(constant) }

    private fun tone(t: Int) {
        val g = tones ?: runCatching { ToneGenerator(AudioManager.STREAM_SYSTEM, 60) }.getOrNull()?.also { tones = it } ?: return
        g.startTone(t, 120)
    }

    /** Pause music while listening, if the user asked for it. */
    private fun requestFocus() {
        if (!settings.pauseMedia) return
        val req = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
            .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ASSISTANT).setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
            .build()
        getSystemService(AudioManager::class.java).requestAudioFocus(req)
        focus = req
    }

    private fun abandonFocus() {
        focus?.let { getSystemService(AudioManager::class.java).abandonAudioFocusRequest(it) }
        focus = null
    }

    // ------------------------------------------------------------ helpers

    /** Called after the text or caret moved: auto-capitals and suggestions. */
    private fun afterEdit() {
        val ic = currentInputConnection ?: return
        val info = currentInputEditorInfo ?: return
        if (ui.field == FieldKind.Number || ui.field == FieldKind.Phone) return
        val caps = ui.layer == Layer.Letters && ui.shift != Shift.Lock && ic.getCursorCapsMode(info.inputType) != 0
        val before = ic.getTextBeforeCursor(48, 0) ?: ""
        val suggestions = if (!ui.field.voice || selEnd > selStart) emptyList() else lexicon.suggest(Lexicon.currentWord(before), 2)
        val shift = if (ui.shift == Shift.Lock) Shift.Lock else if (caps) Shift.Once else if (ui.shift == Shift.Once && before.isNotEmpty() && !caps) Shift.Off else ui.shift
        if (shift != ui.shift || suggestions != ui.suggestions) ui = ui.copy(shift = shift, suggestions = suggestions, newWords = if (suggestions.isEmpty()) ui.newWords else emptyList())
    }

    private fun readClipboard() {
        val clip = runCatching { getSystemService(ClipboardManager::class.java).primaryClip }.getOrNull() ?: return
        val sensitiveClip = Build.VERSION.SDK_INT >= 33 && clip.description.extras?.getBoolean(ClipDescription.EXTRA_IS_SENSITIVE) == true
        if (sensitiveClip || clip.description.label == "Voicio") return // passwords and our own fallback copies are never kept
        val text = clip.getItemAt(0)?.coerceToText(this)?.toString() ?: return
        clips.add(text.take(2000), System.currentTimeMillis())
        refreshClips()
    }

    private fun refreshClips() {
        val now = System.currentTimeMillis()
        ui = if (!ui.field.voice || ui.incognito) ui.copy(clips = emptyList(), recentClip = null)
        else ui.copy(clips = clips.visible(now), recentClip = clips.recent(now))
    }

    /** The chip: (label, chosen by the app's kind?, why). */
    private fun styleFor(p: String?): Triple<String, Boolean, String> {
        val override = p?.let { settings.styleOverride(it) }
        val app = appLabel(p)
        if (override != null) return Triple(Style.parse(override)?.name ?: getString(R.string.ime_style_auto), false, "$app · your choice")
        val auto = Style.forApp(p)
        return when (auto) {
            Style.Formal -> Triple(auto.name, true, "$app · mail apps are formal")
            Style.Casual -> Triple(auto.name, true, "$app · chat apps are casual")
            else -> Triple(getString(R.string.ime_style_auto), true, "$app · written as said")
        }
    }

    private fun appLabel(p: String? = pkg): String = try {
        if (p.isNullOrEmpty()) "" else packageManager.getApplicationLabel(packageManager.getApplicationInfo(p, 0)).toString()
    } catch (e: Exception) { "" }

    private fun enterKind(info: EditorInfo): EnterKind {
        if (info.imeOptions and EditorInfo.IME_FLAG_NO_ENTER_ACTION != 0) return EnterKind.Return
        return when (info.imeOptions and EditorInfo.IME_MASK_ACTION) {
            EditorInfo.IME_ACTION_SEND -> EnterKind.Send
            EditorInfo.IME_ACTION_SEARCH -> EnterKind.Search
            EditorInfo.IME_ACTION_GO -> EnterKind.Go
            EditorInfo.IME_ACTION_NEXT -> EnterKind.Next
            EditorInfo.IME_ACTION_DONE -> EnterKind.Done
            else -> EnterKind.Return
        }
    }

    private fun sensitive(text: String) = ClipData.newPlainText("Voicio", text).apply {
        description.extras = PersistableBundle().apply { putBoolean("android.content.extra.IS_SENSITIVE", true) }
    }

    companion object {
        private const val DOUBLE_TAP_MS = 300L
        private const val WHISPER_GAIN = 3f
        private const val LANDSCAPE_HEIGHT = 0.62f

        /** True while the Voicio Keyboard is on screen (the bubble hides then). */
        @Volatile var showing = false
            private set

        private fun component(context: Context) = ComponentName(context, VoicioIme::class.java)

        /** The user turned the Voicio Keyboard on in Settings › Keyboards. */
        fun isEnabled(context: Context): Boolean =
            context.getSystemService(InputMethodManager::class.java).enabledInputMethodList.any { it.component == component(context) }

        /** The Voicio Keyboard is the current keyboard. */
        fun isSelected(context: Context): Boolean = runCatching {
            Settings.Secure.getString(context.contentResolver, Settings.Secure.DEFAULT_INPUT_METHOD)?.let(ComponentName::unflattenFromString) == component(context)
        }.getOrDefault(false)

        /** Opens Settings › On-screen keyboards, where the user switches the Voicio Keyboard on. */
        fun openEnableSettings(context: Context) {
            runCatching { context.startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
        }

        /** Shows the system keyboard picker (works while the app has window focus). */
        fun showPicker(context: Context) {
            context.getSystemService(InputMethodManager::class.java).showInputMethodPicker()
        }
    }
}
