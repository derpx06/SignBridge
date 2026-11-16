package com.whispr.app.ime

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.zIndex
import com.whispr.app.R
import com.whispr.app.ui.Glyph
import kotlin.math.abs
import kotlin.math.roundToInt

/** Key glyph paths (24x24 strokes, same family as ui/Kit.kt Icons). */
internal object KeyIcons {
    const val Shift = "M12 4l8 8h-4.5v7h-7v-7H4z"
    const val ShiftLock = "M12 4l8 8h-4.5v4h-7v-4H4z M8.5 20h7"
    const val Back = "M9 5h11v14H9l-6-7z M12.5 9.5l5 5 M17.5 9.5l-5 5"
    const val Return = "M19 5v7a2 2 0 0 1-2 2H6 M10 10l-4 4 4 4"
    const val Send = "M4 12l16-8-6 16-3-7z M11 13l9-9"
    const val Search = "M11 4a7 7 0 1 0 0 14a7 7 0 1 0 0-14z M20 20l-4-4"
    const val Go = "M5 12h14 M13 6l6 6-6 6"
    const val Next = "M9 6l6 6-6 6"
    const val Done = "M5 12l5 5 9-10"
    const val Globe = "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z M3 12h18 M12 3c3 3.5 3 14.5 0 18 M12 3c-3 3.5-3 14.5 0 18"
    const val Emoji = "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z M8.5 14.5c1.8 2 5.2 2 7 0 M9 9.5h.01 M15 9.5h.01"
    const val Clipboard = "M8 5h8a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2z M9 5V3h6v2"
    const val OneHand = "M7 4h10a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z M9 16h6"
    const val Close = "M6 6l12 12 M18 6L6 18"
    const val Lock = "M8 11h8a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2v-5a2 2 0 0 1 2-2z M9 11V8a3 3 0 0 1 6 0v3"
    const val Pin = "M9 4h6l-1 6 3 3H7l3-3z M12 13v7"
    const val Expand = "M4 9V4h5 M20 9V4h-5 M4 15v5h5 M20 15v5h-5"
    const val Swap = "M7 7h12 M15 3l4 4-4 4 M17 17H5 M9 13l-4 4 4 4"
    const val Space = "M5 11v3h14v-3"
    const val Undo = "M9 14L4 9l5-5 M4 9h10a6 6 0 0 1 0 12h-3"
}

private val KeyShape = RoundedCornerShape(11.dp)

/** The key board: rows of keys, 50 dp tall (times the height setting), 6 dp apart, as on the artboard. */
@Composable
fun KeyArea(ui: ImeUi, actions: ImeActions, p: KeyPalette, modifier: Modifier = Modifier) {
    val rows = Layouts.board(ui.layer, ui.field, ui.shift, ui.numberRow, ui.globe)
    val keyH = 50.dp * ui.height
    Column(modifier.fillMaxWidth().padding(start = 6.dp, end = 6.dp, top = 4.dp, bottom = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        rows.forEachIndexed { r, row ->
            val numberRowShown = ui.numberRow && ui.layer == Layer.Letters && ui.field.voice && r == 0
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                // the top letter row shows its long-press digit in the corner when there is no number row
                val hints = !ui.numberRow && ui.layer == Layer.Letters && r == 0 && ui.field.voice
                row.forEach { k -> Key(k, ui, actions, p, if (numberRowShown) keyH * 0.8f else keyH, hint = if (hints) k.alts.firstOrNull() else null) }
            }
        }
    }
}

@Composable
private fun RowScope.Key(k: KeySpec, ui: ImeUi, actions: ImeActions, p: KeyPalette, height: Dp, hint: String?) {
    if (k.kind == K.Spacer) { Spacer(Modifier.weight(k.weight)); return }
    val modifierKey = k.kind !in setOf(K.Char, K.Space)
    val bg = when {
        k.kind == K.Enter -> p.accent
        k.kind == K.Shift && ui.shift != Shift.Off -> p.keyActive
        modifierKey -> p.mod
        else -> p.key
    }
    val description = when (k.kind) {
        K.Shift -> stringResource(R.string.ime_shift)
        K.Back -> stringResource(R.string.ime_backspace)
        K.Space -> stringResource(R.string.ime_space)
        K.Enter -> ui.enter.name
        K.Globe -> stringResource(R.string.ime_globe)
        K.Emoji -> stringResource(R.string.ime_emoji)
        K.ToLetters -> stringResource(R.string.ime_letters)
        K.ToSymbols, K.ToSymbols2 -> stringResource(R.string.ime_symbols)
        else -> k.label
    }
    var pressed by remember { mutableStateOf(false) }
    var altIndex by remember { mutableIntStateOf(-1) }
    val density = LocalDensity.current
    val gesture = keyGesture(k, actions, density.run { 30.dp.toPx() }, density.run { 10.dp.toPx() }, density.run { 28.dp.toPx() },
        onPressed = { pressed = it }, onAlt = { altIndex = it })
    Box(
        Modifier.weight(k.weight).height(height).zIndex(if (pressed) 2f else 0f)
            .semantics { role = Role.Button; contentDescription = description; onClick { tap(k, actions); true } }
            .then(gesture),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier.matchParentSize().shadow(if (p.dark) 0.dp else 1.dp, KeyShape, clip = false).clip(KeyShape)
                .background(if (pressed && k.kind != K.Enter) p.keyPressed else bg)
                .drawBehind { if (p.dark) drawLine(Color.White.copy(alpha = 0.06f), Offset(size.width * 0.15f, 0.5f), Offset(size.width * 0.85f, 0.5f), 1f) },
        )
        KeyFace(k, ui, p)
        if (hint != null) Text(hint, Modifier.align(Alignment.TopEnd).padding(top = 3.dp, end = 5.dp), style = TextStyle(fontSize = 9.sp, color = p.hint))
        if (pressed && k.kind == K.Char && altIndex < 0) Preview(k.label, p, height)
        if (altIndex >= 0) Alternates(k.alts, altIndex, p, height)
    }
}

