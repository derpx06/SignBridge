package com.whispr.app.ui

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import com.whispr.app.flow.FlowAccessibilityService
import com.whispr.app.ime.VoicioIme
import android.content.Context
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.foundation.background
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.whispr.app.BuildConfig
import com.whispr.app.R
import com.whispr.app.data.DataNotice
import com.whispr.app.data.HistoryStore
import com.whispr.app.data.NotesStore
import com.whispr.app.edit.SmartEditInstaller
import com.whispr.app.edit.SmartEditModel
import com.whispr.app.flow.FlowSettings
import com.whispr.app.flow.ModelDownloader
import com.whispr.app.flow.ModelStore
import com.whispr.app.flow.Preflight
import com.whispr.core.cloud.CloudProviders
import com.whispr.core.dictation.Trigger

/** Starts the speech model download, or says why it cannot start (offline, no space). */
internal fun startModelDownload(context: Context): String? = when (ModelDownloader.preflight(context)) {
    Preflight.Offline -> context.getString(R.string.dl_offline_title) + ". " + context.getString(R.string.dl_offline_body)
    Preflight.NoSpace -> context.getString(R.string.dl_space_title) + ". " + context.getString(R.string.dl_space_body)
    else -> { ModelDownloader.reset(); ModelDownloader.start(context); null }
}

/** A settings group: tinted icon tile and Bricolage title over one white card. */
// ---------------------------------------------------------------- settings

private val Modes = listOf(Trigger.Mode.HoldOrTap, Trigger.Mode.Hold, Trigger.Mode.Toggle)
private val ModeNames = listOf("Tap or hold", "Hold to talk", "Tap to start and stop")

/** A settings group: a small spaced heading over one white card. */
@Composable
private fun SettingsGroup(name: String, content: @Composable ColumnScope.() -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(name, style = T.Overline, color = C.Muted, modifier = Modifier.padding(horizontal = 6.dp).semantics { heading() })
        Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(22.dp)).background(C.Card), content = content)
    }
}

/** One row: a tinted 32 dp icon tile, title and subtitle, then a switch or a chevron. */
@Composable
internal fun SetRow(
    title: String, sub: String?, icon: String, tile: Color, titleColor: Color = C.Ink, divider: Boolean = true,
    checked: Boolean? = null, onCheck: (Boolean) -> Unit = {}, onClick: (() -> Unit)? = null,
) {
    val base = Modifier.fillMaxWidth().heightIn(min = 58.dp)
    Row(
        base.then(
            when {
                checked != null -> Modifier.toggleable(checked, role = Role.Switch, onValueChange = onCheck)
                onClick != null -> Modifier.clickable(onClick = onClick)
                else -> Modifier
            },
        ).drawBehind { if (divider) drawLine(C.Bg, Offset(0f, size.height), Offset(size.width, size.height), 1.dp.toPx()) }
            .padding(horizontal = 14.dp, vertical = S.S),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(S.M),
    ) {
        Box(Modifier.size(32.dp).clip(RoundedCornerShape(10.dp)).background(tile), contentAlignment = Alignment.Center) { Glyph(icon, C.Ink, 15.dp, 2.2f) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(title, style = T.Title.copy(fontSize = 15.sp, fontWeight = FontWeight.SemiBold), color = titleColor)
            if (sub != null) Text(sub, style = T.Caption, color = C.Muted)
        }
        when {
            checked != null -> SmallSwitch(checked)
            onClick != null -> Glyph(Icons.Chevron, C.Chevron, 16.dp, 2.4f)
        }
    }
}

/** The artboards' 46 x 28 dp switch: orange when on. */
@Composable
internal fun SmallSwitch(on: Boolean) {
    val x by animateDpAsState(if (on) 21.dp else 3.dp, Motion.soft(), label = "sw")
    val bg by animateColorAsState(if (on) C.Accent else C.ToggleOff, Motion.quick(), label = "swbg")
    Box(Modifier.size(46.dp, 28.dp).clip(CircleShape).background(bg)) {
        Box(Modifier.offset(x, 3.dp).size(22.dp).shadow(1.5.dp, CircleShape).clip(CircleShape).background(Color.White))
    }
}

