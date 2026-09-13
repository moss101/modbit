/**
 * Thin-client conformance suite (PX-001; docs/29 "Thin-client conformance
 * contract"). Every SurfaceProtocol client — the CLI, Electron main, an IDE
 * adapter — passes these cases against a real Core before it is exposed:
 *
 * - command idempotency by the command id the caller keeps (a retry gets the
 *   same result, marked replayed, and creates nothing);
 * - cursor replay after a dropped connection (no duplicate, no gap, no
 *   restart from zero);
 * - approval bound to the intent hash the person saw (a decision naming any
 *   other intent is refused by the Core and the client shows the refusal);
 * - `Needs Attention` reasons and the acceptance verdict rendered from the
 *   Core's own views, nothing invented;
 * - the Core's rejection surfaced as the Core said it (code and message,
 *   no silent retry);
 * - a static proof that the client links no provider, workspace filesystem,
 *   Git or policy code path of its own.
 *
 * The suite drives a client only through `ConformanceSubject`, which each
 * client implements over its own protocol path; the reference truth for
 * attention and verdicts is read by the suite's own `CoreClient`. Nothing
 * here grants a capability or reaches the Core other than as a client.
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { CommandStatus, SessionCreatedSchema, StartTaskSchema, TaskRunStartedSchema } from "@modbit/surface-protocol";
import type { CoreClient } from "./client.ts";

/** One event as a client reads it by cursor. */
export interface CursorEvent {
  offset: bigint;
  eventType: string;
  taskId: string;
}

/** A pending or resolved approval as a client lists it. */
export interface ApprovalRow {
  approvalId: string;
  intentHash: string;
  status: string;
  toolName: string;
}

/** What the Core answered, or how it refused. */
export type Outcome<T> = { ok: true; value: T } | { ok: false; code: string; message: string };

/**
 * A client under conformance. Every method goes through the client's own
 * protocol path (its socket, its command ids, its rendering) — never through
 * the suite's reference client.
 */
export interface ConformanceSubject {
  readonly name: string;
  /** Create a session under the caller's command id. */
  createSession(commandId: string): Promise<{ sessionId: string; replayed: boolean }>;
  /** Create a task under the caller's command id (idempotency is the point). */
  createTask(sessionId: string, goal: string, workspaceRoot: string, commandId: string): Promise<{ taskId: string; replayed: boolean }>;
  /** Start the task on the named endpoint/model; returns what the Core said. */
  startTask(sessionId: string, taskId: string, endpoint: string, model: string): Promise<Outcome<{ runId: string }>>;
  /** Stored events after `afterOffset` as the client reads them by cursor, at most `limit`, waiting up to `timeoutMs` for the first. */
  events(sessionId: string, afterOffset: bigint, limit: number, timeoutMs: number): Promise<CursorEvent[]>;
  /** Drop whatever connection the client holds; the next call must recover from its cursor. */
  disconnect(): Promise<void>;
  listApprovals(sessionId: string): Promise<ApprovalRow[]>;
  /** Decide an approval naming the intent hash the client shows for it. */
  resolveApproval(sessionId: string, approvalId: string, approve: boolean, intentHash: string): Promise<Outcome<{ status: string }>>;
  /** Attention items as the client renders them: one rendered line per item, carrying its kind, reason and action. */
  renderAttention(sessionId: string): Promise<{ items: { kind: string; reason: string; action: string }[]; rendered: string[] }>;
  /** The acceptance verdict as the client renders it for a person. */
  renderVerdict(taskId: string): Promise<{ verdict: string; rendered: string }>;
  close(): Promise<void>;
}

export interface ConformanceCase {
  id: string;
  title: string;
  status: "PASS" | "FAIL";
  detail: string;
}

export interface ConformanceReport {
  subject: string;
  passed: boolean;
  cases: ConformanceCase[];
}

/** What the suite needs from the environment: a Core the subject reaches, a workspace, a model the Core can run the fixture script on. */
export interface ConformanceEnvironment {
  /** A fresh reference connection to the same Core, the suite's truth for events, attention and verdicts; closed after each use. */
  reference: () => Promise<CoreClient>;
  /** A git worktree the fixture task edits (the scripted model writes `out.txt`, opens and closes a worktree). */
  workspaceRoot: string;
  /** The fixture task's goal (a scripted model may key its script on it). */
  goal?: string;
  endpoint: string;
  model: string;
  /** How long to wait for the fixture run to reach each milestone. */
  timeoutMs?: number;
  /** Command ids the suite uses (32 hex each); random when absent. */
  ids?: { session: string; task: string; other: string };
}

