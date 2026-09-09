/**
 * Node-side SurfaceProtocol client for Electron main (docs/30, docs/32).
 * Mirrors crates/protocol: 4-byte big-endian length + SurfaceFrame, 4 MiB
 * ceiling enforced before allocation, boot-secret handshake, commands with
 * acks, and a subscription that delivers stored events by offset.
 *
 * This is the only place in the desktop app that speaks to the Core; the
 * renderer never sees the socket, the secret, or Node.
 */
import { connect, type Socket } from "node:net";
import { create, fromBinary, toBinary, type MessageInitShape } from "@bufbuild/protobuf";
import {
  AcquireSessionLeaseSchema,
  ClientKind,
  CodeViewModelSchema,
  CommandAckSchema,
  CommandEnvelopeSchema,
  CommandStatus,
  CreateSessionSchema,
  CreateTaskSchema,
  DecideReviewSchema,
  GetCodeViewSchema,
  GetRecoveryReportSchema,
  GetReviewBundleSchema,
  GetSessionSnapshotSchema,
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
  constructor(public code: string, message: string) {
    super(`${code}: ${message}`);
  }
}

export class RejectedError extends Error {
  constructor(public code: string, message: string) {
    super(`${code}: ${message}`);
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
  onEvent: ((e: StoredEventFrame) => void) | null = null;
  onClose: ((reason: string) => void) | null = null;

  static async connect(ready: ReadyLine, kind: ClientKind, build: string): Promise<CoreClient> {
    if (ready.protocol.major !== PROTOCOL_VERSION.major) {
      throw new ProtocolError("PROTOCOL_MISMATCH", `core speaks ${ready.protocol.major}.x, client ${PROTOCOL_VERSION.major}.x`);
    }
    const c = new CoreClient();
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

  leaseGeneration(sessionId: string): bigint | undefined {
    return this.leases.get(sessionId);
  }

  async createTask(sessionId: string, goalText: string, commandId?: Uint8Array, workspaceRoot = ""): Promise<{ taskId: string; offset: bigint; replayed: boolean }> {
    const payload = toBinary(CreateTaskSchema, create(CreateTaskSchema, { sessionId: { value: unhex(sessionId) }, goalText, executionProfile: "local_trusted", origin: "desktop", workspaceRoot }));
    const ack = await this.command("CreateTask", payload, commandId, this.leases.get(sessionId));
    const r = fromBinary(TaskCreatedSchema, ack.result);
    return { taskId: hex(r.taskId?.value ?? new Uint8Array()), offset: r.offset, replayed: ack.status === CommandStatus.REPLAYED };
  }

  async getSessionSnapshot(sessionId: string): Promise<SessionSnapshot> {
    const ack = await this.command("GetSessionSnapshot", toBinary(GetSessionSnapshotSchema, create(GetSessionSnapshotSchema, { sessionId: { value: unhex(sessionId) } })));
    return fromBinary(SessionSnapshotSchema, ack.result);
  }

  async getRecoveryReport(): Promise<RecoveryReport> {
    const ack = await this.command("GetRecoveryReport", toBinary(GetRecoveryReportSchema, create(GetRecoveryReportSchema, {})));
    return fromBinary(RecoveryReportSchema, ack.result);
  }

  async startTask(sessionId: string, taskId: string): Promise<{ runId: string; resumed: boolean; endpoint: string; model: string }> {
    const payload = toBinary(StartTaskSchema, create(StartTaskSchema, { taskId: { value: unhex(taskId) } }));
    const ack = await this.command("StartTask", payload, undefined, this.leases.get(sessionId));
    const r = fromBinary(TaskRunStartedSchema, ack.result);
    return { runId: hex(r.runId?.value ?? new Uint8Array()), resumed: r.resumed, endpoint: r.endpoint, model: r.model };
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

  subscribe(sessionId: string, afterOffset: bigint): void {
    this.socket.write(encodeFrame({ body: { case: "subscribe", value: create(SubscribeEventsSchema, { sessionId: { value: unhex(sessionId) }, afterOffset }) } }));
  }
}

// Re-exported for callers that build ids without importing the schema package.
export { ClientKind, CommandAckSchema, IdSchema };
