package com.whispr.app.ui

import android.Manifest
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.consumeWindowInsets
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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.lazy.staggeredgrid.LazyVerticalStaggeredGrid
import androidx.compose.foundation.lazy.staggeredgrid.StaggeredGridCells
import androidx.compose.foundation.lazy.staggeredgrid.itemsIndexed
import androidx.compose.foundation.lazy.staggeredgrid.rememberLazyStaggeredGridState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Snackbar
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.SwipeToDismissBox
import androidx.compose.material3.SwipeToDismissBoxValue
import androidx.compose.material3.Text
import androidx.compose.material3.rememberSwipeToDismissBoxState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.whispr.app.R
import com.whispr.app.data.DataNotice
import com.whispr.app.data.HistoryEntry
import com.whispr.app.data.HistoryStore
import com.whispr.app.data.Note
import com.whispr.app.data.NotesStore
import com.whispr.app.flow.FlowAccessibilityService
import com.whispr.app.flow.FlowSettings
import com.whispr.app.flow.ModelDownloader
import com.whispr.app.flow.ModelStore
import com.whispr.app.ime.VoicioIme
import com.whispr.core.dictation.Trigger
import java.util.Calendar
import java.util.Date
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

/** The four tabs of the bottom bar (PhoneAppA), with the artboards' icon paths. */
private enum class Tab(val label: String, val icon: String) {
    Home("Home", "M3 11l9-7 9 7 M5 10v10h14V10"),
    History("History", Icons.Clock),
    Notes("Notes", "M6 3h9l4 4v14H6z M14 3v5h5 M9 13h7 M9 17h5"),
    Settings("Settings", Icons.Sliders),
}

/** One snackbar at a time. [commit] runs when the snackbar goes away without Undo (or the screen closes). */
internal class Snacks(val host: SnackbarHostState, private val scope: CoroutineScope) {
    fun show(message: String, undo: (() -> Unit)? = null, commit: () -> Unit = {}) {
        host.currentSnackbarData?.dismiss()
        scope.launch {
            var undone = false
            try {
                undone = host.showSnackbar(message, actionLabel = if (undo != null) "Undo" else null, duration = SnackbarDuration.Short) == SnackbarResult.ActionPerformed
            } finally {
                if (undone) undo?.invoke() else commit()
            }
        }
    }
}

/**
 * The everyday app: four tabs (Home, History, Notes, Settings) in a white bottom bar with a peach pill, and
 * sub-screens that slide in over them: "models", "model", "smart", "cloud", "words", "about" or "note:<id>".
 */
