import type { Field } from "./fields";

/** Insert at the caret. Rich editors get real editing commands; plain fields fall back to InputEvent, then to value. */
export function insertText(el: Field, text: string, mode: "type" | "paste"): boolean {
  el.focus({ preventScroll: true });
  const before = readValue(el);
  if (mode === "paste" && dispatchPaste(el, text)) return true; // the page handled the paste event itself
  if (execInsert(el, text) && readValue(el) !== before) return true;
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
    // Last resort for fields that ignore editing commands: set the value and tell the page's framework.
    const start = el.selectionStart ?? el.value.length;
    const end = el.selectionEnd ?? start;
    const value = el.value.slice(0, start) + text + el.value.slice(end);
    const setter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), "value")?.set;
    setter ? setter.call(el, value) : (el.value = value);
    el.setSelectionRange(start + text.length, start + text.length);
    el.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: text }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
    return true;
  }
  return readValue(el) !== before;
}

const readValue = (el: Field) => (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement ? el.value : el.textContent ?? "");

function execInsert(el: Field, text: string): boolean {
  const lines = text.split("\n");
  let ok = true;
  lines.forEach((line, i) => {
    if (i > 0) ok = (el instanceof HTMLInputElement ? false : document.execCommand("insertLineBreak")) || ok;
    if (line) ok = document.execCommand("insertText", false, line) && ok;
  });
  return ok;
}

function dispatchPaste(el: Field, text: string): boolean {
  try {
    const dt = new DataTransfer();
    dt.setData("text/plain", text);
    const ev = new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true });
    el.dispatchEvent(ev);
    return ev.defaultPrevented; // the page handled it itself
  } catch { return false; }
}

export function pressEnter(el: Field) {
  const init = { key: "Enter", code: "Enter", keyCode: 13, which: 13, bubbles: true, cancelable: true } as KeyboardEventInit;
  const down = new KeyboardEvent("keydown", init);
  el.dispatchEvent(down);
  el.dispatchEvent(new KeyboardEvent("keypress", init));
  if (!down.defaultPrevented && !(el instanceof HTMLInputElement)) document.execCommand("insertLineBreak");
  el.dispatchEvent(new KeyboardEvent("keyup", init));
}
