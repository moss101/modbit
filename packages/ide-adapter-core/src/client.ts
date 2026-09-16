/**
 * Node-side SurfaceProtocol client (docs/30, docs/32; the thin-client
 * contract of docs/29, PX-001). Mirrors crates/protocol: 4-byte big-endian
 * length + SurfaceFrame, 4 MiB ceiling enforced before allocation,
 * boot-secret handshake, commands with acks, and a subscription that
 * delivers stored events by offset.
 *
 * Electron main and every IDE adapter speak to the Core through this class
 * and nothing else; a renderer or an editor never sees the socket, the
 * secret, or Node. It owns no orchestration, context, Git state, policy or
 * tool execution and holds no provider credential: every mutation is a
 * command the Core decides, idempotent by the command id the caller keeps.
 */
import { connect, type Socket } from "node:net";
import { create, fromBinary, toBinary, type MessageInitShape } from "@bufbuild/protobuf";
import {
  AcquireSessionLeaseSchema,
  AttachmentIngestedSchema,
  IngestAttachmentSchema,
  ClientKind,
  CodeViewModelSchema,
  CommandAckSchema,
  CommandEnvelopeSchema,
  CommandStatus,
  CreateSessionSchema,
  CreateTaskSchema,
  DecideReviewSchema,
  ListApprovalsSchema,
  ApprovalListSchema,
  type ApprovalList,
  ResolveApprovalSchema,
  ApprovalResolvedAckSchema,
  type ApprovalResolvedAck,
  GetTaskAssuranceSchema,
  TaskAssuranceViewSchema,
  type TaskAssuranceView,
  GetTaskStatusSchema,
  SubmitExternalDiagnosticsSchema,
  QueueInputSchema,
  InputQueuedSchema,
  ExternalDiagnosticsAckSchema,
  type ExternalDiagnosticsAck,
  TaskStatusSchema,
  type TaskStatus,
  ApplyUserPatchSchema,
  UserPatchAppliedAckSchema,
  type UserPatchAppliedAck,
  GetCodeViewSchema,
  GetRecoveryReportSchema,
  GetReviewBundleSchema,
  GetContextInspectorSchema,
  GetAttentionSchema,
  AttentionViewSchema,
  type AttentionView,
  GetSessionSnapshotSchema,
  LanguageListSchema,
  ListLanguagesSchema,
  IdSchema,
  RecoveryReportSchema,
  ReviewBundleSchema,
  ReviewDecidedSchema,
  SessionCreatedSchema,
  SessionLeaseAcquiredSchema,
  SessionSnapshotSchema,
  StartTaskSchema,
  SubscribeEventsSchema,
  SurfaceFrameSchema,
  TaskCreatedSchema,
  TaskRunStartedSchema,
  type CodeViewModel,
  ContextInspectorViewSchema,
  type ContextInspectorView,
  GetTaskEconomicsSchema,
  SetTaskSelectionSchema,
  TaskSelectionRecordedSchema,
  TaskEconomicsViewSchema,
  ConfigureProviderSchema,
  ProviderConfiguredSchema,
  ListModelsSchema,
  ModelListSchema,
  ProbeModelSchema,
  ModelProbedSchema,
  TrustRepositorySchema,
  RepositoryTrustedSchema,
  ListStarterTasksSchema,
  StarterTaskListSchema,
  OpenPullRequestSchema,
  UpdatePullRequestSchema,
  PullRequestAckSchema,
  type PullRequestAck,
  RespondToQuestionSchema,
  QuestionRespondedSchema,
  type TaskEconomicsView,
  type LanguageList,
  type CommandAck,
  type RecoveryReport,
  type ReviewBundle,
  type ReviewDecided,
  type SessionSnapshot,
  type StoredEventFrame,
  type SurfaceFrame,
} from "@modbit/surface-protocol";
import { randomBytes } from "node:crypto";

export const MAX_FRAME_BYTES = 4 * 1024 * 1024;
export const PROTOCOL_VERSION = { major: 1, minor: 0 };

export interface ReadyLine {
  endpoint: string;
  bootSecretHex: string;
  protocol: { major: number; minor: number };
}