private object P {
    const val Keyboard = "M3 7h18v11H3z M7 11h1 M11 11h1 M15 11h2 M7 14h10"
    const val Bubble = "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z"
    const val Lines = "M4 7h16 M4 12h10 M4 17h13"
    const val Plus = "M5 12h14 M12 5v14"
    const val Cube = "M12 3l8 4.5v9L12 21l-8-4.5v-9z"
    const val Cloud = "M7 18a5 5 0 0 1-.6-9.96A6 6 0 0 1 18 9a4.5 4.5 0 0 1-.5 9z"
    const val Pause = "M9 6v12 M15 6v12"
}

@Composable
internal fun SettingsScreen(settings: FlowSettings, snacks: Snacks, onOpen: (String) -> Unit, onReset: () -> Unit) {
    val context = LocalContext.current
    var refresh by remember { mutableIntStateOf(0) }
    val tick = rememberResumeTick() + refresh
    val kbOn = tick >= 0 && VoicioIme.isEnabled(context)
    val bubbleOn = tick >= 0 && FlowAccessibilityService.instance != null
    val model = tick >= 0 && ModelStore.ready(context)
    val smartInstalled = tick >= 0 && SmartEditModel.installed(context)
    var clean by remember { mutableStateOf(settings.cleanup) }
    var commands by remember { mutableStateOf(settings.spokenCommands) }
    var smart by remember { mutableStateOf(settings.smartFormat) }
    var live by remember { mutableStateOf(settings.liveText) }
    var mode by remember { mutableIntStateOf(Modes.indexOf(settings.shortcutMode).coerceAtLeast(0)) }
    var days by remember { mutableIntStateOf(settings.historyDays) }
    var ask by remember { mutableStateOf<String?>(null) } // which dialog is open
    val cloud = CloudProviders.byId(settings.cloudProvider)?.takeIf { settings.cloudMode != "off" }

    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).statusBarsPadding().padding(start = S.L, end = S.L, top = S.M, bottom = S.XL),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        PageTitle("Settings")
        SettingsGroup("INPUT") {
            SetRow(
                "Voicio Keyboard", if (kbOn) "On · switch to it with the globe key" else "Off · tap to turn it on", P.Keyboard, C.Peach,
                onClick = { if (kbOn) runCatching { VoicioIme.showPicker(context) } else VoicioIme.openEnableSettings(context) },
            )
            SetRow(
                "Floating bubble", if (bubbleOn) "On" else "Off", P.Bubble, C.Sky,
                onClick = { openBubbleSettings(context) },
            )
            SetRow("Bubble trigger", ModeNames[mode], Icons.Clock, C.Soft, onClick = { ask = "mode" }, divider = false)
        }
        SettingsGroup("WRITING") {
            SetRow("Clean up my text", "Punctuation, fillers, numbers", P.Lines, C.Lime, checked = clean, onCheck = { clean = it; settings.cleanup = it })
            SetRow("Spoken commands", "“as a list”, “scratch that”", P.Plus, C.Butter, checked = commands, onCheck = { commands = it; settings.spokenCommands = it })
            SetRow("Smart formatting", "Times, emails, capitals", Icons.Sparkle, C.Peach, checked = smart, onCheck = { smart = it; settings.smartFormat = it })
            SetRow("Show words as I talk", "Live words in the bubble", Icons.Notes, C.Sky, checked = live, onCheck = { live = it; settings.liveText = it })
            SetRow("Words and snippets", "Names, terms and shortcuts", Icons.Book, C.Lime, onClick = { onOpen("words") }, divider = false)
        }
        SettingsGroup("MODELS AND CLOUD") {
            SetRow("Models", listOfNotNull("Parakeet 110M".takeIf { model } ?: "Parakeet 110M · not downloaded", "S1-mini".takeIf { smartInstalled }).joinToString(" · "), P.Cube, C.IrisTile, onClick = { onOpen("models") })
            SetRow(
                "Cloud providers", if (cloud != null) "On · ${providerTitle(cloud)}" else "Off · ${CloudProviders.all.size} available", P.Cloud, C.Sky,
                onClick = { onOpen("cloud") }, divider = false,
            )
        }
        SettingsGroup("PRIVACY") {
            SetRow(
                if (bubbleOn) "Pause Voicio" else "Turn on the bubble",
                if (bubbleOn) "Turns the bubble fully off. Some bank apps warn while it is on; Voicio Keyboard needs no permission."
                else "The bubble is off. Turn it on under Accessibility.",
                P.Pause, C.Border,
                onClick = {
                    if (bubbleOn) { pauseBubble(); refresh++; snacks.show("Voicio paused. Turn it back on here.") }
                    else openBubbleSettings(context)
                },
            )
            SetRow("Keep history", when (days) { 0 -> "Off · nothing is kept"; else -> "$days days · text only, never audio" }, Icons.Clock, C.Sky, onClick = { ask = "days" })
            SetRow("Delete all history", null, Icons.Trash, C.CoralSoft, titleColor = C.Coral, onClick = { ask = "history" })
            SetRow(stringResource(R.string.set_reset), "Erases history, notes, words, settings and models", Icons.Trash, C.CoralSoft, titleColor = C.Coral, onClick = { ask = "reset" }, divider = false)
        }
        SettingsGroup("ABOUT") {
            SetRow("Privacy and about", "Version ${BuildConfig.VERSION_NAME} · open source", Icons.Info, C.Border, onClick = { onOpen("about") }, divider = false)
        }
    }

    when (val kind = ask) {
        "mode" -> ChoiceDialog("Bubble trigger", ModeNames, mode, onDismiss = { ask = null }) { mode = it; settings.shortcutMode = Modes[it] }
        "days" -> ChoiceDialog("Keep history", listOf("Off", "7 days", "30 days"), listOf(0, 7, 30).indexOf(days).coerceAtLeast(2), onDismiss = { ask = null }) {
            val d = listOf(0, 7, 30)[it]
            if (d == 0 && HistoryStore.items.isNotEmpty()) ask = "historyOff" else { days = d; settings.historyDays = d }
        }
        "history", "historyOff" -> ConfirmDialog(
            "Delete all history?", "Every saved dictation is erased from this phone. This cannot be undone.", "Delete", onDismiss = { ask = null },
        ) {
            HistoryStore.clear()
            if (kind == "historyOff") { days = 0; settings.historyDays = 0 }
        }
        "reset" -> ConfirmDialog(
            "Reset Voicio?", "This erases history, notes, words, settings and the downloaded models, then starts the welcome again.", "Reset", onDismiss = { ask = null },
        ) {
            ModelDownloader.delete(context); HistoryStore.clear(); NotesStore.clear()
            com.whispr.app.data.SnippetStore.ensure(context); com.whispr.app.data.SnippetStore.clear()
            SmartEditInstaller.uninstall(context); DataNotice.dismiss(context)
            settings.clearAll(); onReset()
        }
    }
}

