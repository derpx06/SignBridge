package com.whispr.app.ui

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.draw.scale
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.whispr.app.R
import com.whispr.app.engine.AudioCapture
import com.whispr.app.flow.FlowAccessibilityService
import com.whispr.app.flow.FlowSettings
import com.whispr.app.flow.ModelDownloader
import com.whispr.app.flow.ModelStore
import com.whispr.app.flow.Preflight
import com.whispr.app.ime.VoicioIme
import com.whispr.core.cloud.CloudProviders
import kotlinx.coroutines.delay

private const val WELCOME = 1
private const val DICTATE = 2
private const val MIC = 3
private const val TURN_ON = 4
private const val MODEL = 5
private const val TRY = 6
private const val STEPS = 6

/**
 * First run (PhoneIntroA/B artboards): welcome, how you'll dictate (keyboard, bubble or both), microphone,
 * the switches Android needs, the model, and a first dictation. Every step scrolls so large text never clips.
 */
@Composable
fun Onboarding(settings: FlowSettings, onDone: () -> Unit) {
    StatusBarIcons(dark = true)
    var step by rememberSaveable { mutableIntStateOf(WELCOME) }
    // First run starts with both chosen (the artboard); a replay starts from what the user picked before.
    var keyboard by rememberSaveable { mutableStateOf(!settings.onboarded || settings.inputMode != "bubble") }
    var bubble by rememberSaveable { mutableStateOf(!settings.onboarded || settings.inputMode != "keyboard") }
    var cloud by rememberSaveable { mutableStateOf(false) }
    fun back() { if (step > WELCOME) step-- }
    BackHandler(enabled = step > WELCOME || cloud) { if (cloud) cloud = false else back() }
    val finish = { settings.onboarded = true; onDone() }
    if (cloud) { CloudScreen(settings, onBack = { cloud = false }); return }
    AnimatedContent(
        targetState = step,
        transitionSpec = {
            val forward = targetState > initialState
            (slideInHorizontally(Motion.enter()) { if (forward) it / 4 else -it / 4 } + fadeIn(Motion.enter())) togetherWith
                (slideOutHorizontally(Motion.exit()) { if (forward) -it / 4 else it / 4 } + fadeOut(Motion.exit()))
        },
        label = "onboarding",
        modifier = Modifier.fillMaxSize().background(C.Bg),
    ) { s ->
        when (s) {
            WELCOME -> WelcomeStep { step = DICTATE }
            DICTATE -> DictateStep(keyboard, bubble, { keyboard = it }, { bubble = it }, onBack = ::back) {
                settings.inputMode = when { keyboard && bubble -> "both"; keyboard -> "keyboard"; else -> "bubble" }
                step = MIC
            }
            MIC -> MicStep(onBack = ::back) { step = TURN_ON }
            TURN_ON -> TurnOnStep(keyboard, bubble, onBack = ::back) { step = MODEL }
            MODEL -> ModelStep(settings, onCloud = { cloud = true }, onBack = ::back) { step = TRY }
            else -> TryStep(settings, keyboard, onBack = ::back, onFinish = finish)
        }
    }
}

// ---------------------------------------------------------------- frame

/** Six bars: done ink, current orange, the rest sand. */
@Composable
private fun Progress(step: Int) {
    val label = "Step $step of $STEPS"
    Row(Modifier.fillMaxWidth().semantics { contentDescription = label }, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
        for (i in 1..STEPS) {
            val c by animateColorAsState(when { i < step -> C.Ink; i == step -> C.Accent; else -> C.Step }, Motion.quick(), label = "bar")
            Box(Modifier.weight(1f).height(6.dp).clip(RoundedCornerShape(3.dp)).background(c))
        }
    }
}

/** One step: progress, a Bricolage title with a line under it, the content, and a pinned Back + primary footer. */
@Composable
private fun StepFrame(
    step: Int,
    title: String,
    body: String,
    primary: String,
    onPrimary: () -> Unit,
    onBack: () -> Unit,
    primaryEnabled: Boolean = true,
    bottom: (@Composable ColumnScope.() -> Unit)? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(Modifier.fillMaxSize().background(C.Bg).imePadding()) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).statusBarsPadding()) {
            Box(Modifier.padding(start = 22.dp, end = 22.dp, top = 14.dp)) { Progress(step) }
            Column(Modifier.padding(start = 22.dp, end = 22.dp, top = 24.dp)) {
                Text(title, style = T.H1.copy(fontSize = 32.sp, lineHeight = 34.sp, letterSpacing = (-1).sp), color = C.Ink, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
                Text(body, style = T.Body.copy(lineHeight = 21.sp), color = C.InkSoft, modifier = Modifier.padding(top = S.S))
            }
            Column(Modifier.padding(start = S.L, end = S.L, top = 18.dp, bottom = S.L), verticalArrangement = Arrangement.spacedBy(S.M), content = content)
        }
        if (bottom != null) Column(content = bottom)
        else Footer(primary, onPrimary, onBack, primaryEnabled)
    }
}

