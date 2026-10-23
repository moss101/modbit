/**
 * The conversation's per-viewer preferences (AFW-C04, AFW-C05): the density
 * (Compact is the default) and whether code blocks open. Renderer-local like
 * the shell's other preferences; never task state. A missing or hostile stored
 * value falls back to the default.
 */
import { DENSITIES, type Density } from "../../shared/conversation-types.ts";

export interface ConversationPrefs {
  density: Density;
  codeOpen: boolean;
}

export const CONVERSATION_PREFS_KEY = "modbit.conversation.v1";
export const DEFAULT_CONVERSATION_PREFS: ConversationPrefs = { density: "COMPACT", codeOpen: false };

export function parseConversationPrefs(raw: string | null): ConversationPrefs {
  if (!raw) return DEFAULT_CONVERSATION_PREFS;
  try {
    const v: unknown = JSON.parse(raw);
    if (typeof v !== "object" || v === null || Array.isArray(v)) return DEFAULT_CONVERSATION_PREFS;
    const o = v as Record<string, unknown>;
    return { density: DENSITIES.find((d) => d === o["density"]) ?? "COMPACT", codeOpen: o["codeOpen"] === true };
  } catch {
    return DEFAULT_CONVERSATION_PREFS;
  }
}

export function loadConversationPrefs(): ConversationPrefs {
  try {
    return parseConversationPrefs(localStorage.getItem(CONVERSATION_PREFS_KEY));
  } catch {
    return DEFAULT_CONVERSATION_PREFS;
  }
}

export function saveConversationPrefs(p: ConversationPrefs): void {
  try {
    localStorage.setItem(CONVERSATION_PREFS_KEY, JSON.stringify(p));
  } catch {
    // a per-viewer convenience: the conversation works without it
  }
}
