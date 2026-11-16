// Full-screen first run in the fewest steps: one button asks for the microphone while the recommended pack downloads by itself, then try it.
import { useEffect, useRef, useState } from "react";
import { Button, Chip, Icon, Lattice, Logo } from "@voicio/ui";
import { send, useEverywhere, useSettings, useShortcut, useSnapshot } from "../shared/hooks";
import { NO_MIC_BODY, NO_MIC_TITLE } from "../shared/errors";
import { DEFAULT_ID, byId, formatBytes, subtitleOf, totalBytes } from "../shared/models";
import { ModelPanel } from "../shared/widgets";
import { HEARD_DB, startMeter, useMicProbe } from "../shared/mic";
import "./welcome.css";

const STEPS = ["Welcome", "Microphone", "Try it"] as const;
const PHRASE = "Hi Priya, the invoice is attached. Could you confirm the total by Friday? Thanks!".split(" ");

export function Welcome() {
  const [settings, setSettings] = useSettings(); // also applies the saved theme
  const snap = useSnapshot();
  const keys = useShortcut();
  const [step, setStep] = useState(0);
  const [dir, setDir] = useState<"fwd" | "back">("fwd");
  const [mic, setMic] = useState<"unknown" | "granted" | "denied" | "none">("unknown");
  const [everywhere, setEverywhere] = useEverywhere();
  const main = useRef<HTMLElement>(null);
  const autoStarted = useRef(false);
  const dl = snap.models[DEFAULT_ID];

  useEffect(() => {
    let p: PermissionStatus | undefined;
    void navigator.permissions.query({ name: "microphone" as PermissionName }).then((r) => {
      p = r; const upd = () => setMic(r.state === "granted" ? "granted" : r.state === "denied" ? "denied" : "unknown"); upd(); r.onchange = upd;
    }).catch(() => {});
    return () => { if (p) p.onchange = null; };
  }, []);
  // No choices to make: the recommended pack starts downloading by itself (the page says so), unless one is already there.
  useEffect(() => {
    if (autoStarted.current || !dl) return;
    if (dl.state === "missing" || dl.state === "paused") { autoStarted.current = true; send({ t: "model", op: "install", id: DEFAULT_ID }); }
    else if (dl.state !== "unknown") autoStarted.current = true;
  }, [dl]);

  const go = (n: number) => { setDir(n > step ? "fwd" : "back"); setStep(n); };
  const ask = async () => {
    try { (await navigator.mediaDevices.getUserMedia({ audio: true })).getTracks().forEach((t) => t.stop()); setMic("granted"); go(2); }
    catch (e) { setMic((e as DOMException).name === "NotFoundError" ? "none" : "denied"); go(1); }
  };
  const close = () => { void chrome.tabs.getCurrent().then((t) => (t?.id ? chrome.tabs.remove(t.id) : window.close())).catch(() => window.close()); };
  const primary = () => (step === 0 ? (mic === "granted" ? go(2) : void ask()) : step === 1 ? (mic === "granted" && go(2)) : close());

  // Move focus to the new step's heading so screen readers announce it; Enter continues, Esc goes back.
  useEffect(() => { const h = main.current?.querySelector("h1") as HTMLElement | null; h?.setAttribute("tabindex", "-1"); h?.focus({ preventScroll: true }); }, [step]);
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (e.key === "Escape" && step > 0 && t.tagName !== "SELECT") go(step === 2 && mic === "granted" ? 0 : step - 1);
      else if (e.key === "Enter" && !["TEXTAREA", "SELECT", "BUTTON", "A", "INPUT", "SUMMARY"].includes(t.tagName)) primary();
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  });
  const label = step === 0 ? (mic === "granted" ? "Get started" : "Allow microphone and start") : step === 1 ? "Continue" : "Done";
  const blocked = step === 1 && mic !== "granted";

  return (
    <div className="wl">
      <header className="wl-head">
        <div className="p-row" style={{ gap: 10 }}><Logo size={32} /><span className="p-brand">voicio</span></div>
        <ol className="wl-steps" aria-label="Progress">
          {STEPS.map((s, i) => (
            <li key={s}>
              <button className={`wl-step${i === step ? " on" : i < step ? " done" : ""}`} aria-current={i === step ? "step" : undefined} aria-label={`Step ${i + 1} of ${STEPS.length}: ${s}`}
                disabled={i > step && !(i === 2 && mic === "granted")} onClick={() => go(i)}>
                <span className="wl-dot">{i < step ? <Icon name="check" size={13} stroke={2.6} /> : i + 1}</span>
                <span className="lbl">{s}</span>
              </button>
            </li>
          ))}
        </ol>
        <span className="hide-sm" style={{ width: 120 }} />
      </header>

      <main ref={main} style={{ flex: 1, display: "flex", justifyContent: "center", padding: "4px 24px 20px" }}>
        <div key={step} className={`wl-card wl-page${dir === "back" ? " back" : ""}`}>
          {step === 0 && <Intro />}
          {step === 1 && <MicStep mic={mic} setMic={setMic} micId={settings.micId} setMicId={(micId) => setSettings({ micId })} />}
          {step === 2 && <Ready snap={snap} keys={keys} everywhere={everywhere} setEverywhere={setEverywhere} />}
        </div>
      </main>

      <footer className="p-row wl-foot">
        <Button kind="ghost" onClick={() => go(step === 2 && mic === "granted" ? 0 : step - 1)} disabled={step === 0}>Back</Button>
        <span className="p-hint" id="wl-reason">{blocked ? "Allow the microphone to continue, or skip for now." : "Speech is recognised on your computer. Audio leaves it only if you turn on a cloud provider."}</span>
        <span className="p-row" style={{ gap: 8 }}>
          {step === 1 && mic !== "granted" && <Button kind="ghost" onClick={() => go(2)}>Skip for now</Button>}
          <Button kind="primary" large onClick={primary} disabled={blocked} aria-describedby={blocked ? "wl-reason" : undefined}>{label}</Button>
        </span>
      </footer>
    </div>
  );
}

