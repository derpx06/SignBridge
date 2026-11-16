package com.whispr.app.snap

import android.Manifest
import androidx.activity.compose.LocalActivityResultRegistryOwner
import androidx.activity.result.ActivityResultRegistry
import androidx.activity.result.ActivityResultRegistryOwner
import androidx.activity.result.contract.ActivityResultContract
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.printToString
import androidx.compose.ui.test.performSemanticsAction
import androidx.core.app.ActivityOptionsCompat
import com.whispr.app.R
import com.whispr.app.data.HistoryEntry
import com.whispr.app.data.HistoryStore
import com.whispr.app.data.Note
import com.whispr.app.data.NotesStore
import com.whispr.app.flow.FlowSettings
import com.whispr.app.flow.ModelDownloader
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.widthIn
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.whispr.app.ui.C
import com.whispr.app.ui.Comb
import com.whispr.app.ui.Lattice
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import com.whispr.app.ui.MainShell
import com.whispr.app.ui.NoteEditor
import com.whispr.app.ui.Onboarding
import com.whispr.app.ui.VoicioTheme
import org.junit.After
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** Every app screen. Navigation is done the way a user does it: by tapping. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [34], qualifiers = "w412dp-h915dp-xxhdpi")
class AppSnapshots {
    val rule = createComposeRule()
    @get:Rule val chain = composeRule().around(rule)
    private val settings by lazy { FlowSettings(app) }

    @Before fun setUp() { resetGlobals(); rule.mainClock.autoAdvance = false }
    @After fun tearDown() = resetGlobals()

    private fun tap(text: String) {
        // The semantics action, not a synthetic touch: touches inside scrolling columns get lost with the clock paused.
        val target = rule.onAllNodes(hasText(text) and hasClickAction())
        if (target.fetchSemanticsNodes().isNotEmpty()) target[0].performSemanticsAction(SemanticsActions.OnClick)
        else {
            val loose = rule.onAllNodes(hasText(text), useUnmergedTree = true)
            if (loose.fetchSemanticsNodes().isEmpty()) error("No \"$text\" on screen:\n" + rule.onRoot().printToString(maxDepth = 6))
            loose[0].performClick()
        }
        // Screens settle (enter animations, step transitions) within 2.5 s of virtual time.
        settle()
    }

    private fun setupDone() {
        shadowOf(app).grantPermissions(Manifest.permission.RECORD_AUDIO)
        setAccessibility(true)
        installFakeModel()
        shadowOf(app.getSystemService(android.os.PowerManager::class.java)).setIgnoringBatteryOptimizations(app.packageName, true)
    }

    private fun history() {
        val now = System.currentTimeMillis()
        HistoryStore.items.addAll(listOf(
            HistoryEntry(now - 2 * 60_000, "Running about ten minutes late, the train stopped outside the station. Grab a table and order for me.", "WhatsApp"),
            HistoryEntry(now - 50 * 60_000, "Can we move the design review to Thursday at 3 pm?", "Gmail"),
            HistoryEntry(now - 3 * 3_600_000, "Buy milk, eggs and coffee beans", "Keep"),
            HistoryEntry(now - 26 * 3_600_000, "Thanks for the notes. I'll send the updated draft tonight.", "Slack"),
        ))
    }

    private fun notes() {
        val now = System.currentTimeMillis()
        NotesStore.items.addAll(listOf(
            Note(1, "Groceries", "• Milk\n• Eggs\n• Coffee beans\n• Bread", now - 60_000),
            Note(2, "Design review", "Move the review to Thursday. Bring the new onboarding flow and the field states. Ask about the small-screen layout.", now - 3_600_000),
            Note(3, "Book ideas", "A detective who solves cases by listening only.", now - 86_400_000),
            Note(4, "", "Call the landlord about the heating before Friday.", now - 3 * 86_400_000),
            Note(5, "Trip", "1. Book flights\n2. Hotel near the old town\n3. Train pass", now - 5 * 86_400_000),
        ))
    }

    /** As MainActivity hosts it: a readable column of at most 640 dp in the middle. */
    private fun shell() = rule.setContent {
        VoicioTheme {
            Box(Modifier.fillMaxSize().background(C.Bg), contentAlignment = Alignment.TopCenter) {
                Box(Modifier.widthIn(max = 640.dp).fillMaxHeight()) { MainShell(settings) {} }
            }
        }
    }

    // ------------------------------------------------------------ onboarding

    @Composable
    private fun Denying(content: @Composable () -> Unit) {
        val owner = object : ActivityResultRegistryOwner {
            override val activityResultRegistry = object : ActivityResultRegistry() {
                override fun <I, O> onLaunch(requestCode: Int, contract: ActivityResultContract<I, O>, input: I, options: ActivityOptionsCompat?) {
                    dispatchResult(requestCode, false)
                }
            }
        }
        CompositionLocalProvider(LocalActivityResultRegistryOwner provides owner, content = content)
    }

