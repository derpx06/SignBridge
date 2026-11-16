import { describe, expect, it } from "vitest";
import { restrictedPage } from "./page";

describe("pages Chrome keeps extensions off", () => {
  it("flags the New Tab page, chrome:// pages, about: pages and the Web Store", () => {
    for (const u of ["chrome://newtab/", "chrome-search://local-ntp/local-ntp.html", "chrome://extensions", "about:blank", "edge://settings", "https://chromewebstore.google.com/detail/x", "https://chrome.google.com/webstore/category/extensions", undefined])
      expect(restrictedPage(u)).toBe(true);
  });
  it("lets ordinary websites through", () => {
    for (const u of ["https://dictionary.cambridge.org/dictionary/english/get", "http://127.0.0.1:3000/", "https://www.godaddy.com/en-in", "https://chrome.google.com/other"])
      expect(restrictedPage(u)).toBe(false);
  });
});
