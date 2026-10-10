/**
 * The typed command registry (AFW-A15, AFW-A16): one place that says what a
 * command is called, what it does and which chords run it. The palette, the
 * shortcut help and (later) the native menu all read this registry, so they
 * can never disagree. Registering the same id or the same chord twice in one
 * scope throws: the conflict test of QUAL-PX-045 is the constructor.
 */
import { canonicalChord, chordMatches, parseChord, type KeyEventLike } from "./keys.ts";

export interface CommandDef<C = void> {
  id: string;
  title: string;
  /** Palette and help grouping. */
  group: string;
  /** Chords that run it, e.g. `mod+n`. */
  keys?: readonly string[];
  /** Extra words the palette matches. */
  keywords?: readonly string[];
  /** Bindings conflict only within a scope; the default is "global". */
  scope?: string;
  /** Hide from the palette (a key-only command). */
  palette?: boolean;
  /** A reason string disables the command and is shown; true/undefined enables it. */
  enabled?: (ctx: C) => true | string;
  run: (ctx: C) => void | Promise<void>;
}

export class CommandRegistry<C = void> {
  private byId = new Map<string, CommandDef<C>>();
  private byChord = new Map<string, string>();

  register(def: CommandDef<C>): () => void {
    if (this.byId.has(def.id)) throw new Error(`command "${def.id}" is already registered`);
    const scope = def.scope ?? "global";
    const claimed: string[] = [];
    for (const k of def.keys ?? []) {
      parseChord(k);
      const slot = `${scope}:${canonicalChord(k)}`;
      const owner = this.byChord.get(slot);
      if (owner) throw new Error(`chord "${k}" is already bound to "${owner}" in scope "${scope}" (cannot bind it to "${def.id}")`);
      if (claimed.includes(slot)) throw new Error(`command "${def.id}" binds chord "${k}" twice`);
      claimed.push(slot);
    }
    this.byId.set(def.id, def);
    for (const slot of claimed) this.byChord.set(slot, def.id);
    return () => {
      this.byId.delete(def.id);
      for (const slot of claimed) this.byChord.delete(slot);
    };
  }

  get(id: string): CommandDef<C> | undefined {
    return this.byId.get(id);
  }

  all(): CommandDef<C>[] {
    return [...this.byId.values()];
  }

  /** The command a key event runs in a scope, if any. */
  forEvent(e: KeyEventLike, isMac: boolean, scope = "global"): CommandDef<C> | undefined {
    return this.all().find((d) => (d.scope ?? "global") === scope && (d.keys ?? []).some((k) => chordMatches(e, k, isMac)));
  }

  /** Whether a command may run now: true, or the reason it cannot. */
  availability(id: string, ctx: C): true | string {
    const d = this.byId.get(id);
    if (!d) return `unknown command ${id}`;
    return d.enabled ? d.enabled(ctx) : true;
  }

  async run(id: string, ctx: C): Promise<boolean> {
    const d = this.byId.get(id);
    if (!d || this.availability(id, ctx) !== true) return false;
    await d.run(ctx);
    return true;
  }
}
