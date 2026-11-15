/**
 * Which task the keyboard decides for (REQ-PX-204, DR-PX-2026-11-11-021 MWS-11). With two tasks side by side each
 * conversation is drawn, and only one has the keyboard: its section carries `data-key-focus="true"`, the other "false"; with
 * one task its conversation carries "true". Every keyboard decision (the approve and deny keys, Enter, Shift+Enter and Escape
 * on a docked approval card, steer) is routed here to the task that owns the focused region, never to whichever conversation
 * happens to be first in the page or last to dock a card. Nothing is hidden to make that true: the other task's card stays on
 * screen and answers the pointer.
 */
export const KEY_FOCUS_ATTR = "data-key-focus";

const CONVERSATION = '[data-testid="conversation"]';

/** The conversation that has the keyboard: the one marked so, else the only one (a page shows none). */
export function keyConversation(doc: Document = document): HTMLElement | null {
  return doc.querySelector<HTMLElement>(`${CONVERSATION}[${KEY_FOCUS_ATTR}="true"]`) ?? (doc.querySelectorAll(CONVERSATION).length === 1 ? doc.querySelector<HTMLElement>(CONVERSATION) : null);
}

/**
 * The task a keyboard command acts on: the task of the element that has focus (a row in the task list, a conversation, an
 * attention item), else the conversation that has the keyboard. Null where neither says.
 */
export function keyTaskId(doc: Document = document): string | null {
  const own = doc.activeElement && doc.activeElement !== doc.body ? doc.activeElement.closest("[data-task-id]")?.getAttribute("data-task-id") : null;
  return own ?? keyConversation(doc)?.getAttribute("data-task-id") ?? null;
}

/**
 * Whether a control inside `el` may take a keyboard decision: true outside any conversation (a gallery, a page), and inside one
 * only while that conversation has the keyboard.
 */
export function ownsKeys(el: Element | null): boolean {
  const owner = el?.closest(`[${KEY_FOCUS_ATTR}]`);
  return !owner || owner.getAttribute(KEY_FOCUS_ATTR) === "true";
}
