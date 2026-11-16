package com.whispr.app.ime

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.whispr.app.R
import com.whispr.app.flow.NoticeKind
import com.whispr.app.flow.Phase
import com.whispr.app.ui.C
import com.whispr.app.ui.Glyph
import com.whispr.app.ui.Lattice

/** Colours of one keyboard theme (artboard "Android: Voicio Keyboard"). */
data class KeyPalette(
    val dark: Boolean, val bg: Color, val key: Color, val keyPressed: Color, val keyActive: Color, val mod: Color,
    val legend: Color, val hint: Color, val accent: Color, val onAccent: Color, val accentText: Color,
    val barText: Color, val icon: Color, val chip: Color, val chipText: Color, val orb: Color, val orbRing: Color,
)

val InkKeys = KeyPalette(
    dark = true, bg = Color(0xFF16130F), key = Color(0xFF2A2620), keyPressed = Color(0xFF46403A), keyActive = Color(0xFF4A4236), mod = Color(0xFF3A342B),
    legend = Color(0xFFF3EDE1), hint = Color(0xFF8F8676), accent = C.Accent, onAccent = C.Ink, accentText = C.Core,
    barText = Color(0xFFE9E1D2), icon = Color(0xFFB9AF9E), chip = Color(0xFF2A2620), chipText = Color(0xFFE9E1D2), orb = Color(0xFF2A2620), orbRing = Color(0x40FF7A45),
)

val PaperKeys = KeyPalette(
    dark = false, bg = Color(0xFFF3EFE6), key = Color.White, keyPressed = Color(0xFFEDE6DA), keyActive = Color(0xFFD5CBB9), mod = Color(0xFFE3DACB),
    legend = C.Ink, hint = Color(0xFF8A8272), accent = C.Accent, onAccent = C.Ink, accentText = C.AccentInk,
    barText = C.InkSoft, icon = C.Muted, chip = Color.White, chipText = C.Ink, orb = C.Ink, orbRing = Color.Transparent,
)

// The listening panel is the same ink capsule in both themes (it is the brand surface, like the bubble).
private val PanelBg = Color(0xFF221E19)
private val PanelButton = Color(0xFF2F2A23)
private val PanelText = Color(0xFFEFE7D7)
private val PanelTail = Color(0xFF8F8676)

/** The whole keyboard. Stateless: the service owns [ui] and handles [actions]. */
@Composable
fun KeyboardView(ui: ImeUi, actions: ImeActions) {
    val p = if (ui.dark) InkKeys else PaperKeys
    val f = ui.flow
    val listening = f.phase == Phase.Warming || f.phase == Phase.Listening
    val panel = ui.field.voice && (listening || f.phase == Phase.Working || f.phase == Phase.Done || f.notice != null)
    val sidePanel = ui.layer.panel
    val body: @Composable ColumnScope.() -> Unit = {
        when {
            ui.field == FieldKind.Password -> SecureLine(p)
            !ui.field.voice -> Spacer(Modifier.height(8.dp))
            panel && !sidePanel -> VoicePanel(ui, actions)
            else -> FlowBar(ui, actions, p)
        }
        val keysAlpha by animateFloatAsState(if (listening && !sidePanel) 0.35f else 1f, tween(220), label = "dim")
        when (ui.layer) {
            Layer.Emoji -> EmojiPanel(ui, actions, p)
            Layer.Clipboard -> ClipPanel(ui, actions, p)
            Layer.Menu -> MenuPanel(ui, actions, p)
            Layer.History -> HistoryPanel(ui, actions, p)
            else -> OneHanded(ui, actions, p) { KeyArea(ui, actions, p, Modifier.alpha(keysAlpha)) }
        }
    }
    if (ui.floating) Floating(ui, actions, p, body)
    else Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp)).background(p.bg).navigationBarsPadding().padding(bottom = 6.dp), content = body)
}

