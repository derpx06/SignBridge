import { useEffect, useState, type ReactNode } from "react";
import { COMMAND_PHRASES, type CommandId, type Settings, type Snapshot, runPipeline } from "@voicio/core";
import { Button, Card, Chip, Icon, IconButton, Keycaps, Logo, Segmented, Switch, type IconName } from "@voicio/ui";
import { send, useEverywhere, useSettings, useShortcut, useSnapshot } from "../shared/hooks";
import { CATALOG, DEFAULT_ID, byId, formatBytes, infoOf, subtitleOf, totalBytes } from "../shared/models";
import { CloudCard } from "./CloudCard";
import { ErrorNotice, MicPicker, MicStage, ModelPanel, Transcript, TranscriptActions, StatusChip } from "../shared/widgets";

type Section = "welcome" | "dictate" | "general" | "sites" | "words" | "appearance" | "model" | "cloud" | "privacy" | "about";
const NAV: { id: Section; label: string; icon: IconName; tint: string }[] = [
  { id: "dictate", label: "Dictate", icon: "mic", tint: "#FFD8C2" }, { id: "general", label: "General", icon: "settings", tint: "#E8E2D6" },
  { id: "sites", label: "Sites", icon: "globe", tint: "#D9ECFF" }, { id: "words", label: "Words & commands", icon: "book", tint: "#E6F7C4" },
  { id: "appearance", label: "Appearance", icon: "sun", tint: "#FFE7A8" }, { id: "model", label: "Speech models", icon: "cpu", tint: "#FFC9B0" },
  { id: "cloud", label: "Cloud", icon: "cloud", tint: "#C9E2FF" }, { id: "privacy", label: "Privacy", icon: "shield", tint: "#D8F2C2" },
  { id: "about", label: "About", icon: "info", tint: "#EFE9DD" },
];
/** Swatch per model, the same colours as the desktop pack cards. */
const SWATCH: Record<string, string> = { "moonshine-tiny": "#C5F07A", "moonshine-base": "#FFB38A" };
const VERSION = chrome.runtime.getManifest().version;

export function Options() {
  const snap = useSnapshot();
  const [settings, set] = useSettings();
  const keys = useShortcut();
  const initial = location.hash.slice(1) as Section;
  const [section, setSection] = useState<Section>(initial === "welcome" || NAV.some((n) => n.id === initial) ? initial : "dictate");
  useEffect(() => {
    const fn = () => { const h = location.hash.slice(1) as Section; if (h === "welcome" || NAV.some((n) => n.id === h)) setSection(h); };
    window.addEventListener("hashchange", fn);
    return () => window.removeEventListener("hashchange", fn);
  }, []);
  useEffect(() => { document.title = `Voicio · ${NAV.find((n) => n.id === section)?.label ?? "Welcome"}`; }, [section]);

  return (
    <div className="opt-grid" style={{ minHeight: "100vh", background: "var(--bg)" }}>
      <nav className="opt-nav" aria-label="Settings sections" style={{ padding: "24px 16px 18px", display: "flex", flexDirection: "column", gap: 4, position: "sticky", top: 0, height: "100vh", overflowY: "auto" }}>
        <div className="p-row opt-brand" style={{ gap: 10, padding: "0 8px 24px" }}><Logo size={40} /><span className="p-brand" style={{ fontSize: 25 }}>voicio</span></div>
        {NAV.map((n) => (
          <button key={n.id} aria-current={section === n.id ? "page" : undefined} onClick={() => { setSection(n.id); history.replaceState(null, "", `#${n.id}`); }}
            className="p-row" style={{ minHeight: 46, padding: "0 10px", gap: 12, borderRadius: 14, border: 0, textAlign: "left", background: "transparent", color: "var(--tx)", font: "500 15px var(--font)", flex: "none" }}>
            <span className="v-tile" style={{ width: 28, height: 28, borderRadius: 9, background: n.tint }}><Icon name={n.icon} size={15} stroke={2.2} /></span>{n.label}
          </button>
        ))}
        <div style={{ flex: 1, minHeight: 20 }} />
        <button className="p-row opt-foot" onClick={() => { setSection("model"); history.replaceState(null, "", "#model"); }}
          style={{ width: "100%", minHeight: 62, padding: "10px 12px", gap: 10, borderRadius: 18, border: 0, background: "var(--surf)", boxShadow: "var(--lift)", color: "var(--tx)", textAlign: "left" }}>
          <span style={{ width: 36, height: 36, flex: "none", borderRadius: 11, background: SWATCH[snap.activeModel] ?? "#FFB38A", display: "grid", placeItems: "center", color: "#16130F" }}><Icon name="mic" size={16} stroke={2.4} /></span>
          <span style={{ flex: 1, minWidth: 0 }}><b style={{ display: "block", font: "700 14px/18px var(--font)" }}>{byId(snap.activeModel).name}</b>
            <span className="p-hint" style={{ display: "flex", alignItems: "center", gap: 5 }}><i className="v-dot" style={{ color: snap.model.state === "installed" ? "var(--ok)" : "var(--warn)" }} />{snap.model.state === "installed" ? `${infoOf(byId(snap.activeModel)).languages} · works offline` : "Not downloaded yet"}</span></span>
        </button>
      </nav>
      <main className="opt-main" style={{ padding: "36px 44px 64px" }}>
        <div style={{ maxWidth: 760 }}>
          {section === "welcome" && <Welcome snap={snap} keys={keys} go={(x) => { setSection(x); history.replaceState(null, "", `#${x}`); }} />}
          {section === "dictate" && <DictateSection snap={snap} keys={keys} settings={settings} set={set} />}
          {section === "general" && <General settings={settings} set={set} keys={keys} />}
          {section === "sites" && <Sites settings={settings} set={set} />}
          {section === "words" && <Words settings={settings} set={set} />}
          {section === "appearance" && <Appearance settings={settings} set={set} />}
          {section === "model" && <Model snap={snap} go={(x) => { setSection(x); history.replaceState(null, "", `#${x}`); }} />}
          {section === "cloud" && <><H title="Cloud" sub="Optional. Use your own speech provider account; on-device stays the default." /><CloudCard /></>}
          {section === "privacy" && <Privacy />}
          {section === "about" && <About />}
        </div>
      </main>
    </div>
  );
}

