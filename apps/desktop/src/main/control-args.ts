/**
 * Argument validation for the control handlers (REQ-PX-057, 058, 062;
 * REQ-EV-0103): everything the renderer sends is hostile until it is checked
 * here, and what passes is the narrowest shape the Core's command takes. These
 * checks narrow; they never decide. Whether a pattern is a valid rule, which
 * mode approves what or whether a restore is exact is the Core's answer.
 */
import { RUN_MODES, type AddRuleInput, type CheckpointTargetInput } from "../shared/control-types.ts";

export const bad = (what: string): never => {
  throw new Error(`BAD_ARGUMENT: ${what}`);
};

const HEX32 = /^[0-9a-f]{32}$/;
const UUID = /^[0-9a-f]{8}-?[0-9a-f]{4}-?[0-9a-f]{4}-?[0-9a-f]{4}-?[0-9a-f]{12}$/i;
// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f]/;

export function validRunMode(v: unknown): string {
  if (typeof v !== "string" || !(RUN_MODES as readonly string[]).includes(v)) return bad(`mode must be one of ${RUN_MODES.join(", ")}`);
  return v;
}

export function validBool(v: unknown, what: string): boolean {
  if (typeof v !== "boolean") return bad(`${what} must be a boolean`);
  return v;
}

/** An approval id: the Core names it as a UUID; the wire wants 32 hex characters. */
export function validApprovalId(v: unknown): string {
  const s = typeof v === "string" ? v.replace(/-/g, "").toLowerCase() : "";
  if (!HEX32.test(s)) return bad("approvalId must be an approval id");
  return s;
}

/** The intent hash the person saw: a sha256 in hex. An empty or malformed hash is never forwarded, so a decision can not be made without naming one. */
export function validIntentHash(v: unknown): string {
  if (typeof v !== "string" || !/^[0-9a-f]{64}$/.test(v)) return bad("intentHash must be the 64-hex-digit intent the card showed");
  return v;
}

export function validReason(v: unknown): string {
  if (v === undefined || v === null) return "";
  if (typeof v !== "string") return bad("reason must be text");
  return v.slice(0, 2000);
}

/** An argv prefix of a rule: 1..16 tokens of 1..512 characters, no control characters. The Core refuses the rest (wrappers, bare programs, operators). */
export function validPattern(v: unknown): string[] {
  if (!Array.isArray(v) || v.length === 0 || v.length > 16) return bad("pattern must be 1..16 tokens");
  return v.map((t) => {
    if (typeof t !== "string" || t.length === 0 || t.length > 512 || CONTROL.test(t)) return bad("each pattern token must be 1..512 printable characters");
    return t;
  });
}

export function validRuleId(v: unknown): string {
  if (typeof v !== "string" || v.length === 0 || v.length > 128 || CONTROL.test(v)) return bad("ruleId must be a rule id");
  return v;
}

export function validRuleInput(v: unknown, now: number): AddRuleInput {
  if (typeof v !== "object" || v === null || Array.isArray(v)) return bad("rule must be an object");
  const r = v as Record<string, unknown>;
  const pattern = validPattern(r.pattern);
  if (r.scope !== "TASK" && r.scope !== "REPO" && r.scope !== "USER") return bad("scope must be TASK, REPO or USER");
  let expiresAtMs = 0;
  if (r.expiresAtMs !== undefined && r.expiresAtMs !== null && r.expiresAtMs !== 0) {
    // An expiry is a moment in the future, at most ten years away.
    if (typeof r.expiresAtMs !== "number" || !Number.isInteger(r.expiresAtMs) || r.expiresAtMs <= now || r.expiresAtMs > now + 10 * 365 * 86_400_000) return bad("expiresAtMs must be a future time or 0");
    expiresAtMs = r.expiresAtMs;
  }
  const coversAlwaysAsk = r.coversAlwaysAsk === undefined ? false : validBool(r.coversAlwaysAsk, "coversAlwaysAsk");
  return { pattern, scope: r.scope, expiresAtMs, coversAlwaysAsk };
}

