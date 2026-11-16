import { useEffect, useState } from "react";
import { siteRule, type Settings, type Snapshot } from "@voicio/core";
import { Button, Card, Chip, EmptyState, Icon, IconButton, Keycaps, Logo, Segmented, Switch } from "@voicio/ui";
import { recentHosts } from "../shared/storage";
import { send, useEverywhere, useSettings, useShortcut, useSnapshot } from "../shared/hooks";
import { byId, formatBytes, totalBytes } from "../shared/models";
import { ErrorNotice, MicStage, ModelPanel, StatusChip, Transcript, TranscriptActions } from "../shared/widgets";
import { useMicProbe } from "../shared/mic";
import { restrictedPage } from "../shared/page";
import { NO_MIC_BODY, NO_MIC_TITLE } from "../shared/errors";

type Tab = "dictate" | "site" | "settings";
type Mic = "granted" | "prompt" | "denied" | "none" | "unknown";

/** The popup's view of the microphone: granted with no input device counts as "none". */
function useMic(): [Mic, () => void] {
  const p = useMicProbe();
  const mic: Mic = p.permission === "granted" ? (p.devices.length ? "granted" : p.ready ? "none" : "unknown") : p.permission;
  return [mic, p.refresh];
}

export function Popup() {
  useEffect(() => { const port = chrome.runtime.connect({ name: "voicio-popup" }); return () => port.disconnect(); }, []); // lets the background know the popup is open
  const snap = useSnapshot();
  const [settings, set] = useSettings();
  const keys = useShortcut();
  const [tab, setTab] = useState<Tab>("dictate");
  const [mic, recheck] = useMic();
  const modelOk = snap.model.state === "installed";
  const modelKnown = snap.model.state !== "unknown";

  let body;
  if (modelKnown && !modelOk) body = <NoModel snap={snap} mic={mic} />;
  else if (mic === "denied") body = <Blocked />;
  else if (mic === "none") body = <NoMic recheck={recheck} />;
  else if (mic === "prompt") body = <FirstRun />;
  else body = (
    <>
      <Segmented label="Popup section" block value={tab} onChange={setTab} options={[{ value: "dictate", label: "Dictate" }, { value: "site", label: "This site" }, { value: "settings", label: "Settings" }]} />
      <div style={{ flex: 1, minHeight: 0, marginTop: 12, display: "flex", flexDirection: "column" }}>
        {tab === "dictate" && <DictateTab snap={snap} keys={keys} settings={settings} set={set} />}
        {tab === "site" && <SiteTab settings={settings} set={set} />}
        {tab === "settings" && <SettingsTab settings={settings} set={set} keys={keys} />}
      </div>
    </>
  );

  return (
    <div style={{ width: 360, minHeight: 440, maxHeight: 600, display: "flex", flexDirection: "column", background: "var(--bg)" }}>
      <header className="p-row" style={{ height: 58, padding: "0 12px 0 16px", justifyContent: "space-between", flex: "none" }}>
        <div className="p-row" style={{ gap: 9 }}><Logo size={28} /><span className="p-brand" style={{ fontSize: 21 }}>voicio</span></div>
        <div className="p-row" style={{ gap: 6 }}>
          <StatusChip snap={snap} />
          <IconButton icon="settings" label="Open all settings" onClick={() => void chrome.runtime.openOptionsPage()} />
        </div>
      </header>
      <main style={{ flex: 1, minHeight: 0, overflowY: "auto", padding: "4px 16px 12px", display: "flex", flexDirection: "column" }}>{body}</main>
      <footer className="p-row" style={{ height: 38, padding: "0 16px", gap: 6, justifyContent: "center", flex: "none", color: "var(--tx3)" }}>
        <Icon name="lock" size={12} /><span className="p-hint">{snap.cloud ? "Cloud speech: this dictation is sent to your provider." : "On-device speech."}</span>
        {!snap.cloud && <button className="p-link" style={{ fontSize: 12.5 }} onClick={openCloud}>Use a cloud provider</button>}
      </footer>
    </div>
  );
}

/** The cloud manager lives in Settings; the popup only links to it. */
const openCloud = () => void chrome.tabs.create({ url: chrome.runtime.getURL("src/options/index.html#cloud") });

function DictateTab({ snap, keys, settings, set }: { snap: Snapshot; keys: string[]; settings: Settings; set: (p: Partial<Settings>) => void }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10, flex: 1, minHeight: 0 }}>
      {snap.rec === "error" ? <ErrorNotice snap={snap} /> : <><ReachNotice snap={snap} /><PageCheck snap={snap} /></>}
      <div style={{ flex: "none" }}><MicStage snap={snap} keys={keys} size={52} compact /></div>
      <Transcript snap={snap} fill />
      <TranscriptActions snap={snap} />
    </div>
  );
}

