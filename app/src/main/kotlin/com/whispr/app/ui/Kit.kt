package com.whispr.app.ui

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.scale
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.view.WindowCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import kotlin.math.PI
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.sin

// ---------------------------------------------------------------- icons

/** 24x24 stroke icons, the same paths as the design. */
object Icons {
    const val Mic = "M12 3a3 3 0 0 0-3 3v5a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3z M5 11a7 7 0 0 0 14 0 M12 18v3"
    const val Home = "M3 11l9-8 9 8v9a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z"
    const val Clock = "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z M12 7v5l3 2"
    const val Notes = "M6 3h9l4 4v14H6z M9 12h7 M9 16h7"
    const val Book = "M5 4h12a2 2 0 0 1 2 2v14H7a2 2 0 0 1-2-2z M5 18a2 2 0 0 1 2-2h12"
    const val Sliders = "M4 7h10 M18 7h2 M4 17h2 M10 17h10 M16 5a2 2 0 1 0 0 4a2 2 0 1 0 0-4z M8 15a2 2 0 1 0 0 4a2 2 0 1 0 0-4z"
    const val Check = "M5 12l5 5 9-10"
    const val Chevron = "M9 6l6 6-6 6"
    const val Shield = "M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z"
    const val Lock = "M7 11h10a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2z M8 11V8a4 4 0 0 1 8 0v3"
    const val Search = "M11 4a7 7 0 1 0 0 14a7 7 0 1 0 0-14z M20 20l-4-4"
    const val Copy = "M11 9h7a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-7a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2z M5 15V6a2 2 0 0 1 2-2h9"
    const val Plus = "M12 5v14 M5 12h14"
    const val Trash = "M4 7h16 M9 7V4h6v3 M6 7l1 13h10l1-13"
    const val Arrow = "M5 12h14 M13 6l6 6-6 6"
    const val Down = "M12 5v14 M6 13l6 6 6-6"
    const val Person = "M12 3a2 2 0 1 0 0 4a2 2 0 1 0 0-4z M5 9l7 1 7-1 M12 10v5 M9 21l3-6 3 6"
    const val Info = "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z M12 8v5 M12 16h.01"
    const val Layers = "M5 5h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2z M3 10h18"
    const val Back = "M15 6l-6 6 6 6"
    const val Close = "M6 6l12 12 M18 6L6 18"
    const val More = "M12 5h.01 M12 12h.01 M12 19h.01"
    const val Cloud = "M7 18a4 4 0 0 1-.6-7.95A6 6 0 0 1 18 9.5a4.25 4.25 0 0 1-.5 8.5z"
    const val Sparkle = "M12 3l1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9z M19 16l.8 2.2L22 19l-2.2.8L19 22l-.8-2.2L16 19l2.2-.8z"
    const val Battery = "M4 8h13a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V9a1 1 0 0 1 1-1z M21 11v2"
    const val Keyboard = "M3 7h18v11H3z M7 11h1 M11 11h1 M15 11h2 M7 14h10"
    const val Lines = "M4 7h16 M4 12h10 M4 17h13"
    const val Bolt = "M13 3L5 14h6l-1 7 8-11h-6z"
    const val Star = "M12 3l2.5 5.5L20 9l-4 4 1 6-5-3-5 3 1-6-4-4 5.5-.5z"
    const val Cube = "M12 3l8 4.5v9L12 21l-8-4.5v-9z M4 7.5l8 4.5 8-4.5 M12 12v9"
    const val Window = "M4 6h16v12H4z M4 10h16"
    const val Chip = "M8 6h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2z M10 3v3 M14 3v3 M10 18v3 M14 18v3 M3 10h3 M3 14h3 M18 10h3 M18 14h3"
}

@Composable
fun Glyph(path: String, tint: Color, size: Dp = 24.dp, stroke: Float = 2f, modifier: Modifier = Modifier) {
    val parsed = remember(path) { PathParser().parsePathString(path).toPath() }
    Canvas(modifier.size(size)) {
        scale(this.size.minDimension / 24f, pivot = Offset.Zero) {
            drawPath(parsed, tint, style = Stroke(stroke, cap = StrokeCap.Round, join = StrokeJoin.Round))
        }
    }
}

// ---------------------------------------------------------------- motion helpers

/** Scales down a little while pressed, springs back on release. */
fun Modifier.pressable(onClick: () -> Unit): Modifier = composed {
    val source = remember { MutableInteractionSource() }
    val pressed by source.collectIsPressedAsState()
    val s by animateFloatAsState(if (pressed) 0.96f else 1f, Motion.snappy(), label = "press")
    graphicsLayer { scaleX = s; scaleY = s }.clickable(source, indication = null, role = Role.Button, onClick = onClick)
}