@Composable
fun MainShell(settings: FlowSettings, onReplayWelcome: () -> Unit) {
    val context = LocalContext.current
    LaunchedEffect(Unit) { DataNotice.load(context); HistoryStore.ensure(context, settings.historyDays); NotesStore.ensure(context) }
    var tab by rememberSaveable { mutableIntStateOf(0) }
    var sub by rememberSaveable { mutableStateOf<String?>(null) }
    val snacks = Snacks(remember { SnackbarHostState() }, rememberCoroutineScope())
    StatusBarIcons(dark = true)
    BackHandler(enabled = sub == null && tab != 0) { tab = 0 }
    BackHandler(enabled = sub != null && sub?.startsWith("note:") == false) { sub = if (sub == "model" || sub == "smart") "models" else null }

    AnimatedContent(
        targetState = sub,
        transitionSpec = {
            val forward = targetState != null
            (slideInHorizontally(Motion.enter()) { if (forward) it / 3 else -it / 3 } + fadeIn(Motion.enter())) togetherWith
                (slideOutHorizontally(Motion.exit()) { if (forward) -it / 6 else it / 6 } + fadeOut(Motion.exit()))
        },
        label = "sub",
        modifier = Modifier.fillMaxSize().background(C.Bg),
    ) { s ->
        when {
            s == null -> Scaffold(
                containerColor = C.Bg,
                contentWindowInsets = WindowInsets(0, 0, 0, 0),
                bottomBar = { NavBar(tab) { tab = it } },
                floatingActionButton = {
                    AnimatedVisibility(tab == Tab.Notes.ordinal, enter = scaleIn(Motion.soft()) + fadeIn(), exit = scaleOut(Motion.exit()) + fadeOut(Motion.exit())) {
                        Fab("New voice note") { sub = "note:${System.currentTimeMillis()}" }
                    }
                },
                snackbarHost = {
                    SnackbarHost(snacks.host) { data -> Snackbar(data, shape = Corners.Control, containerColor = C.Ink, contentColor = C.Cream, actionColor = C.Core) }
                },
            ) { pad ->
                AnimatedContent(
                    targetState = tab,
                    transitionSpec = { fadeIn(Motion.enter()) togetherWith fadeOut(Motion.exit()) },
                    label = "tabs",
                    modifier = Modifier.fillMaxSize().padding(pad).consumeWindowInsets(pad),
                ) { t ->
                    when (t) {
                        Tab.Home.ordinal -> HomeScreen(settings, onOpenModels = { sub = "models" })
                        Tab.History.ordinal -> HistoryScreen(settings, snacks)
                        Tab.Notes.ordinal -> NotesScreen(onOpen = { sub = "note:$it" })
                        else -> SettingsScreen(settings, snacks, onOpen = { sub = it }, onReset = onReplayWelcome)
                    }
                }
            }
            s == "about" -> AboutScreen(onBack = { sub = null }, onReplay = onReplayWelcome)
            s == "models" -> ModelsScreen(settings, onBack = { sub = null }, onOpen = { sub = it })
            s == "model" -> ModelScreen(onBack = { sub = "models" })
            s == "smart" -> SmartEditScreen(settings, onBack = { sub = "models" })
            s == "cloud" -> CloudScreen(settings, onBack = { sub = null })
            s == "words" -> WordsScreen(settings, snacks, onBack = { sub = null })
            else -> {
                val id = s.removePrefix("note:").toLongOrNull() ?: 0L
                val note = remember(id) { NotesStore.items.firstOrNull { it.id == id } ?: Note(id, "", "", id) }
                NoteEditor(note, onDismiss = { sub = null }, onDeleted = { gone ->
                    sub = null
                    snacks.show("Note deleted", undo = { NotesStore.upsert(gone) })
                })
            }
        }
    }
}

/** White bar, hairline on top, a 60 x 32 dp peach pill behind the chosen icon. */
@Composable
private fun NavBar(selected: Int, onSelect: (Int) -> Unit) {
    Row(
        Modifier.fillMaxWidth().background(C.Card).drawBehind { drawLine(C.Line, Offset.Zero, Offset(size.width, 0f), 1.dp.toPx()) }
            .navigationBarsPadding().heightIn(min = 76.dp).padding(start = S.S, end = S.S, top = S.S, bottom = 14.dp),
    ) {
        Tab.entries.forEachIndexed { i, t ->
            val on = i == selected
            val pill by animateColorAsState(if (on) C.Peach else Color.Transparent, Motion.quick(), label = "pill")
            Column(
                Modifier.weight(1f).heightIn(min = S.Touch).clip(Corners.Item).selectable(on, role = Role.Tab) { onSelect(i) },
                horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Box(Modifier.size(60.dp, 32.dp).clip(RoundedCornerShape(16.dp)).background(pill), contentAlignment = Alignment.Center) { Glyph(t.icon, C.Ink, 20.dp, 2.2f) }
                Text(t.label, style = T.Label.copy(fontWeight = if (on) FontWeight.ExtraBold else FontWeight.Medium), color = C.Ink, maxLines = 1)
            }
        }
    }
}

