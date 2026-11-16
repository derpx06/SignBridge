// Renders the toolbar/store icons from the brand mark (packages/ui/src/mark.ts) in headless Chromium.
// usage: node e2e/icons.mjs   (writes public/icons/{16,32,48,128}.png)
import { chromium } from "playwright";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { markSvg } from "../../../packages/ui/src/mark.ts";

const here = path.dirname(fileURLToPath(import.meta.url));
const browser = await chromium.launch({ executablePath: "/usr/bin/chromium", args: ["--headless=new", "--no-sandbox"] });
try {
  const page = await browser.newPage({ deviceScaleFactor: 1 });
  for (const px of [16, 32, 48, 128]) {
    // 128: Chrome wants about 96-112 px of artwork with room for a soft shadow; the small sizes use the full square.
    const art = px === 128 ? 112 : px;
    const pad = (px - art) / 2;
    await page.setContent(`<body style="margin:0;background:transparent"><div id=i style="width:${px}px;height:${px}px;position:relative">
      <div style="position:absolute;left:${pad}px;top:${pad}px;width:${art}px;height:${art}px;border-radius:${art * 0.235}px;${px === 128 ? "box-shadow:0 4px 8px rgba(22,19,15,.3)" : ""}">${markSvg(art)}</div></div>`);
    await page.locator("#i").screenshot({ path: path.join(here, "..", "public", "icons", `${px}.png`), omitBackground: true });
  }
} finally { await browser.close(); }
console.log("icons written");
