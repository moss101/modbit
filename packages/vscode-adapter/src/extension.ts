/**
 * VS Code hosting of the Modbit adapter (PX-002; docs/29): commands, the
 * task panel, diagnostics forwarding and the persisted session/cursor. The
 * editor's own features are untouched; every action is a Core command the
 * adapter issues through the shared thin client. No workspace write, no tool,
 * no Git, no provider credential has a code path here.
 */
import * as vscode from "vscode";
import { basename, join, relative } from "node:path";
import { ModbitAdapter, type EditorDiagnostic, type TaskRow } from "./adapter.ts";

class TaskTree implements vscode.TreeDataProvider<TaskRow> {
  private readonly changed = new vscode.EventEmitter<void>();
  readonly onDidChangeTreeData = this.changed.event;
  constructor(private readonly adapter: ModbitAdapter) {}
  refresh(): void {
    this.changed.fire();
  }
  getTreeItem(row: TaskRow): vscode.TreeItem {
    const item = new vscode.TreeItem(row.goal || row.taskId.slice(0, 8), vscode.TreeItemCollapsibleState.None);
    item.id = row.taskId;
    item.description = row.state;
    item.contextValue = row.state;
    item.tooltip = `${row.taskId} · ${row.state} · ${row.origin}`;
    return item;
  }
  getChildren(): TaskRow[] {
    return [...this.adapter.tasks.values()];
  }
}

const SEVERITY = ["error", "warning", "information", "hint"] as const;