/** A 48 dp icon-only button with a spoken label. */
fun Modifier.iconButton(description: String, onClick: () -> Unit): Modifier =
    size(S.Touch).clip(CircleShape).semantics { contentDescription = description }.clickable(role = Role.Button, onClick = onClick)

/** Slides up and fades in, staggered by [index]. Plays once when the item first appears. */
fun Modifier.enter(index: Int = 0, distance: Dp = 16.dp): Modifier = composed {
    val progress = remember { Animatable(0f) }
    LaunchedEffect(Unit) {
        kotlinx.coroutines.delay(index.coerceAtMost(6) * Motion.STAGGER_MS)
        progress.animateTo(1f, Motion.enter())
    }
    graphicsLayer {
        alpha = progress.value
        translationY = (1f - progress.value) * distance.toPx()
    }
}

// ---------------------------------------------------------------- system

internal tailrec fun Context.findActivity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.findActivity()
    else -> null
}

internal fun Context.openAppSettings() = runCatching {
    startActivity(android.content.Intent(android.provider.Settings.ACTION_APPLICATION_DETAILS_SETTINGS, android.net.Uri.parse("package:$packageName")).addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK))
}

/** Opens the Accessibility list with Voicio highlighted. Never crashes, whatever the phone maker allows. */
internal fun openBubbleSettings(context: Context) = com.whispr.app.flow.FlowAccessibilityService.turnBackOn(context)

/** Turns the bubble off for real (as if switched off in Settings). False when it was not on. */
internal fun pauseBubble(): Boolean = com.whispr.app.flow.FlowAccessibilityService.instance?.let { it.disableSelf(); true } ?: false

/** Dark status-bar icons over light screens, light icons over the dark hero and splash. */
@Composable
fun StatusBarIcons(dark: Boolean) {
    val view = LocalView.current
    if (view.isInEditMode) return
    SideEffect {
        view.context.findActivity()?.window?.let { WindowCompat.getInsetsController(it, view).isAppearanceLightStatusBars = dark }
    }
}

/** Ticks each time the app comes back to the foreground, so permission checks re-run after a trip to Settings. */
@Composable
internal fun rememberResumeTick(): Int {
    var tick by remember { mutableIntStateOf(0) }
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    DisposableEffect(lifecycle) {
        val o = LifecycleEventObserver { _, e -> if (e == Lifecycle.Event.ON_RESUME) tick++ }
        lifecycle.addObserver(o)
        onDispose { lifecycle.removeObserver(o) }
    }
    return tick
}

// ---------------------------------------------------------------- brand art

/** Honeycomb palettes: the brand on ink, and two for light illustration panels. */
enum class Comb(val mid: Color, val rim: Color, val rimAlpha: Float, val unlit: Color) {
    /** On an ink tile or capsule: bevelled cream cells round an orange core. */
    Brand(C.Cream, C.Cream, 1f, C.Cream.copy(alpha = 0.16f)),
    /** On a peach panel: bevelled ink cells round the orange core ("warm"). */
    Light(C.Ink, C.Ink, 1f, C.Faint),
    /** On a lime panel (microphone, ready). */
    Lime(Color(0xFF8BCB45), C.Ink, 0.14f, C.Faint),
}

/** One cell of honeycomb v2: axial (q, r), its ring (0..3), angle and centre in units of `s`. */
private class Cell(val q: Int, val r: Int, val ring: Int, val angle: Float, val x: Float, val y: Float)

/** 37 pointy-top hexagons within axial radius 3 (rings of 1, 6, 12, 18), ordered centre out then by angle. */
private val Cells: List<Cell> = buildList {
    for (r in -3..3) for (q in -3..3) {
        val ring = maxOf(kotlin.math.abs(q), kotlin.math.abs(r), kotlin.math.abs(q + r))
        if (ring > 3) continue
        val x = (kotlin.math.sqrt(3.0) * (q + r / 2.0)).toFloat(); val y = 1.5f * r
        add(Cell(q, r, ring, atan2(y, x), x, y))
    }
}.sortedWith(compareBy({ it.ring }, { (it.angle + 2 * PI.toFloat() + PI.toFloat() / 2) % (2 * PI.toFloat()) }))

/** The seven cells that form the tick. */
private val TickCells = setOf(-1 to -1, -1 to 0, -1 to 1, 0 to 0, 1 to -1, 2 to -2, 3 to -3)