/** A short list of choices in a dialog (trigger mode, history length). */
@Composable
private fun ChoiceDialog(title: String, options: List<String>, selected: Int, onDismiss: () -> Unit, onPick: (Int) -> Unit) {
    androidx.compose.material3.AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = C.Card,
        title = { Text(title, style = T.H2, color = C.Ink) },
        text = {
            Column {
                options.forEachIndexed { i, o ->
                    Row(
                        Modifier.fillMaxWidth().heightIn(min = S.Touch).clip(Corners.Control).selectable(i == selected, role = Role.RadioButton) { onDismiss(); onPick(i) }.padding(horizontal = S.S),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(S.M),
                    ) {
                        Tick(i == selected)
                        Text(o, style = T.Body, color = C.Ink)
                    }
                }
            }
        },
        confirmButton = { androidx.compose.material3.TextButton(onDismiss) { Text("Close", style = T.Button, color = C.Muted) } },
    )
}

// ---------------------------------------------------------------- models

private val ProviderMarks = mapOf(
    "groq" to "Gq", "openai" to "OA", "gemini" to "Gm", "deepgram" to "Dg", "elevenlabs" to "El", "mistral" to "Mi",
    "assemblyai" to "AA", "azure" to "Az", "sarvam" to "Sv", "soniox" to "Sx", "speechmatics" to "Sm", "together" to "Tg",
    "lemonfox" to "Lf", "lemonfoxeu" to "LE", "cartesia" to "Ca", "openrouter" to "OR",
)
internal fun providerMark(id: String, maker: String) = ProviderMarks[id] ?: maker.filter { it.isLetter() }.take(2)
internal val ProviderTints = listOf(C.Peach, C.Sky, C.Lime, C.Butter, C.IrisTile)

