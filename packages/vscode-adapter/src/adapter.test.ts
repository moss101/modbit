/**
 * PX-002 (docs/29): the adapter behind the extension against a real Core —
 * the editor "restarts" mid-task (the adapter stops, its Core dies with it,
 * a new adapter starts on the same persisted state) and resumes the same
 * session from its cursor with the task still waiting where it was; then it
 * steers, decides the approval by the intent it showed, and reviews. Runs
 * when `MODBIT_CORE_BIN` names a built Core.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ModbitAdapter, type AdapterState } from "./adapter.ts";

const coreBin = process.env.MODBIT_CORE_BIN;

function scriptedModel(script: { calls: { name: string; args: unknown }[] }[]): Promise<{ server: Server; url: string }> {
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", () => {
        const parsed = JSON.parse(body || "{}") as { messages?: { role: string }[] };
        const results = (parsed.messages ?? []).filter((m) => m.role === "tool").length;
        const reply = script[results] ?? { calls: [] };
        res.writeHead(200, { "content-type": "text/event-stream" });
        const send = (o: unknown) => res.write(`data: ${JSON.stringify(o)}\n\n`);
        reply.calls.forEach((c, i) => send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { tool_calls: [{ index: i, id: `call_${results}_${i}`, type: "function", function: { name: c.name, arguments: JSON.stringify(c.args) } }] }, finish_reason: null }] }));
        if (reply.calls.length === 0) send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: "Nothing further." }, finish_reason: null }] });
        send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: reply.calls.length ? "tool_calls" : "stop" }], usage: { prompt_tokens: 10, completion_tokens: 5 } });
        res.write("data: [DONE]\n\n");
        res.end();
      });
    });
    server.listen(0, "127.0.0.1", () => resolveServer({ server, url: `http://127.0.0.1:${(server.address() as { port: number }).port}` }));
  });
}

async function until<T>(what: string, timeoutMs: number, probe: () => Promise<T | null>): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const v = await probe();
    if (v !== null) return v;
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${what}`);
    await new Promise((r) => setTimeout(r, 250));
  }
}

test("PX-E2E-002 (harness half): an editor restart mid-task resumes the session by cursor; steer, approve by intent, review", { skip: !coreBin && "MODBIT_CORE_BIN unset (runs in the desktop-e2e job)", timeout: 900_000 }, async () => {
  const repo = mkdtempSync(join(tmpdir(), "modbit-adapter-repo-"));
  writeFileSync(join(repo, "notes.txt"), "line 1\nline 2\nline 3\n");
  const git = (...args: string[]) => execFileSync("git", ["-C", repo, ...args], { stdio: "ignore" });
  git("init", "-q", "-b", "main");
  git("config", "core.autocrlf", "false");
  git("add", "-A");
  git("-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
  const wt = `${repo}-wt`;
  const { server, url } = await scriptedModel([
    { calls: [{ name: "plan.update", args: { outcome: "annotate", expected_files: ["notes.txt"], protected_effects: ["git.worktree.close"] } }] },
    { calls: [{ name: "fs.read", args: { path: "notes.txt" } }] },
    { calls: [{ name: "git.worktree.create", args: { branch: "t/adapter", path: wt } }] },
    { calls: [{ name: "git.worktree.close", args: { path: wt } }] },
    { calls: [{ name: "change.apply", args: { path: "notes.txt", op: "replace", content: "line 1\nline 2 annotated\nline 3\n" } }] },
    { calls: [{ name: "task.complete", args: { summary: "annotated", self_review: { findings: [] } } }] },
  ]);
  process.env.MODBIT_OPENAI_BASE_URL = url;
  process.env.OPENAI_API_KEY = "";
  process.env.ANTHROPIC_API_KEY = "";
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-adapter-profile-"));
  // The editor's persisted state: what survives a restart.
  const store = new Map<string, string>();
  const state: AdapterState = { get: (k) => store.get(k), set: (k, v) => void (v === undefined ? store.delete(k) : store.set(k, v)) };
  const events: string[] = [];
  const live: ModbitAdapter[] = [];
  const make = () => {
    const a = new ModbitAdapter({ coreBinary: coreBin!, dataDir, workspaceRoot: repo, state, build: "test", onEvent: (e) => events.push(`${e.offset}:${e.event?.eventType ?? ""}`), log: (l) => console.log(`[adapter] ${l}`) });
    live.push(a);
    return a;
  };
  try {
    const first = make();
    await first.start();
    const session = first.session();
    const created = await first.createTask("annotate the notes", "openai", "gpt-5");
    assert.equal(first.tasks.get(created.taskId)?.origin, "ide_adapter");
    await until("the approval", 240_000, async () => (await first.approvals()).find((a) => a.status === "REQUESTED") ?? null);
    const cursorBefore = first.currentCursor();
    assert.ok(cursorBefore > 0n);
    // The editor restarts: the adapter stops, its Core (tethered) goes with it.
    first.stop();
    await new Promise((r) => setTimeout(r, 1500));
    const second = make();
    await second.start();
    assert.equal(second.session(), session, "the same session is joined");
    assert.equal(store.get("modbit.session"), session);
    // Resumed by cursor: nothing before the cursor is delivered again, the task is where it was.
    const resumedFrom = BigInt(store.get("modbit.cursor") ?? "0");
    assert.ok(resumedFrom >= cursorBefore, `cursor ${resumedFrom} >= ${cursorBefore}`);
    const row = second.tasks.get(created.taskId);
    assert.ok(row, "the task is listed from the snapshot");
    assert.equal(row.state, "Waiting");
    const pending = await until("the approval after the restart", 60_000, async () => (await second.approvals()).find((a) => a.status === "REQUESTED") ?? null);
    const id = Buffer.from(pending.approvalId?.value ?? new Uint8Array()).toString("hex");
    // Steer, then decide by the intent shown; a decision naming another intent is refused.
    const steered = await second.steer(created.taskId, "keep it short");
    assert.ok(steered.sequence >= 1n);
    const other = (pending.intentHash.startsWith("0") ? "1" : "0") + pending.intentHash.slice(1);
    await assert.rejects(second.decideApproval(id, true, other), /INTENT_MISMATCH/);
    assert.equal(await second.decideApproval(id, true, pending.intentHash), "APPROVED");
    // The Core suspended the run at the restart (RESTART_AWAITING_APPROVAL: "resolve the
    // open approval and resume with StartTask"); the editor resumes it — the same run.
    const attention = await second.attention();
    assert.ok(attention.items.some((i) => i.kind === "RESTART" || i.action.includes("StartTask")), attention.items.map((i) => `${i.kind}: ${i.reason} -> ${i.action}`).join(" | ") || "no attention items");
    const resumed = await second.resume(created.taskId, "openai", "gpt-5");
    assert.equal(resumed.resumed, true);
    await until("ReadyForReview", 240_000, async () => (second.tasks.get(created.taskId)?.state === "ReadyForReview" ? true : null));
    const bundle = await second.review(created.taskId);
    assert.ok(bundle.files.some((f) => f.path === "notes.txt"));
    const decided = await second.decideReview(created.taskId, "ACCEPT", "Annotate", bundle.workspaceRevision);
    assert.equal(decided.taskState, "Completed");
    // The panel follows the log: the completion arrives by cursor.
    await until("the panel to show Completed", 30_000, async () => (second.tasks.get(created.taskId)?.state === "Completed" ? true : null));
    // Every event the second adapter saw is past the cursor it resumed from.
    const seenAfter = events.filter((e) => BigInt(e.split(":")[0]!) > cursorBefore);
    assert.ok(seenAfter.length > 0);
    second.stop();
  } finally {
    for (const a of live) a.stop();
    server.close();
  }
});
