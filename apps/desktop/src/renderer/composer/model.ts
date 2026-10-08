/**
 * The composer's pure logic (REQ-PX-054, -055, -056): modes and their cycle,
 * placeholders, what a key does, the @ and / triggers, drafts and history,
 * the queue tray's rows, the send-behaviour words, the model picker's rows and
 * the policy-blocked tray. Nothing here reads the DOM, the clock (callers pass
 * `now`) or the Core: the Core's answers go in, plain view data comes out, and
 * the same functions are tested on their own and used by the components.
 *
 * The renderer holds no authority (docs/81). A mode chosen here is a request
 * until the Core acknowledges it; a model shown as blocked is blocked because
 * the Core said so; a queue row's position is the Core's.
 */
import type { InputModeId, ModelCatalogView, ModelEntryView, ModelVariantView, PostureView, QueuedInputItem, SendBehaviorState, SlashEntryView, SlashInventoryView, TaskModeId } from "../../shared/composer-types.ts";
import { TASK_MODES } from "../../shared/composer-types.ts";

// ------------------------------------------------------------------ modes

export interface ModeDef {
  id: TaskModeId;
  label: string;
  /** A glyph that is never the only signal: the label always accompanies it. */
  icon: string;
  /** What the Core enforces in this mode, in a sentence (AFW-D05). */
  gist: string;
  placeholder: string;
}

/** Presentation only; the posture each mode means is the Core's (`GetTaskPosture`). */
export const MODE_DEFS: Record<TaskModeId, ModeDef> = {
  AGENT: { id: "AGENT", label: "Agent", icon: "◆", gist: "Works in your workspace within the permissions you gave it.", placeholder: "Ask Modbit to change something, or follow up on this task" },
  PLAN: { id: "PLAN", label: "Plan", icon: "☰", gist: "Reads and writes a plan; changes nothing until you accept the plan.", placeholder: "Describe what you want planned" },
  DEBUG: { id: "DEBUG", label: "Debug", icon: "✱", gist: "Reproduces the problem before it is allowed to change anything.", placeholder: "Describe the bug, what you expected and what happened" },
  MULTITASK: { id: "MULTITASK", label: "Multitask", icon: "⧉", gist: "Lets the agent hand parts of the work to subagents.", placeholder: "Describe work that can be split into parts" },
  ASK: { id: "ASK", label: "Ask", icon: "?", gist: "Read-only: answers from the code without changing or running anything.", placeholder: "Ask a question about this project" },
};

/** Shift+Tab walks Plan, Debug, Multitask, Ask and then returns to Agent (AFW-D03). */
export const MODE_CYCLE: readonly TaskModeId[] = ["AGENT", "PLAN", "DEBUG", "MULTITASK", "ASK"];

export function nextMode(current: TaskModeId, backwards = false): TaskModeId {
  const i = MODE_CYCLE.indexOf(current);
  const n = MODE_CYCLE.length;
  return MODE_CYCLE[(i + (backwards ? n - 1 : 1)) % n]!;
}

export function isModeId(v: unknown): v is TaskModeId {
  return typeof v === "string" && (TASK_MODES as readonly string[]).includes(v);
}

/**
 * Whether the mode shown is the Core's. A mode the person asked for that the
 * Core has not acknowledged is "unconfirmed" and is never drawn as active.
 */
export type ModeStatus = "confirmed" | "unconfirmed" | "pending-boundary";
export function modeStatus(requested: TaskModeId | null, posture: Pick<PostureView, "mode" | "modeInForce"> | null): ModeStatus {
  if (!posture) return "unconfirmed";
  if (requested !== null && requested !== posture.mode) return "unconfirmed";
  return posture.mode === posture.modeInForce ? "confirmed" : "pending-boundary";
}

export interface PlaceholderContext {
  mode: TaskModeId;
  /** The task's state in the Core's words (Running, Queued, ...). */
  state: string;
  running: boolean;
  hasMessages: boolean;
}

