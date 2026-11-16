package com.whispr.app.snap

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.whispr.app.flow.FlowField
import com.whispr.app.flow.ShadowPad
import com.whispr.app.flow.FlowState
import com.whispr.app.flow.Notice
import com.whispr.app.flow.NoticeKind
import com.whispr.app.flow.Phase
import com.whispr.app.flow.wantsBar
import com.whispr.app.ui.VoicioTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

private val ONE_LINE = "Running about ten minutes late"
private val TWO_LINES = "Running about ten minutes late, the train stopped outside the station so grab a table and order for me please"

/** The floating field in every state, over a fake chat app with its keyboard up, as the service places it. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w412dp-h915dp-xxhdpi")
class FieldSnapshots {
    val rule = createComposeRule()
    @get:Rule val chain = composeRule().around(rule)

    private fun field(name: String, state: FlowState, dockRight: Boolean = true, fontScale: Float = 1f) {
        rule.mainClock.autoAdvance = false
        rule.setContent {
            val d = LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(d.density, fontScale)) { VoicioTheme { Phone(state, dockRight) } }
        }
        rule.snap(name)
    }

    @Test fun idle() = field("field_01_idle_orb", FlowState())
    @Test fun idleLeft() = field("field_02_idle_orb_left", FlowState(), dockRight = false)
    @Test fun warming() = field("field_03_warming", FlowState(Phase.Warming))
    @Test fun listeningEmpty() = field("field_04_listening_empty", FlowState(Phase.Listening, level = 0.05f))
    @Test fun listening1() = field("field_05_listening_1line", FlowState(Phase.Listening, live = ONE_LINE, level = 0.6f))
    @Test fun listening2() = field("field_06_listening_2lines", FlowState(Phase.Listening, live = TWO_LINES, level = 0.5f))
    @Test fun listeningCloud() = field("field_07_listening_cloud", FlowState(Phase.Listening, live = ONE_LINE, level = 0.4f, cloud = true))
    @Test fun working() = field("field_08_working", FlowState(Phase.Working, live = ONE_LINE))
    @Test fun done() = field("field_09_done_check", FlowState(Phase.Done))
    @Test fun undo() = field("field_10_idle_undo", FlowState(canUndo = true))
    @Test fun undoLeft() = field("field_11_idle_undo_left", FlowState(canUndo = true), dockRight = false)
    @Test fun warn() = field("field_12_warn", FlowState(notice = Notice(NoticeKind.Warn, "Hold the bubble a little longer while you speak")))
    @Test fun error() = field("field_13_error_retry", FlowState(notice = Notice(NoticeKind.Error, "Couldn't hear anything. Check the microphone.", retry = true)))
    @Test fun info() = field("field_14_info", FlowState(notice = Notice(NoticeKind.Info, "Copied. Paste it where you want it.")))

    @Config(qualifiers = "w360dp-h640dp-xhdpi")
    @Test fun smallListening() = field("field_20_small_listening_2lines", FlowState(Phase.Listening, live = TWO_LINES, level = 0.5f))
    @Config(qualifiers = "w360dp-h640dp-xhdpi")
    @Test fun smallIdle() = field("field_21_small_idle_undo", FlowState(canUndo = true))
    @Test fun font130() = field("field_22_font130_listening_2lines", FlowState(Phase.Listening, live = TWO_LINES, level = 0.5f), fontScale = 1.3f)
    @Test fun font130Error() = field("field_23_font130_error", FlowState(notice = Notice(NoticeKind.Error, "Couldn't hear anything. Check the microphone.", retry = true)), fontScale = 1.3f)
    @Config(qualifiers = "w915dp-h412dp-land-xxhdpi")
    @Test fun landscape() = field("field_24_landscape_listening", FlowState(Phase.Listening, live = TWO_LINES, level = 0.5f))
    @Config(qualifiers = "w915dp-h412dp-land-xxhdpi")
    @Test fun landscapeIdle() = field("field_25_landscape_idle", FlowState())
}

private val AppBg = Color(0xFFEFEAE2)
private val AppBar = Color(0xFF1F2C34)
private val Mine = Color(0xFFD9FDD3)
private val KeyboardBg = Color(0xFFE8EAED)
private val Key = Color.White
private val KeyInk = Color(0xFF202124)

/** A chat app with the keyboard up, the input glued to the keyboard, and the field placed like the service does. */
@Composable
private fun Phone(state: FlowState, dockRight: Boolean) {
    BoxWithConstraints(Modifier.fillMaxSize().background(AppBg)) {
        val land = maxWidth > maxHeight
        val keyH = if (land) 34.dp else 46.dp
        val inputH = 60.dp
        val kbH = keyH * 4 + 8.dp * 5 + 40.dp
        Column(Modifier.fillMaxSize()) {
            Row(Modifier.fillMaxWidth().height(if (land) 48.dp else 64.dp).background(AppBar).padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(36.dp).clip(CircleShape).background(Color(0xFF6B7C85)))
                Column(Modifier.padding(start = 12.dp)) {
                    Text("Sam", style = TextStyle(fontSize = 17.sp, color = Color.White))
                    Text("online", style = TextStyle(fontSize = 12.sp, color = Color(0xFFB0BEC5)))
                }
            }
            Column(Modifier.weight(1f).fillMaxWidth().padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp, Alignment.Bottom)) {
                Bubble("Are you still coming tonight?", false)
                Bubble("Yes! Leaving work now", true)
                Bubble("Great, we're at the corner table by the window", false)
            }
            Row(Modifier.fillMaxWidth().height(inputH).padding(horizontal = 8.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(
                    Modifier.weight(1f).height(44.dp).clip(RoundedCornerShape(22.dp)).background(Color.White).padding(horizontal = 16.dp),
                    contentAlignment = Alignment.CenterStart,
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text("Message", style = TextStyle(fontSize = 16.sp, color = Color(0xFF8696A0)))
                    }
                    Box(Modifier.width(2.dp).height(22.dp).background(Color(0xFF00A884)))
                }
                Box(Modifier.padding(start = 8.dp).size(44.dp).clip(CircleShape).background(Color(0xFF00A884)))
            }
            Keyboard(Modifier.fillMaxWidth().height(kbH), keyH)
        }
        // The window: orb right on the input's top edge, bar 8 dp above it (both carry a 4 dp shadow margin).
        val barWidth = (maxWidth - 24.dp).coerceIn(200.dp, 280.dp)
        val bottom = kbH + inputH + if (state.wantsBar()) 8.dp - ShadowPad else 0.dp
        Box(
            Modifier.align(if (state.wantsBar()) Alignment.BottomCenter else if (dockRight) Alignment.BottomEnd else Alignment.BottomStart)
                .padding(bottom = bottom, start = 8.dp - ShadowPad, end = 8.dp - ShadowPad)
                .clipToBounds(), // the overlay window clips like this
        ) {
            FlowField(
                state, visible = true, dockRight = dockRight, barWidth = barWidth,
                onBarLayout = {}, onGrab = {}, onOrbClick = {}, onCancel = {}, onDone = {}, onRetry = {}, onUndo = {},
            )
        }
    }
}

