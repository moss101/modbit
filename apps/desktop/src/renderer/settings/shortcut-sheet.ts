/**
 * Settings' Shortcuts section (REQ-PX-208, DR-PX-2026-11-11-021 MWS-14 and section 12): every binding the app has, generated
 * from the three key maps the app dispatches through, never written by hand:
 *  - the shell's command registry (`shell/commands.ts`), whose chords the shell's key scope runs anywhere;
 *  - `commandFor` (`keyboard.ts`), the single keys and the confirmation keys the app's key listener and the Review answer;
 *  - `rowKeyAction` (`keyboard.ts`), the keys a focused task row in the list takes.
 * Each row carries the key events that run it, and each event is checked against the same map that dispatches it: a registry
 * chord whose event the registry does not resolve to that command, or a key the map does not answer, is not listed. The words
 * are the registry's titles and `COMMAND_WORDS` / `ROW_KEY_WORDS`. Pure: no DOM.
 */
import type { CommandDef } from "@modbit/ui/logic";
import { parseChord } from "@modbit/ui/logic";
import { COMMAND_WORDS, ROW_KEY_WORDS, commandFor, rowKeyAction, type Command, type KeyContext, type KeyLike, type RowKeyAction } from "../keyboard.ts";

/** Where a binding acts. */
export type ShortcutScope = "anywhere" | "keys" | "row" | "review" | "confirming";

export const SCOPE_WORDS: Record<ShortcutScope, { title: string; note: string }> = {
  anywhere: { title: "Anywhere", note: "These work while you type, too." },
  keys: { title: "Single keys", note: "These work when the cursor is not in a text field." },
  row: { title: "On a task in the list", note: "With a task in the list focused." },
  review: { title: "In a task's review", note: "While a task's review is open." },
  confirming: { title: "While a confirmation is open", note: "When an action asks you to confirm it." },
};

/** One key event that runs the binding, as the probe in the end-to-end check dispatches it. */
export interface KeyProbe extends KeyLike {
  /** The display chord for `Kbd` (`mod+n`, `shift+?`, `arrowdown`). */
  chord: string;
}

export interface ShortcutRow {
  /** The registry's id, `key-<scope>:<command>` for `commandFor`, or `row:<action>`. */
  id: string;
  scope: ShortcutScope;
  what: string;
  probes: KeyProbe[];
}

export interface ShortcutSheet {
  rows: ShortcutRow[];
  /** Bindings a map declares that its own dispatch does not answer: none in a healthy build (asserted by the unit test). */
  unbound: string[];
}

const ALIAS_KEY: Record<string, string> = { plus: "+", minus: "-", comma: ",", slash: "/", space: " ", escape: "Escape", enter: "Enter", tab: "Tab", arrowup: "ArrowUp", arrowdown: "ArrowDown", arrowleft: "ArrowLeft", arrowright: "ArrowRight", backspace: "Backspace" };

/** The key event a registry chord stands for on this platform. */
export function chordEvent(chord: string, isMac: boolean): KeyProbe {
  const c = parseChord(chord);
  return { key: ALIAS_KEY[c.key] ?? c.key, ctrlKey: c.ctrl || (c.mod && !isMac), metaKey: c.mod && isMac, altKey: c.alt, shiftKey: c.shift, chord };
}

/** The chord text a key event shows as. */
export function eventChord(e: KeyLike, isMac: boolean): string {
  const name = e.key === " " ? "space" : e.key === "+" ? "plus" : e.key === "," ? "comma" : e.key.length === 1 ? e.key.toLowerCase() : e.key.toLowerCase();
  const mod = isMac ? e.metaKey : e.ctrlKey;
  const ctrl = isMac && e.ctrlKey;
  const mods = [ctrl ? "ctrl" : "", e.altKey ? "alt" : "", e.shiftKey && e.key !== "?" ? "shift" : "", mod ? "mod" : ""].filter(Boolean);
  return [...mods, name].join("+");
}

