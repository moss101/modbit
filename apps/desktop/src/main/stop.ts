/**
 * The browser host's emergency-stop lifecycle (IMP-EV-0085, docs/22). Pure:
 * no Electron here, so it is unit-tested.
 *
 * A stop is raised on a session — from the Browser panel or by any
 * `EmergencyStopActivated` on the log — and fences the host's agent input
 * for it. The panel tells the person the stop holds "until a new session
 * lease is taken"; this is where that is true: the stop remembers the
 * highest session lease generation the host had seen when it was raised, and
 * a `SessionLeaseAcquired` above it lifts the host's fence. Until then, and
 * across any number of replays of the same events, the stop stands.
 *
 * The Core is the authority for what runs: its own stop (every new effect
 * of the session refused, `StartTask` refused) is a projection of the log and
 * is not this host's to lift; the host never admits what the Core refuses, it
 * only stops refusing what the Core allows.
 */
export class StopRegistry {
  private readonly stops = new Map<string, { reason: string; atLease: number }>();
  private readonly leases = new Map<string, number>();

  /** Raise (or restate) the stop on `sessionId`. The first raise fixes the lease generation it lifts above. */
  stop(sessionId: string, reason: string): void {
    const why = reason || "emergency stop";
    const standing = this.stops.get(sessionId);
    this.stops.set(sessionId, { reason: why, atLease: standing?.atLease ?? this.leases.get(sessionId) ?? 0 });
  }

  /**
   * A session lease of `generation` was acquired on `sessionId` (a
   * `SessionLeaseAcquired` event). Returns true when it lifted a standing
   * stop. A generation at or below the one the stop was raised under (a
   * replayed or stale event) lifts nothing.
   */
  leaseAcquired(sessionId: string, generation: number): boolean {
    if (!Number.isFinite(generation) || generation <= 0) return false;
    if (generation > (this.leases.get(sessionId) ?? 0)) this.leases.set(sessionId, generation);
    const standing = this.stops.get(sessionId);
    if (!standing || generation <= standing.atLease) return false;
    this.stops.delete(sessionId);
    return true;
  }

  /** The reason the session is stopped for, or null. */
  reason(sessionId: string): string | null {
    return this.stops.get(sessionId)?.reason ?? null;
  }
}