@Composable
private fun Footer(primary: String, onPrimary: () -> Unit, onBack: () -> Unit, enabled: Boolean = true, modifier: Modifier = Modifier) {
    Row(modifier.fillMaxWidth().navigationBarsPadding().padding(start = 22.dp, end = 22.dp, bottom = 24.dp, top = S.S), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        BigButton(stringResourceSafe(R.string.back), onBack, fill = C.Sand, ink = C.Ink, weight = FontWeight.SemiBold)
        BigButton(primary, onPrimary, Modifier.weight(1f), enabled = enabled)
    }
}

/** The 56 dp, 18 dp-corner button of the artboards. */
@Composable
internal fun BigButton(
    text: String, onClick: () -> Unit, modifier: Modifier = Modifier, fill: Color = C.Ink, ink: Color = Color.White,
    enabled: Boolean = true, weight: FontWeight = FontWeight.Bold,
) {
    val bg by animateColorAsState(if (enabled) fill else C.Step, Motion.quick(), label = "big")
    Box(
        modifier.heightIn(min = 56.dp).clip(Corners.Button).background(bg)
            .then(if (enabled) Modifier.pressable(onClick) else Modifier.semantics { contentDescription = text })
            .padding(horizontal = 20.dp, vertical = S.M),
        contentAlignment = Alignment.Center,
    ) { Text(text, style = T.Button.copy(fontSize = 17.sp, fontWeight = weight), color = if (enabled) ink else C.Muted, textAlign = TextAlign.Center) }
}

@Composable private fun stringResourceSafe(id: Int) = androidx.compose.ui.res.stringResource(id)

// ---------------------------------------------------------------- 1 welcome

@Composable
private fun WelcomeStep(onNext: () -> Unit) {
    BoxWithConstraints(Modifier.fillMaxSize().background(C.Bg)) {
        if (maxWidth > maxHeight && maxWidth >= 520.dp) { WelcomeWide(onNext); return@BoxWithConstraints }
        val artH = (maxHeight * 0.5f).coerceIn(240.dp, 460.dp)
        Column(Modifier.fillMaxSize()) {
            Column(Modifier.weight(1f).verticalScroll(rememberScrollState())) {
                Art(Panel.Peach, Modifier.statusBarsPadding().padding(start = 12.dp, end = 12.dp, top = 12.dp).fillMaxWidth().height(artH), gx = 0.5f, gy = 0.45f, shape = RoundedCornerShape(34.dp)) {
                    Lattice(size = (artH * 0.58f).coerceAtMost(250.dp), tile = null, palette = Comb.Light)
                    Capsule("see you at the cafe at four", Modifier.align(Alignment.BottomCenter).padding(bottom = 26.dp))
                }
                Column(Modifier.padding(start = 26.dp, end = 26.dp, top = 26.dp), verticalArrangement = Arrangement.spacedBy(S.M)) {
                    Text(
                        buildAnnotatedString { append("Say it.\n"); pushStyle(SpanStyle(color = C.Accent)); append("It's typed."); pop() },
                        style = T.Display.copy(fontSize = 40.sp, lineHeight = 40.sp, letterSpacing = (-1.4).sp), color = C.Ink,
                    )
                    Text("Talk in any app and Voicio writes it out cleanly. It runs on this phone. No account.", style = T.Body.copy(fontSize = 16.sp, lineHeight = 23.sp), color = C.InkSoft)
                }
            }
            Column(Modifier.fillMaxWidth().navigationBarsPadding().padding(start = 26.dp, end = 26.dp, bottom = 22.dp, top = S.M), verticalArrangement = Arrangement.spacedBy(S.M)) {
                BigButton(stringResourceSafe(R.string.welcome_cta), onNext, Modifier.fillMaxWidth())
                Text("Takes about a minute", style = T.Caption, color = C.Muted, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth())
            }
        }
    }
}