/** Shown whenever the popup is opened on a page extensions cannot use (even before dictating). */
function PageCheck({ snap }: { snap: Snapshot }) {
  const [restricted, setRestricted] = useState(false);
  useEffect(() => { void chrome.tabs.query({ active: true, currentWindow: true }).then(([t]) => setRestricted(restrictedPage(t?.url))).catch(() => {}); }, []);
  return restricted && snap.reach !== "restricted" ? <RestrictedNotice /> : null;
}

/** Chrome keeps extensions off its own pages. Say so before the user wonders why nothing can be typed. */
function RestrictedNotice() {
  return (
    <Card style={{ flex: "none" }}>
      <div role="status" className="p-row" style={{ alignItems: "flex-start" }}>
        <Icon name="info" size={16} style={{ color: "var(--sky-tx)", marginTop: 2 }} />
        <div><div className="p-label">Extensions are blocked on this page</div>
          <div className="p-hint" style={{ color: "var(--tx2)" }}>Chrome keeps them off its own pages (New Tab, chrome://). Dictate and use Copy, or open a website to type into it.</div></div>
      </div>
    </Card>
  );
}

/** The bar could not be shown on the page, so dictation ran here. One click to make it work everywhere. */
function ReachNotice({ snap }: { snap: Snapshot }) {
  const [everywhere, setEverywhere] = useEverywhere();
  if (snap.reach === "restricted") return <RestrictedNotice />;
  if (!snap.reach || everywhere) return null;
  return (
    <Card style={{ background: "var(--warn-soft)" }}>
      <div role="status">
        <div className="p-label">Voicio couldn't show its bar on this page</div>
        <div className="p-hint" style={{ color: "var(--tx2)", margin: "2px 0 8px" }}>Allow it on all websites and the bar appears wherever you press the shortcut. Nothing is sent anywhere.</div>
        <Button kind="primary" onClick={() => void setEverywhere(true)}>Allow on all websites</Button>
        <div className="p-hint p-mono" style={{ marginTop: 8, wordBreak: "break-word" }}>{snap.reach}</div>
      </div>
    </Card>
  );
}

function SiteTab({ settings, set }: { settings: Settings; set: (p: Partial<Settings>) => void }) {
  const [host, setHost] = useState("");
  const [recent, setRecent] = useState<string[]>([]);
  useEffect(() => {
    void chrome.tabs.query({ active: true, currentWindow: true }).then(([t]) => { try { setHost(new URL(t?.url ?? "").hostname); } catch { /* not a web page */ } });
    void recentHosts().then(setRecent);
  }, []);
  const rule = host ? siteRule(settings, host) : undefined;
  const put = (r: "allow" | "never" | undefined) => set({ sites: [...settings.sites.filter((s) => s.host !== host), ...(r ? [{ host, rule: r }] : [])] });
  const allowHere = async () => {
    if (await chrome.permissions.request({ origins: [`*://${host}/*`] })) put("allow");
  };
  if (!host) return <EmptyState icon="globe" title="Not a web page" body="Open a website to set how Voicio behaves there." />;
  return (
    <div className="p-scroll" style={{ display: "flex", flexDirection: "column", gap: 10, overflowY: "auto" }}>
      <Card>
        <div className="p-row" style={{ justifyContent: "space-between" }}>
          <div><div className="p-mono" style={{ fontSize: 13 }}>{host}</div>
            <div className="p-hint">{rule === "never" ? "Voicio is off here" : rule === "allow" ? "Mic button shown in fields" : "Works when you press the shortcut"}</div></div>
          <Switch label={`Voicio on ${host}`} checked={rule !== "never"} onChange={(v) => put(v ? undefined : "never")} />
        </div>
        <div className="p-row" style={{ marginTop: 10, gap: 8, flexWrap: "wrap" }}>
          {rule !== "allow" && rule !== "never" && <Button icon="mic" onClick={() => void allowHere()}>Show mic in fields</Button>}
          {rule === "never" ? <Button onClick={() => put(undefined)}>Allow on this site</Button> : <Button kind="danger" onClick={() => put("never")}>Never on this site</Button>}
        </div>
      </Card>
      <div className="p-row" style={{ padding: "2px 4px", gap: 10 }}>
        <Icon name="lock" size={14} style={{ color: "var(--tx3)" }} />
        <div style={{ flex: 1 }}><div className="p-label">Password fields</div><div className="p-hint">Always off. Can't be changed.</div></div>
      </div>
      {recent.length > 0 && <div><p className="p-sec">Recent sites</p>
        {recent.slice(0, 5).map((h) => { const r = siteRule(settings, h); return (
          <div key={h} className="v-row" style={{ minHeight: 34 }}><Icon name="globe" size={14} style={{ color: "var(--tx3)" }} /><span className="p-mono" style={{ flex: 1, fontSize: 12.5 }}>{h}</span>
            <Chip kind={r === "never" ? "err" : "ok"}>{r === "never" ? "Never" : "On"}</Chip></div>); })}</div>}
    </div>
  );
}

function SettingsTab({ settings, set, keys }: { settings: Settings; set: (p: Partial<Settings>) => void; keys: string[] }) {
  return (
    <div className="p-scroll" style={{ display: "flex", flexDirection: "column", gap: 12, overflowY: "auto" }}>
      <div className="p-row" style={{ justifyContent: "space-between" }}>
        <div><div className="p-label">Shortcut</div><button className="p-link" onClick={() => void send({ t: "open", page: "shortcuts" })}>Change in Chrome</button></div>
        <Keycaps keys={keys} />
      </div>
      <label className="p-row" style={{ justifyContent: "space-between" }}>
        <span className="p-label">Dictate into the focused field</span>
        <Switch label="Dictate into the focused field" checked={settings.intoFocusedField} onChange={(v) => set({ intoFocusedField: v })} />
      </label>
      <Field label="Mode"><Segmented label="Mode" block value={settings.mode} onChange={(mode) => set({ mode })} options={[{ value: "toggle", label: "Tap to toggle" }, { value: "hold", label: "Hold to talk" }]} /></Field>
      <Field label="Insert"><Segmented label="Insert as" block value={settings.insertAs} onChange={(insertAs) => set({ insertAs })} options={[{ value: "type", label: "Type" }, { value: "paste", label: "Paste" }]} /></Field>
      <Field label="Auto-stop after silence"><Segmented label="Auto-stop" block value={String(settings.autostopSecs)} onChange={(v) => set({ autostopSecs: Number(v) as 0 | 3 | 10 })} options={[{ value: "0", label: "Off" }, { value: "3", label: "3 s" }, { value: "10", label: "10 s" }]} /></Field>
      <Field label="Appearance"><Segmented label="Appearance" block value={settings.theme} onChange={(theme) => set({ theme })} options={[{ value: "system", label: "System" }, { value: "light", label: "Light" }, { value: "dark", label: "Dark" }]} /></Field>
      <Field label="Cloud speech (optional)"><Button icon="globe" onClick={openCloud}>Use a cloud provider</Button></Field>
      <Button icon="ext" onClick={() => void chrome.runtime.openOptionsPage()}>Advanced settings</Button>
    </div>
  );
}
const Field = ({ label, children }: { label: string; children: React.ReactNode }) => (<div style={{ display: "flex", flexDirection: "column", gap: 6 }}><span className="p-label">{label}</span>{children}</div>);

/* ---------- other states ---------- */
function NoModel({ snap, mic }: { snap: Snapshot; mic: Mic }) {
  void mic;
  const busy = snap.model.state === "downloading" || snap.model.state === "paused";
  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 14 }}>
      <EmptyState icon="download" title={busy ? "Downloading the speech model" : "Download the speech model"}
        body={`Voicio recognises speech inside Chrome. ${byId(snap.activeModel).name} is ${formatBytes(totalBytes(byId(snap.activeModel)))} and works offline once saved.`} />
      <ModelPanel snap={snap} compact />
      <button className="p-link" onClick={() => void send({ t: "open", page: "options" })}>What gets downloaded?</button>
    </div>
  );
}
function NoMic({ recheck }: { recheck: () => void }) {
  return <div style={{ flex: 1, display: "grid", placeItems: "center" }}><EmptyState icon="micOff" title={NO_MIC_TITLE} body={NO_MIC_BODY}><div className="p-row" style={{ gap: 8, justifyContent: "center" }}><Button kind="primary" onClick={() => void send({ t: "open", page: "restart" })}>Restart Chrome</Button><Button onClick={recheck}>Check again</Button></div></EmptyState></div>;
}
function Blocked() {
  return <div style={{ flex: 1, display: "grid", placeItems: "center" }}><EmptyState icon="micOff" title="Microphone blocked" body="Chrome isn't letting Voicio use the microphone. It takes a few seconds to turn back on."><Button kind="primary" onClick={() => void send({ t: "open", page: "permission-blocked" })}>Show me how</Button></EmptyState></div>;
}
function FirstRun() {
  const steps = [["Download the speech model", "done"], ["Allow the microphone", "active"], ["Try it", "todo"]] as const;
  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", justifyContent: "center", gap: 12 }}>
      <div><div className="p-hint">Step 2 of 3</div><h2 className="p-h2" style={{ marginTop: 2 }}>Almost ready</h2></div>
      {steps.map(([t, s], i) => (
        <Card key={t} accent={s === "active"} style={{ opacity: s === "todo" ? 0.6 : 1 }}>
          <div className="p-row"><span style={{ width: 22, height: 22, borderRadius: "50%", display: "grid", placeItems: "center", background: s === "done" ? "var(--ink)" : s === "active" ? "var(--ac)" : "var(--line2)", color: s === "done" ? "var(--on-ink)" : s === "active" ? "#16130F" : "var(--tx3)", font: "800 12px var(--font)" }}>{s === "done" ? <Icon name="check" size={12} stroke={2.5} /> : i + 1}</span>
            <span className="p-label" style={{ flex: 1 }}>{t}</span></div>
          {s === "active" && <Button kind="primary" large block style={{ marginTop: 10 }} onClick={() => void send({ t: "open", page: "permission" })}>Allow microphone</Button>}
        </Card>
      ))}
    </div>
  );
}
