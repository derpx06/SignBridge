package com.whispr.app.ui

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import com.whispr.app.data.Note
import com.whispr.app.data.NotesStore
import com.whispr.app.edit.LlmTextEditor
import com.whispr.app.edit.SmartEditModel
import com.whispr.app.engine.NoteRecorder
import com.whispr.app.flow.FlowSettings
import com.whispr.core.dictation.NoteFormat
import com.whispr.core.dictation.Pipeline
import com.whispr.core.edit.EditMode
import com.whispr.core.edit.EditRequest
import com.whispr.core.edit.EditResult
import com.whispr.core.edit.GuardedEditor
import com.whispr.core.edit.RulesEditor
import com.whispr.core.edit.text
import kotlin.concurrent.thread

/** The newest words of a long live transcript, starting at a word boundary. */
private fun tail(text: String, max: Int = 140): String {
    val t = text.trim()
    if (t.length <= max) return t
    return "…" + t.takeLast(max).let { it.substringAfter(' ', it) }
}

/**
 * A voice-powered note: type, or tap the microphone and talk. What you say is cleaned, formatted
 * (paragraphs, bullet lists, numbered steps) and added to the note. With Smart Edit installed the
 * note can also be cleaned, proofread or shortened by the on-device AI. Back saves; everything stays on the phone.
 */