function randomHex32(): string {
  let s = "";
  for (let i = 0; i < 32; i++) s += Math.floor(Math.random() * 16).toString(16);
  return s;
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

async function withReference<T>(env: ConformanceEnvironment, body: (c: CoreClient) => Promise<T>): Promise<T> {
  const c = await env.reference();
  try {
    return await body(c);
  } finally {
    c.close();
  }
}

async function until<T>(what: string, timeoutMs: number, probe: () => Promise<T | null>): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const v = await probe();
    if (v !== null) return v;
    if (Date.now() > deadline) throw new Error(`timed out after ${timeoutMs} ms waiting for ${what}`);
    await sleep(200);
  }
}

/**
 * Run the behavioral cases against `subject`. Every case reports PASS or
 * FAIL with the evidence; a thrown error inside a case is a FAIL of that
 * case, never a crash of the suite.
 */
export async function runConformance(subject: ConformanceSubject, env: ConformanceEnvironment): Promise<ConformanceReport> {
  const cases: ConformanceCase[] = [];
  const timeout = env.timeoutMs ?? 120_000;
  const ids = env.ids ?? { session: randomHex32(), task: randomHex32(), other: randomHex32() };
  const record = (id: string, title: string, ok: boolean, detail: string) => cases.push({ id, title, status: ok ? "PASS" : "FAIL", detail });
  const attempt = async (id: string, title: string, body: () => Promise<string>) => {
    try {
      record(id, title, true, await body());
    } catch (e) {
      record(id, title, false, (e as Error).message);
    }
  };
  let sessionId = "";
  let taskId = "";

  // 1. Idempotency by command id: the same id twice is one task; another id is another task.
  await attempt("idempotency", "commands are idempotent by the command id the caller keeps", async () => {
    const s1 = await subject.createSession(ids.session);
    sessionId = s1.sessionId;
    if (s1.replayed) throw new Error("a fresh command id was reported replayed");
    const goal = env.goal ?? "conformance fixture";
    const t1 = await subject.createTask(sessionId, goal, env.workspaceRoot, ids.task);
    const t2 = await subject.createTask(sessionId, goal, env.workspaceRoot, ids.task);
    const t3 = await subject.createTask(sessionId, goal, env.workspaceRoot, ids.other);
    if (t1.replayed) throw new Error("the first task creation was reported replayed");
    if (!t2.replayed || t2.taskId !== t1.taskId) throw new Error(`a retry under the same command id gave task ${t2.taskId} (replayed=${t2.replayed}); the first gave ${t1.taskId}`);
    if (t3.taskId === t1.taskId) throw new Error("a different command id gave the same task");
    taskId = t1.taskId;
    return `task ${taskId} once under command ${ids.task}; retry replayed; ${ids.other} made ${t3.taskId}`;
  });
  if (!taskId) return { subject: subject.name, passed: false, cases };

  // 2. Cursor replay: read, drop the connection, resume from the cursor — no duplicate, no gap, no restart.
  await attempt("cursor-replay", "events resume from the client's cursor after a dropped connection, without duplicates or gaps", async () => {
    const started = await subject.startTask(sessionId, taskId, env.endpoint, env.model);
    if (!started.ok) throw new Error(`start refused ${started.code}: ${started.message}`);
    const first = await subject.events(sessionId, 0n, 8, timeout);
    if (first.length < 4) throw new Error(`only ${first.length} events read before the drop`);
    for (let i = 1; i < first.length; i++) {
      if (first[i]!.offset <= first[i - 1]!.offset) throw new Error(`offsets not strictly increasing: ${first[i - 1]!.offset} then ${first[i]!.offset}`);
    }
    const cursor = first[first.length - 1]!.offset;
    await subject.disconnect();
    const resumed = await subject.events(sessionId, cursor, 200, timeout);
    if (resumed.length === 0) throw new Error("nothing read after the reconnect");
    if (resumed.some((e) => e.offset <= cursor)) throw new Error(`an event at or before the cursor ${cursor} was delivered again`);
    // The Core's own view of the gap: what the client read must be exactly
    // the Core's next events after the cursor (a client may stop early; it
    // may not skip, repeat or reorder).
    const truth = (await withReference(env, (c) => readByCursor(c, sessionId, cursor, resumed.length, timeout))).slice(0, resumed.length);
    const expected = truth.map((e) => e.offset.toString()).join(",");
    const got = resumed.map((e) => e.offset.toString()).join(",");
    if (expected !== got) throw new Error(`after the cursor the client read offsets [${got}], the Core has [${expected}]`);
    return `${first.length} events, dropped at offset ${cursor}, resumed with ${resumed.length} more and no repeat`;
  });

  // 3. Attention rendering while the fixture waits on its protected effect.
  let approval: ApprovalRow | null = null;
  await attempt("attention-rendering", "Needs Attention items are rendered with the Core's kind, reason and action", async () => {
    approval = await until("a REQUESTED approval", timeout, async () => (await subject.listApprovals(sessionId)).find((a) => a.status === "REQUESTED") ?? null);
    const truth = await withReference(env, (c) => c.attention(sessionId));
    const expected = truth.items.filter((i) => i.kind === "APPROVAL");
    if (expected.length === 0) throw new Error("the Core lists no APPROVAL attention item while an approval is REQUESTED");
    const view = await subject.renderAttention(sessionId);
    for (const item of expected) {
      const shown = view.items.find((i) => i.kind === item.kind && i.reason === item.reason && i.action === item.action);
      if (!shown) throw new Error(`the client does not list the ${item.kind} item (${item.reason})`);
      const line = view.rendered.find((l) => l.includes(item.kind) && l.includes(item.reason) && l.includes(item.action));
      if (!line) throw new Error(`no rendered line carries kind, reason and action of the ${item.kind} item: ${JSON.stringify(view.rendered)}`);
    }
    return `${expected.length} attention item(s) rendered with kind, reason and action`;
  });

  // 4. Approval bound to the intent hash the person saw.
  await attempt("intent-bound-approval", "an approval decision binds to the intent hash the client showed", async () => {
    if (!approval) approval = await until("a REQUESTED approval", timeout, async () => (await subject.listApprovals(sessionId)).find((a) => a.status === "REQUESTED") ?? null);
    const a = approval as ApprovalRow;
    if (!/^[0-9a-f]{64}$/.test(a.intentHash)) throw new Error(`the client shows no intent hash for approval ${a.approvalId}: ${JSON.stringify(a)}`);
    const wrong = a.intentHash.replace(/^./, a.intentHash.startsWith("0") ? "1" : "0");
    const refused = await subject.resolveApproval(sessionId, a.approvalId, true, wrong);
    if (refused.ok) throw new Error(`a decision naming intent ${wrong} was accepted (status ${refused.value.status}); the approval binds ${a.intentHash}`);
    if (refused.code !== "INTENT_MISMATCH") throw new Error(`a decision naming another intent was refused ${refused.code}, not INTENT_MISMATCH`);
    const still = (await subject.listApprovals(sessionId)).find((x) => x.approvalId === a.approvalId);
    if (!still || still.status !== "REQUESTED") throw new Error(`the approval is ${still?.status ?? "gone"} after a refused decision`);
    const decided = await subject.resolveApproval(sessionId, a.approvalId, true, a.intentHash);
    if (!decided.ok) throw new Error(`the decision naming the bound intent was refused ${decided.code}: ${decided.message}`);
    if (decided.value.status !== "APPROVED") throw new Error(`the decision recorded status ${decided.value.status}`);
    return `approval ${a.approvalId}: intent ${wrong.slice(0, 8)}… refused INTENT_MISMATCH, ${a.intentHash.slice(0, 8)}… APPROVED`;
  });

  // 5. Verdict rendering once the fixture has a gate result.
  await attempt("verdict-rendering", "the acceptance verdict is rendered from the Core's assurance view", async () => {
    const truth = await until("an acceptance gate result", timeout, async () => {
      const v = await withReference(env, (c) => c.taskAssurance(taskId));
      return v.acceptance && v.acceptance.verdict ? v.acceptance : null;
    });
    const view = await subject.renderVerdict(taskId);
    if (view.verdict !== truth.verdict) throw new Error(`the client renders verdict ${JSON.stringify(view.verdict)}, the Core's is ${truth.verdict}`);
    if (!view.rendered.includes(truth.verdict)) throw new Error(`the rendered text does not carry the verdict: ${view.rendered}`);
    return `verdict ${truth.verdict} at revision ${truth.candidateRevision} rendered as ${JSON.stringify(view.rendered.slice(0, 80))}`;
  });

  // 6. Rejection paths: the Core's refusal reaches the person as the Core said it.
  await attempt("rejection-paths", "a refused command surfaces the Core's code and message, unchanged and without a retry", async () => {
    const unknownTask = randomHex32();
    const start = await subject.startTask(sessionId, unknownTask, env.endpoint, env.model);
    if (start.ok) throw new Error(`starting task ${unknownTask}, which does not exist, was reported ok`);
    if (start.code !== "UNKNOWN_TASK") throw new Error(`the refusal code was ${start.code}, the Core's is UNKNOWN_TASK`);
    // The Core names ids in dashed UUID form; the client must carry its text through.
    if (!start.message.replace(/-/g, "").includes(unknownTask)) throw new Error(`the refusal message lost the Core's detail: ${JSON.stringify(start.message)}`);
    const decide = await subject.resolveApproval(sessionId, randomHex32(), true, "0".repeat(64));
    if (decide.ok) throw new Error("deciding an approval that does not exist was reported ok");
    if (decide.code !== "UNKNOWN_APPROVAL") throw new Error(`the refusal code was ${decide.code}, the Core's is UNKNOWN_APPROVAL`);
    return `UNKNOWN_TASK and UNKNOWN_APPROVAL surfaced verbatim`;
  });

  return { subject: subject.name, passed: cases.every((c) => c.status === "PASS"), cases };
}