/** Models (PhoneAppB): the speech models on this phone, model first, then the cloud providers as tiles. */
@Composable
internal fun ModelsScreen(settings: FlowSettings, onBack: () -> Unit, onOpen: (String) -> Unit) {
    val context = LocalContext.current
    var refresh by remember { mutableIntStateOf(0) }
    val tick = rememberResumeTick() + refresh
    val ready = tick >= 0 && ModelStore.ready(context)
    val smart = tick >= 0 && SmartEditModel.installed(context)
    val phase = ModelDownloader.phase
    val busy = phase == ModelDownloader.Phase.Downloading || phase == ModelDownloader.Phase.Unpacking
    LaunchedEffect(phase) { if (phase == ModelDownloader.Phase.Done) { ModelDownloader.reset(); refresh++ } }
    var error by remember { mutableStateOf<String?>(null) }
    SubScreen("Models", null, onBack) {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text("ON THIS PHONE", style = T.Overline, color = C.Muted, modifier = Modifier.padding(horizontal = S.XS))
            ModelRow(
                "P1", C.Peach, "Parakeet 110M", "English · by NVIDIA",
                if (ready) "${ModelDownloader.INSTALLED_MB} MB · installed" else "${ModelDownloader.DOWNLOAD_MB} MB download",
                state = when { ready -> "In use"; busy -> "${(ModelDownloader.progress * 100).toInt()}%"; else -> "Get" },
                kind = when { ready -> 0; busy -> 1; else -> 2 },
                onAction = { if (!ready && !busy) error = startModelDownload(context) },
                onClick = { onOpen("model") },
            )
            ModelRow(
                "S1", C.Lime, "S1-mini clean-up", "Smart punctuation and lists · English",
                if (smart) "357 MB · installed" else "484 MB download",
                state = if (smart) (if (settings.aiPolish) "In use" else "Use") else "Get", kind = if (smart && settings.aiPolish) 0 else if (smart) 1 else 2,
                onAction = { onOpen("smart") }, onClick = { onOpen("smart") },
            )
            error?.let { Notice(NoticeKind.Error, "Download did not start", body = it) }
            Text("CLOUD · YOUR OWN KEY", style = T.Overline, color = C.Muted, modifier = Modifier.padding(start = S.XS, end = S.XS, top = S.S))
            Column(
                Modifier.fillMaxWidth().clip(Corners.Item).background(C.Card).clickable(onClickLabel = "Open cloud providers") { onOpen("cloud") }.padding(S.M),
                verticalArrangement = Arrangement.spacedBy(S.S),
            ) {
                CloudProviders.all.take(10).chunked(5).forEachIndexed { row, chunk ->
                    Row(horizontalArrangement = Arrangement.spacedBy(S.S)) {
                        chunk.forEachIndexed { i, p ->
                            Box(Modifier.weight(1f).height(44.dp).clip(RoundedCornerShape(13.dp)).background(ProviderTints[(row * 5 + i) % 5]), contentAlignment = Alignment.Center) {
                                Text(providerMark(p.id, p.maker), style = T.Label.copy(fontSize = 13.sp, fontWeight = FontWeight.ExtraBold), color = C.Ink)
                            }
                        }
                    }
                }
                Text("${CloudProviders.all.size} providers with your own key. Off unless you turn one on.", style = T.Caption, color = C.Muted)
            }
        }
    }
}

/** A model-first row: a tinted mark, the model's name, languages and maker, size, and one action pill. */
@Composable
private fun ModelRow(mark: String, tint: Color, name: String, langs: String, size: String, state: String, kind: Int, onAction: () -> Unit, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clip(Corners.Item).background(C.Card).clickable(onClick = onClick).padding(S.M),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(S.M),
    ) {
        Box(Modifier.size(46.dp).clip(RoundedCornerShape(14.dp)).background(tint), contentAlignment = Alignment.Center) { Text(mark, style = T.Label.copy(fontSize = 14.sp, fontWeight = FontWeight.ExtraBold), color = C.Ink) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(name, style = T.Title.copy(fontSize = 15.sp, fontWeight = FontWeight.ExtraBold), color = C.Ink)
            Text(langs, style = T.Caption, color = C.InkSoft)
            Text(size, style = T.Caption.copy(fontSize = 11.sp, fontWeight = FontWeight.SemiBold), color = C.Muted)
        }
        TonalButton(state, onAction, fill = when (kind) { 0 -> C.Ink; 2 -> C.Accent; else -> C.Soft }, ink = if (kind == 0) Color.White else C.Ink)
    }
}


