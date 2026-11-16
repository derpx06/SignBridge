package com.whispr.app.flow

/**
 * The one place that decides where the bubble window goes. Pure, so it is unit-tested.
 *
 * Single source of truth: the saved edge ([Input.dockRight]) and vertical fraction ([Input.yFrac], 0 = top of the
 * usable area, 1 = bottom). The result depends only on these and the screen, never on where the window is now, so
 * repeated identical inputs give the identical answer and there is no feedback loop. The pill moves only for a drag
 * (which rewrites the saved fraction), a rotation (new [Input.usable]) or a keyboard covering it ([Input.imeTop]).
 * The words popup is placed relative to the pill and never changes the pill's position.
 */
object Placement {
    /**
     * [usable] is inside the system bars and cutouts, NOT reduced by the keyboard. [w] is the window width,
     * [pillH] the height of the orb/pill including its shadow pad; [popupH] the room reserved for the words popup
     * (0 for the orb). All in px.
     */
    class Input(
        val usable: Box, val imeTop: Int?, val w: Int, val pillH: Int, val popupH: Int,
        val bar: Boolean, val dockRight: Boolean, val yFrac: Float?,
        val pad: Int, val margin: Int, val restLift: Int, val imeGap: Int,
    )

    /** Window top-left, and whether the popup sits above the pill (else below it). */
    data class Result(val x: Int, val y: Int, val popupAbove: Boolean, val pillTop: Int)

    fun place(i: Input): Result {
        val u = i.usable
        val w = minOf(i.w, u.width)
        val x = if (i.bar) u.left + (u.width - w) / 2 else if (i.dockRight) u.right - w - i.margin else u.left + i.margin
        val maxTop = maxOf(u.bottom - i.pillH, u.top)
        var top = i.yFrac?.let { u.top + Math.round(it.coerceIn(0f, 1f) * (maxTop - u.top)) } ?: (u.bottom + i.pad - i.pillH - i.restLift)
        i.imeTop?.let { top = minOf(top, it + i.pad - i.pillH - i.imeGap) } // only a keyboard that would cover it lifts it
        top = top.coerceIn(u.top, maxTop)
        val bottom = i.imeTop?.coerceIn(u.top, u.bottom) ?: u.bottom
        val above = i.popupH == 0 || top - i.popupH >= u.top || top - u.top >= bottom - (top + i.pillH)
        return Result(x, if (above) top - i.popupH else top, above, top)
    }

    /** Fraction to save after a drag that left the pill's top at [pillTop]. */
    fun fracOf(usable: Box, pillH: Int, pillTop: Int): Float {
        val travel = maxOf(usable.bottom - pillH, usable.top) - usable.top
        return if (travel <= 0) 0f else ((pillTop - usable.top).toFloat() / travel).coerceIn(0f, 1f)
    }

    /** Edge nearest to a window centred at [centerX]. */
    fun dockRightFor(usable: Box, centerX: Int) = centerX > (usable.left + usable.right) / 2
}

/** Plain pixel box (android.graphics.Rect is not usable in JVM unit tests). */
data class Box(val left: Int, val top: Int, val right: Int, val bottom: Int) {
    val width get() = right - left
    val height get() = bottom - top
}
