/** Wire event → plain, structured-clone-safe object for the renderer. */
import type { StoredEventFrame } from "@modbit/surface-protocol";

export interface WireEvent {
  offset: string;
  sequence: string;
  eventType: string;
  aggregateId: string;
  aggregateType: string;
  taskId: string | null;
  sessionId: string;
  /** Decoded JSON payload (the Core stores inline JSON or an object ref). */
  payload: unknown;
  occurredAtMs: number;
  /** When the main process read the frame off the Core's socket, epoch ms: the latency budgets split into the Core's side and the renderer's by it. */
  receivedAtMs: number;
}

export function serializeEvent(e: StoredEventFrame): WireEvent {
  const ev = e.event;
  let payload: unknown = null;
  try {
    const p = JSON.parse(Buffer.from(ev?.payload ?? []).toString("utf8")) as { kind?: string; payload?: unknown };
    payload = p.kind === "inline" ? p.payload : p;
  } catch {
    payload = null;
  }
  return {
    offset: e.offset.toString(),
    sequence: (ev?.sequence ?? 0n).toString(),
    eventType: ev?.eventType ?? "",
    aggregateId: Buffer.from(ev?.aggregateId?.value ?? []).toString("hex"),
    aggregateType: ev?.aggregateType ?? "",
    taskId: ev?.taskId ? Buffer.from(ev.taskId.value).toString("hex") : null,
    sessionId: Buffer.from(ev?.sessionId?.value ?? []).toString("hex"),
    payload,
    // Millisecond precision: the interaction budgets (docs/65 AFW-K01) are measured from this stamp to the frame that shows the event.
    receivedAtMs: Date.now(),
    occurredAtMs: Number(ev?.recordedAt?.seconds ?? 0n) * 1000 + Math.floor((ev?.recordedAt?.nanos ?? 0) / 1_000_000),
  };
}