/** Landscape and tablets: the art on the left, the words and the button on the right. */
@Composable
private fun WelcomeWide(onNext: () -> Unit) {
    Row(Modifier.fillMaxSize().statusBarsPadding().navigationBarsPadding().padding(12.dp), horizontalArrangement = Arrangement.spacedBy(S.XL)) {
        Art(Panel.Peach, Modifier.weight(1f).fillMaxHeight(), gx = 0.5f, gy = 0.45f, shape = RoundedCornerShape(34.dp)) {
            Lattice(size = 180.dp, tile = null, palette = Comb.Light)
            Capsule("see you at the cafe at four", Modifier.align(Alignment.BottomCenter).padding(bottom = 18.dp))
        }
        Column(Modifier.weight(1f).fillMaxHeight().verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(S.M, Alignment.CenterVertically)) {
            Text(
                buildAnnotatedString { append("Say it.\n"); pushStyle(SpanStyle(color = C.Accent)); append("It's typed."); pop() },
                style = T.Display.copy(fontSize = 40.sp, lineHeight = 40.sp, letterSpacing = (-1.4).sp), color = C.Ink,
            )
            Text("Talk in any app and Voicio writes it out cleanly. It runs on this phone. No account.", style = T.Body.copy(fontSize = 16.sp, lineHeight = 23.sp), color = C.InkSoft)
            BigButton(stringResourceSafe(R.string.welcome_cta), onNext, Modifier.fillMaxWidth().padding(top = S.S))
            Text("Takes about a minute", style = T.Caption, color = C.Muted, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth())
        }
    }
}

/** The ink capsule from the desktop and the bubble: a small honeycomb and live words. */
@Composable
internal fun Capsule(text: String, modifier: Modifier = Modifier) {
    Row(
        modifier.height(40.dp).shadow(12.dp, RoundedCornerShape(20.dp), ambientColor = C.AccentDeep, spotColor = C.AccentDeep).clip(RoundedCornerShape(20.dp)).background(C.Ink)
            .padding(start = 8.dp, end = 16.dp).clearAndSetSemantics { },
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Lattice(size = 30.dp, tile = null, active = true, level = 0.5f)
        Text(text, style = T.Body2.copy(fontSize = 13.sp), color = C.CapsuleText, maxLines = 1)
    }
}

// ---------------------------------------------------------------- 2 how you'll dictate

@Composable
private fun DictateStep(keyboard: Boolean, bubble: Boolean, setKeyboard: (Boolean) -> Unit, setBubble: (Boolean) -> Unit, onBack: () -> Unit, onNext: () -> Unit) {
    StepFrame(
        DICTATE, "How do you want to dictate?", "Pick one or both. You can change it any time.",
        primary = when { keyboard && bubble -> "Continue with both"; keyboard -> "Continue with the keyboard"; bubble -> "Continue with the bubble"; else -> "Pick at least one" },
        onPrimary = onNext, onBack = onBack, primaryEnabled = keyboard || bubble,
    ) {
        InputCard(
            keyboard, setKeyboard, "Voicio Keyboard", "Safest", C.Lime, C.LimeInk,
            "A full keyboard with a big mic. Switch to it from any keyboard's globe key.",
            "Needs no special permission.",
            art = { KeyboardArt() }, artFill = C.Ink,
        )
        InputCard(
            bubble, setBubble, "Floating bubble", "Any keyboard", C.Peach, C.AccentDeep,
            "A small bubble appears next to any text box. Keep using your own keyboard.",
            "Works with any keyboard. Uses the Accessibility permission; some bank apps warn while it is on.",
            art = { BubbleArt() }, artFill = C.Soft,
        )
    }
}

@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
private fun InputCard(
    on: Boolean, onChange: (Boolean) -> Unit, title: String, tag: String, tagFill: Color, tagInk: Color, text: String, note: String,
    art: @Composable () -> Unit, artFill: Color,
) {
    val border by animateColorAsState(if (on) C.Ink else Color.Transparent, Motion.quick(), label = "cardBorder")
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(24.dp)).background(C.Card).border(2.5.dp, border, RoundedCornerShape(24.dp))
            .toggleable(on, role = Role.Checkbox, onValueChange = onChange).padding(14.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Box(Modifier.size(96.dp, 84.dp).clip(RoundedCornerShape(18.dp)).background(artFill).clearAndSetSemantics { }) { art() }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            androidx.compose.foundation.layout.FlowRow(horizontalArrangement = Arrangement.spacedBy(S.S), verticalArrangement = Arrangement.spacedBy(4.dp), itemVerticalAlignment = Alignment.CenterVertically) {
                Text(title, style = T.Title.copy(fontSize = 17.sp, fontWeight = FontWeight.ExtraBold), color = C.Ink)
                Tag(tag, tagFill, tagInk)
            }
            Text(text, style = T.Body2.copy(fontSize = 13.sp, lineHeight = 18.sp), color = C.InkSoft)
            Text(note, style = T.Caption.copy(lineHeight = 17.sp), color = C.Muted)
        }
        Tick(on)
    }
}

