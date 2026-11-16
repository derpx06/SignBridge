package com.whispr.app.ui

import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.background
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.whispr.app.edit.SmartEditInstaller
import com.whispr.app.edit.SmartEditModel
import com.whispr.app.flow.FlowSettings
import kotlin.concurrent.thread

/** The install runs on its own thread; this keeps its progress when the user leaves and comes back. */
private object SmartEditJob {
    var progress by mutableStateOf<Float?>(null)
    var error by mutableStateOf<String?>(null)
}

/** A scrolling sub-screen under a back [TopBar]. */
@Composable
internal fun SubScreen(title: String, subtitle: String?, onBack: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    val scroll = rememberScrollState()
    Column(Modifier.fillMaxSize()) {
        TopBar(title, subtitle = subtitle, onBack = onBack, lifted = scroll.value > 0)
        Column(
            Modifier.fillMaxSize().imePadding().verticalScroll(scroll).navigationBarsPadding().padding(start = S.L, end = S.L, top = S.S, bottom = S.XL),
            verticalArrangement = Arrangement.spacedBy(S.XL), content = content,
        )
    }
}

/** The optional on-device AI model: what it does, what it costs, and how to turn it on or remove it. */
@Composable
internal fun SmartEditScreen(settings: FlowSettings, onBack: () -> Unit) {
    val context = LocalContext.current
    val p = SmartEditJob.progress
    var installed by remember { mutableStateOf(SmartEditModel.installed(context)) }
    androidx.compose.runtime.LaunchedEffect(p == null) { if (p == null) installed = SmartEditModel.installed(context) }
    var polish by remember { mutableStateOf(settings.aiPolish) }
    var confirm by remember { mutableStateOf(false) }

    SubScreen("Smart Edit", "Optional AI, on this phone", onBack) {
        Column(
            Modifier.fillMaxWidth().clip(Corners.Card).panel(Panel.Balanced).padding(S.L + S.XS).enter(0),
            verticalArrangement = Arrangement.spacedBy(S.M),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("S1-mini", style = T.H1, color = C.Ink)
                    Text("English clean-up · by Superwhisper", style = T.Body2, color = C.InkSoft)
                }
                Lattice(size = 64.dp, square = true, active = p != null, level = 0.3f, sweep = p != null)
            }
            Text(
                "Fixes grammar and wording, and can clean up or shorten text you select in other apps. Works offline. The basic tidy-up is always on and needs nothing.",
                style = T.Body2, color = C.InkSoft,
            )
            Box(Modifier.fillMaxWidth().height(1.5.dp).background(C.Ink.copy(alpha = 0.1f)))
            Text("484 MB download · 357 MB on this phone · ~400 MB memory while working", style = T.Label.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.SemiBold), color = C.InkSoft)
        }
        AnimatedContent(Triple(p != null, installed, SmartEditJob.error), label = "smart", modifier = Modifier.enter(2)) { (busy, have, err) ->
            Column(verticalArrangement = Arrangement.spacedBy(S.M)) {
                when {
                    busy -> CardBox {
                        Column(Modifier.padding(S.L), verticalArrangement = Arrangement.spacedBy(S.M)) {
                            val v = SmartEditJob.progress ?: 0f
                            Text(if (v >= 0.9f) "Making it smaller for your phone…" else "Downloading · ${(v * 100 / 0.9f).toInt().coerceAtMost(100)}%", style = T.Title, color = C.Ink)
                            LinearProgressIndicator(progress = { v }, modifier = Modifier.fillMaxWidth().height(6.dp).clip(Corners.Pill), color = C.Accent, trackColor = C.Track, drawStopIndicator = {}, gapSize = 0.dp)
                            Text("You can leave this screen. It keeps going.", style = T.Body2, color = C.Muted)
                        }
                    }
                    have -> {
                        CardBox { SwitchRow("Polish dictations with AI", "Adds about 3 seconds. Uses about 400 MB while working, freed 45 seconds later.", polish) { polish = it; settings.aiPolish = it } }
                        GhostButton("Remove the model (frees 357 MB)", { confirm = true }, color = C.Coral)
                    }
                    else -> {
                        if (err != null) Notice(NoticeKind.Error, "Download failed", body = err)
                        PrimaryButton(if (err != null) "Try again" else "Download (484 MB)", {
                            SmartEditJob.error = null; SmartEditJob.progress = 0f
                            val app = context.applicationContext
                            thread {
                                try { SmartEditInstaller.install(app) { v -> SmartEditJob.progress = v } }
                                catch (e: Exception) { SmartEditJob.error = e.message ?: "Download failed" }
                                SmartEditJob.progress = null
                            }
                        })
                        Text("Use Wi-Fi. Needs about 900 MB free while installing, 357 MB after.", style = T.Caption, color = C.Muted, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth())
                    }
                }
            }
        }
    }
    if (confirm) ConfirmDialog("Remove Smart Edit?", "Frees 357 MB. You can download it again any time.", "Remove", onDismiss = { confirm = false }) {
        SmartEditInstaller.uninstall(context); installed = false; polish = false; settings.aiPolish = false
    }
}
