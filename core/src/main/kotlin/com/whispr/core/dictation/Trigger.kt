package com.whispr.core.dictation

/**
 * The dictation shortcut as a pure state machine (port of Voicio's `trigger.rs`).
 *
 * Time is injected: every call takes `nowMs`, a monotonic millisecond clock owned by the caller.
 * - [Mode.Hold] (default): recording lasts exactly while the key is held; a press shorter than
 *   [MIN_HOLD_MS] is discarded and never latches.
 * - [Mode.Toggle]: a press starts, the next press stops.
 * - [Mode.HoldOrTap]: holding is push-to-talk, a quick tap latches until the next tap.
 *
 * Quirks handled: key-repeat presses are ignored; a release immediately followed by a press (X11
 * repeat) is cancelled within [RELEASE_GRACE_MS]; presses within [DEBOUNCE_MS] count once.
 */
class Trigger(private var mode: Mode = Mode.Hold) {

    enum class Mode {
        Hold, Toggle, HoldOrTap;

        companion object {
            /** Settings text to mode; unknown text falls back to the safe default, [Hold]. */
            fun parse(text: String): Mode = when (text) {
                "toggle" -> Toggle
                "hold-or-tap" -> HoldOrTap
                else -> Hold
            }
        }
    }

    /** What the dictation is doing right now, as seen by the caller at the moment of a press. */
    enum class Dictation { Idle, Listening, Busy }

    enum class Action {
        None,
        Start,
        Stop,

        /** The key was let go too soon (Hold mode): throw the recording away and say why. */
        Discard,

        /** The key was pressed while the last dictation is still being finished. */
        Busy,
    }

    private data class PendingRelease(val releasedAt: Long, val effectiveAt: Long)

    private var held = false
    private var startedAt: Long? = null
    private var lastPress: Long? = null
    private var release: PendingRelease? = null

    fun setMode(mode: Mode) {
        this.mode = mode
    }

    /** The key that started the current dictation is still down. */
    fun holding(): Boolean = held && startedAt != null

    fun press(nowMs: Long, dictation: Dictation): Action {
        if (release != null) {
            release = null
            held = true // key repeat: the "release" was not real, the key is still down
            return Action.None
        }
        if (held || lastPress?.let { nowMs - it < DEBOUNCE_MS } == true) return Action.None
        held = true
        lastPress = nowMs
        return when (dictation) {
            Dictation.Idle -> { startedAt = nowMs; Action.Start }
            Dictation.Listening -> { startedAt = null; Action.Stop }
            Dictation.Busy -> { startedAt = null; Action.Busy }
        }
    }

    fun release(nowMs: Long) {
        if (!held) return
        held = false
        val started = startedAt ?: return
        val heldToTalk = mode != Mode.Toggle && nowMs - started >= minOf(MIN_HOLD_MS, HOLD_TO_TALK_MS)
        release = PendingRelease(nowMs, nowMs + if (heldToTalk) RELEASE_TAIL_MS else RELEASE_GRACE_MS)
    }

    /** Call when time passes: acts on a release whose grace period has ended. */
    fun due(nowMs: Long): Action {
        val pending = release ?: return Action.None
        if (nowMs < pending.effectiveAt) return Action.None
        release = null
        val started = startedAt ?: return Action.None
        startedAt = null
        val heldFor = pending.releasedAt - started
        return when {
            mode == Mode.Hold && heldFor >= MIN_HOLD_MS -> Action.Stop
            mode == Mode.Hold -> Action.Discard
            mode == Mode.HoldOrTap && heldFor >= HOLD_TO_TALK_MS -> Action.Stop
            else -> Action.None // a tap: keep listening until the next press
        }
    }

    /** The next moment [due] has something to do, so the caller can wake up in time. */
    fun deadlineMs(): Long? = release?.effectiveAt

    /**
     * The dictation ended some other way (cancel, error, silence): forget the key state. The key may
     * still be down, so `held` survives: a repeat event must not start a new dictation.
     */
    fun reset() {
        startedAt = null
        lastPress = null
        release = null
    }

    companion object {
        const val HOLD_TO_TALK_MS = 300L

        /** In Hold mode a shorter press is a slip of the finger, not a dictation. */
        const val MIN_HOLD_MS = 250L
        const val RELEASE_GRACE_MS = 50L

        /** After a held key is let go the mic stays open this long, so the last word is not clipped. */
        const val RELEASE_TAIL_MS = 150L
        const val DEBOUNCE_MS = 30L
    }
}
