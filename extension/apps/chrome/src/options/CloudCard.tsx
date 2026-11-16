import { useEffect, useState } from "react";
import { CLOUD, CLOUD_ERROR_TEXT, CLOUD_EXCLUDED, cloudById, hostFor, originsFor, testKey, type CloudConfig, type CloudMode, type CloudProvider } from "@voicio/core";
import { Button, Card, Chip, Icon, Segmented, Switch } from "@voicio/ui";
import { getCloud, setCloud } from "../shared/storage";

const mask = (k: string) => `${"•".repeat(10)}${k.slice(-4)}`;
/** Registry extras the core type does not carry (yet): `featured` picks the first list, `order` sorts it. */
const meta = (p: CloudProvider) => p.spec as unknown as { featured?: boolean; order?: number };
const sorted = (ps: CloudProvider[]) => ps.map((p, i) => ({ p, i })).sort((a, b) => (meta(a.p).order ?? a.i) - (meta(b.p).order ?? b.i)).map((x) => x.p);
const FEATURED = sorted(CLOUD.filter((p) => meta(p).featured));
const MORE = sorted(CLOUD.filter((p) => !meta(p).featured));
const TINTS = ["#FFD8C2", "#D9ECFF", "#E6F7C4", "#FFE7A8", "#FFC9B0", "#C9E2FF", "#EFE9DD", "#D8F2C2"];
/** Provider first: the company is the title ("Google Gemini" for Google's entry, "Lemonfox (EU)" for a second entry by one maker). */
const titleOf = (p: CloudProvider) => {
  if (p.maker === "Google") return "Google Gemini";
  const twin = CLOUD.find((o) => o.maker === p.maker && o.id !== p.id);
  const tag = twin && /\(([^)]+)\)/.exec(p.name)?.[1];
  return tag ? `${p.maker} (${tag})` : p.maker;
};
/** The price note's first clause: "About $0.04 per hour (turbo), 10 s minimum" -> "About $0.04 per hour". */
const shortPrice = (n: string) => n.split(/[;(]/)[0]!.replace(/,\s*$/, "").trim();
const modelOf = (c: CloudConfig, p: CloudProvider) => (c.models[p.id] && p.models.includes(c.models[p.id]!) ? c.models[p.id]! : p.defaultModel);
/** Logo tile text: "Microsoft Azure" -> "MA", "Groq" -> "Gr". */
const initials = (maker: string) => { const w = maker.split(/\s+/); return w.length > 1 ? `${w[0]![0]}${w[1]![0]}`.toUpperCase() : `${maker[0]!.toUpperCase()}${maker[1] ?? ""}`; };

/** Optional bring-your-own-key cloud speech, provider first (providers from the shared registry). Off by default; each
 *  provider needs its own consent (which asks Chrome for that one host), a key that passes a test, and then can be the default. */
export function CloudCard() {
  const [cfg, setCfg] = useState<CloudConfig | null>(null);
  const [sel, setSel] = useState("");
  const [draft, setDraft] = useState("");
  const [replacing, setReplacing] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [asking, setAsking] = useState(false); // waiting for Chrome's permission prompt
  const [confirmForget, setConfirmForget] = useState(false);
  const [bad, setBad] = useState(""); // provider whose saved key failed its last test: it can't be made the default until it passes
  useEffect(() => { void getCloud().then((c) => { setCfg(c); setSel(c.provider); }); }, []);
  if (!cfg) return null;
  const save = (c: CloudConfig) => { setCfg(c); void setCloud(c); setMsg(null); };
  const pick = (id: string) => { setSel(sel === id ? "" : id); setMsg(null); setDraft(""); setReplacing(false); setConfirmForget(false); };
  const active = cloudById(cfg.provider);
  const status = (p: CloudProvider) => (!cfg.keys[p.id] ? null : bad === p.id ? "bad" : "ok");

  const row = (p: CloudProvider, i: number) => {
    const open = sel === p.id, st = status(p);
    return (
      <li key={p.id} style={{ borderRadius: 20, background: open ? "var(--surf2)" : "transparent", transition: "background .2s var(--ease)" }}>
        <button aria-expanded={open} onClick={() => pick(p.id)} className="p-row cloud-row"
          style={{ width: "100%", minHeight: 64, padding: "10px 12px", gap: 12, border: 0, borderRadius: 20, background: "transparent", color: "var(--tx)", textAlign: "left" }}>
          <span aria-hidden style={{ width: 40, height: 40, flex: "none", borderRadius: 13, display: "grid", placeItems: "center", background: TINTS[i % TINTS.length], color: "#16130F", font: "800 14px var(--head)" }}>{initials(p.maker)}</span>
          <span style={{ flex: 1, minWidth: 0 }}>
            <span className="p-row" style={{ gap: 6, flexWrap: "wrap" }}>
              <b style={{ font: "700 15px/20px var(--font)" }}>{titleOf(p)}</b>
              <Chip kind="line">{modelOf(cfg, p)}</Chip>
              {cfg.provider === p.id && <Chip kind="ink">Default</Chip>}
              {p.slow && <Chip kind="sky">Slower</Chip>}
              {p.experimental && <Chip kind="warn">Experimental</Chip>}
            </span>
            <span className="p-hint" style={{ display: "block" }}>{p.speedNote} · {shortPrice(p.priceNote)}</span>
          </span>
          {st === "ok" && <Chip kind="ok" icon="check">Connected</Chip>}
          {st === "bad" && <Chip kind="err">Key rejected</Chip>}
          <Icon name="chevDown" size={16} style={{ color: "var(--tx3)", transform: open ? "rotate(180deg)" : "none", transition: "transform .2s var(--ease)" }} />
        </button>
        {open && detail(p)}
      </li>
    );
  };

  // A plain function, not a component: a component declared here would remount (and drop focus) on every keystroke.
  const detail = (p: CloudProvider) => {
    const c = cfg!;
    const key = c.keys[p.id] ?? "";
    const model = modelOf(c, p);
    const host = hostFor(p, c.region);
    const consented = c.consented.includes(p.id);
    const isDefault = c.provider === p.id;
    const live = p.liveSupported && c.live.includes(p.id);
    /** Turning Live on asks again for the live hosts (consent given before Live existed covered only the batch host). */
    const setLive = async (on: boolean) => {
      if (!on) return save({ ...c, live: c.live.filter((x) => x !== p.id) });
      const ok = await chrome.permissions.request({ origins: originsFor(p, c.region) }).catch(() => false);
      if (ok) save({ ...c, live: [...c.live.filter((x) => x !== p.id), p.id] });
      else setMsg({ ok: false, text: "Chrome did not grant access to the provider's live service, so Live stays off." });
    };
    const consent = async () => {
      if (p.needsRegion && !c.region.trim()) { setMsg({ ok: false, text: CLOUD_ERROR_TEXT.region }); return; }
      setAsking(true);
      // Every host this provider can use (batch, plus the live token and WebSocket hosts): without them Live never turns on.
      const ok = await chrome.permissions.request({ origins: originsFor(p, c.region) }).catch(() => false);
      setAsking(false);
      if (ok) save({ ...c, consented: [...c.consented, p.id] });
      else setMsg({ ok: false, text: "Chrome did not grant access to the provider, so nothing was turned on." });
    };
    /** Test the key, then report. A key that passes on save becomes the default provider; a failing key is kept but can't be used. */
    const check = async (k: string, next: CloudConfig, makeDefault: boolean) => {
      setBusy(true); setMsg(null);
      const r = await testKey(p, k, { model, region: next.region });
      setBusy(false);
      setBad(r.ok ? "" : p.id);
      const provider = r.ok && makeDefault ? p.id : !r.ok && next.provider === p.id ? "" : next.provider;
      save({ ...next, provider });
      setMsg(r.ok ? { ok: true, text: `Connected. ${p.maker} accepted this key${makeDefault ? ` and ${titleOf(p)} is now the default` : ""}.` } : { ok: false, text: `Key rejected: ${CLOUD_ERROR_TEXT[r.error]} The key is saved but is not used until a test passes.` });
    };
    const saveKey = () => {
      const k = draft.trim();
      if (!k || busy) return;
      setDraft(""); setReplacing(false);
      void check(k, { ...c, keys: { ...c.keys, [p.id]: k } }, true);
    };
    const forget = async () => {
      const { [p.id]: _gone, ...keys } = c.keys;
      save({ ...c, provider: isDefault ? "" : c.provider, keys, consented: c.consented.filter((x) => x !== p.id), live: c.live.filter((x) => x !== p.id) });
      await chrome.permissions.remove({ origins: originsFor(p, c.region) }).catch(() => false);
    };
    return (
      <div style={{ padding: "2px 16px 16px 64px", display: "flex", flexDirection: "column", gap: 12 }} className="cloud-detail">
        <div className="p-hint" style={{ color: "var(--tx2)" }}>{p.priceNote}. You pay {p.maker} directly.{p.notes && ` ${p.notes}`}</div>
        <div className="p-row" style={{ gap: 10, flexWrap: "wrap" }}>
          <label className="p-row" style={{ gap: 8 }}><span className="p-label">Model</span>
            <select className="p-input p-mono" value={model} disabled={p.models.length < 2} onChange={(e) => save({ ...c, models: { ...c.models, [p.id]: e.target.value } })}>
              {p.models.map((m) => <option key={m} value={m}>{m}{m === p.defaultModel ? " (default)" : ""}</option>)}
            </select>
          </label>
          {p.needsRegion && (
            <label className="p-row" style={{ gap: 8 }}><span className="p-label">Region</span>
              <input className="p-input" aria-label="Azure region or resource host" placeholder="westeurope" spellCheck={false} value={c.region} onChange={(e) => save({ ...c, region: e.target.value.trim() })} style={{ width: 220 }} />
            </label>
          )}
        </div>

        {!consented ? (
          <div style={{ padding: 16, borderRadius: 18, background: "var(--warn-soft)" }}>
            <div className="p-label">Your audio is sent to {p.maker}</div>
            <div className="p-hint" style={{ color: "var(--tx2)", marginTop: 4 }}>While this is on, each dictation's audio is uploaded to <b className="p-mono">{host}</b> and handled under {p.maker}'s own terms and privacy policy. Voicio sends it nowhere else. Chrome will ask you to allow access to that one site.</div>
            <div style={{ marginTop: 12 }}><Button kind="primary" loading={asking} onClick={() => void consent()}>I understand, allow {p.maker}</Button></div>
          </div>
        ) : (
          <>
            <div className="p-row" style={{ gap: 8, flexWrap: "wrap" }}>
              {key && !replacing
                ? <input className="p-input p-mono" aria-label={`${p.maker} API key`} readOnly value={mask(key)} style={{ flex: "1 1 220px" }} />
                : <input className="p-input p-mono" aria-label={`${p.maker} API key`} type="password" autoComplete="off" spellCheck={false} placeholder="Paste your API key" value={draft} onChange={(e) => setDraft(e.target.value)} onKeyDown={(e) => e.key === "Enter" && saveKey()} style={{ flex: "1 1 220px" }} />}
              {key && !replacing ? <Button onClick={() => setReplacing(true)}>Replace</Button> : <Button kind="primary" icon="key" loading={busy} disabled={!draft.trim()} onClick={saveKey}>Save and test</Button>}
              <Button icon="circleCheck" loading={busy} disabled={!key || replacing} onClick={() => void check(key, c, false)}>Test key</Button>
            </div>
            {msg && <div className="p-hint" role={msg.ok ? "status" : "alert"} style={{ color: msg.ok ? "var(--ok-tx)" : "var(--err)", fontWeight: 600 }}>{msg.text}</div>}
            <div className="p-row" style={{ gap: 8, flexWrap: "wrap" }}>
              {!isDefault && <Button disabled={!key || busy || bad === p.id} onClick={() => save({ ...c, provider: p.id })}>Set as default</Button>}
              {confirmForget
                ? <><span className="p-hint">Remove the {p.maker} key and revoke access to {host}?</span><Button kind="danger" onClick={() => { setConfirmForget(false); void forget(); }}>Remove</Button><Button kind="ghost" onClick={() => setConfirmForget(false)}>Keep</Button></>
                : <Button kind="ghost" icon="trash" onClick={() => setConfirmForget(true)}>Remove key</Button>}
            </div>
            {p.liveSupported && (
              <div className="p-row" style={{ gap: 12, padding: "12px 14px", borderRadius: 16, background: "var(--surf)" }}>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div className="p-row" style={{ gap: 6 }}><span className="p-label">Live</span>{p.live?.experimental && <Chip kind="warn">Experimental</Chip>}</div>
                  <div className="p-hint">Words appear while you speak.</div>
                </div>
                <Switch label={`Live with ${p.maker}`} checked={live} onChange={(v) => void setLive(v)} />
              </div>
            )}
            <div className="p-hint">Keys stay in this browser's local storage, never synced. Audio goes only to {p.maker}{p.liveSupported ? "" : ` (${host})`}.</div>
          </>
        )}
      </div>
    );
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <Card style={{ padding: 20 }}>
        <div className="p-row" style={{ gap: 14, flexWrap: "wrap", justifyContent: "space-between" }}>
          <div style={{ flex: "1 1 240px" }}>
            <div className="p-row" style={{ gap: 8, flexWrap: "wrap" }}>
              <b style={{ font: "800 19px/24px var(--head)", letterSpacing: "-.3px" }}>Use cloud for</b>
              {cfg.mode !== "off" && active ? <Chip kind="ok" dot>{titleOf(active)}</Chip> : <Chip kind="line">Off</Chip>}
            </div>
            <div className="p-hint" style={{ marginTop: 4 }}>On-device recognition stays first: if the cloud fails, the on-device text is used.{cfg.mode !== "off" && !active && " Connect a provider below to turn it on."}</div>
          </div>
          <Segmented<CloudMode> label="Use cloud for" value={cfg.mode} onChange={(mode) => save({ ...cfg, mode })} options={[
            { value: "off", label: "Off" }, { value: "always", label: "All dictations" }, { value: "fallback", label: "Only when no on-device model" },
          ]} />
        </div>
      </Card>

      <Card style={{ padding: "12px 8px" }}>
        <p className="p-sec" style={{ margin: "6px 14px 6px" }}>Providers</p>
        <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: 2 }}>{FEATURED.map(row)}</ul>
        {MORE.length > 0 && (
          <details className="cloud-more" open={MORE.some((p) => p.id === sel || p.id === cfg.provider) || undefined}>
            <summary className="p-row" style={{ minHeight: 44, padding: "0 14px", gap: 8, cursor: "pointer", font: "700 13px var(--font)", color: "var(--ac-tx)", listStyle: "none" }}><Icon name="chevRight" size={14} />More providers ({MORE.length})</summary>
            <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: 2 }}>{MORE.map((p, i) => row(p, i + FEATURED.length))}</ul>
          </details>
        )}
      </Card>

      <details className="cloud-more" style={{ padding: "0 6px" }}>
        <summary className="p-row" style={{ minHeight: 36, gap: 8, cursor: "pointer", font: "600 13px var(--font)", color: "var(--tx3)", listStyle: "none" }}><Icon name="chevRight" size={14} />Not supported ({CLOUD_EXCLUDED.length})</summary>
        <ul className="p-hint" style={{ margin: "6px 0 0", paddingLeft: 22, display: "grid", gap: 6 }}>
          {CLOUD_EXCLUDED.map((x) => <li key={x.id}><b style={{ color: "var(--tx2)" }}>{x.name}</b>: {x.reason}</li>)}
        </ul>
      </details>
    </div>
  );
}