/** A pointy-top hexagon of radius 1 with 28 % rounded corners (quadratic corners, as the design's SVG). */
private val UnitHex: androidx.compose.ui.graphics.Path = androidx.compose.ui.graphics.Path().apply {
    val pts = (0 until 6).map { i -> val a = Math.toRadians(60.0 * i - 90); Offset(cos(a).toFloat(), sin(a).toFloat()) }
    val t = 0.28f
    for (i in 0 until 6) {
        val p = pts[i]; val prev = pts[(i + 5) % 6]; val next = pts[(i + 1) % 6]
        val a = p + (prev - p) * t; val b = p + (next - p) * t
        if (i == 0) moveTo(a.x, a.y) else lineTo(a.x, a.y)
        quadraticTo(p.x, p.y, b.x, b.y)
    }
    close()
}

/** Bevel light from the top-left, shade to the bottom-right, in the unit hexagon's box. */
private val CreamBevel = Brush.linearGradient(0f to Color.White, 0.55f to Color(0xFFF1EADB), 1f to Color(0xFFCDBFA7), start = Offset(-0.866f, -1f), end = Offset(0.866f, 1f))
private val InkBevel = Brush.linearGradient(0f to Color(0xFF4A4237), 1f to C.Ink, start = Offset(-0.866f, -1f), end = Offset(0.866f, 1f))
private val WarmOp = floatArrayOf(1f, 0.95f, 0.75f, 0.45f)
private val CoreBevel = Brush.linearGradient(0f to Color(0xFFFFC9AA), 0.45f to C.Core, 1f to Color(0xFFB8380B), start = Offset(-0.866f, -1f), end = Offset(0.866f, 1f))

// Per-ring looks from the design (index = ring 0..3).
private val RestOp = floatArrayOf(0.8f, 0.32f, 0.32f, 0.2f)
private val BrandOp = floatArrayOf(1f, 0.92f, 0.62f, 0.32f)
private val VoiceOp = floatArrayOf(1f, 1f, 0.75f, 0.35f)
private val VoiceK = floatArrayOf(1.12f, 1.04f, 0.96f, 0.88f)

/**
 * The Voicio honeycomb v2, the one shape everywhere (icon, capsule, phone bubble, keyboard): 37 soft bevelled
 * hexagons, cream round an orange core. Moods, as in the design: at rest the brand look ([glow] > 0) or quiet
 * cream; [active] = the voice, the rings lift from the centre out with [level] (60 ms apart, 200 ms ease-out);
 * [noise] = the outer ring turns slate (heard, ignored); [sweep] = an amber arm round the edge while the text is
 * cleaned; [check] = seven cells form a lime tick; [tintAmount] = a still amber/coral notice look;
 * [progress] = cells light up centre-out (downloads).
 */