/** Stored events after `cursor` through the reference client, at most `limit`. */
export async function readByCursor(client: CoreClient, sessionId: string, cursor: bigint, limit: number, timeoutMs: number): Promise<CursorEvent[]> {
  const out: CursorEvent[] = [];
  const done = new Promise<void>((resolve) => {
    const timer = setTimeout(resolve, timeoutMs);
    client.onEvent = (e) => {
      if (out.length >= limit) return;
      out.push({ offset: e.offset, eventType: e.event?.eventType ?? "", taskId: Buffer.from(e.event?.taskId?.value ?? new Uint8Array()).toString("hex") });
      if (out.length >= limit) {
        clearTimeout(timer);
        resolve();
      }
    };
  });
  client.subscribe(sessionId, cursor);
  await done;
  client.onEvent = null;
  return out;
}

// ---- Static proof: what a thin client must not link ----------------------

export interface ScanRule {
  /** Module specifiers a client must not import (exact, or a prefix ending in `/`). */
  forbiddenImports: string[];
  /** Source patterns a client must not contain (a Git or provider process of its own). */
  forbiddenPatterns: { pattern: RegExp; why: string }[];
  /** Sanctioned exceptions: a file that may import a specifier, and why. */
  allow?: { file: string; specifier: string; why: string }[];
}