@Composable
fun NoteEditor(note: Note, onDismiss: () -> Unit, onDeleted: (Note) -> Unit) {
    val context = LocalContext.current
    val settings = remember { FlowSettings(context) }
    val recorder = remember { NoteRecorder(context) }
    DisposableEffect(Unit) { onDispose { recorder.cancel() } }
    val smart = remember { SmartEditModel.installed(context) }
    val exists = NotesStore.items.any { it.id == note.id }

    var title by rememberSaveable { mutableStateOf(note.title) }
    var body by rememberSaveable { mutableStateOf(note.body) }
    var live by remember { mutableStateOf("") }
    var level by remember { mutableFloatStateOf(0f) }
    var phase by remember { mutableStateOf(Mic.Idle) }
    var message by remember { mutableStateOf<Pair<String, Boolean>?>(null) } // text, is it an error
    var aiBusy by remember { mutableStateOf<String?>(null) }
    var menu by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }

    fun info(text: String) { message = text to false }
    fun error(text: String) { message = text to true }
    fun addSpoken(text: String) {
        if (text.isBlank()) { error("Didn't catch that. Try again, a little closer."); return }
        body = if (body.isBlank()) text else body.trimEnd() + "\n\n" + text
        if (title.isBlank()) title = NoteFormat.structure(text).title
        message = null
    }
    fun begin() {
        phase = Mic.Listening; live = ""; message = null
        recorder.start(onLive = { live = it }, onLevel = { level = it }, onError = { phase = Mic.Idle; level = 0f; error(it) })
    }
    fun finish() {
        phase = Mic.Working; level = 0f
        recorder.stop(settings, polishWithAi = settings.aiPolish && smart) { text -> phase = Mic.Idle; live = ""; addSpoken(text) }
    }
    val askMic = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { ok -> if (ok) begin() else error("Allow the microphone to dictate notes.") }
    fun toggleMic() {
        when (phase) {
            Mic.Idle -> if (context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) begin() else askMic.launch(Manifest.permission.RECORD_AUDIO)
            Mic.Listening -> finish()
            Mic.Working -> Unit
        }
    }
    fun tidy() {
        val cleaned = Pipeline.process(body, settings.dictionary(), Pipeline.Options(cleanup = settings.cleanup, format = settings.smartFormat)).text
        val s = NoteFormat.structure(cleaned)
        body = s.body
        if (title.isBlank()) title = s.title
        info("Tidied.")
    }
    fun ai(label: String, mode: EditMode) {
        if (aiBusy != null || body.isBlank()) return
        aiBusy = label; message = null
        val text = body
        val app = context.applicationContext
        thread {
            val editor = GuardedEditor(LlmTextEditor(app), RulesEditor(settings.dictionary()))
            val r = try { editor.edit(EditRequest(text, mode)) } catch (e: Throwable) {
                EditResult.Kept(text, "Not enough free memory for the AI model. Close a few apps and try again.")
            }
            android.os.Handler(android.os.Looper.getMainLooper()).post {
                aiBusy = null
                body = r.text
                if (r is EditResult.Kept) error(r.reason) else info("$label done.")
            }
        }
    }
    fun save() {
        if (phase != Mic.Idle) { recorder.cancel(); phase = Mic.Idle }
        if (title.isNotBlank() || body.isNotBlank()) NotesStore.upsert(note.copy(title = title.trim(), body = body.trim(), at = System.currentTimeMillis()))
        onDismiss()
    }
    BackHandler { save() }

    Column(Modifier.fillMaxSize().background(C.Bg).imePadding()) {
        TopBar(if (exists) "Note" else "New note", onBack = { save() }) {
            Box {
                Box(Modifier.iconButton("More options") { menu = true }, contentAlignment = Alignment.Center) { Glyph(Icons.More, C.Ink, 24.dp, 3f) }
                DropdownMenu(menu, { menu = false }, containerColor = C.Card) {
                    DropdownMenuItem(
                        text = { Text("Copy", style = T.Body, color = C.Ink) }, leadingIcon = { Glyph(Icons.Copy, C.Ink, 20.dp) },
                        onClick = { menu = false; copyText(context, listOf(title, body).filter { it.isNotBlank() }.joinToString("\n\n")); info("Copied.") },
                    )
                    if (exists) DropdownMenuItem(
                        text = { Text("Delete", style = T.Body, color = C.Coral) }, leadingIcon = { Glyph(Icons.Trash, C.Coral, 20.dp) },
                        onClick = { menu = false; confirmDelete = true },
                    )
                }
            }
        }
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = S.L, vertical = S.S), verticalArrangement = Arrangement.spacedBy(S.M)) {
            Plain(title, { title = it }, "Title", T.H1, KeyboardCapitalization.Words, single = true)
            Plain(body, { body = it }, "Tap the microphone and talk, or type here", T.Body, KeyboardCapitalization.Sentences, Modifier.heightIn(min = 240.dp))
        }
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = S.M), horizontalArrangement = Arrangement.spacedBy(S.XS)) {
            val can = aiBusy == null && body.isNotBlank()
            Tool("Tidy up", can) { tidy() }
            if (smart) {
                Tool(if (aiBusy == "AI clean") "Working…" else "AI clean", can) { ai("AI clean", EditMode.Clean) }
                Tool(if (aiBusy == "Proofread") "Working…" else "Proofread", can) { ai("Proofread", EditMode.Proofread) }
                Tool(if (aiBusy == "Shorten") "Working…" else "Shorten", can) { ai("Shorten", EditMode.Rewrite) }
            }
        }
        AnimatedVisibility(message != null, enter = fadeIn() + slideInVertically { it / 2 }, exit = fadeOut() + slideOutVertically { it / 2 }) {
            val (text, bad) = message ?: ("" to false)
            Text(text, style = T.Body2, color = if (bad) C.Coral else C.Muted, modifier = Modifier.padding(horizontal = S.L + S.XS, vertical = S.XS).semantics { liveRegion = LiveRegionMode.Polite })
        }
        Column(Modifier.fillMaxWidth().navigationBarsPadding().padding(top = S.S, bottom = S.M), horizontalAlignment = Alignment.CenterHorizontally) {
            AnimatedVisibility(phase == Mic.Listening && live.isNotBlank(), enter = fadeIn() + slideInVertically { it / 2 }, exit = fadeOut()) {
                Box(Modifier.padding(start = S.L, end = S.L, bottom = S.M).widthIn(max = 360.dp).clip(Corners.Item).background(C.OverlayBg).padding(horizontal = S.L, vertical = S.M)) {
                    Text(tail(live), style = T.Body2, color = C.OverlayInk, maxLines = 3)
                }
            }
            val pop by animateFloatAsState(if (phase == Mic.Listening) 1f + level * 0.12f else 1f, Motion.snappy(), label = "micPop")
            Box(
                Modifier.size(72.dp).scale(pop).shadow(10.dp, CircleShape).clip(CircleShape).background(C.Ink)
                    .semantics { contentDescription = if (phase == Mic.Idle) "Start dictating" else "Stop dictating" }.pressable { toggleMic() },
                contentAlignment = Alignment.Center,
            ) {
                AnimatedContent(phase == Mic.Idle, transitionSpec = { fadeIn(Motion.quick()) togetherWith fadeOut(Motion.quick()) }, label = "mic") { idle ->
                    if (idle) Glyph(Icons.Mic, Color.White, 30.dp, 2.2f)
                    else Lattice(size = 64.dp, active = true, level = level, tile = null, sweep = phase == Mic.Working)
                }
            }
            Text(
                when (phase) { Mic.Idle -> "Tap to dictate"; Mic.Listening -> "Listening · tap to finish"; Mic.Working -> "Writing it up…" },
                style = T.Label, color = C.Muted, modifier = Modifier.padding(top = S.S),
            )
        }
    }
    if (confirmDelete) ConfirmDialog("Delete this note?", "You can undo this for a few seconds.", "Delete", onDismiss = { confirmDelete = false }) {
        recorder.cancel()
        val gone = NotesStore.items.firstOrNull { it.id == note.id } ?: note
        NotesStore.remove(note); onDeleted(gone)
    }
}

private enum class Mic { Idle, Listening, Working }

/** A borderless field: the note reads like a page, not a form. */
@Composable
private fun Plain(value: String, onChange: (String) -> Unit, hint: String, style: androidx.compose.ui.text.TextStyle, caps: KeyboardCapitalization, modifier: Modifier = Modifier, single: Boolean = false) {
    Box(modifier.fillMaxWidth()) {
        if (value.isEmpty()) Text(hint, style = style, color = C.Muted)
        BasicTextField(
            value, onChange, textStyle = style.copy(color = C.Ink), cursorBrush = SolidColor(C.Accent), singleLine = single,
            keyboardOptions = KeyboardOptions(capitalization = caps), modifier = Modifier.fillMaxWidth().semantics { contentDescription = hint },
        )
    }
}

@Composable
private fun Tool(text: String, enabled: Boolean, onClick: () -> Unit) {
    if (enabled) TonalButton(text, onClick, fill = C.Card) else TonalButton(text, {}, fill = C.Line, ink = C.Muted)
}
