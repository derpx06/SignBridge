import type { CloudErrorKind } from "./cloud";
/** A dictation's cloud step: `only` = no on-device model, the cloud is the sole recogniser (cloud mode "fallback"/"always"). */
export interface CloudJob { provider: string; key: string; model: string; region: string; only?: boolean; /** stream while speaking (the provider's Live toggle) */ live?: boolean }
import type { ErrorKind, RecState } from "./state";

export type ModelState = "unknown" | "missing" | "downloading" | "paused" | "installed" | "failed";
export interface ModelStatus {
  state: ModelState;
  bytes: number;
  total: number;
  speed: number; // bytes per second
  error?: string; // shown with the failed state, e.g. a checksum message
}
export type MicState = "unknown" | "granted" | "prompt" | "denied" | "none";

/** What every UI renders. The background owns it and broadcasts it on every change. */
export interface Snapshot {
  rec: RecState;
  error?: ErrorKind;
  partial: string[];
  final: string;
  secs: number;
  model: ModelStatus; // status of the active model
  models: Record<string, ModelStatus>; // every catalogue model by id
  activeModel: string;
  mic: MicState;
  into: "page" | "popup"; // where the text will go
  host?: string; // host of the page being dictated into
  detail?: string; // technical reason for the last error (shown in diagnostics only)
  device?: string; // label of the microphone in use
  at?: number; // when this state was last published (ms since epoch)
  target?: { tabId: number; frameId: number }; // the page frame that shows the on-page bar
  reach?: string; // set when the on-page bar could not be shown, with the reason
  startedAt?: number; // when listening began (ms since epoch)
  wordCount: number;
  phase?: "cleaning" | "typing";
  cloud?: boolean; // this dictation is sent to the user's cloud provider (opt-in)
  cloudError?: CloudErrorKind; // the cloud step failed and the on-device text was used
}

export const emptyModel = (): ModelStatus => ({ state: "unknown", bytes: 0, total: 0, speed: 0 });

export type Command = "toggle" | "start" | "stop" | "cancel";
export type CommandSource = "popup" | "pill" | "field" | "menu" | "shortcut" | "options";

/** Messages sent to the background service worker. */
export type ToBackground =
  | { t: "cmd"; cmd: Command; from: CommandSource; tabId?: number }
  | { t: "get" }
  | { t: "focus"; editable: boolean; host: string } // content script: a field gained or lost focus
  | { t: "inserted"; ok: boolean; reason?: string }
  | { t: "whoami" } // content script: which tab and frame am I? (answered with { tabId, frameId })
  | { t: "pill-ack" } // content script: the pill is on screen
  | { t: "model"; op: "install" | "pause" | "cancel" | "remove" | "check" | "bench" | "use"; id?: string }
  | { t: "mic"; op: "check" | "ask-done" }
  | { t: "clear" }
  | { t: "shortcut" } // content script: what is the dictation shortcut? (answered with a string such as "Alt+Shift+V")
  | { t: "insert-popup-text" }
  | { t: "open"; page: "options" | "permission" | "permission-blocked" | "shortcuts" | "mic-settings" | "restart" }
  // from the offscreen document
  | { t: "off-partial"; words: string[] }
  | { t: "off-final"; text: string; ms: number; cloud?: boolean; cloudError?: CloudErrorKind }
  | { t: "off-level"; v: number }
  | { t: "off-error"; kind: ErrorKind; detail?: string }
  | { t: "off-device"; label: string } // which microphone is actually being used
  | { t: "off-silence" }
  | { t: "off-model"; id: string; status: ModelStatus }
  | { t: "off-bench"; rtf: number; ms: number; id: string; heapMB: number }
  | { t: "off-log"; msg: string }
  | { t: "off-online" }
  | { t: "off-ready" };

/** Messages sent to the offscreen document. */
export type ToOffscreen =
  | { target: "offscreen"; t: "start"; autostopSecs: number; model: string; micId?: string; cloud?: CloudJob }
  | { target: "offscreen"; t: "warm"; model: string }
  | { target: "offscreen"; t: "stop" }
  | { target: "offscreen"; t: "cancel" }
  | { target: "offscreen"; t: "model"; op: "install" | "pause" | "cancel" | "remove" | "check" | "bench"; id: string };

/** Messages the background sends to a tab's content script. */
export type ToContent =
  | { t: "snapshot"; snap: Snapshot }
  | { t: "level"; v: number }
  | { t: "insert"; text: string; mode: "type" | "paste"; pressEnter: boolean }
  | { t: "settings" }
  | { t: "remove-ui" }
  | { t: "ping" }; // is the content script loaded yet?
