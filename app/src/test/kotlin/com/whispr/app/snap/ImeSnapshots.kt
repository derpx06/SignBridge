package com.whispr.app.snap

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.test.junit4.createComposeRule
import com.whispr.app.flow.FlowState
import com.whispr.app.flow.Notice
import com.whispr.app.flow.NoticeKind
import com.whispr.app.flow.Phase
import com.whispr.app.ime.Clip
import com.whispr.app.ime.EnterKind
import com.whispr.app.ime.FieldKind
import com.whispr.app.ime.ImeActions
import com.whispr.app.ime.ImeUi
import com.whispr.app.ime.KeyboardView
import com.whispr.app.ime.Layer
import com.whispr.app.ime.Shift
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** The Voicio Keyboard in every state, at the bottom of a phone screen like the artboard. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w390dp-h844dp-xxhdpi")
class ImeSnapshots {
    val rule = createComposeRule()
    @get:Rule val chain = composeRule().around(rule)

    private val ready = ImeUi(style = "Casual", styleWhy = "WhatsApp · chat apps are casual", suggestions = listOf("Yes", "On my way"), numberRow = false, enter = EnterKind.Send)

    private fun kb(name: String, ui: ImeUi) {
        rule.mainClock.autoAdvance = false
        rule.setContent {
            Box(Modifier.fillMaxSize().background(Color(0xFFECE5DA)), contentAlignment = Alignment.BottomCenter) { KeyboardView(ui, object : ImeActions {}) }
        }
        rule.snap(name, settle = 1200)
    }

    @Test fun ready() = kb("ime_01_ready_ink", ready)
    @Test fun idleTools() = kb("ime_02_idle_no_suggestions", ready.copy(suggestions = emptyList(), style = "As said"))
    @Test fun shift() = kb("ime_03_shift", ready.copy(shift = Shift.Once, suggestions = emptyList()))
    @Test fun listening() = kb("ime_04_listening_live", ready.copy(flow = FlowState(Phase.Listening, live = "running ten minutes late, save me a seat near the", level = 0.6f)))
    @Test fun listeningEmpty() = kb("ime_05_listening_empty_cloud", ready.copy(flow = FlowState(Phase.Listening, level = 0.2f), privacy = "Cloud · Groq", cloud = "Groq"))
    @Test fun working() = kb("ime_06_working", ready.copy(flow = FlowState(Phase.Working, live = "running ten minutes late")))
    @Test fun typed() = kb("ime_07_typed_undo", ready.copy(flow = FlowState(Phase.Done, live = "Running 10 minutes late.", canUndo = true)))
    @Test fun error() = kb("ime_08_error_retry", ready.copy(flow = FlowState(notice = Notice(NoticeKind.Warn, "Microphone is silent", retry = true))))
    @Test fun password() = kb("ime_09_password", ImeUi(field = FieldKind.Password, numberRow = false, enter = EnterKind.Done))
    @Test fun number() = kb("ime_10_number_pad", ImeUi(field = FieldKind.Phone, enter = EnterKind.Next))
    @Test fun symbols() = kb("ime_11_symbols", ready.copy(layer = Layer.Symbols))
    @Test fun emoji() = kb("ime_12_emoji", ready.copy(layer = Layer.Emoji, recentEmoji = listOf("😀", "👍", "❤️")))
    @Test fun clipboard() = kb("ime_13_clipboard", ready.copy(layer = Layer.Clipboard, clips = listOf(Clip("12 Baker Street, London", 0, pinned = true), Clip("https://example.com/slides", 1))))
    @Test fun oneHanded() = kb("ime_14_one_handed", ready.copy(oneHanded = 1))
    @Test fun floating() = kb("ime_15_floating", ready.copy(floating = true))
    @Test fun floatingListening() = kb("ime_16_floating_listening", ready.copy(floating = true, flow = FlowState(Phase.Listening, live = "running ten minutes late", level = 0.5f)))
    @Test fun paper() = kb("ime_15_paper_formal", ready.copy(dark = false, style = "Formal", suggestions = listOf("Sure", "Thanks"), enter = EnterKind.Return))
    @Test fun paperListening() = kb("ime_16_paper_listening", ready.copy(dark = false, flow = FlowState(Phase.Listening, live = "can you send the slides before Friday", level = 0.5f)))
    @Test fun menu() = kb("ime_17_tools_menu", ready.copy(layer = Layer.Menu, whisper = true, autoStopMs = 1500, rewriteModel = false))
    @Test fun history() = kb("ime_18_history", ready.copy(layer = Layer.History, history = listOf("Running 10 minutes late, save me a seat.", "Can you send the slides before Friday?")))
    @Test fun numberRow() = kb("ime_19_number_row_tall", ready.copy(numberRow = true, height = 1.15f))
    @Test fun addWord() = kb("ime_20_add_to_dictionary", ready.copy(suggestions = emptyList(), newWords = listOf("Kubernetes", "Priya")))
}

@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w844dp-h390dp-xxhdpi")
class ImeLandscapeSnapshots {
    val rule = createComposeRule()
    @get:Rule val chain = composeRule().around(rule)

    private val land = ImeUi(style = "Casual", suggestions = listOf("Yes", "On my way"), numberRow = false, landscape = true, height = 0.62f, enter = EnterKind.Send)

    private fun kb(name: String, ui: ImeUi) {
        rule.mainClock.autoAdvance = false
        rule.setContent {
            Box(Modifier.fillMaxSize().background(Color(0xFFECE5DA)), contentAlignment = Alignment.BottomCenter) { KeyboardView(ui, object : ImeActions {}) }
        }
        rule.snap(name, settle = 1200)
    }

    @Test fun landscape() = kb("ime_17_landscape", land)
    @Test fun landscapeListening() = kb("ime_18_landscape_listening", land.copy(flow = FlowState(Phase.Listening, live = "running ten minutes late", level = 0.5f)))
    @Test fun landscapeFloating() = kb("ime_19_landscape_floating", land.copy(floating = true))
}
