package com.whispr.app.ui

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.whispr.app.R

/**
 * Voicio's colours, "Paper & Ink": warm paper, ink text, one orange accent, and soft lime, sky and butter
 * tints for meaning. No violet, no dark heroes. No screen may use a raw `Color(0x…)`: add a token here instead.
 */
object C {
    val Bg = Color(0xFFF3EFE6)
    val Card = Color(0xFFFFFFFF)
    /** The soft surface inside white cards (fields' wells, list items, secondary buttons). */
    val Soft = Color(0xFFF7F3EA)
    val Ink = Color(0xFF16130F)
    /** The ink tile's gradient ends ("Ink & Ember" icon). */
    val TileTop = Color(0xFF2E2822)
    val TileBottom = Color(0xFF0C0A08)
    /** Body text on tints and secondary copy that must stay strong. */
    val InkSoft = Color(0xFF3D372E)
    val Muted = Color(0xFF5E574B)
    val Border = Color(0xFFE8E2D6)
    val Line = Color(0xFFEFE9DD)
    /** Placeholder bars in illustrations, inactive cells. */
    val Faint = Color(0x1A16130F)
    /** Segmented-control track on a white card. */
    val Track = Color(0xFFF3EFE6)
    val ToggleOff = Color(0xFFE3DDD0)
    /** Secondary buttons on paper ("Back"), segmented tracks. */
    val Sand = Color(0xFFE9E3D7)
    /** Unfilled steps of the setup progress bar. */
    val Step = Color(0xFFDDD5C6)
    /** An unticked selection circle; placeholder bars in art. */
    val Unticked = Color(0xFFE3DACB)
    val Dash = Color(0xFFCFC6B6)
    val Chevron = Color(0xFF8A8272)
    /** The Models settings tile (the one cool tint in the artboards). */
    val IrisTile = Color(0xFFEBE4FF)
    /** Keys and buttons on the ink keyboard and capsule. */
    val InkKey = Color(0xFF2A2620)
    val InkButton = Color(0xFF2F2A23)
    /** Words in the ink capsule, and the newest (still settling) words. */
    val CapsuleText = Color(0xFFEFE7D7)
    val CapsuleDim = Color(0xFF8F8676)

    val Accent = Color(0xFFF2541B)
    /** Orange text on light surfaces (links, overlines). */
    val AccentInk = Color(0xFFB83A0E)
    val AccentDeep = Color(0xFF8A2A08)
    val AccentSoft = Color(0xFFFFE3D3)
    val Peach = AccentSoft
    val PeachTile = Color(0xFFFFD8C2)

    val Lime = Color(0xFFEAF8D2)
    val LimeInk = Color(0xFF2F5B0B)
    val Green = Color(0xFF4E9A13)
    val GreenSoft = Lime
    val Sky = Color(0xFFE2F0FF)
    val SkyTile = Color(0xFFD9ECFF)
    val SkyInk = Color(0xFF1F4E8C)
    val Butter = Color(0xFFFFF0B8)
    val ButterTile = Color(0xFFFFE7A8)
    val ButterInk = Color(0xFF6B4E00)
    val LimeTile = Color(0xFFE6F7C4)
    val Amber = ButterInk
    val AmberSoft = Butter
    val AmberInk = ButterInk
    val Coral = Color(0xFFB42318)
    val CoralSoft = Color(0xFFFDECEA)

    // The honeycomb (the brand mark, the capsule and the phone bubble draw it).
    val Cream = Color(0xFFF7F3EA)
    val Core = Color(0xFFFF7A45)
    val NoiseRim = Color(0xFFB9C3CF)
    val Working = Color(0xFFFFC24B)
    val Typed = Color(0xFFB8F06A)
    val CombError = Color(0xFFFF6B5B)

    // The floating field (flow/) draws with these, so the app and the overlay stay one product.
    val OverlayBg = Ink
    val OverlayRim = Color(0x1FF7F3EA)
    val OverlayInk = Cream
    val OverlayMuted = Color(0x99F7F3EA)
    val OverlayGo = Accent
    val OverlayCancel = Color(0xFF2A2620)
    val OverlayWarn = Working
    val OverlayError = CombError
    val OverlayInfo = Typed
}

/** The only gradients: illustration panels, pack/model cards and the Home key card. Soft, never dark. */
enum class Panel(val from: Color, val to: Color, val glow: Color) {
    Peach(Color(0xFFFFF1E8), Color(0xFFFFD7C0), Color(0xFFFFC6A2)),
    Lime(Color(0xFFF6FDEB), Color(0xFFE3F6C6), Color(0xFFD7F5A8)),
    Sky(Color(0xFFF2F8FF), Color(0xFFDCECFF), Color(0xFFCFE5FF)),
    /** Pack cards: Lite, Balanced, Max. */
    Lite(Color(0xFFF3FCE4), Color(0xFFDDF3B8), Color(0x00FFFFFF)),
    Balanced(Color(0xFFFFF1E8), Color(0xFFFFD7C0), Color(0x00FFFFFF)),
    Max(Color(0xFFEEF6FF), Color(0xFFCFE5FF), Color(0x00FFFFFF)),
}

/** A diagonal wash with a radial highlight at ([gx], [gy]) as fractions of the size. */
fun Modifier.panel(p: Panel, gx: Float = 0.75f, gy: Float = 0.25f): Modifier = drawBehind {
    drawRect(Brush.linearGradient(listOf(p.from, p.to), Offset.Zero, Offset(size.width, size.height)))
    if (p.glow.alpha > 0f) drawRect(Brush.radialGradient(listOf(p.glow, p.glow.copy(alpha = 0f)), Offset(size.width * gx, size.height * gy), size.maxDimension * 0.6f))
}

