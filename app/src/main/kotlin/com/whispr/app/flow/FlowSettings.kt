package com.whispr.app.flow

import android.content.Context
import android.content.SharedPreferences
import com.whispr.core.dictation.Dictionary
import com.whispr.core.dictation.Trigger

/** User settings for dictation, kept in plain SharedPreferences (no secrets live here). */
class FlowSettings(context: Context) {
    private val prefs: SharedPreferences = context.getSharedPreferences("voicio-settings", Context.MODE_PRIVATE)

    /** "hold-or-tap" (default: tap = hands-free, hold = push-to-talk), "hold" or "toggle", same names as the desktop `shortcut_mode`. */
    var shortcutMode: Trigger.Mode
        get() = Trigger.Mode.parse(prefs.getString("shortcut_mode", "hold-or-tap").orEmpty())
        set(value) = prefs.edit().putString("shortcut_mode", when (value) {
            Trigger.Mode.Hold -> "hold"
            Trigger.Mode.Toggle -> "toggle"
            Trigger.Mode.HoldOrTap -> "hold-or-tap"
        }).apply()

    /** Show words in the bubble while speaking (otherwise only after release). */
    var liveText: Boolean
        get() = prefs.getBoolean("live_text", true)
        set(value) = prefs.edit().putBoolean("live_text", value).apply()

    /** Show the bubble whenever the screen is on, not only while a text field is focused. */
    var alwaysShowBubble: Boolean
        get() = prefs.getBoolean("always_show_bubble", false)
        set(value) = prefs.edit().putBoolean("always_show_bubble", value).apply()

    var bubbleX: Int
        get() = prefs.getInt("bubble_x", -1) // -1 = never moved (right edge); Int.MAX_VALUE = right edge; anything else = left edge
        set(value) = prefs.edit().putInt("bubble_x", value).apply()

    /** Vertical position as a fraction of the usable height (0 top, 1 bottom); -1 = never moved (rests low). Survives rotation. */
    var bubbleFrac: Float
        get() = prefs.getFloat("bubble_frac", -1f)
        set(value) = prefs.edit().putFloat("bubble_frac", value).apply()

    /** True once the welcome and setup have been completed (or skipped to the end). */
    var onboarded: Boolean
        get() = prefs.getBoolean("onboarded", false)
        set(value) = prefs.edit().putBoolean("onboarded", value).apply()

    /** Wipes every setting (used by "Reset app"). */
    fun clearAll() = prefs.edit().clear().apply()

    /** Remove fillers and repeats, then apply voice commands and the dictionary. */
    var cleanup: Boolean
        get() = prefs.getBoolean("cleanup", true)
        set(value) = prefs.edit().putBoolean("cleanup", value).apply()

    /** Smart formatting: numbers, times, addresses, punctuation and capitals. */
    var smartFormat: Boolean
        get() = prefs.getBoolean("smart_format", true)
        set(value) = prefs.edit().putBoolean("smart_format", value).apply()

    /** Run dictations through the on-device AI model too (needs Smart Edit installed; adds about 3 seconds). */
    var aiPolish: Boolean
        get() = prefs.getBoolean("ai_polish", false)
        set(value) = prefs.edit().putBoolean("ai_polish", value).apply()

    /** Cloud provider id, or "" = off (default). Audio only leaves the phone when this is set AND the user consented to that provider. */
    var cloudProvider: String
        get() = prefs.getString("cloud_provider", "").orEmpty()
        set(value) = prefs.edit().putString("cloud_provider", value).apply()

    /** The default provider's model id ("" = its default). Stored per provider; see [cloudModelFor]. */
    var cloudModel: String
        get() = cloudModelFor(cloudProvider)
        set(value) = setCloudModelFor(cloudProvider, value)

    /** The model chosen for provider [id] ("" = its default). The single value saved by older versions belongs to the provider that was chosen then. */
    fun cloudModelFor(id: String): String =
        prefs.getString("cloud_model_$id", null) ?: if (id.isNotEmpty() && id == cloudProvider) prefs.getString("cloud_model", "").orEmpty() else ""

    fun setCloudModelFor(id: String, model: String) { if (id.isNotEmpty()) prefs.edit().putString("cloud_model_$id", model).apply() }

    /** "off" (default), "always" (every dictation; on-device text is the fallback) or "fallback" (only when no on-device model is installed).
     *  Installs from before this setting had a chosen, consented provider mean "always". */
    var cloudMode: String
        get() = prefs.getString("cloud_mode", null) ?: if (cloudProvider.isNotEmpty() && hasCloudConsent(cloudProvider)) "always" else "off"
        set(value) = prefs.edit().putString("cloud_mode", value).apply()

    /** Azure only: the Speech resource's region ("westeurope") or its full host. */
    var cloudRegion: String
        get() = prefs.getString("cloud_region", "").orEmpty()
        set(value) = prefs.edit().putString("cloud_region", value.trim()).apply()

    /** Providers whose "Live" toggle is on (words stream in while speaking); off by default. */
    val cloudLive: Set<String> get() = prefs.getStringSet("cloud_live", emptySet())!!
    fun isCloudLive(id: String) = id in cloudLive
    fun setCloudLive(id: String, on: Boolean) = prefs.edit().putStringSet("cloud_live", if (on) cloudLive + id else cloudLive - id).apply()

    fun hasCloudConsent(id: String) = prefs.getStringSet("cloud_consent", emptySet())!!.contains(id)
    fun giveCloudConsent(id: String) = prefs.edit().putStringSet("cloud_consent", prefs.getStringSet("cloud_consent", emptySet())!! + id).apply()

