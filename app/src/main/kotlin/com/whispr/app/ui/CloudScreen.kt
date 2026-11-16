package com.whispr.app.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.whispr.app.cloud.CloudClient
import com.whispr.app.data.CloudKeyStore
import com.whispr.app.flow.FlowSettings
import com.whispr.core.cloud.CloudProvider
import com.whispr.core.cloud.CloudProviders
import kotlin.concurrent.thread

/** "off", "always", "fallback" with the artboard's short labels. */
private val Modes = listOf("off" to "Off", "always" to "Always", "fallback" to "No model")

private data class KeyStatus(val text: String, val good: Boolean)

/** A saved key shown safely: its first four and last three characters. */
private fun masked(key: String) = if (key.length <= 8) "••••••••" else key.take(4) + "••••••••" + key.takeLast(3)

/**
 * Cloud providers (PhoneAppB), provider first: when to use the cloud, the providers you have a key for (the default
 * one outlined, with the exact host its audio goes to), then every other provider with "Add key". Keys are tested
 * on adding, and making a provider the default asks consent naming that host. Off by default.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun CloudScreen(settings: FlowSettings, onBack: () -> Unit) {
    val context = LocalContext.current
    var mode by remember { mutableStateOf(settings.cloudMode) }
    var default by remember { mutableStateOf(settings.cloudProvider) }
    var open by rememberSaveable { mutableStateOf<String?>(null) }
    var keys by remember { mutableStateOf(CloudProviders.all.mapNotNull { p -> CloudKeyStore.get(context, p.id)?.let { p.id to it } }.toMap()) }
    val status = remember { mutableStateMapOf<String, KeyStatus>() }
    var testing by remember { mutableStateOf<String?>(null) }
    var asking by remember { mutableStateOf<CloudProvider?>(null) }
    var removing by remember { mutableStateOf<CloudProvider?>(null) }
    var modeNote by remember { mutableStateOf<String?>(null) }

    fun makeDefault(p: CloudProvider) {
        settings.cloudProvider = p.id; default = p.id
        if (mode == "off") { mode = "always"; settings.cloudMode = "always" }
        modeNote = null
    }
    fun test(p: CloudProvider, key: String, model: String) {
        testing = p.id; status.remove(p.id)
        val region = settings.cloudRegion
        thread { // one second of silence; pressing the button is consent for this one request
            val r = CloudClient.test(p, key, model, region)
            status[p.id] = if (r == null) KeyStatus("Connected. The key works.", true) else KeyStatus("Key rejected · $r", false)
            testing = null
        }
    }
    val connected = CloudProviders.all.filter { it.id in keys }.sortedBy { if (it.id == default) 0 else 1 }
    val others = CloudProviders.all.filter { it.id !in keys }

    SubScreen("Cloud providers", null, onBack) {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            // Use cloud for: Off / Always / No model.
            Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(C.Sand).padding(4.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                Modes.forEach { (id, label) ->
                    val on = mode == id
                    val bg by animateColorAsState(if (on) C.Card else Color.Transparent, Motion.quick(), label = "seg")
                    Box(
                        Modifier.weight(1f).heightIn(min = 40.dp).then(if (on) Modifier.shadow(2.dp, RoundedCornerShape(12.dp)) else Modifier)
                            .clip(RoundedCornerShape(12.dp)).background(bg)
                            .selectable(on, role = Role.RadioButton) {
                                val p = CloudProviders.byId(default)
                                if (id != "off" && (p == null || p.id !in keys || !settings.hasCloudConsent(p.id))) modeNote = "Add a key and make a provider your default first."
                                else { mode = id; settings.cloudMode = id; modeNote = null }
                            },
                        contentAlignment = Alignment.Center,
                    ) { Text(label, style = T.Label, color = C.Ink) }
                }
            }
            Text(
                when (mode) {
                    "always" -> "Every dictation goes to your default provider. If it fails, the on-device text is used."
                    "fallback" -> "The cloud is used only while no on-device model is downloaded."
                    else -> "Off: every dictation stays on this phone."
                },
                style = T.Caption.copy(lineHeight = 17.sp), color = C.Muted, modifier = Modifier.padding(horizontal = S.XS),
            )
            modeNote?.let { Text(it, style = T.Body2, color = C.Coral, modifier = Modifier.padding(horizontal = S.XS).semantics { liveRegion = LiveRegionMode.Polite }) }

            connected.forEach { p ->
                ConnectedCard(
                    p, keys.getValue(p.id), isDefault = p.id == default, status = status[p.id], testing = testing == p.id, settings = settings,
                    onReplace = { open = p.id },
                    onDefault = { if (settings.hasCloudConsent(p.id)) makeDefault(p) else asking = p },
                    onRemove = { removing = p },
                )
                if (open == p.id) KeyEditor(p, settings, testing == p.id, status[p.id], replacing = true) { key, model -> CloudKeyStore.set(context, p.id, key); keys = keys + (p.id to key); open = null; test(p, key, model) }
            }
            if (others.isNotEmpty()) Text(if (connected.isEmpty()) "PROVIDERS" else "MORE PROVIDERS", style = T.Overline, color = C.Muted, modifier = Modifier.padding(start = S.XS, top = S.XS))
            others.forEachIndexed { i, p ->
                Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp)).background(C.Card)) {
                    Row(
                        Modifier.fillMaxWidth().clickable(onClickLabel = "Add key") { open = if (open == p.id) null else p.id }.padding(horizontal = S.M, vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(S.M),
                    ) {
                        Logo(p, i + connected.size, 40)
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                            Text(providerTitle(p), style = T.Title.copy(fontSize = 15.sp), color = C.Ink)
                            Text(providerNote(p), style = T.Caption, color = C.Muted, maxLines = 2, overflow = TextOverflow.Ellipsis)
                            Tag(modelOf(p, settings), C.Soft, C.InkSoft)
                        }
                        Text(if (open == p.id) "Close" else "Add key", style = T.Label.copy(fontSize = 13.sp), color = C.AccentInk)
                    }
                    AnimatedVisibility(open == p.id, enter = fadeIn() + expandVertically(), exit = fadeOut() + shrinkVertically()) {
                        KeyEditor(p, settings, testing == p.id, status[p.id], replacing = false, inCard = true) { key, model ->
                            CloudKeyStore.set(context, p.id, key); keys = keys + (p.id to key); open = null; test(p, key, model)
                        }
                    }
                }
            }
            if (CloudProviders.excluded.isNotEmpty()) Column(Modifier.fillMaxWidth().padding(top = S.S).clip(Corners.Item).background(C.Soft).padding(S.L), verticalArrangement = Arrangement.spacedBy(S.XS)) {
                Text("Not included", style = T.Label, color = C.InkSoft)
                CloudProviders.excluded.forEach { e -> Text("${e.name}: ${e.reason}", style = T.Caption, color = C.Muted) }
            }
            Text(
                "Keys are stored encrypted on this phone and sent only to their own provider. Prices and speeds are the providers' own figures.",
                style = T.Caption, color = C.Muted, textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth().padding(top = S.S),
            )
        }
    }
    asking?.let { p ->
        ConfirmDialog(
            "Send audio to ${p.maker}?",
            "While cloud is on, each dictation's audio goes to ${p.hostFor(settings.cloudRegion)} (${providerTitle(p)}, ${modelOf(p, settings)}) under ${p.maker}'s terms and your account. Voicio sends it nowhere else. You can turn it off any time.",
            "I agree", onDismiss = { asking = null }, destructive = false,
        ) { settings.giveCloudConsent(p.id); makeDefault(p) }
    }
    removing?.let { p ->
        ConfirmDialog("Remove the ${p.maker} key?", if (p.id == default) "This is your default provider, so cloud transcription turns off." else "You can add it again any time.", "Remove", onDismiss = { removing = null }) {
            CloudKeyStore.set(context, p.id, null); keys = keys - p.id; status.remove(p.id)
            if (p.id == default) { settings.cloudProvider = ""; default = ""; settings.cloudMode = "off"; mode = "off" }
        }
    }
}

@Composable
private fun Logo(p: CloudProvider, index: Int, size: Int) {
    Box(Modifier.size(size.dp).clip(RoundedCornerShape((size * 0.32f).dp)).background(ProviderTints[index % ProviderTints.size]), contentAlignment = Alignment.Center) {
        Text(providerMark(p.id, p.maker), style = T.Label.copy(fontSize = if (size >= 44) 15.sp else 12.sp, fontWeight = FontWeight.ExtraBold), color = C.Ink)
    }
}

/** A provider with a saved key: outlined when it is the default, masked key, where audio goes, and the actions. */
@Composable
private fun ConnectedCard(
    p: CloudProvider, key: String, isDefault: Boolean, status: KeyStatus?, testing: Boolean, settings: FlowSettings,
    onReplace: () -> Unit, onDefault: () -> Unit, onRemove: () -> Unit,
) {
    var model by remember(p.id) { mutableStateOf(modelOf(p, settings)) }
    Column(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(22.dp)).background(C.Card).border(2.5.dp, if (isDefault) C.Ink else Color.Transparent, RoundedCornerShape(22.dp)).padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(S.M)) {
            Logo(p, 0, 44)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(providerTitle(p), style = T.Title.copy(fontWeight = FontWeight.ExtraBold), color = C.Ink)
                Text(providerNote(p), style = T.Caption, color = C.Muted, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
            when {
                testing -> Tag("Testing…", C.Soft, C.InkSoft)
                status?.good == false -> Tag("Key rejected", C.CoralSoft, C.Coral)
                else -> Tag("Connected", C.Lime, C.LimeInk)
            }
        }
        Row(
            Modifier.fillMaxWidth().heightIn(min = 46.dp).clip(RoundedCornerShape(14.dp)).background(C.Soft).padding(start = S.M),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(masked(key), style = T.Body2.copy(letterSpacing = 2.sp), color = C.InkSoft, modifier = Modifier.weight(1f))
            Box(Modifier.heightIn(min = 46.dp).clip(Corners.Control).clickable(role = Role.Button, onClick = onReplace).padding(horizontal = S.M), contentAlignment = Alignment.Center) {
                Text("Replace", style = T.Label.copy(fontSize = 13.sp), color = C.AccentInk)
            }
        }
        status?.takeIf { !it.good }?.let { Text(it.text, style = T.Body2, color = C.Coral, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }) }
        ModelPicker(p, model) { model = it; settings.setCloudModelFor(p.id, it) }
        Text(
            "While on, each dictation's audio goes to ${p.hostFor(settings.cloudRegion)} under ${p.maker}'s terms. Voicio sends it nowhere else. If ${p.maker} fails, your on-device text is used.",
            style = T.Caption.copy(lineHeight = 17.sp), color = C.InkSoft,
        )
        Text(p.priceNote, style = T.Caption.copy(lineHeight = 17.sp), color = C.Muted)
        LiveToggle(p, settings)
        Row(horizontalArrangement = Arrangement.spacedBy(S.S)) {
            Box(
                Modifier.weight(1f).heightIn(min = 44.dp).clip(RoundedCornerShape(14.dp)).background(if (isDefault) C.Ink else C.Ink)
                    .then(if (isDefault) Modifier else Modifier.pressable(onDefault)),
                contentAlignment = Alignment.Center,
            ) { Text(if (isDefault) "Default provider" else "Make default", style = T.Button.copy(fontSize = 14.sp), color = Color.White) }
            Box(
                Modifier.heightIn(min = 44.dp).clip(RoundedCornerShape(14.dp)).background(C.Soft).pressable(onRemove).padding(horizontal = 14.dp),
                contentAlignment = Alignment.Center,
            ) { Text("Remove", style = T.Button.copy(fontSize = 14.sp), color = C.Coral) }
        }
    }
}

