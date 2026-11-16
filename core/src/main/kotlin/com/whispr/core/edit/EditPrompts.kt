package com.whispr.core.edit

/**
 * The exact prompts the small model is run with. Wording matters a lot for a 0.6B model, so these are
 * the tested forms and live in one place. The Clean prompt is the one S1-mini was trained on (changing
 * it makes the model garble its output, per its model card).
 */
object EditPrompts {
    private const val CLEAN_SYSTEM = "You are a text normalizer for speech-to-text transcripts. The input begins with a control line specifying the styling, structure, and context settings; clean the transcript to match those settings and output only the cleaned text."
    private const val CLEAN_CONTROL = "[Styling: semi-formal] [Structure: prose] [Context: general]"
    private const val PROOFREAD = "Fix spelling, grammar and punctuation. Do not change the meaning or add anything. Reply with only the corrected text."
    private const val SHORTEN = "Make the message shorter and clearer. Keep every fact, number and name. Reply with only the new message."

    /** Qwen-style chat template with thinking switched off (an empty think block), as on the desktop. */
    private fun chat(system: String, user: String) =
        "<|im_start|>system\n$system<|im_end|>\n<|im_start|>user\n$user<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n"

    fun build(request: EditRequest): String = when (request.mode) {
        EditMode.Clean -> chat(CLEAN_SYSTEM, "$CLEAN_CONTROL\n${request.text}")
        EditMode.Proofread -> chat(PROOFREAD, request.text)
        EditMode.Rewrite -> chat(SHORTEN, request.text)
    }

    /** Upper bound for the answer: about twice the input, so a runaway answer is cut off early. */
    fun maxNewTokens(text: String): Int = (text.length / 3 * 2 + 24).coerceIn(32, 512)
}
