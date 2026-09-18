/**
 * Electron main (docs/32 "Security settings"): the only process that talks to
 * the Core. It spawns/supervises `modbit-core`, holds the boot secret, and
 * exposes a narrow, validated IPC surface to the sandboxed renderer through the
 * preload bridge. The renderer gets durable ids and Core events; it never gets
 * the socket, the secret, Node, or the filesystem.
 */
import { app, BrowserWindow, ipcMain, Notification, safeStorage, session, type IpcMainInvokeEvent } from "electron";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { CoreSupervisor, freshId, type CoreClient, type CoreStatus } from "@modbit/ide-adapter-core";
import { serializeEvent, type WireEvent } from "./events.js";
import { BrowserHost } from "./browser.js";
import { CredentialStore } from "./credentials.js";

const dataDir = process.env.MODBIT_DATA_DIR ?? join(app.getPath("userData"), "modbit");
// A profile named by MODBIT_DATA_DIR is a whole profile: the renderer's
// storage (preferences, caches) lives under it too, so two profiles — or
// two E2E runs — never share what one viewer stored (PX-024).
if (process.env.MODBIT_DATA_DIR) app.setPath("userData", join(dataDir, "electron"));
const coreBin = process.env.MODBIT_CORE_BIN ?? resolve(app.getAppPath(), "..", "..", "target", "debug", process.platform === "win32" ? "modbit-core.exe" : "modbit-core");
mkdirSync(dataDir, { recursive: true });

/** Client-local convenience only: which session this window last used. The
 *  session itself is Core truth; losing this file loses nothing durable. */
const stateFile = join(dataDir, "desktop-state.json");
function loadLocalState(): { sessionId?: string } {
  try {
    return existsSync(stateFile) ? (JSON.parse(readFileSync(stateFile, "utf8")) as { sessionId?: string }) : {};
  } catch {
    return {};
  }
}
function saveLocalState(s: { sessionId?: string }): void {
  writeFileSync(stateFile, JSON.stringify(s));
}

let win: BrowserWindow | null = null;
let subscription: { sessionId: string; cursor: bigint } | null = null;

/**
 * REQ-PX-022, docs/39 step 2: the provider credential lives in the OS
 * keychain's custody through Electron main. `safeStorage` encrypts it with a
 * key the operating system keeps (Keychain on macOS, DPAPI on Windows,
 * libsecret on Linux) and only the ciphertext touches disk; the renderer
 * never sees the value, and the Core receives it over the local socket and
 * holds it in memory. When the OS offers no encryption the key is not
 * persisted at all and the user is told so.
 */
const providerFile = join(dataDir, "provider.enc");
type ProviderRecord = { provider: string; baseUrl: string; keyCiphertext: string };
function loadProvider(): ProviderRecord | null {
  try {
    return existsSync(providerFile) ? (JSON.parse(readFileSync(providerFile, "utf8")) as ProviderRecord) : null;
  } catch {
    return null;
  }
}
function storeProvider(provider: string, baseUrl: string, apiKey: string): { persisted: boolean } {
  if (!safeStorage.isEncryptionAvailable()) return { persisted: false };
  const keyCiphertext = safeStorage.encryptString(apiKey).toString("base64");
  writeFileSync(providerFile, JSON.stringify({ provider, baseUrl, keyCiphertext } satisfies ProviderRecord), { mode: 0o600 });
  return { persisted: true };
}
function recallProviderKey(rec: ProviderRecord): string | null {
  try {
    return safeStorage.isEncryptionAvailable() ? safeStorage.decryptString(Buffer.from(rec.keyCiphertext, "base64")) : null;
  } catch {
    return null;
  }
}
/** Hand a stored credential to a (re)started Core, so a restart does not
 *  undo provider setup. Never logs the value. */
async function handProviderToCore(c: CoreClient): Promise<void> {
  const rec = loadProvider();
  if (!rec) return;
  const key = recallProviderKey(rec);
  if (key === null) return;
  await c.configureProvider(rec.provider, key, rec.baseUrl).catch(() => {});
}

function send(channel: string, payload: unknown): void {
  if (win && !win.isDestroyed()) win.webContents.send(channel, payload);
}

const supervisor = new CoreSupervisor(
  coreBin,
  dataDir,
  {
    status(s: CoreStatus) {
      send("core:status", s);
    },
    client(c: CoreClient) {
      c.onEvent = (e) => {
        const ev = serializeEvent(e);
        if (subscription) subscription.cursor = BigInt(ev.offset);
        // IMP-EV-0085: an emergency stop anyone raised on the session halts
        // the browser host's input at once, independent of the Core's loop.
        if (ev.eventType === "EmergencyStopActivated" && ev.sessionId) browserHost.emergencyStop(ev.sessionId, String((ev.payload as { reason?: unknown } | null)?.reason ?? "emergency stop"));
        send("core:event", ev);
      };
      // Recovery: re-take the session lease (a new generation fences any stale
      // owner), re-attach the live subscription from the last cursor we delivered,
      // and tell the renderer exactly what the Core recovered (docs/39 PX-023).
      const local = loadLocalState();
      void handProviderToCore(c);
      void handCredentialsToCore(c);
      if (local.sessionId) void c.joinSessionLease(local.sessionId, `desktop ${app.getVersion()}`).catch(() => {});
      if (subscription) c.subscribe(subscription.sessionId, subscription.cursor);
      // M7.1: the views this process still holds attach again to their sessions.
      void browserHost.reattachAll();
      void c
        .getRecoveryReport()
        .then((r) =>
          send("core:recovery", {
            bootGeneration: r.bootGeneration.toString(),
            lastOffset: r.lastOffset.toString(),
            eventsVerified: r.eventsVerified.toString(),
            aggregatesVerified: r.aggregatesVerified.toString(),
            projectionsRebuilt: r.projectionsRebuilt,
            sessions: r.sessions.toString(),
            tasks: r.tasks.toString(),
            notes: r.notes,
            recoveryMs: r.recoveryMs.toString(),
          }),
        )
        .catch(() => {});
    },
  },
  app.getVersion(),
);