const H = ({ title, sub, right }: { title: string; sub?: string; right?: ReactNode }) => (
  <header className="p-row" style={{ justifyContent: "space-between", marginBottom: 24, alignItems: "flex-end" }}>
    <div><h1 className="p-h1">{title}</h1>{sub && <p style={{ margin: "8px 0 0", color: "var(--tx2)", fontSize: 15, lineHeight: "22px" }}>{sub}</p>}</div>{right}
  </header>
);
const Row = ({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) => (
  <div className="p-row opt-row" style={{ justifyContent: "space-between", padding: "14px 0", gap: 24 }}>
    <div><div className="p-label">{label}</div>{hint && <div className="p-hint">{hint}</div>}</div><div style={{ flex: "none" }}>{children}</div>
  </div>
);
type P = { settings: Settings; set: (p: Partial<Settings>) => void };

function Welcome({ snap, keys, go }: { snap: Snapshot; keys: string[]; go: (s: Section) => void }) {
  const [mic, setMic] = useState<PermissionState | "unknown">("unknown");
  useEffect(() => {
    let p: PermissionStatus | undefined;
    void navigator.permissions.query({ name: "microphone" as PermissionName }).then((r) => { p = r; setMic(r.state); r.onchange = () => setMic(r.state); }).catch(() => {});
    return () => { if (p) p.onchange = null; };
  }, []);
  const [everywhere, setEverywhere] = useEverywhere();
  const m = snap.model;
  const modelDone = m.state === "installed";
  const micDone = mic === "granted";
  const step = (n: number, done: boolean, active: boolean, title: string, body: ReactNode, action?: ReactNode) => (
    <Card accent={active} style={{ opacity: !done && !active ? 0.65 : 1 }}>
      <div className="p-row" style={{ alignItems: "flex-start", gap: 14 }}>
        <span style={{ width: 28, height: 28, borderRadius: "50%", flex: "none", display: "grid", placeItems: "center", background: done ? "var(--ink)" : active ? "var(--ac)" : "var(--line2)", color: done ? "var(--on-ink)" : active ? "#16130F" : "var(--tx3)", font: "800 13px var(--font)" }}>{done ? <Icon name="check" size={14} stroke={2.5} /> : n}</span>
        <div style={{ flex: 1, minWidth: 0 }}><div className="p-label" style={{ fontSize: 14 }}>{title}</div><div className="p-hint" style={{ color: "var(--tx2)", marginTop: 2 }}>{body}</div>{action && <div style={{ marginTop: 12 }}>{action}</div>}</div>
      </div>
    </Card>
  );
  return (
    <>
      <div className="p-row" style={{ gap: 14, marginBottom: 24 }}>
        <Logo size={56} />
        <div><h1 className="p-h1">Welcome to <em>Voicio</em></h1>
          <p style={{ margin: "2px 0 0", color: "var(--tx2)" }}>Speak, and it types. Speech is recognised on this computer.</p></div>
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        {step(1, modelDone, !modelDone, modelDone ? "Speech model ready" : "Choose a speech model",
          modelDone ? `${byId(snap.activeModel).name} is saved in Chrome. Voicio now works with Wi-Fi off.` : "Pick one to download once from huggingface.co. Nothing downloads until you press Download. You can change it later in Settings.",
          !modelDone && (
            <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
              {CATALOG.map((c) => (
                <Card key={c.id} style={{ background: "var(--surf2)", boxShadow: "none" }}>
                  <div className="p-row" style={{ justifyContent: "space-between", gap: 12, flexWrap: "wrap" }}>
                    <div className="p-row" style={{ gap: 12 }}>
                      <span style={{ width: 36, height: 36, flex: "none", borderRadius: 11, background: SWATCH[c.id] ?? "#FFB38A" }} />
                      <div>
                        <div className="p-label" style={{ fontSize: 15, display: "flex", gap: 8, alignItems: "center" }}>{c.name} {c.id === DEFAULT_ID && <Chip kind="ink">Recommended</Chip>}</div>
                        <div className="p-hint">{subtitleOf(c)} · {formatBytes(totalBytes(c))}{c.wer > 0 ? ` · ${c.wer.toFixed(1)}% word errors` : ""}</div>
                      </div>
                    </div>
                    <ModelPanel snap={snap} id={c.id} compact />
                  </div>
                </Card>
              ))}
            </div>
          ))}
        {step(2, micDone, modelDone && !micDone, micDone ? "Microphone allowed" : "Allow the microphone",
          micDone ? "Voicio only listens while you dictate." : "Chrome asks once. The mic is only on while you're dictating.",
          !micDone && <Button kind="primary" disabled={mic === "denied"} onClick={() => void send({ t: "open", page: "permission" })}>{mic === "denied" ? "Blocked in Chrome" : "Allow microphone"}</Button>)}
        {step(3, everywhere, modelDone && micDone && !everywhere, everywhere ? "Works on every website" : "Use it on any website",
          everywhere ? "The dictation bar shows up wherever you press the shortcut." : "Chrome asks once. Voicio only touches the text field you dictate into, and never reads or sends page content.",
          !everywhere && <Button kind="primary" onClick={() => void setEverywhere(true)}>Allow on all websites</Button>)}
        {step(4, false, modelDone && micDone && everywhere, "Try it", <>Click a text field on any page and press <Keycaps keys={keys} />. Say “new line” or “scratch that” to edit as you speak.</>,
          <Button onClick={() => go("dictate")}>Try it here</Button>)}
      </div>
      <p className="p-hint" style={{ marginTop: 16 }}>Shortcut not working? Chrome lets you change it at <button className="p-link" onClick={() => void send({ t: "open", page: "shortcuts" })}>chrome://extensions/shortcuts</button>.</p>
    </>
  );
}

function DictateSection({ snap, keys, settings, set }: P & { snap: Snapshot; keys: string[] }) {
  return (
    <>
      <H title="Dictate" sub="Try it here, or press the shortcut on any page." right={<StatusChip snap={snap} />} />
      <div style={{ marginBottom: 12 }}><ErrorNotice snap={snap} /></div>
      <Card style={{ padding: 24 }}>
        <MicStage snap={snap} keys={keys} size={88} />
        <div style={{ height: 16 }} />
        <Transcript snap={snap} tall />
        <div style={{ height: 10 }} />
        <TranscriptActions snap={snap} />
        <div className="p-row" style={{ justifyContent: "space-between", marginTop: 14 }}>
          <span className="p-label">Dictate into the focused field</span>
          <Switch label="Dictate into the focused field" checked={settings.intoFocusedField} onChange={(v) => set({ intoFocusedField: v })} />
        </div>
      </Card>
      <div style={{ height: 12 }} />
      <Card style={{ padding: 20 }}>
        <div className="p-label" style={{ marginBottom: 2 }}>Microphone</div>
        <div className="p-hint" style={{ marginBottom: 12 }}>Pick the input Voicio listens to, and check that it works.</div>
        <MicPicker micId={settings.micId} onChange={(micId) => set({ micId })} />
      </Card>
    </>
  );
}

function General({ settings, set, keys }: P & { keys: string[] }) {
  const [everywhere, setEverywhere] = useEverywhere();
  return (
    <>
      <H title="General" />
      <Card style={{ padding: "4px 22px" }}>
      <Row label="Shortcut" hint="Chrome owns extension shortcuts, so you change it there.">
        <div className="p-row" style={{ gap: 12 }}><Keycaps keys={keys} /><Button icon="ext" onClick={() => void send({ t: "open", page: "shortcuts" })}>Change</Button></div>
      </Row>
      <Row label="Mode" hint={settings.mode === "toggle" ? "Press once to start, again to finish." : "Hold the shortcut while you speak. Works while a text field is focused."}>
        <Segmented label="Mode" value={settings.mode} onChange={(mode) => set({ mode })} options={[{ value: "toggle", label: "Tap to toggle" }, { value: "hold", label: "Hold to talk" }]} />
      </Row>
      <Row label="Insert as" hint={settings.insertAs === "type" ? "Types at the caret. Works in almost every field." : "Sends one paste. Better for editors that handle paste themselves."}>
        <Segmented label="Insert as" value={settings.insertAs} onChange={(insertAs) => set({ insertAs })} options={[{ value: "type", label: "Typing" }, { value: "paste", label: "Paste" }]} />
      </Row>
      <Row label="Auto-stop after silence" hint="Finishes for you when you stop talking.">
        <Segmented label="Auto-stop" value={String(settings.autostopSecs)} onChange={(v) => set({ autostopSecs: Number(v) as 0 | 3 | 10 })} options={[{ value: "0", label: "Off" }, { value: "3", label: "3 s" }, { value: "10", label: "10 s" }]} />
      </Row>
      <Row label="Works on every website" hint="Shows the dictation bar wherever you press the shortcut."><Switch label="Works on every website" checked={everywhere} onChange={(v) => void setEverywhere(v)} /></Row>
      <Row label="Right-click “Dictate here”"><Switch label="Right-click menu" checked={settings.contextMenu} onChange={(contextMenu) => set({ contextMenu })} /></Row>
      <Row label="Mic button in text fields" hint="Shows on sites you allow."><Switch label="Mic button in text fields" checked={settings.fieldMic} onChange={(fieldMic) => set({ fieldMic })} /></Row>
      </Card>
    </>
  );
}

function Sites({ settings, set }: P) {
  const [q, setQ] = useState("");
  const host = q.trim().replace(/^https?:\/\//, "").replace(/\/.*$/, "").toLowerCase();
  const rows = settings.sites.filter((s) => s.host.includes(host));
  const put = (h: string, rule?: "allow" | "never") => set({ sites: [...settings.sites.filter((s) => s.host !== h), ...(rule ? [{ host: h, rule }] : [])] });
  const allowHost = async (h: string) => { if (h && await chrome.permissions.request({ origins: [`*://${h}/*`] }).catch(() => false)) put(h, "allow"); };
  const allow = () => allowHost(host);
  return (
    <>
      <H title="Sites" sub="Control where Voicio shows its mic button and where it stays silent." />
      <div className="p-row" style={{ gap: 8, marginBottom: 12 }}>
        <div style={{ flex: 1, position: "relative" }}>
          <span style={{ position: "absolute", left: 13, top: 13, color: "var(--tx3)" }}><Icon name="search" size={16} /></span>
          <input aria-label="Site" className="p-input" value={q} onChange={(e) => setQ(e.target.value)} placeholder="example.com" style={{ width: "100%", paddingLeft: 38 }} />
        </div>
        <Button large onClick={() => void allow()} disabled={!host}>Allow</Button>
        <Button large kind="danger" onClick={() => put(host, "never")} disabled={!host}>Never on this site</Button>
      </div>
      <Card style={{ padding: 4 }}>
        {rows.length === 0 && <div className="p-hint" style={{ padding: 16, textAlign: "center" }}>No sites match. Press Allow or Never to add it.</div>}
        {rows.map((s) => (
          <div key={s.host} className="v-row"><Icon name="globe" size={14} style={{ color: "var(--tx3)" }} /><span className="p-mono" style={{ flex: 1, fontSize: 13 }}>{s.host}</span>
            <span className="p-hint" aria-hidden="true" style={{ minWidth: 56, textAlign: "right", color: s.rule === "allow" ? "var(--ok-tx)" : "var(--err)" }}>{s.rule === "allow" ? "Allowed" : "Never"}</span>
            <Switch label={`Allow Voicio on ${s.host}`} checked={s.rule === "allow"} onChange={(v) => (v ? void allowHost(s.host) : put(s.host, "never"))} />
            <IconButton icon="trash" label={`Remove ${s.host}`} destructive onClick={() => put(s.host)} /></div>))}
      </Card>
      <div style={{ height: 12 }} />
      <Card><div className="p-row"><Icon name="lock" size={16} style={{ color: "var(--tx3)" }} /><div style={{ flex: 1 }}><div className="p-label">Dictate into password fields</div><div className="p-hint">Always off. Password and card fields are never touched.</div></div><Switch label="Dictate into password fields" checked={false} disabled onChange={() => {}} /></div></Card>
    </>
  );
}

const CMD_INFO: Record<CommandId, [IconName, string]> = {
  newLine: ["enter", "Inserts a line break"], newParagraph: ["pilcrow", "Inserts a blank line"], scratchThat: ["undo", "Deletes the last phrase"],
  pressEnter: ["keyboard", "Sends the Enter key"], stopListening: ["stop", "Ends dictation and inserts"],
};
function Words({ settings, set }: P) {
  const [when, setWhen] = useState(""); const [write, setWrite] = useState("");
  const add = () => { if (when.trim() && write.trim()) { set({ dictionary: [...settings.dictionary, { when: when.trim(), write: write.trim() }] }); setWhen(""); setWrite(""); } };
  const preview = runPipeline("tell jason to run kube control and press enter", { dictionary: settings.dictionary, commands: settings.commands }).text;
  return (
    <>
      <H title="Words & commands" sub="Fix names the recogniser gets wrong and choose which spoken commands work." />
      <p className="p-sec">Words and corrections</p>
      <Card style={{ padding: 8 }}>
        {settings.dictionary.length === 0 && <div className="p-hint" style={{ padding: 12 }}>Nothing yet. Example: when I say “kube control”, write “kubectl”.</div>}
        {settings.dictionary.map((d, i) => (
          <div key={i} className="v-row"><Chip kind="ok">Correction</Chip><span style={{ flex: 1 }}>{d.when}</span><Icon name="arrowRight" size={14} style={{ color: "var(--tx3)" }} /><span className="p-mono" style={{ flex: 1 }}>{d.write}</span>
            <IconButton icon="trash" label={`Delete ${d.when}`} destructive onClick={() => set({ dictionary: settings.dictionary.filter((_, j) => j !== i) })} /></div>))}
        <div className="p-row" style={{ padding: 8, gap: 8 }}>
          <input aria-label="When I say" className="p-input" placeholder="When I say" value={when} onChange={(e) => setWhen(e.target.value)} style={{ flex: 1 }} onKeyDown={(e) => e.key === "Enter" && add()} />
          <input aria-label="Write" className="p-input" placeholder="Write" value={write} onChange={(e) => setWrite(e.target.value)} style={{ flex: 1, fontFamily: "var(--mono)" }} onKeyDown={(e) => e.key === "Enter" && add()} />
          <Button icon="plus" onClick={add} disabled={!when.trim() || !write.trim()}>Add word</Button>
        </div>
      </Card>
      <p className="p-hint" style={{ marginTop: 8 }}>Preview: “tell jason to run kube control and press enter” → <span className="p-mono">{preview}</span></p>
      <div style={{ height: 20 }} />
      <p className="p-sec">Voice commands</p>
      <Card style={{ padding: 4 }}>
        {(Object.keys(COMMAND_PHRASES) as CommandId[]).map((id) => (
          <div key={id} className="v-row" style={{ minHeight: 48 }}>
            <span className="v-tile" style={{ background: "#E6F7C4" }}><Icon name={CMD_INFO[id][0]} size={16} stroke={2.2} /></span>
            <div style={{ flex: 1 }}><div className="p-mono" style={{ fontSize: 13 }}>“{COMMAND_PHRASES[id].join(" ")}”</div><div className="p-hint">{CMD_INFO[id][1]}</div></div>
            <Switch label={COMMAND_PHRASES[id].join(" ")} checked={settings.commands[id] !== false} onChange={(v) => set({ commands: { ...settings.commands, [id]: v } })} />
          </div>))}
      </Card>
      <p className="p-hint" style={{ marginTop: 8 }}>Say “literally” first to type the words instead — “literally new line”.</p>
    </>
  );
}

function Appearance({ settings, set }: P) {
  const themes: { v: Settings["theme"]; label: string; icon: IconName }[] = [{ v: "system", label: "System", icon: "monitor" }, { v: "light", label: "Light", icon: "sun" }, { v: "dark", label: "Dark", icon: "moon" }];
  const pos: { v: Settings["pillPos"]; label: string }[] = [{ v: "dock", label: "Bottom centre" }, { v: "above-left", label: "Above left" }, { v: "above-right", label: "Above right" }, { v: "below-left", label: "Below left" }, { v: "below-right", label: "Below right" }];
  return (
    <>
      <H title="Appearance" />
      <p className="p-sec">Theme</p>
      <div role="radiogroup" aria-label="Theme" style={{ display: "grid", gridTemplateColumns: "repeat(3,1fr)", gap: 12, marginBottom: 24 }}>
        {themes.map((t) => (
          <button key={t.v} role="radio" aria-checked={settings.theme === t.v} onClick={() => set({ theme: t.v })} style={{ background: "var(--surf)", borderRadius: 20, padding: 18, border: 0, boxShadow: settings.theme === t.v ? "inset 0 0 0 2px var(--ink)" : "0 18px 40px rgba(60,40,10,.05)", display: "flex", flexDirection: "column", alignItems: "center", gap: 8 }}>
            <Icon name={t.icon} size={22} /><span className="p-label">{t.label}</span></button>))}
      </div>
      <p className="p-sec">Pill position</p>
      <div role="radiogroup" aria-label="Pill position" style={{ display: "grid", gridTemplateColumns: "repeat(2,1fr)", gap: 12 }}>
        {pos.map((p) => (
          <button key={p.v} role="radio" aria-checked={settings.pillPos === p.v} onClick={() => set({ pillPos: p.v })} style={{ background: "var(--surf)", borderRadius: 20, padding: 14, border: 0, boxShadow: settings.pillPos === p.v ? "inset 0 0 0 2px var(--ink)" : "0 18px 40px rgba(60,40,10,.05)", textAlign: "left" }}>
            <div style={{ position: "relative", height: 54, background: "var(--surf2)", borderRadius: 12, marginBottom: 8 }}>
              <i style={{ position: "absolute", width: 40, height: 10, borderRadius: 8, background: "var(--ink)", [p.v.startsWith("above") ? "top" : "bottom"]: -5, [p.v.endsWith("left") ? "left" : "right"]: 8 }} /></div>
            <span className="p-label">{p.label}</span></button>))}
      </div>
    </>
  );
}

function Model({ snap, go }: { snap: Snapshot; go: (s: Section) => void }) {
  const [bench, setBench] = useState<Record<string, { rtf: number; ms: number }>>({});
  const [running, setRunning] = useState("");
  useEffect(() => {
    const fn = (m: { t?: string; rtf?: number; ms?: number; id?: string }) => { if (m?.t === "bench" && m.id) { setBench((b) => ({ ...b, [m.id!]: { rtf: m.rtf!, ms: m.ms! } })); setRunning(""); } };
    chrome.runtime.onMessage.addListener(fn);
    return () => chrome.runtime.onMessage.removeListener(fn);
  }, []);
  const anyDownloading = Object.values(snap.models).some((x) => x.state === "downloading");
  const op = (o: "install" | "pause" | "cancel" | "remove" | "use", id: string) => send({ t: "model", op: o, id });
  return (
    <>
      <H title="Speech models" sub="The model that turns your voice into text. It runs inside Chrome, on this computer." />
      <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        {CATALOG.map((m) => {
          const st = snap.models[m.id];
          const state = st?.state ?? "unknown";
          const active = snap.activeModel === m.id;
          const installed = state === "installed";
          return (
            <Card key={m.id} style={{ padding: 20, background: state === "failed" ? "var(--err-soft)" : undefined }}>
              <div className="p-row" style={{ gap: 16, alignItems: "flex-start", flexWrap: "wrap" }}>
                <span style={{ width: 48, height: 48, borderRadius: 15, flex: "none", background: SWATCH[m.id] ?? "#FFB38A", display: "grid", placeItems: "center", color: "#16130F", font: "800 16px var(--head)" }}>{m.name.split(" ").map((w) => w[0]).join("")}</span>
                <div style={{ flex: "1 1 260px", minWidth: 0 }}>
                  <div className="p-row" style={{ gap: 8, flexWrap: "wrap" }}>
                    <b style={{ font: "800 19px/24px var(--head)", letterSpacing: "-.3px" }}>{m.name}</b>
                    {m.id === DEFAULT_ID && <Chip kind="ink">Recommended</Chip>}
                    {active && installed && <Chip kind="ok" icon="check">In use</Chip>}
                    {installed && !active && <Chip>Downloaded</Chip>}
                    {state === "failed" && <Chip kind="err">Failed</Chip>}
                  </div>
                  <div className="p-hint" style={{ marginTop: 2, fontSize: 13.5 }}>{subtitleOf(m)}</div>
                  <div className="p-hint" style={{ marginTop: 6, color: "var(--tx2)" }}>{infoOf(m).tagline}</div>
                  <div className="p-row" style={{ gap: 6, marginTop: 10, flexWrap: "wrap" }}>
                    <Chip kind="line">{formatBytes(totalBytes(m))} download</Chip>
                    {m.wer > 0 && <Chip kind="line">{m.wer.toFixed(1)}% word errors</Chip>}
                    <Chip kind="line">{m.license.toUpperCase()}</Chip>
                  </div>
                  {(state === "downloading" || state === "paused") && <div style={{ marginTop: 12 }}><ModelPanel snap={snap} id={m.id} compact /></div>}
                  {state === "failed" && <div className="p-hint" style={{ color: "var(--tx2)", marginTop: 6 }}>{st?.error}</div>}
                  {bench[m.id] && <div className="p-hint p-mono" style={{ marginTop: 8 }}>Speed test: {bench[m.id]!.rtf.toFixed(2)}× real time · {bench[m.id]!.ms} ms for a 5 s clip</div>}
                </div>
                <div className="p-row" style={{ gap: 6, flex: "none" }}>
                  {installed && !active && <Button kind="primary" onClick={() => op("use", m.id)}>Use</Button>}
                  {installed && <Button icon="gauge" loading={running === m.id} disabled={!!running} onClick={() => { setRunning(m.id); void send({ t: "model", op: "bench", id: m.id }); }}>Test speed</Button>}
                  {installed && <IconButton icon="trash" label={`Remove ${m.name}`} destructive onClick={() => op("remove", m.id)} />}
                  {(state === "missing" || state === "failed" || state === "unknown") && <Button kind={active ? "primary" : "secondary"} icon="download" disabled={anyDownloading} onClick={() => op("install", m.id)}>{state === "failed" ? "Retry" : "Download"}</Button>}
                </div>
              </div>
            </Card>
          );
        })}
      </div>
      <Card style={{ marginTop: 12, background: "var(--sky)", boxShadow: "none" }}>
        <div className="p-row" style={{ gap: 12, flexWrap: "wrap" }}>
          <span className="v-tile" style={{ background: "#C9E2FF" }}><Icon name="cloud" size={16} stroke={2.2} /></span>
          <div style={{ flex: "1 1 240px" }}><div className="p-label">Want more languages or bigger models?</div><div className="p-hint" style={{ color: "var(--tx2)" }}>Bring your own key for Groq, OpenAI, Deepgram and more. Off by default.</div></div>
          <Button onClick={() => go("cloud")}>Set up cloud</Button>
        </div>
      </Card>
      <p className="p-hint" style={{ marginTop: 16 }}>Downloaded once from huggingface.co and checked against a fixed checksum. After that, Voicio works with Wi-Fi off. Only one model is kept in memory at a time, and it is released 10 minutes after you stop dictating. Every model here stays under 1 GB of memory, extension included.</p>
    </>
  );
}

function Privacy() {
  return (
    <>
      <H title="Privacy" />
      <Card style={{ padding: 24, background: "var(--ok-soft)", boxShadow: "none" }}>
        <div className="p-row" style={{ alignItems: "flex-start", gap: 14 }}>
          <span className="v-tile" style={{ width: 44, height: 44, borderRadius: 14, background: "#C5F07A" }}><Icon name="shield" size={22} stroke={2.2} /></span>
          <div>
            <h2 className="p-h2">On-device by default</h2>
            <p style={{ margin: "6px 0 14px", color: "var(--tx2)" }}>Your voice is turned into text by a model running inside Chrome on this computer, and typed into the page you're on. Audio goes to a cloud provider only if you add your own key under Cloud and turn it on.</p>
            {["No analytics or tracking", "No account needed", "Works with Wi-Fi off", "Password fields always skipped"].map((t) => <div key={t} className="p-row" style={{ gap: 8, minHeight: 26 }}><span style={{ color: "var(--ok-tx)", display: "flex" }}><Icon name="check" size={14} stroke={2.6} /></span>{t}</div>)}
          </div>
        </div>
      </Card>
      <p style={{ marginTop: 14, display: "flex", gap: 20, flexWrap: "wrap" }}>
        <a className="p-link" href="https://github.com/derpx06/voicio/blob/main/PRIVACY.md" target="_blank" rel="noreferrer">Privacy policy ↗</a>
        <a className="p-link" href="https://github.com/derpx06/voicio" target="_blank" rel="noreferrer">Source code ↗</a>
      </p>
      <p className="p-hint">With cloud off, the only network use is the one-time model download from huggingface.co.</p>
    </>
  );
}

/** What Voicio decided and why: hostnames and outcomes only, never dictated text. Handy when something does not work. */
function Diagnostics() {
  const [rows, setRows] = useState<{ t: string; e: string; d?: Record<string, unknown> }[]>([]);
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    const read = async () => setRows(((await chrome.storage.local.get("diag")).diag as typeof rows | undefined) ?? []);
    void read();
    const fn = (c: Record<string, unknown>, a: string) => { if (a === "local" && c.diag) void read(); };
    chrome.storage.onChanged.addListener(fn as never);
    return () => chrome.storage.onChanged.removeListener(fn as never);
  }, []);
  const text = rows.map((r) => `${r.t}  ${r.e}${r.d ? "  " + JSON.stringify(r.d) : ""}`).join("\n");
  return (
    <>
      <p className="p-sec" style={{ marginTop: 20 }}>Diagnostics</p>
      <Card>
        <div className="p-hint" style={{ marginBottom: 8 }}>What Voicio did on the last dictations. It lists website names and outcomes only, never what you said.</div>
        <pre className="p-mono" style={{ margin: 0, maxHeight: 180, overflow: "auto", fontSize: 11.5, lineHeight: "16px", color: "var(--tx2)", whiteSpace: "pre-wrap" }}>{text || "Nothing yet. Press the shortcut on a page."}</pre>
        <div className="p-row" style={{ gap: 8, marginTop: 10 }}>
          <Button icon={copied ? "check" : "copy"} onClick={() => { void navigator.clipboard.writeText(text); setCopied(true); setTimeout(() => setCopied(false), 1500); }}>{copied ? "Copied" : "Copy log"}</Button>
          <Button kind="ghost" onClick={() => void chrome.storage.local.remove("diag")}>Clear</Button>
        </div>
      </Card>
    </>
  );
}

