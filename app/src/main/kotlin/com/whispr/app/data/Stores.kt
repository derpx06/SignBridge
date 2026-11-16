package com.whispr.app.data

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.security.crypto.EncryptedFile
import androidx.security.crypto.MasterKey
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/**
 * Set when an encrypted file exists but cannot be read (usually the Keystore key changed, e.g. after a
 * restore or a lock-screen reset). The Home screen shows a one-time "history was reset" notice.
 */
object DataNotice {
    var reset by mutableStateOf(false)
    private const val PREFS = "voicio-settings"
    fun flag(context: Context) { reset = true; context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean("data_reset", true).apply() }
    fun load(context: Context) { if (context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean("data_reset", false)) reset = true }
    fun dismiss(context: Context) { reset = false; context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean("data_reset", false).apply() }
}

/** A small encrypted text file. Used for history and notes so nothing readable sits on disk. */
internal object SecureFile {
    private fun builder(context: Context, file: File) = EncryptedFile.Builder(
        context, file,
        MasterKey.Builder(context).setKeyScheme(MasterKey.KeyScheme.AES256_GCM).build(),
        EncryptedFile.FileEncryptionScheme.AES256_GCM_HKDF_4KB,
    ).build()

    fun read(context: Context, file: File): String? {
        if (!file.exists()) return null
        return try {
            builder(context, file).openFileInput().use { it.readBytes().decodeToString() }
        } catch (e: Exception) {
            DataNotice.flag(context) // damaged or from another install: start fresh rather than crash, but say so
            null
        }
    }

    fun write(context: Context, file: File, text: String) {
        val tmp = File(file.parentFile, file.name + ".tmp")
        tmp.delete()
        builder(context, tmp).openFileOutput().use { it.write(text.encodeToByteArray()) }
        file.delete()
        tmp.renameTo(file)
    }
}

data class HistoryEntry(val at: Long, val text: String, val app: String) {
    val words: Int get() = text.trim().split(Regex("\\s+")).count { it.isNotEmpty() }
}

/** Dictation history: text only, never audio, encrypted, trimmed to the retention window. */
object HistoryStore {
    val items = mutableStateListOf<HistoryEntry>() // newest first
    private var file: File? = null
    private var appContext: Context? = null
    private var retentionDays = 30

    @Synchronized
    fun ensure(context: Context, days: Int) {
        if (file != null) { retentionDays = days; return }
        appContext = context.applicationContext
        file = File(context.filesDir, "history.bin")
        retentionDays = days
        val raw = SecureFile.read(context, file!!) ?: return
        val loaded = try {
            val arr = JSONArray(raw)
            (0 until arr.length()).map { arr.getJSONObject(it).let { o -> HistoryEntry(o.getLong("at"), o.getString("text"), o.optString("app")) } }
        } catch (e: Exception) { emptyList() }
        items.clear()
        items.addAll(loaded)
        prune()
    }

    @Synchronized
    fun add(context: Context, text: String, app: String, days: Int) {
        ensure(context, days)
        if (days == 0) return // "keep nothing"
        items.add(0, HistoryEntry(System.currentTimeMillis(), text, app))
        prune()
        save()
    }

    @Synchronized fun remove(entry: HistoryEntry) { items.remove(entry); save() }
    @Synchronized fun clear() { items.clear(); save() }

    private fun prune() {
        val cutoff = System.currentTimeMillis() - retentionDays * 86_400_000L
        if (retentionDays > 0) items.removeAll { it.at < cutoff }
        while (items.size > 1000) items.removeAt(items.lastIndex)
    }

    private fun save() {
        val f = file ?: return
        val c = appContext ?: return
        val arr = JSONArray()
        items.forEach { arr.put(JSONObject().put("at", it.at).put("text", it.text).put("app", it.app)) }
        SecureFile.write(c, f, arr.toString())
    }
}

data class Note(val id: Long, val title: String, val body: String, val at: Long)

/** Short notes the user dictates or types in the app. Encrypted like history. */
object NotesStore {
    val items = mutableStateListOf<Note>() // newest first
    private var file: File? = null
    private var appContext: Context? = null

    @Synchronized
    fun ensure(context: Context) {
        if (file != null) return
        appContext = context.applicationContext
        file = File(context.filesDir, "notes.bin")
        val raw = SecureFile.read(context, file!!) ?: return
        try {
            val arr = JSONArray(raw)
            items.addAll((0 until arr.length()).map { arr.getJSONObject(it).let { o -> Note(o.getLong("id"), o.getString("title"), o.getString("body"), o.getLong("at")) } })
        } catch (e: Exception) { /* start empty */ }
    }

    @Synchronized
    fun upsert(note: Note) {
        val i = items.indexOfFirst { it.id == note.id }
        if (i >= 0) items.removeAt(i)
        items.add(0, note)
        save()
    }

    @Synchronized fun remove(note: Note) { items.removeAll { it.id == note.id }; save() }
    @Synchronized fun clear() { items.clear(); save() }

    private fun save() {
        val f = file ?: return
        val c = appContext ?: return
        val arr = JSONArray()
        items.forEach { arr.put(JSONObject().put("id", it.id).put("title", it.title).put("body", it.body).put("at", it.at)) }
        SecureFile.write(c, f, arr.toString())
    }
}

/** Cloud-provider API keys, one per provider, in an AES-GCM encrypted file (Keystore-backed MasterKey). Never logged. */
object CloudKeyStore {
    private fun file(c: Context) = File(c.filesDir, "cloud_keys.bin")

    @Synchronized
    private fun load(c: Context): JSONObject = try { JSONObject(SecureFile.read(c, file(c)) ?: "{}") } catch (e: Exception) { JSONObject() }

    @Synchronized fun get(c: Context, provider: String): String? = load(c).optString(provider).takeIf { it.isNotBlank() }

    @Synchronized
    fun set(c: Context, provider: String, key: String?) {
        val o = load(c)
        if (key.isNullOrBlank()) o.remove(provider) else o.put(provider, key.trim())
        SecureFile.write(c, file(c), o.toString())
    }
}