/** Context-specific placeholders in Modbit's own words (AFW-D01). */
export function placeholderFor(ctx: PlaceholderContext): string {
  if (ctx.running) return "Add to this task: Enter queues it for after this turn";
  if (ctx.state === "ReadyForReview") return "Waiting for your review. A message here is queued for when the task runs again";
  if (ctx.mode !== "AGENT") return MODE_DEFS[ctx.mode].placeholder;
  return ctx.hasMessages ? "Follow up on this task" : "Describe what you want done";
}

export interface Suggestion {
  mode: TaskModeId;
  label: string;
}
/** Suggestion chips below the empty composer; the active mode's chip is hidden (AFW-D02). */
export function suggestionsFor(active: TaskModeId): Suggestion[] {
  return (["PLAN", "MULTITASK"] as const).filter((m) => m !== active).map((m) => ({ mode: m, label: MODE_DEFS[m].label }));
}

// ----------------------------------------------------------------- keys

export interface KeyLike {
  key: string;
  shiftKey: boolean;
  altKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  isComposing?: boolean;
}

export type EnterAction = "send" | "send-alternate" | "newline" | "none";

/**
 * What Enter does (AFW-D11). Enter sends; Shift+Enter is a new line; the
 * primary modifier with Enter is the alternate send. Enter during an IME
 * composition is the IME's.
 */
export function enterAction(e: KeyLike, isMac: boolean): EnterAction {
  if (e.key !== "Enter" || e.isComposing) return "none";
  const mod = isMac ? e.metaKey : e.ctrlKey;
  if (mod && !e.altKey) return "send-alternate";
  if (e.shiftKey) return "newline";
  if (e.altKey || e.ctrlKey || e.metaKey) return "none";
  return "send";
}

export interface SendContext {
  /** A run is executing the task: a send is input to it. */
  running: boolean;
  /** The task is waiting to be started or resumed by this very send (never ran, or paused). */
  startable: boolean;
  behavior: Pick<SendBehaviorState, "whileRunning"> | null;
}

export interface SendPlan {
  /** The mode QueueInput carries: DEFAULT hands the choice to the Core's send behaviour. */
  mode: InputModeId;
  /** Start (resume) the run after the input is recorded. */
  start: boolean;
  /** The message is held behind a running turn (the education tray's moment). */
  queues: boolean;
}

/**
 * What a send means, from the task's state and the key used. While a turn
 * runs, Enter is the Core's default behaviour and the alternate key is the
 * other of steer and queue (so the alternate never equals the default).
 */
export function planSend(ctx: SendContext, alternate: boolean): SendPlan {
  if (!ctx.running) return { mode: "FOLLOW_UP", start: ctx.startable, queues: false };
  if (!alternate) {
    const w = ctx.behavior?.whileRunning ?? "QUEUE";
    return { mode: "DEFAULT", start: false, queues: w === "QUEUE" || w === "COLLECT" };
  }
  const w = ctx.behavior?.whileRunning ?? "QUEUE";
  return w === "STEER" ? { mode: "FOLLOW_UP", start: false, queues: true } : { mode: "STEER", start: false, queues: false };
}

/** The consequence of the alternate key while a turn runs, for its label (AFW-E05): never "without stopping". */
export function alternateLabel(behavior: Pick<SendBehaviorState, "whileRunning"> | null): string {
  return (behavior?.whileRunning ?? "QUEUE") === "STEER" ? "Queue for after this turn" : "Steer: applied at the next safe point, and may cut the current model call";
}

// ------------------------------------------------------------- triggers

export interface Trigger {
  kind: "mention" | "slash";
  /** What was typed after the trigger character. */
  query: string;
  /** Index of the trigger character in the text. */
  start: number;
  /** Index just after the query (the caret). */
  end: number;
}

/**
 * The menu the caret is in, if any: `@word` after whitespace or at the start
 * opens the mention menu; a `/word` that is the whole first line opens the
 * slash menu (so a path in prose never does).
 */
export function triggerAt(text: string, caret: number): Trigger | null {
  const before = text.slice(0, caret);
  const at = /(^|\s)@([^\s@]*)$/.exec(before);
  if (at) {
    const query = at[2]!;
    return { kind: "mention", query, start: caret - query.length - 1, end: caret };
  }
  const slash = /^\/([^\s/]*)$/.exec(before);
  if (slash && !text.slice(caret).includes("\n") && !text.slice(caret).trim()) return { kind: "slash", query: slash[1]!, start: 0, end: caret };
  return null;
}

