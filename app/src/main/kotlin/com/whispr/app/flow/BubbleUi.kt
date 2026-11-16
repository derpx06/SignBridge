package com.whispr.app.flow

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandHorizontally
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkHorizontally
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.widthIn
import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.draw.scale
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.lerp
import androidx.compose.ui.unit.sp
import com.whispr.app.R
import com.whispr.app.ui.C
import com.whispr.app.ui.Glyph
import com.whispr.app.ui.Icons
import com.whispr.app.ui.Lattice
import com.whispr.app.ui.Motion
import com.whispr.app.ui.T
import com.whispr.app.ui.pressable

// Overlay tokens (android-field-ux.md §3), "Paper & Ink": an ink capsule, cream words, an orange Done.
private val OverlayBg = C.OverlayBg
private val OverlayRim = Color.Transparent
/** The idle orb's rim is stronger, so it also stands out over dark apps. */
private val OrbRim = Color(0x40FF7A45)
private val OverlayInk = C.CapsuleText
private val OverlayMuted = C.OverlayMuted
private val OverlayGo = C.OverlayGo
private val OverlayCancel = C.InkButton
private val OverlayWarn = C.OverlayWarn
private val OverlayError = C.OverlayError
private val OverlayInfo = C.OverlayInfo

private const val CloseGlyph = "M6 6l12 12 M18 6L6 18"
private const val UndoGlyph = "M9 15L4 10l5-5 M4 10h10a5 5 0 0 1 0 10h-2"

/** The idle orb, and the transparent margin round every window so the shadow is not clipped. */
val OrbSize = 56.dp
val ShadowPad = 8.dp
internal val PillHeight = 60.dp
/** Room kept above the pill for the live-words popup (two lines + padding + gap), so the window size is constant. */
internal val PopupReserve = 68.dp
private val PopupText = T.Body.copy(fontSize = 14.sp, lineHeight = 20.sp)

/** Live words are shown from the END: the newest words stay visible and the oldest scroll away. */
fun tailOf(text: String, max: Int = 96): String {
    val t = text.trim()
    if (t.length <= max) return t
    val cut = t.takeLast(max)
    val whole = cut.substringAfter(' ', cut) // start at a word boundary, not mid-word
    return "…$whole"
}

/** True while the field shows the full bar rather than the orb. */
internal fun FlowState.wantsBar() = phase != Phase.Idle || notice != null

/**
 * The floating dictation field. Idle: a 56 dp orb docked to the screen edge (tap = hands-free,
 * hold = push-to-talk; the touches are handled by [OverlayHost], this only reports where the orb is
 * via [onGrab]). Dictating: a keyboard-wide bar that grows out of the orb's corner, with Cancel, the
 * lattice, two lines of live words and Done. Then a check, and back to the orb with an Undo pill.
 *
 * [onBarLayout] tells the service which shape the window has (the bar lingers while it folds back),
 * so it can position the window for it.
 */
@Composable
fun FlowField(
    state: FlowState,
    visible: Boolean,
    dockRight: Boolean,
    barWidth: Dp,
    onBarLayout: (Boolean) -> Unit,
    onGrab: (android.graphics.Rect) -> Unit,
    onOrbClick: () -> Unit,
    onCancel: () -> Unit,
    onDone: () -> Unit,
    onRetry: () -> Unit,
    onUndo: () -> Unit,
    popupAbove: Boolean = true,
) {
    val wantBar = state.wantsBar()
    val grow = remember { Animatable(if (wantBar) 1f else 0f) }
    LaunchedEffect(wantBar) { grow.animateTo(if (wantBar) 1f else 0f, Motion.snappy()) }
    val barLayout = wantBar || grow.value > 0f
    SideEffect { onBarLayout(barLayout) }

    // Whole window springs in and out when a field gains or loses focus.
    val shown = remember { Animatable(0f) }
    LaunchedEffect(visible) { shown.animateTo(if (visible) 1f else 0f, Motion.soft()) }
    val rootMod = Modifier
        .scale(0.6f + 0.4f * shown.value.coerceAtMost(1.1f))
        .alpha(shown.value.coerceIn(0f, 1f))
        .padding(ShadowPad)

    if (barLayout) Bar(state, rootMod, grow.value, wantBar, dockRight, barWidth, popupAbove, onGrab, onCancel, onDone, onRetry)
    else Orb(state, rootMod, dockRight, onGrab, onOrbClick, onUndo)
}

