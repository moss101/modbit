/**
 * The control lease as this host applies it (M7.6, docs/22 "Live user
 * takeover"). Pure: no Electron here, so it is unit-tested.
 */
/** Whether an agent input stamped `requestGeneration` may run under the
 *  lease this host knows (`current`, held by `controller`). Pure, so it is
 *  testable: a stale generation is refused even when the agent holds
 *  control again — an input from before a takeover never lands after it. */
export function admitsAgentInput(requestGeneration: number, current: number, controller: "AGENT" | "USER"): { ok: true } | { ok: false; code: "HUMAN_ACTIVE" | "STALE_GENERATION" } {
  if (requestGeneration < current) return { ok: false, code: "STALE_GENERATION" };
  if (controller === "USER") return { ok: false, code: "HUMAN_ACTIVE" };
  return { ok: true };
}

/**
 * Whether a raw input event from the view is the person acting (IMP-EV-0087):
 * a key press, a mouse button press or a scroll wheel turn. Hovering is not
 * acting; a key release or a mouse release only ends what a press began.
 * Pure, so it is testable without Electron. The caller still ignores what the
 * host itself is dispatching for the agent.
 */
export function isHumanInput(type: string): boolean {
  return type === "keyDown" || type === "mouseDown" || type === "mouseWheel";
}

