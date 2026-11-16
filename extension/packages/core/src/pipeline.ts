// Text pipeline: ASR final -> dictionary -> voice commands -> filler removal -> tidy.
// Pure and synchronous so it runs in the offscreen document, the popup preview and tests.

export interface DictionaryEntry {
  when: string; // what the recogniser hears, e.g. "kube control"
  write: string; // what to type, e.g. "kubectl"
}

export type CommandId = "newLine" | "newParagraph" | "scratchThat" | "pressEnter" | "stopListening";

export const COMMAND_PHRASES: Record<CommandId, string[]> = {
  newLine: ["new", "line"],
  newParagraph: ["new", "paragraph"],
  scratchThat: ["scratch", "that"],
  pressEnter: ["press", "enter"],
  stopListening: ["stop", "listening"],
};

export interface PipelineOptions {
  dictionary?: DictionaryEntry[];
  commands?: Partial<Record<CommandId, boolean>>; // default: all on
  removeFillers?: boolean; // default true
  finalPunctuation?: boolean; // add "." to a plain sentence, default true
}

export interface PipelineResult {
  text: string; // ready to insert, may contain "\n"
  pressEnter: boolean; // a "press enter" command ended the text
  stop: boolean; // "stop listening" was said
}

const FILLERS = new Set(["um", "umm", "uh", "uhh", "uhm", "er", "erm", "hmm"]);
const BREAK = "\u0000"; // internal marker for a line break; never in user text

const bare = (w: string) => w.toLowerCase().replace(/^[^\p{L}\p{N}']+|[^\p{L}\p{N}']+$/gu, "");

function matches(words: string[], at: number, phrase: string[]): boolean {
  return phrase.every((p, i) => bare(words[at + i] ?? "") === p);
}

function applyDictionary(words: string[], entries: DictionaryEntry[]): string[] {
  const rules = entries
    .map((e) => ({ when: e.when.toLowerCase().split(/\s+/).filter(Boolean), write: e.write }))
    .filter((r) => r.when.length > 0)
    .sort((a, b) => b.when.length - a.when.length); // longest phrase wins
  const out: string[] = [];
  for (let i = 0; i < words.length; ) {
    const rule = rules.find((r) => matches(words, i, r.when));
    if (rule) {
      const last = words[i + rule.when.length - 1] ?? "";
      const tail = last.match(/[^\p{L}\p{N}']+$/u)?.[0] ?? ""; // keep trailing "," "." from the recogniser
      out.push(rule.write + tail);
      i += rule.when.length;
    } else {
      out.push(words[i] as string);
      i++;
    }
  }
  return out;
}

export function runPipeline(raw: string, options: PipelineOptions = {}): PipelineResult {
  let words = raw.split(/\s+/).filter(Boolean);
  if (options.dictionary?.length) words = applyDictionary(words, options.dictionary);

  const enabled = (id: CommandId) => options.commands?.[id] !== false;
  const out: string[] = [];
  let pressEnter = false;
  let stop = false;

  for (let i = 0; i < words.length; ) {
    // "literally new line" types the words instead of running the command
    if (bare(words[i] as string) === "literally" && i + 1 < words.length) {
      const hit = (Object.keys(COMMAND_PHRASES) as CommandId[]).find((id) => matches(words, i + 1, COMMAND_PHRASES[id]));
      if (hit) {
        const n = COMMAND_PHRASES[hit].length;
        out.push(...words.slice(i + 1, i + 1 + n));
        i += 1 + n;
        continue;
      }
    }
    const hit = (Object.keys(COMMAND_PHRASES) as CommandId[]).find((id) => enabled(id) && matches(words, i, COMMAND_PHRASES[id]));
    if (!hit) {
      out.push(words[i] as string);
      i++;
      continue;
    }
    i += COMMAND_PHRASES[hit].length;
    if (hit === "newLine") out.push(BREAK);
    else if (hit === "newParagraph") out.push(BREAK, BREAK);
    else if (hit === "scratchThat") {
      // drop the words back to the previous break or sentence end
      while (out.length && out[out.length - 1] !== BREAK && !/[.!?]$/.test(out[out.length - 1] as string)) out.pop();
    } else if (hit === "pressEnter") pressEnter = true;
    else if (hit === "stopListening") {
      stop = true;
      break;
    }
  }

  let kept = out;
  if (options.removeFillers !== false) kept = out.filter((w) => w === BREAK || !FILLERS.has(bare(w)));

  return { text: tidy(kept, options.finalPunctuation !== false), pressEnter, stop };
}

function tidy(parts: string[], finalPunctuation: boolean): string {
  let text = "";
  for (const p of parts) {
    if (p === BREAK) text = text.replace(/ +$/, "") + "\n";
    else text += (text === "" || text.endsWith("\n") ? "" : " ") + p;
  }
  text = text
    .replace(/ +([,.!?;:])/g, "$1")
    .replace(/\bi\b(?=$|[\s',.!?])/g, "I")
    .replace(/(^|[.!?]\s+|\n)(\p{Ll})/gu, (_, a: string, b: string) => a + b.toUpperCase());
  if (finalPunctuation) {
    const lines = text.split("\n");
    const last = lines.length - 1;
    const words = (lines[last] ?? "").trim().split(/\s+/).filter(Boolean).length;
    if (words >= 3 && !/[.!?:;,]$/.test(lines[last] as string)) lines[last] += ".";
    text = lines.join("\n");
  }
  return text.trim() === "" ? "" : text;
}