const up = (i: number) => ({ animationDelay: `${i * 90}ms` });

/* ---------- 1. welcome ---------- */
function Intro() {
  return (
    <>
      <div className="wl-col">
        <div className="wl-up"><h1>Hi. Let's get you <em>talking.</em></h1>
          <p className="lead">Private voice dictation for Chrome. Press a shortcut, say what you mean, and your words are typed into any text box.</p></div>
        <ul className="wl-points">
          <Feature i={1} icon="lock" tint="#FFD8C2" title="Private" body="Speech is recognised inside Chrome. Audio is never uploaded unless you add a cloud provider." />
          <Feature i={2} icon="download" tint="#E6F7C4" title="Works offline" body={`The ${byId(DEFAULT_ID).name} model (${formatBytes(totalBytes(byId(DEFAULT_ID)))}, from huggingface.co) downloads by itself, once.`} />
          <Feature i={3} icon="globe" tint="#D9ECFF" title="Any website" body="Mail, docs, chats, search bars: anywhere you type." />
        </ul>
      </div>
      <div className="wl-panel peach wl-up" style={up(2)}><LiveDemo /></div>
    </>
  );
}
const Feature = ({ i, icon, tint, title, body }: { i: number; icon: "lock" | "download" | "globe"; tint: string; title: string; body: string }) => (
  <li className="wl-up" style={up(i)}><span className="v-tile" style={{ width: 40, height: 40, borderRadius: 13, background: tint }}><Icon name={icon} size={18} stroke={2.2} /></span>
    <span><span className="p-label" style={{ fontSize: 15, display: "block" }}>{title}</span><span className="p-hint" style={{ fontSize: 13.5, lineHeight: "19px" }}>{body}</span></span></li>
);