/** Parse the single line a Core prints once it is listening. */
export function parseReadyLine(line: string): ReadyLine | null {
  const prefix = "MODBIT_CORE_READY ";
  if (!line.startsWith(prefix)) return null;
  const kv = new Map(line.slice(prefix.length).split(/\s+/).map((p) => p.split("=", 2) as [string, string]));
  const endpoint = kv.get("endpoint");
  const secret = kv.get("secret");
  const proto = kv.get("protocol")?.split(".");
  if (!endpoint || !secret || !proto || proto.length !== 2) return null;
  return { endpoint, bootSecretHex: secret, protocol: { major: Number(proto[0]), minor: Number(proto[1]) } };
}

export class ProtocolError extends Error {
  readonly code: string;
  constructor(code: string, message: string) {
    super(`${code}: ${message}`);
    this.code = code;
  }
}

export class RejectedError extends Error {
  readonly code: string;
  constructor(code: string, message: string) {
    super(`${code}: ${message}`);
    this.code = code;
  }
}

/** Incremental frame decoder over a byte stream. */
export class FrameDecoder {
  private buf: Buffer<ArrayBufferLike> = Buffer.alloc(0);
  push(chunk: Buffer): SurfaceFrame[] {
    this.buf = this.buf.length === 0 ? chunk : Buffer.concat([this.buf, chunk]);
    const out: SurfaceFrame[] = [];
    for (;;) {
      if (this.buf.length < 4) return out;
      const len = this.buf.readUInt32BE(0);
      if (len > MAX_FRAME_BYTES) throw new ProtocolError("FRAME_TOO_LARGE", `${len} bytes exceeds ${MAX_FRAME_BYTES}`);
      if (this.buf.length < 4 + len) return out;
      const body = this.buf.subarray(4, 4 + len);
      this.buf = this.buf.subarray(4 + len);
      const frame = fromBinary(SurfaceFrameSchema, body);
      if (frame.body.case === undefined) throw new ProtocolError("MALFORMED_FRAME", "empty frame body");
      out.push(frame);
    }
  }
}

export function encodeFrame(frame: MessageInitShape<typeof SurfaceFrameSchema>): Buffer {
  const bytes = toBinary(SurfaceFrameSchema, create(SurfaceFrameSchema, frame));
  if (bytes.length > MAX_FRAME_BYTES) throw new ProtocolError("FRAME_TOO_LARGE", `${bytes.length} bytes`);
  const len = Buffer.alloc(4);
  len.writeUInt32BE(bytes.length, 0);
  return Buffer.concat([len, Buffer.from(bytes)]);
}

export function freshId(): Uint8Array {
  return new Uint8Array(randomBytes(16));
}

export const hex = (b: Uint8Array): string => Buffer.from(b).toString("hex");
export const unhex = (s: string): Uint8Array => new Uint8Array(Buffer.from(s, "hex"));

type Pending = { resolve: (a: CommandAck) => void; reject: (e: Error) => void };

/** One authenticated connection to a Core. */
export class CoreClient {
  private socket!: Socket;
  private decoder = new FrameDecoder();
  private pending: Pending[] = [];
  private closed = false;
  /** The `CreateTask.origin` this client's kind implies (docs/30). */
  readonly origin: "desktop" | "cli" | "ide_adapter";
  onEvent: ((e: StoredEventFrame) => void) | null = null;
  onClose: ((reason: string) => void) | null = null;

  private constructor(kind: ClientKind) {
    this.origin = kind === ClientKind.IDE_ADAPTER ? "ide_adapter" : kind === ClientKind.CLI ? "cli" : "desktop";
  }

  static async connect(ready: ReadyLine, kind: ClientKind, build: string): Promise<CoreClient> {
    if (ready.protocol.major !== PROTOCOL_VERSION.major) {
      throw new ProtocolError("PROTOCOL_MISMATCH", `core speaks ${ready.protocol.major}.x, client ${PROTOCOL_VERSION.major}.x`);
    }
    const c = new CoreClient(kind);
    await c.open(ready, kind, build);
    return c;
  }

