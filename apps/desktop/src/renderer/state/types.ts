import { DEFAULT_PREFERENCES, type NotificationPreferences } from "../notifications.ts";
import type { ModbitBridge } from "../../preload/preload.ts";

declare global {
  interface Window {
    modbit: ModbitBridge;
  }
}

export type CoreStatus =
  | { state: "starting"; restarts: number }
  | { state: "connected"; pid: number; endpoint: string; restarts: number }
  | { state: "restarting"; reason: string; restarts: number; retryInMs: number }
  | { state: "failed"; reason: string; restarts: number };

export type ScreenState = "loading" | "empty" | "populated" | "error";

export interface RecoveryInfo {
  bootGeneration: string;
  lastOffset: string;
  eventsVerified: string;
  aggregatesVerified: string;
  projectionsRebuilt: boolean;
  sessions: string;
  tasks: string;
  notes: string[];
  recoveryMs: string;
}

export function hex32(): string {
  const b = new Uint8Array(16);
  crypto.getRandomValues(b);
  return Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
}

export const PREFS_KEY = "modbit.notifications";
export function loadPreferences(): NotificationPreferences {
  try {
    const raw = localStorage.getItem(PREFS_KEY);
    if (!raw) return DEFAULT_PREFERENCES;
    const p = JSON.parse(raw) as Partial<NotificationPreferences>;
    return { os: { ...DEFAULT_PREFERENCES.os, ...(p.os ?? {}) }, quietHours: p.quietHours ?? null };
  } catch {
    return DEFAULT_PREFERENCES;
  }
}


