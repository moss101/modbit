/**
 * PX-001 / PX-E2E-001: the conformance suite against the CLI and the shared
 * protocol client on a real Core, and against deliberately broken clients
 * that must fail it; the static proof over the first-party clients.
 *
 * The behavioral half runs when `MODBIT_CORE_BIN` names a built Core (CI's
 * desktop-e2e job; locally after `cargo build -p modbit-core -p modbit-execd
 * -p modbit-cli`); the static half always runs.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { ClientKind, CoreClient, CoreSupervisor } from "./index.ts";
import { THIN_CLIENT_RULE, clientSubject, runConformance, scanCargoClosure, scanClientSources, type ApprovalRow, type ConformanceReport, type ConformanceSubject, type CursorEvent, type Outcome } from "./conformance.ts";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..", "..", "..");
const coreBin = process.env.MODBIT_CORE_BIN;
const cliBin = process.env.MODBIT_CLI_BIN ?? (coreBin ? join(dirname(coreBin), process.platform === "win32" ? "modbit-cli.exe" : "modbit-cli") : undefined);

// ---- Static proof ---------------------------------------------------------

/** The first-party clients and their sanctioned exceptions. */
const CLIENTS = [
  {
    root: join(repoRoot, "packages", "ide-adapter-core"),
    allow: [{ file: "src/conformance.ts", specifier: "node:fs", why: "the static scan reads client sources; it touches no workspace" }],
  },
  {
    root: join(repoRoot, "apps", "desktop"),
    allow: [
      { file: "src/main/main.ts", specifier: "node:fs", why: "profile-local desktop state (the session id) under the app's own data dir; never a workspace file" },
      { file: "src/main/credentials.ts", specifier: "node:fs", why: "M7.8: the credential broker's ciphertext records (safeStorage) under the app's own data dir; never a workspace file" },
    ],
  },
  { root: join(repoRoot, "packages", "vscode-adapter"), allow: [] },
];

test("static proof: the first-party thin clients link no provider, workspace filesystem, Git or policy code", () => {
  for (const c of CLIENTS) {
    const findings = scanClientSources(c.root, { ...THIN_CLIENT_RULE, allow: c.allow });
    assert.deepEqual(findings, [], `${c.root}: ${JSON.stringify(findings, null, 1)}`);
  }
});

test("static proof: a client that links a Git library, a provider SDK or reads a credential fails the scan", () => {
  const dir = mkdtempSync(join(tmpdir(), "modbit-broken-client-"));
  mkdirSync(join(dir, "src"));
  writeFileSync(join(dir, "src", "index.ts"), `import simpleGit from "simple-git";\nimport OpenAI from "openai";\nimport { spawn } from "node:child_process";\nexport const key = process.env.OPENAI_API_KEY;\nspawn("git", ["status"]);\n`);
  const findings = scanClientSources(dir);
  const whats = findings.map((f) => f.what);
  assert.ok(whats.some((w) => w.includes("simple-git")), JSON.stringify(findings));
  assert.ok(whats.some((w) => w.includes("openai")), JSON.stringify(findings));
  assert.ok(whats.some((w) => w.includes("OPENAI_API_KEY")), JSON.stringify(findings));
  assert.ok(whats.some((w) => w.startsWith("spawn(")), JSON.stringify(findings));
  assert.equal(scanClientSources(dir, { ...THIN_CLIENT_RULE, allow: [{ file: "src/index.ts", specifier: "simple-git", why: "test" }] }).filter((f) => f.what.includes("simple-git")).length, 0);
});

