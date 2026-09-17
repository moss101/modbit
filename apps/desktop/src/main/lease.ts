/**
 * The control lease as this host applies it (M7.6, docs/22 "Live user
 * takeover"). Pure: no Electron here, so it is unit-tested.
 */
/** Whether an agent input stamped `requestGeneration` may run under the
 *  lease this host knows (`current`, held by `controller`). Pure, so it is
 *  testable: a stale generation is refused even when the agent holds
 *  control again — an input from before a takeover never lands after it. */
export function admitsAgentInput(requestGeneration: number, current: number, controller: "AGENT" | "USER"): { ok: true } | { ok: false; code: "USER_HAS_CONTROL" | "STALE_GENERATION" } {
  if (requestGeneration < current) return { ok: false, code: "STALE_GENERATION" };
  if (controller === "USER") return { ok: false, code: "USER_HAS_CONTROL" };
  return { ok: true };
}