@Composable
internal fun Tag(text: String, fill: Color, ink: Color) =
    Text(text, style = T.Label.copy(fontSize = 11.sp, lineHeight = 14.sp), color = ink, maxLines = 1, modifier = Modifier.clip(Corners.Pill).background(fill).padding(horizontal = 8.dp, vertical = 3.dp))

/** The selection circle: ink with a white tick when on. */
@Composable
internal fun Tick(on: Boolean) {
    val bg by animateColorAsState(if (on) C.Ink else C.Unticked, Motion.quick(), label = "tick")
    Box(Modifier.size(26.dp).clip(CircleShape).background(bg), contentAlignment = Alignment.Center) { if (on) Glyph(Icons.Check, Color.White, 14.dp, 3.4f) }
}

@Composable
private fun KeyboardArt() {
    Column(Modifier.fillMaxSize().padding(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(Modifier.fillMaxWidth().height(22.dp).clip(RoundedCornerShape(8.dp)).background(C.InkKey).padding(horizontal = 6.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Box(Modifier.size(16.dp).clip(CircleShape).background(C.Core))
            Box(Modifier.fillMaxWidth(0.6f).height(5.dp).clip(RoundedCornerShape(3.dp)).background(Color(0xFF6E675B)))
        }
        repeat(2) {
            Row(horizontalArrangement = Arrangement.spacedBy(3.dp)) { repeat(10) { Box(Modifier.weight(1f).height(14.dp).clip(RoundedCornerShape(3.dp)).background(C.InkKey)) } }
        }
    }
}

@Composable
private fun BubbleArt() {
    Box(Modifier.fillMaxSize()) {
        Box(Modifier.offset(10.dp, 14.dp).size(52.dp, 7.dp).clip(RoundedCornerShape(4.dp)).background(C.Unticked))
        Box(Modifier.offset(10.dp, 28.dp).size(36.dp, 7.dp).clip(RoundedCornerShape(4.dp)).background(C.Unticked))
        Box(Modifier.align(Alignment.BottomEnd).padding(10.dp).size(34.dp).shadow(6.dp, CircleShape).clip(CircleShape).background(C.Ink), contentAlignment = Alignment.Center) {
            Box(Modifier.size(12.dp).clip(CircleShape).background(C.Core))
        }
    }
}

// ---------------------------------------------------------------- 3 microphone

@Composable
private fun MicStep(onBack: () -> Unit, onNext: () -> Unit) {
    val context = LocalContext.current
    val tick = rememberResumeTick()
    var answers by rememberSaveable { mutableIntStateOf(0) }
    var asked by rememberSaveable { mutableStateOf(false) }
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { asked = true; answers++ }
    val granted = tick >= 0 && answers >= 0 && context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
    // Android stops showing the dialog after two refusals; then the only way is the app's settings page.
    val blocked = !granted && asked && context.findActivity()?.shouldShowRequestPermissionRationale(Manifest.permission.RECORD_AUDIO) == false

    var attempt by remember { mutableIntStateOf(0) }
    var level by remember { mutableFloatStateOf(0f) }
    var heard by remember { mutableStateOf(false) }
    var silent by remember { mutableStateOf(false) }
    var failed by remember { mutableStateOf(false) }
    DisposableEffect(granted, attempt) {
        heard = false; silent = false; failed = false; level = 0f
        val capture = if (granted) AudioCapture(
            onChunk = { _, l -> level = ((l.rmsDb + 55) / 45).coerceIn(0.0, 1.0).toFloat(); if (l.rmsDb > -38) heard = true },
            onFailure = { failed = true },
        ) else null
        capture?.start()
        onDispose { capture?.stop() }
    }
    LaunchedEffect(granted, attempt) { if (granted) { delay(6000); if (!heard) silent = true } }
    val problem = granted && (failed || (silent && !heard))

    StepFrame(
        MIC, "Say hello.", "Voicio listens only while you dictate. Speech is turned into text on this phone.",
        primary = when { !granted && blocked -> "Open app settings"; !granted -> "Allow microphone"; problem -> "Try again"; else -> "Continue" },
        onPrimary = {
            when {
                !granted && blocked -> context.openAppSettings()
                !granted -> launcher.launch(Manifest.permission.RECORD_AUDIO)
                problem -> attempt++
                else -> onNext()
            }
        },
        onBack = onBack,
    ) {
        // Shrink the art when a notice shows, so the notice and its action fit above the buttons without scrolling.
        val noticeShown = (!granted && blocked) || heard || failed || silent
        val artHeight by animateDpAsState(if (noticeShown) 250.dp else 330.dp, Motion.soft(), label = "micArt")
        Art(Panel.Lime, Modifier.fillMaxWidth().height(artHeight), gx = 0.5f, gy = 0.45f, shape = RoundedCornerShape(30.dp)) {
            Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(18.dp)) {
                val pop by animateFloatAsState(1f + level * 0.06f, Motion.snappy(), label = "pop")
                Box(
                    Modifier.size(150.dp).drawBehind {
                        drawCircle(C.Green.copy(alpha = 0.07f), size.minDimension / 2 + 30.dp.toPx())
                        drawCircle(C.Green.copy(alpha = 0.14f), size.minDimension / 2 + 14.dp.toPx())
                    }.scale(pop).clip(CircleShape).background(C.Ink),
                    contentAlignment = Alignment.Center,
                ) { Lattice(size = 120.dp, tile = null, active = granted, level = if (granted) level else 0f, glow = 0.4f) }
                Text(if (granted) "Listening…" else "Waiting for the microphone", style = T.Title.copy(fontSize = 14.sp, fontWeight = FontWeight.SemiBold), color = C.LimeInk)
            }
        }
        if (!granted && asked) GhostButton("Not now, I'll allow it later", onNext, color = C.InkSoft)
        when {
            !granted && blocked -> Notice(NoticeKind.Warn, stringResourceSafe(R.string.mic_blocked))
            heard -> GoodNews("Loud and clear", "Your voice is well above the room noise.")
            failed -> Notice(NoticeKind.Error, stringResourceSafe(R.string.mic_failed))
            silent -> Notice(NoticeKind.Warn, stringResourceSafe(R.string.mic_silent), action = "Skip this check", onAction = onNext)
        }
    }
}


/** Lime status card: a green tick disc, a title and a line. */
@Composable
internal fun GoodNews(title: String, body: String?) {
    Row(
        Modifier.fillMaxWidth().clip(Corners.Item).background(C.Lime).padding(horizontal = S.L, vertical = 14.dp).semantics { liveRegion = LiveRegionMode.Polite },
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(S.M),
    ) {
        Box(Modifier.size(34.dp).clip(CircleShape).background(C.Green), contentAlignment = Alignment.Center) { Glyph(Icons.Check, Color.White, 16.dp, 3f) }
        Column(Modifier.weight(1f)) {
            Text(title, style = T.Title.copy(fontSize = 15.sp), color = C.LimeInk)
            if (body != null) Text(body, style = T.Body2.copy(fontSize = 13.sp), color = Color(0xFF3D5A1C))
        }
    }
}

// ---------------------------------------------------------------- 4 turn it on

@Composable
private fun TurnOnStep(keyboard: Boolean, bubble: Boolean, onBack: () -> Unit, onNext: () -> Unit) {
    val context = LocalContext.current
    val tick = rememberResumeTick()
    var poll by remember { mutableIntStateOf(0) }
    val kbOn = tick + poll >= 0 && VoicioIme.isEnabled(context)
    val accOn = tick + poll >= 0 && FlowAccessibilityService.instance != null
    var waiting by rememberSaveable { mutableStateOf<String?>(null) } // which switch we sent the user to
    // Auto-return: while the user is in Settings, watch for the switch and bring Voicio back to the front.
    LaunchedEffect(waiting) {
        val w = waiting ?: return@LaunchedEffect
        while (true) {
            delay(500); poll++
            val done = if (w == "kb") VoicioIme.isEnabled(context) else FlowAccessibilityService.instance != null
            if (done) {
                waiting = null
                context.findActivity()?.let { a -> runCatching { a.startActivity(Intent(a, a::class.java).addFlags(Intent.FLAG_ACTIVITY_REORDER_TO_FRONT or Intent.FLAG_ACTIVITY_SINGLE_TOP)) } }
                break
            }
        }
    }
    val title = if (keyboard && bubble) "Two quick switches." else "One quick switch."
    StepFrame(
        TURN_ON, title, "Android asks you to turn ${if (keyboard && bubble) "these" else "this"} on yourself. We'll bring you back here after${if (keyboard && bubble) " each" else ""}.",
        primary = if ((!keyboard || kbOn) && (!bubble || accOn)) "Continue" else "Continue for now", onPrimary = onNext, onBack = onBack,
    ) {
        var n = 0
        if (keyboard) SwitchCard(
            "${++n}", C.Peach, "Voicio Keyboard", "Turn on Voicio in your keyboard list.", "Settings › Keyboards › Voicio", kbOn,
            if (kbOn) "Done · keyboard is on" else "Open keyboard settings",
        ) { waiting = "kb"; VoicioIme.openEnableSettings(context) }
        if (bubble) SwitchCard(
            "${++n}", C.Sky, "Floating bubble", "Allow Voicio under Accessibility so the bubble can type for you.", "Accessibility › Installed apps › Voicio", accOn,
            if (accOn) "Done · bubble is on" else "Open Accessibility",
        ) { waiting = "acc"; openBubbleSettings(context) }
        if (bubble && !accOn) Text(
            "If Android says the setting is restricted: open App info, tap ⋮, choose Allow restricted settings, then try again.",
            style = T.Caption.copy(lineHeight = 17.sp), color = C.Muted, modifier = Modifier.padding(horizontal = 6.dp),
        )
    }
}

@Composable
private fun SwitchCard(n: String, badge: Color, title: String, text: String, path: String, on: Boolean, action: String, onOpen: () -> Unit) {
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(24.dp)).background(C.Card).padding(S.L), verticalArrangement = Arrangement.spacedBy(S.M)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(S.M)) {
            Box(Modifier.size(36.dp).clip(RoundedCornerShape(12.dp)).background(badge), contentAlignment = Alignment.Center) { Text(n, style = T.Title.copy(fontSize = 15.sp, fontWeight = FontWeight.ExtraBold), color = C.Ink) }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(title, style = T.Title.copy(fontWeight = FontWeight.ExtraBold), color = C.Ink)
                Text(text, style = T.Body2.copy(fontSize = 13.sp, lineHeight = 18.sp), color = C.InkSoft)
            }
        }
        Row(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(C.Soft).padding(horizontal = S.M, vertical = 10.dp).clearAndSetSemantics { },
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Text(path, style = T.Body2.copy(fontSize = 13.sp), color = C.InkSoft, modifier = Modifier.weight(1f))
            MiniSwitch(on)
        }
        Box(
            Modifier.fillMaxWidth().heightIn(min = 48.dp).clip(RoundedCornerShape(15.dp)).background(if (on) C.Lime else C.Ink)
                .then(if (on) Modifier.semantics { contentDescription = action } else Modifier.pressable(onOpen)),
            contentAlignment = Alignment.Center,
        ) { Text(action, style = T.Button.copy(fontSize = 15.sp), color = if (on) C.LimeInk else Color.White) }
    }
}