@Composable
fun Lattice(
    modifier: Modifier = Modifier, size: Dp = 56.dp, active: Boolean = false, level: Float = 0f, tile: Color? = C.Ink,
    sweep: Boolean = false,
    check: Float = 0f,
    tint: Color = C.Working, tintAmount: Float = 0f,
    noise: Float = 0f,
    glow: Float = 0.4f,
    palette: Comb = Comb.Brand,
    /** A rounded-square tile (the app icon) instead of a circle (the orb). */
    square: Boolean = false,
    progress: Float? = null,
) {
    val target = if (active) level.coerceIn(0f, 1f) else 0f
    val live by animateFloatAsState(target, tween(200, easing = androidx.compose.animation.core.FastOutSlowInEasing), label = "voice")

    // Loudness history, newest first, one step per 60 ms: ring n shows the loudness n steps ago.
    val history = remember { FloatArray(4) }
    var frame by remember { mutableIntStateOf(0) }
    val latest by androidx.compose.runtime.rememberUpdatedState(live)
    LaunchedEffect(active) {
        while (active) {
            kotlinx.coroutines.delay(60)
            for (i in 3 downTo 1) history[i] = history[i - 1]
            history[0] = latest
            frame++
        }
    }
    val clock = rememberInfiniteTransition(label = "comb")
    val breath by clock.animateFloat(0f, 1f, infiniteRepeatable(tween(1400, easing = LinearEasing), RepeatMode.Restart), label = "b")
    val spin by clock.animateFloat(0f, 1f, infiniteRepeatable(tween(1100, easing = LinearEasing), RepeatMode.Restart), label = "s")
    val sweepOn by animateFloatAsState(if (sweep) 1f else 0f, tween(200), label = "sweepOn")

    Canvas(modifier.size(size)) {
        frame // redraw whenever the history moves
        val box = this.size.minDimension
        if (tile != null) {
            // "Ink & Ember": the ink tile is a soft 160-degree gradient with a faint top light.
            val brush = if (tile == C.Ink) Brush.linearGradient(0f to C.TileTop, 0.55f to C.Ink, 1f to C.TileBottom, start = Offset(box * 0.33f, 0f), end = Offset(box * 0.67f, box))
            else androidx.compose.ui.graphics.SolidColor(tile)
            if (square) {
                val cr = androidx.compose.ui.geometry.CornerRadius(box * 0.235f)
                drawRoundRect(brush, cornerRadius = cr)
                drawRoundRect(Brush.verticalGradient(listOf(Color.White.copy(alpha = 0.08f), Color.Transparent), 0f, box * 0.5f), cornerRadius = cr)
            } else drawCircle(brush, box / 2)
        }
        // The comb is drawn at a share of the box: 76 % on the icon tile, 70 % in a circle, 92 % bare.
        val s = box * (if (tile == null) 0.92f else if (square) 0.76f else 0.70f) / 11.6f
        val origin = Offset(this.size.width / 2, this.size.height / 2)
        val brand = palette == Comb.Brand
        val calm = !active && progress == null
        if (brand && tile != null && (active || glow > 0f) && tintAmount < 1f && check < 1f) { // warm glow behind the core
            drawCircle(Brush.radialGradient(listOf(C.Core.copy(alpha = 0.4f * (1f - tintAmount) * (1f - check)), Color.Transparent), origin, s * 4f), s * 4f, origin)
        }
        Cells.forEachIndexed { idx, cell ->
            val ring = cell.ring
            // base look
            var op: Float; var k: Float
            when {
                calm && glow <= 0f -> { op = RestOp[ring]; k = 0.92f }
                calm -> { op = if (palette == Comb.Light) WarmOp[ring] else BrandOp[ring]; k = if (palette == Comb.Light && ring == 0) 1.1f else 1f }
                else -> {
                    val lift = (0.35f + 0.65f * history[ring]).coerceIn(0f, 1f) + 0.04f * sin(breath * 2 * PI.toFloat() - ring * 0.9f)
                    op = lerp(RestOp[ring], VoiceOp[ring], lift.coerceIn(0f, 1f)); k = lerp(0.92f, VoiceK[ring], lift)
                }
            }
            var fill: Brush = when {
                ring == 0 -> CoreBevel
                brand -> CreamBevel
                palette == Comb.Light -> InkBevel
                else -> androidx.compose.ui.graphics.SolidColor(if (ring == 1) palette.mid else palette.rim)
            }
            if (palette == Comb.Lime && ring >= 2) op *= palette.rimAlpha * 2.2f
            var flat: Color? = null // a state colour replacing the bevel
            var flatAmount = 0f
            if (progress != null) {
                val lit = idx.toFloat() / Cells.size < progress
                if (lit) { fill = CoreBevel; op = 1f } else { flat = palette.unlit.copy(alpha = 1f); flatAmount = 1f; op = palette.unlit.alpha }
                k = 0.96f
            }
            if (noise > 0f && ring == 3 && check == 0f) {
                flat = Color(0xFF9FB0C2); flatAmount = noise; op = lerp(op, 0.75f, noise); k = lerp(k, 0.9f, noise)
            }
            if (sweepOn > 0f) {
                val w = ((cos(cell.angle + 0.9f - spin * 2 * PI.toFloat()) + 1) / 2)
                val sop = if (ring == 0) 0.9f else if (ring == 3) 0.15f + 0.85f * w else 0.3f
                val sk = if (ring == 3) 0.84f + 0.16f * w else 0.9f
                flat = C.Working; flatAmount = sweepOn; op = lerp(op, sop, sweepOn); k = lerp(k, sk, sweepOn)
            }
            if (check > 0f) {
                val on = (cell.q to cell.r) in TickCells
                if (on) { flat = C.Typed; flatAmount = check }
                op = lerp(op, if (on) 1f else 0.14f, check); k = lerp(k, if (on) 1.04f else 0.88f, check)
            }
            if (tintAmount > 0f) {
                flat = tint; flatAmount = tintAmount; op = lerp(op, if (ring >= 2) 0.45f else 0.9f, tintAmount); k = lerp(k, 0.92f, tintAmount)
            }
            val radius = s * 0.82f * k
            val a = op.coerceIn(0f, 1f)
            val f = flat; val fa = if (f != null) flatAmount else 0f; val base = fill
            withTransform({ translate(origin.x + cell.x * s, origin.y + cell.y * s); scale(radius, radius, Offset.Zero) }) {
                if (fa < 1f) drawPath(UnitHex, base, alpha = a * (1f - fa))
                if (f != null) drawPath(UnitHex, f, alpha = a * fa)
            }
        }
    }
}

