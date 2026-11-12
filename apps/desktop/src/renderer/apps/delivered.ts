/**
 * What this window has been given about each task (REQ-PX-202; DR-PX-2026-11-11-021 MWS-10, the Activity pane): the Core's
 * events as main pushed them, and the transcript rows the open conversation read. A view buffer, not a store: it is held in
 * memory only, bounded, never written anywhere, empty after a reload until the Core delivers again, and nothing in the app
 * takes a decision from it. It makes no read of its own. The Activity pane lists it and reads nothing else.
 */
import { useSyncExternalStore } from "react";
import type { TranscriptRowView } from "../../shared/conversation-types.ts";

/** One event as it was delivered. The payload is kept as the Core sent it (the Core redacts what it sends). */
export interface DeliveredEvent {
  offset: string;
  eventType: string;
  aggregateType: string;
  occurredAtMs: number;
  payload: unknown;
}

export interface Delivered {
  events: readonly DeliveredEvent[];
  rows: readonly TranscriptRowView[];
  /** The log offset the rows were projected at; "0" before any transcript was delivered. */
  asOfOffset: string;
}

export const MAX_EVENTS_PER_TASK = 2000;
export const MAX_TASKS = 64;
const NONE: Delivered = { events: [], rows: [], asOfOffset: "0" };

/** A piece of a message that is still being written: the message is listed once it is whole, a piece is not an entry. */
const isPiece = (eventType: string): boolean => eventType === "AssistantTextDelta";

interface WireEvent {
  offset?: unknown;
  eventType?: unknown;
  aggregateType?: unknown;
  taskId?: unknown;
  occurredAtMs?: unknown;
  payload?: unknown;
}

export interface DeliveredStore {
  /** Takes one pushed event. Returns whether it was kept (it names a task, is not a piece of a message and was not already delivered). */
  event(raw: unknown): boolean;
  /** Takes the rows a conversation read for a task, replacing the ones before. */
  transcript(taskId: string, rows: readonly TranscriptRowView[], asOfOffset: string): void;
  get(taskId: string): Delivered;
  subscribe(cb: () => void): () => void;
}

export function createDeliveredStore(): DeliveredStore {
  const byTask = new Map<string, Delivered>();
  const listeners = new Set<() => void>();
  const put = (taskId: string, next: Delivered) => {
    // The newest task goes last; the oldest falls out once there are too many.
    byTask.delete(taskId);
    byTask.set(taskId, next);
    if (byTask.size > MAX_TASKS) byTask.delete(byTask.keys().next().value as string);
    for (const l of listeners) l();
  };
  return {
    event(raw) {
      const e = (raw ?? {}) as WireEvent;
      if (typeof e.taskId !== "string" || !e.taskId || typeof e.eventType !== "string" || typeof e.offset !== "string" || !/^\d+$/.test(e.offset)) return false;
      if (isPiece(e.eventType)) return false;
      const cur = byTask.get(e.taskId) ?? NONE;
      if (cur.events.some((x) => x.offset === e.offset)) return false;
      const ev: DeliveredEvent = { offset: e.offset, eventType: e.eventType, aggregateType: typeof e.aggregateType === "string" ? e.aggregateType : "", occurredAtMs: typeof e.occurredAtMs === "number" ? e.occurredAtMs : 0, payload: e.payload ?? null };
      const events = [...cur.events, ev];
      // The Core delivers in offset order; a reconnect can deliver an earlier one late.
      if (cur.events.length > 0 && BigInt(cur.events[cur.events.length - 1]!.offset) > BigInt(ev.offset)) events.sort((a, b) => (BigInt(a.offset) < BigInt(b.offset) ? -1 : 1));
      put(e.taskId, { ...cur, events: events.slice(-MAX_EVENTS_PER_TASK) });
      return true;
    },
    transcript(taskId, rows, asOfOffset) {
      if (!taskId) return;
      const cur = byTask.get(taskId) ?? NONE;
      if (cur.rows === rows && cur.asOfOffset === asOfOffset) return;
      put(taskId, { ...cur, rows, asOfOffset });
    },
    get: (taskId) => byTask.get(taskId) ?? NONE,
    subscribe(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
  };
}

/** The window's one buffer. The shell feeds it the pushed events; the open conversation feeds it the rows it read. */
export const delivered = createDeliveredStore();

/** Feeds the buffer from the event subscription the rest of the renderer uses. Returns the unsubscribe. */
export function followDelivered(): () => void {
  return window.modbit.onEvent((raw) => void delivered.event(raw));
}

export function useDelivered(taskId: string): Delivered {
  return useSyncExternalStore(delivered.subscribe, () => delivered.get(taskId));
}