/** The small green switch drawn in the "where to find it" line. */
@Composable
private fun MiniSwitch(on: Boolean) {
    val x by androidx.compose.animation.core.animateDpAsState(if (on) 21.dp else 3.dp, Motion.soft(), label = "mini")
    Box(Modifier.size(44.dp, 26.dp).clip(CircleShape).background(if (on) C.Green else Color(0xFFD9D1C2))) {
        Box(Modifier.offset(x, 3.dp).size(20.dp).clip(CircleShape).background(Color.White))
    }
}

// ---------------------------------------------------------------- 5 model

@Composable
private fun ModelStep(settings: FlowSettings, onCloud: () -> Unit, onBack: () -> Unit, onNext: () -> Unit) {
    val context = LocalContext.current
    var meteredOk by rememberSaveable { mutableStateOf(false) }
    var blocked by remember { mutableStateOf<Preflight?>(null) }
    val phase = ModelDownloader.phase
    val busy = phase == ModelDownloader.Phase.Downloading || phase == ModelDownloader.Phase.Unpacking
    val ready = rememberResumeTick() >= 0 && (phase == ModelDownloader.Phase.Done || ModelStore.ready(context))
    val failed = phase == ModelDownloader.Phase.Failed
    fun start() {
        val pre = ModelDownloader.preflight(context)
        blocked = pre.takeIf { it != Preflight.Ok && !(it == Preflight.Metered && (meteredOk || ModelDownloader.hasPartial(context))) }
        if (blocked == null) { ModelDownloader.reset(); ModelDownloader.start(context) }
    }
    val shown by animateFloatAsState(if (ready) 1f else ModelDownloader.progress, androidx.compose.animation.core.tween(400), label = "progress")
    val cloudOn = settings.cloudMode != "off" && CloudProviders.byId(settings.cloudProvider) != null
    StepFrame(
        MODEL, "Pick a model.", "It downloads once and then works offline.",
        primary = when {
            ready || busy || cloudOn -> "Continue"
            blocked == Preflight.Metered -> "Download anyway"
            failed || blocked != null -> "Try again"
            else -> "Download ${ModelDownloader.DOWNLOAD_MB} MB"
        },
        onPrimary = {
            when {
                ready || busy || cloudOn -> onNext()
                blocked == Preflight.Metered -> { meteredOk = true; blocked = null; ModelDownloader.reset(); ModelDownloader.start(context) }
                else -> start()
            }
        },
        onBack = onBack,
    ) {
        PickCard(
            Panel.Balanced, "Parakeet 110M", "English · by NVIDIA · fast and accurate on any phone",
            "${ModelDownloader.DOWNLOAD_MB} MB download · ${ModelDownloader.INSTALLED_MB} MB on this phone · works offline",
            tag = when { ready -> "Ready"; busy -> if (phase == ModelDownloader.Phase.Unpacking) "Checking" else "${(shown * 100).toInt()}%"; else -> "Recommended" }, selected = true,
        ) {
            if (busy) LinearProgressIndicator(progress = { shown }, modifier = Modifier.fillMaxWidth().height(6.dp).clip(Corners.Pill), color = C.Accent, trackColor = C.Card, drawStopIndicator = {}, gapSize = 0.dp)
        }
        PickCard(Panel.Lite, "S1-mini clean-up", "Smart punctuation, fillers and lists · optional", "484 MB · add it later in Settings › Models", tag = "Optional", selected = false)
        when {
            blocked == Preflight.Offline -> Notice(NoticeKind.Error, stringResourceSafe(R.string.dl_offline_title), body = stringResourceSafe(R.string.dl_offline_body))
            blocked == Preflight.NoSpace -> Notice(NoticeKind.Error, stringResourceSafe(R.string.dl_space_title), body = stringResourceSafe(R.string.dl_space_body))
            blocked == Preflight.Metered -> Notice(NoticeKind.Warn, stringResourceSafe(R.string.dl_metered_title), body = androidx.compose.ui.res.stringResource(R.string.dl_metered_body, ModelDownloader.DOWNLOAD_MB))
            failed -> Notice(NoticeKind.Error, stringResourceSafe(R.string.dl_stopped), body = ModelDownloader.error)
        }
        Row(
            Modifier.fillMaxWidth().clip(Corners.Item).drawBehind {
                drawRoundRect(C.Dash, style = Stroke(2.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(8.dp.toPx(), 6.dp.toPx()))), cornerRadius = CornerRadius(20.dp.toPx()))
            }.clickable(role = Role.Button, onClick = onCloud).padding(horizontal = S.L, vertical = 14.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(S.M),
        ) {
            IconTile(Icons.Cloud, C.SkyInk, C.Sky, 40.dp)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(if (cloudOn) "Cloud: ${CloudProviders.byId(settings.cloudProvider)?.let { providerTitle(it) }}" else "Use a cloud provider instead", style = T.Title.copy(fontSize = 15.sp), color = C.Ink)
                Text(
                    "${CloudProviders.all.take(5).joinToString(", ") { it.maker }} and ${CloudProviders.all.size - 5} more, with your own key",
                    style = T.Caption, color = C.Muted,
                )
            }
        }
    }
}

