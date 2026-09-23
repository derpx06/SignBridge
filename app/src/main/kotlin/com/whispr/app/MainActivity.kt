package com.whispr.app

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import com.whispr.app.storage.EncryptedTranscriptRepository
import com.whispr.app.storage.DefaultModelPackManifest
import com.whispr.app.storage.ModelPackState
import com.whispr.app.storage.PlainTextTranscriptExporter
import com.whispr.core.FinalTranscriptSegment
import com.whispr.core.SavedSession
import java.time.Instant

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { WhisprApp() }
    }
}

@Composable
@OptIn(ExperimentalMaterial3Api::class)
private fun WhisprApp() {
    val context = LocalContext.current
    val repository = remember { EncryptedTranscriptRepository(context.applicationContext) }
    val controller = remember { RecordingController(context.applicationContext, repository) }
    val modelDownloads = remember { ModelDownloadController(context.applicationContext) }
    val navigation = rememberNavController()
    val startDestination = if (modelDownloads.isReady) "sessions" else "setup"
    var permissionRequested by remember { mutableStateOf(false) }
    val permissionLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        permissionRequested = true
        if (granted) controller.start() else controller.showError("Microphone permission is required to transcribe speech.")
    }

    MaterialTheme {
        Surface(Modifier.fillMaxSize()) {
            NavHost(navigation, startDestination = startDestination) {
                composable("setup") {
                    ModelSetupScreen(
                        state = modelDownloads.state,
                        onDownload = modelDownloads::download,
                        onContinue = { navigation.navigate("sessions") { popUpTo("setup") { inclusive = true } } },
                    )
                }
                composable("sessions") {
                    SessionsScreen(
                        sessions = controller.sessions,
                        onRecord = {
                            if (ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) controller.start()
                            else permissionLauncher.launch(Manifest.permission.RECORD_AUDIO)
                            navigation.navigate("recording")
                        },
                        onOpen = { navigation.navigate("detail/${it.id}") },
                        onSetup = { navigation.navigate("setup") },
                    )
                }
                composable("recording") { RecordingScreen(controller, onBack = { navigation.popBackStack() }) }
                composable("detail/{id}") { entry ->
                    val session = controller.sessions.firstOrNull { it.id == entry.arguments?.getString("id") }
                    if (session == null) Text("Transcript not found", Modifier.padding(24.dp))
                    else DetailScreen(session, onBack = { navigation.popBackStack() })
                }
            }
        }
    }
    LaunchedEffect(permissionRequested) { if (permissionRequested) controller.refresh() }
}

@Composable
@OptIn(ExperimentalMaterial3Api::class)
private fun SessionsScreen(sessions: List<SavedSession>, onRecord: () -> Unit, onOpen: (SavedSession) -> Unit, onSetup: () -> Unit) {
    Scaffold(topBar = { TopAppBar(title = { Text("Whispr") }) }) { padding ->
        Column(Modifier.fillMaxSize().padding(padding).padding(horizontal = 20.dp, vertical = 16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Text("Private, live transcription", style = MaterialTheme.typography.headlineSmall)
            Text("Speech stays on this device. Start a session when you are ready.")
            val packsConfigured = DefaultModelPackManifest.descriptors.all { it.configurationUrl.startsWith("https://") && it.sha256.length == 64 }
            Card(Modifier.fillMaxWidth()) {
                Text(
                    if (packsConfigured) "Offline language and speaker packs are ready to download."
                    else "Offline model packs are not configured in this build; the device recognizer will be used when available.",
                    Modifier.padding(16.dp),
                )
            }
            OutlinedButton(onClick = onSetup, modifier = Modifier.fillMaxWidth()) { Text("Language model setup") }
            Button(onClick = onRecord, modifier = Modifier.fillMaxWidth().height(52.dp).semantics { contentDescription = "Start a new recording" }) { Text("Start recording") }
            Text("Saved sessions", style = MaterialTheme.typography.titleLarge)
            if (sessions.isEmpty()) Card(Modifier.fillMaxWidth()) { Text("No transcripts yet. Your finished sessions will appear here.", Modifier.padding(20.dp)) }
            else LazyColumn(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                items(sessions, key = { it.id }) { session ->
                    Card(onClick = { onOpen(session) }, modifier = Modifier.fillMaxWidth()) {
                        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                            Text("Session ${session.id.takeLast(6)}", style = MaterialTheme.typography.titleMedium)
                            Text("${session.segments.size} transcript segments")
                            Text(Instant.ofEpochMilli(session.completedAtMs).toString(), style = MaterialTheme.typography.bodySmall)
                        }
                    }
                }
            }
        }
    }
}

