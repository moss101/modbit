/**
 * The exact intent of a native-control approval (PX-074; docs/66 CUC-C01),
 * read from the approval's own `scope_json` where the Core's tool bound it:
 * the application's identity as the operating system vouches for it, the
 * window, the element or screenshot, the action and its arguments in full, the
 * scope. Read here, in main, from the Core's record; the renderer is handed
 * plain views and invents nothing. Everything an application or the model
 * wrote (a window title, an element name, the reason a grant is wanted) is
 * data: it is bounded, kept as text and never read as an instruction.
 */
import type { ComputerIntentView } from "../shared/computer-types.ts";

const MAX_TEXT = 400;

const clip = (s: string, max = MAX_TEXT): string => (s.length > max ? `${s.slice(0, max)}…` : s);
const str = (v: unknown, max = MAX_TEXT): string => (typeof v === "string" ? clip(v, max) : "");
const num = (v: unknown): number => (typeof v === "number" && Number.isFinite(v) ? v : 0);
const obj = (v: unknown): Record<string, unknown> => (v && typeof v === "object" && !Array.isArray(v) ? (v as Record<string, unknown>) : {});

/** The arguments in full as compact JSON text, each string clipped; typed text is shown, not hidden. */
function argsText(v: unknown): { text: string; truncated: boolean } {
  let truncated = false;
  const walk = (x: unknown, depth: number): unknown => {
    if (typeof x === "string") {
      if (x.length > 2000) truncated = true;
      return x.length > 2000 ? `${x.slice(0, 2000)}…` : x;
    }
    if (Array.isArray(x)) return depth > 3 ? "[…]" : x.slice(0, 40).map((e) => walk(e, depth + 1));
    if (x && typeof x === "object") {
      if (depth > 3) return "{…}";
      return Object.fromEntries(Object.entries(x as Record<string, unknown>).slice(0, 40).map(([k, val]) => [k, walk(val, depth + 1)]));
    }
    return x;
  };
  let text = v === undefined || v === null ? "" : JSON.stringify(walk(v, 0));
  if (text.length > 4000) {
    text = `${text.slice(0, 4000)}…`;
    truncated = true;
  }
  return { text, truncated };
}

/** The computer intent of an approval's scope; null when the approval is not a native-control one (or the scope does not say). */
export function computerIntentOf(scopeJson: string): ComputerIntentView | null {
  let scope: Record<string, unknown>;
  try {
    scope = obj(JSON.parse(scopeJson));
  } catch {
    return null;
  }
  const c = obj(obj(scope.intent).computer);
  if (Object.keys(c).length === 0) return null;
  const tool = str(c.tool, 80);
  if (!tool.startsWith("computer.")) return null;
  const kind = c.kind === "observation_grant" ? "observation_grant" : "action";
  const app = obj(c.application);
  const win = obj(c.window);
  const t = obj(c.target);
  const tk = t.kind === "element" || t.kind === "coordinate" || t.kind === "focus_or_keys" ? t.kind : null;
  const a = argsText(c.arguments);
  const g = obj(c.grant);
  return {
    tool,
    kind,
    scope: str(c.scope, 16),
    application: { bundleId: str(app.bundle_id, 200), executablePath: str(app.executable_path, 500), signingIdentity: str(app.signing_identity, 200), pid: num(app.pid) },
    applicationName: str(c.application_name, 200),
    window: { id: str(win.id, 120), title: str(win.title, 300) },
    target: tk ? { kind: tk, snapshot: str(t.snapshot, 80), id: str(t.id, 80), role: str(t.role, 80), name: str(t.name, 300), frame: str(t.frame, 80), windowVersion: num(t.window_version) } : null,
    action: str(c.action, 40),
    modality: str(c.modality, 20),
    argumentsText: a.text,
    argumentsTruncated: a.truncated,
    reason: str(c.reason, 600),
    takesDisplayAndCursor: c.takes_display_and_cursor === true,
    grant: kind === "observation_grant" ? { endsWith: str(g.ends_with, 80), maxSeconds: num(g.max_seconds), callCap: num(g.observation_call_cap) } : null,
    reversible: c.reversible === true,
    beforeArtifact: "",
  };
}
