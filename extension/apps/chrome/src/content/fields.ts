export type Field = HTMLInputElement | HTMLTextAreaElement | HTMLElement;

const NON_TEXT = new Set(["checkbox", "radio", "button", "submit", "file", "range", "color", "image", "reset", "hidden"]);

export function isEditable(el: Element | null): el is Field {
  if (!el) return false;
  if (el instanceof HTMLTextAreaElement) return !el.readOnly && !el.disabled;
  if (el instanceof HTMLInputElement) return !NON_TEXT.has(el.type) && !el.readOnly && !el.disabled;
  return el instanceof HTMLElement && el.isContentEditable;
}

/** The focused element, looking through open shadow roots. */
export function deepActive(): Element | null {
  let el: Element | null = document.activeElement;
  while (el?.shadowRoot?.activeElement) el = el.shadowRoot.activeElement;
  return el;
}

export const fieldRect = (el: Field) => el.getBoundingClientRect();
