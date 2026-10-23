/**
 * PX-100 (IDE-adapter half): the shared client sets a task's mode and
 * execution preference through the same typed commands as the desktop and the
 * CLI, and shows the Core's answer. The Core is the only decider: a task
 * created ASK is refused a write by the Capability Kernel, a value the Core
 * does not know comes back as its typed error, and a mode set from the CLI is
 * what the adapter reads after a Core restart.
 *
 * Runs when `MODBIT_CORE_BIN` names a built Core (CI's desktop-e2e job;
 * locally after `cargo build -p modbit-core -p modbit-execd -p modbit-cli`).
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { ObjectiveProfile, TaskMode } from "@modbit/surface-protocol";
import { ClientKind, CoreClient, CoreSupervisor, RejectedError } from "./index.ts";

const coreBin = process.env.MODBIT_CORE_BIN;
const cliBin = process.env.MODBIT_CLI_BIN ?? (coreBin ? join(dirname(coreBin), process.platform === "win32" ? "modbit-cli.exe" : "modbit-cli") : undefined);

/** A scripted OpenAI-compatible model that keeps every request body it was sent. */
function scriptedModel(script: { text?: string; calls?: { name: string; args: unknown }[] }[]): Promise<{ server: Server; url: string; seen: Record<string, unknown>[] }> {
  const seen: Record<string, unknown>[] = [];
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", () => {
        const parsed = JSON.parse(body || "{}") as { messages?: { role: string }[] };
        seen.push(parsed as Record<string, unknown>);
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
      resolveServer({ server, url: `http://127.0.0.1:${addr.port}`, seen });
    });
  });
}

function git(cwd: string, ...args: string[]): void {
  execFileSync("git", ["-C", cwd, ...args], { encoding: "utf8" });
}

