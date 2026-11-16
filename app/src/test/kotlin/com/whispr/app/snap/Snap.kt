package com.whispr.app.snap

import android.app.Application
import androidx.compose.runtime.MutableFloatState
import androidx.compose.runtime.MutableState
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onRoot
import androidx.test.core.app.ApplicationProvider
import com.github.takahirom.roborazzi.captureRoboImage
import com.whispr.app.data.HistoryStore
import com.whispr.app.data.NotesStore
import com.whispr.app.flow.FlowAccessibilityService
import com.whispr.app.flow.ModelDownloader
import com.whispr.app.flow.ModelStore
import java.io.File

/** Shared bits for the JVM screenshot tests. Run: ./gradlew :app:testDebugUnitTest --tests 'com.whispr.app.snap.*' */
internal val app: Application get() = ApplicationProvider.getApplicationContext()

internal fun str(id: Int, vararg args: Any) = app.getString(id, *args)

/**
 * Robolectric does not pick up ComponentActivity from ui-test-manifest here (namespace differs from the
 * application id), so register it before the compose rule launches it.
 */
internal fun composeRule(): org.junit.rules.RuleChain = org.junit.rules.RuleChain.outerRule { base, _ ->
    object : org.junit.runners.model.Statement() {
        override fun evaluate() {
            org.robolectric.Shadows.shadowOf(app.packageManager)
                .addActivityIfNotPresent(android.content.ComponentName(app, androidx.activity.ComponentActivity::class.java))
            base.evaluate()
        }
    }
}

/** Lets enter animations and springs finish (the clock is driven by hand), then writes build/snap/[name].png. */
internal fun ComposeContentTestRule.snap(name: String, settle: Long = 2500) {
    mainClock.advanceTimeBy(settle)
    waitForIdle()
    onRoot().captureRoboImage("build/snap/$name.png")
}

/** The download state is process-wide and only the downloader writes it; tests poke it through the backing fields. */
@Suppress("UNCHECKED_CAST")
internal fun setDownload(phase: ModelDownloader.Phase, progress: Float = 0f, error: String? = null) {
    fun field(n: String) = ModelDownloader::class.java.getDeclaredField("$n\$delegate").apply { isAccessible = true }.get(ModelDownloader)
    (field("phase") as MutableState<ModelDownloader.Phase>).value = phase
    (field("progress") as MutableFloatState).floatValue = progress
    (field("error") as MutableState<String?>).value = error
}

/** Pretends the accessibility service is (or is not) switched on. */
internal fun setAccessibility(on: Boolean) {
    FlowAccessibilityService::class.java.getDeclaredField("instance").apply { isAccessible = true }
        .set(null, if (on) FlowAccessibilityService() else null)
}

internal fun installFakeModel() {
    val asr = ModelStore.asr(app).apply { mkdirs() }
    listOf("encoder.int8.onnx", "decoder.int8.onnx", "joiner.int8.onnx", "tokens.txt").forEach { File(asr, it).writeText("x") }
    ModelStore.vad(app).writeText("x")
}

internal fun resetGlobals() {
    setDownload(ModelDownloader.Phase.Idle)
    setAccessibility(false)
    HistoryStore.items.clear()
    NotesStore.items.clear()
    com.whispr.app.data.SnippetStore.items.clear()
}
