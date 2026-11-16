// One state machine for popup, options, pill and background. Pure: (state, event) -> state.
export type RecState = "ready" | "listening" | "processing" | "done" | "error" | "nomodel";
export type ErrorKind = "no-speech" | "output-blocked" | "mic-blocked" | "mic-prompt" | "no-mic" | "no-field" | "engine" | "mic-busy" | "mic-silent" | "mic-lost";

export interface Session {
  rec: RecState;
  error?: ErrorKind;
  partial: string[]; // live words; the last few are "unsettled"
  final: string;
}

export type SessionEvent =
  | { type: "start"; cloudOnly?: boolean } // cloudOnly: no on-device model, the cloud provider takes the dictation
  | { type: "partial"; words: string[] }
  | { type: "stop" } // user or VAD ended listening
  | { type: "final"; text: string }
  | { type: "inserted" }
  | { type: "fail"; error: ErrorKind }
  | { type: "cancel" } // Esc or x
  | { type: "reset" }
  | { type: "model"; installed: boolean };

export const initialSession = (modelInstalled: boolean): Session => ({
  rec: modelInstalled ? "ready" : "nomodel",
  partial: [],
  final: "",
});

export function reduce(s: Session, e: SessionEvent): Session {
  switch (e.type) {
    case "model":
      if (s.rec === "listening" || s.rec === "processing") return s;
      return { ...s, rec: e.installed ? "ready" : "nomodel", error: undefined };
    case "start":
      return s.rec === "ready" || s.rec === "done" || s.rec === "error" || (s.rec === "nomodel" && e.cloudOnly) ? { rec: "listening", partial: [], final: "" } : s;
    case "partial":
      return s.rec === "listening" ? { ...s, partial: e.words } : s;
    case "stop":
      return s.rec === "listening" ? { ...s, rec: "processing" } : s;
    case "final":
      return s.rec === "processing" ? { ...s, final: e.text } : s;
    case "inserted":
      return s.rec === "processing" ? { ...s, rec: "done" } : s;
    case "fail":
      return s.rec === "listening" || s.rec === "processing" ? { ...s, rec: "error", error: e.error } : s;
    case "reset": // back to ready after "done"; the last transcript stays until the next start
      return s.rec === "done" || s.rec === "error" ? { ...s, rec: "ready", error: undefined } : s; // never ends a recording in progress
    case "cancel":
      return s.rec === "nomodel" ? { ...s, partial: [], final: "" } : { rec: "ready", partial: [], final: "" };
  }
}

/** How many trailing words are shown dimmed as "unsettled". */
export const UNSETTLED_WORDS = 3;