const HEX32 = /^[0-9a-f]{32}$/;

// M7.1: the browser host — one sandboxed WebContentsView per browser
// session, attached to the Core over this process's client connection.
// M7.8 (docs/22 "Credentials"): the credential broker — login secrets in
// safeStorage custody, bound to an origin; the Core learns handles only.
const credentials = new CredentialStore(join(dataDir, "credentials.enc"), {
  available: () => safeStorage.isEncryptionAvailable(),
  encrypt: (p) => safeStorage.encryptString(p).toString("base64"),
  decrypt: (c) => safeStorage.decryptString(Buffer.from(c, "base64")),
});
/** Register every handle with a (re)started Core: handle, label, origin, account name. */
async function handCredentialsToCore(c: CoreClient): Promise<void> {
  for (const h of credentials.list()) await c.registerBrowserCredential({ handle: h.handle, label: h.label, origin: h.origin, username: h.username }).catch(() => {});
}
const browserHost = new BrowserHost(
  () => win,
  () => supervisor.current(),
  (channel, payload) => send(channel, payload),
  (handle, pageOrigin) => credentials.secretFor(handle, pageOrigin),
);
function requireClient(): CoreClient {
  const c = supervisor.current();
  if (!c) throw new Error("CORE_UNAVAILABLE: the local Core is restarting; your last persisted state is shown");
  return c;
}
function requireSessionId(v: unknown): string {
  if (typeof v !== "string" || !HEX32.test(v)) throw new Error("BAD_ARGUMENT: session id must be 32 hex chars");
  return v;
}
function requireGoal(v: unknown): string {
  if (typeof v !== "string" || v.trim().length === 0 || v.length > 20_000) throw new Error("BAD_ARGUMENT: goal must be 1..20000 chars");
  return v.trim();
}
function requireTaskId(v: unknown): string {
  if (typeof v !== "string" || !HEX32.test(v)) throw new Error("BAD_ARGUMENT: task id must be 32 hex chars");
  return v;
}
function optionalWorkspaceRoot(v: unknown): string {
  if (v === undefined || v === null || v === "") return "";
  if (typeof v !== "string" || v.length > 4096 || v.includes("\0")) throw new Error("BAD_ARGUMENT: workspace root must be a path");
  return v;
}
function requireRelativePath(v: unknown): string {
  if (typeof v !== "string" || v.length === 0 || v.length > 4096 || v.startsWith("/") || v.includes("..") || v.includes("\0")) throw new Error("BAD_ARGUMENT: path must be workspace-relative");
  return v;
}

// Every handler validates its arguments (REQ-EV-0103: a malformed renderer
// message is rejected here, never forwarded).
//
// REQ-EV-0075 (docs/32 "Security settings"): every handler answers the app's
// own renderer only — the top frame of this window. A message from any other
// web contents or frame (a page in a browser view, a child frame, a window an
// attacker opened with the bridge) is refused before its arguments are read,
// and the refusal is on a bounded audit the renderer's diagnostics can show.
// The privileged effects themselves — the Core socket and its boot secret,
// the shell, the filesystem, credential values, the browser views' input —
// live in this process and the Core; the bridge names typed requests only.
const ipcRefusals: { channel: string; senderId: number; frameUrl: string; reason: string; atMs: number }[] = [];
function noteRefusal(channel: string, e: IpcMainInvokeEvent, reason: string): void {
  ipcRefusals.push({ channel, senderId: e.sender.id, frameUrl: e.senderFrame?.url ?? "", reason, atMs: Date.now() });
  if (ipcRefusals.length > 200) ipcRefusals.splice(0, ipcRefusals.length - 200);
}
function handle(channel: string, fn: (e: IpcMainInvokeEvent, ...args: unknown[]) => unknown): void {
  ipcMain.handle(channel, (e: IpcMainInvokeEvent, ...args: unknown[]) => {
    const appContents = win?.webContents;
    if (!appContents || appContents.isDestroyed() || e.sender !== appContents) {
      noteRefusal(channel, e, "not the app window");
      throw new Error("SENDER_REFUSED: only the app's own renderer may ask this");
    }
    if (!e.senderFrame || e.senderFrame !== appContents.mainFrame) {
      noteRefusal(channel, e, "not the top frame");
      throw new Error("SENDER_REFUSED: only the app's top frame may ask this");
    }
    return fn(e, ...args);
  });
}
handle("core:status", () => supervisor.status);
handle("core:localState", () => loadLocalState());
// Context Inspector (REQ-EV-0035 / 0131 / 0175): what the pack selected and
// excluded, and what the prompt envelope injected.
// Workspace context bridge (REQ-EV-0141 / 0160): what the reviewer has
// selected becomes context. Selection grants nothing; the Core enforces that.
handle(
  "task:select",
  async (_e: IpcMainInvokeEvent, sessionId: unknown, taskId: unknown, selection: unknown) => {
    const sid = requireSessionId(sessionId);
    const tid = requireTaskId(taskId);
    const s = selection as { paths?: unknown; symbol?: unknown; lineStart?: unknown; lineEnd?: unknown; reviewHunks?: unknown; source?: unknown };
    const strings = (v: unknown): string[] =>
      Array.isArray(v) ? v.filter((x): x is string => typeof x === "string" && x.length > 0 && x.length <= 4096) : [];
    const num = (v: unknown): number => (typeof v === "number" && Number.isInteger(v) && v >= 0 && v < 1_000_000 ? v : 0);
    const source = typeof s.source === "string" && ["review", "editor", "cli", "desktop"].includes(s.source) ? s.source : "desktop";
    return requireClient().setTaskSelection(sid, tid, {
      paths: strings(s.paths),
      symbol: typeof s.symbol === "string" && s.symbol.length <= 512 ? s.symbol : "",
      lineStart: num(s.lineStart),
      lineEnd: num(s.lineEnd),
      reviewHunks: strings(s.reviewHunks),
      source,
    });
  },
);