  private open(ready: ReadyLine, kind: ClientKind, build: string): Promise<void> {
    return new Promise((resolve, reject) => {
      let handshake = true;
      this.socket = connect(ready.endpoint);
      this.socket.once("error", (e) => (handshake ? reject(e) : this.fail(e.message)));
      this.socket.on("close", () => this.fail("connection closed"));
      this.socket.on("data", (chunk: Buffer) => {
        let frames: SurfaceFrame[];
        try {
          frames = this.decoder.push(chunk);
        } catch (e) {
          this.fail((e as Error).message);
          return;
        }
        for (const f of frames) {
          if (handshake) {
            handshake = false;
            if (f.body.case === "helloAck" && f.body.value.compatible) resolve();
            else if (f.body.case === "helloAck") reject(new ProtocolError("INCOMPATIBLE", f.body.value.reason));
            else if (f.body.case === "error") reject(new ProtocolError(f.body.value.code, f.body.value.message));
            else reject(new ProtocolError("UNEXPECTED_FRAME", f.body.case ?? "none"));
            continue;
          }
          this.dispatch(f);
        }
      });
      this.socket.once("connect", () => {
        this.socket.write(
          encodeFrame({
            body: {
              case: "clientHello",
              value: {
                hello: { protocolVersion: PROTOCOL_VERSION, clientKind: kind, clientBuild: build, supportedCommandTypes: [] },
                auth: { bootSecret: unhex(ready.bootSecretHex) },
              },
            },
          }),
        );
      });
    });
  }

  private dispatch(f: SurfaceFrame): void {
    switch (f.body.case) {
      case "commandAck": {
        const p = this.pending.shift();
        if (!p) return;
        const ack = f.body.value;
        if (ack.status === CommandStatus.REJECTED) p.reject(new RejectedError(ack.errorCode, ack.errorMessage));
        else p.resolve(ack);
        return;
      }
      case "event":
        this.onEvent?.(f.body.value);
        return;
      case "error":
        this.fail(`${f.body.value.code}: ${f.body.value.message}`);
        return;
      default:
        this.fail(`unexpected frame ${f.body.case}`);
    }
  }

  private fail(reason: string): void {
    if (this.closed) return;
    this.closed = true;
    for (const p of this.pending.splice(0)) p.reject(new ProtocolError("DISCONNECTED", reason));
    this.socket.destroy();
    this.onClose?.(reason);
  }

  close(): void {
    this.fail("closed by client");
  }

  /** Session lease generations this client holds (docs/13 fencing). */
  private leases = new Map<string, bigint>();

  command(commandType: string, payload: Uint8Array, commandId: Uint8Array = freshId(), expectedGeneration?: bigint): Promise<CommandAck> {
    if (this.closed) return Promise.reject(new ProtocolError("DISCONNECTED", "client closed"));
    const envelope = create(CommandEnvelopeSchema, { commandId: { value: commandId }, commandType, schemaVersion: 1, payload, ...(expectedGeneration !== undefined ? { expectedGeneration } : {}) });
    return new Promise((resolve, reject) => {
      this.pending.push({ resolve, reject });
      this.socket.write(encodeFrame({ body: { case: "command", value: envelope } }));
    });
  }

  async createSession(commandId?: Uint8Array): Promise<{ sessionId: string; offset: bigint }> {
    const ack = await this.command("CreateSession", toBinary(CreateSessionSchema, create(CreateSessionSchema, {})), commandId);
    const r = fromBinary(SessionCreatedSchema, ack.result);
    return { sessionId: hex(r.sessionId?.value ?? new Uint8Array()), offset: r.offset };
  }

  /** Become the session's single mutation owner; returns the new generation. */
  async acquireSessionLease(sessionId: string, owner: string): Promise<bigint> {
    const payload = toBinary(AcquireSessionLeaseSchema, create(AcquireSessionLeaseSchema, { sessionId: { value: unhex(sessionId) }, owner }));
    const ack = await this.command("AcquireSessionLease", payload);
    const r = fromBinary(SessionLeaseAcquiredSchema, ack.result);
    this.leases.set(sessionId, r.leaseGeneration);
    return r.leaseGeneration;
  }

  /**
   * Join the session's lease in force (docs/13 fencing; the CLI's reference
   * behavior): a reconnecting client presents the generation the Core
   * records, and acquires a new one only when none exists. Acquiring afresh
   * would fence out the run this same person is waiting on.
   */
  async joinSessionLease(sessionId: string, owner: string): Promise<bigint> {
    const snapshot = await this.getSessionSnapshot(sessionId);
    if (snapshot.leaseGeneration > 0n) {
      this.leases.set(sessionId, snapshot.leaseGeneration);
      return snapshot.leaseGeneration;
    }
    return this.acquireSessionLease(sessionId, owner);
  }

