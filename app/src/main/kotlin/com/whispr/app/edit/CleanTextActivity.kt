package com.whispr.app.edit

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.fadeIn
import androidx.compose.animation.slideInVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.heightIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import kotlin.concurrent.thread
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.whispr.app.flow.FlowSettings
import com.whispr.app.ui.C
import com.whispr.app.ui.GhostButton
import com.whispr.app.ui.PrimaryButton
import com.whispr.app.ui.T
import com.whispr.app.ui.VoicioTheme
import com.whispr.app.edit.LlmTextEditor
import com.whispr.core.edit.EditMode
import com.whispr.core.edit.EditRequest
import com.whispr.core.edit.EditResult
import com.whispr.core.edit.GuardedEditor
import com.whispr.core.edit.RulesEditor
import com.whispr.core.edit.TextEditor
import com.whispr.core.edit.text

/**
 * "Voicio: Clean up": appears in the text-selection menu of other apps (and as a share target).
 * Cleans the selected text with the same pipeline as dictation, shows before and after, and either
 * replaces the selection (editable fields) or copies the result.
 */
class CleanTextActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val incoming = intent.getCharSequenceExtra(Intent.EXTRA_PROCESS_TEXT) ?: intent.getCharSequenceExtra(Intent.EXTRA_TEXT)
        val original = incoming?.toString().orEmpty()
        val readOnly = intent.getBooleanExtra(Intent.EXTRA_PROCESS_TEXT_READONLY, true)
        val canReplace = intent.action == Intent.ACTION_PROCESS_TEXT && !readOnly
        val settings = FlowSettings(this)
        val rules = RulesEditor(settings.dictionary())
        val smart = SmartEditModel.installed(this)
        val ai = GuardedEditor(model = LlmTextEditor(this), rules = rules)

        setContent {
            VoicioTheme {
                var result by remember { mutableStateOf<EditResult>(rules.edit(EditRequest(original))) }
                var busy by remember { mutableStateOf(false) }
                var mode by remember { mutableStateOf("Basic") }
                val run = { label: String, m: EditMode ->
                    mode = label; busy = true
                    thread {
                        val r = try { ai.edit(EditRequest(original, m)) } catch (e: Exception) {
                            EditResult.Kept(rules.edit(EditRequest(original)).text, "Could not use the AI model (${e.message ?: "error"}), so only the basic clean-up was applied.")
                        }
                        runOnUiThread { result = r; busy = false }
                    }
                    Unit
                }
                Sheet(original, result, canReplace, smart, busy, mode,
                    onBasic = { mode = "Basic"; result = rules.edit(EditRequest(original)) },
                    onMode = run,
                    onReplace = { text ->
                        setResult(RESULT_OK, Intent().putExtra(Intent.EXTRA_PROCESS_TEXT, text)); finish()
                    },
                    onCopy = { text ->
                        getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Voicio", text))
                        Toast.makeText(this, "Copied", Toast.LENGTH_SHORT).show(); finish()
                    },
                    onClose = { finish() })
            }
        }
    }
}

@Composable
private fun Sheet(original: String, result: EditResult, canReplace: Boolean, smart: Boolean, busy: Boolean, mode: String, onBasic: () -> Unit, onMode: (String, EditMode) -> Unit, onReplace: (String) -> Unit, onCopy: (String) -> Unit, onClose: () -> Unit) {
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { shown = true }
    val cleaned = result.text
    val unchanged = cleaned == original.trim()
    Box(Modifier.fillMaxSize().clickable(indication = null, interactionSource = remember { androidx.compose.foundation.interaction.MutableInteractionSource() }) { onClose() }, contentAlignment = Alignment.BottomCenter) {
        AnimatedVisibility(shown, enter = fadeIn() + slideInVertically { it / 2 }) {
            Column(
                Modifier.fillMaxWidth().clip(RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp)).background(C.Bg)
                    .clickable(indication = null, interactionSource = remember { androidx.compose.foundation.interaction.MutableInteractionSource() }) { }
                    .padding(20.dp).navigationBarsPadding(),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text("Clean up", style = T.H2, color = C.Ink)
                if (original.isBlank()) {
                    Text("Select some text first, then choose Voicio: Clean up.", style = T.Body, color = C.Muted)
                    PrimaryButton("Close", onClose)
                } else {
                    if (smart) Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        listOf("Basic" to null, "AI clean" to EditMode.Clean, "Proofread" to EditMode.Proofread, "Shorten" to EditMode.Rewrite).forEach { (label, m) ->
                            val on = mode == label
                            Box(
                                Modifier.clip(RoundedCornerShape(18.dp)).background(if (on) C.Ink else C.Card)
                                    .clickable(enabled = !busy) { if (m == null) onBasic() else onMode(label, m) }.padding(horizontal = 12.dp, vertical = 8.dp),
                            ) { Text(label, style = T.Label.copy(fontSize = 12.sp), color = if (on) androidx.compose.ui.graphics.Color.White else C.InkSoft) }
                        }
                    }
                    Label("Before"); Card(original, C.Card, C.Muted)
                    Label(if (busy) "Working… (the AI model takes a few seconds)" else if (unchanged) "After (already clean)" else "After"); Card(if (busy) "…" else cleaned, C.Peach, C.Ink)
                    if (result is EditResult.Kept) Text(result.reason, style = T.Small, color = C.Amber)
                    if (canReplace) PrimaryButton("Replace selection", { onReplace(cleaned) }, enabled = !busy)
                    PrimaryButton(if (canReplace) "Copy instead" else "Copy", { onCopy(cleaned) }, color = if (canReplace) C.Card else C.Ink, ink = if (canReplace) C.Ink else androidx.compose.ui.graphics.Color.White)
                    GhostButton("Cancel", onClose)
                }
            }
        }
    }
}

@Composable private fun Label(t: String) = Text(t, style = T.Label.copy(fontSize = 12.sp), color = C.Muted)

@Composable
private fun Card(text: String, fill: androidx.compose.ui.graphics.Color, ink: androidx.compose.ui.graphics.Color) {
    Box(Modifier.fillMaxWidth().heightIn(max = 160.dp).clip(RoundedCornerShape(16.dp)).background(fill).verticalScroll(rememberScrollState()).padding(14.dp)) {
        Text(text, style = T.Body, color = ink)
    }
}
