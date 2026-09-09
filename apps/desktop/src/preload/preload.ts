/**
 * Preload bridge (docs/32): exposes only the SurfaceProtocol functions the
 * renderer needs, over Electron's validated IPC. No Node, no fs, no shell.
 */
import { contextBridge, ipcRenderer } from "electron";

export interface ModbitBridge {
  coreStatus(): Promise<unknown>;
  localState(): Promise<{ sessionId?: string }>;
  createSession(): Promise<string>;
  sessionSnapshot(sessionId: string): Promise<unknown>;
  createTask(sessionId: string, goal: string, commandIdHex: string): Promise<{ taskId: string; offset: bigint; replayed: boolean }>;
  subscribe(sessionId: string, afterOffset: string): Promise<void>;
  onEvent(cb: (e: unknown) => void): () => void;
  onCoreStatus(cb: (s: unknown) => void): () => void;
  debugCoreInfo(): Promise<{ pid: number; endpoint: string } | null>;
}

const bridge: ModbitBridge = {
  coreStatus: () => ipcRenderer.invoke("core:status"),
  localState: () => ipcRenderer.invoke("core:localState"),
  createSession: () => ipcRenderer.invoke("session:create"),
  sessionSnapshot: (sessionId) => ipcRenderer.invoke("session:snapshot", sessionId),
  createTask: (sessionId, goal, commandIdHex) => ipcRenderer.invoke("task:create", sessionId, goal, commandIdHex),
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
  debugCoreInfo: () => ipcRenderer.invoke("debug:coreInfo"),
};

contextBridge.exposeInMainWorld("modbit", bridge);
