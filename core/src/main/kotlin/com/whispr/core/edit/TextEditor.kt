package com.whispr.core.edit

import com.whispr.core.dictation.CleanupRules
import com.whispr.core.dictation.Dictionary
import com.whispr.core.dictation.Pipeline

/** What to do with a piece of text. */
enum class EditMode { Clean, Proofread, Rewrite }

enum class Tone { Formal, Casual, Concise, Friendly }

data class EditRequest(val text: String, val mode: EditMode = EditMode.Clean, val tone: Tone? = null)

sealed interface EditResult {
    /** The edited text and whether a model (not just rules) produced it. */
    data class Edited(val text: String, val usedModel: Boolean) : EditResult
    /** The model's answer was refused, so [text] is the rules-only result. [reason] is shown to the user. */
    data class Kept(val text: String, val reason: String) : EditResult
}

val EditResult.text: String get() = when (this) { is EditResult.Edited -> text; is EditResult.Kept -> text }

/** Anything that can edit text: the always-on rules, or a model behind [GuardedEditor]. */
fun interface TextEditor {
    fun edit(request: EditRequest): EditResult
}

/** Tier 0: the rule pipeline (fillers, repeats, smart formatting, your words). No model, instant. */
class RulesEditor(private val dictionary: Dictionary = Dictionary(emptyList()), private val options: Pipeline.Options = Pipeline.Options()) : TextEditor {
    override fun edit(request: EditRequest): EditResult =
        EditResult.Edited(Pipeline.process(request.text, dictionary, options).text, usedModel = false)
}

/**
 * Decides whether a model's answer may replace the text. The rule for Clean and Proofread is the
 * strict desktop guardrail (almost the same words, same negations, same numbers). A tone Rewrite
 * may change words, so it is held to what must never change: negations, numbers, and a sane length.
 */
object EditGuard {
    fun accept(original: String, candidate: String, mode: EditMode): Boolean {
        val c = candidate.trim()
        if (c.isEmpty()) return false
        return when (mode) {
            EditMode.Clean, EditMode.Proofread -> CleanupRules.guardrail(original, c)
            EditMode.Rewrite -> {
                val before = CleanupRules.words(original).size.coerceAtLeast(1)
                val after = CleanupRules.words(c).size
                after in (before * 2 / 5)..(before * 5 / 2 + 3) &&
                    CleanupRules.negations(original) == CleanupRules.negations(c) &&
                    CleanupRules.numbersConsistent(original, c)
            }
        }
    }
}

/** Runs [model] when there is one, keeps its answer only if [EditGuard] accepts it, else falls back to [rules]. */
class GuardedEditor(private val model: TextEditor?, private val rules: TextEditor) : TextEditor {
    override fun edit(request: EditRequest): EditResult {
        val base = rules.edit(request).text
        val m = model ?: return EditResult.Edited(base, usedModel = false)
        val answer = try { m.edit(request.copy(text = base)).text } catch (e: Exception) {
            return EditResult.Kept(base, "The AI model was not available, so only the basic clean-up was applied.")
        }
        return if (EditGuard.accept(base, answer, request.mode)) EditResult.Edited(answer.trim(), usedModel = true)
        else EditResult.Kept(base, "Kept your wording: the AI's version changed the meaning.")
    }
}
