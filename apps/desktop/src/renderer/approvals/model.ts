/**
 * The approval stack's view model (REQ-PX-058; docs/65 AFW-F01..F13): pure
 * functions over the Core's views. The words are Modbit's own. Nothing here
 * decides whether an effect may run: the Core's kernel asked, and the Core
 * checks the intent hash of every decision. This file only chooses what to say
 * about the question, how the deck is ordered and which key means what.
 */
import type { ApprovalReasonView, DockApprovalView } from "../../shared/control-types.ts";

/** What each always-ask class means, in the words the cards and the run-mode dialog share (AFW-F08). */
export const ASK_CLASS_WORDS: Record<string, { label: string; detail: string }> = {
  OUTSIDE_WORKSPACE_WRITE: { label: "Writes outside the workspace", detail: "creating or changing a file that is not inside the task's workspace" },
  NETWORK: { label: "Network", detail: "fetching from or connecting to another machine" },
  SECRET: { label: "Secrets", detail: "any use of a stored secret or credential" },
  PROTECTED_PATH: { label: "Protected paths", detail: "a protected path or a configuration file" },
  DELETION: { label: "Deletions", detail: "removing files or data" },
  PUSH: { label: "Pushes", detail: "sending commits to a remote or deploying" },
  ESCALATION: { label: "Escalation", detail: "asking for more access than the task was given" },
};

export const askClassLabel = (c: string): string => ASK_CLASS_WORDS[c]?.label ?? c.replace(/_/g, " ").toLowerCase();

const TOOL_TITLES: Record<string, string> = {
  "shell.exec": "Run a command",
  "shell.start": "Start a terminal command",
  "test.run": "Run the tests",
  "git.worktree.close": "Close a worktree",
  "git.worktree.create": "Create a worktree",
  "git.commit": "Commit changes",
  "git.push": "Push commits",
  "change.apply": "Change a file",
};

/** A readable title for the tool (the tool's own name stays beside it). */
export function humanTitle(toolName: string): string {
  const known = TOOL_TITLES[toolName];
  if (known) return known;
  const words = toolName.replace(/[._]+/g, " ").trim();
  return words ? words.charAt(0).toUpperCase() + words.slice(1) : "A protected action";
}

/** `ProtectedWrite` -> `protected write`. */
export const effectWords = (effectClass: string): string => effectClass.replace(/([a-z])([A-Z])/g, "$1 $2").replace(/_/g, " ").toLowerCase();

/** Effects the person cannot take back ask twice before they run (docs/39). */
export const isIrreversible = (effectClass: string): boolean => /destructive|external/i.test(effectClass);

export interface Why {
  /** One line: the cause. */
  headline: string;
  /** What it means for the choice. */
  detail: string;
}

/** Why the Core asked, from its typed reason (AFW-F12); an absent reason is said to be absent, never guessed. */
export function whyAsked(r: ApprovalReasonView): Why {
  const code = r.code;
  if (code === "MODE_ASK") return { headline: "Every protected effect asks in the Ask mode.", detail: "Change the run mode to let rules you make run without asking." };
  if (code === "NOT_IN_ALLOWLIST") return { headline: "No allowlist rule covers this command.", detail: r.contained ? "It would run inside a sandbox, but the mode in force does not approve that on its own." : "Run it once, or allow its prefix so the same kind of command runs without asking." };
  if (code.startsWith("ALWAYS_ASK:")) {
    const cls = code.slice("ALWAYS_ASK:".length);
    const w = ASK_CLASS_WORDS[cls];
    return { headline: `${w ? w.label : cls.replace(/_/g, " ").toLowerCase()} always ask.`, detail: w ? `This is ${w.detail}. It asks in every run mode unless you made a narrow rule for it.` : "This kind of effect asks in every run mode." };
  }
  return { headline: "The Core did not give a reason for asking.", detail: "The effect is protected, so it needs your decision." };
}