/** The keys tried against `commandFor` and the row map: every key a person can press alone or with the primary modifier. */
const KEYS: readonly string[] = [..."abcdefghijklmnopqrstuvwxyz0123456789", "/", "?", ",", ".", "-", "=", "`", "\\", "[", "]", ";", "'", " ", "Enter", "Escape", "Tab", "Backspace", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End", "PageUp", "PageDown"];

function events(isMac: boolean): KeyLike[] {
  const out: KeyLike[] = [];
  for (const key of KEYS) {
    const shiftKey = key === "?";
    out.push({ key, ctrlKey: false, metaKey: false, altKey: false, shiftKey });
    out.push({ key, ctrlKey: !isMac, metaKey: isMac, altKey: false, shiftKey });
  }
  return out;
}

/** Builds the sheet from the registry and the two key maps, for one platform. */
export function shortcutSheet(defs: readonly CommandDef<never>[], resolve: (e: KeyLike, isMac: boolean) => string | undefined, isMac: boolean): ShortcutSheet {
  const rows: ShortcutRow[] = [];
  const unbound: string[] = [];
  // 1. The registry: its chords, each resolved back to its own command by the registry's own matcher.
  for (const d of defs) {
    if ((d.scope ?? "global") !== "global") continue;
    const probes: KeyProbe[] = [];
    for (const k of d.keys ?? []) {
      const e = chordEvent(k, isMac);
      if (resolve(e, isMac) === d.id) probes.push(e);
      else unbound.push(`${d.id}: ${k}`);
    }
    if (probes.length > 0) rows.push({ id: d.id, scope: "anywhere", what: d.title, probes });
  }
  // 2. `commandFor`: what it answers for each key in each context. A key the registry takes first (its scope runs before the
  // app's listener) is the registry's, and is listed there only.
  const contexts: [ShortcutScope, KeyContext][] = [
    ["keys", { editable: false, screen: "fleet", confirming: false, isMac }],
    ["review", { editable: false, screen: "review", confirming: false, isMac }],
    ["confirming", { editable: false, screen: "fleet", confirming: true, isMac }],
  ];
  const byCommand = new Map<string, ShortcutRow>();
  const typing: KeyContext = { editable: true, screen: "fleet", confirming: false, isMac };
  for (const [scope, ctx] of contexts) {
    for (const e of events(isMac)) {
      if (scope !== "confirming" && resolve(e, isMac) !== undefined) continue;
      const cmd = commandFor(e, ctx);
      if (!cmd) continue;
      // In a review, a key that means the same as in the list is the list's row; only what differs is the review's.
      if (scope === "review" && commandFor(e, contexts[0]![1]) === cmd) continue;
      // A key that also works while typing (a modifier chord, Escape) acts anywhere.
      const where: ShortcutScope = scope === "keys" && commandFor(e, typing) === cmd ? "anywhere" : scope;
      const id = `key-${where}:${cmd}`;
      const row = byCommand.get(id) ?? { id, scope: where, what: COMMAND_WORDS[cmd as Command], probes: [] };
      if (!byCommand.has(id)) {
        byCommand.set(id, row);
        rows.push(row);
      }
      // A key that does the same with the modifier held (a confirmation reads the key alone) is listed once, plain.
      if ((e.ctrlKey || e.metaKey) && row.probes.some((p) => p.key === e.key && !p.ctrlKey && !p.metaKey)) continue;
      row.probes.push({ ...e, chord: eventChord(e, isMac) });
    }
  }
  // 3. The task row's keys.
  const byAction = new Map<RowKeyAction, ShortcutRow>();
  for (const e of events(isMac)) {
    const a = rowKeyAction(e);
    if (!a) continue;
    // The modified arrows move as the plain ones do; the plain key is the one listed.
    if ((e.ctrlKey || e.metaKey) && rowKeyAction({ ...e, ctrlKey: false, metaKey: false }) === a) continue;
    const row = byAction.get(a) ?? { id: `row:${a}`, scope: "row" as const, what: ROW_KEY_WORDS[a], probes: [] };
    if (!byAction.has(a)) {
      byAction.set(a, row);
      rows.push(row);
    }
    row.probes.push({ ...e, chord: eventChord(e, isMac) });
  }
  return { rows, unbound };
}

/** The scopes in the order the section shows them. */
export const SCOPES: readonly ShortcutScope[] = ["anywhere", "keys", "row", "review", "confirming"];