/** The text with a trigger replaced by `insert` (and a trailing space for a mention). */
export function applyTrigger(text: string, t: Trigger, insert: string): { text: string; caret: number } {
  const head = text.slice(0, t.start) + insert;
  const next = head + text.slice(t.end);
  return { text: next, caret: head.length };
}

// ---------------------------------------------------------- slash menu

export interface SlashItem {
  entry: SlashEntryView;
  /** Why it cannot be chosen, in the Core's words; null when it can. */
  disabledReason: string | null;
}

export interface SlashMenu {
  items: SlashItem[];
  /** Index of the first item after the divider; 0 = none. */
  dividerAt: number;
}

const SCOPE_LABEL: Record<string, string> = { SYSTEM: "System", PROJECT: "Project", USER: "User", EXTENSION: "Extension" };
export const scopeLabel = (s: string): string => SCOPE_LABEL[s] ?? s.toLowerCase();

/**
 * The slash menu: the Core's own order, built-ins first, then the divider
 * (`SlashInventoryView.dividerAt`), filtered by what is typed. An entry the
 * Core marks disabled stays in the list with its reason. A skill is named when
 * a run starts, so while a turn runs a skill is listed but cannot be chosen.
 */
export function slashMenu(inv: SlashInventoryView, query: string, opts: { running: boolean }): SlashMenu {
  const q = query.trim().toLowerCase();
  const rows = inv.entries.map((entry, index) => ({ entry, index }));
  const kept = rows.filter(({ entry }) => q === "" || entry.displayName.toLowerCase().includes(q) || entry.id.toLowerCase().includes(q));
  const items: SlashItem[] = kept.map(({ entry }) => {
    let reason: string | null = null;
    if (!entry.enabled) reason = entry.trustDetail || `${entry.trust.toLowerCase().replace(/_/g, " ")}: the Core will not let this reach a model`;
    else if (entry.kind === "SKILL" && opts.running) reason = "A skill is named when a run starts. It can be chosen once this turn ends.";
    else if (entry.kind !== "SKILL") reason = "This kind of entry is listed for reference; running it from the composer is not available yet.";
    return { entry, disabledReason: reason };
  });
  const dividerIndex = inv.dividerAt > 0 ? kept.findIndex(({ index }) => index >= inv.dividerAt) : -1;
  return { items, dividerAt: dividerIndex > 0 ? dividerIndex : 0 };
}

// ------------------------------------------------------------- mentions

export type MentionKind = "file" | "folder" | "terminal" | "conversation" | "changes";
export interface Mention {
  kind: MentionKind;
  /** What the message text carries after the @. */
  value: string;
  label: string;
}

export function mentionText(m: Mention): string {
  return `@${m.value}`;
}

/** The mentions whose text is still in the message (a deleted `@path` drops its chip). */
export function liveMentions(text: string, mentions: readonly Mention[]): Mention[] {
  return mentions.filter((m) => {
    const t = mentionText(m);
    const i = text.indexOf(t);
    return i >= 0 && (i + t.length === text.length || /\s/.test(text[i + t.length]!)) && (i === 0 || /\s/.test(text[i - 1]!));
  });
}

/** The files a message names, as the workspace paths the Core's selection takes. */
export function filePaths(mentions: readonly Mention[]): string[] {
  return mentions.filter((m) => m.kind === "file").map((m) => m.value);
}

// ------------------------------------------------------ drafts, history