/** The ink "New voice note" button with the honeycomb as its icon. */
@Composable
private fun Fab(text: String, onClick: () -> Unit) {
    Row(
        Modifier.height(56.dp).shadow(10.dp, RoundedCornerShape(20.dp)).clip(RoundedCornerShape(20.dp)).background(C.Ink)
            .semantics { contentDescription = text }.pressable(onClick).padding(start = S.L, end = 20.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Lattice(size = 26.dp, tile = null)
        Text(text, style = T.Button.copy(fontSize = 15.sp), color = Color.White)
    }
}

// ---------------------------------------------------------------- shared bits

private fun ago(at: Long): String {
    val m = (System.currentTimeMillis() - at) / 60_000
    return when {
        m < 1 -> "Just now"
        m < 60 -> "$m min ago"
        m < 24 * 60 -> "${m / 60} h ago"
        else -> java.text.SimpleDateFormat("d MMM", java.util.Locale.getDefault()).format(Date(at))
    }
}

private fun clock(at: Long) = java.text.SimpleDateFormat("HH:mm", java.util.Locale.getDefault()).format(Date(at))

private fun dayKey(at: Long): Int = Calendar.getInstance().apply { timeInMillis = at }.let { it.get(Calendar.YEAR) * 400 + it.get(Calendar.DAY_OF_YEAR) }

/** How the bubble is used, in the user's chosen trigger mode. */
internal fun triggerVerb(mode: Trigger.Mode) = if (mode == Trigger.Mode.Hold) "Hold" else "Tap"

internal fun copyText(context: android.content.Context, text: String) =
    context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Voicio", text))

/** A big Bricolage page title (History, Notes, Settings). */
@Composable
internal fun PageTitle(text: String, modifier: Modifier = Modifier) =
    Text(text, style = T.H1.copy(fontSize = 32.sp, lineHeight = 36.sp, letterSpacing = (-1).sp), color = C.Ink, modifier = modifier.padding(horizontal = 6.dp).semantics { heading() })

private val ChipTints = listOf(C.Sky to C.SkyInk, C.Butter to C.ButterInk, C.Peach to C.AccentDeep, C.Lime to C.LimeInk)

/** A small tinted pill (the app a dictation went to); the colour follows the text. */
@Composable
private fun KindChip(text: String) {
    if (text.isEmpty()) return
    val (fill, ink) = ChipTints[Math.floorMod(text.hashCode(), ChipTints.size)]
    Tag(text, fill, ink)
}

// ---------------------------------------------------------------- home

@Composable
private fun HomeScreen(settings: FlowSettings, onOpenModels: () -> Unit) {
    val context = LocalContext.current
    val today = dayKey(System.currentTimeMillis())
    val todays = HistoryStore.items.filter { dayKey(it.at) == today }
    var refresh by remember { mutableIntStateOf(0) }
    val tick = rememberResumeTick() + refresh
    val mic = tick >= 0 && context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
    val bubbleOn = tick >= 0 && FlowAccessibilityService.instance != null
    val keyboardOn = tick >= 0 && VoicioIme.isEnabled(context)
    val model = tick >= 0 && ModelStore.ready(context)
    val wantsBubble = settings.inputMode != "keyboard"
    val wantsKeyboard = settings.inputMode != "bubble"
    var practice by rememberSaveable { mutableStateOf("") }
    val phase = ModelDownloader.phase
    val downloading = phase == ModelDownloader.Phase.Downloading || phase == ModelDownloader.Phase.Unpacking
    var downloadError by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(phase) {
        if (phase == ModelDownloader.Phase.Done) { ModelDownloader.reset(); refresh++ } else if (phase == ModelDownloader.Phase.Failed) downloadError = ModelDownloader.error
    }
    val askMic = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
        refresh++
        if (!ok && context.findActivity()?.shouldShowRequestPermissionRationale(Manifest.permission.RECORD_AUDIO) == false) context.openAppSettings()
    }
    val inputReady = (wantsBubble && bubbleOn) || (wantsKeyboard && keyboardOn)
    val ready = mic && model && inputReady

    Column(
        Modifier.fillMaxSize().imePadding().verticalScroll(rememberScrollState()).statusBarsPadding().padding(start = S.L, end = S.L, top = S.M, bottom = S.XL),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Row(Modifier.fillMaxWidth().heightIn(min = S.Touch).padding(horizontal = 6.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("voicio", style = T.H1.copy(fontSize = 26.sp, letterSpacing = (-0.8).sp), color = C.Ink, modifier = Modifier.weight(1f))
            StatusPill(if (ready) "Ready · on device" else "Needs setup", ready)
        }
        // The hero: what to do, on a peach wash, with the warm honeycomb.
        androidx.compose.foundation.layout.BoxWithConstraints(Modifier.fillMaxWidth().heightIn(min = 200.dp).clip(RoundedCornerShape(28.dp)).panel(Panel.Peach, gx = 0.85f, gy = 0.25f).enter(0)) {
            // 150 dp on a regular phone; smaller on narrow ones so it never covers the title.
            Lattice(Modifier.align(Alignment.TopEnd).offset(x = (-8).dp, y = 10.dp), size = minOf(150.dp, maxWidth * 0.34f), tile = null, palette = Comb.Light)
            Column(Modifier.padding(22.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text("Talk in any app.", style = T.H1.copy(fontSize = 30.sp, lineHeight = 31.sp, letterSpacing = (-1).sp), color = C.Ink, modifier = Modifier.widthIn(max = 220.dp))
                Text(
                    when {
                        wantsKeyboard && wantsBubble -> "Tap the honeycomb on Voicio Keyboard, or the bubble next to any text box."
                        wantsKeyboard -> "Switch to Voicio Keyboard with the globe key, then tap the honeycomb."
                        else -> "${triggerVerb(settings.shortcutMode)} the bubble next to any text box and talk."
                    },
                    style = T.Body2.copy(lineHeight = 20.sp), color = C.InkSoft, modifier = Modifier.widthIn(max = 210.dp),
                )
                Row(Modifier.padding(top = S.XS), horizontalArrangement = Arrangement.spacedBy(S.S)) {
                    if (wantsKeyboard) Tag(if (keyboardOn) "Keyboard on" else "Keyboard off", if (keyboardOn) C.Ink else C.Card, if (keyboardOn) Color.White else C.Ink)
                    if (wantsBubble) Tag(if (bubbleOn) "Bubble on" else "Bubble off", if (bubbleOn && !wantsKeyboard) C.Ink else C.Card, if (bubbleOn && !wantsKeyboard) Color.White else C.Ink)
                }
            }
        }
        if (DataNotice.reset) {
            Notice(NoticeKind.Warn, context.getString(R.string.data_reset_title), body = context.getString(R.string.data_reset_body), action = context.getString(R.string.dismiss)) { DataNotice.dismiss(context) }
        }
        if (!ready) {
            CardBox(Modifier.enter(1), shape = RoundedCornerShape(22.dp)) {
                Text("FINISH SETUP", style = T.Label, color = C.Muted, modifier = Modifier.padding(start = 14.dp, top = 14.dp))
                if (!mic) SetupRow(Icons.Mic, C.Peach, "Microphone", "Hears you only while you dictate", "Allow") { askMic.launch(Manifest.permission.RECORD_AUDIO) }
                if (wantsKeyboard && !keyboardOn) SetupRow(Icons.Keyboard, C.Peach, "Voicio Keyboard", "Turn it on in your keyboard list", "Turn on") { VoicioIme.openEnableSettings(context) }
                if (wantsBubble && !bubbleOn) SetupRow("M12 3a9 9 0 110 18 9 9 0 010-18z", C.Sky, "Floating bubble", "Allow it under Accessibility", "Turn on") { openBubbleSettings(context) }
                if (!model) SetupRow(
                    Icons.Down, C.Lime, "Parakeet 110M",
                    when {
                        downloading && phase == ModelDownloader.Phase.Unpacking -> "Unpacking. This takes a minute."
                        downloading -> "Downloading · ${(ModelDownloader.progress * 100).toInt()}%"
                        downloadError != null -> downloadError!!
                        else -> "Speech model · ${ModelDownloader.DOWNLOAD_MB} MB, once"
                    },
                    if (downloading) null else if (downloadError != null) "Retry" else "Get", error = downloadError != null && !downloading,
                ) { downloadError = startModelDownload(context) }
            }
        }
        Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(22.dp)).background(C.Card).padding(14.dp).enter(2), verticalArrangement = Arrangement.spacedBy(S.S)) {
            Text("TRY IT HERE", style = T.Label, color = C.Muted)
            Field(practice, { practice = it }, "Tap, then talk…", minLines = 2, soft = true)
        }
        Row(Modifier.enter(3), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            StatCard("Words today", todays.sumOf { it.words }, Modifier.weight(1f))
            StatCard("Dictations", todays.size, Modifier.weight(1f))
        }
        Row(
            Modifier.fillMaxWidth().clip(Corners.Item).background(C.Card).clickable(onClick = onOpenModels).padding(horizontal = 14.dp, vertical = S.M).enter(4),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(S.M),
        ) {
            Box(Modifier.size(44.dp).clip(RoundedCornerShape(14.dp)).background(C.Ink), contentAlignment = Alignment.Center) { Lattice(size = 36.dp, tile = null) }
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text("Parakeet 110M", style = T.Title.copy(fontSize = 15.sp), color = C.Ink)
                Text(if (model) "English · by NVIDIA · on this phone" else "English · by NVIDIA · not downloaded", style = T.Caption, color = C.Muted)
            }
            Text("Change", style = T.Label.copy(fontSize = 13.sp), color = C.AccentInk)
        }
    }
}