@Composable
private fun Orb(
    state: FlowState, modifier: Modifier, dockRight: Boolean,
    onGrab: (android.graphics.Rect) -> Unit, onOrbClick: () -> Unit, onUndo: () -> Unit,
) {
    val orbLabel = stringResource(R.string.field_orb_description)
    val orbAction = stringResource(R.string.field_orb_action)
    val undo = state.canUndo && state.phase == Phase.Idle && state.notice == null
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        if (undo && dockRight) UndoPill(onUndo)
        Box(
            Modifier
                .size(OrbSize)
                .onGloballyPositioned { onGrab(it.boundsInWindow().toAndroid()) }
                // No alpha layer here: it would clip the shadow to a square.
                .shadow(6.dp, CircleShape, ambientColor = Color.Black.copy(alpha = 0.35f), spotColor = Color.Black.copy(alpha = 0.35f))
                .clip(CircleShape)
                .background(OverlayBg)
                .border(1.dp, OrbRim, CircleShape)
                .semantics {
                    contentDescription = orbLabel
                    role = Role.Button
                    onClick(orbAction) { onOrbClick(); true }
                },
            contentAlignment = Alignment.Center,
        ) {
            Lattice(size = 46.dp, tile = null, glow = 0.6f)
        }
        if (undo && !dockRight) UndoPill(onUndo)
    }
}