    private fun onboarding() = rule.setContent { VoicioTheme { Denying { Onboarding(settings) {} } } }
    private fun toMic() { onboarding(); tap(str(R.string.welcome_cta)); tap("Continue with both") }
    private fun toTurnOn() { shadowOf(app).grantPermissions(Manifest.permission.RECORD_AUDIO); toMic(); tap("Continue") }
    private fun toModel() { toTurnOn(); tap("Continue for now") }

    @Test fun onb01Welcome() { onboarding(); rule.snap("onb_01_welcome", 2400) }
    @Test fun onb02Dictate() { onboarding(); tap(str(R.string.welcome_cta)); rule.snap("onb_02_dictate_both") }
    @Test fun onb03DictateKeyboardOnly() { onboarding(); tap(str(R.string.welcome_cta)); tap("Floating bubble"); rule.snap("onb_03_dictate_keyboard_only") }
    @Test fun onb04MicAsk() { toMic(); rule.snap("onb_04_mic_ask") }
    @Test fun onb05MicDenied() { toMic(); tap("Allow microphone"); rule.snap("onb_05_mic_denied") }
    @Test fun onb06MicGranted() {
        shadowOf(app).grantPermissions(Manifest.permission.RECORD_AUDIO)
        toMic(); rule.snap("onb_06_mic_listening", 800)
        rule.snap("onb_07_mic_silent", 7000)
    }
    @Test fun onb08TurnOn() { toTurnOn(); rule.snap("onb_08_turn_on_both") }
    @Test fun onb09TurnOnDone() { setAccessibility(true); toTurnOn(); rule.snap("onb_09_turn_on_bubble_done") }
    @Test fun onb10Model() {
        toModel(); rule.snap("onb_10_model")
        setDownload(ModelDownloader.Phase.Downloading, 0.42f); rule.snap("onb_11_model_downloading")
        setDownload(ModelDownloader.Phase.Failed, 0.3f, "The connection dropped. Check your internet and try again."); rule.snap("onb_12_model_failed")
    }
    @Test fun onb13Try() { toModel(); setDownload(ModelDownloader.Phase.Downloading, 0.5f); settle(); tap("Continue"); rule.snap("onb_13_try") }
    @Test fun onb14TryReady() { setupDone(); toModel(); tap("Continue"); rule.snap("onb_14_try_ready") }

    @Config(qualifiers = "w360dp-h640dp-xhdpi")
    @Test fun onb15WelcomeSmall() { onboarding(); rule.snap("onb_15_welcome_small") }

    @Config(qualifiers = "w915dp-h412dp-land-xxhdpi")
    @Test fun onb16WelcomeLandscape() { onboarding(); rule.snap("onb_16_welcome_landscape") }

    // ------------------------------------------------------------ main app

    @Test fun home01Fresh() { shell(); rule.snap("home_01_fresh") }

    @Config(qualifiers = "w412dp-h1500dp-xxhdpi")
    @Test fun home02FreshFull() { shell(); rule.snap("home_02_fresh_full") }

    @Test fun home03SetUp() { setupDone(); history(); shell(); rule.snap("home_03_ready") }

    @Test fun home04Downloading() { setDownload(ModelDownloader.Phase.Downloading, 0.42f); shell(); rule.snap("home_04_downloading") }

    @Test fun history01Empty() { shell(); tap("History"); rule.snap("history_01_empty") }

    @Test fun history02Filled() { history(); shell(); tap("History"); rule.snap("history_02_filled") }

    @Test fun notes01Empty() { shell(); tap("Notes"); rule.snap("notes_01_empty") }

    @Test fun notes02Grid() { notes(); shell(); tap("Notes"); rule.snap("notes_02_grid") }

    @Test fun notes03EditorNew() { shell(); tap("Notes"); tap("New voice note"); rule.snap("notes_03_editor_new") }

    @Test fun notes04EditorFilled() {
        notes()
        rule.setContent { VoicioTheme { NoteEditor(NotesStore.items[1], onDismiss = {}, onDeleted = {}) } }
        rule.snap("notes_04_editor_filled")
    }

    @Test fun settings01() { shell(); tap("Settings"); rule.snap("settings_01") }

    @Config(qualifiers = "w412dp-h1700dp-xxhdpi")
    @Test fun settings02Full() { setupDone(); shell(); tap("Settings"); rule.snap("settings_02_full") }

    @Test fun words01Empty() { shell(); tap("Settings"); tap("Words and snippets"); rule.snap("words_01_empty") }

    @Test fun words02Filled() {
        settings.dictionaryText = "priya => Priya\nkubernetes => Kubernetes\nai for bharat => AI4Bharat\nsherpa onyx => sherpa-onnx\npost gress => Postgres\nvoice io => Voicio\ngnome => GNOME\nbengaluru => Bengaluru"
        shell(); tap("Settings"); tap("Words and snippets"); rule.snap("words_02_filled")
    }

    @Test fun words03Snippets() {
        // In memory only: Robolectric has no AndroidKeyStore for the encrypted file.
        com.whispr.app.data.SnippetStore.items.addAll(listOf(
            com.whispr.app.data.Snippet("my address", "14 Lake Road, Indiranagar, Bengaluru 560038"),
            com.whispr.app.data.Snippet("sign off", "Thanks,\nSam"),
        ))
        shell(); tap("Settings"); tap("Words and snippets"); tap("Snippets"); rule.snap("words_03_snippets")
    }