export async function activate(context: vscode.ExtensionContext): Promise<ModbitAdapter | undefined> {
  const folder = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  const output = vscode.window.createOutputChannel("Modbit");
  context.subscriptions.push(output);
  if (!folder) {
    output.appendLine("no workspace folder open; Modbit stays idle");
    return undefined;
  }
  const config = vscode.workspace.getConfiguration("modbit");
  const coreBinary = process.env.MODBIT_CORE_BIN ?? config.get<string>("coreBinary", "modbit-core");
  const dataDir = process.env.MODBIT_DATA_DIR ?? join(context.globalStorageUri.fsPath, "profile");
  const state = {
    get: (k: string) => context.workspaceState.get<string>(k),
    set: (k: string, v: string | undefined) => context.workspaceState.update(k, v),
  };
  const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 50);
  status.text = "$(sync~spin) Modbit: starting Core";
  status.show();
  context.subscriptions.push(status);
  const adapter = new ModbitAdapter({
    coreBinary,
    dataDir,
    workspaceRoot: folder,
    state,
    build: String((context.extension.packageJSON as { version?: string }).version ?? "0"),
    log: (line) => output.appendLine(line),
    onStatus: (s) => {
      status.text = s.state === "connected" ? "$(check) Modbit: Core connected" : s.state === "restarting" ? "$(warning) Modbit: Core restarting" : s.state === "failed" ? "$(error) Modbit: Core failed" : "$(sync~spin) Modbit: starting Core";
      status.tooltip = "reason" in s ? s.reason : s.state;
    },
    onEvent: (e) => {
      if (e.event?.taskId) tree.refresh();
      status.tooltip = `cursor ${e.offset}`;
    },
  });
  const tree = new TaskTree(adapter);
  context.subscriptions.push(vscode.window.registerTreeDataProvider("modbit.tasks", tree));
  context.subscriptions.push({ dispose: () => adapter.stop() });

  const pickTask = async (states?: string[]): Promise<TaskRow | undefined> => {
    const rows = [...adapter.tasks.values()].filter((r) => !states || states.includes(r.state));
    if (rows.length === 0) {
      void vscode.window.showInformationMessage("Modbit: no matching task");
      return undefined;
    }
    const picked = await vscode.window.showQuickPick(rows.map((r) => ({ label: r.goal || r.taskId.slice(0, 8), description: r.state, row: r })), { placeHolder: "Task" });
    return picked?.row;
  };
  const surface = async <T>(what: string, body: () => Promise<T>): Promise<T | undefined> => {
    try {
      return await body();
    } catch (e) {
      const msg = (e as Error).message;
      output.appendLine(`${what}: ${msg}`);
      void vscode.window.showErrorMessage(`Modbit ${what}: ${msg}`);
      return undefined;
    }
  };

  context.subscriptions.push(
    vscode.commands.registerCommand("modbit.createTask", async (goalArg?: string) => {
      const goal = goalArg ?? (await vscode.window.showInputBox({ prompt: "What should Modbit do in this workspace?", ignoreFocusOut: true }));
      if (!goal) return undefined;
      const r = await surface("create task", () => adapter.createTask(goal));
      tree.refresh();
      if (r) output.appendLine(`task ${r.taskId} started (run ${r.runId})`);
      return r;
    }),
    vscode.commands.registerCommand("modbit.steer", async (taskIdArg?: string, textArg?: string) => {
      const row = taskIdArg ? adapter.tasks.get(taskIdArg) : await pickTask(["Running", "Waiting", "Queued"]);
      if (!row) return undefined;
      const text = textArg ?? (await vscode.window.showInputBox({ prompt: `Steer "${row.goal}"`, ignoreFocusOut: true }));
      if (!text) return undefined;
      return surface("steer", () => adapter.steer(row.taskId, text));
    }),
    vscode.commands.registerCommand("modbit.approve", async (approvalIdArg?: string, approveArg?: boolean) => {
      const pending = (await surface("list approvals", () => adapter.approvals()))?.filter((a) => a.status === "REQUESTED") ?? [];
      const chosen = approvalIdArg ? pending.find((a) => Buffer.from(a.approvalId?.value ?? new Uint8Array()).toString("hex") === approvalIdArg) : (await vscode.window.showQuickPick(pending.map((a) => ({ label: a.toolName, description: `${a.effectClass} · intent ${a.intentHash.slice(0, 12)}…`, a })), { placeHolder: "Protected effect awaiting your decision" }))?.a;
      if (!chosen) return undefined;
      const approve = approveArg ?? (await vscode.window.showQuickPick(["approve", "deny"], { placeHolder: `${chosen.toolName} — intent ${chosen.intentHash}` })) === "approve";
      const id = Buffer.from(chosen.approvalId?.value ?? new Uint8Array()).toString("hex");
      // The decision names the intent hash the editor showed (PX-001).
      return surface("decide approval", () => adapter.decideApproval(id, approve, chosen.intentHash));
    }),
    vscode.commands.registerCommand("modbit.review", async (taskIdArg?: string) => {
      const row = taskIdArg ? adapter.tasks.get(taskIdArg) : await pickTask(["ReadyForReview"]);
      if (!row) return undefined;
      const bundle = await surface("review", () => adapter.review(row.taskId));
      if (!bundle) return undefined;
      const lines = [
        `# Modbit review — ${row.goal}`,
        `task ${row.taskId} · ${bundle.taskState} · workspace revision ${bundle.workspaceRevision} · base ${bundle.baseCommit.slice(0, 8)}`,
        "",
        ...bundle.files.flatMap((f) => [`## ${f.path} (${f.status}) file revision ${f.fileRevision.slice(0, 12)}`, ...f.hunks.flatMap((h) => [h.header, ...h.lines]), ""]),
        "## Verification",
        ...bundle.verificationRuns.map((v) => `- ${v.stage} ${v.status} ${v.checkIds.map((id, i) => `${id}=${v.checkStatuses[i] ?? ""}`).join(" ")}`),
        "## Evidence",
        ...bundle.evidenceLinks.map((e) => `- ${e}`),
      ];
      const doc = await vscode.workspace.openTextDocument({ content: lines.join("\n"), language: "markdown" });
      await vscode.window.showTextDocument(doc, { preview: true });
      return { taskId: row.taskId, workspaceRevision: bundle.workspaceRevision.toString(), files: bundle.files.length };
    }),
    vscode.commands.registerCommand("modbit.decideReview", async (taskIdArg?: string, decisionArg?: "ACCEPT" | "RETURN", noteArg?: string) => {
      const row = taskIdArg ? adapter.tasks.get(taskIdArg) : await pickTask(["ReadyForReview"]);
      if (!row) return undefined;
      const bundle = await surface("review", () => adapter.review(row.taskId));
      if (!bundle) return undefined;
      const decision = decisionArg ?? ((await vscode.window.showQuickPick(["ACCEPT", "RETURN"], { placeHolder: `Decide the review of "${row.goal}" at revision ${bundle.workspaceRevision}` })) as "ACCEPT" | "RETURN" | undefined);
      if (!decision) return undefined;
      const note = noteArg ?? (await vscode.window.showInputBox({ prompt: decision === "ACCEPT" ? "Commit message" : "What should change?", ignoreFocusOut: true })) ?? "";
      const r = await surface("decide review", () => adapter.decideReview(row.taskId, decision, note, bundle.workspaceRevision));
      tree.refresh();
      return r ? { taskState: r.taskState, commit: r.commit } : undefined;
    }),
    vscode.commands.registerCommand("modbit.forwardDiagnostics", async (taskIdArg?: string) => {
      const row = taskIdArg ? adapter.tasks.get(taskIdArg) : await pickTask();
      if (!row) return undefined;
      const items: EditorDiagnostic[] = [];
      for (const [uri, diags] of vscode.languages.getDiagnostics()) {
        if (uri.scheme !== "file" || !uri.fsPath.startsWith(folder)) continue;
        const doc = vscode.workspace.textDocuments.find((d) => d.uri.toString() === uri.toString());
        if (!doc) continue;
        const path = relative(folder, uri.fsPath).replace(/\\/g, "/");
        for (const d of diags) {
          items.push({
            path,
            lineStart: d.range.start.line,
            charStart: d.range.start.character,
            lineEnd: d.range.end.line,
            charEnd: d.range.end.character,
            severity: SEVERITY[d.severity] ?? "information",
            ...(typeof d.code === "string" || typeof d.code === "number" ? { code: String(d.code) } : {}),
            message: d.message,
            text: doc.getText(),
          });
        }
      }
      if (items.length === 0) {
        void vscode.window.showInformationMessage("Modbit: the editor reports no diagnostics on open documents");
        return undefined;
      }
      const r = await surface("forward diagnostics", () => adapter.forwardDiagnostics(row.taskId, `vscode:${basename(vscode.env.appName)}`, vscode.version, items));
      if (r) output.appendLine(`forwarded ${r.recorded} diagnostic(s) at workspace revision ${r.workspaceRevision} (${r.discarded} discarded)`);
      return r ? { recorded: r.recorded, discarded: r.discarded, workspaceRevision: r.workspaceRevision.toString() } : undefined;
    }),
    vscode.commands.registerCommand("modbit.resume", async (taskIdArg?: string) => {
      const row = taskIdArg ? adapter.tasks.get(taskIdArg) : await pickTask(["Waiting", "Queued"]);
      if (!row) return undefined;
      const r = await surface("resume", () => adapter.resume(row.taskId));
      tree.refresh();
      return r;
    }),
    vscode.commands.registerCommand("modbit.cursor", () => ({ session: adapter.session(), cursor: adapter.currentCursor().toString() })),
  );
  await surface("start", () => adapter.start());
  // The adapter is the extension's export: the host's own tests drive it.
  return adapter;
}

export function deactivate(): void {}
