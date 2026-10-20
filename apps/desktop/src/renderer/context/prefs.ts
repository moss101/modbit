import type { UsageSetting } from "./model.ts";

/** The person's choice of when the usage summary shows (AFW-H06): a per-viewer convenience, kept in the renderer's storage, never state the Core needs. */
const KEY = "modbit.usageSummary";

export function parseUsageSetting(raw: unknown): UsageSetting {
  return raw === "always" || raw === "never" || raw === "auto" ? raw : "auto";
}

export function loadUsageSetting(): UsageSetting {
  try {
    return parseUsageSetting(localStorage.getItem(KEY));
  } catch {
    return "auto";
  }
}

export function saveUsageSetting(v: UsageSetting): void {
  try {
    localStorage.setItem(KEY, v);
  } catch {
    // Storage may be blocked; the choice then lasts for the session only.
  }
}