/** Floating keyboard: a card you drag by its handle; the rest of the window is transparent and passes touches through. */
@Composable
private fun Floating(ui: ImeUi, actions: ImeActions, p: KeyPalette, body: @Composable ColumnScope.() -> Unit) {
    val a by rememberUpdatedState(actions)
    androidx.compose.foundation.layout.BoxWithConstraints(Modifier.fillMaxSize()) {
        val boxW = constraints.maxWidth
        val boxH = constraints.maxHeight
        val margin = with(androidx.compose.ui.platform.LocalDensity.current) { 12.dp.roundToPx() }
        val cardW = minOf(maxWidth - 16.dp, if (ui.landscape) 520.dp else 400.dp)
        var size by remember { mutableStateOf(androidx.compose.ui.unit.IntSize.Zero) }
        var moved by remember { mutableStateOf<androidx.compose.ui.unit.IntOffset?>(if (ui.floatX >= 0 && ui.floatY >= 0) androidx.compose.ui.unit.IntOffset(ui.floatX, ui.floatY) else null) }
        fun clamp(x: Int, y: Int) = androidx.compose.ui.unit.IntOffset(x.coerceIn(0, maxOf(0, boxW - size.width)), y.coerceIn(0, maxOf(0, boxH - size.height)))
        fun pos() = moved?.let { clamp(it.x, it.y) } ?: clamp((boxW - size.width) / 2, boxH - size.height - margin)
        Column(
            Modifier.offset { pos() }.width(cardW)
                .onSizeChanged { size = it }
                .onGloballyPositioned { c -> val r = c.boundsInWindow(); a.floatBounds(r.left.toInt(), r.top.toInt(), r.right.toInt(), r.bottom.toInt()) }
                .shadow(18.dp, RoundedCornerShape(24.dp)).clip(RoundedCornerShape(24.dp)).background(p.bg).padding(bottom = 6.dp),
        ) {
            Row(
                Modifier.fillMaxWidth().height(30.dp).pointerInput(Unit) {
                    detectDragGestures(
                        onDrag = { change, drag -> change.consume(); val c = pos(); moved = clamp(c.x + drag.x.toInt(), c.y + drag.y.toInt()) },
                        onDragEnd = { moved?.let { a.floatMoved(it.x, it.y) } },
                    )
                }.semantics { contentDescription = "Move keyboard" },
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Spacer(Modifier.weight(1f))
                Box(Modifier.size(width = 40.dp, height = 5.dp).clip(RoundedCornerShape(3.dp)).background(p.hint))
                Box(Modifier.weight(1f), contentAlignment = Alignment.CenterEnd) {
                    Text(stringResource(R.string.ime_dock), Modifier.padding(end = 6.dp).clip(RoundedCornerShape(12.dp)).clickable(role = Role.Button) { a.floating(false) }.padding(horizontal = 10.dp, vertical = 4.dp),
                        style = TextStyle(fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = p.accentText))
                }
            }
            body()
        }
    }
}

// ------------------------------------------------------------ bars

@Composable
private fun SecureLine(p: KeyPalette) {
    Row(Modifier.fillMaxWidth().height(50.dp).semantics(mergeDescendants = true) {}, horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
        Glyph(KeyIcons.Lock, Color(0xFFB9AF9E).takeIf { p.dark } ?: C.Muted, 15.dp, stroke = 2.2f)
        Spacer(Modifier.size(8.dp))
        Text(stringResource(R.string.ime_password), style = TextStyle(fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = if (p.dark) Color(0xFFB9AF9E) else C.Muted))
    }
}