private fun lerp(a: Float, b: Float, t: Float) = a + (b - a) * t

/** A soft gradient panel for illustrations (onboarding art, empty states, pack cards). */
@Composable
fun Art(panel: Panel, modifier: Modifier = Modifier, gx: Float = 0.7f, gy: Float = 0.3f, shape: androidx.compose.ui.graphics.Shape = Corners.Stage, content: @Composable BoxScope.() -> Unit) {
    Box(modifier.clip(shape).panel(panel, gx, gy), contentAlignment = Alignment.Center, content = content)
}

/** The ink tile with the honeycomb and the lowercase wordmark. */
@Composable
fun Wordmark(modifier: Modifier = Modifier, mark: Dp = 32.dp, style: androidx.compose.ui.text.TextStyle = T.H2) {
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Lattice(size = mark, square = true)
        Text("voicio", style = style, color = C.Ink)
    }
}

// ---------------------------------------------------------------- buttons

/** The one filled button: ink with white text, 18 dp corners, at least 52 dp tall; large text grows it. */
@Composable
fun PrimaryButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, color: Color = C.Ink, enabled: Boolean = true, ink: Color = Color.White) {
    val bg by animateColorAsState(if (enabled) color else C.Border, Motion.quick(), label = "btn")
    Box(
        modifier.fillMaxWidth().heightIn(min = 52.dp).clip(Corners.Button).background(bg)
            .then(if (enabled) Modifier.pressable(onClick) else Modifier.semantics { contentDescription = text })
            .padding(horizontal = S.XL, vertical = S.M),
        contentAlignment = Alignment.Center,
    ) { Text(text, style = T.Button, color = if (enabled) ink else C.Muted, textAlign = TextAlign.Center) }
}

/** The second choice next to a primary button: soft paper fill, ink text. */
@Composable
fun SoftButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, fill: Color = C.Card, ink: Color = C.Ink) =
    PrimaryButton(text, onClick, modifier, color = fill, ink = ink)

/** A text-only button for the quiet choice. */
@Composable
fun GhostButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, color: Color = C.InkSoft) {
    Box(modifier.fillMaxWidth().heightIn(min = S.Touch).clip(Corners.Button).clickable(role = Role.Button, onClick = onClick).padding(horizontal = S.L, vertical = S.S), contentAlignment = Alignment.Center) {
        Text(text, style = T.Button, color = color, textAlign = TextAlign.Center)
    }
}

/** A small button ("Fix", "Download", "Copy") with a full 48 dp touch target. Orange = get something. */
@Composable
fun TonalButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, fill: Color = C.Soft, ink: Color = C.Ink) {
    Box(modifier.heightIn(min = S.Touch).pressable(onClick), contentAlignment = Alignment.Center) {
        Box(Modifier.heightIn(min = 40.dp).clip(RoundedCornerShape(13.dp)).background(fill).padding(horizontal = 14.dp, vertical = S.S), contentAlignment = Alignment.Center) {
            Text(text, style = T.Label.copy(fontSize = androidx.compose.ui.unit.TextUnit(13f, androidx.compose.ui.unit.TextUnitType.Sp)), color = ink, maxLines = 1)
        }
    }
}

// ---------------------------------------------------------------- surfaces

/** The one card style: white, 24 dp corners, no border. Callers pad the content with [S.L]. */
@Composable
fun CardBox(modifier: Modifier = Modifier, border: Color = Color.Transparent, borderWidth: Dp = 0.dp, fill: Color = C.Card, shape: androidx.compose.ui.graphics.Shape = Corners.Card, content: @Composable ColumnScope.() -> Unit) {
    // Clipped first, so a clickable card's ripple keeps the rounded corners.
    Column(Modifier.clip(shape).then(modifier).background(fill).then(if (borderWidth > 0.dp) Modifier.border(borderWidth, border, shape) else Modifier), content = content)
}