test("static proof: a Rust client whose dependency closure reaches a forbidden crate fails the scan", () => {
  const meta = {
    packages: [
      { id: "cli 0.0.0", name: "modbit-cli" },
      { id: "proto 0.0.0", name: "modbit-protocol" },
      { id: "prov 0.0.0", name: "modbit-providers" },
    ],
    resolve: { nodes: [{ id: "cli 0.0.0", deps: [{ pkg: "proto 0.0.0" }] }, { id: "proto 0.0.0", deps: [{ pkg: "prov 0.0.0" }] }, { id: "prov 0.0.0", deps: [] }] },
  };
  const findings = scanCargoClosure(meta, "modbit-cli", ["modbit-providers"]);
  assert.equal(findings.length, 1);
  assert.match(findings[0]!.why, /modbit-protocol → modbit-providers/);
  assert.deepEqual(scanCargoClosure(meta, "modbit-cli", ["modbit-git"]), []);
});

/** Crates a thin client must never reach: providers, workspace files, Git, policy, tools, the runtime. */
const FORBIDDEN_CRATES = ["modbit-providers", "modbit-workspace", "modbit-git", "modbit-policy", "modbit-tools", "modbit-core-runtime", "modbit-event-store", "modbit-terminal", "modbit-sandbox", "modbit-effects", "modbit-secrets"];

test("static proof: modbit-cli's dependency closure reaches no provider, workspace, Git, policy or tool crate", { skip: !coreBin && "MODBIT_CORE_BIN unset (runs in the desktop-e2e job)" }, () => {
  const run = (args: string[]) => spawnSync("cargo", ["metadata", "--format-version", "1", ...args], { cwd: repoRoot, encoding: "utf8", maxBuffer: 256 * 1024 * 1024 });
  let out = run(["--offline", "--locked"]);
  if (out.status !== 0) out = run(["--locked"]);
  assert.equal(out.status, 0, out.stderr);
  const meta = JSON.parse(out.stdout) as Parameters<typeof scanCargoClosure>[0];
  assert.deepEqual(scanCargoClosure(meta, "modbit-cli", FORBIDDEN_CRATES), []);
});

// ---- Behavioral cases on a real Core ------------------------------------

/** Scripted OpenAI-compatible model: the reply is chosen by the number of tool results in the request. */
function scriptedModel(script: { text?: string; calls?: { name: string; args: unknown }[] }[]): Promise<{ server: Server; url: string }> {
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", () => {
        const parsed = JSON.parse(body || "{}") as { messages?: { role: string }[] };
        const results = (parsed.messages ?? []).filter((m) => m.role === "tool").length;
        const reply = script[results] ?? { text: "Nothing further." };
        res.writeHead(200, { "content-type": "text/event-stream" });
        const send = (o: unknown) => res.write(`data: ${JSON.stringify(o)}\n\n`);
        if (reply.text) send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: reply.text }, finish_reason: null }] });
        (reply.calls ?? []).forEach((c, i) => send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { tool_calls: [{ index: i, id: `call_${results}_${i}`, type: "function", function: { name: c.name, arguments: JSON.stringify(c.args) } }] }, finish_reason: null }] }));
        send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: reply.calls?.length ? "tool_calls" : "stop" }], usage: { prompt_tokens: 10, completion_tokens: 5 } });
        res.write("data: [DONE]\n\n");
        res.end();
      });
    });
    server.listen(0, "127.0.0.1", () => {
      const addr = server.address() as { port: number };
      resolveServer({ server, url: `http://127.0.0.1:${addr.port}` });
    });
  });
}

function git(cwd: string, ...args: string[]): string {
  return execFileSync("git", ["-C", cwd, ...args], { encoding: "utf8" });
}