    @Test fun models01() { shell(); tap("Settings"); tap("Models"); rule.snap("models_01") }

    @Test fun models02Ready() { installFakeModel(); shell(); tap("Settings"); tap("Models"); rule.snap("models_02_ready") }

    @Test fun models03Parakeet() { shell(); tap("Settings"); tap("Models"); tap("Parakeet 110M"); rule.snap("models_03_parakeet_detail") }

    @Test fun models04Downloading() {
        setDownload(ModelDownloader.Phase.Downloading, 0.42f); shell(); tap("Settings"); tap("Models"); tap("Parakeet 110M"); rule.snap("models_04_parakeet_downloading")
    }

    @Test fun models05Smart() { shell(); tap("Settings"); tap("Models"); tap("S1-mini clean-up"); rule.snap("models_05_smart_edit") }

    @Config(qualifiers = "w412dp-h1300dp-xxhdpi")
    @Test fun cloud01() { shell(); tap("Settings"); tap("Cloud providers"); rule.snap("cloud_01_off") }

    @Config(qualifiers = "w412dp-h1300dp-xxhdpi")
    @Test fun cloud02Open() { shell(); tap("Settings"); tap("Cloud providers"); settle(); tap("Deepgram"); rule.snap("cloud_02_add_key_open") }

    @Config(qualifiers = "w412dp-h1300dp-xxhdpi")
    @Test fun cloud03Azure() { shell(); tap("Settings"); tap("Cloud providers"); settle(); tap("Microsoft Azure"); rule.snap("cloud_03_azure_region") }

    @Config(qualifiers = "w412dp-h1300dp-xxhdpi")
    @Test fun cloud04Live() { shell(); tap("Settings"); tap("Cloud providers"); settle(); tap("OpenAI"); rule.snap("cloud_04_live_toggle") }

    @Config(qualifiers = "w412dp-h1300dp-xxhdpi")
    @Test fun about01() { shell(); tap("Settings"); tap("Privacy and about"); rule.snap("about_01") }

    @Config(qualifiers = "w360dp-h640dp-xhdpi")
    @Test fun small01Home() { shell(); rule.snap("small_01_home") }

    @Config(qualifiers = "w915dp-h412dp-land-xxhdpi")
    @Test fun land01Home() { history(); shell(); rule.snap("land_01_home") }

    /** Small clock steps, so a transition started by the last tap actually runs (one big jump can leave it unstarted). */
    private fun settle() { repeat(25) { rule.mainClock.advanceTimeBy(100); rule.waitForIdle() } }

    // ------------------------------------------------------------ brand

    /** The adaptive launcher icon under three launcher masks, its monochrome layer, and every honeycomb mood. */
    @Test fun brand01() {
        rule.setContent {
            VoicioTheme {
                Column(Modifier.fillMaxSize().background(C.Bg).padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                        listOf(CircleShape, RoundedCornerShape(28.dp), RoundedCornerShape(10.dp)).forEach { shape ->
                            // 108 dp layers, of which the launcher shows the middle 72 dp.
                            Box(Modifier.size(108.dp).clip(shape), contentAlignment = Alignment.Center) {
                                Image(painterResource(R.drawable.ic_launcher_background), null, Modifier.requiredSize(162.dp))
                                Image(painterResource(R.drawable.ic_launcher_foreground), null, Modifier.requiredSize(162.dp))
                            }
                        }
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(16.dp), verticalAlignment = Alignment.CenterVertically) {
                        Box(Modifier.size(72.dp).clip(CircleShape).background(Color(0xFF3A4A5A)), contentAlignment = Alignment.Center) {
                            Image(painterResource(R.drawable.ic_launcher_monochrome), null, Modifier.requiredSize(108.dp), colorFilter = androidx.compose.ui.graphics.ColorFilter.tint(Color(0xFFDCE6F0)))
                        }
                        Lattice(size = 48.dp, square = true)
                        Lattice(size = 32.dp, square = true)
                        Lattice(size = 20.dp, square = true)
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Lattice(size = 64.dp, glow = 0f)
                        Lattice(size = 64.dp, active = true, level = 0.8f)
                        Lattice(size = 64.dp, active = true, level = 0.02f, noise = 0.7f)
                        Lattice(size = 64.dp, active = true, sweep = true)
                        Lattice(size = 64.dp, check = 1f)
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        Lattice(size = 64.dp, tint = C.CombError, tintAmount = 1f)
                        Lattice(size = 64.dp, tint = C.Working, tintAmount = 1f)
                        Lattice(size = 64.dp, progress = 0.45f)
                        Lattice(size = 64.dp, tile = null, palette = Comb.Light, progress = 0.45f)
                        Lattice(size = 64.dp, tile = null, palette = Comb.Lime, active = true, level = 0.6f)
                    }
                }
            }
        }
        rule.snap("brand_01_icon_and_moods")
    }
}