@Composable
fun Chip(text: String, fill: Color = C.Peach, ink: Color = C.AccentDeep) {
    Box(Modifier.heightIn(min = 24.dp).clip(Corners.Pill).background(fill).padding(horizontal = 10.dp, vertical = 4.dp), contentAlignment = Alignment.Center) {
        Text(text, style = T.Label, color = ink, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** A status pill with a dot ("Ready", "Needs setup"). */
@Composable
fun StatusPill(text: String, good: Boolean, modifier: Modifier = Modifier) {
    Row(
        modifier.clip(Corners.Pill).background(if (good) C.Lime else C.Butter).padding(horizontal = 12.dp, vertical = 7.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Box(Modifier.size(8.dp).clip(CircleShape).background(if (good) C.Green else C.Working))
        Text(text, style = T.Label, color = if (good) C.LimeInk else C.ButterInk)
    }
}

@Composable
fun SectionLabel(text: String, modifier: Modifier = Modifier) =
    Text(text.uppercase(), style = T.Overline, color = C.Muted, modifier = modifier.padding(horizontal = S.XS).semantics { heading() })

/** A Bricolage section heading with an optional link on the right ("All models", "History"). */
@Composable
fun SectionTitle(text: String, modifier: Modifier = Modifier, action: String? = null, onAction: () -> Unit = {}) {
    Row(modifier.fillMaxWidth().heightIn(min = 40.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(text, style = T.Section, color = C.Ink, modifier = Modifier.weight(1f).semantics { heading() })
        if (action != null) Box(Modifier.heightIn(min = S.Touch).clip(Corners.Pill).clickable(role = Role.Button, onClick = onAction).padding(horizontal = S.S), contentAlignment = Alignment.Center) {
            Text(action, style = T.Title.copy(fontSize = androidx.compose.ui.unit.TextUnit(14f, androidx.compose.ui.unit.TextUnitType.Sp)), color = C.AccentInk)
        }
    }
}

/** A group heading: tinted icon tile and a Bricolage title, as the desktop settings groups. */
@Composable
fun GroupTitle(text: String, icon: String, fill: Color, modifier: Modifier = Modifier, note: String? = null) {
    Row(modifier.padding(horizontal = S.XS), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        IconTile(icon, C.Ink, fill, 28.dp)
        Text(text, style = T.H2.copy(fontSize = androidx.compose.ui.unit.TextUnit(19f, androidx.compose.ui.unit.TextUnitType.Sp)), color = C.Ink, modifier = Modifier.semantics { heading() })
        if (note != null) Text(note, style = T.Label, color = C.Muted)
    }
}

/** A line between rows of a group, inset 16 dp. */
@Composable
fun RowDivider() = Box(Modifier.padding(horizontal = S.L).fillMaxWidth().height(1.5.dp).background(C.Bg))

/**
 * A row of a group: optional icon tile, title, subtitle and a trailing control. 56 dp tall (72 dp with a
 * subtitle) and grows with large text. With [onClick] and no [trailing] it shows a chevron.
 */
@Composable
fun ListRow(
    title: String,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    icon: String? = null,
    iconTint: Color = C.Ink,
    iconFill: Color = C.PeachTile,
    titleColor: Color = C.Ink,
    onClick: (() -> Unit)? = null,
    trailing: (@Composable RowScope.() -> Unit)? = null,
) {
    Row(
        modifier.fillMaxWidth().heightIn(min = if (subtitle != null) 72.dp else 56.dp)
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = S.L, vertical = S.M),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(S.M),
    ) {
        if (icon != null) IconTile(icon, iconTint, iconFill)
        Column(Modifier.weight(1f)) {
            Text(title, style = T.Title, color = titleColor)
            if (subtitle != null) Text(subtitle, style = T.Body2, color = C.Muted)
        }
        when {
            trailing != null -> trailing()
            onClick != null -> Glyph(Icons.Chevron, C.Muted, 20.dp)
        }
    }
}

@Composable
fun IconTile(icon: String, tint: Color = C.Ink, fill: Color = C.PeachTile, size: Dp = 40.dp) {
    Box(Modifier.size(size).clip(RoundedCornerShape(size * 0.32f)).background(fill), contentAlignment = Alignment.Center) { Glyph(icon, tint, size * 0.5f, 2.2f) }
}

/** A numbered step: one style everywhere. */
@Composable
fun StepNumber(n: String, fill: Color = C.Ink, ink: Color = Color.White) {
    Box(Modifier.size(32.dp).clip(CircleShape).background(fill), contentAlignment = Alignment.Center) { Text(n, style = T.Label, color = ink) }
}

enum class NoticeKind(val fill: Color, val ink: Color, val icon: String) {
    Info(C.Sky, C.SkyInk, Icons.Info),
    Warn(C.Butter, C.ButterInk, Icons.Info),
    Error(C.CoralSoft, C.Coral, Icons.Info),
    Success(C.Lime, C.LimeInk, Icons.Check),
}

/** The one notice card: butter to adjust something, coral for errors, sky for information, lime for good news. */
@Composable
fun Notice(kind: NoticeKind, title: String, modifier: Modifier = Modifier, body: String? = null, action: String? = null, onAction: () -> Unit = {}) {
    CardBox(modifier.fillMaxWidth(), fill = kind.fill, shape = Corners.Item) {
        Row(Modifier.padding(start = S.L, end = S.L, top = S.L, bottom = if (action == null) S.L else 0.dp), horizontalArrangement = Arrangement.spacedBy(S.M)) {
            Glyph(kind.icon, kind.ink, 22.dp, 2.2f)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(title, style = T.Title, color = kind.ink)
                if (body != null) Text(body, style = T.Body2, color = C.InkSoft)
            }
        }
        if (action != null) {
            Box(Modifier.fillMaxWidth().padding(end = S.S, bottom = S.XS), contentAlignment = Alignment.CenterEnd) {
                TonalButton(action, onAction, fill = C.Card, ink = C.Ink)
            }
        }
    }
}

/** Empty lists: the honeycomb on a small peach panel, a title and one line of help. */
@Composable
fun EmptyState(title: String, body: String, modifier: Modifier = Modifier) {
    // Sits a little above the middle of the free space (in a list, where height is unbounded, at the top).
    Column(
        modifier.fillMaxWidth().fillMaxHeight(0.8f).padding(horizontal = S.XXL, vertical = S.XXL).enter(),
        horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center,
    ) {
        Art(Panel.Peach, Modifier.size(132.dp), shape = RoundedCornerShape(36.dp)) { Lattice(size = 76.dp, square = true) }
        Text(title, style = T.H2, color = C.Ink, textAlign = TextAlign.Center, modifier = Modifier.padding(top = S.XL))
        Text(body, style = T.Body2, color = C.Muted, textAlign = TextAlign.Center, modifier = Modifier.padding(top = S.XS))
    }
}

// ---------------------------------------------------------------- chrome

/**
 * The one top bar: paper, at least 64 dp under the status bar. Tabs use a large Bricolage title; sub-screens
 * get a back arrow and a smaller title. [lifted] draws a hairline once content scrolls under it.
 */
@Composable
fun TopBar(
    title: String,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    onBack: (() -> Unit)? = null,
    lifted: Boolean = false,
    actions: @Composable RowScope.() -> Unit = {},
) {
    val line by animateColorAsState(if (lifted) C.Border else C.Bg, Motion.quick(), label = "lift")
    Column(modifier.fillMaxWidth().background(C.Bg).statusBarsPadding()) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(start = if (onBack != null) S.XS else S.L + S.XS, end = S.XS, top = S.S, bottom = S.S),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (onBack != null) Box(Modifier.iconButton("Back", onBack), contentAlignment = Alignment.Center) { Glyph(Icons.Back, C.Ink, 20.dp, 2.4f) }
            Column(Modifier.weight(1f).padding(start = if (onBack != null) S.XS else 0.dp, end = S.S)) {
                Text(title, style = T.H1.copy(fontSize = 26.sp, lineHeight = 30.sp, letterSpacing = (-0.8).sp), color = C.Ink, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.semantics { heading() })
                if (subtitle != null) Text(subtitle, style = T.Body2, color = C.Muted, modifier = Modifier.padding(top = 2.dp))
            }
            actions()
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(line))
    }
}

// ---------------------------------------------------------------- inputs

/** The one text field: white, 18 dp corners, a 2 dp paper border that turns ink while focused. */
@Composable
internal fun Field(
    value: String, onChange: (String) -> Unit, hint: String, modifier: Modifier = Modifier, minLines: Int = 1,
    capitalization: KeyboardCapitalization = KeyboardCapitalization.Sentences, mask: Boolean = false, singleLine: Boolean = mask,
    textStyle: androidx.compose.ui.text.TextStyle = T.Body,
    /** Always an ink border (the artboards' practice box). */
    focusInk: Boolean = false,
    /** A soft well without a border (Home's "Try it here"). */
    soft: Boolean = false,
) {
    val source = remember { MutableInteractionSource() }
    val focused by source.collectIsFocusedAsState()
    val border by animateColorAsState(when { focusInk || focused -> C.Ink; soft -> C.Soft; else -> C.Border }, Motion.quick(), label = "field")
    val shape = if (soft || focusInk) RoundedCornerShape(14.dp) else Corners.Button
    Box(
        modifier.fillMaxWidth().heightIn(min = 52.dp).clip(shape).background(if (soft) C.Soft else C.Card).border(2.dp, border, shape)
            .padding(horizontal = S.L, vertical = 14.dp),
        contentAlignment = Alignment.CenterStart,
    ) {
        if (value.isEmpty()) Text(hint, style = textStyle, color = C.Muted)
        BasicTextField(
            value, onChange, textStyle = textStyle.copy(color = C.Ink), cursorBrush = SolidColor(C.Accent), minLines = minLines, singleLine = singleLine,
            interactionSource = source,
            keyboardOptions = if (mask) KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false, keyboardType = KeyboardType.Password, imeAction = ImeAction.Done)
            else KeyboardOptions(capitalization = capitalization, imeAction = if (singleLine) ImeAction.Done else ImeAction.Default),
            visualTransformation = if (mask) PasswordVisualTransformation() else VisualTransformation.None,
            modifier = Modifier.fillMaxWidth().semantics { contentDescription = hint },
        )
    }
}