/** A model-first card on its pack colour, ink border when chosen. */
@Composable
private fun PickCard(panel: Panel, name: String, line: String, facts: String, tag: String, selected: Boolean, extra: @Composable ColumnScope.() -> Unit = {}) {
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(22.dp)).panel(panel).border(2.5.dp, if (selected) C.Ink else Color.Transparent, RoundedCornerShape(22.dp))
            .padding(horizontal = S.L, vertical = 14.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(name, style = T.H2.copy(fontSize = 22.sp), color = C.Ink, modifier = Modifier.weight(1f))
            Tag(tag, if (selected) C.Ink else C.Card, if (selected) Color.White else C.Ink)
        }
        Text(line, style = T.Body2.copy(fontSize = 13.sp, lineHeight = 18.sp), color = C.InkSoft)
        Text(facts, style = T.Label, color = C.InkSoft)
        extra()
    }
}

// ---------------------------------------------------------------- 6 try it

@Composable
private fun TryStep(settings: FlowSettings, keyboard: Boolean, onBack: () -> Unit, onFinish: () -> Unit) {
    val context = LocalContext.current
    val tick = rememberResumeTick()
    val mic = tick >= 0 && context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
    val model = tick >= 0 && ModelStore.ready(context)
    val cloud = CloudProviders.byId(settings.cloudProvider)?.takeIf { settings.cloudMode != "off" }
    var practice by rememberSaveable { mutableStateOf("") }
    StepFrame(
        TRY, "Now say something.", if (keyboard) "Tap the box, then the honeycomb, and talk." else "Tap the box, then the bubble, and talk.",
        primary = "Finish", onPrimary = onFinish, onBack = onBack,
        bottom = { if (keyboard) KeyboardPreview(onFinish) else Footer("Finish", onFinish, onBack) },
    ) {
        Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(22.dp)).background(C.Card).padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text("PRACTICE", style = T.Label, color = C.Muted)
            Field(practice, { practice = it }, "Try: “Note to self, as a list: book flights, renew passport, call mum.”", minLines = 4, focusInk = true)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(S.S)) {
            Summary("INPUT", when (settings.inputMode) { "both" -> "Both"; "keyboard" -> "Keyboard"; else -> "Bubble" }, C.Peach, Modifier.weight(1f))
            Summary("MIC", if (mic) "Ready" else "Off", if (mic) C.Lime else C.Butter, Modifier.weight(1f))
            Summary("MODEL", when { model -> "Parakeet"; cloud != null -> cloud.maker; else -> "Not yet" }, if (model || cloud != null) C.Sky else C.Butter, Modifier.weight(1f))
        }
        if (practice.isNotBlank()) GoodNews(stringResourceSafe(R.string.try_it_works), null)
    }
}

