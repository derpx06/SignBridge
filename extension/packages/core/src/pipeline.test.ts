import { describe, expect, it } from "vitest";
import { runPipeline } from "./pipeline";
import { initialSession, reduce } from "./state";
import { loadSettings, defaultSettings } from "./settings";
import { isForbiddenField } from "./fields";

const run = (s: string, o = {}) => runPipeline(s, o).text;

describe("pipeline", () => {
  it("capitalises and ends a plain sentence", () => {
    expect(run("hello there my friend")).toBe("Hello there my friend.");
  });
  it("new line and new paragraph", () => {
    expect(run("first line new line second line")).toBe("First line\nSecond line");
    expect(run("one two three new paragraph four five six")).toBe("One two three\n\nFour five six.");
  });
  it("scratch that removes the last phrase", () => {
    expect(run("send it to bob scratch that send it to alice")).toBe("Send it to alice.");
    expect(run("Good morning. buy milk scratch that")).toBe("Good morning.");
  });
  it("literally escapes a command", () => {
    expect(run("say literally new line to start a line")).toBe("Say new line to start a line.");
  });
  it("press enter and stop listening set flags", () => {
    const r = runPipeline("hello world press enter");
    expect(r.pressEnter).toBe(true);
    expect(r.text).toBe("Hello world");
    expect(runPipeline("hi there stop listening ignored words").stop).toBe(true);
  });
  it("applies dictionary corrections, longest first", () => {
    const dictionary = [
      { when: "kube control", write: "kubectl" },
      { when: "voice yo", write: "Voicio" },
      { when: "jason", write: "JSON" },
    ];
    expect(run("run kube control now please", { dictionary })).toBe("Run kubectl now please.");
    expect(run("welcome to voice yo", { dictionary })).toBe("Welcome to Voicio.");
    expect(run("parse jason, then stop", { dictionary })).toBe("Parse JSON, then stop.");
  });
  it("removes fillers but keeps words", () => {
    expect(run("so um I think uh we should go")).toBe("So I think we should go.");
    expect(runPipeline("um hello there friend", { removeFillers: false }).text).toBe("Um hello there friend.");
  });
  it("respects disabled commands", () => {
    expect(run("a b c new line d e f", { commands: { newLine: false } })).toBe("A b c new line d e f.");
  });
  it("fixes the pronoun I", () => {
    expect(run("i think i'm right here")).toBe("I think I'm right here.");
  });
  it("returns empty for nothing", () => {
    expect(run("")).toBe("");
    expect(run("um uh")).toBe("");
  });
});

describe("state machine", () => {
  it("runs a full take", () => {
    let s = initialSession(true);
    s = reduce(s, { type: "start" });
    s = reduce(s, { type: "partial", words: ["hi"] });
    s = reduce(s, { type: "stop" });
    s = reduce(s, { type: "final", text: "Hi." });
    s = reduce(s, { type: "inserted" });
    expect(s.rec).toBe("done");
  });
  it("cannot start without a model; Esc returns to ready", () => {
    expect(reduce(initialSession(false), { type: "start" }).rec).toBe("nomodel");
    expect(reduce(initialSession(false), { type: "start", cloudOnly: true }).rec).toBe("listening");
    const l = reduce(initialSession(true), { type: "start" });
    expect(reduce(l, { type: "cancel" }).rec).toBe("ready");
    expect(reduce(l, { type: "fail", error: "mic-blocked" }).error).toBe("mic-blocked");
  });
});

describe("settings and fields", () => {
  it("recovers from broken storage", () => {
    expect(loadSettings({ mode: "nope", insertAs: "paste" }).insertAs).toBe("paste");
    expect(loadSettings({ mode: "nope" }).mode).toBe("toggle");
    expect(loadSettings(null)).toEqual(defaultSettings());
    expect(loadSettings({ pillOffset: { x: 12, y: -40 } }).pillOffset).toEqual({ x: 12, y: -40 });
    expect(loadSettings({ pillOffset: "nope" }).pillOffset).toEqual({ x: 0, y: 0 });
  });
  it("blocks password and card fields", () => {
    expect(isForbiddenField({ type: "password" })).toBe(true);
    expect(isForbiddenField({ type: "text", autocomplete: "section-x cc-number" })).toBe(true);
    expect(isForbiddenField({ type: "text", autocomplete: "email" })).toBe(false);
  });
});
