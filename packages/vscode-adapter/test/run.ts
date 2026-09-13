/**
 * PX-E2E-002 runner: downloads a pinned VS Code, starts a scripted
 * OpenAI-compatible model in this process, and launches the real extension
 * host with the adapter on a fixture repository against a real local Core
 * (`MODBIT_CORE_BIN`). The suite runs inside VS Code (`suite.ts`).
 */
import { runTests } from "@vscode/test-electron";
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

// Bundled to CommonJS: the extension host loads dist/test/suite.cjs beside this file.
declare const __dirname: string;
const here = __dirname;
const extensionDevelopmentPath = resolve(here, "..", "..");
const coreBin = process.env.MODBIT_CORE_BIN;
if (!coreBin) {
  console.log("MODBIT_CORE_BIN unset: the extension-host test needs a built Core; skipped");
  process.exit(0);
}

function git(cwd: string, ...args: string[]): void {
  execFileSync("git", ["-C", cwd, ...args], { stdio: "ignore" });
}

const repo = mkdtempSync(join(tmpdir(), "modbit-vscode-repo-"));
// A committed type error: the built-in TypeScript service reports it without any edit.
writeFileSync(join(repo, "src.ts"), "export function total(q: number): number {\n  return q * 2;\n}\nexport const n: number = 'text';\n");
writeFileSync(join(repo, "notes.txt"), "line 1\nline 2\nline 3\n");
git(repo, "init", "-q", "-b", "main");
git(repo, "config", "core.autocrlf", "false");
git(repo, "add", "-A");
git(repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
const wt = `${repo}-wt`;
// The fixture task: plan, a protected worktree close (the approval), an edit, completion.
const script = [
  { calls: [{ name: "plan.update", args: { outcome: "annotate the notes", expected_files: ["notes.txt"], protected_effects: ["git.worktree.close"] } }] },
  { calls: [{ name: "fs.read", args: { path: "notes.txt" } }] },
  { calls: [{ name: "git.worktree.create", args: { branch: "t/vscode", path: wt } }] },
  { calls: [{ name: "git.worktree.close", args: { path: wt } }] },
  { calls: [{ name: "change.apply", args: { path: "notes.txt", op: "replace", content: "line 1\nline 2 annotated\nline 3\n" } }] },
  { calls: [{ name: "task.complete", args: { summary: "annotated", self_review: { findings: [] } } }] },
];
const server = createServer((req, res) => {
  let body = "";
  req.on("data", (c) => (body += c));
  req.on("end", () => {
    const parsed = JSON.parse(body || "{}") as { messages?: { role: string }[] };
    const results = (parsed.messages ?? []).filter((m) => m.role === "tool").length;
    const reply = script[results] ?? { calls: [] as { name: string; args: unknown }[] };
    res.writeHead(200, { "content-type": "text/event-stream" });
    const send = (o: unknown) => res.write(`data: ${JSON.stringify(o)}\n\n`);
    reply.calls.forEach((c, i) => send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { tool_calls: [{ index: i, id: `call_${results}_${i}`, type: "function", function: { name: c.name, arguments: JSON.stringify(c.args) } }] }, finish_reason: null }] }));
    if (reply.calls.length === 0) send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: "Nothing further." }, finish_reason: null }] });
    send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: reply.calls.length ? "tool_calls" : "stop" }], usage: { prompt_tokens: 10, completion_tokens: 5 } });
    res.write("data: [DONE]\n\n");
    res.end();
  });
});
server.listen(0, "127.0.0.1", async () => {
  const addr = server.address() as { port: number };
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-vscode-profile-"));
  try {
    await runTests({
      version: process.env.MODBIT_VSCODE_VERSION ?? "1.104.0",
      extensionDevelopmentPath,
      extensionTestsPath: resolve(here, "suite.cjs"),
      launchArgs: [repo, "--disable-extensions", "--disable-workspace-trust", "--disable-gpu", "--skip-welcome", "--skip-release-notes"],
      extensionTestsEnv: {
        MODBIT_CORE_BIN: coreBin,
        MODBIT_DATA_DIR: dataDir,
        MODBIT_OPENAI_BASE_URL: `http://127.0.0.1:${addr.port}`,
        OPENAI_API_KEY: "",
        ANTHROPIC_API_KEY: "",
        MODBIT_TEST_REPO: repo,
      },
    });
    console.log("extension-host suite passed");
    server.close();
    process.exit(0);
  } catch (e) {
    console.error(`extension-host suite failed: ${(e as Error).message}`);
    server.close();
    process.exit(1);
  }
});