/** The argv as one line a person can read: a token with a space or a quote is wrapped so the boundaries show. */
export function commandLine(argv: readonly string[]): string {
  return argv.map((t) => (t === "" || /[\s"'\\]/.test(t) ? JSON.stringify(t) : t)).join(" ");
}

const MAX_LADDER = 8;

/** The prefixes a rule could take, shortest first: `["git"]`, `["git","status"]`, ... (the Core refuses the ones that are too wide). */
export function prefixLadder(argv: readonly string[]): string[][] {
  const out: string[][] = [];
  for (let n = 1; n <= Math.min(argv.length, MAX_LADDER); n++) out.push(argv.slice(0, n));
  return out;
}

/** The default prefix is the exact command (up to the ladder's reach): choosing something wider is the person's act. */
export function defaultPrefix(argv: readonly string[]): string[] {
  return argv.slice(0, Math.min(argv.length, MAX_LADDER));
}

/** Whether "always allow this prefix" is offered for the card, and if not, why (the Core still validates any rule that is made). */
export function alwaysAvailable(a: DockApprovalView): { ok: true } | { ok: false; why: string } {
  if (!a.intent.argv || a.intent.argv.length === 0) return { ok: false, why: "Only a command with an argument list can be allowed by prefix." };
  if (a.reason.askClasses.length > 0 || a.reason.code.startsWith("ALWAYS_ASK:")) return { ok: false, why: "This kind of effect always asks. A rule that covers it is made in the run-mode settings, on purpose." };
  if (isIrreversible(a.effectClass)) return { ok: false, why: "An effect that cannot be undone is never allowed by a rule from this card." };
  return { ok: true };
}

/** The deck's order: the open task's approvals first, then the other agents', each group oldest request first (ties are stable). */
export function deckOrder(list: readonly DockApprovalView[], openTaskId: string | null = null): DockApprovalView[] {
  const own = (a: DockApprovalView): number => (openTaskId !== null && a.taskId === openTaskId ? 0 : 1);
  return [...list].sort((a, b) => own(a) - own(b) || a.requestedAtMs - b.requestedAtMs || (a.approvalId < b.approvalId ? -1 : a.approvalId > b.approvalId ? 1 : 0));
}

/** The index to surface after the list changed: the card the person was on, if it is still pending; otherwise the oldest. */
export function surfacedIndex(list: readonly DockApprovalView[], previousId: string | null): number {
  if (list.length === 0) return 0;
  if (previousId) {
    const i = list.findIndex((a) => a.approvalId === previousId);
    if (i >= 0) return i;
  }
  return 0;
}

export function stepIndex(index: number, count: number, dir: 1 | -1): number {
  if (count <= 1) return 0;
  return (index + dir + count) % count;
}

export type CardKey = "run" | "always" | "skip" | "confirm" | "back";
export interface KeyFacts {
  key: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  /** Where focus is: an editable field, a button/select/link, or neither. */
  focus: "editable" | "control" | "neutral";
  /** Focus is inside the approval card. */
  inCard: boolean;
  /** The second step of an irreversible effect is showing. */
  confirming: boolean;
  alwaysOffered: boolean;
  /** The surfaced card belongs to another agent than the open conversation: it is decided with the pointer, never by a bare key. */
  foreign?: boolean;
}

/**
 * Which decision a key means (AFW-F04, stricter): Enter runs, Shift+Enter allows the prefix, Escape skips. A
 * person typing in a text field outside the card never decides anything; a focused button keeps its own Enter;
 * an irreversible effect asks a second time, and while it does Enter confirms and Escape goes back.
 */
export function cardKey(k: KeyFacts): CardKey | null {
  if (k.ctrlKey || k.metaKey || k.altKey) return null;
  if (k.foreign) return null;
  if (k.focus === "editable" && !k.inCard) return null;
  if (k.key === "Escape") return k.confirming ? "back" : "skip";
  if (k.key !== "Enter") return null;
  // A focused control activates itself on Enter (Skip is a button too).
  if (k.focus === "control" || k.focus === "editable") return null;
  if (k.confirming) return k.shiftKey ? null : "confirm";
  if (k.shiftKey) return k.alwaysOffered ? "always" : null;
  return "run";
}

/** A human line for a refused decision, in the Core's own typed words. */
export function refusalWords(message: string): string {
  const m = message.replace(/^Error invoking remote method '[^']*': (Error: )?/, "");
  if (/INTENT_MISMATCH/.test(m)) return "The Core refused: the effect is no longer the one this card showed. Nothing was decided; the card now shows what is asked.";
  if (/STALE_APPROVAL/.test(m)) return "That effect is no longer waiting for a decision.";
  return `The Core refused: ${m.replace(/^BAD_ARGUMENT: /, "")}`;
}