private val Sans = FontFamily.SansSerif
/** Bricolage Grotesque ExtraBold (SIL OFL 1.1), headings only. */
val Display = FontFamily(Font(R.font.bricolage_extrabold, FontWeight.ExtraBold))

/** Type scale. Heavy, tight Bricolage headings and calm sans body text. Never `.copy(fontSize = …)`: pick a style. */
object T {
    val Display = TextStyle(fontFamily = com.whispr.app.ui.Display, fontWeight = FontWeight.ExtraBold, fontSize = 36.sp, lineHeight = 38.sp, letterSpacing = (-1.2).sp)
    val H1 = TextStyle(fontFamily = com.whispr.app.ui.Display, fontWeight = FontWeight.ExtraBold, fontSize = 30.sp, lineHeight = 34.sp, letterSpacing = (-0.9).sp)
    val H2 = TextStyle(fontFamily = com.whispr.app.ui.Display, fontWeight = FontWeight.ExtraBold, fontSize = 21.sp, lineHeight = 26.sp, letterSpacing = (-0.3).sp)
    /** Section headings ("Speech model", "Recently typed"). */
    val Section = TextStyle(fontFamily = com.whispr.app.ui.Display, fontWeight = FontWeight.ExtraBold, fontSize = 22.sp, lineHeight = 26.sp, letterSpacing = (-0.4).sp)
    val Title = TextStyle(fontFamily = Sans, fontWeight = FontWeight.Bold, fontSize = 16.sp, lineHeight = 22.sp)
    val Body = TextStyle(fontFamily = Sans, fontWeight = FontWeight.Normal, fontSize = 15.sp, lineHeight = 22.sp)
    val Body2 = TextStyle(fontFamily = Sans, fontWeight = FontWeight.Normal, fontSize = 14.sp, lineHeight = 20.sp)
    val Caption = TextStyle(fontFamily = Sans, fontWeight = FontWeight.Normal, fontSize = 12.sp, lineHeight = 16.sp)
    val Label = TextStyle(fontFamily = Sans, fontWeight = FontWeight.Bold, fontSize = 12.sp, lineHeight = 16.sp)
    /** Small capitals with air between the letters ("STEP 2 · MICROPHONE", "TODAY"). */
    val Overline = TextStyle(fontFamily = Sans, fontWeight = FontWeight.Bold, fontSize = 12.sp, lineHeight = 16.sp, letterSpacing = 1.2.sp)
    val Button = TextStyle(fontFamily = Sans, fontWeight = FontWeight.Bold, fontSize = 16.sp, lineHeight = 20.sp)
    /** Big counters and percentages. */
    val Numeral = TextStyle(fontFamily = com.whispr.app.ui.Display, fontWeight = FontWeight.ExtraBold, fontSize = 34.sp, lineHeight = 40.sp, letterSpacing = (-1).sp)

    /** Older name for [Body2], kept for screens outside ui/. */
    val Small = Body2
}

/** Spacing scale. Gutter 16, section gap 24, gap inside a card 12. */
object S {
    val XS = 4.dp
    val S = 8.dp
    val M = 12.dp
    val L = 16.dp
    val XL = 24.dp
    val XXL = 32.dp
    /** The smallest touch target. */
    val Touch = 48.dp
}

/** Corner radii: one per kind of surface. */
object Corners {
    val Card = RoundedCornerShape(24.dp)
    val Item = RoundedCornerShape(20.dp)
    val Control = RoundedCornerShape(14.dp)
    val Button = RoundedCornerShape(18.dp)
    val Stage = RoundedCornerShape(26.dp)
    val Pill = RoundedCornerShape(percent = 50)
}

/** Motion: one easing curve and a couple of springs, used everywhere so the app moves as one thing. */
object Motion {
    val Ease = CubicBezierEasing(0.22f, 1f, 0.36f, 1f)
    const val ENTER_MS = 420
    const val EXIT_MS = 220
    const val STAGGER_MS = 55L
    fun <T> enter() = tween<T>(durationMillis = ENTER_MS, easing = Ease)
    fun <T> exit() = tween<T>(durationMillis = EXIT_MS, easing = Ease)
    /** Colour and small state changes. */
    fun <T> quick() = tween<T>(durationMillis = 220, easing = Ease)
    fun <T> soft() = spring<T>(dampingRatio = Spring.DampingRatioLowBouncy, stiffness = Spring.StiffnessMediumLow)
    fun <T> snappy() = spring<T>(dampingRatio = Spring.DampingRatioNoBouncy, stiffness = Spring.StiffnessMedium)
}

@Composable
fun VoicioTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = lightColorScheme(
            primary = C.Accent, onPrimary = Color.White, primaryContainer = C.AccentSoft, onPrimaryContainer = C.AccentDeep,
            secondaryContainer = C.AccentSoft, onSecondaryContainer = C.AccentDeep,
            background = C.Bg, surface = C.Card, surfaceContainer = C.Card, surfaceContainerHigh = C.Card, surfaceContainerHighest = C.Soft,
            onSurface = C.Ink, onSurfaceVariant = C.Muted, onBackground = C.Ink, outline = C.Border, outlineVariant = C.Border,
            error = C.Coral, inverseSurface = C.Ink, inverseOnSurface = C.Cream, inversePrimary = C.Core,
        ),
        content = content,
    )
}