// ---------------------------------------------------------------- speech model

@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun ModelScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    var refresh by remember { mutableIntStateOf(0) }
    val ready = rememberResumeTick() + refresh >= 0 && ModelStore.ready(context)
    val phase = ModelDownloader.phase
    val busy = phase == ModelDownloader.Phase.Downloading || phase == ModelDownloader.Phase.Unpacking
    var error by remember { mutableStateOf<String?>(null) }
    var confirm by remember { mutableStateOf(false) }
    LaunchedEffect(phase) {
        if (phase == ModelDownloader.Phase.Done) { ModelDownloader.reset(); refresh++ } else if (phase == ModelDownloader.Phase.Failed) error = ModelDownloader.error
    }
    val shown by animateFloatAsState(ModelDownloader.progress, Motion.quick(), label = "model")

    SubScreen("Speech model", "Runs on this phone, works offline", onBack) {
        Column(
            Modifier.fillMaxWidth().clip(Corners.Card).panel(Panel.Lite).padding(S.L + S.XS).enter(0),
            verticalArrangement = Arrangement.spacedBy(S.M),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("Parakeet 110M", style = T.H1, color = C.Ink)
                    Text("English · by NVIDIA", style = T.Body2, color = C.InkSoft)
                }
                Lattice(size = 64.dp, square = true, active = busy, level = 0.3f, sweep = phase == ModelDownloader.Phase.Unpacking, progress = if (busy && phase == ModelDownloader.Phase.Downloading) shown else null)
            }
            Text(stringResource(R.string.model_blurb), style = T.Body2, color = C.InkSoft)
            Column(Modifier.fillMaxWidth().padding(top = S.XS), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Box(Modifier.fillMaxWidth().height(1.5.dp).background(C.Ink.copy(alpha = 0.1f)))
                Part("Parakeet TDT 110M", "speech to text")
                Part("Silero", "voice detection")
                Part("Rule-based tidy-up", "built in")
                Part("Download", "${ModelDownloader.DOWNLOAD_MB} MB, once")
                Part("On this phone", "${ModelDownloader.INSTALLED_MB} MB · about 0.7 GB memory")
            }
            AnimatedContent(ready, label = "ready") { if (it) Chip("Ready · in use", C.Ink, Color.White) else Chip("Not downloaded", C.Card, C.Ink) }
            if (busy) {
                LinearProgressIndicator(progress = { shown }, modifier = Modifier.fillMaxWidth().height(6.dp).clip(Corners.Pill), color = C.Accent, trackColor = C.Card, drawStopIndicator = {}, gapSize = 0.dp)
                Text(
                    if (phase == ModelDownloader.Phase.Unpacking) stringResource(R.string.dl_unpacking) else "Downloading · ${(shown * 100).toInt()}%. You can leave this screen.",
                    style = T.Body2, color = C.InkSoft,
                )
            }
        }
        error?.takeIf { !busy }?.let { Notice(NoticeKind.Error, "Download stopped", body = it) }
        Column(Modifier.enter(2), verticalArrangement = Arrangement.spacedBy(S.S)) {
            when {
                busy -> GhostButton("Stop download", { ModelDownloader.cancel() })
                ready -> GhostButton(stringResource(R.string.set_delete_model), { confirm = true }, color = C.Coral)
                else -> PrimaryButton(if (error != null) "Try again" else stringResource(R.string.model_download_cta, ModelDownloader.DOWNLOAD_MB), { error = startModelDownload(context) })
            }
            Text(
                "Frees its memory after 5 idle minutes. Each file is checked against a fixed checksum before it is used.",
                style = T.Caption, color = C.Muted, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth(),
            )
        }
    }
    if (confirm) ConfirmDialog(
        "Delete the speech model?", "Frees ${ModelDownloader.INSTALLED_MB} MB. Voicio cannot type until you download it again.", "Delete", onDismiss = { confirm = false },
    ) { ModelDownloader.delete(context); refresh++ }
}

// ---------------------------------------------------------------- about

