/**
 * Electron main (docs/32 "Security settings"): the only process that talks to
 * the Core. It spawns/supervises `modbit-core`, holds the boot secret, and
 * exposes a narrow, validated IPC surface to the sandboxed renderer through the
 * preload bridge. The renderer gets durable ids and Core events; it never gets
 * the socket, the secret, Node, or the filesystem.
 */
import { app, BrowserWindow, ipcMain, safeStorage, session, type IpcMainInvokeEvent } from "electron";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { CoreSupervisor, type CoreStatus } from "./core-supervisor.js";
import { freshId, type CoreClient } from "./protocol-client.js";
import { serializeEvent, type WireEvent } from "./events.js";

const dataDir = process.env.MODBIT_DATA_DIR ?? join(app.getPath("userData"), "modbit");
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
        send("core:event", ev);
      };
      // Recovery: re-take the session lease (a new generation fences any stale
      // owner), re-attach the live subscription from the last cursor we delivered,
      // and tell the renderer exactly what the Core recovered (docs/39 PX-023).
      const local = loadLocalState();
      void handProviderToCore(c);
      if (local.sessionId) void c.acquireSessionLease(local.sessionId, `desktop ${app.getVersion()}`).catch(() => {});
      if (subscription) c.subscribe(subscription.sessionId, subscription.cursor);
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
ipcMain.handle("core:status", () => supervisor.status);
ipcMain.handle("core:localState", () => loadLocalState());
// Context Inspector (REQ-EV-0035 / 0131 / 0175): what the pack selected and
// excluded, and what the prompt envelope injected.
// Workspace context bridge (REQ-EV-0141 / 0160): what the reviewer has
// selected becomes context. Selection grants nothing; the Core enforces that.
ipcMain.handle(
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

// Context efficiency metrics (REQ-EV-0173): quality and economics together.
ipcMain.handle("task:economics", async (_e: IpcMainInvokeEvent, taskId: unknown) => {
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

ipcMain.handle("context:inspector", async (_e: IpcMainInvokeEvent, taskId: unknown) => {
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
ipcMain.handle("languages:list", async () => {
  const r = await requireClient().listLanguages();
  return r.languages.map((l) => ({ language: l.language, tier: l.tier, label: l.label, fixture: l.fixture, proven: l.proven, provisional: l.provisional, notClaimed: l.notClaimed, note: l.note }));
});
ipcMain.handle("session:create", async () => {
  const c = requireClient();
  const r = await c.createSession(freshId());
  await c.acquireSessionLease(r.sessionId, `desktop ${app.getVersion()}`);
  saveLocalState({ sessionId: r.sessionId });
  return r.sessionId;
});
ipcMain.handle("session:snapshot", async (_e: IpcMainInvokeEvent, sessionId: unknown) => {
  const sid = requireSessionId(sessionId);
  const s = await requireClient().getSessionSnapshot(sid);
  return {
    sessionId: sid,
    state: s.state,
    generation: Number(s.generation),
    lastOffset: s.lastOffset.toString(),
    tasks: s.tasks.map((t) => ({ taskId: Buffer.from(t.taskId?.value ?? []).toString("hex"), goalText: t.goalText, state: t.state, generation: Number(t.generation), createdAtMs: Number(t.createdAt?.seconds ?? 0n) * 1000 })),
  };
});
ipcMain.handle("task:create", async (_e: IpcMainInvokeEvent, sessionId: unknown, goal: unknown, commandIdHex: unknown, workspaceRoot: unknown) => {
  const sid = requireSessionId(sessionId);
  const g = requireGoal(goal);
  const root = optionalWorkspaceRoot(workspaceRoot);
  // The renderer supplies a stable command id so a retry after a crash replays instead of duplicating.
  const cid = typeof commandIdHex === "string" && HEX32.test(commandIdHex) ? new Uint8Array(Buffer.from(commandIdHex, "hex")) : freshId();
  const c = requireClient();
  if (c.leaseGeneration(sid) === undefined) await c.acquireSessionLease(sid, `desktop ${app.getVersion()}`);
  return c.createTask(sid, g, cid, root);
});
ipcMain.handle("task:start", async (_e: IpcMainInvokeEvent, sessionId: unknown, taskId: unknown) => {
  const sid = requireSessionId(sessionId);
  const tid = requireTaskId(taskId);
  const c = requireClient();
  if (c.leaseGeneration(sid) === undefined) await c.acquireSessionLease(sid, `desktop ${app.getVersion()}`);
  return c.startTask(sid, tid);
});
// REQ-EV-0190: attach a local file to a task. Main reads the bytes (bounded)
// and the Core normalizes them; the renderer never sees a filesystem.
const MAX_ATTACHMENT_BYTES = 3 * 1024 * 1024;
ipcMain.handle("task:attach", async (_e: IpcMainInvokeEvent, sessionId: unknown, taskId: unknown, filePath: unknown) => {
  const sid = requireSessionId(sessionId);
  const tid = requireTaskId(taskId);
  if (typeof filePath !== "string" || filePath.length === 0 || filePath.length > 4096) throw new Error("BAD_ARGUMENT: file path required");
  const abs = resolve(filePath);
  if (!existsSync(abs)) throw new Error("BAD_ARGUMENT: file does not exist");
  const data = readFileSync(abs);
  if (data.byteLength === 0 || data.byteLength > MAX_ATTACHMENT_BYTES) throw new Error(`BAD_ARGUMENT: attachment must be 1..${MAX_ATTACHMENT_BYTES} bytes`);
  const c = requireClient();
  if (c.leaseGeneration(sid) === undefined) await c.acquireSessionLease(sid, `desktop ${app.getVersion()}`);
  return c.ingestAttachment(sid, tid, abs.split(/[\\/]/).pop() ?? "attachment", new Uint8Array(data));
});
// Review surface (docs/20): immutable, revision-bound payloads from the Core.
ipcMain.handle("review:bundle", async (_e: IpcMainInvokeEvent, taskId: unknown) => {
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
ipcMain.handle("review:codeView", async (_e: IpcMainInvokeEvent, taskId: unknown, path: unknown, expectedFileRevision: unknown) => {
  const v = await requireClient().getCodeView(requireTaskId(taskId), requireRelativePath(path), typeof expectedFileRevision === "string" ? expectedFileRevision : "");
  return { workspaceRevision: v.workspaceRevision.toString(), fileRevision: v.fileRevision, path: v.path, contentRef: v.contentRef, syntaxLanguage: v.syntaxLanguage, changedRanges: v.changedRanges, evidenceLinks: v.evidenceLinks, stale: v.stale, text: v.text };
});
ipcMain.handle("review:decide", async (_e: IpcMainInvokeEvent, sessionId: unknown, taskId: unknown, decision: unknown, rejected: unknown, note: unknown, expectedWorkspaceRevision: unknown) => {
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
  if (c.leaseGeneration(sid) === undefined) await c.acquireSessionLease(sid, `desktop ${app.getVersion()}`);
  const d = await c.decideReview(sid, tid, decision, rej, n, rev);
  return { taskState: d.taskState, commit: d.commit, reverted: d.reverted, workspaceRevision: d.workspaceRevision.toString() };
});
// ---- Onboarding (REQ-PX-022, docs/39): provider setup, repository trust,
// starter tasks. The credential crosses main once, from the renderer's input
// field to safeStorage and the Core; it is never returned to the renderer.
ipcMain.handle("onboarding:provider", async (_e: IpcMainInvokeEvent, provider: unknown, apiKey: unknown, baseUrl: unknown) => {
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
ipcMain.handle("onboarding:providerStatus", async () => {
  const rec = loadProvider();
  // What the Core actually has is the truth; the keychain record is only
  // what this profile will hand it on the next start.
  const endpoints = await requireClient().listProviders().catch(() => []);
  const ready = endpoints.filter((e) => e.credentialAvailable).map((e) => e.endpoint);
  return { configured: ready.length > 0, endpoints: [...new Set(ready)], stored: rec !== null, provider: rec?.provider ?? "", keychainAvailable: safeStorage.isEncryptionAvailable() };
});
ipcMain.handle("onboarding:trust", async (_e: IpcMainInvokeEvent, sessionId: unknown, workspaceRoot: unknown) => {
  const sid = requireSessionId(sessionId);
  const root = optionalWorkspaceRoot(workspaceRoot);
  if (!root) throw new Error("BAD_ARGUMENT: workspace root required");
  const abs = resolve(root);
  if (!existsSync(abs)) throw new Error("REPOSITORY_MISSING: that folder does not exist on this machine");
  const c = requireClient();
  if (c.leaseGeneration(sid) === undefined) await c.acquireSessionLease(sid, `desktop ${app.getVersion()}`);
  const r = await c.trustRepository(sid, abs);
  return { workspaceRoot: abs, offset: r.offset };
});
ipcMain.handle("onboarding:starters", async (_e: IpcMainInvokeEvent, workspaceRoot: unknown) => {
  const root = optionalWorkspaceRoot(workspaceRoot);
  if (!root) throw new Error("BAD_ARGUMENT: workspace root required");
  return requireClient().listStarterTasks(resolve(root));
});
ipcMain.handle("events:subscribe", (_e: IpcMainInvokeEvent, sessionId: unknown, afterOffset: unknown) => {
  const sid = requireSessionId(sessionId);
  const after = typeof afterOffset === "string" && /^\d+$/.test(afterOffset) ? BigInt(afterOffset) : 0n;
  subscription = { sessionId: sid, cursor: after };
  requireClient().subscribe(sid, after);
});
ipcMain.handle("debug:coreInfo", () => {
  // Test hook: pid/endpoint only; never the secret.
  const s = supervisor.status;
  return s.state === "connected" ? { pid: s.pid, endpoint: s.endpoint } : null;
});

function createWindow(): void {
  win = new BrowserWindow({
    width: 1200,
    height: 800,
    show: true,
    webPreferences: {
      preload: join(__dirname, "..", "preload", "preload.cjs"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      webSecurity: true,
    },
  });
  win.webContents.on("will-navigate", (e) => e.preventDefault());
  win.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
  void win.loadFile(join(__dirname, "..", "renderer", "index.html"));
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
  supervisor.stop();
});
export type { WireEvent };