/** The company first (users know "OpenAI", not "GPT Transcribe"); the model is a setting inside. */
internal fun providerTitle(p: CloudProvider) = when (p.id) {
    "gemini" -> "Google Gemini"
    "lemonfoxeu" -> "Lemonfox (EU)"
    else -> p.maker
}

/** Speed, then price, in a few words. */
private fun providerNote(p: CloudProvider) = p.speedNote.substringBefore(":") + " · " + p.priceNote.substringBefore(";").substringBefore(" (see")

/** The model saved for [p], or its default. */
private fun modelOf(p: CloudProvider, settings: FlowSettings) = settings.cloudModelFor(p.id).takeIf { it in p.models } ?: p.defaultModel

/** Model: one chip per model the provider offers; a single model shows as a plain line. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ModelPicker(p: CloudProvider, model: String, onPick: (String) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text("Model", style = T.Label, color = C.InkSoft)
        if (p.models.size <= 1) Text(p.defaultModel, style = T.Body2, color = C.Ink)
        else FlowRow(horizontalArrangement = Arrangement.spacedBy(S.S), verticalArrangement = Arrangement.spacedBy(S.S)) {
            p.models.forEach { m ->
                val on = m == model
                Box(
                    Modifier.heightIn(min = 40.dp).clip(Corners.Pill).background(if (on) C.Ink else C.Soft)
                        .selectable(on, role = Role.RadioButton) { onPick(m) }.padding(horizontal = 14.dp, vertical = 10.dp),
                    contentAlignment = Alignment.Center,
                ) { Text(m, style = T.Label, color = if (on) Color.White else C.Ink) }
            }
        }
    }
}

/** Streaming from this provider, where its API supports it: words appear while you speak. */
@Composable
private fun LiveToggle(p: CloudProvider, settings: FlowSettings) {
    if (!p.liveSupported) return
    var on by remember(p.id) { mutableStateOf(settings.isCloudLive(p.id)) }
    Row(
        Modifier.fillMaxWidth().heightIn(min = 52.dp).clip(RoundedCornerShape(14.dp)).background(C.Soft)
            .toggleable(on, role = Role.Switch) { on = it; settings.setCloudLive(p.id, it) }.padding(horizontal = S.M, vertical = S.S),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(S.M),
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(S.S)) {
                Text("Live", style = T.Title.copy(fontSize = 15.sp), color = C.Ink)
                if (p.id == "openai" || p.id == "gemini") Tag("Experimental", C.Butter, C.ButterInk)
            }
            Text("Words appear while you speak", style = T.Caption, color = C.Muted)
        }
        SmallSwitch(on)
    }
}

