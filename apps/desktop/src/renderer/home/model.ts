/**
 * Home's view model (REQ-PX-205): pure words and counts over what the Core and the app say. Nothing here decides anything.
 */

export interface StepFacts {
  provider: boolean;
  repository: boolean;
  firstTask: boolean;
}

/** How many of the steps are done, and the one to do next ("" when all are). */
export function stepProgress(f: StepFacts): { done: number; total: number; next: keyof StepFacts | "" } {
  const order: (keyof StepFacts)[] = ["provider", "repository", "firstTask"];
  const done = order.filter((k) => f[k]).length;
  return { done, total: order.length, next: order.find((k) => !f[k]) ?? "" };
}

/** What an attention item that is not an approval means for the person, in words (the Core's own kind stays on the row as data). */
export function attentionWords(i: { kind: string; reason: string }): string {
  switch (i.kind) {
    case "QUESTION":
      return `Asks you: ${i.reason}`;
    case "CAPACITY":
      return "Waiting for a free run slot";
    case "STALL":
      return "Stopped making progress; it needs a steer";
    case "RESTART":
      return "Paused by a restart; open it to resume";
    case "CONFLICT":
      return "A helper agent was refused: two of them would write the same files";
    case "PROTECTED_EFFECT":
      return "A helper agent reached an action that needs you";
    case "BLOCKER":
    case "FAILURE":
      return `Stopped: ${i.reason}`;
    default:
      return "Needs you";
  }
}

