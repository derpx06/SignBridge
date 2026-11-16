package com.whispr.core.dictation

/**
 * The whole text pipeline for one finished dictation.
 *
 * Order: voice commands -> rule cleanup -> smart formatting -> dictionary.
 *
 * This mirrors the desktop agent, where the worker cleans the raw ASR text first (`rule_clean`, or the
 * guarded model) and `polish()` then applies the user's dictionary to the result, so dictionary
 * entries see clean text and their outputs are never altered afterwards. The extension's TS pipeline
 * runs the dictionary first, but there it has no separate cleanup stage. Voice commands run first
 * because they need the raw words ("scratch that" drops spoken words, not cleaned ones). Cleanup
 * runs per line, since it collapses whitespace and would otherwise swallow the `\n` the commands add.
 * The TS pipeline's tidy step (sentence capitals, final period) is not ported: the desktop ASR
 * already punctuates.
 */
object Pipeline {
    data class Options(
        val commands: Set<VoiceCommands.Command> = VoiceCommands.Command.entries.toSet(),
        val cleanup: Boolean = true,
        /** Smart formatting: numbers, times, addresses, punctuation, capitals, sentence framing. */
        val format: Boolean = true,
        /** [SpokenEdits]: backtracking ("actually", "no wait"), spoken punctuation, "first... second..." lists. */
        val spoken: Boolean = false,
    )

    fun process(raw: String, dictionary: Dictionary, options: Options = Options()): VoiceCommands.Result {
        val commands = VoiceCommands.apply(if (options.spoken) SpokenEdits.backtrack(raw) else raw, options.commands)
        val cleaned = if (options.cleanup) {
            commands.text.split('\n').joinToString("\n", transform = CleanupRules::ruleClean)
        } else {
            commands.text
        }
        val marked = if (options.spoken) cleaned.split('\n').joinToString("\n", transform = SpokenEdits::punctuation) else cleaned
        val formatted = if (options.format) Formatter.format(marked) else marked
        val listed = if (options.spoken) SpokenEdits.ordinalList(formatted) else formatted
        return commands.copy(text = dictionary.replace(listed))
    }
}
