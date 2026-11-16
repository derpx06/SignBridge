import { describe, expect, it } from "vitest";
import { classifyMicError, deviceLabel, isSilent, loadSettings } from "./index";

describe("microphone diagnostics", () => {
  it("maps getUserMedia errors to actions", () => {
    expect(classifyMicError("NotAllowedError")).toBe("mic-blocked");
    expect(classifyMicError("NotFoundError")).toBe("no-mic");
    expect(classifyMicError("OverconstrainedError")).toBe("no-mic");
    expect(classifyMicError("NotReadableError")).toBe("mic-busy");
    expect(classifyMicError("AbortError")).toBe("mic-busy");
    expect(classifyMicError("SomethingNew")).toBe("engine");
  });
  it("only calls digital silence silent, and only after the grace period", () => {
    expect(isSilent(-120, 1000)).toBe(false); // too early
    expect(isSilent(-120, 2500)).toBe(true);
    expect(isSilent(-48, 5000)).toBe(false); // a quiet room is not a dead mic
    expect(isSilent(-95, 5000)).toBe(false);
  });
  it("never shows a blank device name", () => {
    expect(deviceLabel([], "")).toBe("System default");
    expect(deviceLabel([{ id: "a", label: "USB Mic" }], "a")).toBe("USB Mic");
    expect(deviceLabel([], "gone")).toBe("Selected microphone");
  });
  it("keeps the microphone choice, ignores a bad one", () => {
    expect(loadSettings({ micId: "abc" }).micId).toBe("abc");
    expect(loadSettings({ micId: 5 }).micId).toBe("");
  });
});

import { reduce, initialSession } from "./index";
describe("state machine safety", () => {
  it("a late reset never ends a recording in progress", () => {
    const l = reduce(initialSession(true), { type: "start" });
    expect(reduce(l, { type: "reset" }).rec).toBe("listening");
    const p = reduce(l, { type: "stop" });
    expect(reduce(p, { type: "reset" }).rec).toBe("processing");
  });
  it("reset returns a finished dictation to ready, keeping its text", () => {
    let s = reduce(initialSession(true), { type: "start" });
    s = reduce(reduce(s, { type: "stop" }), { type: "final", text: "hello" });
    s = reduce(s, { type: "inserted" });
    expect(s.rec).toBe("done");
    const r = reduce(s, { type: "reset" });
    expect(r.rec).toBe("ready"); expect(r.final).toBe("hello");
  });
});
