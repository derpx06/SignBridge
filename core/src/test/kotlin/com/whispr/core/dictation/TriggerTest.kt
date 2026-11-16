package com.whispr.core.dictation

import com.whispr.core.dictation.Trigger.Action
import com.whispr.core.dictation.Trigger.Dictation
import com.whispr.core.dictation.Trigger.Mode
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Ported from the unit tests in `trigger.rs`; times are plain milliseconds. */
class TriggerTest {

    @Test fun holdRecordsWhileHeldAndStopsAfterTheTail() {
        val t = Trigger(Mode.Hold)
        assertEquals(Action.Start, t.press(0, Dictation.Idle))
        assertTrue(t.holding())
        t.release(2000)
        assertEquals("the mic stays open for the tail", Action.None, t.due(2100))
        assertEquals(Action.Stop, t.due(2160))
    }

    @Test fun aQuickTapNeverLatchesInHoldMode() {
        val t = Trigger(Mode.Hold)
        assertEquals(Action.Start, t.press(0, Dictation.Idle))
        t.release(120)
        assertEquals(Action.Discard, t.due(200))
    }

    @Test fun aPressWhileFinishingSaysBusy() {
        val t = Trigger(Mode.Hold)
        assertEquals(Action.Busy, t.press(0, Dictation.Busy))
        t.release(800)
        assertEquals(Action.None, t.due(900))
    }

    @Test fun aCancelWhileTheKeyIsDownDoesNotRestartOnKeyRepeat() {
        val t = Trigger(Mode.Hold)
        assertEquals(Action.Start, t.press(0, Dictation.Idle))
        t.reset()
        assertEquals("key repeat is not a new press", Action.None, t.press(500, Dictation.Idle))
        t.release(900)
        assertEquals(Action.None, t.due(1100))
        assertEquals("a real new press works again", Action.Start, t.press(2000, Dictation.Idle))
    }

    @Test fun keyRepeatNeverActsAsASecondPress() {
        val t = Trigger(Mode.Hold)
        assertEquals(Action.Start, t.press(0, Dictation.Idle))
        assertEquals(Action.None, t.press(500, Dictation.Listening))
        assertEquals(Action.None, t.press(540, Dictation.Listening))
    }

    @Test fun x11ReleaseThenPressRepeatIsIgnored() {
        val t = Trigger(Mode.Hold)
        assertEquals(Action.Start, t.press(0, Dictation.Idle))
        t.release(600)
        assertEquals(Action.None, t.press(610, Dictation.Listening)) // cancels the release
        t.release(1500)
        assertEquals(Action.Stop, t.due(1660)) // the real release ends it
    }

    @Test fun aSecondPressWithinDebounceCountsOnce() {
        val t = Trigger(Mode.Hold)
        assertEquals(Action.Start, t.press(0, Dictation.Idle))
        t.release(5)
        assertEquals(Action.Discard, t.due(60))
        assertEquals("bounce: too soon after the first", Action.None, t.press(25, Dictation.Idle))
    }

    @Test fun toggleStartsOnOnePressAndStopsOnTheNext() {
        val t = Trigger(Mode.Toggle)
        assertEquals(Action.Start, t.press(0, Dictation.Idle))
        t.release(2000) // even a long hold does not stop it
        assertEquals(Action.None, t.due(2200))
        assertEquals(Action.Stop, t.press(5000, Dictation.Listening))
    }

    @Test fun holdOrTapLatchesOnATapAndStopsOnReleaseAfterAHold() {
        var t = Trigger(Mode.HoldOrTap)
        assertEquals(Action.Start, t.press(0, Dictation.Idle))
        t.release(100)
        assertEquals("a tap keeps listening", Action.None, t.due(200))
        assertEquals(Action.Stop, t.press(3000, Dictation.Listening))
        t.release(3100)
        assertEquals(Action.None, t.due(3300))

        t = Trigger(Mode.HoldOrTap)
        assertEquals(Action.Start, t.press(0, Dictation.Idle))
        t.release(2000)
        assertEquals(Action.Stop, t.due(2160))
    }

    @Test fun modesParseFromTheSettingsText() {
        assertEquals(Mode.Hold, Mode.parse("hold"))
        assertEquals(Mode.Toggle, Mode.parse("toggle"))
        assertEquals(Mode.HoldOrTap, Mode.parse("hold-or-tap"))
        assertEquals("unknown text falls back to the safe default", Mode.Hold, Mode.parse("nonsense"))
    }

    // ---- additions: boundaries and the deadline ----

    @Test fun holdOfExactlyMinHoldStopsAndJustUnderIsDiscarded() {
        val ok = Trigger(Mode.Hold)
        ok.press(0, Dictation.Idle)
        ok.release(Trigger.MIN_HOLD_MS)
        assertEquals(Action.Stop, ok.due(Trigger.MIN_HOLD_MS + Trigger.RELEASE_TAIL_MS))

        val short = Trigger(Mode.Hold)
        short.press(0, Dictation.Idle)
        short.release(Trigger.MIN_HOLD_MS - 1)
        assertEquals(Action.Discard, short.due(Trigger.MIN_HOLD_MS - 1 + Trigger.RELEASE_GRACE_MS))
    }

    @Test fun deadlineReportsWhenDueWillAct() {
        val t = Trigger(Mode.Hold)
        assertNull(t.deadlineMs())
        t.press(0, Dictation.Idle)
        assertNull(t.deadlineMs())
        t.release(1000)
        assertEquals(1000 + Trigger.RELEASE_TAIL_MS, t.deadlineMs())
        t.due(1150)
        assertNull(t.deadlineMs())
    }

    @Test fun holdingIsFalseAfterReleaseAndForStopTaps() {
        val t = Trigger(Mode.Toggle)
        t.press(0, Dictation.Idle)
        assertTrue(t.holding())
        t.release(50)
        assertFalse(t.holding())
        t.due(200)
        t.press(1000, Dictation.Listening) // a stop tap
        assertFalse(t.holding())
    }

    @Test fun releaseWithoutPressIsIgnored() {
        val t = Trigger(Mode.Hold)
        t.release(10)
        assertNull(t.deadlineMs())
        assertEquals(Action.None, t.due(1000))
    }

    @Test fun setModeTakesEffectForTheNextRelease() {
        val t = Trigger(Mode.Hold)
        t.press(0, Dictation.Idle)
        t.setMode(Mode.Toggle)
        t.release(1000)
        assertEquals(Action.None, t.due(1200))
    }
}
