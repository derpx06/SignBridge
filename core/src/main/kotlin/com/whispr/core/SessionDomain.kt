package com.whispr.core

/** A stable reference to a captured audio asset; the domain never opens the asset itself. */
data class AudioReference(val id: String)

/** A transcript item while a session is still live. */
data class ProvisionalTranscriptSegment(
    val id: String,
    val text: String,
    val startMs: Long,
    val endMs: Long,
    val speakerId: String? = null,
) {
    init {
        require(id.isNotBlank()) { "Segment id must not be blank" }
        require(startMs >= 0) { "Segment start must not be negative" }
        require(endMs >= startMs) { "Segment end must not precede its start" }
    }
}

/** An immutable transcript item in a saved session. */
data class FinalTranscriptSegment(
    val id: String,
    val text: String,
    val startMs: Long,
    val endMs: Long,
    val speakerId: String? = null,
)

/** The complete persisted representation of a finished session. */
data class SavedSession(
    val id: String,
    val startedAtMs: Long,
    val stoppedAtMs: Long,
    val completedAtMs: Long,
    val segments: List<FinalTranscriptSegment>,
)

sealed interface SessionState {
    data object Idle : SessionState

    data class Recording(
        val sessionId: String,
        val startedAtMs: Long,
        val audio: AudioReference?,
        val segments: List<ProvisionalTranscriptSegment> = emptyList(),
    ) : SessionState

    data class Finalizing(
        val sessionId: String,
        val startedAtMs: Long,
        val stoppedAtMs: Long,
        val audio: AudioReference?,
        val segments: List<ProvisionalTranscriptSegment>,
    ) : SessionState

    data class Completed(val session: SavedSession) : SessionState

    data class Failure(
        val sessionId: String?,
        val message: String,
        val cause: Throwable? = null,
    ) : SessionState
}

sealed interface SessionEvent {
    data class Start(
        val sessionId: String,
        val startedAtMs: Long,
        val audio: AudioReference? = null,
    ) : SessionEvent

    data class LiveText(val segment: ProvisionalTranscriptSegment) : SessionEvent

    data class SpeakerUpdate(val segmentId: String, val speakerId: String?) : SessionEvent

    data class Stop(val stoppedAtMs: Long) : SessionEvent

    data class FinalizeSuccess(
        val completedAtMs: Long,
    ) : SessionEvent

    data class Failure(val message: String, val cause: Throwable? = null) : SessionEvent
}

class InvalidSessionTransition(
    val state: SessionState,
    val event: SessionEvent,
) : IllegalStateException("${event::class.simpleName} is not valid while ${state::class.simpleName}")

/**
 * Pure state machine for one live transcription session.
 *
 * Invalid events throw [InvalidSessionTransition] rather than changing state, so callers
 * can surface an operational failure without losing the existing session snapshot.
 */
class SessionReducer {
    fun reduce(state: SessionState, event: SessionEvent): SessionState = when (state) {
        SessionState.Idle -> when (event) {
            is SessionEvent.Start -> SessionState.Recording(
                sessionId = event.sessionId,
                startedAtMs = event.startedAtMs,
                audio = event.audio,
            )
            else -> invalid(state, event)
        }

        is SessionState.Recording -> when (event) {
            is SessionEvent.LiveText -> state.copy(segments = replaceOrAppend(state.segments, event.segment))
            is SessionEvent.SpeakerUpdate -> state.copy(segments = replaceSpeaker(state.segments, state, event))
            is SessionEvent.Stop -> {
                require(event.stoppedAtMs >= state.startedAtMs) {
                    "A session cannot stop before it starts"
                }
                SessionState.Finalizing(
                    sessionId = state.sessionId,
                    startedAtMs = state.startedAtMs,
                    stoppedAtMs = event.stoppedAtMs,
                    audio = state.audio,
                    segments = state.segments,
                )
            }
            is SessionEvent.Failure -> SessionState.Failure(state.sessionId, event.message, event.cause)
            else -> invalid(state, event)
        }

        is SessionState.Finalizing -> when (event) {
            is SessionEvent.SpeakerUpdate -> state.copy(segments = replaceSpeaker(state.segments, state, event))
            is SessionEvent.FinalizeSuccess -> {
                require(event.completedAtMs >= state.stoppedAtMs) {
                    "A session cannot complete before it stops"
                }
                SessionState.Completed(
                    SavedSession(
                        id = state.sessionId,
                        startedAtMs = state.startedAtMs,
                        stoppedAtMs = state.stoppedAtMs,
                        completedAtMs = event.completedAtMs,
                        segments = state.segments.map { it.toFinal() },
                    ),
                )
            }
            is SessionEvent.Failure -> SessionState.Failure(state.sessionId, event.message, event.cause)
            else -> invalid(state, event)
        }

        is SessionState.Completed,
        is SessionState.Failure,
        -> invalid(state, event)
    }

    private fun replaceOrAppend(
        segments: List<ProvisionalTranscriptSegment>,
        incoming: ProvisionalTranscriptSegment,
    ): List<ProvisionalTranscriptSegment> {
        val index = segments.indexOfFirst { it.id == incoming.id }
        if (index < 0) return segments + incoming

        val current = segments[index]
        val replacement = incoming.copy(speakerId = incoming.speakerId ?: current.speakerId)
        return segments.toMutableList().also { it[index] = replacement }
    }

    private fun replaceSpeaker(
        segments: List<ProvisionalTranscriptSegment>,
        state: SessionState,
        event: SessionEvent.SpeakerUpdate,
    ): List<ProvisionalTranscriptSegment> {
        val index = segments.indexOfFirst { it.id == event.segmentId }
        if (index < 0) throw InvalidSessionTransition(state, event)
        return segments.toMutableList().also { current ->
            current[index] = current[index].copy(speakerId = event.speakerId)
        }
    }

    private fun ProvisionalTranscriptSegment.toFinal() = FinalTranscriptSegment(
        id = id,
        text = text,
        startMs = startMs,
        endMs = endMs,
        speakerId = speakerId,
    )

    private fun invalid(state: SessionState, event: SessionEvent): Nothing =
        throw InvalidSessionTransition(state, event)
}