  leaseGeneration(sessionId: string): bigint | undefined {
    return this.leases.get(sessionId);
  }

  /** Create a task; with `issueUrl` (PX-010) the origin is `forge_issue` and the Core reads the issue first — an unreadable one is refused and no task exists. */
  async createTask(sessionId: string, goalText: string, commandId?: Uint8Array, workspaceRoot = "", issueUrl?: string): Promise<{ taskId: string; offset: bigint; replayed: boolean; goalText: string }> {
    const payload = toBinary(CreateTaskSchema, create(CreateTaskSchema, { sessionId: { value: unhex(sessionId) }, goalText, executionProfile: "local_trusted", origin: issueUrl ? "forge_issue" : this.origin, workspaceRoot, issueUrl: issueUrl ?? "" }));
    const ack = await this.command("CreateTask", payload, commandId, this.leases.get(sessionId));
    const r = fromBinary(TaskCreatedSchema, ack.result);
    return { taskId: hex(r.taskId?.value ?? new Uint8Array()), offset: r.offset, replayed: ack.status === CommandStatus.REPLAYED, goalText: r.goalText };
  }

  async getSessionSnapshot(sessionId: string): Promise<SessionSnapshot> {
    const ack = await this.command("GetSessionSnapshot", toBinary(GetSessionSnapshotSchema, create(GetSessionSnapshotSchema, { sessionId: { value: unhex(sessionId) } })));
    return fromBinary(SessionSnapshotSchema, ack.result);
  }

  /** REQ-EV-0151 / 0275: every open attention item of a session, derived by
   *  the Core from canonical unresolved state. */
  async attention(sessionId: string): Promise<AttentionView> {
    const ack = await this.command("GetAttention", toBinary(GetAttentionSchema, create(GetAttentionSchema, { sessionId: { value: unhex(sessionId) } })));
    return fromBinary(AttentionViewSchema, ack.result);
  }

  async contextInspector(taskId: string): Promise<ContextInspectorView> {
    const ack = await this.command(
      "GetContextInspector",
      toBinary(GetContextInspectorSchema, create(GetContextInspectorSchema, { taskId: { value: unhex(taskId) } })),
    );
    return fromBinary(ContextInspectorViewSchema, ack.result);
  }

  async setTaskSelection(
    sessionId: string,
    taskId: string,
    sel: { paths: string[]; symbol: string; lineStart: number; lineEnd: number; reviewHunks: string[]; source: string },
  ): Promise<{ offset: string }> {
    const payload = toBinary(
      SetTaskSelectionSchema,
      create(SetTaskSelectionSchema, { taskId: { value: unhex(taskId) }, ...sel }),
    );
    const ack = await this.command("SetTaskSelection", payload, undefined, this.leases.get(sessionId));
    const r = fromBinary(TaskSelectionRecordedSchema, ack.result);
    return { offset: r.offset.toString() };
  }

  async taskEconomics(taskId: string): Promise<TaskEconomicsView> {
    const ack = await this.command(
      "GetTaskEconomics",
      toBinary(GetTaskEconomicsSchema, create(GetTaskEconomicsSchema, { taskId: { value: unhex(taskId) } })),
    );
    return fromBinary(TaskEconomicsViewSchema, ack.result);
  }

  async listLanguages(): Promise<LanguageList> {
    const ack = await this.command("ListLanguages", toBinary(ListLanguagesSchema, create(ListLanguagesSchema, {})));
    return fromBinary(LanguageListSchema, ack.result);
  }

  async getRecoveryReport(): Promise<RecoveryReport> {
    const ack = await this.command("GetRecoveryReport", toBinary(GetRecoveryReportSchema, create(GetRecoveryReportSchema, {})));
    return fromBinary(RecoveryReportSchema, ack.result);
  }

