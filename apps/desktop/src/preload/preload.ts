/**
 * Preload bridge (docs/32): exposes only the SurfaceProtocol functions the
 * renderer needs, over Electron's validated IPC. No Node, no fs, no shell.
 */
import { contextBridge, ipcRenderer } from "electron";

export interface ReviewHunk {
  index: number;
  header: string;
  lines: string[];
}
export interface ReviewFileView {
  path: string;
  status: string;
  binary: boolean;
  fileRevision: string;
  oldContentRef: string;
  newContentRef: string;
  hunks: ReviewHunk[];
}
export interface ReviewBundleView {
  taskId: string;
  taskState: string;
  workspaceRevision: string;
  baseCommit: string;
  workspaceRoot: string;
  files: ReviewFileView[];
  planJson: string;
  selfReviewJson: string;
  verificationRuns: { id: string; stage: string; status: string; candidateRevision: string; checks: { id: string; status: string }[] }[];
  attributions: string[];
  quarantined: string[];
  invariantFindings: string[];
  receipts: number;
  evidenceLinks: string[];
}
export interface CodeView {
  workspaceRevision: string;
  fileRevision: string;
  path: string;
  contentRef: string;
  syntaxLanguage: string;
  changedRanges: number[];
  evidenceLinks: string[];
  stale: boolean;
  text: string;
}

export interface ModbitBridge {
  coreStatus(): Promise<unknown>;
  localState(): Promise<{ sessionId?: string }>;
  createSession(): Promise<string>;
  sessionSnapshot(sessionId: string): Promise<unknown>;
  createTask(sessionId: string, goal: string, commandIdHex: string, workspaceRoot?: string): Promise<{ taskId: string; offset: bigint; replayed: boolean }>;
  startTask(sessionId: string, taskId: string): Promise<{ runId: string; resumed: boolean; endpoint: string; model: string }>;
  reviewBundle(taskId: string): Promise<ReviewBundleView>;
  codeView(taskId: string, path: string, expectedFileRevision?: string): Promise<CodeView>;
  decideReview(sessionId: string, taskId: string, decision: "ACCEPT" | "RETURN", rejected: { path: string; index: number }[], note: string, expectedWorkspaceRevision: string): Promise<{ taskState: string; commit: string; reverted: string[]; workspaceRevision: string }>;
  subscribe(sessionId: string, afterOffset: string): Promise<void>;
  onEvent(cb: (e: unknown) => void): () => void;
  onCoreStatus(cb: (s: unknown) => void): () => void;
  onRecovery(cb: (r: unknown) => void): () => void;
  debugCoreInfo(): Promise<{ pid: number; endpoint: string } | null>;
}

const bridge: ModbitBridge = {
  coreStatus: () => ipcRenderer.invoke("core:status"),
  localState: () => ipcRenderer.invoke("core:localState"),
  createSession: () => ipcRenderer.invoke("session:create"),
  sessionSnapshot: (sessionId) => ipcRenderer.invoke("session:snapshot", sessionId),
  createTask: (sessionId, goal, commandIdHex, workspaceRoot) => ipcRenderer.invoke("task:create", sessionId, goal, commandIdHex, workspaceRoot ?? ""),
  startTask: (sessionId, taskId) => ipcRenderer.invoke("task:start", sessionId, taskId),
  reviewBundle: (taskId) => ipcRenderer.invoke("review:bundle", taskId),
  codeView: (taskId, path, expectedFileRevision) => ipcRenderer.invoke("review:codeView", taskId, path, expectedFileRevision ?? ""),
  decideReview: (sessionId, taskId, decision, rejected, note, expectedWorkspaceRevision) => ipcRenderer.invoke("review:decide", sessionId, taskId, decision, rejected, note, expectedWorkspaceRevision),
  subscribe: (sessionId, afterOffset) => ipcRenderer.invoke("events:subscribe", sessionId, afterOffset),
  onEvent: (cb) => {
    const listener = (_: unknown, e: unknown) => cb(e);
    ipcRenderer.on("core:event", listener);
    return () => ipcRenderer.removeListener("core:event", listener);
  },
  onCoreStatus: (cb) => {
    const listener = (_: unknown, s: unknown) => cb(s);
    ipcRenderer.on("core:status", listener);
    return () => ipcRenderer.removeListener("core:status", listener);
  },
  onRecovery: (cb) => {
    const listener = (_: unknown, r: unknown) => cb(r);
    ipcRenderer.on("core:recovery", listener);
    return () => ipcRenderer.removeListener("core:recovery", listener);
  },
  debugCoreInfo: () => ipcRenderer.invoke("debug:coreInfo"),
};

contextBridge.exposeInMainWorld("modbit", bridge);