    /** Days to keep dictation history; 0 keeps nothing. */
    var historyDays: Int
        get() = prefs.getInt("history_days", 30)
        set(value) = prefs.edit().putInt("history_days", value.coerceAtLeast(0)).apply()

    /** Dictionary lines, one per entry, `spoken => written`. */
    var dictionaryText: String
        get() = prefs.getString("dictionary", "").orEmpty()
        set(value) = prefs.edit().putString("dictionary", value).apply()

    fun dictionary(): Dictionary = Dictionary(
        dictionaryText.lines().mapNotNull { line ->
            val parts = line.split("=>", limit = 2)
            if (parts.size == 2 && parts[0].isNotBlank()) Dictionary.Entry(parts[0].trim(), parts[1].trim()) else null
        },
    )

    // ------------------------------------------------------------ input surfaces and safety

    /** "bubble" (default), "keyboard" or "both": which way the user dictates. The bubble hides when it is "keyboard". */
    var inputMode: String
        get() = prefs.getString("input_mode", "bubble").orEmpty()
        set(value) = prefs.edit().putString("input_mode", value).apply()

    /** Spoken edits: "actually" / "no wait" backtracking, "comma", "first... second..." lists, and commands. */
    var spokenCommands: Boolean
        get() = prefs.getBoolean("spoken_commands", true)
        set(value) = prefs.edit().putBoolean("spoken_commands", value).apply()

    /** Stop listening after this much silence in hands-free mode: 0 (off, default), 1500 or 3000 ms. */
    var autoStopMs: Long
        get() = prefs.getLong("auto_stop_ms", 0L)
        set(value) = prefs.edit().putLong("auto_stop_ms", value).apply()

    /** Whisper mode: more microphone gain for speaking quietly. */
    var whisperMode: Boolean
        get() = prefs.getBoolean("whisper_mode", false)
        set(value) = prefs.edit().putBoolean("whisper_mode", value).apply()

    /** Per-app writing style overrides, "package=Style" ("Formal", "Casual", "List", "Email", or "None" for as said). */
    fun styleOverride(pkg: String): String? = prefs.getStringSet("app_styles", emptySet())!!.firstOrNull { it.startsWith("$pkg=") }?.substringAfter('=')
    fun setStyleOverride(pkg: String, style: String?) {
        val rest = prefs.getStringSet("app_styles", emptySet())!!.filterNot { it.startsWith("$pkg=") }.toSet()
        prefs.edit().putStringSet("app_styles", if (style == null) rest else rest + "$pkg=$style").apply()
    }

    // ------------------------------------------------------------ Voicio Keyboard

    /** "ink" (default), "paper" or "system". */
    var keyboardTheme: String
        get() = prefs.getString("keyboard_theme", "ink").orEmpty()
        set(value) = prefs.edit().putString("keyboard_theme", value).apply()

    /** Key height multiplier, 0.85..1.15. */
    var keyboardHeight: Float
        get() = prefs.getFloat("keyboard_height", 1f)
        set(value) = prefs.edit().putFloat("keyboard_height", value.coerceIn(0.85f, 1.15f)).apply()

    /** Key haptics: 0 off, 1 light, 2 medium (default), 3 strong. */
    var keyHaptics: Int
        get() = prefs.getInt("key_haptics", 2)
        set(value) = prefs.edit().putInt("key_haptics", value.coerceIn(0, 3)).apply()

    /** Key click volume 0..1 (0 = off, default). */
    var keySound: Float
        get() = prefs.getFloat("key_sound", 0f)
        set(value) = prefs.edit().putFloat("key_sound", value.coerceIn(0f, 1f)).apply()

    /** A short tone when listening starts and stops. */
    var voiceSounds: Boolean
        get() = prefs.getBoolean("voice_sounds", false)
        set(value) = prefs.edit().putBoolean("voice_sounds", value).apply()

    /** Pause music and videos while listening (audio focus). */
    var pauseMedia: Boolean
        get() = prefs.getBoolean("pause_media", false)
        set(value) = prefs.edit().putBoolean("pause_media", value).apply()

    var numberRow: Boolean
        get() = prefs.getBoolean("number_row", true)
        set(value) = prefs.edit().putBoolean("number_row", value).apply()

    /** One-handed keyboard: 0 off, -1 left, 1 right. */
    var oneHanded: Int
        get() = prefs.getInt("one_handed", 0)
        set(value) = prefs.edit().putInt("one_handed", value.coerceIn(-1, 1)).apply()

    /** Floating keyboard: a small movable card instead of the docked keyboard. */
    var floating: Boolean
        get() = prefs.getBoolean("kb_floating", false)
        set(value) = prefs.edit().putBoolean("kb_floating", value).apply()

    /** Where the floating card was left (window pixels); -1 means the default spot. */
    var floatX: Int
        get() = prefs.getInt("kb_float_x", -1)
        set(value) = prefs.edit().putInt("kb_float_x", value).apply()
    var floatY: Int
        get() = prefs.getInt("kb_float_y", -1)
        set(value) = prefs.edit().putInt("kb_float_y", value).apply()

    /** Pinned clipboard items (kept until unpinned; everything else expires after an hour and is never written to disk). */
    var pinnedClips: Set<String>
        get() = prefs.getStringSet("pinned_clips", emptySet())!!.toSet()
        set(value) = prefs.edit().putStringSet("pinned_clips", value).apply()

    /** Recently used emoji, newest first. */
    var recentEmoji: List<String>
        get() = prefs.getString("recent_emoji", "").orEmpty().split(' ').filter { it.isNotEmpty() }
        set(value) = prefs.edit().putString("recent_emoji", value.take(24).joinToString(" ")).apply()
}