/** A fresh fixture repository for one conformance run (its worktree is `${repo}-wt`). */
function fixtureRepo(): string {
  const repo = mkdtempSync(join(tmpdir(), "modbit-conformance-repo-"));
  writeFileSync(join(repo, "a.txt"), "a\n");
  git(repo, "init", "-q", "-b", "main");
  git(repo, "config", "core.autocrlf", "false");
  git(repo, "add", "-A");
  git(repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
  return repo;
}

/** A Core for one conformance run: spawned by the shared supervisor, reachable by every subject through its data dir. */
async function spawnCore(env: Record<string, string>): Promise<{ dataDir: string; connect: () => Promise<CoreClient>; stop: () => Promise<void> }> {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-conformance-core-"));
  for (const [k, v] of Object.entries(env)) process.env[k] = v;
  let ready: ((c: CoreClient) => void) | null = null;
  const first = new Promise<CoreClient>((r) => (ready = r));
  const supervisor = new CoreSupervisor(coreBin!, dataDir, { status() {}, client(c) { ready?.(c); ready = null; } }, "conformance", ClientKind.IDE_ADAPTER);
  await supervisor.start();
  const client = await first;
  const connect = () => {
    const line = supervisor.readyLine();
    if (!line) throw new Error("the Core is not in service");
    return CoreClient.connect(line, ClientKind.IDE_ADAPTER, "conformance");
  };
  return {
    dataDir,
    connect,
    stop: async () => {
      client.close();
      supervisor.stop();
      await new Promise((r) => setTimeout(r, 500));
    },
  };
}

/** The CLI, the contract's reference implementation, driven as a person or a script drives it. */
function cliSubject(dataDir: string, env: Record<string, string>): ConformanceSubject {
  const run = (args: string[]): { code: number; out: string; err: string } => {
    const r = spawnSync(cliBin!, ["--data-dir", dataDir, ...args], { encoding: "utf8", env: { ...process.env, ...env } });
    return { code: r.status ?? -1, out: r.stdout ?? "", err: r.stderr ?? "" };
  };
  const refusal = (r: { code: number; out: string; err: string }): Outcome<never> => {
    const m = /command rejected ([A-Z_]+): (.*)/.exec(r.err + r.out);
    return { ok: false, code: m?.[1] ?? `EXIT_${r.code}`, message: m?.[2]?.trim() ?? (r.err + r.out).trim() };
  };
  const parseApprovals = (out: string): ApprovalRow[] =>
    out
      .split("\n")
      .filter((l) => l.startsWith("approval "))
      .map((l) => {
        const kv = new Map(l.split(" ").slice(2).map((p) => p.split("=", 2) as [string, string]));
        return { approvalId: l.split(" ")[1] ?? "", status: kv.get("status") ?? "", toolName: kv.get("tool") ?? "", intentHash: kv.get("intent") ?? "" };
      });
  return {
    name: "modbit-cli",
    async createSession() {
      const r = run(["session", "create"]);
      if (r.code !== 0) throw new Error(r.err);
      return { sessionId: r.out.trim().replace(/^session /, ""), replayed: false };
    },
    async createTask(sessionId, goal, workspaceRoot, commandId) {
      const r = run(["task", "create", "--session", sessionId, "--workspace", workspaceRoot, "--command-id", commandId, goal]);
      if (r.code !== 0) throw new Error(r.err);
      const [, id, flag] = /^task ([0-9a-f]{32})( replayed)?/.exec(r.out.trim()) ?? [];
      return { taskId: id ?? "", replayed: flag !== undefined };
    },
    async startTask(sessionId, taskId, endpoint, model) {
      const r = run(["task", "run", "--session", sessionId, "--task", taskId, "--endpoint", endpoint, "--model", model]);
      if (r.code !== 0) return refusal(r);
      return { ok: true, value: { runId: r.out.trim() } };
    },
    async events(sessionId, afterOffset, limit, timeoutMs) {
      const deadline = Date.now() + timeoutMs;
      for (;;) {
        const r = run(["events", "tail", "--session", sessionId, "--after", afterOffset.toString(), "--count", String(limit), "--json"]);
        if (r.code !== 0) throw new Error(r.err);
        const lines: CursorEvent[] = r.out
          .split("\n")
          .filter((l) => l.startsWith("{"))
          .map((l) => JSON.parse(l) as { offset: number; event_type: string; task_id: string | null })
          .map((e) => ({ offset: BigInt(e.offset), eventType: e.event_type, taskId: e.task_id ?? "" }));
        if (lines.length > 0 || Date.now() > deadline) return lines;
        await new Promise((res) => setTimeout(res, 300));
      }
    },
    async disconnect() {
      // Every invocation is its own connection; the cursor is the caller's.
    },
    async listApprovals(sessionId) {
      const r = run(["approval", "list", "--session", sessionId]);
      if (r.code !== 0) throw new Error(r.err);
      return parseApprovals(r.out);
    },
    async resolveApproval(sessionId, approvalId, approve, intentHash) {
      const r = run(["approval", "resolve", "--session", sessionId, "--approval", approvalId, "--intent", intentHash, approve ? "approve" : "deny", "conformance"]);
      if (r.code !== 0) return refusal(r);
      return { ok: true, value: { status: /status=([A-Z_]+)/.exec(r.out)?.[1] ?? "" } };
    },
    async renderAttention(sessionId) {
      const r = run(["attention", "list", "--session", sessionId]);
      if (r.code !== 0) throw new Error(r.err);
      const lines = r.out.split("\n");
      const items: { kind: string; reason: string; action: string }[] = [];
      const rendered: string[] = [];
      for (let i = 0; i < lines.length; i++) {
        const head = /^ {2}([A-Z_]+) task=/.exec(lines[i] ?? "");
        if (!head) continue;
        const reason = (lines[i + 1] ?? "").trim();
        const action = (lines[i + 2] ?? "").trim().replace(/^-> /, "");
        items.push({ kind: head[1]!, reason, action });
        rendered.push(`${lines[i]}\n${lines[i + 1]}\n${lines[i + 2]}`);
      }
      return { items, rendered };
    },
    async renderVerdict(taskId) {
      const r = run(["task", "assurance", "--task", taskId]);
      if (r.code !== 0) throw new Error(r.err);
      const line = r.out.split("\n").find((l) => l.startsWith("acceptance_gate ")) ?? "";
      return { verdict: line.split(" ")[1] ?? "", rendered: line };
    },
    async close() {},
  };
}

/** Wrappers that break one rule each; the suite must fail exactly there. */
function broken(base: ConformanceSubject, how: "fresh-id-on-retry" | "replay-from-zero" | "ignores-intent" | "swallows-refusal"): ConformanceSubject {
  return {
    ...base,
    name: `${base.name} (${how})`,
    createTask: (s, g, w, id) => base.createTask(s, g, w, how === "fresh-id-on-retry" ? id.slice(0, 16) + Math.floor(Math.random() * 1e15).toString(16).padStart(16, "0") : id),
    events: (s, after, limit, t) => base.events(s, how === "replay-from-zero" ? 0n : after, limit, t),
    resolveApproval: (s, a, ok, intent) => base.resolveApproval(s, a, ok, how === "ignores-intent" ? "" : intent),
    startTask: async (s, t, e, m) => {
      const r = await base.startTask(s, t, e, m);
      return how === "swallows-refusal" && !r.ok ? { ok: false, code: "ERROR", message: "something went wrong" } : r;
    },
  };
}

function failures(r: ConformanceReport): string[] {
  return r.cases.filter((c) => c.status === "FAIL").map((c) => c.id);
}

test("PX-E2E-001: the shared protocol client and the CLI pass every conformance case on a real Core; broken clients fail theirs", { skip: !coreBin && "MODBIT_CORE_BIN unset (runs in the desktop-e2e job)", timeout: 1_500_000 }, async () => {
  // One scripted model for all runs: it answers by the count of tool results
  // and names no path, so every run gets its own repository and worktree.
  const wtOf = new Map<string, string>();
  const { server, url } = await scriptedModel([]);
  server.removeAllListeners("request");
  server.on("request", (req, res) => {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      const parsed = JSON.parse(body || "{}") as { messages?: { role: string; content?: string }[] };
      const results = (parsed.messages ?? []).filter((m) => m.role === "tool").length;
      // The run's repository is named in the system/user prompt: the goal carries the fixture key.
      const text = JSON.stringify(parsed.messages ?? []);
      const key = [...wtOf.keys()].find((k) => text.includes(k)) ?? "";
      const wt = wtOf.get(key) ?? "";
      const script = [
        { calls: [{ name: "plan.update", args: { outcome: "conformance fixture", expected_files: ["out.txt"], protected_effects: ["git.worktree.close"] } }] },
        { calls: [{ name: "change.apply", args: { path: "out.txt", op: "create", content: "hi\n" } }] },
        { calls: [{ name: "git.worktree.create", args: { branch: "t/conformance", path: wt } }] },
        { calls: [{ name: "git.worktree.close", args: { path: wt } }] },
        { calls: [{ name: "task.complete", args: { summary: "done", self_review: { findings: [] } } }] },
      ];
      const reply = script[results] ?? { calls: [] as { name: string; args: unknown }[], text: "Nothing further." };
      res.writeHead(200, { "content-type": "text/event-stream" });
      const send = (o: unknown) => res.write(`data: ${JSON.stringify(o)}\n\n`);
      if ("text" in reply && reply.text) send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: reply.text }, finish_reason: null }] });
      reply.calls.forEach((c, i) => send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { tool_calls: [{ index: i, id: `call_${results}_${i}`, type: "function", function: { name: c.name, arguments: JSON.stringify(c.args) } }] }, finish_reason: null }] }));
      send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: reply.calls.length ? "tool_calls" : "stop" }], usage: { prompt_tokens: 10, completion_tokens: 5 } });
      res.write("data: [DONE]\n\n");
      res.end();
    });
  });
  const env = { MODBIT_OPENAI_BASE_URL: url, OPENAI_API_KEY: "", ANTHROPIC_API_KEY: "" };
  const core = await spawnCore(env);
  try {
    let runs = 0;
    const conformance = (subject: ConformanceSubject) => {
      const repo = fixtureRepo();
      const key = `fixture-${++runs}-${Math.floor(Math.random() * 1e9).toString(16)}`;
      wtOf.set(key, `${repo}-wt`);
      return runConformance(subject, { reference: core.connect, workspaceRoot: repo, goal: `conformance ${key}`, endpoint: "openai", model: "gpt-5", timeoutMs: 240_000 });
    };
    const report = (r: ConformanceReport) => `${r.subject}:\n${r.cases.map((c) => `  ${c.status} ${c.id}: ${c.detail}`).join("\n")}`;
    // The shared client, as Electron main and an IDE adapter use it, and the
    // CLI, the reference implementation — each in its own session, side by
    // side (the Core admits four model runs at once by default).
    const [shared, cli] = await Promise.all([conformance(clientSubject("shared protocol client", core.connect)), conformance(cliSubject(core.dataDir, env))]);
    console.log(report(shared));
    console.log(report(cli));
    assert.ok(shared.passed, report(shared));
    assert.ok(cli.passed, report(cli));
    // A deliberately broken client fails the suite — at the case it breaks.
    const [b1, b2, b3, b4] = await Promise.all([
      conformance(broken(clientSubject("shared", core.connect), "fresh-id-on-retry")),
      conformance(broken(clientSubject("shared", core.connect), "replay-from-zero")),
      conformance(broken(clientSubject("shared", core.connect), "ignores-intent")),
      conformance(broken(clientSubject("shared", core.connect), "swallows-refusal")),
    ]);
    assert.deepEqual(failures(b1), ["idempotency"], report(b1));
    assert.ok(failures(b2).includes("cursor-replay"), report(b2));
    assert.ok(failures(b3).includes("intent-bound-approval"), report(b3));
    assert.ok(failures(b4).includes("rejection-paths"), report(b4));
  } finally {
    await core.stop();
    server.close();
  }
});
