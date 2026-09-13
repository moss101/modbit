/**
 * The adapter behind the VS Code extension (PX-002; docs/29 "VS Code
 * adapter"): everything the extension does that is not editor hosting. It is
 * a thin client on `@modbit/ide-adapter-core` — the Core is spawned and
 * supervised for the profile, the session and the event cursor are the
 * editor's persisted state so a restarted editor resumes where it was, and
 * every action is a command the Core decides: create and start a task, steer
 * it, decide an approval by the intent it showed, read and decide a review,
 * forward the editor's language-service diagnostics with revision provenance.
 * It owns nothing: no workspace write, no tool, no Git, no provider.
 */
import { createHash } from "node:crypto";
import { ClientKind, CoreClient, CoreSupervisor, freshId, hex, type CoreStatus } from "@modbit/ide-adapter-core";
import type { ApprovalView, AttentionView, CodeViewModel, ReviewBundle, ReviewDecided, StoredEventFrame } from "@modbit/surface-protocol";

/** Where the editor keeps the adapter's small durable state (VS Code: `workspaceState`). */
export interface AdapterState {
  get(key: string): string | undefined;
  set(key: string, value: string | undefined): void | PromiseLike<void>;
}

export interface AdapterOptions {
  coreBinary: string;
  dataDir: string;
  workspaceRoot: string;
  state: AdapterState;
  build: string;
  /** A stored event as it arrives by cursor (after the adapter recorded the cursor). */
  onEvent?: (e: StoredEventFrame) => void;
  onStatus?: (s: CoreStatus) => void;
  log?: (line: string) => void;
}

/** One task as the panel lists it, reduced from the snapshot and the events by cursor. */
export interface TaskRow {
  taskId: string;
  goal: string;
  state: string;
  origin: string;
}

/** A diagnostic as the editor reports it, with the document text it was computed on. */
export interface EditorDiagnostic {
  path: string;
  lineStart: number;
  charStart: number;
  lineEnd: number;
  charEnd: number;
  severity: "error" | "warning" | "information" | "hint";
  code?: string;
  message: string;
  /** The document's text as the editor has it (the Core checks the hash against the file). */
  text: string;
}

const STATE_SESSION = "modbit.session";
const STATE_CURSOR = "modbit.cursor";

export class ModbitAdapter {
  private supervisor: CoreSupervisor | null = null;
  private client: CoreClient | null = null;
  private sessionId: string | null = null;
  private cursor = 0n;
  readonly tasks = new Map<string, TaskRow>();
  private readonly opts: AdapterOptions;

  constructor(opts: AdapterOptions) {
    this.opts = opts;
  }

  private log(line: string): void {
    this.opts.log?.(line);
  }

  /** Spawn (or reattach to) the profile's Core, join the persisted session and resume the event stream from the persisted cursor. */
  async start(): Promise<void> {
    const attached = new Promise<void>((resolve, reject) => {
      this.supervisor = new CoreSupervisor(
        this.opts.coreBinary,
        this.opts.dataDir,
        {
          status: (s) => this.opts.onStatus?.(s),
          // Every (re)connection re-attaches: the session is joined again and
          // the stream resumes from the persisted cursor.
          client: (c) => {
            this.client = c;
            this.attach(c).then(resolve, reject);
          },
        },
        this.opts.build,
        ClientKind.IDE_ADAPTER,
      );
    });
    await this.supervisor!.start();
    await attached;
  }

