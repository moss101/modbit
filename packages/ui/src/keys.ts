/**
 * Chords and scoped keybindings (AFW-A16, AFW-H01). A chord is written
 * `mod+shift+k`: `mod` is the primary modifier (Cmd on macOS, Ctrl elsewhere),
 * `ctrl` is the Control key on every platform. A scope owns bindings while it
 * is on the stack; the innermost scope with a binding for the key wins, and a
 * blocking scope (a modal) stops the search so nothing below it fires.
 */
export interface KeyEventLike {
  key: string;
  code?: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

export interface Chord {
  mod: boolean;
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  key: string;
}

/** Named keys and the event keys/codes each stands for. */
const ALIASES: Record<string, { keys: string[]; code?: string; ignoreShift?: boolean }> = {
  plus: { keys: ["+", "="], ignoreShift: true },
  minus: { keys: ["-", "_"], ignoreShift: true },
  comma: { keys: [","], code: "Comma" },
  slash: { keys: ["/", "?"], code: "Slash" },
  space: { keys: [" "] },
  esc: { keys: ["escape"] },
};

export function parseChord(text: string): Chord {
  const parts = text.toLowerCase().split("+");
  // "mod++" and a trailing "+" mean the plus key itself.
  const last = text.endsWith("++") ? "plus" : parts[parts.length - 1]!;
  const mods = new Set(text.endsWith("++") ? parts.slice(0, -2) : parts.slice(0, -1));
  for (const m of mods) if (!["mod", "ctrl", "alt", "shift"].includes(m)) throw new Error(`unknown modifier "${m}" in chord "${text}"`);
  if (!last) throw new Error(`empty chord "${text}"`);
  return { mod: mods.has("mod"), ctrl: mods.has("ctrl"), alt: mods.has("alt"), shift: mods.has("shift"), key: last === "esc" ? "escape" : last };
}

/** The canonical spelling, so two spellings of one chord collide. */
export function canonicalChord(text: string): string {
  const c = parseChord(text);
  return [c.mod ? "mod" : "", c.ctrl ? "ctrl" : "", c.alt ? "alt" : "", c.shift ? "shift" : "", c.key].filter(Boolean).join("+");
}

export function chordMatches(e: KeyEventLike, chord: string | Chord, isMac: boolean): boolean {
  const c = typeof chord === "string" ? parseChord(chord) : chord;
  const wantMeta = c.mod && isMac;
  const wantCtrl = c.ctrl || (c.mod && !isMac);
  if (e.metaKey !== wantMeta || e.ctrlKey !== wantCtrl || e.altKey !== c.alt) return false;
  const alias = ALIASES[c.key];
  if (!(alias?.ignoreShift) && e.shiftKey !== c.shift) return false;
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key.toLowerCase();
  if (alias) return alias.keys.includes(key) || (alias.code !== undefined && e.code === alias.code);
  return key === c.key;
}

const GLYPH: Record<string, string> = { escape: "Esc", enter: "Enter", arrowup: "↑", arrowdown: "↓", arrowleft: "←", arrowright: "→", backspace: "⌫", tab: "Tab", plus: "+", minus: "−", comma: ",", slash: "/", space: "Space" };

/** Display parts for a chord (macOS symbols or words), for the Kbd primitive and the help list. */
export function chordParts(text: string, isMac: boolean): string[] {
  const c = parseChord(text);
  const parts: string[] = [];
  if (c.ctrl) parts.push(isMac ? "⌃" : "Ctrl");
  if (c.alt) parts.push(isMac ? "⌥" : "Alt");
  if (c.shift) parts.push(isMac ? "⇧" : "Shift");
  if (c.mod) parts.push(isMac ? "⌘" : "Ctrl");
  parts.push(GLYPH[c.key] ?? (c.key.length === 1 ? c.key.toUpperCase() : c.key));
  return parts;
}

export function displayChord(text: string, isMac: boolean): string {
  return chordParts(text, isMac).join(isMac ? "" : "+");
}

export function isMacPlatform(): boolean {
  return typeof navigator !== "undefined" && /mac/i.test(navigator.platform);
}

export interface KeyScope {
  id: string;
  /** Nothing below a blocking scope is consulted for a key it does not bind. */
  blocking: boolean;
  /** The handler this scope binds to the key, if any. */
  resolve: (e: KeyEventLike, isMac: boolean) => (() => void) | null;
}

export function bindingsScope(id: string, bindings: Record<string, () => void>, blocking = false): KeyScope {
  const parsed = Object.entries(bindings).map(([chord, run]) => [parseChord(chord), run] as const);
  return { id, blocking, resolve: (e, isMac) => parsed.find(([c]) => chordMatches(e, c, isMac))?.[1] ?? null };
}

export class KeyScopes {
  private stack: KeyScope[] = [];

  push(scope: KeyScope): () => void {
    this.stack.push(scope);
    return () => {
      const i = this.stack.indexOf(scope);
      if (i >= 0) this.stack.splice(i, 1);
    };
  }

  ids(): string[] {
    return this.stack.map((s) => s.id);
  }

  /** Runs the innermost matching binding; true when one ran. */
  dispatch(e: KeyEventLike, isMac: boolean): boolean {
    for (let i = this.stack.length - 1; i >= 0; i--) {
      const scope = this.stack[i]!;
      const run = scope.resolve(e, isMac);
      if (run) {
        run();
        return true;
      }
      if (scope.blocking) return false;
    }
    return false;
  }
}

export const keyScopes = new KeyScopes();
