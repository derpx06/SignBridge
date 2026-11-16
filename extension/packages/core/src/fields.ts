/** Fields we must never dictate into. Locked: no setting changes this. */
export function isForbiddenField(el: { type?: string; autocomplete?: string; getAttribute?: (n: string) => string | null }): boolean {
  const type = (el.type ?? el.getAttribute?.("type") ?? "").toLowerCase();
  const ac = (el.autocomplete ?? el.getAttribute?.("autocomplete") ?? "").toLowerCase();
  return type === "password" || ac.split(/\s+/).some((t) => t.startsWith("cc-"));
}