  private async attach(c: CoreClient): Promise<void> {
    const persisted = this.opts.state.get(STATE_SESSION);
    let sessionId = persisted ?? null;
    if (sessionId) {
      try {
        await c.getSessionSnapshot(sessionId);
      } catch {
        sessionId = null;
      }
    }
    if (!sessionId) {
      const s = await c.createSession(freshId());
      sessionId = s.sessionId;
      await this.opts.state.set(STATE_SESSION, sessionId);
      await this.opts.state.set(STATE_CURSOR, "0");
    }
    this.sessionId = sessionId;
    await c.joinSessionLease(sessionId, `vscode-adapter ${this.opts.build}`);
    // The snapshot is the state; the cursor resumes what happened since.
    const snapshot = await c.getSessionSnapshot(sessionId);
    this.tasks.clear();
    for (const t of snapshot.tasks) {
      const id = hex(t.taskId?.value ?? new Uint8Array());
      // The snapshot spells a wait with its reason (`Waiting(Approval)`); the panel keys on the state.
      this.tasks.set(id, { taskId: id, goal: t.goalText, state: t.state.replace(/\(.*\)$/, ""), origin: t.origin });
    }
    this.cursor = BigInt(this.opts.state.get(STATE_CURSOR) ?? "0");
    if (this.cursor > snapshot.lastOffset) this.cursor = snapshot.lastOffset;
    c.onEvent = (e) => {
      this.cursor = e.offset;
      void this.opts.state.set(STATE_CURSOR, e.offset.toString());
      this.reduce(e);
      this.opts.onEvent?.(e);
    };
    c.subscribe(sessionId, this.cursor);
    this.log(`attached to session ${sessionId.slice(0, 8)} from offset ${this.cursor}`);
  }

  private reduce(e: StoredEventFrame): void {
    const ev = e.event;
    if (!ev?.taskId) return;
    const id = hex(ev.taskId.value);
    const row = this.tasks.get(id);
    switch (ev.eventType) {
      case "TaskCreated": {
        let goal = "";
        try {
          const p = JSON.parse(new TextDecoder().decode(ev.payload)) as { payload?: { goal_text?: string; origin?: string } };
          goal = p.payload?.goal_text ?? "";
          this.tasks.set(id, { taskId: id, goal, state: "Queued", origin: p.payload?.origin ?? "" });
        } catch {
          this.tasks.set(id, { taskId: id, goal, state: "Queued", origin: "" });
        }
        return;
      }
      case "TaskStarted":
      case "TaskResumed":
      case "TaskReturnedToWork":
        if (row) row.state = "Running";
        return;
      case "TaskWaiting":
        if (row) row.state = "Waiting";
        return;
      case "TaskReadyForReview":
        if (row) row.state = "ReadyForReview";
        return;
      case "TaskCompleted":
        if (row) row.state = "Completed";
        return;
      case "TaskFailed":
        if (row) row.state = "Failed";
        return;
      case "TaskCancelled":
        if (row) row.state = "Cancelled";
        return;
      default:
        return;
    }
  }

  /** The current connection, or an error the editor shows. */
  private core(): CoreClient {
    if (!this.client) throw new Error("the Core is not connected");
    return this.client;
  }

  session(): string {
    if (!this.sessionId) throw new Error("no session yet");
    return this.sessionId;
  }

  currentCursor(): bigint {
    return this.cursor;
  }

  /** Create a task for the workspace and start it; the command id is the caller's so a retry replays. */
  async createTask(goal: string, endpoint = "", model = "", commandId: Uint8Array = freshId()): Promise<{ taskId: string; replayed: boolean; runId: string }> {
    const c = this.core();
    const s = this.session();
    const created = await c.createTask(s, goal, commandId, this.opts.workspaceRoot);
    this.tasks.set(created.taskId, { taskId: created.taskId, goal, state: "Queued", origin: "ide_adapter" });
    const { toBinary, create, fromBinary } = await import("@bufbuild/protobuf");
    const { StartTaskSchema, TaskRunStartedSchema } = await import("@modbit/surface-protocol");
    const payload = toBinary(StartTaskSchema, create(StartTaskSchema, { taskId: { value: Buffer.from(created.taskId, "hex") }, endpoint, model }));
    const ack = await c.command("StartTask", payload, undefined, c.leaseGeneration(s));
    const r = fromBinary(TaskRunStartedSchema, ack.result);
    return { taskId: created.taskId, replayed: created.replayed, runId: hex(r.runId?.value ?? new Uint8Array()) };
  }