/** Idle: clipboard, a suggestion, the honeycomb mic, a suggestion, the style chip. */
@Composable
private fun FlowBar(ui: ImeUi, actions: ImeActions, p: KeyPalette) {
    val a by rememberUpdatedState(actions)
    val swipe = Modifier.pointerInput(Unit) {
        val threshold = 28.dp.toPx()
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
            var fired = false
            while (true) {
                val c = awaitPointerEvent(PointerEventPass.Initial).changes.firstOrNull { it.id == down.id } ?: break
                if (!fired && down.position.y - c.position.y > threshold) { fired = true; a.feedback(Feedback.Tick); a.layer(Layer.History) }
                if (fired) c.consume()
                if (!c.pressed) break
            }
        }
    }
    Row(Modifier.fillMaxWidth().height(if (ui.landscape) 52.dp else 62.dp).then(swipe).padding(horizontal = 8.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
        IconKey(KeyIcons.Clipboard, stringResource(R.string.ime_clipboard), p.icon, { actions.layer(Layer.Clipboard) })
        Slot(p) {
            val s = ui.suggestions.getOrNull(0)
            val w = ui.newWords.getOrNull(0)
            when {
                w != null && s == null -> Chip("+ $w", p) { actions.addWord(w) }
                ui.copied != null -> Chip(stringResource(R.string.ime_copy), p) { actions.copyAgain() }
                ui.flow.canUndo -> Chip(stringResource(R.string.ime_undo), p, KeyIcons.Undo) { actions.undo() }
                s != null -> Suggestion(s, p) { actions.suggestion(s) }
                ui.recentClip != null -> Chip(ui.recentClip.text.lines().first().take(18), p, KeyIcons.Clipboard) { actions.paste(ui.recentClip) }
                else -> IconKey(KeyIcons.Emoji, stringResource(R.string.ime_emoji), p.icon, { actions.layer(Layer.Emoji) })
            }
        }
        Orb(ui, actions, p)
        Slot(p) {
            val s = ui.suggestions.getOrNull(1)
            val w = ui.newWords.getOrNull(1)
            if (s != null) Suggestion(s, p) { actions.suggestion(s) }
            else if (w != null && ui.suggestions.isEmpty()) Chip("+ $w", p) { actions.addWord(w) }
            else IconKey(KeyIcons.OneHand, stringResource(R.string.ime_one_handed), p.icon, { actions.oneHanded(if (ui.oneHanded == 0) 1 else 0) })
        }
        if (ui.cloud != null) IconKey(com.whispr.app.ui.Icons.Cloud, stringResource(R.string.ime_cloud) + ": " + ui.cloud, p.icon, { actions.openApp() }, size = 34.dp, background = p.chip, glyph = 16.dp)
        if (ui.style.isNotEmpty()) {
            Box(
                Modifier.height(34.dp).clip(RoundedCornerShape(17.dp)).background(p.chip)
                    .semantics { stateDescription = ui.styleWhy }
                    .combinedClickable(role = Role.Button, onLongClickLabel = "Tools", onLongClick = { actions.feedback(Feedback.Long); actions.openMenu() }) { actions.nextStyle() }
                    .padding(horizontal = 12.dp),
                contentAlignment = Alignment.Center,
            ) { Text(ui.style, style = TextStyle(fontSize = 12.sp, fontWeight = FontWeight.Bold, color = p.chipText)) }
        }
    }
}

@Composable
private fun RowScope.Slot(p: KeyPalette, content: @Composable () -> Unit) =
    Box(Modifier.weight(1f).height(44.dp), contentAlignment = Alignment.Center) { content() }

@Composable
private fun Suggestion(word: String, p: KeyPalette, onClick: () -> Unit) {
    Text(
        word,
        Modifier.fillMaxSize().clip(RoundedCornerShape(12.dp)).clickable(role = Role.Button, onClick = onClick).padding(top = 12.dp),
        style = TextStyle(fontSize = 15.sp, fontWeight = FontWeight.Medium, color = p.barText, textAlign = TextAlign.Center),
        maxLines = 1, overflow = TextOverflow.Ellipsis,
    )
}