@Composable
private fun KeyFace(k: KeySpec, ui: ImeUi, p: KeyPalette) {
    val legend = if (k.kind == K.Enter) p.onAccent else p.legend
    when (k.kind) {
        K.Char -> Text(k.label, style = TextStyle(fontSize = 20.sp, fontWeight = FontWeight.Medium, color = p.legend))
        K.Space -> if (ui.field == FieldKind.Number || ui.field == FieldKind.Phone) Glyph(KeyIcons.Space, p.legend, 22.dp)
            else Text(stringResource(R.string.ime_subtype_en).substringBefore(' '), style = TextStyle(fontSize = 13.sp, color = p.hint))
        K.ToLetters, K.ToSymbols, K.ToSymbols2 -> Text(k.label, style = TextStyle(fontSize = 14.sp, fontWeight = FontWeight.Bold, color = p.legend))
        K.Shift -> Glyph(if (ui.shift == Shift.Lock) KeyIcons.ShiftLock else KeyIcons.Shift, if (ui.shift != Shift.Off) p.accentText else legend, 20.dp)
        K.Back -> Glyph(KeyIcons.Back, legend, 22.dp)
        K.Globe -> Glyph(KeyIcons.Globe, legend, 20.dp)
        K.Emoji -> Glyph(KeyIcons.Emoji, legend, 20.dp)
        K.Enter -> Glyph(when (ui.enter) {
            EnterKind.Send -> KeyIcons.Send; EnterKind.Search -> KeyIcons.Search; EnterKind.Go -> KeyIcons.Go
            EnterKind.Next -> KeyIcons.Next; EnterKind.Done -> KeyIcons.Done; EnterKind.Return -> KeyIcons.Return
        }, legend, 22.dp, stroke = 2.6f)
        K.Spacer -> Unit
    }
}

/** The pressed letter, magnified above the key. */
@Composable
private fun Preview(label: String, p: KeyPalette, height: Dp) {
    Box(
        Modifier.wrapContentSize(unbounded = true).offset(y = -height + 4.dp).size(width = 44.dp, height = height)
            .shadow(6.dp, KeyShape).clip(KeyShape).background(p.keyPressed),
        contentAlignment = Alignment.Center,
    ) { Text(label, style = TextStyle(fontSize = 26.sp, fontWeight = FontWeight.Medium, color = p.legend)) }
}

/** Long-press alternatives; slide sideways to pick one, lift to type it. */
@Composable
private fun Alternates(alts: List<String>, selected: Int, p: KeyPalette, height: Dp) {
    Row(
        Modifier.wrapContentSize(unbounded = true, align = Alignment.CenterStart).offset(y = -height - 2.dp)
            .shadow(8.dp, KeyShape).clip(KeyShape).background(p.mod).padding(3.dp),
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        alts.forEachIndexed { i, a ->
            Box(Modifier.size(width = 30.dp, height = height - 6.dp).clip(RoundedCornerShape(8.dp)).background(if (i == selected) p.accent else Color.Transparent), contentAlignment = Alignment.Center) {
                Text(a, style = TextStyle(fontSize = 20.sp, fontWeight = FontWeight.Medium, color = if (i == selected) p.onAccent else p.legend))
            }
        }
    }
}

/** What a plain tap does (also TalkBack's double-tap). */
private fun tap(k: KeySpec, a: ImeActions) = when (k.kind) {
    K.Char -> a.text(k.out)
    K.Shift -> a.shift()
    K.Back -> a.backspace()
    K.Space -> a.space()
    K.Enter -> a.enter()
    K.ToSymbols -> a.layer(Layer.Symbols)
    K.ToSymbols2 -> a.layer(Layer.Symbols2)
    K.ToLetters -> a.layer(Layer.Letters)
    K.Globe -> a.globe()
    K.Emoji -> a.layer(Layer.Emoji)
    K.Spacer -> Unit
}