  /** Resume a task whose run the Core suspended (a restart while it waited): `StartTask` on the same task, marked resumed. */
  async resume(taskId: string, endpoint = "", model = ""): Promise<{ runId: string; resumed: boolean }> {
    const c = this.core();
    const s = this.session();
    const { toBinary, create, fromBinary } = await import("@bufbuild/protobuf");
    const { StartTaskSchema, TaskRunStartedSchema } = await import("@modbit/surface-protocol");
    const payload = toBinary(StartTaskSchema, create(StartTaskSchema, { taskId: { value: Buffer.from(taskId, "hex") }, endpoint, model }));
    const ack = await c.command("StartTask", payload, undefined, c.leaseGeneration(s));
    const r = fromBinary(TaskRunStartedSchema, ack.result);
    return { runId: hex(r.runId?.value ?? new Uint8Array()), resumed: r.resumed };
  }

  /** Steer a task: durable input the loop applies at its next boundary. */
  async steer(taskId: string, text: string): Promise<{ sequence: bigint; offset: bigint }> {
    return this.core().queueInput(this.session(), taskId, text, "STEER");
  }

  async approvals(): Promise<ApprovalView[]> {
    return (await this.core().listApprovals(this.session())).approvals;
  }

  /** Decide an approval naming the intent the editor showed for it; the Core refuses any other. */
  async decideApproval(approvalId: string, approve: boolean, intentHash: string, reason = "decided in the editor"): Promise<string> {
    const r = await this.core().resolveApproval(this.session(), approvalId, approve, reason, intentHash);
    return r.status;
  }

  async attention(): Promise<AttentionView> {
    return this.core().attention(this.session());
  }

  async review(taskId: string): Promise<ReviewBundle> {
    return this.core().getReviewBundle(taskId);
  }

  async codeView(taskId: string, path: string, expectedFileRevision = ""): Promise<CodeViewModel> {
    return this.core().getCodeView(taskId, path, expectedFileRevision);
  }

  /** Accept or return a review at the revision the editor showed; the Core refuses a stale one. */
  async decideReview(taskId: string, decision: "ACCEPT" | "RETURN", note: string, expectedWorkspaceRevision: bigint, rejected: { path: string; index: number }[] = []): Promise<ReviewDecided> {
    return this.core().decideReview(this.session(), taskId, decision, rejected, note, expectedWorkspaceRevision);
  }

  /**
   * Forward the editor's language-service diagnostics for a task (PX-004),
   * bound to the workspace revision the Core reports now and to the sha256
   * of each document's text as the editor has it. The Core drops a
   * diagnostic whose file on disk differs and refuses a stale batch.
   */
  async forwardDiagnostics(taskId: string, source: string, sourceVersion: string, items: EditorDiagnostic[]): Promise<{ recorded: number; discarded: number; batchRef: string; workspaceRevision: bigint }> {
    const c = this.core();
    if (items.length === 0) throw new Error("nothing to forward");
    const view = await c.getCodeView(taskId, items[0]!.path);
    const ack = await c.submitExternalDiagnostics(this.session(), taskId, {
      source,
      sourceVersion,
      workspaceRevision: view.workspaceRevision,
      diagnostics: items.map((d) => ({
        path: d.path,
        lineStart: d.lineStart,
        charStart: d.charStart,
        lineEnd: d.lineEnd,
        charEnd: d.charEnd,
        severity: d.severity,
        ...(d.code !== undefined ? { code: d.code } : {}),
        message: d.message,
        fileRevision: createHash("sha256").update(d.text).digest("hex"),
      })),
    });
    return { recorded: ack.recorded, discarded: ack.discarded, batchRef: ack.batchRef, workspaceRevision: view.workspaceRevision };
  }

  /** Stop supervising: the Core exits with the editor (it is tethered to this process). */
  stop(): void {
    this.client?.close();
    this.client = null;
    this.supervisor?.stop();
    this.supervisor = null;
  }
}