/** A whole row that toggles: title, optional subtitle, and the switch. */
@Composable
fun SwitchRow(title: String, subtitle: String?, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = if (subtitle != null) 72.dp else 56.dp).toggleable(checked, role = Role.Switch, onValueChange = onChange)
            .padding(start = S.L, end = S.S, top = S.M, bottom = S.M),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(S.M),
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, style = T.Title, color = C.Ink)
            if (subtitle != null) Text(subtitle, style = T.Body2, color = C.Muted)
        }
        Toggle(checked, null)
    }
}

/** The one confirmation dialog for anything that cannot be undone. */
@Composable
fun ConfirmDialog(title: String, body: String, confirm: String, onDismiss: () -> Unit, destructive: Boolean = true, onConfirm: () -> Unit) {
    androidx.compose.material3.AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = C.Card,
        title = { Text(title, style = T.H2, color = C.Ink) },
        text = { Text(body, style = T.Body, color = C.InkSoft) },
        confirmButton = { androidx.compose.material3.TextButton({ onDismiss(); onConfirm() }) { Text(confirm, style = T.Button, color = if (destructive) C.Coral else C.AccentInk) } },
        dismissButton = { androidx.compose.material3.TextButton(onDismiss) { Text("Cancel", style = T.Button, color = C.Muted) } },
    )
}