@Composable
private fun Summary(k: String, v: String, fill: Color, modifier: Modifier) {
    Column(modifier.clip(RoundedCornerShape(14.dp)).background(fill).padding(horizontal = S.M, vertical = 10.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(k, style = T.Label.copy(fontSize = 10.sp, letterSpacing = 0.8.sp), color = C.InkSoft)
        Text(v, style = T.Label.copy(fontSize = 13.sp), color = C.Ink, maxLines = 1)
    }
}

/** The bottom of the Try step: a preview of the Voicio Keyboard (the honeycomb mic over the keys) and Finish. */
@Composable
private fun KeyboardPreview(onFinish: () -> Unit) {
    Column(
        Modifier.fillMaxWidth().padding(horizontal = S.S).clip(RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp)).background(C.Ink).navigationBarsPadding()
            .padding(start = 10.dp, end = 10.dp, top = 10.dp, bottom = 18.dp),
        verticalArrangement = Arrangement.spacedBy(S.S),
    ) {
        Box(Modifier.fillMaxWidth().height(58.dp).clearAndSetSemantics { }, contentAlignment = Alignment.Center) {
            Box(Modifier.size(56.dp).drawBehind { drawCircle(C.Core.copy(alpha = 0.25f), size.minDimension / 2 + 8.dp.toPx()) }.clip(CircleShape).background(C.InkKey), contentAlignment = Alignment.Center) {
                Lattice(size = 44.dp, tile = null)
            }
        }
        Column(Modifier.clearAndSetSemantics { }, verticalArrangement = Arrangement.spacedBy(5.dp)) {
            repeat(2) { Row(horizontalArrangement = Arrangement.spacedBy(5.dp)) { repeat(10) { Box(Modifier.weight(1f).height(36.dp).clip(RoundedCornerShape(9.dp)).background(C.InkKey)) } } }
        }
        BigButton("Finish", onFinish, Modifier.fillMaxWidth().padding(top = S.XS), fill = C.Accent, ink = C.Ink, weight = FontWeight.ExtraBold)
    }
}