private fun feedbackFor(k: KeySpec) = when (k.kind) {
    K.Back -> Feedback.Delete
    K.Space -> Feedback.Space
    K.Enter -> Feedback.Enter
    else -> Feedback.Key
}

/**
 * Touch handling per key kind:
 * - letters: tap types; hold 380 ms shows alternatives, slide to choose, lift to type.
 * - backspace: tap deletes; hold repeats letters, then whole words after 1.5 s; swipe left selects words, lift deletes them.
 * - space: tap types a space; slide moves the caret; hold opens the keyboard picker.
 */
@Composable
private fun keyGesture(
    k: KeySpec, actions: ImeActions, swipeStartPx: Float, cursorStepPx: Float, wordStepPx: Float,
    onPressed: (Boolean) -> Unit, onAlt: (Int) -> Unit,
): Modifier {
    val a by rememberUpdatedState(actions)
    val spec by rememberUpdatedState(k)
    return Modifier.pointerInput(k.kind, k.label) {
        awaitEachGesture {
            val down = awaitFirstDown()
            down.consume()
            val key = spec
            onPressed(true)
            a.feedback(feedbackFor(key))
            try {
                when (key.kind) {
                    K.Back -> {
                        val start = System.currentTimeMillis()
                        var last = start
                        var repeated = false
                        var selecting = false
                        var words = 0
                        while (true) {
                            val ev = withTimeoutOrNull(40) { awaitPointerEvent() }
                            val now = System.currentTimeMillis()
                            if (ev != null) {
                                val c = ev.changes.first()
                                if (!c.pressed) break
                                val dx = c.position.x - down.position.x
                                if (!selecting && dx < -swipeStartPx && !repeated) selecting = true
                                if (selecting) {
                                    val n = if (dx < -swipeStartPx) ((-dx - swipeStartPx) / wordStepPx).toInt() + 1 else 0
                                    if (n != words) { words = n; a.selectWordsBack(n); a.feedback(Feedback.Tick) }
                                }
                                c.consume()
                                continue
                            }
                            if (selecting) continue
                            val held = now - start
                            if (held >= 1500 && now - last >= 160) { a.deleteWord(); a.feedback(Feedback.Delete); last = now; repeated = true }
                            else if (held in 400..1499 && now - last >= 55) { a.backspace(); last = now; repeated = true }
                        }
                        when {
                            selecting -> a.finishSelect(delete = words > 0)
                            !repeated -> a.backspace()
                        }
                    }
                    K.Space -> {
                        var moved = 0
                        var dragging = false
                        var held = false
                        while (true) {
                            val ev = withTimeoutOrNull(550) { awaitPointerEvent() }
                            if (ev == null) { if (!dragging && !held) { held = true; a.feedback(Feedback.Long); a.picker() }; continue }
                            val c = ev.changes.first()
                            if (!c.pressed) break
                            val dx = c.position.x - down.position.x
                            if (!dragging && abs(dx) > cursorStepPx) dragging = true
                            if (dragging) {
                                val steps = (dx / cursorStepPx).toInt()
                                if (steps != moved) { a.cursor(steps - moved); moved = steps; a.feedback(Feedback.Tick) }
                            }
                            c.consume()
                        }
                        if (!dragging && !held) a.space()
                    }
                    K.Char -> {
                        var cancelled = false
                        val up = withTimeoutOrNull(380) { waitForUpOrCancellation().also { if (it == null) cancelled = true } }
                        if (up != null) a.text(key.out)
                        else if (!cancelled && key.alts.isNotEmpty()) {
                            a.feedback(Feedback.Long)
                            var index = 0
                            onAlt(0)
                            while (true) {
                                val c = awaitPointerEvent().changes.first()
                                if (!c.pressed) { a.text(key.alts[index]); break }
                                val i = ((c.position.x - down.position.x) / (32.dp.toPx())).roundToInt().coerceIn(0, key.alts.lastIndex)
                                if (i != index) { index = i; onAlt(i) }
                                c.consume()
                            }
                        } else if (!cancelled) {
                            waitForUpOrCancellation()?.let { a.text(key.out) }
                        }
                    }
                    else -> {
                        if (waitForUpOrCancellation() != null) tap(key, a)
                    }
                }
            } finally {
                onPressed(false)
                onAlt(-1)
            }
        }
    }
}

/** A round 40 dp icon button for the bar and panels. */
@Composable
internal fun IconKey(path: String, description: String, tint: Color, onClick: () -> Unit, modifier: Modifier = Modifier, size: Dp = 40.dp, background: Color = Color.Transparent, glyph: Dp = 18.dp, stroke: Float = 2f) {
    val click by rememberUpdatedState(onClick)
    Box(
        modifier.size(size).clip(RoundedCornerShape(50)).background(background)
            .semantics { role = Role.Button; contentDescription = description; onClick { click(); true } }
            .pointerInput(Unit) { awaitEachGesture { awaitFirstDown().consume(); if (waitForUpOrCancellation() != null) click() } },
        contentAlignment = Alignment.Center,
    ) { Glyph(path, tint, glyph, stroke = stroke) }
}