/** A looping, scripted preview of dictation: speak, words appear live, then the bar says done. */
function LiveDemo() {
  const [n, setN] = useState(0);
  const [phase, setPhase] = useState<"idle" | "listen" | "done">("idle");
  useEffect(() => {
    let t: number; let i = 0; let ph: typeof phase = "idle";
    const tick = () => {
      if (ph === "idle") { ph = "listen"; i = 0; setN(0); setPhase("listen"); t = window.setTimeout(tick, 700); }
      else if (ph === "listen") { i++; setN(i); if (i >= PHRASE.length) { ph = "done"; setPhase("done"); t = window.setTimeout(tick, 2200); } else t = window.setTimeout(tick, 260); }
      else { ph = "idle"; setPhase("idle"); setN(0); t = window.setTimeout(tick, 900); }
    };
    t = window.setTimeout(tick, 600);
    return () => clearTimeout(t);
  }, []);
  const shown = PHRASE.slice(0, n);
  return (
    <div className="wl-demo" aria-hidden>
      <div className="win">
        <div className="tb"><i /><i /><i /><span>mail.example.com</span></div>
        <div className="bd">
          <div className="lb">To: Priya</div>
          <div className="tx">
            {shown.map((w, k) => <span key={k} className={`w${phase === "listen" && k >= n - 3 ? " dim" : ""}`}>{w} </span>)}
            <span className="caret" />
          </div>
        </div>
      </div>
      <div className={`wl-pill${phase === "idle" ? " idle" : ""}`}>
        {phase === "done"
          ? <><Lattice mode="done" /><span><b>Typed</b> · {PHRASE.length} words</span></>
          : <><Lattice mode={phase === "listen" && n > 0 ? "listening" : "idle"} level={phase === "listen" && n > 0 ? 0.55 + 0.4 * Math.sin(n * 1.7) ** 2 : 0} /><span>{shown.slice(-4).join(" ")}</span></>}
      </div>
    </div>
  );
}
/* ---------- download status (no choices: the recommended pack) ---------- */
function useOnline() {
  const [on, setOn] = useState(navigator.onLine);
  useEffect(() => {
    const f = () => setOn(navigator.onLine);
    window.addEventListener("online", f); window.addEventListener("offline", f);
    return () => { window.removeEventListener("online", f); window.removeEventListener("offline", f); };
  }, []);
  return on;
}

function PackStatus({ snap }: { snap: ReturnType<typeof useSnapshot> }) {
  const online = useOnline();
  const st = snap.models[DEFAULT_ID];
  const retry = () => send({ t: "model", op: "install", id: DEFAULT_ID });
  // Still at 0 bytes after 15 s of "downloading": the network is almost certainly blocking huggingface.co.
  const [stalled, setStalled] = useState(false);
  useEffect(() => {
    setStalled(false);
    if (st?.state !== "downloading" || st.bytes > 0) return;
    const t = setTimeout(() => setStalled(true), 15000);
    return () => clearTimeout(t);
  }, [st?.state, st?.bytes]);
  const failed = st?.state === "failed";
  const trouble = failed || stalled || (!online && st?.state !== "installed");
  const wasOffline = useRef(false);
  useEffect(() => {
    if (!online) wasOffline.current = true;
    else if (wasOffline.current && failed) { wasOffline.current = false; retry(); }
  }, [online]); // eslint-disable-line react-hooks/exhaustive-deps
  return (
    <>
      <div className="wl-model wl-up" style={{ cursor: "default" }}>
        <div className="p-label">{byId(DEFAULT_ID).name}</div>
        <div className="p-hint">{subtitleOf(byId(DEFAULT_ID))} · works offline</div>
        <div style={{ marginTop: 10 }}>{st?.state === "installed" ? <Chip kind="ok" icon="check">Ready. Works offline.</Chip> : <ModelPanel snap={snap} id={DEFAULT_ID} compact />}</div>
      </div>
      {trouble && (
        <div className="wl-model warn wl-up" role="alert" style={{ cursor: "default" }}>
          <div className="p-row" style={{ alignItems: "flex-start", gap: 10 }}>
            <Icon name="alertCircle" size={18} style={{ color: "var(--warn)", marginTop: 2 }} />
            <div style={{ flex: 1 }}>
              <div className="p-label">{!online ? "You're offline" : stalled ? "The download can't start. The network may be blocking huggingface.co." : "Couldn't reach huggingface.co"}</div>
              <div className="p-hint" style={{ color: "var(--tx2)", marginTop: 4 }}>
                {online ? "A school or work network, a VPN or an ad blocker can block it. Try another network, then press Retry; it resumes where it stopped." : "It retries by itself when you're back online."}
                {failed && st?.error ? <span className="p-mono" style={{ display: "block", marginTop: 4 }}>{st.error.slice(0, 140)}</span> : null}
              </div>
              <div style={{ marginTop: 10 }}><Button kind="primary" icon="download" disabled={!online} onClick={retry}>Retry</Button></div>
            </div>
          </div>
        </div>
      )}
    </>
  );
}

