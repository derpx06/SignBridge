package com.whispr.core

import org.junit.Assert.assertEquals
import org.junit.Assert.fail
import org.junit.Test

class SessionReducerTest {
    private val reducer = SessionReducer()

    @Test
    fun `progresses from idle through recording and finalizing to a completed transcript-only session`() {
        val recording = reducer.reduce(SessionState.Idle, SessionEvent.Start(sessionId = "session-1", startedAtMs = 100))
        val withText = reducer.reduce(
            recording,
            SessionEvent.LiveText(
                segment = ProvisionalTranscriptSegment(
                    id = "segment-1",
                    text = "Hello there",
                    startMs = 0,
                    endMs = 850,
                ),
            ),
        )
        val finalizing = reducer.reduce(withText, SessionEvent.Stop(stoppedAtMs = 1_000))
        val completed = reducer.reduce(finalizing, SessionEvent.FinalizeSuccess(completedAtMs = 1_100))

        val completedState = completedState(completed)
        assertEquals("session-1", completedState.session.id)
        assertEquals("Hello there", completedState.session.segments.single().text)
    }

    @Test
    fun `rejects transitions that are not legal from the current lifecycle state`() {
        assertInvalidTransition {
            reducer.reduce(SessionState.Idle, SessionEvent.Stop(stoppedAtMs = 10))
        }

        val recording = reducer.reduce(SessionState.Idle, SessionEvent.Start("session-1", 100))
        assertInvalidTransition {
            reducer.reduce(recording, SessionEvent.FinalizeSuccess(completedAtMs = 200))
        }
    }

    @Test
    fun `replaces a delayed speaker label on the matching stable segment id`() {
        val recording = reducer.reduce(SessionState.Idle, SessionEvent.Start("session-1", 0))
        val withText = reducer.reduce(
            recording,
            SessionEvent.LiveText(ProvisionalTranscriptSegment("segment-1", "Hello", 0, 300)),
        )
        val updated = reducer.reduce(withText, SessionEvent.SpeakerUpdate(segmentId = "segment-1", speakerId = "speaker-a"))

        val updatedRecording = recordingState(updated)
        assertEquals("speaker-a", updatedRecording.segments.single().speakerId)
    }

    @Test
    fun `persists a delayed speaker label received while finalizing`() {
        val recording = reducer.reduce(SessionState.Idle, SessionEvent.Start("session-1", 0))
        val withText = reducer.reduce(
            recording,
            SessionEvent.LiveText(ProvisionalTranscriptSegment("segment-1", "Hello", 0, 300)),
        )
        val finalizing = reducer.reduce(withText, SessionEvent.Stop(stoppedAtMs = 100))
        val labeled = reducer.reduce(
            finalizing,
            SessionEvent.SpeakerUpdate(segmentId = "segment-1", speakerId = "speaker-a"),
        )
        val completed = reducer.reduce(labeled, SessionEvent.FinalizeSuccess(completedAtMs = 101))

        assertEquals("speaker-a", completedState(completed).session.segments.single().speakerId)
    }

    @Test
    fun `completed session cannot retain audio from recording input`() {
        val recording = reducer.reduce(
            SessionState.Idle,
            SessionEvent.Start(sessionId = "session-1", startedAtMs = 0, audio = AudioReference("audio-1")),
        )
        val completed = reducer.reduce(
            reducer.reduce(recording, SessionEvent.Stop(stoppedAtMs = 100)),
            SessionEvent.FinalizeSuccess(completedAtMs = 101),
        )

        val completedState = completedState(completed)
        assertEquals(
            SavedSession(
                id = "session-1",
                startedAtMs = 0,
                stoppedAtMs = 100,
                completedAtMs = 101,
                segments = emptyList(),
            ),
            completedState.session,
        )
    }

    private fun assertInvalidTransition(block: () -> Unit) {
        try {
            block()
            fail("Expected InvalidSessionTransition")
        } catch (_: InvalidSessionTransition) {
            // Expected.
        }
    }

    private fun completedState(state: SessionState): SessionState.Completed =
        state as? SessionState.Completed ?: throw AssertionError("Expected a completed session")

    private fun recordingState(state: SessionState): SessionState.Recording =
        state as? SessionState.Recording ?: throw AssertionError("Expected recording state")
}