// PX-023: the typed status of one task (REQ-EV-0073) — what a fresh
// snapshot does not carry: the latest attention diagnostic, class, code,
// the user's action, the recovery path and the evidence refs.
handle("task:status", async (_e: IpcMainInvokeEvent, taskId: unknown) => {
  const tid = requireTaskId(taskId);
  const v = await requireClient().taskStatus(tid);
  return { state: v.state, waitReason: v.waitReason, runState: v.runState, loopAlive: v.loopAlive, lastOffset: v.lastOffset.toString(), attentionReason: v.attentionReason, failureClass: v.failureClass, failureCode: v.failureCode, retryable: v.retryable, userAction: v.userAction, recoveryPath: v.recoveryPath, evidenceRefs: v.evidenceRefs };
});
// Context efficiency metrics (REQ-EV-0173): quality and economics together.
handle("task:economics", async (_e: IpcMainInvokeEvent, taskId: unknown) => {
  const tid = requireTaskId(taskId);
  const v = await requireClient().taskEconomics(tid);
  return {
    state: v.state,
    verified: v.verified,
    checksPassed: v.checksPassed,
    checksFailed: v.checksFailed,
    model: v.model,
    modelCalls: v.modelCalls,
    inputTokens: v.inputTokens.toString(),
    cachedInputTokens: v.cachedInputTokens.toString(),
    outputTokens: v.outputTokens.toString(),
    costUsd: v.costUsd,
    pricingKnown: v.pricingKnown === 1,
    toolCalls: v.toolCalls,
    wallMs: v.wallMs.toString(),
    modelMs: v.modelMs.toString(),
    toolMs: v.toolMs.toString(),
    prefixCacheHits: v.prefixCacheHits,
    prefixCacheMisses: v.prefixCacheMisses,
    compactionEpochs: v.compactionEpochs,
    contextTokensInjected: v.contextTokensInjected.toString(),
  };
});