@Composable
@OptIn(ExperimentalMaterial3Api::class)
private fun ModelSetupScreen(state: ModelPackState, onDownload: () -> Unit, onContinue: () -> Unit) {
    Scaffold(topBar = { TopAppBar(title = { Text("Set up Whispr") }) }) { padding ->
        Column(Modifier.fillMaxSize().padding(padding).padding(20.dp), verticalArrangement = Arrangement.spacedBy(18.dp)) {
            Text("Download your language model", style = MaterialTheme.typography.headlineSmall)
            Text("Whispr downloads the official multilingual Whisper Tiny pack to private app storage. After setup, speech can be processed without sending recordings to Whispr.")
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("Whisper Tiny · multilingual", style = MaterialTheme.typography.titleMedium)
                    Text("Includes Hindi and English recognition. The first download is large; keep Wi‑Fi enabled.")
                    when (state) {
                        ModelPackState.Absent -> Button(onClick = onDownload, modifier = Modifier.fillMaxWidth()) { Text("Download model") }
                        is ModelPackState.Downloading -> { Text("Downloading model…"); androidx.compose.material3.CircularProgressIndicator() }
                        is ModelPackState.Ready -> Button(onClick = onContinue, modifier = Modifier.fillMaxWidth()) { Text("Continue to Whispr") }
                        is ModelPackState.Error -> {
                            Text("Download failed: ${state.message}", color = MaterialTheme.colorScheme.error)
                            Button(onClick = onDownload, modifier = Modifier.fillMaxWidth()) { Text("Try again") }
                        }
                    }
                }
            }
            if (state is ModelPackState.Ready) Text("Model ready. You can start recording.")
        }
    }
}

@Composable
@OptIn(ExperimentalMaterial3Api::class)
private fun RecordingScreen(controller: RecordingController, onBack: () -> Unit) {
    val state = controller.state
    Scaffold(topBar = { TopAppBar(title = { Text(if (state.isRecording) "Listening" else "New recording") }) }) { padding ->
        Column(Modifier.fillMaxSize().padding(padding).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            state.error?.let { Card(Modifier.fillMaxWidth()) { Text(it, Modifier.padding(16.dp), color = MaterialTheme.colorScheme.error) } }
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(if (state.isRecording) "Transcribing live" else "Ready to record", style = MaterialTheme.typography.titleLarge)
                    Text("Live words use the device speech recognizer with offline preference. Speaker labels settle when the runtime supports diarization.")
                }
            }
            if (state.segments.isEmpty() && state.partialText.isBlank()) Text("Your transcript will appear here as you speak.", Modifier.padding(vertical = 24.dp))
            else LazyColumn(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                items(state.segments, key = { it.id }) { TranscriptRow(it) }
                if (state.partialText.isNotBlank()) item { Text(state.partialText, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center) {
                if (state.isRecording) Button(onClick = controller::stop, modifier = Modifier.height(52.dp).semantics { contentDescription = "Stop recording" }) { Text("Stop and save") }
                else {
                    Button(onClick = controller::start, modifier = Modifier.height(52.dp).semantics { contentDescription = "Start recording" }) { Text("Start recording") }
                    Spacer(Modifier.width(12.dp)); OutlinedButton(onClick = onBack, modifier = Modifier.height(52.dp)) { Text("Back") }
                }
            }
        }
    }
}

@Composable
@OptIn(ExperimentalMaterial3Api::class)
private fun DetailScreen(session: SavedSession, onBack: () -> Unit) {
    val context = LocalContext.current
    val exported = remember(session.id) { PlainTextTranscriptExporter().sharePayload("Whispr transcript", session) }
    Scaffold(topBar = { TopAppBar(title = { Text("Transcript") }) }) { padding ->
        Column(Modifier.fillMaxSize().padding(padding).padding(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            Text("Session ${session.id.takeLast(6)}", style = MaterialTheme.typography.headlineSmall)
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Button(onClick = { context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).apply { type = exported.mimeType; putExtra(Intent.EXTRA_TEXT, exported.text) }, "Share transcript")) }) { Text("Share") }
                OutlinedButton(onClick = onBack) { Text("Back") }
            }
            LazyColumn(verticalArrangement = Arrangement.spacedBy(10.dp)) { items(session.segments, key = { it.id }) { TranscriptRow(it) } }
        }
    }
}

@Composable
private fun TranscriptRow(segment: FinalTranscriptSegment) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(segment.speakerId ?: "Speaker pending", style = MaterialTheme.typography.labelLarge)
            Text(segment.text)
            Text("${segment.startMs}–${segment.endMs} ms", style = MaterialTheme.typography.bodySmall)
        }
    }
}
