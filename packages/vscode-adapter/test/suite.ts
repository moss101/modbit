/**
 * Runs inside the VS Code extension host (PX-E2E-002): the real extension,
 * a real local Core (spawned by the adapter), a fixture repository and the
 * scripted model the runner started. Mocha is the harness VS Code loads.
 */
import * as vscode from "vscode";
import Mocha from "mocha";
import * as assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import type { ModbitAdapter } from "../src/adapter.ts";

async function until<T>(what: string, timeoutMs: number, probe: () => Promise<T | null | undefined>): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const v = await probe();
    if (v !== null && v !== undefined) return v;
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${what}`);
    await new Promise((r) => setTimeout(r, 300));
  }
}

export function run(): Promise<void> {
  const mocha = new Mocha({ ui: "bdd", color: false, timeout: 600_000 });
  mocha.suite.emit("pre-require", globalThis, "suite", mocha);
  const repo = process.env.MODBIT_TEST_REPO ?? "";

  describe("PX-E2E-002: the VS Code adapter drives a real task", () => {
    let adapter: ModbitAdapter;
    let taskId = "";

    it("activates against the real Core and lists no task yet", async () => {
      const ext = vscode.extensions.all.find((e) => (e.packageJSON as { name?: string }).name === "@modbit/vscode-adapter" || e.id.toLowerCase().includes("vscode-adapter"));
      assert.ok(ext, `the extension is installed in the host: ${vscode.extensions.all.map((e) => e.id).join(", ")}`);
      adapter = (await ext.activate()) as ModbitAdapter;
      assert.ok(adapter, "activate returns the adapter");
      await until("the session", 60_000, async () => {
        try {
          return adapter.session();
        } catch {
          return null;
        }
      });
      assert.equal(adapter.tasks.size, 0);
    });

    it("creates a task from the editor and the Core runs it to its protected effect", async () => {
      const r = (await vscode.commands.executeCommand("modbit.createTask", "annotate the notes")) as { taskId: string; runId: string };
      assert.match(r.taskId, /^[0-9a-f]{32}$/);
      taskId = r.taskId;
      const row = adapter.tasks.get(taskId);
      assert.ok(row && row.origin === "ide_adapter", JSON.stringify(row));
      await until("the approval", 240_000, async () => (await adapter.approvals()).find((a) => a.status === "REQUESTED") ?? null);
      // The approval's request and the task's wait are two events; the
      // adapter applies them in order, and a poll that saw the approval first
      // sees the wait a moment later (hosted macOS).
      await until("the task waiting on it", 30_000, async () => (adapter.tasks.get(taskId)?.state === "Waiting" ? true : null));
      assert.equal(adapter.tasks.get(taskId)?.state, "Waiting");
    });

    it("forwards the editor's language-service diagnostics with revision provenance", async () => {
      const uri = vscode.Uri.file(join(repo, "src.ts"));
      const doc = await vscode.workspace.openTextDocument(uri);
      await vscode.window.showTextDocument(doc);
      // The committed type error, reported by the built-in TypeScript service.
      await until("a TypeScript diagnostic", 120_000, async () => (vscode.languages.getDiagnostics(uri).length > 0 ? true : null));
      // An unsaved edit: the document's text is not the file on disk, so the
      // Core drops every diagnostic of it — provenance holds, nothing is written.
      const edit = new vscode.WorkspaceEdit();
      edit.insert(uri, new vscode.Position(0, 0), "// unsaved\n");
      await vscode.workspace.applyEdit(edit);
      const forwarded = (await vscode.commands.executeCommand("modbit.forwardDiagnostics", taskId)) as { recorded: number; discarded: number; workspaceRevision: string };
      assert.equal(forwarded.recorded, 0, JSON.stringify(forwarded));
      assert.ok(forwarded.discarded >= 1, JSON.stringify(forwarded));
      // Reverted, the document is the file again: the batch is recorded at the Core's revision.
      await vscode.commands.executeCommand("workbench.action.files.revert");
      await until("the reverted document", 30_000, async () => (doc.getText().startsWith("export function") ? true : null));
      const again = (await vscode.commands.executeCommand("modbit.forwardDiagnostics", taskId)) as { recorded: number; discarded: number; workspaceRevision: string };
      assert.ok(again.recorded >= 1, JSON.stringify(again));
      assert.match(again.workspaceRevision, /^\d+$/);
      assert.equal(readFileSync(join(repo, "src.ts"), "utf8").startsWith("export function"), true, "the adapter wrote nothing");
    });

    it("steers the task and decides the approval by the intent it showed", async () => {
      const s = (await vscode.commands.executeCommand("modbit.steer", taskId, "keep the note short")) as { sequence: bigint };
      assert.ok(s.sequence >= 1n);
      const pending = (await adapter.approvals()).find((a) => a.status === "REQUESTED");
      assert.ok(pending);
      const id = Buffer.from(pending.approvalId?.value ?? new Uint8Array()).toString("hex");
      const status = (await vscode.commands.executeCommand("modbit.approve", id, true)) as string;
      assert.equal(status, "APPROVED");
    });

    it("reviews the result from the editor and accepts it; the workspace was written only by the Core", async () => {
      await until("ReadyForReview", 240_000, async () => (adapter.tasks.get(taskId)?.state === "ReadyForReview" ? true : null));
      const view = (await vscode.commands.executeCommand("modbit.review", taskId)) as { files: number; workspaceRevision: string };
      assert.ok(view.files >= 1, JSON.stringify(view));
      const doc = vscode.window.activeTextEditor?.document;
      assert.ok(doc && doc.getText().includes("notes.txt") && doc.getText().includes("line 2 annotated"));
      const decided = (await vscode.commands.executeCommand("modbit.decideReview", taskId, "ACCEPT", "Annotate the notes")) as { taskState: string; commit: string };
      assert.equal(decided.taskState, "Completed");
      assert.match(decided.commit, /^[0-9a-f]{7,}$/);
      assert.equal(readFileSync(join(repo, "notes.txt"), "utf8"), "line 1\nline 2 annotated\nline 3\n");
      const c = (await vscode.commands.executeCommand("modbit.cursor")) as { cursor: string };
      assert.ok(BigInt(c.cursor) > 0n);
    });
  });

  return new Promise((resolve, reject) => {
    mocha.run((failures) => (failures ? reject(new Error(`${failures} failure(s)`)) : resolve()));
  });
}
