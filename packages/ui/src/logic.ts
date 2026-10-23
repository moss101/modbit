/**
 * The React-free half of @modbit/ui (`@modbit/ui/logic`): layers, chords and key
 * scopes, the command registry, palette ranking, the tray store and keyboard
 * navigation. Pure TypeScript, so `node --test` exercises it with no DOM.
 */
export { TrayStore, trays, type TrayAction, type TrayDef, type TraySnapshot, type TrayTone } from "./trays.ts";
export { LayerStack, layers, type Layer } from "./layers.ts";
export { KeyScopes, bindingsScope, canonicalChord, chordMatches, chordParts, displayChord, isMacPlatform, keyScopes, parseChord, type Chord, type KeyEventLike, type KeyScope } from "./keys.ts";
export { CommandRegistry, type CommandDef } from "./commands.ts";
export { rank, scoreOf, type Rankable } from "./rank.ts";
export { navTarget, nextEnabled, typeAheadTarget, type Orientation } from "./nav.ts";