@Composable
private fun Chip(label: String, p: KeyPalette, icon: String? = null, onClick: () -> Unit) {
    Row(
        Modifier.height(34.dp).clip(RoundedCornerShape(17.dp)).background(p.chip).clickable(role = Role.Button, onClick = onClick).padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        if (icon != null) Glyph(icon, p.icon, 14.dp)
        Text(label, style = TextStyle(fontSize = 12.sp, fontWeight = FontWeight.Bold, color = p.chipText), maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** The 56 dp honeycomb mic: tap = talk until Done, hold = talk while held, double-tap = continuous. */
@Composable
private fun Orb(ui: ImeUi, actions: ImeActions, p: KeyPalette) {
    val a by rememberUpdatedState(actions)
    Box(
        Modifier.size(if (ui.landscape) 44.dp else 56.dp)
            .then(if (p.dark) Modifier else Modifier.shadow(8.dp, CircleShape))
            .clip(CircleShape).background(p.orb).border(1.dp, p.orbRing, CircleShape)
            .semantics { role = Role.Button; contentDescription = "Voicio"; onClick(label = "Dictate") { a.micDown(); a.micUp(); true } }
            .pointerInput(Unit) { awaitEachGesture { awaitFirstDown().consume(); a.micDown(); waitForUpOrCancellation(); a.micUp() } },
        contentAlignment = Alignment.Center,
    ) { Lattice(size = if (ui.landscape) 34.dp else 44.dp, tile = null) }
}

/** Listening, working, typed and notices: Cancel, the live honeycomb, the words, Done. */
@Composable
private fun VoicePanel(ui: ImeUi, actions: ImeActions) {
    val f = ui.flow
    val listening = f.phase == Phase.Warming || f.phase == Phase.Listening
    val check by animateFloatAsState(if (f.phase == Phase.Done && f.notice == null) 1f else 0f, tween(260), label = "typed")
    val notice = f.notice
    Row(
        Modifier.fillMaxWidth().padding(start = 8.dp, end = 8.dp, top = 8.dp).clip(RoundedCornerShape(22.dp)).background(PanelBg).padding(12.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically,
    ) {
        IconKey(KeyIcons.Close, stringResource(android.R.string.cancel), PanelText, { actions.cancel() }, size = 48.dp, background = PanelButton, glyph = 18.dp, stroke = 2.6f)
        Lattice(
            size = 54.dp, tile = null, active = f.phase == Phase.Listening, level = f.level,
            sweep = f.phase == Phase.Working, check = check,
            tint = if (notice?.kind == NoticeKind.Error) C.CombError else C.Working,
            tintAmount = if (notice != null && notice.kind != NoticeKind.Info) 1f else 0f,
        )
        Box(Modifier.weight(1f).semantics { liveRegion = LiveRegionMode.Polite }) {
            when {
                notice != null -> Text(notice.text, style = TextStyle(fontSize = 15.sp, lineHeight = 20.sp, fontWeight = FontWeight.Medium, color = when (notice.kind) {
                    NoticeKind.Error -> C.CombError; NoticeKind.Warn -> C.Working; NoticeKind.Info -> PanelText
                }), maxLines = 2, overflow = TextOverflow.Ellipsis)
                f.phase == Phase.Working -> Text(stringResource(R.string.ime_working), style = TextStyle(fontSize = 16.sp, color = PanelTail))
                f.live.isEmpty() -> Column {
                    Text(stringResource(if (ui.continuous) R.string.ime_continuous else R.string.ime_listening), style = TextStyle(fontSize = 16.sp, color = PanelTail))
                    Privacy(ui)
                }
                else -> LiveWords(f.live)
            }
        }
        when {
            listening -> IconKey(KeyIcons.Done, stringResource(R.string.ime_stop), C.Ink, { actions.done() }, size = 48.dp, background = C.Accent, glyph = 20.dp, stroke = 3f)
            notice?.retry == true -> IconKey(KeyIcons.Undo, "Retry", C.Ink, { actions.retry() }, size = 48.dp, background = C.Accent, glyph = 20.dp, stroke = 2.6f)
            f.canUndo -> IconKey(KeyIcons.Undo, stringResource(R.string.ime_undo), PanelText, { actions.undo() }, size = 48.dp, background = PanelButton, glyph = 20.dp, stroke = 2.4f)
            else -> Spacer(Modifier.size(48.dp))
        }
    }
}

/** Where the audio goes, always visible while listening ("On device" or "Cloud · Groq"). */
@Composable
private fun Privacy(ui: ImeUi) {
    Text(
        ui.privacy + if (ui.whisper) " · Whisper" else "",
        style = TextStyle(fontSize = 11.sp, fontWeight = FontWeight.SemiBold, letterSpacing = 0.4.sp, color = if (ui.privacy.startsWith("Cloud")) C.Working else Color(0xFF8F8676)),
    )
}

/** The last two lines of words; the newest two words dimmed, as they may still change. */
@Composable
private fun LiveWords(live: String) {
    val words = live.trim().split(' ')
    val tailFrom = maxOf(0, words.size - 2)
    val text = buildAnnotatedString {
        append(words.subList(0, tailFrom).joinToString(" "))
        if (tailFrom > 0) append(' ')
        withStyle(SpanStyle(color = PanelTail)) { append(words.subList(tailFrom, words.size).joinToString(" ")) }
    }
    Text(text, style = TextStyle(fontSize = 16.sp, lineHeight = 21.sp, color = PanelText), maxLines = 2, overflow = TextOverflow.Clip,
        modifier = Modifier.fillMaxWidth())
}

// ------------------------------------------------------------ one-handed

@Composable
private fun OneHanded(ui: ImeUi, actions: ImeActions, p: KeyPalette, keys: @Composable () -> Unit) {
    if (ui.oneHanded == 0 || !ui.field.voice) { keys(); return }
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        val controls = @Composable {
            Column(Modifier.weight(0.16f), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(16.dp)) {
                IconKey(KeyIcons.Expand, stringResource(R.string.ime_full_width), p.icon, { actions.oneHanded(0) }, background = p.mod)
                IconKey(KeyIcons.Swap, stringResource(R.string.ime_move_side), p.icon, { actions.oneHanded(-ui.oneHanded) }, background = p.mod)
            }
        }
        if (ui.oneHanded > 0) controls()
        Box(Modifier.weight(0.84f)) { keys() }
        if (ui.oneHanded < 0) controls()
    }
}

// ------------------------------------------------------------ emoji

@Composable
private fun EmojiPanel(ui: ImeUi, actions: ImeActions, p: KeyPalette) {
    val drawable = remember { val paint = android.graphics.Paint(); EmojiSet.categories.map { (tab, cps) -> tab to cps.map(EmojiSet::glyph).filter { runCatching { paint.hasGlyph(it) }.getOrDefault(true) } } }
    var tab by remember { mutableIntStateOf(if (ui.recentEmoji.isEmpty()) 1 else 0) }
    val tabs = listOf("🕘" to ui.recentEmoji) + drawable
    Column(Modifier.fillMaxWidth().height(panelHeight(ui)).padding(horizontal = 6.dp)) {
        LazyVerticalGrid(GridCells.Adaptive(44.dp), Modifier.weight(1f).fillMaxWidth()) {
            items(tabs[tab].second) { e ->
                Box(Modifier.size(44.dp).clip(RoundedCornerShape(10.dp)).clickable { actions.emoji(e) }, contentAlignment = Alignment.Center) {
                    Text(e, style = TextStyle(fontSize = 26.sp))
                }
            }
        }
        Row(Modifier.fillMaxWidth().height(48.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(2.dp)) {
            Box(Modifier.height(40.dp).clip(RoundedCornerShape(11.dp)).background(p.mod).clickable { actions.layer(Layer.Letters) }.padding(horizontal = 12.dp), contentAlignment = Alignment.Center) {
                Text("ABC", style = TextStyle(fontSize = 14.sp, fontWeight = FontWeight.Bold, color = p.legend))
            }
            tabs.forEachIndexed { i, (label, _) ->
                Box(Modifier.weight(1f).height(40.dp).clip(RoundedCornerShape(10.dp)).background(if (i == tab) p.key else Color.Transparent).clickable { tab = i }, contentAlignment = Alignment.Center) {
                    Text(label, style = TextStyle(fontSize = 18.sp))
                }
            }
            IconKey(KeyIcons.Back, stringResource(R.string.ime_backspace), p.legend, { actions.backspace() }, background = p.mod, glyph = 20.dp)
        }
    }
}

/** Emoji and clipboard panels take the place of the keys at the same height, so the app does not jump. */
private fun panelHeight(ui: ImeUi) = (50.dp * ui.height) * 4 + 8.dp * 3 + 12.dp + if (ui.numberRow) 48.dp * ui.height else 0.dp

// ------------------------------------------------------------ clipboard

@Composable
private fun ClipPanel(ui: ImeUi, actions: ImeActions, p: KeyPalette) {
    Column(Modifier.fillMaxWidth().height(panelHeight(ui)).padding(horizontal = 10.dp)) {
        Row(Modifier.fillMaxWidth().height(48.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.height(36.dp).clip(RoundedCornerShape(11.dp)).background(p.mod).clickable { actions.layer(Layer.Letters) }.padding(horizontal = 12.dp), contentAlignment = Alignment.Center) {
                Text("ABC", style = TextStyle(fontSize = 14.sp, fontWeight = FontWeight.Bold, color = p.legend))
            }
            Spacer(Modifier.size(12.dp))
            Text(stringResource(R.string.ime_clipboard), style = TextStyle(fontSize = 15.sp, fontWeight = FontWeight.Bold, color = p.legend))
        }
        if (ui.clips.isEmpty()) {
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                Text(stringResource(R.string.ime_clipboard_empty), style = TextStyle(fontSize = 13.sp, color = p.hint, textAlign = TextAlign.Center), modifier = Modifier.padding(24.dp))
            }
        } else LazyColumn(Modifier.weight(1f).fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            items(ui.clips, key = { it.text }) { c ->
                Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(p.key).clickable { actions.paste(c) }.padding(start = 14.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(c.text, Modifier.weight(1f).padding(vertical = 10.dp), style = TextStyle(fontSize = 14.sp, lineHeight = 19.sp, color = p.legend), maxLines = 2, overflow = TextOverflow.Ellipsis)
                    IconKey(KeyIcons.Pin, stringResource(if (c.pinned) R.string.ime_unpin else R.string.ime_pin), if (c.pinned) p.accentText else p.hint, { actions.pin(c) }, glyph = 16.dp)
                }
            }
        }
    }
}

// ------------------------------------------------------------ tools menu and history

@Composable
private fun PanelHeader(title: String, p: KeyPalette, actions: ImeActions) {
    Row(Modifier.fillMaxWidth().height(48.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.height(36.dp).clip(RoundedCornerShape(11.dp)).background(p.mod).clickable { actions.layer(Layer.Letters) }.padding(horizontal = 12.dp), contentAlignment = Alignment.Center) {
            Text("ABC", style = TextStyle(fontSize = 14.sp, fontWeight = FontWeight.Bold, color = p.legend))
        }
        Spacer(Modifier.size(12.dp))
        Text(title, style = TextStyle(fontSize = 15.sp, fontWeight = FontWeight.Bold, color = p.legend))
    }
}

@Composable
private fun Toggle(label: String, on: Boolean, p: KeyPalette, enabled: Boolean = true, onClick: () -> Unit) {
    Box(
        Modifier.height(38.dp).clip(RoundedCornerShape(19.dp)).background(if (on) p.accent else p.key).alpha(if (enabled) 1f else 0.4f)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick).padding(horizontal = 14.dp),
        contentAlignment = Alignment.Center,
    ) { Text(label, style = TextStyle(fontSize = 13.sp, fontWeight = FontWeight.Bold, color = if (on) p.onAccent else p.legend)) }
}

/** Long-press the style chip: rewrite on device, whisper mode, auto-stop, one-handed, theme, and where audio goes. */
@Composable
private fun MenuPanel(ui: ImeUi, actions: ImeActions, p: KeyPalette) {
    Column(Modifier.fillMaxWidth().height(panelHeight(ui)).verticalScroll(rememberScrollState()).padding(horizontal = 10.dp).padding(bottom = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        PanelHeader(stringResource(R.string.ime_tools), p, actions)
        Text(stringResource(if (ui.rewriting) R.string.ime_rewriting else R.string.ime_rewrite_title), style = TextStyle(fontSize = 12.sp, fontWeight = FontWeight.Bold, letterSpacing = 0.8.sp, color = p.hint))
        val labels = mapOf(
            Rewrite.Formal to R.string.ime_style_formal, Rewrite.Casual to R.string.ime_style_casual, Rewrite.Shorter to R.string.ime_rewrite_shorter,
            Rewrite.Grammar to R.string.ime_rewrite_grammar, Rewrite.List to R.string.ime_style_list, Rewrite.Email to R.string.ime_style_email,
        )
        Wrap {
            Rewrite.entries.forEach { k ->
                Toggle(stringResource(labels.getValue(k)), false, p, enabled = !ui.rewriting && (ui.rewriteModel || !k.needsModel)) { actions.rewrite(k) }
            }
        }
        if (!ui.rewriteModel) Text(stringResource(R.string.ime_rewrite_needs_model), style = TextStyle(fontSize = 12.sp, lineHeight = 16.sp, color = p.hint))
        Wrap {
            Toggle(stringResource(R.string.ime_whisper), ui.whisper, p) { actions.toggleWhisper() }
            Toggle(stringResource(R.string.ime_auto_stop) + ": " + when (ui.autoStopMs) { 0L -> "Off"; 1500L -> "1.5 s"; else -> "${ui.autoStopMs / 1000} s" }, ui.autoStopMs > 0, p) { actions.cycleAutoStop() }
            Toggle(stringResource(R.string.ime_one_handed), ui.oneHanded != 0, p) { actions.oneHanded(if (ui.oneHanded == 0) 1 else 0) }
            Toggle(stringResource(R.string.ime_floating), ui.floating, p) { actions.floating(!ui.floating) }
            Toggle(if (ui.dark) "Paper" else "Ink", false, p) { actions.theme() }
        }
        Text(ui.privacy + if (ui.incognito) " · " + stringResource(R.string.ime_incognito) else "", style = TextStyle(fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = p.hint))
    }
}

@Composable
private fun Wrap(content: @Composable () -> Unit) {
    androidx.compose.foundation.layout.FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) { content() }
}

/** Swipe up on the bar: the last 20 dictations, tap to type one again. */
@Composable
private fun HistoryPanel(ui: ImeUi, actions: ImeActions, p: KeyPalette) {
    Column(Modifier.fillMaxWidth().height(panelHeight(ui)).padding(horizontal = 10.dp)) {
        PanelHeader(stringResource(R.string.ime_history), p, actions)
        if (ui.history.isEmpty()) {
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                Text(stringResource(if (ui.incognito) R.string.ime_incognito else R.string.ime_history_empty), style = TextStyle(fontSize = 13.sp, color = p.hint, textAlign = TextAlign.Center), modifier = Modifier.padding(24.dp))
            }
        } else LazyColumn(Modifier.weight(1f).fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            items(ui.history) { t ->
                Text(t, Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(p.key).clickable { actions.reinsert(t) }.padding(horizontal = 14.dp, vertical = 10.dp),
                    style = TextStyle(fontSize = 14.sp, lineHeight = 19.sp, color = p.legend), maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}