/** A checkpoint reference: the Core's own id text, a turn id, a turn ordinal or a name; at most one. */
export function validTarget(v: unknown): CheckpointTargetInput {
  if (v === undefined || v === null) return {};
  if (typeof v !== "object" || Array.isArray(v)) return bad("target must be an object");
  const t = v as Record<string, unknown>;
  const out: CheckpointTargetInput = {};
  let given = 0;
  if (t.checkpointId !== undefined && t.checkpointId !== "") {
    if (typeof t.checkpointId !== "string" || !UUID.test(t.checkpointId)) return bad("checkpointId must be a checkpoint id");
    out.checkpointId = t.checkpointId;
    given++;
  }
  if (t.turnId !== undefined && t.turnId !== "") {
    const s = typeof t.turnId === "string" ? t.turnId.replace(/-/g, "").toLowerCase() : "";
    if (!HEX32.test(s)) return bad("turnId must be a turn id");
    out.turnId = s;
    given++;
  }
  if (t.turnOrdinal !== undefined && t.turnOrdinal !== 0) {
    if (typeof t.turnOrdinal !== "number" || !Number.isInteger(t.turnOrdinal) || t.turnOrdinal < 1 || t.turnOrdinal > 1_000_000) return bad("turnOrdinal must be a turn number");
    out.turnOrdinal = t.turnOrdinal;
    given++;
  }
  if (t.name !== undefined && t.name !== "") {
    if (typeof t.name !== "string" || t.name.length > 64 || CONTROL.test(t.name)) return bad("name must be a checkpoint name");
    out.name = t.name;
    given++;
  }
  if (given > 1) return bad("name the checkpoint by one of checkpointId, turnId, turnOrdinal or name");
  return out;
}

/** A repository-relative path the person chose to keep: bounded, no control characters, never absolute and never leaving the workspace. */
export function validKeepPaths(v: unknown): string[] {
  if (v === undefined || v === null) return [];
  if (!Array.isArray(v) || v.length > 2000) return bad("keepPaths must be a list of at most 2000 paths");
  return v.map((p) => {
    if (typeof p !== "string" || p.length === 0 || p.length > 4096 || CONTROL.test(p) || p.startsWith("/") || /^[A-Za-z]:/.test(p) || p.split(/[\\/]/).includes("..")) return bad("a kept path must be a path inside the workspace");
    return p;
  });
}

export function validExpected(v: unknown): { path: string; contentHash: string }[] {
  if (v === undefined || v === null) return [];
  if (!Array.isArray(v) || v.length > 5000) return bad("expected must be a list of at most 5000 entries");
  return v.map((e) => {
    const x = e as { path?: unknown; contentHash?: unknown };
    if (typeof x?.path !== "string" || x.path.length === 0 || x.path.length > 4096 || CONTROL.test(x.path)) return bad("an expected entry needs a path");
    if (typeof x.contentHash !== "string" || !(x.contentHash === "" || /^[0-9a-f]{64}$/.test(x.contentHash))) return bad("an expected entry needs a sha256 or an empty hash");
    return { path: x.path, contentHash: x.contentHash };
  });
}

/** A command id the renderer keeps so a retried restore or fork acts once: 32 hex characters. */
export function validCommandId(v: unknown): Uint8Array | undefined {
  if (v === undefined || v === null || v === "") return undefined;
  if (typeof v !== "string" || !HEX32.test(v)) return bad("commandId must be 32 hex characters");
  return new Uint8Array(Buffer.from(v, "hex"));
}

/** A non-negative whole number as a decimal string (a 64-bit count on the wire). */
export function validCount(v: unknown, what: string): bigint {
  if (v === undefined || v === null || v === "") return 0n;
  if (typeof v !== "string" || !/^\d{1,18}$/.test(v)) return bad(`${what} must be a decimal count`);
  return BigInt(v);
}

export function validGoal(v: unknown): string {
  if (v === undefined || v === null) return "";
  if (typeof v !== "string" || v.length > 20_000) return bad("goal must be text of at most 20000 characters");
  return v;
}