handle("context:inspector", async (_e: IpcMainInvokeEvent, taskId: unknown) => {
  const tid = requireTaskId(taskId);
  const v = await requireClient().contextInspector(tid);
  return {
    packId: v.packId,
    workspaceRevision: v.workspaceRevision.toString(),
    tokenBudget: v.tokenBudget,
    tokenUsed: v.tokenUsed,
    complete: v.complete,
    injectedTokens: v.injectedTokens.toString(),
    injectedRefs: v.injectedRefs,
    rejectedRefs: v.rejectedRefs,
    omittedCount: v.omittedCount,
    omittedPaths: v.omittedPaths,
    compactionEpoch: v.compactionEpoch,
    compactionEpochs: v.compactionEpochs,
    compactedEntries: v.compactedEntries.toString(),
    manifestRef: v.manifestRef,
    prefixCacheHits: v.prefixCacheHits,
    prefixCacheMisses: v.prefixCacheMisses,
    entries: v.entries.map((e) => ({
      entryId: e.entryId,
      sourceRef: e.sourceRef,
      path: e.path,
      lineStart: e.lineStart,
      lineEnd: e.lineEnd,
      reason: e.reason,
      sources: e.sources,
      freshness: e.freshness,
      tokenCost: e.tokenCost,
      injected: e.injected,
      used: e.used,
      stub: e.stub,
    })),
  };
});
// PX-026: honest language labels in every client (docs/76).
handle("languages:list", async () => {
  const r = await requireClient().listLanguages();
  return r.languages.map((l) => ({ language: l.language, tier: l.tier, label: l.label, fixture: l.fixture, proven: l.proven, provisional: l.provisional, notClaimed: l.notClaimed, note: l.note }));
});
// REQ-EV-0151 / 0275: attention items from the Core (canonical unresolved
// state only), rendered as the board's attention strip.
handle("attention:list", async (_e: IpcMainInvokeEvent, sessionId: unknown) => {
  const sid = requireSessionId(sessionId);
  const v = await requireClient().attention(sid);
  return {
    lastOffset: v.lastOffset.toString(),
    items: v.items.map((i) => ({ kind: i.kind, taskId: Buffer.from(i.taskId?.value ?? []).toString("hex"), reference: i.reference, reason: i.reason, action: i.action, sinceOffset: i.sinceOffset.toString() })),
  };
});
handle("session:create", async () => {
  const c = requireClient();
  const r = await c.createSession(freshId());
  await c.acquireSessionLease(r.sessionId, `desktop ${app.getVersion()}`);
  saveLocalState({ sessionId: r.sessionId });
  return r.sessionId;
});
handle("session:snapshot", async (_e: IpcMainInvokeEvent, sessionId: unknown) => {
  const sid = requireSessionId(sessionId);
  const s = await requireClient().getSessionSnapshot(sid);
  return {
    sessionId: sid,
    state: s.state,
    generation: Number(s.generation),
    lastOffset: s.lastOffset.toString(),
    tasks: s.tasks.map((t) => ({ taskId: Buffer.from(t.taskId?.value ?? []).toString("hex"), goalText: t.goalText, state: t.state, generation: Number(t.generation), createdAtMs: Number(t.createdAt?.seconds ?? 0n) * 1000, origin: t.origin, parentTaskId: t.parentTaskId ? Buffer.from(t.parentTaskId.value).toString("hex") : null })),
  };
});
handle("task:create", async (_e: IpcMainInvokeEvent, sessionId: unknown, goal: unknown, commandIdHex: unknown, workspaceRoot: unknown, issueUrl: unknown) => {
  const sid = requireSessionId(sessionId);
  // PX-010: from an issue, the goal may be empty (the Core names the task after it).
  const issue = typeof issueUrl === "string" && issueUrl.trim().length > 0 ? issueUrl.trim() : undefined;
  if (issue !== undefined && (issue.length > 2_000 || !/^https:\/\/[^\s/]+\/[^\s/]+\/[^\s/]+\/issues\/\d+$/.test(issue))) throw new Error("BAD_ARGUMENT: issue URL must be https://<host>/<owner>/<repo>/issues/<number>");
  const g = issue !== undefined && (typeof goal !== "string" || goal.trim().length === 0) ? "" : requireGoal(goal);
  const root = optionalWorkspaceRoot(workspaceRoot);
  // The renderer supplies a stable command id so a retry after a crash replays instead of duplicating.
  const cid = typeof commandIdHex === "string" && HEX32.test(commandIdHex) ? new Uint8Array(Buffer.from(commandIdHex, "hex")) : freshId();
  const c = requireClient();
  if (c.leaseGeneration(sid) === undefined) await c.joinSessionLease(sid, `desktop ${app.getVersion()}`);
  return c.createTask(sid, g, cid, root, issue);
});
handle("task:start", async (_e: IpcMainInvokeEvent, sessionId: unknown, taskId: unknown) => {
  const sid = requireSessionId(sessionId);
  const tid = requireTaskId(taskId);
  const c = requireClient();
  if (c.leaseGeneration(sid) === undefined) await c.joinSessionLease(sid, `desktop ${app.getVersion()}`);
  return c.startTask(sid, tid);
});
// PX-024: cancel and steer the focused task from the keyboard. Cancel is
// confirmed in the renderer before it reaches here; steering queues one
// line of input under the session lease (QueueInput STEER).
handle("task:cancel", async (_e: IpcMainInvokeEvent, sessionId: unknown, taskId: unknown) => {
  const sid = requireSessionId(sessionId);
  const tid = requireTaskId(taskId);
  const c = requireClient();
  if (c.leaseGeneration(sid) === undefined) await c.joinSessionLease(sid, `desktop ${app.getVersion()}`);
  return c.cancelTask(sid, tid);
});
handle("task:steer", async (_e: IpcMainInvokeEvent, sessionId: unknown, taskId: unknown, text: unknown) => {
  const sid = requireSessionId(sessionId);
  const tid = requireTaskId(taskId);
  if (typeof text !== "string" || text.trim().length === 0 || text.length > 20_000) throw new Error("BAD_ARGUMENT: text");
  const c = requireClient();
  if (c.leaseGeneration(sid) === undefined) await c.joinSessionLease(sid, `desktop ${app.getVersion()}`);
  const r = await c.queueInput(sid, tid, text, "STEER");
  return { sequence: r.sequence.toString(), offset: r.offset.toString() };
});
// REQ-EV-0190: attach a local file to a task. Main reads the bytes (bounded)
// and the Core normalizes them; the renderer never sees a filesystem.
const MAX_ATTACHMENT_BYTES = 3 * 1024 * 1024;
handle("task:attach", async (_e: IpcMainInvokeEvent, sessionId: unknown, taskId: unknown, filePath: unknown) => {
  const sid = requireSessionId(sessionId);
  const tid = requireTaskId(taskId);
  if (typeof filePath !== "string" || filePath.length === 0 || filePath.length > 4096) throw new Error("BAD_ARGUMENT: file path required");
  const abs = resolve(filePath);
  if (!existsSync(abs)) throw new Error("BAD_ARGUMENT: file does not exist");
  const data = readFileSync(abs);
  if (data.byteLength === 0 || data.byteLength > MAX_ATTACHMENT_BYTES) throw new Error(`BAD_ARGUMENT: attachment must be 1..${MAX_ATTACHMENT_BYTES} bytes`);
  const c = requireClient();
  if (c.leaseGeneration(sid) === undefined) await c.joinSessionLease(sid, `desktop ${app.getVersion()}`);
  return c.ingestAttachment(sid, tid, abs.split(/[\\/]/).pop() ?? "attachment", new Uint8Array(data));
});
// Review surface (docs/20): immutable, revision-bound payloads from the Core.
handle("review:bundle", async (_e: IpcMainInvokeEvent, taskId: unknown) => {
  const b = await requireClient().getReviewBundle(requireTaskId(taskId));
  return {
    taskId: Buffer.from(b.taskId?.value ?? []).toString("hex"),
    taskState: b.taskState,
    workspaceRevision: b.workspaceRevision.toString(),
    baseCommit: b.baseCommit,
    workspaceRoot: b.workspaceRoot,
    files: b.files.map((f) => ({ path: f.path, status: f.status, binary: f.binary, fileRevision: f.fileRevision, oldContentRef: f.oldContentRef, newContentRef: f.newContentRef, hunks: f.hunks.map((h) => ({ index: h.index, header: h.header, lines: h.lines })) })),
    planJson: b.planJson,
    selfReviewJson: b.selfReviewJson,
    verificationRuns: b.verificationRuns.map((v) => ({ id: v.verificationRunId, stage: v.stage, status: v.status, candidateRevision: v.candidateRevision, checks: v.checkIds.map((id, i) => ({ id, status: v.checkStatuses[i] ?? "" })) })),
    attributions: b.attributions,
    quarantined: b.quarantined,
    invariantFindings: b.invariantFindings,
    receipts: b.receipts,
    evidenceLinks: b.evidenceLinks,
  };
});
handle("review:codeView", async (_e: IpcMainInvokeEvent, taskId: unknown, path: unknown, expectedFileRevision: unknown) => {
  const v = await requireClient().getCodeView(requireTaskId(taskId), requireRelativePath(path), typeof expectedFileRevision === "string" ? expectedFileRevision : "");
  return { workspaceRevision: v.workspaceRevision.toString(), fileRevision: v.fileRevision, path: v.path, contentRef: v.contentRef, syntaxLanguage: v.syntaxLanguage, changedRanges: v.changedRanges, evidenceLinks: v.evidenceLinks, stale: v.stale, text: v.text };
});
handle("review:decide", async (_e: IpcMainInvokeEvent, sessionId: unknown, taskId: unknown, decision: unknown, rejected: unknown, note: unknown, expectedWorkspaceRevision: unknown) => {
  const sid = requireSessionId(sessionId);
  const tid = requireTaskId(taskId);
  if (decision !== "ACCEPT" && decision !== "RETURN") throw new Error("BAD_ARGUMENT: decision must be ACCEPT or RETURN");
  if (!Array.isArray(rejected) || rejected.length > 10_000) throw new Error("BAD_ARGUMENT: rejected must be a list");
  const rej = rejected.map((r) => {
    const x = r as { path?: unknown; index?: unknown };
    if (typeof x.path !== "string" || typeof x.index !== "number" || !Number.isInteger(x.index) || x.index < 0) throw new Error("BAD_ARGUMENT: rejected hunk must be {path, index}");
    return { path: requireRelativePath(x.path), index: x.index };
  });
  const n = typeof note === "string" && note.length <= 20_000 ? note : "";
  const rev = typeof expectedWorkspaceRevision === "string" && /^\d+$/.test(expectedWorkspaceRevision) ? BigInt(expectedWorkspaceRevision) : 0n;
  const c = requireClient();
  if (c.leaseGeneration(sid) === undefined) await c.joinSessionLease(sid, `desktop ${app.getVersion()}`);
  const d = await c.decideReview(sid, tid, decision, rej, n, rev);
  return { taskState: d.taskState, commit: d.commit, reverted: d.reverted, workspaceRevision: d.workspaceRevision.toString() };
});
// PX-005 (docs/20, docs/29): a person's one-hunk edit goes to the Core's
// ChangeTransaction bound to the revisions the review showed; main holds
// nothing of it after the call.
handle("review:patch", async (_e: IpcMainInvokeEvent, sessionId: unknown, taskId: unknown, patch: unknown) => {
  const sid = requireSessionId(sessionId);
  const tid = requireTaskId(taskId);
  const x = (patch ?? {}) as { path?: unknown; old?: unknown; new?: unknown; expectedWorkspaceRevision?: unknown; expectedFileRevision?: unknown };
  if (typeof x.old !== "string" || typeof x.new !== "string" || x.old.length === 0 || x.old.length > 262_144 || x.new.length > 262_144) throw new Error("BAD_ARGUMENT: patch needs old (non-empty) and new text, at most 256 KiB each");
  if (typeof x.expectedWorkspaceRevision !== "string" || !/^\d+$/.test(x.expectedWorkspaceRevision)) throw new Error("BAD_ARGUMENT: expectedWorkspaceRevision must be the revision the review showed");
  const c = requireClient();
  if (c.leaseGeneration(sid) === undefined) await c.joinSessionLease(sid, `desktop ${app.getVersion()}`);
  const r = await c.applyUserPatch(sid, tid, {
    path: requireRelativePath(x.path),
    old: x.old,
    new: x.new,
    expectedWorkspaceRevision: BigInt(x.expectedWorkspaceRevision),
    expectedFileRevision: typeof x.expectedFileRevision === "string" ? x.expectedFileRevision : "",
  });
  return { workspaceRevision: r.workspaceRevision.toString(), previousRevision: r.previousRevision.toString(), fileRevision: r.fileRevision, beforeHash: r.beforeHash, matchTier: r.matchTier, offset: r.offset.toString(), replayed: r.replayed };
});
// PX-007: the pull request from the accepted candidate. The first call
// returns APPROVAL_PENDING (the push and the POST are one protected effect
// bound to an intent hash); the renderer decides it through
// `approval:resolve` and calls again for OPENED/UPDATED — or DENIED, which
// leaves the branch local. The forge token never leaves the Core.
handle("review:pullRequest", async (_e: IpcMainInvokeEvent, sessionId: unknown, taskId: unknown, expectedCandidateRevision: unknown, update: unknown) => {
  const sid = requireSessionId(sessionId);
  const tid = requireTaskId(taskId);
  if (typeof expectedCandidateRevision !== "string" || !/^\d+$/.test(expectedCandidateRevision)) throw new Error("BAD_ARGUMENT: expectedCandidateRevision must be the revision the review accepted");
  const c = requireClient();
  if (c.leaseGeneration(sid) === undefined) await c.joinSessionLease(sid, `desktop ${app.getVersion()}`);
  const a = await c.openPullRequest(sid, tid, BigInt(expectedCandidateRevision), { update: update === true });
  return { status: a.status, approvalId: a.approvalId, intentHash: a.intentHash, number: a.number.toString(), url: a.url, branch: a.branch, headSha: a.headSha, candidateRevision: a.candidateRevision.toString(), receipts: a.effectReceiptIds.length, replayed: a.replayed, detail: a.detail };
});
// PX-001/PX-023: a decision on a protected effect names the intent hash the
// person saw; the Core refuses any other (INTENT_MISMATCH).
handle("approval:resolve", async (_e: IpcMainInvokeEvent, sessionId: unknown, approvalId: unknown, approve: unknown, reason: unknown, intentHash: unknown) => {
  const sid = requireSessionId(sessionId);
  // The Core names an approval as a UUID (hyphenated); the wire wants its bytes.
  const aid = typeof approvalId === "string" ? approvalId.replace(/-/g, "") : "";
  if (!/^[0-9a-f]{32}$/.test(aid)) throw new Error("BAD_ARGUMENT: approvalId");
  if (typeof intentHash !== "string" || intentHash.length === 0 || intentHash.length > 128) throw new Error("BAD_ARGUMENT: intentHash must be the intent the approval showed");
  const c = requireClient();
  if (c.leaseGeneration(sid) === undefined) await c.joinSessionLease(sid, `desktop ${app.getVersion()}`);
  const r = await c.resolveApproval(sid, aid, approve === true, typeof reason === "string" ? reason.slice(0, 2000) : "", intentHash);
  return { approvalId: Buffer.from(r.approvalId?.value ?? []).toString("hex"), status: r.status, offset: r.offset.toString() };
});
// REQ-EV-0222 / PX-023: the person's answer to the agent's typed question
// (an option id, or free text when the question allows it).
handle("question:respond", async (_e: IpcMainInvokeEvent, sessionId: unknown, taskId: unknown, questionId: unknown, optionId: unknown, text: unknown) => {
  const sid = requireSessionId(sessionId);
  const tid = requireTaskId(taskId);
  if (typeof questionId !== "string" || questionId.length === 0 || questionId.length > 128) throw new Error("BAD_ARGUMENT: questionId");
  const c = requireClient();
  if (c.leaseGeneration(sid) === undefined) await c.joinSessionLease(sid, `desktop ${app.getVersion()}`);
  return c.respondToQuestion(sid, tid, questionId, typeof optionId === "string" ? optionId.slice(0, 128) : "", typeof text === "string" ? text.slice(0, 20_000) : "");
});
// ---- M7.1 browser sessions (docs/22): the renderer asks main to open a
// session for a task and to place the view; everything the page does stays
// in main's sandboxed view and the Core's log.
handle("browser:open", async (_e: IpcMainInvokeEvent, sessionId: unknown, taskId: unknown) => {
  const sid = requireSessionId(sessionId);
  const tid = requireTaskId(taskId);
  const c = requireClient();
  if (c.leaseGeneration(sid) === undefined) await c.joinSessionLease(sid, `desktop ${app.getVersion()}`);
  return browserHost.open(sid, tid);
});
handle("browser:show", (_e: IpcMainInvokeEvent, browserSessionId: unknown, bounds: unknown) => {
  if (typeof browserSessionId !== "string" || !HEX32.test(browserSessionId)) throw new Error("BAD_ARGUMENT: browserSessionId");
  const b = (bounds ?? {}) as { x?: unknown; y?: unknown; width?: unknown; height?: unknown };
  const n = (v: unknown) => (typeof v === "number" && Number.isFinite(v) && v >= 0 && v <= 20_000 ? v : null);
  const rect = { x: n(b.x), y: n(b.y), width: n(b.width), height: n(b.height) };
  if (rect.x === null || rect.y === null || rect.width === null || rect.height === null) throw new Error("BAD_ARGUMENT: bounds");
  return browserHost.show(browserSessionId, rect as { x: number; y: number; width: number; height: number });
});
handle("browser:hide", (_e: IpcMainInvokeEvent, browserSessionId: unknown) => {
  if (typeof browserSessionId !== "string" || !HEX32.test(browserSessionId)) throw new Error("BAD_ARGUMENT: browserSessionId");
  browserHost.hide(browserSessionId);
});
handle("browser:close", async (_e: IpcMainInvokeEvent, browserSessionId: unknown) => {
  if (typeof browserSessionId !== "string" || !HEX32.test(browserSessionId)) throw new Error("BAD_ARGUMENT: browserSessionId");
  await browserHost.close(browserSessionId);
});
handle("browser:describe", (_e: IpcMainInvokeEvent, browserSessionId: unknown) => {
  if (typeof browserSessionId !== "string" || !HEX32.test(browserSessionId)) throw new Error("BAD_ARGUMENT: browserSessionId");
  return browserHost.describe(browserSessionId);
});
handle("browser:session", async (_e: IpcMainInvokeEvent, browserSessionId: unknown, taskId: unknown) => {
  if (typeof browserSessionId !== "string" || !HEX32.test(browserSessionId)) throw new Error("BAD_ARGUMENT: browserSessionId");
  const tid = requireTaskId(taskId);
  const v = await requireClient().browserSession(browserSessionId, tid);
  return { browserSessionId, taskId: tid, partition: v.partition, controller: v.controller, leaseGeneration: v.leaseGeneration.toString(), hostAttached: v.hostAttached, hostKind: v.hostKind, url: v.url, title: v.title, stateVersion: v.stateVersion.toString(), fingerprint: v.fingerprint, closed: v.closed };
});
handle("browser:log", () => browserHost.log.slice());
// IMP-EV-0085: the person's emergency stop — the host fences its input
// first (no model loop in the way), then the Core blocks every new effect.
handle("browser:emergencyStop", async (_e: IpcMainInvokeEvent, sessionId: unknown, reason: unknown) => {
  const sid = requireSessionId(sessionId);
  const why = typeof reason === "string" && reason.length <= 200 ? reason : "emergency stop";
  browserHost.emergencyStop(sid, why);
  const r = await requireClient().emergencyStop(sid, why);
  return { leasesRevoked: r.leasesRevoked, offset: r.offset.toString() };
});
// M7.6: the person takes or returns control of the session (the same
// session; the agent's input is blocked while they hold it).
handle("browser:control", (_e: IpcMainInvokeEvent, browserSessionId: unknown, controller: unknown) => {
  if (typeof browserSessionId !== "string" || !HEX32.test(browserSessionId)) throw new Error("BAD_ARGUMENT: browserSessionId");
  if (controller !== "AGENT" && controller !== "USER") throw new Error("BAD_ARGUMENT: controller must be AGENT or USER");
  return browserHost.setControl(browserSessionId, controller);
});
// The person's own typing into the view (what the E2E uses to type as the person while it holds control).
handle("browser:typeAsPerson", (_e: IpcMainInvokeEvent, browserSessionId: unknown, text: unknown) => {
  if (typeof browserSessionId !== "string" || !HEX32.test(browserSessionId)) throw new Error("BAD_ARGUMENT: browserSessionId");
  if (typeof text !== "string" || text.length > 200) throw new Error("BAD_ARGUMENT: text");
  return browserHost.typeAsPerson(browserSessionId, text);
});
handle("browser:probe", (_e: IpcMainInvokeEvent, browserSessionId: unknown) => {
  if (typeof browserSessionId !== "string" || !HEX32.test(browserSessionId)) throw new Error("BAD_ARGUMENT: browserSessionId");
  return browserHost.probe(browserSessionId);
});
// PX-023 notification delivery: the renderer decides what warrants an OS
// notification (opt-in per kind, quiet hours); main only hands the text to
// the OS and keeps a bounded log of what it delivered (what the E2E reads
// instead of watching the notification centre). No secret, no path, no
// Core payload is in a notification: title and one line.
const deliveredNotifications: { id: string; title: string; body: string; atMs: number; shown: boolean }[] = [];
handle("notify:deliver", (_e: IpcMainInvokeEvent, id: unknown, title: unknown, body: unknown) => {
  if (typeof id !== "string" || id.length > 128 || typeof title !== "string" || title.length > 200 || typeof body !== "string" || body.length > 1000) throw new Error("BAD_ARGUMENT: notification");
  const shown = Notification.isSupported() && process.env.MODBIT_SUPPRESS_OS_NOTIFICATIONS !== "1";
  if (shown) new Notification({ title, body, silent: true }).show();
  deliveredNotifications.push({ id, title, body, atMs: Date.now(), shown });
  if (deliveredNotifications.length > 200) deliveredNotifications.splice(0, deliveredNotifications.length - 200);
  return { shown };
});
handle("notify:log", () => deliveredNotifications.slice());
// ---- Credentials (M7.8): the secret crosses main once, from the renderer's
// input to safeStorage; what comes back is a handle. The Core is told the
// handle, label, origin and account name.
handle("credential:add", async (_e: IpcMainInvokeEvent, label: unknown, origin: unknown, username: unknown, secret: unknown) => {
  if (typeof label !== "string" || label.length > 200) throw new Error("BAD_ARGUMENT: label");
  if (typeof origin !== "string" || origin.length > 2048) throw new Error("BAD_ARGUMENT: origin");
  if (typeof username !== "string" || username.length > 200) throw new Error("BAD_ARGUMENT: username");
  if (typeof secret !== "string" || secret.length === 0 || secret.length > 4096 || secret.includes("\0")) throw new Error("BAD_ARGUMENT: secret");
  const h = credentials.add(label, origin, username, secret);
  await requireClient().registerBrowserCredential({ handle: h.handle, label: h.label, origin: h.origin, username: h.username });
  return h;
});
handle("credential:list", () => credentials.list());
handle("credential:remove", async (_e: IpcMainInvokeEvent, handle: unknown) => {
  if (typeof handle !== "string" || !/^cred_[0-9a-f]{12}$/.test(handle)) throw new Error("BAD_ARGUMENT: handle");
  const removed = credentials.remove(handle);
  await requireClient().forgetBrowserCredential(handle).catch(() => {});
  return { removed };
});
// ---- Onboarding (REQ-PX-022, docs/39): provider setup, repository trust,
// starter tasks. The credential crosses main once, from the renderer's input
// field to safeStorage and the Core; it is never returned to the renderer.
handle("onboarding:provider", async (_e: IpcMainInvokeEvent, provider: unknown, apiKey: unknown, baseUrl: unknown) => {
  if (typeof provider !== "string" || !["openai", "anthropic"].includes(provider)) throw new Error("BAD_ARGUMENT: provider must be openai or anthropic");
  if (typeof apiKey !== "string" || apiKey.length > 4096 || apiKey.includes("\0")) throw new Error("BAD_ARGUMENT: key");
  const url = typeof baseUrl === "string" && baseUrl.length <= 2048 && /^https?:\/\//.test(baseUrl) ? baseUrl : "";
  const c = requireClient();
  const configured = await c.configureProvider(provider, apiKey, url);
  const model = configured.models.find((m) => m.includes("mini")) ?? configured.models[0] ?? "";
  // The live test call: the provider answers, or the cause is named.
  const probe = await c.probeModel(configured.endpoint, model);
  const ok = probe.status === "COMPLETED";
  // What the live call could not confirm is not left registered: the Core
  // forgets the endpoint and the credential with it.
  if (!ok) await c.configureProvider(provider, "", "").catch(() => {});
  const stored = ok && apiKey.length > 0 ? storeProvider(provider, url, apiKey) : { persisted: false };
  return {
    endpoint: configured.endpoint,
    model,
    ok,
    errorCode: probe.errorCode,
    errorMessage: probe.errorMessage,
    persisted: stored.persisted,
    keychainAvailable: safeStorage.isEncryptionAvailable(),
  };
});
handle("onboarding:providerStatus", async () => {
  const rec = loadProvider();
  // What the Core actually has is the truth; the keychain record is only
  // what this profile will hand it on the next start.
  const endpoints = await requireClient().listProviders().catch(() => []);
  const ready = endpoints.filter((e) => e.credentialAvailable).map((e) => e.endpoint);
  return { configured: ready.length > 0, endpoints: [...new Set(ready)], stored: rec !== null, provider: rec?.provider ?? "", keychainAvailable: safeStorage.isEncryptionAvailable() };
});
handle("onboarding:trust", async (_e: IpcMainInvokeEvent, sessionId: unknown, workspaceRoot: unknown) => {
  const sid = requireSessionId(sessionId);
  const root = optionalWorkspaceRoot(workspaceRoot);
  if (!root) throw new Error("BAD_ARGUMENT: workspace root required");
  const abs = resolve(root);
  if (!existsSync(abs)) throw new Error("REPOSITORY_MISSING: that folder does not exist on this machine");
  const c = requireClient();
  if (c.leaseGeneration(sid) === undefined) await c.joinSessionLease(sid, `desktop ${app.getVersion()}`);
  const r = await c.trustRepository(sid, abs);
  return { workspaceRoot: abs, offset: r.offset };
});
handle("onboarding:starters", async (_e: IpcMainInvokeEvent, workspaceRoot: unknown) => {
  const root = optionalWorkspaceRoot(workspaceRoot);
  if (!root) throw new Error("BAD_ARGUMENT: workspace root required");
  return requireClient().listStarterTasks(resolve(root));
});
handle("events:subscribe", (_e: IpcMainInvokeEvent, sessionId: unknown, afterOffset: unknown) => {
  const sid = requireSessionId(sessionId);
  const after = typeof afterOffset === "string" && /^\d+$/.test(afterOffset) ? BigInt(afterOffset) : 0n;
  subscription = { sessionId: sid, cursor: after };
  requireClient().subscribe(sid, after);
});
handle("debug:coreInfo", () => {
  // Test hook: pid/endpoint only; never the secret.
  const s = supervisor.status;
  return s.state === "connected" ? { pid: s.pid, endpoint: s.endpoint } : null;
});
// Diagnostics (REQ-EV-0075/0076): what this process refused and how it
// brought its renderer back — for the renderer's diagnostics and the E2E.
handle("debug:ipcRefusals", () => ipcRefusals.slice());
handle("debug:rendererLog", () => rendererLog.slice());