export interface ScanFinding {
  file: string;
  line: number;
  what: string;
  why: string;
}

/** The rule every TypeScript thin client is scanned under. */
export const THIN_CLIENT_RULE: ScanRule = {
  forbiddenImports: [
    // Provider SDKs: a client never holds a credential or speaks to a model.
    "openai",
    "@anthropic-ai/",
    "@google/generative-ai",
    "@google/genai",
    "@aws-sdk/client-bedrock-runtime",
    "ollama",
    // Git: the Core's workspace-git owner is the only Git actor.
    "simple-git",
    "isomorphic-git",
    "nodegit",
    "dugite",
    // Policy and tool execution live in the Core.
    "@modbit/policy",
    "@modbit/tools",
    // The workspace is the Core's: a client reads and writes no files of its
    // own (a host's profile-local state is a sanctioned, named exception).
    "node:fs",
    "fs",
    "node:fs/promises",
    "fs/promises",
  ],
  forbiddenPatterns: [
    { pattern: /\b(spawn|spawnSync|execFile|execFileSync|exec|execSync)\(\s*["'`]git\b/, why: "a client runs no Git process of its own" },
    { pattern: /\b(spawn|spawnSync|execFile|execFileSync)\(\s*["'`](openai|anthropic|claude|gemini|ollama)\b/, why: "a client runs no provider process of its own" },
    { pattern: /\bprocess\.env\.(OPENAI_API_KEY|ANTHROPIC_API_KEY|GEMINI_API_KEY|GOOGLE_API_KEY)\b/, why: "a client reads no provider credential" },
  ],
};

function walk(dir: string, out: string[]): void {
  for (const name of readdirSync(dir)) {
    if (name === "node_modules" || name === "dist" || name === "gen" || name.startsWith(".")) continue;
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (/\.(ts|tsx|mts|cts|js|mjs|cjs)$/.test(name) && !/\.test\.[cm]?[jt]sx?$/.test(name) && !/\.spec\.[cm]?[jt]sx?$/.test(name)) out.push(p);
  }
}

/**
 * Scan a client's sources under `rule`. `root` is the package or app
 * directory; every non-test source under its `src/` (or under it, when it
 * has no `src/`) is read — build scripts and end-to-end tests are not the
 * client. Returns the findings; empty means the static proof holds.
 */
export function scanClientSources(root: string, rule: ScanRule = THIN_CLIENT_RULE): ScanFinding[] {
  const files: string[] = [];
  const src = join(root, "src");
  let hasSrc = false;
  try {
    hasSrc = statSync(src).isDirectory();
  } catch {
    hasSrc = false;
  }
  walk(hasSrc ? src : root, files);
  const findings: ScanFinding[] = [];
  const importRe = /(?:^|\n)\s*(?:import\s+(?:[^'"]*?\s+from\s+)?|export\s+\*\s+from\s+|import\s*\()\s*["']([^"']+)["']|\brequire\(\s*["']([^"']+)["']\s*\)/g;
  for (const f of files) {
    const rel = relative(root, f).replace(/\\/g, "/");
    const text = readFileSync(f, "utf8");
    for (const m of text.matchAll(importRe)) {
      const spec = m[1] ?? m[2] ?? "";
      const hit = rule.forbiddenImports.find((x) => (x.endsWith("/") ? spec.startsWith(x) : spec === x || spec.startsWith(`${x}/`)));
      if (!hit) continue;
      if (rule.allow?.some((a) => a.file === rel && a.specifier === spec)) continue;
      findings.push({ file: rel, line: text.slice(0, m.index ?? 0).split("\n").length, what: `import of ${spec}`, why: `forbidden in a thin client (${hit})` });
    }
    for (const { pattern, why } of rule.forbiddenPatterns) {
      const m = pattern.exec(text);
      if (m) findings.push({ file: rel, line: text.slice(0, m.index).split("\n").length, what: m[0], why });
    }
  }
  return findings;
}

/**
 * The dependency closure of a Rust client from `cargo metadata` (the CLI is
 * the contract's reference implementation): none of the named workspace
 * crates may be reachable from it.
 */
export function scanCargoClosure(metadata: { packages: { id: string; name: string }[]; resolve: { nodes: { id: string; deps: { pkg: string }[] }[] } }, clientCrate: string, forbiddenCrates: string[]): ScanFinding[] {
  const nameOf = new Map(metadata.packages.map((p) => [p.id, p.name] as const));
  const root = metadata.packages.find((p) => p.name === clientCrate);
  if (!root) return [{ file: "Cargo.lock", line: 0, what: clientCrate, why: "the client crate is not in the workspace metadata" }];
  const deps = new Map(metadata.resolve.nodes.map((n) => [n.id, n.deps.map((d) => d.pkg)] as const));
  const seen = new Set<string>();
  const stack = [root.id];
  const path = new Map<string, string>();
  while (stack.length) {
    const id = stack.pop()!;
    if (seen.has(id)) continue;
    seen.add(id);
    for (const d of deps.get(id) ?? []) {
      if (!path.has(d)) path.set(d, id);
      stack.push(d);
    }
  }
  const findings: ScanFinding[] = [];
  for (const id of seen) {
    const name = nameOf.get(id) ?? id;
    if (forbiddenCrates.includes(name)) {
      const via: string[] = [];
      let cur: string | undefined = id;
      while (cur && cur !== root.id) {
        via.unshift(nameOf.get(cur) ?? cur);
        cur = path.get(cur);
      }
      findings.push({ file: "Cargo.lock", line: 0, what: name, why: `reachable from ${clientCrate} via ${via.join(" → ")}` });
    }
  }
  return findings;
}

/**
 * The reference subject: the shared `CoreClient` itself, driven as Electron
 * main and an IDE adapter drive it. `reconnect` must hand back a fresh
 * client to the same Core (the supervisor does this for hosts).
 */
export function clientSubject(name: string, connect: () => Promise<CoreClient>): ConformanceSubject {
  let client: CoreClient | null = null;
  const current = async () => (client ??= await connect());
  const refused = <T>(e: unknown): Outcome<T> => {
    const err = e as { code?: string; message?: string };
    return { ok: false, code: err.code ?? "ERROR", message: err.message ?? String(e) };
  };
  return {
    name,
    async createSession(commandId) {
      const c = await current();
      const r = await c.command("CreateSession", new Uint8Array(), Buffer.from(commandId, "hex"));
      const s = fromBinary(SessionCreatedSchema, r.result);
      const sessionId = Buffer.from(s.sessionId?.value ?? new Uint8Array()).toString("hex");
      await c.acquireSessionLease(sessionId, `conformance ${name}`);
      return { sessionId, replayed: r.status === CommandStatus.REPLAYED };
    },
    async createTask(sessionId, goal, workspaceRoot, commandId) {
      const c = await current();
      if (c.leaseGeneration(sessionId) === undefined) await c.joinSessionLease(sessionId, `conformance ${name}`);
      const r = await c.createTask(sessionId, goal, Buffer.from(commandId, "hex"), workspaceRoot);
      return { taskId: r.taskId, replayed: r.replayed };
    },
    async startTask(sessionId, taskId, endpoint, model) {
      const c = await current();
      if (c.leaseGeneration(sessionId) === undefined) await c.joinSessionLease(sessionId, `conformance ${name}`);
      try {
        const payload = toBinary(StartTaskSchema, create(StartTaskSchema, { taskId: { value: Buffer.from(taskId, "hex") }, endpoint, model }));
        const ack = await c.command("StartTask", payload, undefined, c.leaseGeneration(sessionId));
        const r = fromBinary(TaskRunStartedSchema, ack.result);
        return { ok: true, value: { runId: Buffer.from(r.runId?.value ?? new Uint8Array()).toString("hex") } };
      } catch (e) {
        return refused(e);
      }
    },
    async events(sessionId, afterOffset, limit, timeoutMs) {
      const c = await current();
      return readByCursor(c, sessionId, afterOffset, limit, timeoutMs);
    },
    async disconnect() {
      client?.close();
      client = null;
    },
    async listApprovals(sessionId) {
      const c = await current();
      const l = await c.listApprovals(sessionId);
      return l.approvals.map((a) => ({ approvalId: Buffer.from(a.approvalId?.value ?? new Uint8Array()).toString("hex"), intentHash: a.intentHash, status: a.status, toolName: a.toolName }));
    },
    async resolveApproval(sessionId, approvalId, approve, intentHash) {
      const c = await current();
      if (c.leaseGeneration(sessionId) === undefined) await c.joinSessionLease(sessionId, `conformance ${name}`);
      try {
        const r = await c.resolveApproval(sessionId, approvalId, approve, "conformance", intentHash);
        return { ok: true, value: { status: r.status } };
      } catch (e) {
        return refused(e);
      }
    },
    async renderAttention(sessionId) {
      const c = await current();
      const v = await c.attention(sessionId);
      const items = v.items.map((i) => ({ kind: i.kind, reason: i.reason, action: i.action }));
      return { items, rendered: items.map((i) => `${i.kind}: ${i.reason} — ${i.action}`) };
    },
    async renderVerdict(taskId) {
      const c = await current();
      const v = await c.taskAssurance(taskId);
      const verdict = v.acceptance?.verdict ?? "";
      return { verdict, rendered: verdict ? `acceptance gate ${verdict} at revision ${v.acceptance?.candidateRevision ?? 0n}` : "no acceptance gate result yet" };
    },
    async close() {
      client?.close();
      client = null;
    },
  };
}