  /** The endpoints the Core has, with whether each has a credential; never the credential. */
  async listProviders(): Promise<{ endpoint: string; model: string; credentialAvailable: boolean }[]> {
    const ack = await this.command("ListModels", toBinary(ListModelsSchema, create(ListModelsSchema, {})));
    const r = fromBinary(ModelListSchema, ack.result);
    return r.models.map((m) => ({ endpoint: m.endpoint, model: m.model, credentialAvailable: m.credentialAvailable }));
  }

  /** REQ-PX-022: hand the Core a provider credential. Main is the only
   *  process that ever holds it in the clear; the Core keeps it in memory. */
  async configureProvider(provider: string, apiKey: string, baseUrl = ""): Promise<{ endpoint: string; credentialAvailable: boolean; models: string[] }> {
    const payload = toBinary(ConfigureProviderSchema, create(ConfigureProviderSchema, { provider, apiKey, baseUrl }));
    const ack = await this.command("ConfigureProvider", payload);
    const r = fromBinary(ProviderConfiguredSchema, ack.result);
    return { endpoint: r.endpoint, credentialAvailable: r.credentialAvailable, models: r.models };
  }

  /** The live test call that confirms a provider (docs/39 step 2). */
  async probeModel(endpoint: string, model: string): Promise<{ status: string; text: string; errorCode: string; errorMessage: string }> {
    const payload = toBinary(ProbeModelSchema, create(ProbeModelSchema, { endpoint, model, prompt: "Reply with the single word pong.", withTools: false, timeoutMs: 20_000n }));
    const ack = await this.command("ProbeModel", payload);
    const r = fromBinary(ModelProbedSchema, ack.result);
    return { status: r.status, text: r.text, errorCode: r.errorCode, errorMessage: r.errorMessage };
  }

  /** Explicit, scoped repository trust (docs/39 step 3). */
  async trustRepository(sessionId: string, workspaceRoot: string): Promise<{ offset: string }> {
    const payload = toBinary(TrustRepositorySchema, create(TrustRepositorySchema, { sessionId: { value: unhex(sessionId) }, workspaceRoot, scope: "repository" }));
    const ack = await this.command("TrustRepository", payload, undefined, this.leases.get(sessionId));
    const r = fromBinary(RepositoryTrustedSchema, ack.result);
    return { offset: r.offset.toString() };
  }

  /** Starter tasks for the detected stack (docs/39 step 4). */
  async listStarterTasks(workspaceRoot: string): Promise<{ stacks: string[]; tasks: { id: string; title: string; goalText: string; stack: string }[] }> {
    const payload = toBinary(ListStarterTasksSchema, create(ListStarterTasksSchema, { workspaceRoot }));
    const ack = await this.command("ListStarterTasks", payload);
    const r = fromBinary(StarterTaskListSchema, ack.result);
    return { stacks: r.stacks, tasks: r.tasks.map((t) => ({ id: t.id, title: t.title, goalText: t.goalText, stack: t.stack })) };
  }

  async startTask(sessionId: string, taskId: string): Promise<{ runId: string; resumed: boolean; endpoint: string; model: string }> {
    const payload = toBinary(StartTaskSchema, create(StartTaskSchema, { taskId: { value: unhex(taskId) } }));
    const ack = await this.command("StartTask", payload, undefined, this.leases.get(sessionId));
    const r = fromBinary(TaskRunStartedSchema, ack.result);
    return { runId: hex(r.runId?.value ?? new Uint8Array()), resumed: r.resumed, endpoint: r.endpoint, model: r.model };
  }

  /** REQ-EV-0190: normalize a channel attachment through the Core's media pipeline (bytes stay in the Core by digest). */
  async ingestAttachment(sessionId: string, taskId: string, filename: string, data: Uint8Array): Promise<{ attachmentId: string; kind: string; mime: string; contentRef: string; offset: string; replayed: boolean }> {
    const payload = toBinary(IngestAttachmentSchema, create(IngestAttachmentSchema, { taskId: { value: unhex(taskId) }, filename, channel: "desktop", data }));
    const ack = await this.command("IngestAttachment", payload, undefined, this.leases.get(sessionId));
    const r = fromBinary(AttachmentIngestedSchema, ack.result);
    return { attachmentId: r.attachmentId, kind: r.kind, mime: r.mime, contentRef: r.contentRef, offset: r.offset.toString(), replayed: r.replayed };
  }