@Composable
private fun SetupRow(icon: String, tile: Color, title: String, sub: String, fix: String?, error: Boolean = false, onFix: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(start = 14.dp, end = S.S, top = S.S, bottom = S.S),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(S.M),
    ) {
        IconTile(icon, C.Ink, tile, 36.dp)
        Column(Modifier.weight(1f)) {
            Text(title, style = T.Title.copy(fontSize = 15.sp), color = C.Ink)
            Text(sub, style = T.Caption, color = if (error) C.Coral else C.Muted)
        }
        if (fix != null) TonalButton(fix, onFix, fill = C.Accent, ink = C.Ink)
        else Box(Modifier.size(S.Touch), contentAlignment = Alignment.Center) { Lattice(size = 32.dp, sweep = true) }
    }
}

@Composable
private fun StatCard(label: String, value: Int, modifier: Modifier) {
    val v by animateFloatAsState(value.toFloat(), tween(900, easing = Motion.Ease), label = "count")
    Column(modifier.clip(Corners.Item).background(C.Card).padding(14.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(label, style = T.Caption, color = C.Muted)
        Text(v.toInt().toString(), style = T.Numeral.copy(fontSize = 30.sp, lineHeight = 34.sp), color = C.Ink)
    }
}

// ---------------------------------------------------------------- history

@Composable
private fun HistoryScreen(settings: FlowSettings, snacks: Snacks) {
    val context = LocalContext.current
    var query by rememberSaveable { mutableStateOf("") }
    val hidden = remember { mutableStateListOf<HistoryEntry>() } // swiped away, waiting for Undo
    val shown = HistoryStore.items.filter { it !in hidden && (query.isBlank() || it.text.contains(query, true) || it.app.contains(query, true)) }
    val now = dayKey(System.currentTimeMillis())
    val list = rememberLazyListState()
    Column(Modifier.fillMaxSize().imePadding().statusBarsPadding().padding(top = S.M)) {
        PageTitle("History", Modifier.padding(horizontal = S.L))
        if (HistoryStore.items.isEmpty()) {
            EmptyState("Nothing here yet", "Your dictations appear here, as text only. Audio is never saved.")
            return@Column
        }
        SearchField(query, { query = it }, "Search what you said", Modifier.padding(start = S.L, end = S.L, top = 14.dp))
        LazyColumn(Modifier.fillMaxSize(), state = list, contentPadding = PaddingValues(start = S.L, end = S.L, top = 14.dp, bottom = S.XL), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            if (shown.isEmpty()) item("none") { EmptyState("No matches", "Try a different word.") }
            var last = -1
            shown.forEach { e ->
                val k = dayKey(e.at)
                if (k != last) {
                    val first = last == -1
                    last = k
                    val label = when (now - k) { 0 -> "Today"; 1 -> "Yesterday"; else -> java.text.SimpleDateFormat("EEEE, d MMM", java.util.Locale.getDefault()).format(Date(e.at)) }
                    item(key = "d$k") { SectionLabel(label, Modifier.animateItem().padding(top = if (first) 0.dp else S.S, start = 2.dp)) }
                }
                item(key = "${e.at}:${e.text.hashCode()}") {
                    HistoryCard(
                        e, Modifier.animateItem(),
                        onCopy = { copyText(context, e.text); snacks.show("Copied") },
                        onDelete = { hidden += e; snacks.show("Dictation deleted", undo = { hidden -= e }, commit = { hidden -= e; HistoryStore.remove(e) }) },
                    )
                }
            }
            item("foot") {
                Text(
                    "Tap to copy, swipe to delete. Text only, on this phone, kept ${if (settings.historyDays == 0) "for no time" else "for ${settings.historyDays} days"}.",
                    style = T.Caption, color = C.Muted, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth().padding(top = S.S),
                )
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun HistoryCard(e: HistoryEntry, modifier: Modifier, onCopy: () -> Unit, onDelete: () -> Unit) {
    val state = rememberSwipeToDismissBoxState(confirmValueChange = { if (it != SwipeToDismissBoxValue.Settled) { onDelete(); true } else false })
    SwipeToDismissBox(
        state, modifier = modifier,
        backgroundContent = {
            val end = state.dismissDirection == SwipeToDismissBoxValue.EndToStart
            Box(Modifier.fillMaxSize().clip(Corners.Item).background(C.CoralSoft).padding(horizontal = S.XL), contentAlignment = if (end) Alignment.CenterEnd else Alignment.CenterStart) {
                Glyph(Icons.Trash, C.Coral, 22.dp)
            }
        },
    ) {
        Column(
            Modifier.fillMaxWidth().clip(Corners.Item).background(C.Card).clickable(onClickLabel = "Copy", onClick = onCopy)
                .semantics { customActions = listOf(CustomAccessibilityAction("Delete") { onDelete(); true }) }.padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(e.text, style = T.Body.copy(lineHeight = 21.sp), color = C.Ink, maxLines = 8, overflow = TextOverflow.Ellipsis)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(S.S)) {
                KindChip(e.app)
                Text(clock(e.at), style = T.Caption, color = C.Muted)
            }
        }
    }
}

@Composable
internal fun SearchField(value: String, onChange: (String) -> Unit, hint: String, modifier: Modifier = Modifier) {
    Row(
        modifier.fillMaxWidth().heightIn(min = S.Touch).clip(RoundedCornerShape(16.dp)).background(C.Card).padding(start = 14.dp, end = S.XS),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Glyph(Icons.Search, C.Muted, 16.dp, 2.4f)
        Box(Modifier.weight(1f).padding(vertical = S.M)) {
            if (value.isEmpty()) Text(hint, style = T.Body, color = C.Muted, maxLines = 1)
            BasicTextField(
                value, onChange, textStyle = T.Body.copy(color = C.Ink), cursorBrush = SolidColor(C.Accent), singleLine = true,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search), modifier = Modifier.fillMaxWidth().semantics { contentDescription = hint },
            )
        }
        AnimatedVisibility(value.isNotEmpty(), enter = fadeIn(), exit = fadeOut()) {
            Box(Modifier.iconButton("Clear search") { onChange("") }, contentAlignment = Alignment.Center) { Glyph(Icons.Close, C.Muted, 18.dp) }
        }
    }
}

// ---------------------------------------------------------------- notes

@Composable
private fun NotesScreen(onOpen: (Long) -> Unit) {
    val grid = rememberLazyStaggeredGridState()
    Column(Modifier.fillMaxSize().statusBarsPadding().padding(top = S.M)) {
        PageTitle("Notes", Modifier.padding(horizontal = S.L))
        if (NotesStore.items.isEmpty()) {
            EmptyState("No notes yet", "Tap New voice note and talk. Voicio writes it up.")
            return@Column
        }
        LazyVerticalStaggeredGrid(
            columns = StaggeredGridCells.Fixed(2),
            state = grid,
            modifier = Modifier.fillMaxSize(),
            // Clears the floating button (56 dp) with a gutter on each side.
            contentPadding = PaddingValues(start = S.L, end = S.L, top = 14.dp, bottom = S.L + 56.dp + S.L),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalItemSpacing = 10.dp,
        ) {
            itemsIndexed(NotesStore.items, key = { _, n -> n.id }) { i, n ->
                // The artboard's rhythm: a peach card, two white, a lime one.
                val tint = when (i % 4) { 0 -> Panel.Balanced; 3 -> Panel.Lite; else -> null }
                Column(
                    Modifier.animateItem().heightIn(min = 150.dp).clip(RoundedCornerShape(22.dp))
                        .then(if (tint != null) Modifier.panel(tint) else Modifier.background(C.Card))
                        .clickable { onOpen(n.id) }.padding(14.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Text(n.title.ifBlank { "Untitled" }, style = T.Title.copy(fontSize = 15.sp, fontWeight = FontWeight.ExtraBold), color = C.Ink, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    if (n.body.isNotBlank()) Text(n.body, style = T.Body2.copy(fontSize = 13.sp, lineHeight = 18.sp), color = C.InkSoft, maxLines = 8, overflow = TextOverflow.Ellipsis)
                    Text(ago(n.at), style = T.Caption.copy(fontSize = 11.sp), color = C.Muted)
                }
            }
        }
    }
}