@Composable
private fun Bubble(text: String, mine: Boolean) {
    Box(Modifier.fillMaxWidth(), contentAlignment = if (mine) Alignment.CenterEnd else Alignment.CenterStart) {
        Text(
            text, style = TextStyle(fontSize = 15.sp, color = Color(0xFF111B21)),
            modifier = Modifier.widthIn(max = 280.dp).clip(RoundedCornerShape(10.dp)).background(if (mine) Mine else Color.White).padding(horizontal = 10.dp, vertical = 7.dp),
        )
    }
}

@Composable
private fun Keyboard(modifier: Modifier, keyH: androidx.compose.ui.unit.Dp) {
    Column(modifier.background(KeyboardBg).padding(4.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Box(Modifier.fillMaxWidth().height(32.dp))
        listOf("qwertyuiop", "asdfghjkl", "zxcvbnm").forEach { row ->
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(5.dp, Alignment.CenterHorizontally)) {
                row.forEach { ch ->
                    Box(Modifier.weight(1f, fill = false).widthIn(max = 60.dp).width(34.dp).height(keyH).clip(RoundedCornerShape(6.dp)).background(Key), contentAlignment = Alignment.Center) {
                        Text(ch.toString(), style = TextStyle(fontSize = 20.sp, color = KeyInk))
                    }
                }
            }
        }
        Row(Modifier.fillMaxWidth().padding(horizontal = 40.dp), horizontalArrangement = Arrangement.spacedBy(5.dp)) {
            Box(Modifier.width(48.dp).height(keyH).clip(RoundedCornerShape(6.dp)).background(Color(0xFFC9CDD2)))
            Box(Modifier.weight(1f).height(keyH).clip(RoundedCornerShape(6.dp)).background(Key).border(0.dp, Color.Transparent))
            Box(Modifier.width(48.dp).height(keyH).clip(RoundedCornerShape(6.dp)).background(Color(0xFFC9CDD2)))
        }
    }
}
