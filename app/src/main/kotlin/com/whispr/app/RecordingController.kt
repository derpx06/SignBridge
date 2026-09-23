package com.whispr.app

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.whispr.app.storage.TranscriptRepository
import com.whispr.core.FinalTranscriptSegment
import com.whispr.core.ProvisionalTranscriptSegment
import com.whispr.core.SavedSession
import com.whispr.core.SessionEvent
import com.whispr.core.SessionReducer
import com.whispr.core.SessionState
import java.util.UUID

data class RecordingUiState(
    val isRecording: Boolean = false,
    val partialText: String = "",
    val segments: List<FinalTranscriptSegment> = emptyList(),
    val error: String? = null,
)

/** Coordinates the Android recognizer with the pure reducer and encrypted repository. */
class RecordingController(context: Context, private val repository: TranscriptRepository) {
    private val appContext = context.applicationContext
    private val reducer = SessionReducer()
    private var sessionState: SessionState = SessionState.Idle
    private var recognizer: SpeechRecognizer? = null
    private var sessionStartedAt = 0L
    private var segmentNumber = 0
    private var offlineAttempted = true

    var state: RecordingUiState by mutableStateOf(RecordingUiState())
        private set

    val sessions: List<SavedSession> get() = repository.list()

    fun refresh() { state = state.copy(error = null) }

    fun showError(message: String) { state = state.copy(error = message, isRecording = false) }

    fun start() {
        if (state.isRecording) return
        if (!SpeechRecognizer.isRecognitionAvailable(appContext)) {
            showError("This device has no speech recognition service. Install an offline language pack and try again.")
            return
        }
        sessionStartedAt = System.currentTimeMillis()
        segmentNumber = 0
        offlineAttempted = true
        sessionState = reducer.reduce(SessionState.Idle, SessionEvent.Start(UUID.randomUUID().toString(), sessionStartedAt))
        state = RecordingUiState(isRecording = true)
        recognizer = SpeechRecognizer.createSpeechRecognizer(appContext).also { it.setRecognitionListener(listener) }
        listen()
    }

    fun stop() {
        if (!state.isRecording) return
        recognizer?.stopListening()
        recognizer?.destroy()
        recognizer = null
        sessionState = reducer.reduce(sessionState, SessionEvent.Stop(System.currentTimeMillis()))
        sessionState = reducer.reduce(sessionState, SessionEvent.FinalizeSuccess(System.currentTimeMillis()))
        repository.save((sessionState as SessionState.Completed).session)
        state = RecordingUiState()
    }

    private fun listen(preferOffline: Boolean = offlineAttempted) {
        val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
            putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
            putExtra(RecognizerIntent.EXTRA_LANGUAGE, "en-IN")
            putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
            putExtra(RecognizerIntent.EXTRA_PREFER_OFFLINE, preferOffline)
            putExtra(RecognizerIntent.EXTRA_MAX_RESULTS, 1)
        }
        recognizer?.startListening(intent)
    }

    private val listener = object : RecognitionListener {
        override fun onReadyForSpeech(params: Bundle?) = Unit
        override fun onBeginningOfSpeech() = Unit
        override fun onRmsChanged(rmsdB: Float) = Unit
        override fun onBufferReceived(buffer: ByteArray?) = Unit
        override fun onEndOfSpeech() = Unit
        override fun onEvent(eventType: Int, params: Bundle?) = Unit
        override fun onError(error: Int) {
            if (!state.isRecording || error == SpeechRecognizer.ERROR_CLIENT) return
            if (offlineAttempted) {
                // Some Android builds ship no offline language pack. Give the user a
                // working transcription path once, then surface a stable error.
                offlineAttempted = false
                listen(preferOffline = false)
            } else {
                showError("Speech recognition failed (code $error). Check microphone permission and network, then try again.")
            }
        }
        override fun onPartialResults(results: Bundle?) {
            results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull()?.let { if (it.isNotBlank()) updateText(it, false) }
        }
        override fun onResults(results: Bundle?) {
            results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull()?.let { if (it.isNotBlank()) updateText(it, true) }
            if (state.isRecording) listen()
        }
    }

    private fun updateText(text: String, isFinal: Boolean) {
        if (!state.isRecording) return
        val elapsed = System.currentTimeMillis() - sessionStartedAt
        val segment = ProvisionalTranscriptSegment("live", text, maxOf(0, elapsed - 700), elapsed)
        sessionState = reducer.reduce(sessionState, SessionEvent.LiveText(segment))
        if (isFinal) {
            sessionState = reducer.reduce(sessionState, SessionEvent.LiveText(segment.copy(id = "segment-${segmentNumber++}")))
            state = state.copy(partialText = "", segments = currentSegments())
        } else state = state.copy(partialText = text)
    }

    private fun currentSegments(): List<FinalTranscriptSegment> = when (val current = sessionState) {
        is SessionState.Recording -> current.segments.filterNot { it.id == "live" }.map { it.toFinal() }
        is SessionState.Finalizing -> current.segments.map { it.toFinal() }
        else -> emptyList()
    }

    private fun ProvisionalTranscriptSegment.toFinal() = FinalTranscriptSegment(id, text, startMs, endMs, speakerId)
}