  async getReviewBundle(taskId: string): Promise<ReviewBundle> {
    const ack = await this.command("GetReviewBundle", toBinary(GetReviewBundleSchema, create(GetReviewBundleSchema, { taskId: { value: unhex(taskId) } })));
    return fromBinary(ReviewBundleSchema, ack.result);
  }

  async getCodeView(taskId: string, path: string, expectedFileRevision = ""): Promise<CodeViewModel> {
    const ack = await this.command("GetCodeView", toBinary(GetCodeViewSchema, create(GetCodeViewSchema, { taskId: { value: unhex(taskId) }, path, expectedFileRevision })));
    return fromBinary(CodeViewModelSchema, ack.result);
  }

  async decideReview(sessionId: string, taskId: string, decision: "ACCEPT" | "RETURN", rejected: { path: string; index: number }[], note: string, expectedWorkspaceRevision: bigint): Promise<ReviewDecided> {
    const payload = toBinary(DecideReviewSchema, create(DecideReviewSchema, { taskId: { value: unhex(taskId) }, decision, rejected: rejected.map((r) => ({ path: r.path, index: r.index })), note, expectedWorkspaceRevision }));
    const ack = await this.command("DecideReview", payload, undefined, this.leases.get(sessionId));
    return fromBinary(ReviewDecidedSchema, ack.result);
  }

  /** PX-005: a one-hunk direct edit through the Core's ChangeTransaction; the renderer keeps no buffer. */
  async applyUserPatch(sessionId: string, taskId: string, p: { path: string; old: string; new: string; expectedWorkspaceRevision: bigint; expectedFileRevision: string }): Promise<UserPatchAppliedAck> {
    const payload = toBinary(ApplyUserPatchSchema, create(ApplyUserPatchSchema, { taskId: { value: unhex(taskId) }, path: p.path, old: p.old, new: p.new, expectedWorkspaceRevision: p.expectedWorkspaceRevision, expectedFileRevision: p.expectedFileRevision, source: "review" }));
    const ack = await this.command("ApplyUserPatch", payload, undefined, this.leases.get(sessionId));
    return fromBinary(UserPatchAppliedAckSchema, ack.result);
  }

  async listApprovals(sessionId: string): Promise<ApprovalList> {
    const ack = await this.command("ListApprovals", toBinary(ListApprovalsSchema, create(ListApprovalsSchema, { sessionId: { value: unhex(sessionId) } })));
    return fromBinary(ApprovalListSchema, ack.result);
  }

  /**
   * Decide a protected effect. The decision names the intent hash the person
   * saw (PX-001, docs/29): the Core refuses INTENT_MISMATCH for any other,
   * so a stale approval view can never approve a different effect.
   */
  async resolveApproval(sessionId: string, approvalId: string, approve: boolean, reason: string, intentHash: string, commandId?: Uint8Array): Promise<ApprovalResolvedAck> {
    const payload = toBinary(ResolveApprovalSchema, create(ResolveApprovalSchema, { approvalId: { value: unhex(approvalId) }, approve, reason, intentHash }));
    const ack = await this.command("ResolveApproval", payload, commandId, this.leases.get(sessionId));
    return fromBinary(ApprovalResolvedAckSchema, ack.result);
  }

  /**
   * PX-007: open (or update) the task's pull request from the accepted
   * candidate. The first call returns APPROVAL_PENDING with the approval
   * that binds the push and the POST; after the approval is resolved the
   * same call (same revision) returns OPENED/UPDATED — or DENIED, which
   * leaves the branch local. Every attempt is a receipt in the log.
   */
  async openPullRequest(sessionId: string, taskId: string, expectedCandidateRevision: bigint, opts: { base?: string; title?: string; remote?: string; update?: boolean } = {}): Promise<PullRequestAck> {
    const fields = { taskId: { value: unhex(taskId) }, expectedCandidateRevision, base: opts.base ?? "", title: opts.title ?? "", remote: opts.remote ?? "" };
    const payload = opts.update ? toBinary(UpdatePullRequestSchema, create(UpdatePullRequestSchema, fields)) : toBinary(OpenPullRequestSchema, create(OpenPullRequestSchema, fields));
    const ack = await this.command(opts.update ? "UpdatePullRequest" : "OpenPullRequest", payload, undefined, this.leases.get(sessionId));
    return fromBinary(PullRequestAckSchema, ack.result);
  }

