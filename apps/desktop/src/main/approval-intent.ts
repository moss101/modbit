/**
 * What an approval card shows (REQ-PX-058; docs/65 AFW-F01, AFW-F11, AFW-F12):
 * the typed reason the Core asked, taken from the approval's scope, and the
 * exact intent, taken from the call's recorded arguments. Both are read here,
 * in main, from the Core's own records; the renderer is handed plain views and
 * invents nothing. A tool's arguments are data from the model: they are
 * bounded, never interpreted as an instruction and never executed.
 */
import type { ApprovalIntentView, ApprovalReasonView } from "../shared/control-types.ts";

const MAX_ARGS_PREVIEW = 4000;
const MAX_STRING = 240;
const MAX_LIST = 40;

const strings = (v: unknown, max = MAX_LIST): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === "string").slice(0, max) : []);

/** The reason block of an approval's `scope_json` (the Kernel writes it; unknown or missing parts stay empty rather than invented). */
export function reasonOf(scopeJson: string): ApprovalReasonView {
  let s: Record<string, unknown> = {};
  try {
    const v: unknown = JSON.parse(scopeJson);
    if (v && typeof v === "object" && !Array.isArray(v)) s = v as Record<string, unknown>;
  } catch {
    // A scope that does not parse is shown as "no reason given".
  }
  const str = (k: string): string => (typeof s[k] === "string" ? (s[k] as string).slice(0, 200) : "");
  return {
    code: str("ask_reason"),
    runMode: str("run_mode"),
    askClasses: strings(s.ask_classes),
    contained: s.contained === true,
    capabilities: strings(s.capabilities),
    executionProfile: str("execution_profile"),
  };
}

const hostOf = (u: string): string | null => {
  try {
    const url = new URL(u);
    return url.host || null;
  } catch {
    return null;
  }
};

const clip = (s: string): string => (s.length > MAX_STRING ? `${s.slice(0, MAX_STRING)}…` : s);

/** The arguments with long strings clipped and the whole bounded: the exact thing asked, readable. */
function preview(v: unknown): { text: string; truncated: boolean } {
  let truncated = false;
  const walk = (x: unknown, depth: number): unknown => {
    if (typeof x === "string") {
      if (x.length > MAX_STRING) truncated = true;
      return clip(x);
    }
    if (Array.isArray(x)) {
      if (x.length > MAX_LIST) truncated = true;
      return depth > 4 ? "[…]" : x.slice(0, MAX_LIST).map((e) => walk(e, depth + 1));
    }
    if (x && typeof x === "object") {
      if (depth > 4) return "{…}";
      const out: Record<string, unknown> = {};
      for (const [k, val] of Object.entries(x as Record<string, unknown>).slice(0, MAX_LIST)) out[k] = walk(val, depth + 1);
      return out;
    }
    return x;
  };
  let text = JSON.stringify(walk(v, 0));
  if (text.length > MAX_ARGS_PREVIEW) {
    text = `${text.slice(0, MAX_ARGS_PREVIEW)}…`;
    truncated = true;
  }
  return { text, truncated };
}

const EMPTY: ApprovalIntentView = { found: false, argv: null, command: "", cwd: "", paths: [], hosts: [], escalation: "", argsPreview: "", truncated: false };

/** The intent of a call, from its recorded arguments (JSON text); `EMPTY` when they are not available. */
export function intentOf(argumentsJson: string | null): ApprovalIntentView {
  if (argumentsJson === null) return { ...EMPTY };
  let a: unknown;
  try {
    a = JSON.parse(argumentsJson);
  } catch {
    return { ...EMPTY };
  }
  if (!a || typeof a !== "object" || Array.isArray(a)) return { ...EMPTY };
  const o = a as Record<string, unknown>;
  const argv = Array.isArray(o.argv) && o.argv.every((t) => typeof t === "string") && o.argv.length > 0 ? (o.argv as string[]).slice(0, 64) : null;
  const paths = [...strings(o.paths), ...(typeof o.path === "string" ? [o.path] : []), ...(typeof o.file === "string" ? [o.file] : [])].slice(0, MAX_LIST).map(clip);
  const urls = [...(typeof o.url === "string" ? [o.url] : []), ...strings(o.urls)];
  const hosts = [...new Set(urls.map(hostOf).filter((h): h is string => h !== null))].slice(0, MAX_LIST);
  const p = preview(o);
  return {
    found: true,
    argv,
    command: typeof o.command === "string" ? clip(o.command) : "",
    cwd: typeof o.cwd === "string" ? clip(o.cwd) : "",
    paths,
    hosts,
    escalation: typeof o.escalation === "string" ? o.escalation.slice(0, 16) : "",
    argsPreview: p.text,
    truncated: p.truncated,
  };
}
