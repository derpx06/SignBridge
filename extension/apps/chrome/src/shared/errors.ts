// One place for what each failure says and what the user can do about it (pill, popup and settings all use it).
import type { ErrorKind } from "@voicio/core";

/** One wording for "no microphone", used by the pill, popup, settings and welcome. */
export const NO_MIC_TITLE = "Chrome can't find a microphone";
export const NO_MIC_BODY = "Check it is plugged in and not muted. If it worked before, Chrome lost its audio devices (after Bluetooth or sound changes): restarting Chrome brings them back.";

export type FixAction = "retry" | "allow" | "unblock" | "choose-mic" | "restart";
export interface ErrorCopy { title: string; body: string; actions: FixAction[] }

/** Google Docs, Sheets and Slides draw their own text editor and ignore typed text from extensions. */
export const isGoogleEditor = (host?: string) => host === "docs.google.com";

export function errorCopy(kind: ErrorKind | undefined, detail?: string, device?: string, host?: string): ErrorCopy {
  if (isGoogleEditor(host) && (kind === "no-field" || kind === "output-blocked"))
    return { title: "Google Docs can't take dictated text", body: "Its editor ignores text typed by extensions. Use Docs' own Tools > Voice typing, or dictate from the Voicio popup and paste.", actions: [] };
  const dev = device || (detail && !/^[A-Z][A-Za-z]+Error$/.test(detail) ? detail.split(":")[0] : "") || "Your microphone";
  switch (kind) {
    case "mic-blocked": return { title: "Microphone blocked", body: "Chrome isn't letting Voicio use the microphone. One click to fix it.", actions: ["unblock"] };
    case "mic-prompt": return { title: "Allow the microphone", body: "A new tab opened to ask. Come back when you've allowed it.", actions: ["allow", "retry"] };
    case "no-mic": return { title: NO_MIC_TITLE, body: NO_MIC_BODY, actions: ["restart", "retry", "choose-mic"] };
    case "mic-busy": return { title: "Microphone is busy", body: "Another app may be using it. Close that app, or choose another microphone.", actions: ["retry", "choose-mic"] };
    case "mic-silent": return { title: "No sound from your microphone", body: `${dev} is muted or isn't the right input. Unmute it or choose another microphone.`, actions: ["choose-mic", "retry"] };
    case "mic-lost": return { title: "Microphone disconnected", body: "Reconnect it, or choose another microphone.", actions: ["choose-mic", "retry"] };
    case "output-blocked": return { title: "Couldn't insert the text", body: "This page blocked it. Your words are in the Voicio popup.", actions: ["retry"] };
    case "no-field": return { title: "Click a text field first", body: "Then press the shortcut again.", actions: ["retry"] };
    case "engine": return { title: "Something went wrong", body: detail ? `The speech engine stopped: ${detail.slice(0, 120)}` : "The speech engine stopped. Try again.", actions: ["retry"] };
    default: return { title: "Didn't catch that", body: "Try again a little closer to the microphone.", actions: ["retry"] };
  }
}