  async taskAssurance(taskId: string): Promise<TaskAssuranceView> {
    const ack = await this.command("GetTaskAssurance", toBinary(GetTaskAssuranceSchema, create(GetTaskAssuranceSchema, { taskId: { value: unhex(taskId) } })));
    return fromBinary(TaskAssuranceViewSchema, ack.result);
  }

  /**
   * PX-004: hand the Core what the editor's language services see, bound to
   * the workspace revision and the per-file content hashes they were computed
   * on. Evidence for context and the verification plan's inputs — never a
   * verification result. The Core refuses another revision (STALE_REVISION)
   * and a malformed batch (MALFORMED).
   */
  async submitExternalDiagnostics(
    sessionId: string,
    taskId: string,
    batch: {
      source: string;
      sourceVersion: string;
      workspaceRevision: bigint;
      diagnostics: { path: string; lineStart: number; charStart: number; lineEnd: number; charEnd: number; severity: "error" | "warning" | "information" | "hint"; code?: string; message: string; fileRevision: string }[];
    },
    commandId?: Uint8Array,
  ): Promise<ExternalDiagnosticsAck> {
    const payload = toBinary(
      SubmitExternalDiagnosticsSchema,
      create(SubmitExternalDiagnosticsSchema, {
        taskId: { value: unhex(taskId) },
        source: batch.source,
        sourceVersion: batch.sourceVersion,
        workspaceRevision: batch.workspaceRevision,
        diagnostics: batch.diagnostics.map((d) => ({ path: d.path, lineStart: d.lineStart, charStart: d.charStart, lineEnd: d.lineEnd, charEnd: d.charEnd, severity: d.severity, code: d.code ?? "", message: d.message, fileRevision: d.fileRevision })),
      }),
    );
    const ack = await this.command("SubmitExternalDiagnostics", payload, commandId, this.leases.get(sessionId));
    return fromBinary(ExternalDiagnosticsAckSchema, ack.result);
  }

  /** Steer, collect for, or follow up a task (REQ-EV-0191): durable input the loop applies at its next boundary. */
  async queueInput(sessionId: string, taskId: string, text: string, mode: "STEER" | "COLLECT" | "FOLLOW_UP" = "STEER", inputId = hex(freshId())): Promise<{ sequence: bigint; offset: bigint }> {
    const payload = toBinary(QueueInputSchema, create(QueueInputSchema, { taskId: { value: unhex(taskId) }, inputId, mode, text }));
    const ack = await this.command("QueueInput", payload, undefined, this.leases.get(sessionId));
    const r = fromBinary(InputQueuedSchema, ack.result);
    return { sequence: r.sequence, offset: r.offset };
  }

  /** REQ-EV-0222: answer the agent's typed question; the run resumes with StartTask. */
  async respondToQuestion(sessionId: string, taskId: string, questionId: string, optionId: string, text: string): Promise<{ questionId: string; alreadyAnswered: boolean }> {
    const payload = toBinary(RespondToQuestionSchema, create(RespondToQuestionSchema, { taskId: { value: unhex(taskId) }, questionId, optionId, text }));
    const ack = await this.command("RespondToQuestion", payload, undefined, this.leases.get(sessionId));
    const r = fromBinary(QuestionRespondedSchema, ack.result);
    return { questionId: r.questionId, alreadyAnswered: r.alreadyAnswered };
  }

  async taskStatus(taskId: string): Promise<TaskStatus> {
    const ack = await this.command("GetTaskStatus", toBinary(GetTaskStatusSchema, create(GetTaskStatusSchema, { taskId: { value: unhex(taskId) } })));
    return fromBinary(TaskStatusSchema, ack.result);
  }

  subscribe(sessionId: string, afterOffset: bigint): void {
    this.socket.write(encodeFrame({ body: { case: "subscribe", value: create(SubscribeEventsSchema, { sessionId: { value: unhex(sessionId) }, afterOffset }) } }));
  }
}

// Re-exported for callers that build ids without importing the schema package.
export { ClientKind, CommandAckSchema, IdSchema };
