package com.whispr.app.flow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PlacementTest {
    private val phone = Box(0, 100, 1080, 2300) // inside status + nav bars
    private val pillH = 76
    private fun input(
        usable: Box = phone, ime: Int? = null, frac: Float? = 0.5f, bar: Boolean = false, right: Boolean = true,
        w: Int = 76, popup: Int = 0,
    ) = Placement.Input(usable, ime, if (bar) 600 else w, pillH, if (bar) 204 else popup, bar, right, frac, 16, 0, 192, 32)

    private fun place(i: Placement.Input) = Placement.place(i)

    @Test fun savedFractionMapsToHeightAndDocksToEdge() {
        val r = place(input(frac = 0f)); assertEquals(100, r.y); assertEquals(1080 - 76, r.x)
        assertEquals(2300 - pillH, place(input(frac = 1f)).y)
        assertEquals(0, place(input(right = false)).x)
    }

    @Test fun neverMovedRestsLow() {
        val r = place(input(frac = null)); assertEquals(2300 + 16 - pillH - 192, r.y)
    }

    @Test fun rotationKeepsFractionAndEdge() {
        val land = Box(0, 60, 2300, 1080)
        val p = place(input(frac = 0.25f)); val l = place(input(usable = land, frac = 0.25f))
        assertEquals(100 + Math.round(0.25f * (2300 - pillH - 100)), p.y)
        assertEquals(60 + Math.round(0.25f * (1080 - pillH - 60)), l.y)
        assertEquals(2300 - 76, l.x)
        assertEquals(place(input(usable = land, frac = 0.25f)), l) // rotating back and forth is stable
    }

    @Test fun keyboardLiftsOnlyWhenItCoversAndReturnsWhenGone() {
        val low = place(input(frac = 0.95f))
        val up = place(input(frac = 0.95f, ime = 1400))
        assertTrue(up.y + pillH - 16 <= 1400 - 32) // visible bottom clears the keyboard by the gap
        assertEquals(low, place(input(frac = 0.95f, ime = null)))
        val high = place(input(frac = 0.1f)) // already above the keyboard: untouched
        assertEquals(high, place(input(frac = 0.1f, ime = 1400)))
    }

    @Test fun keyboardHeightChangeDoesNotMovePillThatIsClear() {
        assertEquals(place(input(frac = 0.2f, ime = 1500)), place(input(frac = 0.2f, ime = 1300)))
    }

    @Test fun popupSitsAboveThenFlipsBelowNearTop() {
        val mid = place(input(frac = 0.5f, bar = true))
        assertTrue(mid.popupAbove); assertEquals(mid.pillTop - 204, mid.y)
        val top = place(input(frac = 0f, bar = true))
        assertFalse(top.popupAbove); assertEquals(100, top.pillTop); assertEquals(top.pillTop, top.y)
    }

    @Test fun popupNeverMovesPill() {
        for (f in listOf(0f, 0.03f, 0.5f, 1f)) {
            val orb = place(input(frac = f)); val bar = place(input(frac = f, bar = true))
            assertEquals(orb.pillTop, bar.pillTop)
        }
    }

    @Test fun barIsCentredAndClampedToScreen() {
        assertEquals((1080 - 600) / 2, place(input(bar = true)).x)
        val narrow = Box(0, 100, 400, 800)
        val r = place(input(usable = narrow, bar = true)); assertEquals(0, r.x) // 600 wide bar clamps to 400
    }

    @Test fun smallScreenStaysInsideAndFlipsToTheRoomierSide() {
        val tiny = Box(0, 0, 320, 300)
        for (f in listOf(0f, 0.5f, 1f, null)) {
            val r = place(input(usable = tiny, frac = f, bar = true, w = 76))
            assertTrue(r.pillTop in 0..(300 - pillH))
        }
        assertFalse(place(input(usable = tiny, frac = 0f, bar = true)).popupAbove)
        assertTrue(place(input(usable = tiny, frac = 1f, bar = true)).popupAbove)
        assertEquals(0, place(input(usable = Box(0, 0, 100, 50), frac = 0.5f)).pillTop) // shorter than the pill
    }

    @Test fun cutoutInsetsOffsetXAndY() {
        val cut = Box(120, 160, 1080 - 90, 2300) // landscape cutout on the left, camera notch on top
        assertEquals(160, place(input(usable = cut, frac = 0f)).y)
        assertEquals(120, place(input(usable = cut, right = false)).x)
        assertEquals(1080 - 90 - 76, place(input(usable = cut)).x)
    }

    @Test fun repeatedIdenticalInputsGiveIdenticalResult() {
        val a = place(input(frac = 0.7f, ime = 1500, bar = true))
        repeat(50) { assertEquals(a, place(input(frac = 0.7f, ime = 1500, bar = true))) }
    }

    @Test fun fractionRoundTripsThroughDrag() {
        for (top in listOf(100, 777, 1234, 2224)) {
            val f = Placement.fracOf(phone, pillH, top)
            assertTrue(kotlin.math.abs(place(input(frac = f)).y - top) <= 1)
        }
        assertEquals(0f, Placement.fracOf(Box(0, 0, 10, 50), pillH, 20))
        assertTrue(Placement.dockRightFor(phone, 700)); assertFalse(Placement.dockRightFor(phone, 300))
    }
}