/** A minimal key-value store (localStorage, or a map in a test). */
export interface KV {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export const COMPOSER_KEY = "modbit.composer.v1";
const MAX_DRAFTS = 40;
const MAX_HISTORY = 50;
const MAX_DRAFT_CHARS = 20_000;

export interface Draft {
  text: string;
  mentions: Mention[];
  /** The mode the person had chosen while writing (a request, not the Core's mode). */
  skill: string | null;
}

export interface ComposerStore {
  drafts: Record<string, Draft>;
  history: Record<string, string[]>;
  /** The send-behaviour education tray was acknowledged (AFW-E04). */
  educationAck: boolean;
}

export const EMPTY_STORE: ComposerStore = { drafts: {}, history: {}, educationAck: false };

const isStringArray = (v: unknown): v is string[] => Array.isArray(v) && v.every((x) => typeof x === "string");

export function parseStore(raw: string | null): ComposerStore {
  if (!raw) return EMPTY_STORE;
  try {
    const v = JSON.parse(raw) as Record<string, unknown>;
    const drafts: Record<string, Draft> = {};
    const d = (v.drafts ?? {}) as Record<string, unknown>;
    for (const [id, x] of Object.entries(d)) {
      const o = x as Record<string, unknown> | null;
      if (!o || typeof o.text !== "string") continue;
      const mentions = Array.isArray(o.mentions) ? o.mentions.filter((m): m is Mention => !!m && typeof (m as Mention).value === "string" && typeof (m as Mention).label === "string" && typeof (m as Mention).kind === "string") : [];
      drafts[id] = { text: o.text.slice(0, MAX_DRAFT_CHARS), mentions, skill: typeof o.skill === "string" ? o.skill : null };
    }
    const history: Record<string, string[]> = {};
    for (const [id, h] of Object.entries((v.history ?? {}) as Record<string, unknown>)) if (isStringArray(h)) history[id] = h.slice(-MAX_HISTORY);
    return { drafts, history, educationAck: v.educationAck === true };
  } catch {
    return EMPTY_STORE;
  }
}

export function loadStore(kv: KV | null): ComposerStore {
  try {
    return parseStore(kv?.getItem(COMPOSER_KEY) ?? null);
  } catch {
    return EMPTY_STORE;
  }
}

export function saveStore(kv: KV | null, store: ComposerStore): void {
  try {
    kv?.setItem(COMPOSER_KEY, JSON.stringify(store));
  } catch {
    // Storage can be unavailable (a private window, a quota); the composer works without it.
  }
}

/** The draft for a task replaced (an empty one removes it); the oldest drafts fall off. */
export function withDraft(store: ComposerStore, taskId: string, draft: Draft | null): ComposerStore {
  const { [taskId]: _gone, ...rest } = store.drafts;
  void _gone;
  const empty = !draft || (draft.text.length === 0 && draft.mentions.length === 0 && !draft.skill);
  const entries = empty ? Object.entries(rest) : [...Object.entries(rest), [taskId, { ...draft!, text: draft!.text.slice(0, MAX_DRAFT_CHARS) }] as const];
  return { ...store, drafts: Object.fromEntries(entries.slice(-MAX_DRAFTS)) };
}

/** A sent prompt added to a task's history: newest last, no immediate repeat, bounded. */
export function withHistory(store: ComposerStore, taskId: string, text: string): ComposerStore {
  const t = text.trim();
  if (!t) return store;
  const list = store.history[taskId] ?? [];
  if (list[list.length - 1] === t) return store;
  return { ...store, history: { ...store.history, [taskId]: [...list, t].slice(-MAX_HISTORY) } };
}

export interface HistoryCursor {
  /** Index into the list; -1 = the draft the person was writing. */
  index: number;
  /** The unsent text to return to past the newest entry. */
  stash: string;
}
export const HISTORY_START: HistoryCursor = { index: -1, stash: "" };

/** Alt+Up goes to the previous prompt, Alt+Down back toward the draft (AFW-D10). */
export function walkHistory(list: readonly string[], cur: HistoryCursor, dir: "older" | "newer", current: string): { cursor: HistoryCursor; text: string } | null {
  if (list.length === 0) return null;
  if (dir === "older") {
    const index = cur.index === -1 ? list.length - 1 : Math.max(0, cur.index - 1);
    if (cur.index === 0) return null;
    return { cursor: { index, stash: cur.index === -1 ? current : cur.stash }, text: list[index]! };
  }
  if (cur.index === -1) return null;
  if (cur.index >= list.length - 1) return { cursor: HISTORY_START, text: cur.stash };
  return { cursor: { index: cur.index + 1, stash: cur.stash }, text: list[cur.index + 1]! };
}

// ------------------------------------------------------- the queue tray

export interface QueueRow {
  inputId: string;
  position: number;
  text: string;
  /** STEER | COLLECT | FOLLOW_UP: how the item will be dispatched. */
  mode: string;
  /** One truncated line for the row. */
  line: string;
  /** What the item will do when it is dispatched, in words that state the consequence. */
  modeLabel: string;
  edited: boolean;
  untrusted: boolean;
  canMoveUp: boolean;
  canMoveDown: boolean;
}

const ROW_CHARS = 120;
export const MODE_WORDS: Record<string, string> = {
  FOLLOW_UP: "Queued: its own turn after this one",
  COLLECT: "Collected: joined with other collected messages into one next turn",
  STEER: "Steer: applied at the next safe point",
};

export function queueHeader(n: number): string {
  return `${n} queued`;
}

export function queueRows(items: readonly QueuedInputItem[]): QueueRow[] {
  const queued = items.filter((i) => i.state === "QUEUED").sort((a, b) => a.position - b.position);
  return queued.map((i, k) => {
    const flat = i.text.replace(/\s+/g, " ").trim();
    return { inputId: i.inputId, position: i.position, text: i.text, mode: i.mode, line: flat.length > ROW_CHARS ? `${flat.slice(0, ROW_CHARS - 1)}…` : flat, modeLabel: MODE_WORDS[i.mode] ?? i.mode.toLowerCase(), edited: i.edited, untrusted: i.untrusted, canMoveUp: k > 0, canMoveDown: k < queued.length - 1 };
  });
}

/** The input a row's up or down moves before, for `ReorderQueuedInput` (empty = the end). */
export function reorderTarget(rows: readonly QueueRow[], inputId: string, dir: "up" | "down"): { inputId: string; before: string } | null {
  const i = rows.findIndex((r) => r.inputId === inputId);
  if (i < 0) return null;
  if (dir === "up") return i === 0 ? null : { inputId, before: rows[i - 1]!.inputId };
  if (i >= rows.length - 1) return null;
  return { inputId, before: i + 2 < rows.length ? rows[i + 2]!.inputId : "" };
}

/** What Send now does, from the Core's own consequence text and the setting (AFW-E06): the label states it. */
export function sendNowLabel(behavior: Pick<SendBehaviorState, "sendNow" | "sendNowConsequence"> | null): { label: string; title: string } {
  const how = behavior?.sendNow ?? "INTERRUPT";
  const label = how === "STEER" ? "Send now (steer)" : "Send now (interrupts)";
  return { label, title: behavior?.sendNowConsequence || (how === "STEER" ? "Applied at the next safe point; it may cut the current model call." : "Cuts the current model call at a safe point, keeps what was said so far as partial, then starts a new turn from this message.") };
}

/** The education tray is due the first time a send would queue behind a running turn (AFW-E04). */
export function educationDue(store: Pick<ComposerStore, "educationAck">, queues: boolean): boolean {
  return queues && !store.educationAck;
}

export const EDUCATION = {
  title: "Messages sent during a turn are queued",
  lines: [
    "Queue (the default): your message waits and becomes its own turn after the current one ends.",
    "Collect: messages are joined into one turn after the current one ends.",
    "Steer: your message is applied at the next safe point and may cut the current model call.",
  ],
} as const;

// ------------------------------------------------------ the stopped state

export interface StoppedState {
  /** The user message the Stop interrupted; editable in place, never re-sent without the person. */
  text: string;
  at: string;
}

// --------------------------------------------------- background terminals

export function elapsedLabel(startedAtMs: number, nowMs: number): string {
  const s = Math.max(0, Math.floor((nowMs - startedAtMs) / 1000));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${String(s % 60).padStart(2, "0")}s`;
  return `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, "0")}m`;
}

export interface TerminalRowIn {
  terminalId: string;
  title: string;
  state: string;
  startedAtMs: number;
  owner: string;
}
/** The terminals the chip counts: those still running (the Core's state words). */
export function runningTerminals<T extends Pick<TerminalRowIn, "state">>(list: readonly T[]): T[] {
  return list.filter((t) => t.state.toLowerCase() === "running");
}

// --------------------------------------------------------- the model chip

export function findModel(catalog: ModelCatalogView | null, endpoint: string, model: string): ModelEntryView | null {
  return catalog?.models.find((m) => m.endpoint === endpoint && m.model === model) ?? null;
}

export interface ModelChip {
  /** The model name, or "Auto" when the router chooses. */
  name: string;
  /** The dimmer variant label; empty when none. */
  variant: string;
  /** How the Core is routing, for a title: no registry means DIRECT, said as it is. */
  routing: string;
}

export function modelChip(posture: PostureView | null, catalog: ModelCatalogView | null): ModelChip {
  if (!posture) return { name: "Model", variant: "", routing: "" };
  const p = posture.preference;
  const r = posture.routing;
  const routing = r.outcome ? `${r.outcome.toLowerCase()}${r.reasonCode ? `: ${r.reasonCode.toLowerCase().replace(/_/g, " ")}` : ""}` : "";
  if (p.pinModel) {
    const m = findModel(catalog, p.pinEndpoint, p.pinModel);
    const eff = p.effort || m?.defaultEffort || "";
    return { name: p.pinModel, variant: eff ? `${eff} effort` : "", routing };
  }
  const objective = p.objective ? p.objective.charAt(0) + p.objective.slice(1).toLowerCase() : "Balance";
  return { name: "Auto", variant: objective, routing };
}

export interface PickerRow {
  id: string;
  endpoint: string;
  model: string;
  /** The model's own name; the variant is part of the row's identity (AFW-D14). */
  name: string;
  variant: ModelVariantView;
  selected: boolean;
  /** The Core's typed reason this row cannot be chosen; null when it can. */
  blocked: { code: string; reason: string } | null;
  raisesCost: boolean;
}

export function pickerRows(catalog: ModelCatalogView | null, posture: PostureView | null, query: string): PickerRow[] {
  if (!catalog) return [];
  const q = query.trim().toLowerCase();
  const p = posture?.preference;
  const rows: PickerRow[] = [];
  for (const m of catalog.models) {
    if (q && !m.model.toLowerCase().includes(q) && !m.provider.toLowerCase().includes(q)) continue;
    const blocked = m.pinAllowed ? null : { code: m.pinRefusalCode || "NOT_AVAILABLE", reason: m.blockedByPolicy || (m.credentialAvailable ? "The Core will not route to this model." : "No credential is configured for this model's provider.") };
    for (const v of m.variants) {
      const pinned = !!p && p.pinEndpoint === m.endpoint && p.pinModel === m.model;
      const effort = p?.effort || m.defaultEffort;
      const selected = pinned && (v.effort === "" ? true : v.effort === effort || (effort === "" && v.isDefault));
      rows.push({ id: `${m.endpoint}/${m.model}#${v.effort || "standard"}`, endpoint: m.endpoint, model: m.model, name: m.model, variant: v, selected, blocked, raisesCost: v.raisesCost });
    }
  }
  return rows;
}