/** Model, (Azure) region and key for one provider; "Add and test" saves the key and tests it straight away. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun KeyEditor(
    p: CloudProvider, settings: FlowSettings, testing: Boolean, status: KeyStatus?, replacing: Boolean, inCard: Boolean = false,
    onAdd: (key: String, model: String) -> Unit,
) {
    var key by remember(p.id) { mutableStateOf("") }
    var model by remember(p.id) { mutableStateOf(modelOf(p, settings)) }
    var region by remember(p.id) { mutableStateOf(settings.cloudRegion) }
    Column(
        Modifier.fillMaxWidth().then(if (inCard) Modifier.padding(start = S.M, end = S.M, bottom = S.M) else Modifier.clip(RoundedCornerShape(18.dp)).background(C.Card).padding(S.M)),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).background(C.Soft).padding(S.M), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(p.priceNote, style = T.Caption.copy(lineHeight = 17.sp), color = C.InkSoft)
            Text(p.speedNote, style = T.Caption.copy(lineHeight = 17.sp), color = C.InkSoft)
            if (p.experimental) Text("Experimental: not every detail is confirmed by the provider's docs.", style = T.Caption, color = C.ButterInk)
        }
        ModelPicker(p, model) { model = it; settings.setCloudModelFor(p.id, it) }
        LiveToggle(p, settings)
        if (p.needsRegion) Field(region, { region = it; settings.cloudRegion = it }, "Region, e.g. westeurope", singleLine = true, capitalization = KeyboardCapitalization.None)
        Field(key, { key = it }, if (replacing) "Paste the new ${p.maker} key" else "Paste your ${p.maker} key", mask = true)
        Box(
            Modifier.fillMaxWidth().heightIn(min = 46.dp).clip(RoundedCornerShape(14.dp)).background(if (key.isNotBlank() && !testing && (!p.needsRegion || region.isNotBlank())) C.Ink else C.Step)
                .then(if (key.isNotBlank() && !testing && (!p.needsRegion || region.isNotBlank())) Modifier.pressable { onAdd(key.trim(), model); key = "" } else Modifier),
            contentAlignment = Alignment.Center,
        ) { Text(if (testing) "Testing…" else "Add and test", style = T.Button.copy(fontSize = 14.sp), color = if (key.isNotBlank()) Color.White else C.Muted) }
        status?.let { Text(it.text, style = T.Body2, color = if (it.good) C.LimeInk else C.Coral, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }) }
    }
}