function About() {
  return (
    <>
      <H title="About" />
      <Card style={{ padding: 22 }}><div className="p-row" style={{ gap: 16 }}><Logo size={64} /><div style={{ flex: 1 }}><b style={{ font: "800 24px/28px var(--head)", letterSpacing: "-.5px" }}>voicio for Chrome</b><div className="p-hint p-mono">Version {VERSION} · Manifest V3</div></div><Chip kind="line">GPL-3.0-or-later</Chip></div></Card>
      <p className="p-sec" style={{ marginTop: 20 }}>Licences</p>
      <Card style={{ padding: 4 }}>
        {[["Voicio", "GPL-3.0-or-later"], ["Moonshine (Useful Sensors)", "MIT"], ["Transformers.js · ONNX Runtime Web", "Apache-2.0 / MIT"], ["Test audio: LibriSpeech", "CC-BY-4.0"]].map(([n, l]) => (
          <div key={n} className="v-row" style={{ justifyContent: "space-between" }}><span>{n}</span><span className="p-mono p-hint">{l}</span></div>))}
      </Card>
      <Diagnostics />
      <p className="p-hint" style={{ marginTop: 12 }}>Speech models (Moonshine) run inside Chrome. The “S1-mini” by “Superwhisper” clean-up and NVIDIA Parakeet models are used only by the Voicio desktop app.</p>
    </>
  );
}
