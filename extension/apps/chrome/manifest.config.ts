import { defineManifest } from "@crxjs/vite-plugin";
import pkg from "./package.json";
import cloud from "../../../providers/cloud-stt.json";

// Cloud providers come from the shared registry: every host they use (Azure's region hosts as wildcards).
const cloudHosts = [...new Set(cloud.providers.flatMap((p) => p.needs_region ? [`https://${p.host.replace("{region}", "*")}`, "https://*.cognitiveservices.azure.com"] : [`https://${p.host}`]))];
// Live (streaming) providers with a browser path: the short-lived token host and the WebSocket host.
const liveSpecs = cloud.providers.flatMap((p) => ("live" in p && p.live?.browser ? [p.live as { url: string; browser: { token: { url: string } } }] : []));
const liveTokenHosts = [...new Set(liveSpecs.map((l) => `https://${new URL(l.browser.token.url).host}`))];
const liveWsHosts = [...new Set(liveSpecs.map((l) => `wss://${new URL(l.url).host}`))];
const liveHttpsHosts = liveWsHosts.map((h) => h.replace("wss://", "https://"));

export default defineManifest({
  manifest_version: 3,
  name: "Voicio for Chrome",
  short_name: "Voicio",
  version: pkg.version,
  description: "Private voice dictation. Speech is recognised inside Chrome on your computer. Audio leaves it only if you turn on an optional cloud provider.",
  minimum_chrome_version: "116",
  icons: { "16": "icons/16.png", "32": "icons/32.png", "48": "icons/48.png", "128": "icons/128.png" },
  action: {
    default_title: "Voicio",
    default_popup: "src/popup/index.html",
    default_icon: { "16": "icons/16.png", "32": "icons/32.png", "48": "icons/48.png" },
  },
  options_page: "src/options/index.html",
  background: { service_worker: "src/background/index.ts", type: "module" },
  permissions: ["storage", "contextMenus", "offscreen", "activeTab", "scripting", "clipboardWrite"],
  // Least privilege: the content script is injected with activeTab when the user acts (shortcut, menu, popup),
  // and registered permanently only for sites the user explicitly allows (optional host permission, asked per site).
  // Cloud provider hosts are listed too (also covered by <all_urls>), so each one can be granted on its own.
  optional_host_permissions: ["<all_urls>", ...[...new Set([...cloudHosts, ...liveTokenHosts, ...liveHttpsHosts])].map((h) => `${h}/*`)],
  // The e2e build runs without prompts, so it gets the permission up front. Never set in a release build.
  ...(process.env.VOICIO_E2E ? { host_permissions: ["<all_urls>"] } : {}),
  commands: {
    "toggle-dictation": {
      suggested_key: { default: "Alt+Shift+V" },
      description: "Start or stop dictation",
    },
  },
  content_security_policy: {
    extension_pages: `script-src 'self' 'wasm-unsafe-eval'; object-src 'self'; connect-src 'self' https://huggingface.co https://*.huggingface.co https://*.hf.co ${[...new Set([...cloudHosts, ...liveTokenHosts])].join(" ")} ${liveWsHosts.join(" ")};`,
  },
  // Cross-origin isolation lets the wasm runtime use threads. CORS fetches (model download) are unaffected.
  ...({ cross_origin_embedder_policy: { value: "require-corp" }, cross_origin_opener_policy: { value: "same-origin" } } as object),
  // ort/* is loaded by the extension's own worker (same origin), so nothing is exposed to web pages.
});