// REQ-EV-0076 (docs/32): the renderer is a view of the Core's session, not
// its owner. When its process dies — a crash, the OS killing it — this
// process keeps the Core connection, the session lease and the browser views,
// notes what happened, and opens a fresh window in the old one's place; the
// new renderer takes the session snapshot from the Core and subscribes from
// its cursor, as at any start. Nothing of the session lives in the renderer
// to lose.
const rendererLog: { reason: string; exitCode: number; reloaded: boolean; atMs: number }[] = [];
function createWindow(bounds?: Electron.Rectangle): void {
  const w = new BrowserWindow({
    ...(bounds ?? { width: 1200, height: 800 }),
    show: true,
    webPreferences: {
      preload: join(__dirname, "..", "preload", "preload.cjs"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      webSecurity: true,
    },
  });
  win = w;
  w.webContents.on("will-navigate", (e) => e.preventDefault());
  w.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
  w.webContents.on("render-process-gone", (_e, details) => {
    const replace = details.reason !== "clean-exit" && win === w && !w.isDestroyed();
    rendererLog.push({ reason: details.reason, exitCode: details.exitCode, reloaded: replace, atMs: Date.now() });
    if (rendererLog.length > 50) rendererLog.splice(0, rendererLog.length - 50);
    if (!replace) return;
    // The views the dead renderer showed leave the old window first (they
    // are this process's, not the window's); the new renderer shows them
    // again when it opens its panel.
    browserHost.windowReplaced(w);
    createWindow(w.getBounds());
    w.destroy();
  });
  void w.loadFile(join(__dirname, "..", "renderer", "index.html"));
}

app.whenReady().then(async () => {
  session.defaultSession.webRequest.onHeadersReceived((details, cb) => {
    cb({ responseHeaders: { ...details.responseHeaders, "Content-Security-Policy": ["default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'"] } });
  });
  createWindow();
  await supervisor.start();
});

app.on("window-all-closed", () => {
  app.quit();
});
// Quitting (window close, Cmd+Q, or a harness closing the app) must stop the
// Core child and the socket, or the main process lingers.
app.on("before-quit", () => {
  browserHost.closeAll();
  supervisor.stop();
});
export type { WireEvent };
