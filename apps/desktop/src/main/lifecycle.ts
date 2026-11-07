/**
 * Window and quit lifecycle (REQ-PX-049, docs/65 AFW-A02 and AFW-J11), pure so
 * the node tests cover the decisions without Electron.
 *
 * What a quit really does is stated here once and the dialog says only that:
 * the desktop supervises its Core with a parent tether, so quitting stops the
 * Core. Everything the Core accepted is already durable, so a task that was
 * running is suspended at its last saved step and recovered the next time the
 * app opens. What happens then is the person's choice, asked at the quit and
 * recorded in the Core (`EndSession`): "Quit and resume on reopen" has the
 * desktop resume those tasks as soon as the next start has recovered them;
 * "Quit and leave suspended" leaves them waiting for the person to resume
 * them. Nothing continues while the app is closed either way. The Core's
 * terminal broker keeps a background terminal for its orphan grace (60 seconds
 * by default) and then stops it.
 *
 * Quitting also ends the session: the Core runs the session-end hooks to
 * completion first (AFW-J11), and the quit waits for them up to
 * SESSION_END_DEADLINE_MS. A hook that fails is shown and journaled; one that
 * is still running at the deadline is reported, never silently abandoned.
 */

/** The default window (AFW-A02) and the smallest one (until a single-pane mode earns less). */
export const WINDOW_DEFAULT = { width: 1280, height: 800 } as const;
export const WINDOW_MIN = { width: 900, height: 600 } as const;

export interface ActiveWork {
  /** Tasks whose loop is running now. */
  runningTasks: number;
  /** Background terminals whose process is running now. */
  runningTerminals: number;
}

export const noWork = (w: ActiveWork): boolean => w.runningTasks === 0 && w.runningTerminals === 0;

const plural = (n: number, one: string, many: string): string => `${n} ${n === 1 ? one : many}`;

/** The longest the quit waits for the session-end hooks (each hook has its own timeout, at most 30 s). */
export const SESSION_END_DEADLINE_MS = 45_000;

export interface QuitPrompt {
  message: string;
  detail: string;
  /** The first button is the default and the Escape answer: keep the app open. */
  buttons: string[];
}

/** What the person's answer to the quit question means. */
export type QuitChoice = "stay" | "quit" | "quit-resume";

/** The answer to `quitPrompt(work)` that button `response` is: only the first button keeps the app open. */
export function quitChoice(work: ActiveWork, response: number): QuitChoice {
  const p = quitPrompt(work);
  if (response <= 0 || response >= p.buttons.length) return "stay";
  return work.runningTasks > 0 && response === 1 ? "quit-resume" : "quit";
}

/** The quit confirmation: what is running and, truthfully, what quitting does to it. */
export function quitPrompt(w: ActiveWork): QuitPrompt {
  const parts: string[] = [];
  if (w.runningTasks > 0) parts.push(plural(w.runningTasks, "task is running", "tasks are running"));
  if (w.runningTerminals > 0) parts.push(plural(w.runningTerminals, "background terminal is running", "background terminals are running"));
  const lines: string[] = ["Quitting stops the local Core that runs your tasks."];
  if (w.runningTasks > 0) lines.push("Everything the Core accepted is already saved. Running tasks are suspended at their last saved step and recovered the next time you open Modbit; nothing continues while Modbit is closed. Choose whether they resume by themselves when you reopen it, or wait for you to resume them.");
  if (w.runningTerminals > 0) lines.push("Background terminals keep running for about a minute and then stop with the Core.");
  const buttons = w.runningTasks > 0 ? ["Keep Modbit open", "Quit and resume on reopen", "Quit and leave suspended"] : ["Keep Modbit open", "Quit and stop"];
  return { message: `${parts.join(" and ")}. Quit anyway?`, detail: lines.join("\n\n"), buttons };
}

/** "3 tasks resumed, 1 needs attention": the recovery counts a restart shows, from the Core's own task states. */
export interface RecoveryCounts {
  recovered: number;
  resumed: number;
  needAttention: number;
}

export function recoverySummary(c: RecoveryCounts): string {
  if (c.recovered === 0) return "No tasks to recover.";
  const parts: string[] = [];
  if (c.resumed > 0) parts.push(`${plural(c.resumed, "task", "tasks")} resumed`);
  if (c.needAttention > 0) parts.push(`${c.needAttention} ${c.needAttention === 1 ? "needs" : "need"} attention`);
  const rest = c.recovered - c.resumed - c.needAttention;
  if (rest > 0) parts.push(`${rest} recovered idle`);
  return `${parts.join(", ")}.`;
}

export interface ShutdownStep {
  name: string;
  run: () => Promise<void> | void;
}

export interface ShutdownReport {
  completed: string[];
  /** Steps that threw, with the reason: shown and journaled, never swallowed. */
  failed: { name: string; reason: string }[];
  /** Steps still running when the deadline passed: the quit does not wait for them. */
  timedOut: string[];
}

/**
 * Runs the shutdown steps in parallel and returns when all have settled or the
 * deadline passes, whichever is first: a slow or hung step (a session-end
 * hook, say) is reported, never waited on past the deadline, and a failing one
 * is recorded rather than lost.
 */
export async function runShutdown(steps: readonly ShutdownStep[], deadlineMs: number): Promise<ShutdownReport> {
  const report: ShutdownReport = { completed: [], failed: [], timedOut: [] };
  const pending = new Set(steps.map((s) => s.name));
  const runs = steps.map(async (s) => {
    try {
      await s.run();
      report.completed.push(s.name);
    } catch (e) {
      report.failed.push({ name: s.name, reason: e instanceof Error ? e.message : String(e) });
    } finally {
      pending.delete(s.name);
    }
  });
  let timer: ReturnType<typeof setTimeout> | undefined;
  const deadline = new Promise<void>((resolve) => {
    timer = setTimeout(resolve, deadlineMs);
  });
  await Promise.race([Promise.all(runs), deadline]);
  clearTimeout(timer);
  report.timedOut = [...pending];
  return report;
}

/** One session-end hook run, as the Core reported it. */
export interface SessionEndRun {
  hook: string;
  outcome: string;
  detail: string;
}

/** Outcomes of a session-end hook that the person must be told about. */
const HOOK_FAILURES = new Set(["FAILED", "TIMEOUT", "MALFORMED", "DENIED", "UNLOADED"]);

/** The visible failure text for the runs that did not end well, or null when every hook ran to completion. */
export function sessionEndFailure(runs: readonly SessionEndRun[]): string | null {
  const bad = runs.filter((r) => HOOK_FAILURES.has(r.outcome));
  if (bad.length === 0) return null;
  const lines = bad.map((r) => `${r.hook}: ${r.outcome.toLowerCase()}${r.detail ? ` (${r.detail})` : ""}`);
  return `${plural(bad.length, "session-end hook did", "session-end hooks did")} not finish cleanly: ${lines.join("; ")}`;
}