/** A committed fixture repository with a configured no-op check (FIX-03). */
function fixtureRepo(): string {
  const repo = mkdtempSync(join(tmpdir(), "modbit-modes-repo-"));
  writeFileSync(join(repo, "a.txt"), "a\n");
  mkdirSync(join(repo, ".modbit"), { recursive: true });
  writeFileSync(join(repo, ".modbit", "verification.json"), JSON.stringify({ commands: [{ id: "fixture-noop", argv: ["git", "--version"] }] }));
  git(repo, "init", "-q", "-b", "main");
  git(repo, "config", "core.autocrlf", "false");
  git(repo, "add", "-A");
  git(repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
  return repo;
}

/** A Core on `dataDir` under the shared supervisor, and a client to it. */
async function coreOn(dataDir: string): Promise<{ client: CoreClient; stop: () => Promise<void> }> {
  let ready: ((c: CoreClient) => void) | null = null;
  const first = new Promise<CoreClient>((r) => (ready = r));
  const supervisor = new CoreSupervisor(coreBin!, dataDir, { status() {}, client(c) { ready?.(c); ready = null; } }, "modes-test", ClientKind.IDE_ADAPTER);
  await supervisor.start();
  const client = await first;
  return {
    client,
    stop: async () => {
      client.close();
      supervisor.stop();
      await new Promise((r) => setTimeout(r, 500));
    },
  };
}

async function refusal(p: Promise<unknown>): Promise<string> {
  try {
    await p;
  } catch (e) {
    if (e instanceof RejectedError) return e.code;
    throw e;
  }
  throw new Error("expected a typed refusal");
}

async function untilIdle(client: CoreClient, taskId: string): Promise<string> {
  for (let i = 0; i < 600; i++) {
    const st = await client.taskStatus(taskId);
    if (!st.loopAlive) return st.state;
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error("the run never ended");
}

test("PX-100: the adapter sets mode and preference through the Core, the Core decides, and a mode set from the CLI shows after a restart", { skip: !coreBin && "MODBIT_CORE_BIN unset (runs in the desktop-e2e job)" }, async () => {
  const repo = fixtureRepo();
  const script = [
    { calls: [{ name: "plan.update", args: { outcome: "write b.txt", expected_files: ["b.txt"] } }] },
    // Asked for anyway: the Core refuses it at the Capability Kernel.
    { calls: [{ name: "change.apply", args: { path: "b.txt", op: "replace", content: "b\n" } }] },
    { calls: [{ name: "task.complete", args: { summary: "planned", self_review: { findings: [] } } }] },
  ];
  const model = await scriptedModel(script);
  process.env.MODBIT_OPENAI_BASE_URL = model.url;
  process.env.OPENAI_API_KEY = "";
  process.env.ANTHROPIC_API_KEY = "";
  process.env.MODBIT_OPENAI_MODELS = "gpt-fx=1/2;ctx=200000;out=65536;reasoning=true;effort=low;tier=flex";
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-modes-core-"));
  let core = await coreOn(dataDir);
  try {
    const { sessionId } = await core.client.createSession();
    await core.client.acquireSessionLease(sessionId, "modes-test");
    // The mode, profile and preference are named at creation; the Core derives the rest.
    const { taskId } = await core.client.createTask(sessionId, "write b.txt", undefined, repo, undefined, {
      mode: TaskMode.ASK,
      objective: ObjectiveProfile.COST,
      effort: "high",
    });
    let posture = await core.client.taskPosture(taskId);
    assert.equal(posture.mode, TaskMode.ASK);
    assert.equal(posture.modeInForce, TaskMode.ASK);
    assert.equal(posture.posture?.writes, false);
    assert.equal(posture.preference?.objective, ObjectiveProfile.COST);
    assert.equal(posture.preference?.effort, "high");
    // With no signed registry the routing outcome is DIRECT, with a typed reason.
    assert.equal(posture.routing?.outcome, "DIRECT");
    assert.equal(posture.routing?.reasonCode, "NO_ACTIVE_REGISTRY");
    const snapshot = await core.client.getSessionSnapshot(sessionId);
    assert.equal(snapshot.tasks.find((t) => hexOf(t.taskId?.value) === taskId)?.mode, TaskMode.ASK);

    // The run: the tier rides on the start, the effort from creation; the
    // write the model asks for is refused by the Core and the file is untouched.
    await core.client.startTask(sessionId, taskId, { model: "gpt-fx", serviceTier: "priority" });
    assert.equal(await untilIdle(core.client, taskId), "ReadyForReview");
    assert.equal(existsSync(join(repo, "b.txt")), false, "an ASK task wrote a file");
    const first = model.seen[0] as { reasoning_effort?: string; service_tier?: string; tools?: { function: { name: string } }[] };
    assert.equal(first.reasoning_effort, "high");
    assert.equal(first.service_tier, "priority");
    const offered = model.seen.flatMap((b) => ((b as typeof first).tools ?? []).map((t) => t.function.name));
    assert.ok(!offered.includes("change.apply"), "the write tool was offered to an ASK task");
    posture = await core.client.taskPosture(taskId);
    assert.equal(posture.preference?.effortApplied, "high");
    assert.equal(posture.routing?.outcome, "DIRECT");

    // Typed refusals come back from the Core, not from the client.
    assert.equal(await refusal(core.client.setTaskMode(sessionId, taskId, TaskMode.UNSPECIFIED)), "MODE_REQUIRED");
    assert.equal(await refusal(core.client.setTaskMode(sessionId, taskId, 99 as TaskMode)), "UNKNOWN_MODE");
    assert.equal(await refusal(core.client.setExecutionPreference(sessionId, taskId, { effort: "extreme" })), "INVALID_EFFORT");
    assert.equal(await refusal(core.client.setExecutionPreference(sessionId, taskId, {})), "EMPTY_PREFERENCE");
    assert.equal(await refusal(core.client.setExecutionPreference(sessionId, taskId, { pin: { endpoint: "openai", model: "no-such-model" } })), "UNKNOWN_MODEL");
    assert.equal((await core.client.taskPosture(taskId)).mode, TaskMode.ASK, "a refusal changes nothing");

    // A typed change, and a preference patch.
    const changed = await core.client.setTaskMode(sessionId, taskId, TaskMode.AGENT);
    assert.equal(changed.previousMode, TaskMode.ASK);
    assert.equal(changed.effective, "IMMEDIATE");
    const set = await core.client.setExecutionPreference(sessionId, taskId, { serviceTier: "flex" });
    assert.equal(set.preference?.serviceTier, "flex");
    assert.equal(set.preference?.effort, "high", "a patch keeps what it does not name");
    assert.equal(set.routing?.reasonCode, "NO_ACTIVE_REGISTRY");

    // The CLI sets the mode (its own Core, on the same profile); the adapter
    // reads it from a restarted Core.
    await core.stop();
    const set2 = spawnSync(cliBin!, ["--data-dir", dataDir, "task", "mode", "--session", sessionId, "--task", taskId, "plan"], { encoding: "utf8", env: process.env });
    assert.equal(set2.status, 0, set2.stderr + set2.stdout);
    core = await coreOn(dataDir);
    posture = await core.client.taskPosture(taskId);
    assert.equal(posture.mode, TaskMode.PLAN);
    assert.equal(posture.preference?.serviceTier, "flex");
    assert.equal(posture.preference?.objective, ObjectiveProfile.COST);
  } finally {
    await core.stop();
    model.server.close();
  }
});

function hexOf(b: Uint8Array | undefined): string {
  return Array.from(b ?? new Uint8Array(), (x) => x.toString(16).padStart(2, "0")).join("");
}
