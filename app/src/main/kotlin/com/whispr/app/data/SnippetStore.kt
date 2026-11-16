package com.whispr.app.data

import android.content.Context
import androidx.compose.runtime.mutableStateListOf
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/** A voice snippet: say [trigger] ("my address") and [text] is typed. */
data class Snippet(val trigger: String, val text: String)

/**
 * Voice snippets, encrypted like history and notes (text only). Used by the Voicio Keyboard; the
 * Words & snippets screen edits them with [put] and [remove].
 */
object SnippetStore {
    val items = mutableStateListOf<Snippet>()
    private var file: File? = null
    private var appContext: Context? = null

    @Synchronized
    fun ensure(context: Context) {
        if (file != null) return
        appContext = context.applicationContext
        file = File(context.filesDir, "snippets.bin")
        val raw = SecureFile.read(context, file!!) ?: return
        val loaded = try {
            val arr = JSONArray(raw)
            (0 until arr.length()).map { arr.getJSONObject(it).let { o -> Snippet(o.getString("trigger"), o.getString("text")) } }
        } catch (e: Exception) { emptyList() }
        items.clear()
        items.addAll(loaded)
    }

    /** trigger -> text, safe to call from any thread. */
    @Synchronized
    fun map(context: Context): Map<String, String> { ensure(context); return items.toList().associate { it.trigger to it.text } }

    @Synchronized
    fun put(context: Context, snippet: Snippet) {
        ensure(context)
        items.removeAll { it.trigger.equals(snippet.trigger, ignoreCase = true) }
        items.add(snippet)
        save()
    }

    @Synchronized
    fun remove(context: Context, snippet: Snippet) { ensure(context); items.remove(snippet); save() }

    @Synchronized
    fun clear() { items.clear(); save() }

    private fun save() {
        val f = file ?: return
        val c = appContext ?: return
        val arr = JSONArray()
        items.forEach { arr.put(JSONObject().put("trigger", it.trigger).put("text", it.text)) }
        SecureFile.write(c, f, arr.toString())
    }
}
