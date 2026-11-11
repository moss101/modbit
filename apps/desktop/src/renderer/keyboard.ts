/**
 * Keyboard model (PX-024; docs/39 "Keyboard model"): global shortcuts, list
 * navigation and review navigation, with confirmation before an
 * irreversible effect. Pure: a key event and where focus is → the command,
 * so the mapping is testable without a DOM and the same on every platform
 * (the primary modifier is ⌘ on macOS and Ctrl elsewhere).
 */
export type Command =
  // Global.
  | "newTask"
  | "search"
  | "jumpAttention"
  | "jumpRunning"
  | "approve"
  | "deny"
  | "cancelTask"
  | "steerTask"
  | "help"
  // Lists.
  | "next"
  | "previous"
  | "open"
  | "back"
  // Review.
  | "hunkNext"
  | "hunkPrevious"
  | "hunkToggle"
  | "toggleSplit"
  | "jumpFailing"
  // Confirmation.
  | "confirm";

export interface KeyLike {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

export interface KeyContext {
  /** Focus is in a text field: single letters type, they never command. */
  editable: boolean;
  screen: "fleet" | "review";
  /** A confirmation is pending (approve/deny/cancel of an irreversible effect). */
  confirming: boolean;
  isMac: boolean;
}

/** The command a key means here, or null when the key is not ours. */
export function commandFor(e: KeyLike, ctx: KeyContext): Command | null {
  const mod = ctx.isMac ? e.metaKey : e.ctrlKey;
  if (ctx.confirming) {
    if (e.key === "Enter") return "confirm";
    if (e.key === "Escape") return "back";
    return null;
  }
  // Modifier shortcuts work everywhere, typing included.
  if (mod && !e.altKey) {
    switch (e.key.toLowerCase()) {
      case "n":
        return "newTask";
      case "/":
        return "help";
      default:
        return null;
    }
  }
  if (e.key === "Escape") return "back";
  if (ctx.editable) return null;
  if (e.altKey || e.ctrlKey || e.metaKey) return null;
  switch (e.key) {
    case "/":
      return "search";
    case "?":
      return "help";
    case "a":
      return "jumpAttention";
    case "r":
      return "jumpRunning";
    case "y":
      return "approve";
    case "x":
      return "deny";
    case "c":
      return "cancelTask";
    case "s":
      return "steerTask";
    case "ArrowDown":
      return ctx.screen === "review" ? "hunkNext" : "next";
    case "ArrowUp":
      return ctx.screen === "review" ? "hunkPrevious" : "previous";
    case "j":
      return ctx.screen === "review" ? "hunkNext" : "next";
    case "k":
      return ctx.screen === "review" ? "hunkPrevious" : "previous";
    case "Enter":
      return ctx.screen === "review" ? "hunkToggle" : "open";
    case "e":
      return ctx.screen === "review" ? "hunkToggle" : null;
    case "u":
      return ctx.screen === "review" ? "toggleSplit" : null;
    case "f":
      return ctx.screen === "review" ? "jumpFailing" : null;
    default:
      return null;
  }
}

/** Whether an element takes typed text (single-letter shortcuts stay off). */
export function isEditable(el: Element | null): boolean {
  if (!el) return false;
  const tag = el.tagName;
  if (tag === "TEXTAREA" || tag === "SELECT") return true;
  if (tag === "INPUT") {
    const type = (el as HTMLInputElement).type;
    return !["checkbox", "radio", "button", "submit", "range"].includes(type);
  }
  return (el as HTMLElement).isContentEditable === true;
}

/** A control that acts on Enter or Space itself (a button, a link, a checkbox): those keys stay its own. */
export function isActivatable(el: Element | null): boolean {
  if (!el) return false;
  const tag = el.tagName;
  if (tag === "BUTTON" || tag === "A" || tag === "SUMMARY" || tag === "SELECT") return true;
  if (tag === "INPUT") return ["checkbox", "radio", "button", "submit"].includes((el as HTMLInputElement).type);
  return false;
}

/** The reference card of the shortcuts, for the help panel and the docs. */
export const SHORTCUTS: { keys: string; command: Command; what: string }[] = [
  { keys: "⌘/Ctrl+N", command: "newTask", what: "New task (focus the goal)" },
  { keys: "/", command: "search", what: "Search: opens the command palette (so does ⌘/Ctrl+K), which finds tasks too" },
  { keys: "a", command: "jumpAttention", what: "Open Inbox on the first item" },
  { keys: "r", command: "jumpRunning", what: "Go to a running task in the list" },
  { keys: "y", command: "approve", what: "Approve the open task's pending effect (Enter confirms an irreversible one)" },
  { keys: "x", command: "deny", what: "Deny the open task's pending effect" },
  { keys: "c", command: "cancelTask", what: "Cancel the open task (Enter confirms)" },
  { keys: "s", command: "steerTask", what: "Steer: go to the open task's composer" },
  { keys: "↑ ↓ / j k", command: "next", what: "Previous / next task in the list; in Review: previous / next change" },
  { keys: "Enter", command: "open", what: "Open the Review of a finished task; in Review: expand or collapse the change" },
  { keys: "Escape", command: "back", what: "Back; cancel a confirmation" },
  { keys: "u", command: "toggleSplit", what: "Review: unified / split" },
  { keys: "f", command: "jumpFailing", what: "Review: jump to the failing check" },
  { keys: "?", command: "help", what: "This list" },
];

/** The keys a focused task in the list takes itself (the list handles them, not `commandFor`): shown in the help with the rest. */
export const LIST_KEYS: { keys: string; what: string }[] = [
  { keys: "Space", what: "On a task in the list: preview it (Enter opens it)" },
  { keys: "p", what: "On a task in the list: pin or unpin it" },
  { keys: "e", what: "On a task in the list: archive it (Undo restores it)" },
];
