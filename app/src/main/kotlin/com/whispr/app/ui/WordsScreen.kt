package com.whispr.app.ui

import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.background
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
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.whispr.app.flow.FlowSettings
import com.whispr.app.data.Snippet
import com.whispr.app.data.SnippetStore

private val WordTints = listOf(C.Peach, C.Sky, C.Lime, C.Butter)

/** Words and snippets (PhoneAppB): names Voicio should get right, and spoken shortcuts for longer text. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun WordsScreen(settings: FlowSettings, snacks: Snacks, onBack: () -> Unit) {
    var tab by rememberSaveable { mutableIntStateOf(0) }
    SubScreen("Words and snippets", null, onBack) {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(C.Sand).padding(4.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                listOf("Words", "Snippets").forEachIndexed { i, label ->
                    val on = tab == i
                    val bg by animateColorAsState(if (on) C.Card else Color.Transparent, Motion.quick(), label = "tab")
                    Box(
                        Modifier.weight(1f).heightIn(min = 42.dp).clip(RoundedCornerShape(12.dp)).background(bg).selectable(on, role = Role.Tab) { tab = i },
                        contentAlignment = Alignment.Center,
                    ) { Text(label, style = T.Label.copy(fontSize = 13.sp), color = C.Ink) }
                }
            }
            if (tab == 0) WordsTab(settings, snacks) else SnippetsTab(snacks)
        }
    }
}

/** A peach-washed white card for adding something. */
@Composable
private fun AddCard(title: String, body: String, content: @Composable () -> Unit) {
    Column(
        Modifier.fillMaxWidth().clip(Corners.Item).background(C.Card)
            .background(Brush.radialGradient(listOf(C.Peach, C.Peach.copy(alpha = 0f)), Offset.Zero, 520f))
            .padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(S.S),
    ) {
        Text(title, style = T.Label.copy(fontSize = 13.sp), color = C.Ink)
        Text(body, style = T.Caption.copy(lineHeight = 17.sp), color = C.InkSoft)
        content()
    }
}

@Composable
private fun AddButton(enabled: Boolean, onClick: () -> Unit) {
    Box(
        Modifier.heightIn(min = 52.dp).clip(RoundedCornerShape(13.dp)).background(if (enabled) C.Ink else C.Step)
            .then(if (enabled) Modifier.pressable(onClick) else Modifier.semantics { contentDescription = "Add" }).padding(horizontal = S.L),
        contentAlignment = Alignment.Center,
    ) { Text("Add", style = T.Button.copy(fontSize = 15.sp), color = if (enabled) Color.White else C.Muted) }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun WordsTab(settings: FlowSettings, snacks: Snacks) {
    var raw by remember { mutableStateOf(settings.dictionaryText) }
    val pairs = remember(raw) { raw.lines().mapNotNull { l -> l.split("=>", limit = 2).takeIf { it.size == 2 && it[0].isNotBlank() }?.let { it[0].trim() to it[1].trim() } }.distinct() }
    fun save(list: List<Pair<String, String>>) { raw = list.distinct().joinToString("\n") { "${it.first} => ${it.second}" }; settings.dictionaryText = raw }
    var word by rememberSaveable { mutableStateOf("") }
    var heard by rememberSaveable { mutableStateOf("") }
    AddCard("Teach a word", "Names and terms Voicio should always get right. If it mishears one, add how it hears it too.") {
        Field(word, { word = it }, "e.g. Kubernetes", singleLine = true, capitalization = KeyboardCapitalization.None, soft = true)
        Row(horizontalArrangement = Arrangement.spacedBy(S.S), verticalAlignment = Alignment.CenterVertically) {
            Field(heard, { heard = it }, "Heard as (optional), e.g. cube ernetes", Modifier.weight(1f), singleLine = true, capitalization = KeyboardCapitalization.None, soft = true)
            AddButton(word.isNotBlank()) {
                val w = word.trim()
                save(pairs + ((heard.trim().ifBlank { w.lowercase() }) to w)); word = ""; heard = ""
            }
        }
    }
    if (pairs.isEmpty()) {
        Text("No words yet. Add a name, a brand or a term you use.", style = T.Body2, color = C.Muted, modifier = Modifier.padding(horizontal = S.XS, vertical = S.S))
    } else {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(S.S), verticalArrangement = Arrangement.spacedBy(S.S)) {
            pairs.forEachIndexed { i, p ->
                val label = if (p.first.equals(p.second, ignoreCase = true)) p.second else "${p.first} → ${p.second}"
                Text(
                    label, style = T.Body2.copy(fontWeight = FontWeight.Bold), color = C.Ink,
                    modifier = Modifier.clip(RoundedCornerShape(13.dp)).background(WordTints[i % WordTints.size])
                        .clickable(onClickLabel = "Remove ${p.second}") {
                            val before = pairs
                            save(pairs - p)
                            snacks.show("Removed \"${p.second}\"", undo = { save(before) })
                        }.padding(horizontal = S.M, vertical = 9.dp),
                )
            }
        }
        Text("Tap a word to remove it.", style = T.Caption, color = C.Muted, modifier = Modifier.padding(horizontal = S.XS))
    }
}

@Composable
private fun SnippetsTab(snacks: Snacks) {
    val context = LocalContext.current
    androidx.compose.runtime.LaunchedEffect(Unit) { SnippetStore.ensure(context) }
    var trigger by rememberSaveable { mutableStateOf("") }
    var text by rememberSaveable { mutableStateOf("") }
    AddCard("Add a snippet", "Say the trigger and Voicio types the whole text, for example \u201cmy address\u201d.") {
        Field(trigger, { trigger = it }, "Trigger, e.g. my address", singleLine = true, soft = true)
        Field(text, { text = it }, "The text it types", minLines = 2, soft = true)
        Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.CenterEnd) {
            AddButton(trigger.isNotBlank() && text.isNotBlank()) {
                SnippetStore.put(context, Snippet(trigger.trim(), text.trim())); trigger = ""; text = ""
            }
        }
    }
    if (SnippetStore.items.isEmpty()) Text("No snippets yet.", style = T.Body2, color = C.Muted, modifier = Modifier.padding(horizontal = S.XS, vertical = S.S))
    SnippetStore.items.sortedBy { it.trigger.lowercase() }.forEachIndexed { i, sn ->
        Row(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp)).background(C.Card).padding(start = 14.dp, top = S.M, bottom = S.M, end = S.XS),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(S.M),
        ) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Tag(sn.trigger, WordTints[i % WordTints.size], C.Ink)
                Text(sn.text, style = T.Body2, color = C.InkSoft, maxLines = 3)
            }
            Box(Modifier.iconButton("Remove ${sn.trigger}") {
                SnippetStore.remove(context, sn)
                snacks.show("Removed \"${sn.trigger}\"", undo = { SnippetStore.put(context, sn) })
            }, contentAlignment = Alignment.Center) { Glyph(Icons.Close, C.Ink, 14.dp, 2.4f) }
        }
    }
}