@Composable
internal fun AboutScreen(onBack: () -> Unit, onReplay: () -> Unit) {
    val uri = LocalUriHandler.current
    val policy = stringResource(R.string.privacy_policy_url)
    var licences by remember { mutableStateOf(false) }
    SubScreen("Privacy and about", null, onBack) {
        Column(
            Modifier.fillMaxWidth().clip(Corners.Card).background(C.Soft).panel(Panel.Peach, gx = 1f, gy = 0f).padding(S.XL).enter(0),
            verticalArrangement = Arrangement.spacedBy(S.L),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(S.L)) {
                Lattice(size = 88.dp, square = true)
                Column {
                    Text("voicio", style = T.Display, color = C.Ink)
                    Text("Version ${BuildConfig.VERSION_NAME} · beta · free and open source", style = T.Body2, color = C.InkSoft)
                }
            }
            Column(verticalArrangement = Arrangement.spacedBy(S.S)) {
                PrimaryButton("Run setup again", onReplay)
                SoftButton(stringResource(R.string.privacy_policy), { runCatching { uri.openUri(policy) } })
            }
        }
        Column(Modifier.fillMaxWidth().clip(Corners.Item).background(C.Lime).padding(S.L + S.XS).enter(1), verticalArrangement = Arrangement.spacedBy(S.S)) {
            Text("Your privacy", style = T.H2, color = C.Ink)
            Promise("Audio is never saved", "The microphone is used only while you dictate.")
            Promise("Nothing is sent unless you choose", "The internet is used to download models, and for a cloud provider only if you turn one on.")
            Promise("Accessibility is for typing only", "Voicio reads the focused field to insert text. It does not read other screens.")
        }
        Column(Modifier.fillMaxWidth().clip(Corners.Item).background(C.Card).padding(S.L + S.XS).enter(2), verticalArrangement = Arrangement.spacedBy(S.S)) {
            Text("Built on", style = T.H2, color = C.Ink)
            Credits.forEach { (n, d) ->
                Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(C.Soft).padding(horizontal = 14.dp, vertical = 12.dp)) {
                    Text(n, style = T.Title.copy(fontSize = androidx.compose.ui.unit.TextUnit(14f, androidx.compose.ui.unit.TextUnitType.Sp)), color = C.Ink)
                    Text(d, style = T.Caption, color = C.Muted)
                }
            }
            GhostButton("All open-source licences", { licences = true }, color = C.AccentInk)
        }
    }
    if (licences) androidx.compose.material3.AlertDialog(
        onDismissRequest = { licences = false },
        containerColor = C.Card,
        title = { Text("Open-source licences", style = T.H2, color = C.Ink) },
        text = { Text("sherpa-onnx: Apache-2.0\nONNX Runtime: MIT\nSilero VAD: MIT\nNVIDIA Parakeet TDT 110M model: CC-BY-4.0\nS1-mini model: Superwhisper\nBricolage Grotesque font: SIL Open Font License 1.1", style = T.Body, color = C.InkSoft) },
        confirmButton = { androidx.compose.material3.TextButton({ licences = false }) { Text("Close", style = T.Button, color = C.AccentInk) } },
    )
}

private val Credits = listOf(
    "Parakeet speech model" to "NVIDIA · CC-BY-4.0",
    "S1-mini clean-up model" to "Superwhisper",
    "sherpa-onnx" to "k2-fsa · Apache-2.0",
    "Silero voice detection" to "Silero · MIT",
    "Bricolage Grotesque" to "Ateliertriay · SIL OFL 1.1",
)

@Composable
private fun Promise(title: String, body: String) {
    Row(horizontalArrangement = Arrangement.spacedBy(S.M)) {
        Box(Modifier.padding(top = 2.dp).size(22.dp).clip(androidx.compose.foundation.shape.CircleShape).background(C.Green), contentAlignment = Alignment.Center) { Glyph(Icons.Check, Color.White, 13.dp, 3f) }
        Column(Modifier.weight(1f)) {
            Text(title, style = T.Title, color = C.LimeInk)
            Text(body, style = T.Body2, color = C.InkSoft)
        }
    }
}

@Composable
private fun Part(name: String, value: String) {
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(S.S)) {
        Text(name, style = T.Label.copy(fontSize = androidx.compose.ui.unit.TextUnit(13f, androidx.compose.ui.unit.TextUnitType.Sp)), color = C.Ink, modifier = Modifier.weight(1f))
        Text(value, style = T.Caption.copy(fontSize = androidx.compose.ui.unit.TextUnit(13f, androidx.compose.ui.unit.TextUnitType.Sp)), color = C.InkSoft)
    }
}
