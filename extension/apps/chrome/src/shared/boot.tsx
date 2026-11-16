import { createRoot } from "react-dom/client";
import { Component, type ReactNode } from "react";
import "@voicio/ui/tokens.css";
import "@voicio/ui/components.css";
import "./fonts.css";
import "./page.css";

/** A crashed page must say so and offer a way out, never turn into a blank screen. */
class Guard extends Component<{ children: ReactNode }, { err?: Error }> {
  state: { err?: Error } = {};
  static getDerivedStateFromError(err: Error) { return { err }; }
  reset = async () => {
    await chrome.storage.session.clear().catch(() => {});
    await chrome.storage.local.remove(["snap"]).catch(() => {});
    location.reload();
  };
  render() {
    if (!this.state.err) return this.props.children;
    return (
      <div style={{ minHeight: "100vh", display: "grid", placeItems: "center", padding: 24 }}>
        <div style={{ maxWidth: 440, display: "flex", flexDirection: "column", gap: 12 }}>
          <h1 style={{ margin: 0, font: "600 22px var(--font)" }}>Something went wrong</h1>
          <p style={{ margin: 0, color: "var(--tx2)" }}>This page hit an error. Your settings and downloaded models are safe.</p>
          <code className="p-mono" style={{ color: "var(--tx3)", fontSize: 12, wordBreak: "break-word" }}>{String(this.state.err.message)}</code>
          <div className="p-row" style={{ gap: 8 }}><button className="v-btn primary" onClick={() => location.reload()}>Reload</button><button className="v-btn" onClick={() => void this.reset()}>Reset temporary data</button></div>
        </div>
      </div>
    );
  }
}

export function mount(node: ReactNode) {
  const el = document.getElementById("root")!;
  el.className = "voicio";
  createRoot(el).render(<Guard>{node}</Guard>);
}