/* ---------- 3. microphone test ---------- */
function MicStep({ mic, setMic, micId, setMicId }: { mic: "unknown" | "granted" | "denied" | "none"; setMic: (m: "unknown" | "granted" | "denied" | "none") => void; micId: string; setMicId: (id: string) => void }) {
  const bars = useRef<HTMLElement[]>([]);
  const [heard, setHeard] = useState(false);
  const [quiet, setQuiet] = useState(false); // nothing but silence for a few seconds
  const [fail, setFail] = useState("");
  const [label, setLabel] = useState("");
  const probe = useMicProbe();
  useEffect(() => { if (probe.ready && probe.permission === "granted" && probe.devices.length === 0) setMic("none"); }, [probe.ready, probe.permission, probe.devices.length, setMic]);

  const ask = async () => {
    try { (await navigator.mediaDevices.getUserMedia({ audio: true })).getTracks().forEach((t) => t.stop()); setMic("granted"); }
    catch (e) { setMic((e as DOMException).name === "NotFoundError" ? "none" : "denied"); }
  };

  // Once allowed, show the live level of the chosen microphone so the user can see that it works.
  useEffect(() => {
    if (mic !== "granted") return;
    let off = false, meter: { stop: () => void } | undefined, hits = 0;
    setHeard(false); setQuiet(false); setFail("");
    const hist = Array(bars.current.length).fill(0);
    const t0 = performance.now();
    void startMeter(micId, (v, db) => {
      hist.shift(); hist.push(v);
      bars.current.forEach((b, i) => { if (b) b.style.transform = `scaleY(${Math.max(0.08, hist[i] ?? 0)})`; });
      if (db > HEARD_DB && ++hits === 8) { setHeard(true); setQuiet(false); }
      if (hits < 8 && performance.now() - t0 > 4000) setQuiet(true);
    }).then((m) => { if (off) m.stop(); else { meter = m; setLabel(m.label); probe.refresh(); } })
      .catch((e: DOMException) => setFail(e.name === "NotReadableError" ? "Another app is using the microphone. Close it and press Check again." : e.name === "OverconstrainedError" || e.name === "NotFoundError" ? "That microphone isn't available." : "The microphone didn't open."));
    return () => { off = true; meter?.stop(); };
  }, [mic, micId]); // eslint-disable-line react-hooks/exhaustive-deps

  const known = micId === "" || probe.devices.some((d) => d.id === micId);
  return (
    <>
      <div className="wl-col">
      <div className="wl-up"><p className="p-sec" style={{ margin: "0 0 10px" }}>Step 2 · Microphone</p><h1>Allow the <em>microphone</em></h1>
        <p className="lead">Chrome asks once. Voicio only listens while you are dictating.</p></div>
      </div>
      <div className="wl-panel lime wl-up" style={{ ...up(1), flexDirection: "column", gap: 16 }}>
        <button className={`wl-micbtn${mic === "granted" ? " ok" : ""}`} onClick={() => mic !== "granted" && void ask()} aria-label={mic === "granted" ? "Microphone allowed" : "Allow microphone"}>
          <Icon name={mic === "granted" ? "check" : "mic"} size={40} stroke={mic === "granted" ? 2.4 : undefined} />
        </button>
        {mic === "granted" ? (
          <>
            <div className="wl-levels" aria-hidden>{Array.from({ length: 28 }, (_, i) => <i key={i} ref={(el) => { if (el) bars.current[i] = el; }} />)}</div>
            <div style={{ minHeight: 28, textAlign: "center" }} role="status">
              {fail ? <span className="p-hint" style={{ color: "var(--warn)" }}>{fail}</span>
                : heard ? <span className="wl-ok" style={{ display: "inline-block" }}><Chip kind="ok" icon="check">I can hear you</Chip></span>
                : quiet ? <span className="p-hint" style={{ color: "var(--warn)" }}>No sound from {label || "this microphone"}. It may be muted or the wrong input. Choose another below.</span>
                : <span className="p-hint">Say something. The bars should move.</span>}
            </div>
            <div style={{ width: "min(420px, 100%)", display: "grid", gap: 6 }}>
              <label className="p-hint" htmlFor="wl-mic">Microphone</label>
              <select id="wl-mic" className="p-input" value={known ? micId : ""} onChange={(e) => setMicId(e.target.value)}>
                <option value="">System default{label && micId === "" ? ` (${label})` : ""}</option>
                {probe.devices.filter((d) => d.id !== "default" && d.id !== "communications").map((d) => <option key={d.id} value={d.id}>{d.label}</option>)}
              </select>
              {!known && <span className="p-hint" style={{ color: "var(--warn)" }}>The microphone you chose isn't connected. Using the system default.</span>}
            </div>
          </>
        ) : mic === "none" ? <><Chip kind="warn">{NO_MIC_TITLE}</Chip><div className="p-hint" style={{ textAlign: "center", maxWidth: 440 }}>{NO_MIC_BODY}</div><div className="p-row" style={{ gap: 8 }}><Button kind="primary" onClick={() => void send({ t: "open", page: "restart" })}>Restart Chrome</Button><Button onClick={() => { probe.refresh(); void ask(); }}>Check again</Button></div></>
          : mic === "denied" ? <><div className="p-hint" style={{ textAlign: "center", maxWidth: 420 }}>Chrome has the microphone blocked for Voicio. One change in Chrome's settings turns it back on, and this page continues by itself.</div><Button kind="primary" onClick={() => void send({ t: "open", page: "permission-blocked" })}>Fix microphone access</Button></>
          : <span className="p-hint">Press the button to allow.</span>}
      </div>
    </>
  );
}

/* ---------- 3. ready: shortcut once, try it, advanced tucked away ---------- */
function Ready({ snap, keys, everywhere, setEverywhere }: { snap: ReturnType<typeof useSnapshot>; keys: string[]; everywhere: boolean; setEverywhere: (v: boolean) => Promise<void> }) {
  const [down, setDown] = useState<Record<string, boolean>>({});
  useEffect(() => {
    const m = (e: KeyboardEvent, v: boolean) => setDown((d) => ({ ...d, [e.key.length === 1 ? e.key.toUpperCase() : e.key]: v }));
    const dn = (e: KeyboardEvent) => m(e, true), upf = (e: KeyboardEvent) => m(e, false);
    window.addEventListener("keydown", dn); window.addEventListener("keyup", upf);
    return () => { window.removeEventListener("keydown", dn); window.removeEventListener("keyup", upf); };
  }, []);
  return (
    <>
      <div className="wl-col">
      <div className="wl-up"><p className="p-sec" style={{ margin: "0 0 10px" }}>All set</p><h1>Click a text box, <em>press the shortcut, talk.</em></h1>
        <p className="lead">Press it again to finish. Esc cancels.</p></div>
      <PackStatus snap={snap} />
      <div className="wl-up" style={up(2)}>
        <label className="p-label" htmlFor="wl-try">Try it here</label>
        <textarea id="wl-try" rows={3} placeholder="Click here, press the shortcut and say a sentence." dir="auto"
          className="p-input" style={{ width: "100%", height: "auto", marginTop: 6, padding: 12, font: "400 15px var(--font)", resize: "vertical" }} />
      </div>
      <details className="wl-up" style={up(3)}>
        <summary className="p-hint" style={{ cursor: "pointer" }}>Advanced options</summary>
        <div style={{ display: "grid", gap: 12, marginTop: 10 }}>
          <div className="wl-how" style={{ justifyContent: "space-between", alignItems: "center", flexWrap: "wrap" }}>
            <div style={{ flex: "1 1 300px" }}>
              <div className="p-label">{everywhere ? "Bar shown on every website" : "Show the bar on every website"}</div>
              <div className="p-hint" style={{ color: "var(--tx2)" }}>Without it the shortcut still works on the page you are on.</div>
            </div>
            {everywhere ? <Chip kind="ok" icon="check">On</Chip> : <Button onClick={() => void setEverywhere(true)}>Allow on all websites</Button>}
          </div>
          <p className="p-hint" style={{ margin: 0 }}>Other speech models, your own cloud provider (off by default; it sends audio to the provider you pick), word lists and per-site rules are in <button className="p-link" onClick={() => void send({ t: "open", page: "options" })}>Settings</button>.</p>
        </div>
      </details>
      </div>
      <div className="wl-panel sky wl-up" style={{ ...up(1), flexDirection: "column", gap: 18 }}>
        <span className="wl-keys" aria-label={`Shortcut: ${keys.join(" plus ")}`}>{keys.map((k, i) => <span key={k} style={{ display: "contents" }}>{i > 0 && <span className="plus">+</span>}<span className={`wl-key${down[k.length === 1 ? k.toUpperCase() : k] ? " down" : ""}`}>{k}</span></span>)}</span>
        <button className="p-link" onClick={() => void send({ t: "open", page: "shortcuts" })}>Change shortcut</button>
      </div>
    </>
  );
}
