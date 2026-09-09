/**
 * Electron main (docs/32 "Security settings"): the only process that talks to
 * the Core. It spawns/supervises `modbit-core`, holds the boot secret, and
 * exposes a narrow, validated IPC surface to the sandboxed renderer through the
 * preload bridge. The renderer gets durable ids and Core events; it never gets
 * the socket, the secret, Node, or the filesystem.
 */
import { app, BrowserWindow, ipcMain, session, type IpcMainInvokeEvent } from "electron";
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
      // Recovery: re-attach the live subscription from the last cursor we delivered,
      // and tell the renderer exactly what the Core recovered (docs/39 PX-023).
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

// Every handler validates its arguments (REQ-EV-0103: a malformed renderer
// message is rejected here, never forwarded).
ipcMain.handle("core:status", () => supervisor.status);
ipcMain.handle("core:localState", () => loadLocalState());
ipcMain.handle("session:create", async () => {
  const r = await requireClient().createSession(freshId());
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
ipcMain.handle("task:create", async (_e: IpcMainInvokeEvent, sessionId: unknown, goal: unknown, commandIdHex: unknown) => {
  const sid = requireSessionId(sessionId);
  const g = requireGoal(goal);
  // The renderer supplies a stable command id so a retry after a crash replays instead of duplicating.
  const cid = typeof commandIdHex === "string" && HEX32.test(commandIdHex) ? new Uint8Array(Buffer.from(commandIdHex, "hex")) : freshId();
  return requireClient().createTask(sid, g, cid);
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