@Composable
private fun UndoPill(onUndo: () -> Unit) {
    val label = stringResource(R.string.field_undo_description)
    Box(
        // Not clipped: a clip here would cut the pill's shadow into a box.
        Modifier.height(48.dp).semantics { contentDescription = label }.pressable(onUndo),
        contentAlignment = Alignment.Center,
    ) {
        Row(
            Modifier.height(40.dp).shadow(4.dp, CircleShape).clip(CircleShape).background(OverlayBg).border(1.dp, OverlayRim, CircleShape)
                .padding(start = 12.dp, end = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Glyph(UndoGlyph, OverlayInk, size = 18.dp)
            Text(stringResource(R.string.field_undo), style = T.Label.copy(fontSize = 14.sp), color = OverlayInk)
        }
    }
}

@Composable
private fun Bar(
    state: FlowState, modifier: Modifier, grow: Float, wantBar: Boolean, dockRight: Boolean, barWidth: Dp, popupAbove: Boolean,
    onGrab: (android.graphics.Rect) -> Unit, onCancel: () -> Unit, onDone: () -> Unit, onRetry: () -> Unit,
) {
    val listening = state.phase == Phase.Listening || state.phase == Phase.Warming
    val working = state.phase == Phase.Working
    val done = state.phase == Phase.Done && state.notice == null
    val kind = state.notice?.kind
    val alert = kind == NoticeKind.Warn || kind == NoticeKind.Error
    val accent = if (kind == NoticeKind.Error) OverlayError else OverlayWarn

    val rim by animateColorAsState(if (alert) accent else OverlayRim, tween(180), label = "rim")
    val rimWidth by animateDpAsState(if (alert) 2.dp else 1.dp, tween(180), label = "rimW")
    val tintAmount by animateFloatAsState(if (alert) 1f else 0f, tween(180), label = "tint")
    val check by animateFloatAsState(if (done) 1f else 0f, tween(260), label = "check")
    val noise by animateFloatAsState(if (listening && state.level < 0.1f) 0.7f else 0f, tween(300), label = "noise")
    val statusText = stringResource(if (state.phase == Phase.Warming) R.string.field_starting else R.string.field_listening)
    val shape = RoundedCornerShape(PillHeight / 2)
    val words = state.notice?.text ?: state.live.takeIf { it.isNotBlank() }

    // Words live in their own small popup above the pill, so the pill stays compact and the buttons stay clear.
    Column(
        // Fixed size: the window never resizes as words or buttons come and go (the popup fades inside reserved space).
        modifier.width(barWidth).alpha(grow.coerceIn(0f, 1f)),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        if (popupAbove) {
        Column(Modifier.fillMaxWidth().height(PopupReserve), verticalArrangement = if (popupAbove) Arrangement.Bottom else Arrangement.Top, horizontalAlignment = Alignment.CenterHorizontally) {
            AnimatedVisibility(wantBar && words != null, enter = fadeIn(tween(160)), exit = fadeOut(tween(120))) {
                Box(Modifier.padding(top = if (popupAbove) 0.dp else 8.dp, bottom = if (popupAbove) 8.dp else 0.dp)) { LivePopup(state) }
            }
        }
        }
        Row(
            Modifier.heightIn(min = PillHeight).animateContentSize()
                .shadow(8.dp, shape, ambientColor = Color.Black.copy(alpha = 0.35f), spotColor = Color.Black.copy(alpha = 0.35f))
                .clip(shape).background(OverlayBg).border(rimWidth, rim, shape)
                .padding(horizontal = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            AnimatedVisibility(!done, enter = fadeIn() + expandHorizontally(), exit = fadeOut() + shrinkHorizontally()) {
                val label = stringResource(if (state.notice != null && !listening && !working) R.string.field_dismiss else R.string.field_cancel)
                RoundButton(label, OverlayCancel, onCancel, size = 44.dp) { Glyph(CloseGlyph, OverlayInk, size = 20.dp, stroke = 2.4f) }
            }
            Box(
                Modifier.size(48.dp).onGloballyPositioned { onGrab(it.boundsInWindow().toAndroid()) }
                    .semantics { if (words == null) { contentDescription = statusText; liveRegion = LiveRegionMode.Polite } },
                contentAlignment = Alignment.Center,
            ) {
                Lattice(
                    size = 40.dp, active = listening || working, level = if (working) 0.3f else state.level, tile = null,
                    sweep = working, check = check, tint = accent, tintAmount = tintAmount, noise = noise, glow = 0.4f,
                )
            }
            if (state.cloud) CloudChip()
            val slot = when {
                state.notice?.retry == true -> Slot.Retry
                listening && state.notice == null -> Slot.Done
                else -> Slot.None
            }
            AnimatedVisibility(slot == Slot.Done, enter = fadeIn() + expandHorizontally(), exit = fadeOut() + shrinkHorizontally()) {
                RoundButton(stringResource(R.string.field_done), OverlayGo, onDone, size = 48.dp) { Glyph(Icons.Check, C.Ink, size = 24.dp, stroke = 3f) }
            }
            AnimatedVisibility(slot == Slot.Retry, enter = fadeIn() + expandHorizontally(), exit = fadeOut() + shrinkHorizontally()) {
                Box(
                    Modifier.height(44.dp).clip(CircleShape).background(accent.copy(alpha = 0.16f))
                        .clickable(role = Role.Button, onClick = onRetry).padding(horizontal = 16.dp),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(stringResource(R.string.field_retry), style = T.Button, color = accent)
                }
            }
        }
        if (!popupAbove) {
        Column(Modifier.fillMaxWidth().height(PopupReserve), verticalArrangement = if (popupAbove) Arrangement.Bottom else Arrangement.Top, horizontalAlignment = Alignment.CenterHorizontally) {
            AnimatedVisibility(wantBar && words != null, enter = fadeIn(tween(160)), exit = fadeOut(tween(120))) {
                Box(Modifier.padding(top = if (popupAbove) 0.dp else 8.dp, bottom = if (popupAbove) 8.dp else 0.dp)) { LivePopup(state) }
            }
        }
        }
    }
}

private enum class Slot { None, Done, Retry }

@Composable
private fun RoundButton(label: String, fill: Color, onClick: () -> Unit, size: Dp = 48.dp, icon: @Composable () -> Unit) {
    Box(
        Modifier.size(size).clip(CircleShape).background(fill)
            .semantics { contentDescription = label }
            .clickable(role = Role.Button, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { icon() }
}

/** The newest two lines of what is being said (or the notice), in a small ink popup; older words fade out at the top. */
@Composable
private fun LivePopup(state: FlowState) {
    val notice = state.notice
    val (text, ink) = if (notice != null) {
        notice.text to when (notice.kind) { NoticeKind.Warn -> OverlayWarn; NoticeKind.Error -> OverlayError; NoticeKind.Info -> OverlayInfo }
    } else tailOf(state.live, 200) to (if (state.phase == Phase.Working) OverlayInk.copy(alpha = 0.6f) else OverlayInk)
    var lines by remember { mutableIntStateOf(1) }
    val twoLines = with(LocalDensity.current) { PopupText.lineHeight.toDp() * 2 }
    Box(
        Modifier
            .widthIn(max = 260.dp)
            .shadow(6.dp, RoundedCornerShape(16.dp), ambientColor = Color.Black.copy(alpha = 0.3f), spotColor = Color.Black.copy(alpha = 0.3f))
            .clip(RoundedCornerShape(16.dp)).background(OverlayBg)
            .padding(horizontal = 14.dp, vertical = 10.dp)
            .heightIn(max = twoLines)
            .clipToBounds()
            .graphicsLayer(compositingStrategy = CompositingStrategy.Offscreen)
            .drawWithContent {
                drawContent()
                if (lines > 2) {
                    val fade = (10.dp.toPx() / size.height).coerceIn(0f, 1f)
                    drawRect(Brush.verticalGradient(0f to Color.Transparent, fade to Color.Black), blendMode = BlendMode.DstIn)
                }
            }
            .semantics { liveRegion = LiveRegionMode.Polite },
        contentAlignment = Alignment.BottomStart,
    ) {
        Text(
            text, style = PopupText, color = ink,
            modifier = Modifier.wrapContentHeight(Alignment.Bottom, unbounded = true),
            onTextLayout = { lines = it.lineCount },
        )
    }
}

@Composable
private fun CloudChip() {
    val label = stringResource(R.string.field_cloud_used)
    Text(
        stringResource(R.string.field_cloud), color = OverlayInfo, style = T.Label.copy(fontWeight = FontWeight.SemiBold, fontSize = 11.sp, lineHeight = 14.sp), maxLines = 1,
        modifier = Modifier.clip(CircleShape).background(OverlayInfo.copy(alpha = 0.14f)).padding(horizontal = 8.dp, vertical = 3.dp)
            .semantics { contentDescription = label },
    )
}

private fun androidx.compose.ui.geometry.Rect.toAndroid() = android.graphics.Rect(left.toInt(), top.toInt(), right.toInt(), bottom.toInt())