/** An on/off switch: orange when on, paper when off; the knob springs across. The touch target is 48 dp. */
@Composable
fun Toggle(checked: Boolean, onChange: ((Boolean) -> Unit)?) {
    val x by animateDpAsState(if (checked) 25.dp else 3.dp, Motion.soft(), label = "knob")
    val bg by animateColorAsState(if (checked) C.Accent else C.ToggleOff, Motion.quick(), label = "track")
    Box(
        Modifier.size(56.dp, S.Touch).then(if (onChange != null) Modifier.toggleable(checked, role = Role.Switch, onValueChange = onChange) else Modifier),
        contentAlignment = Alignment.Center,
    ) {
        Box(Modifier.size(52.dp, 30.dp).clip(CircleShape).background(bg)) {
            Box(Modifier.offset(x, 3.dp).size(24.dp).shadow(1.5.dp, CircleShape).clip(CircleShape).background(Color.White))
        }
    }
}

/** A segmented choice with a sliding white selection. Each segment is a 48 dp target. */
@Composable
fun Segmented(options: List<String>, selected: Int, fill: Boolean = false, onSelect: (Int) -> Unit) {
    BoxWithConstraints(if (fill) Modifier.fillMaxWidth() else Modifier) {
        val inset = 4.dp
        val segW = if (fill) maxWidth / options.size else 88.dp
        val x by animateDpAsState(segW * selected, Motion.snappy(), label = "seg")
        // The row sets the height (large text may wrap a label onto two lines); the pill follows it.
        Box(Modifier.width(segW * options.size).clip(Corners.Control).background(C.Track)) {
            Box(Modifier.matchParentSize()) {
                Box(Modifier.offset(x, 0.dp).width(segW).fillMaxHeight().padding(inset).shadow(2.dp, RoundedPill).clip(RoundedPill).background(C.Card))
            }
            Row(Modifier.height(IntrinsicSize.Min)) {
                options.forEachIndexed { i, label ->
                    val ink by animateColorAsState(if (i == selected) C.Ink else C.Muted, Motion.quick(), label = "segInk")
                    Box(
                        Modifier.width(segW).fillMaxHeight().heightIn(min = S.Touch).selectable(i == selected, role = Role.Tab) { onSelect(i) }.padding(horizontal = S.S, vertical = S.S),
                        contentAlignment = Alignment.Center,
                    ) { Text(label, style = T.Label.copy(fontSize = androidx.compose.ui.unit.TextUnit(13f, androidx.compose.ui.unit.TextUnitType.Sp)), color = ink, textAlign = TextAlign.Center, maxLines = 2) }
                }
            }
        }
    }
}

private val RoundedPill = RoundedCornerShape(11.dp)

@Composable
fun Spacer(h: Dp) = androidx.compose.foundation.layout.Spacer(Modifier.height(h))
