// Microphone diagnostics shared by the offscreen recorder and every page that shows mic state. Pure, so it is unit-tested.
import type { ErrorKind } from "./state";

/** Map a getUserMedia failure (DOMException name) to what the user can do about it. */
export function classifyMicError(name: string): ErrorKind {
  switch (name) {
    case "NotAllowedError":
    case "PermissionDeniedError":
    case "SecurityError":
      return "mic-blocked"; // refused by the user, by Chrome settings or by policy
    case "NotFoundError":
    case "DevicesNotFoundError":
    case "OverconstrainedError": // the chosen device is gone
    case "ConstraintNotSatisfiedError":
      return "no-mic";
    case "NotReadableError": // another app holds it, or the hardware failed
    case "TrackStartError":
    case "AbortError":
      return "mic-busy";
    default:
      return "engine";
  }
}

/** Digital silence (rmsDb of all-zero samples is -120). A merely quiet room is far above this. */
export const SILENT_DB = -110;
export const SILENT_AFTER_MS = 2000;

/** A device that opened but only ever delivers zeros: muted, wrong input, or a dead monitor source. */
export const isSilent = (maxDb: number, elapsedMs: number) => elapsedMs >= SILENT_AFTER_MS && maxDb <= SILENT_DB;

export interface MicDevice { id: string; label: string }
export type MicPermission = "granted" | "prompt" | "denied" | "unknown";
export interface MicProbe { permission: MicPermission; devices: MicDevice[] }

/** Human label for a device id ("" = system default). Falls back so the UI never shows a blank name. */
export function deviceLabel(devices: MicDevice[], id: string): string {
  if (!id) return devices.find((d) => d.id === "default")?.label || "System default";
  return devices.find((d) => d.id === id)?.label || "Selected microphone";
}
