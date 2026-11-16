// The microphone permission page. It asks for the permission itself when Chrome allows asking, and when Chrome has
// it blocked it opens Voicio's own setting, then notices the change by itself (no reload, no "I've allowed it").
import { useEffect, useRef, useState } from "react";
import { Button, Card, Chip, Icon, Logo } from "@voicio/ui";
import { send, useSettings } from "../shared/hooks";
import { micSettingsUrl, useMicProbe } from "../shared/mic";

export function Permission() {
  useSettings(); // applies the saved theme
  const probe = useMicProbe();
  const [msg, setMsg] = useState("");
  const [busy, setBusy] = useState(false);
  const [asked, setAsked] = useState(false);
  const told = useRef(false);
  const state = probe.permission;

  const ask = async () => {
    setBusy(true); setMsg("");
    try {
      (await navigator.mediaDevices.getUserMedia({ audio: true })).getTracks().forEach((t) => t.stop());
    } catch (e) {
      const n = (e as DOMException).name;
      if (n === "NotFoundError" || n === "OverconstrainedError") setMsg("Chrome says there is no microphone. Plug one in, then press Allow microphone again.");
      else if (n === "NotReadableError") setMsg("Another app is using the microphone. Close it and try again.");
      else if (n === "NotAllowedError") setMsg("The prompt was closed or blocked. Choose Allow next time, or use the steps below.");
    } finally { setBusy(false); probe.refresh(); }
  };

  // Chrome shows its prompt for this page right away. If it was dismissed, the buttons stay.
  useEffect(() => { if (state === "prompt" && !asked) { setAsked(true); void ask(); } }, [state]); // eslint-disable-line react-hooks/exhaustive-deps
  // Permission changes are normally pushed; poll as well, because a setting changed in another tab can be missed.
  useEffect(() => { const t = setInterval(probe.refresh, 1500); return () => clearInterval(t); }, [probe.refresh]);
  useEffect(() => { if (state === "granted" && !told.current) { told.current = true; void send({ t: "mic", op: "ask-done" }); const c = setTimeout(() => window.close(), 3500); return () => clearTimeout(c); } }, [state]);

  const noDevice = state === "granted" && probe.ready && probe.devices.length === 0;
  const view: "ok" | "blocked" | "ask" = state === "granted" ? "ok" : state === "denied" ? "blocked" : "ask";

  return (
    <div style={{ minHeight: "100vh", display: "grid", placeItems: "center", padding: 24 }}>
      <div style={{ width: "min(880px, 100%)", display: "grid", gridTemplateColumns: view === "blocked" ? "minmax(0,1fr) minmax(0,1fr)" : "1fr", gap: 40, alignItems: "center" }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 16, maxWidth: 460, margin: view === "blocked" ? 0 : "0 auto" }}>
          <div className="p-row" style={{ gap: 8 }}><Logo size={30} /><span className="p-brand">voicio</span></div>

          {view === "ask" && <>
            <div style={{ width: 72, height: 72, borderRadius: "50%", display: "grid", placeItems: "center", background: "radial-gradient(circle at 34% 28%,#FF9B6E 0%,#F2541B 55%,#D8410E 100%)", color: "#fff", boxShadow: "0 10px 24px rgba(242,84,27,.28)" }}><Icon name="mic" size={32} /></div>
            <h1 style={{ margin: 0, font: "800 36px/40px var(--head)", letterSpacing: "-1px" }}>Allow the microphone</h1>
            <p style={{ margin: 0, color: "var(--tx2)" }}>Chrome is asking at the top of this tab. Choose <b>Allow while visiting the site</b> or <b>Allow this time</b> so Voicio can hear you.</p>
            {[["cpu", "Processed on this computer"], ["power", "The mic is only on while you're dictating"], ["settings", "You can turn it off any time"]].map(([i, t]) => (
              <div key={t} className="p-row"><span className="v-tile" style={{ background: i === "cpu" ? "#E6F7C4" : i === "power" ? "#FFD8C2" : "#D9ECFF" }}><Icon name={i as "cpu"} size={16} stroke={2.2} /></span><span className="p-label">{t}</span></div>))}
            <div className="p-row" style={{ gap: 8 }}><Button kind="primary" large loading={busy} onClick={() => void ask()}>Allow microphone</Button><Button kind="ghost" large onClick={() => window.close()}>Not now</Button></div>
          </>}

          {view === "blocked" && <>
            <Chip kind="warn" icon="micOff">Microphone blocked</Chip>
            <h1 style={{ margin: 0, font: "800 36px/40px var(--head)", letterSpacing: "-1px" }}>Let's turn the mic back on</h1>
            <p style={{ margin: 0, color: "var(--tx2)" }}>Chrome blocked the microphone for Voicio, usually because the prompt was closed a few times. Chrome won't ask again by itself, so it takes one change in its settings.</p>
            <ol style={{ margin: 0, padding: 0, listStyle: "none", display: "grid", gap: 12 }}>
              <Step n={1} text="Open Voicio's microphone setting."><Button kind="primary" icon="ext" onClick={() => void chrome.tabs.create({ url: micSettingsUrl() })}>Open the setting</Button></Step>
              <Step n={2} text="Next to Microphone, choose Allow (or Ask)." />
              <Step n={3} text="Come back to this tab. It continues by itself." />
            </ol>
            <p className="p-hint" style={{ margin: 0 }}>Prefer the address bar? Click the icon to the left of this tab's address, open Site settings, and set Microphone to Allow.</p>
          </>}

          {view === "ok" && <>
            <div style={{ width: 72, height: 72, borderRadius: "50%", display: "grid", placeItems: "center", background: "var(--ok-soft)", color: "var(--ok-tx)" }}><Icon name="circleCheck" size={36} /></div>
            <h1 style={{ margin: 0, font: "800 36px/40px var(--head)", letterSpacing: "-1px" }}>{noDevice ? "Allowed, but no microphone found" : "You're all set"}</h1>
            <p style={{ margin: 0, color: "var(--tx2)" }}>{noDevice ? "Chrome can't see a microphone. Plug one in or check your system sound settings." : "Go back to your page, click a text field, and press Alt+Shift+V. This tab closes by itself."}</p>
            <div><Button kind="primary" large onClick={() => window.close()}>Close</Button></div>
          </>}

          {msg && view !== "blocked" && <span role="status" className="p-hint" style={{ color: "var(--warn)" }}>{msg}</span>}
        </div>

        {view === "blocked" && (
          <Card style={{ padding: 22, background: "var(--sky)" }} >
            <div className="p-hint" style={{ marginBottom: 10 }}>What you'll see in Chrome</div>
            <div style={{ borderRadius: 18, overflow: "hidden", background: "var(--surf2)" }}>
              <div className="p-mono" style={{ padding: "10px 14px", fontSize: 12, color: "var(--tx3)", borderBottom: "1px solid var(--line2)" }}>chrome://settings/content/siteDetails</div>
              <div className="p-row" style={{ padding: "14px", justifyContent: "space-between" }}>
                <span className="p-row" style={{ gap: 8 }}><Icon name="mic" size={16} />Microphone</span>
                <span style={{ borderRadius: 10, padding: "6px 12px", font: "700 13px var(--font)", background: "var(--surf)", boxShadow: "inset 0 0 0 2px var(--ink)" }}>Allow ▾</span>
              </div>
              <div className="p-hint" style={{ padding: "0 14px 14px" }}>Set this to Allow or Ask. Voicio notices the change right away.</div>
            </div>
          </Card>
        )}
      </div>
    </div>
  );
}

const Step = ({ n, text, children }: { n: number; text: string; children?: React.ReactNode }) => (
  <li className="p-row" style={{ alignItems: "flex-start", gap: 12 }}>
    <span style={{ width: 24, height: 24, borderRadius: "50%", flex: "none", display: "grid", placeItems: "center", background: "var(--ink)", color: "var(--on-ink)", font: "800 12px var(--font)" }}>{n}</span>
    <div style={{ paddingTop: 2, display: "grid", gap: 8 }}><span>{text}</span>{children}</div>
  </li>
);