export interface BlockedTray {
  title: string;
  body: string;
  code: string;
}

/** The tray for a model the Core will not route to (AFW-H02): the cause in the Core's words, never the renderer's guess. */
export function blockedTray(model: string, reason: { code: string; reason: string }): BlockedTray {
  return { title: `${model} is not available to you`, body: reason.reason, code: reason.code };
}

/** A typed Core refusal read out of an IPC error message: `CODE: message`. */
export function coreError(e: unknown): { code: string; message: string } {
  const raw = (e as Error | undefined)?.message ?? String(e);
  const text = raw.replace(/^Error invoking remote method '[^']*': (Error: )?/, "");
  const m = /^([A-Z][A-Z0-9_]{2,}): ([\s\S]*)$/.exec(text);
  return m ? { code: m[1]!, message: m[2]! } : { code: "", message: text };
}

/** Codes that mean the policy or the registry refuses a model (AFW-H02). */
export const POLICY_CODES: readonly string[] = ["POLICY_BLOCKED", "MODEL_NOT_ALLOWED", "NO_PROVIDER", "UNKNOWN_MODEL", "NO_CREDENTIAL"];
export const isPolicyRefusal = (code: string): boolean => POLICY_CODES.includes(code);
/** A run that failed because the Core would not route to the model (ROUTE_REFUSED, with the Core's own detail), or a policy code. */
export const isModelRefusal = (code: string): boolean => isPolicyRefusal(code) || code === "ROUTE_REFUSED";
