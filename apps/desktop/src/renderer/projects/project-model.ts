/**
 * The project surfaces' pure view model (REQ-PX-063, 064): the words for a
 * project's rollup, the colour a palette role names, the Core's typed refusal
 * turned into a sentence, the menu a task row offers. Nothing here decides
 * membership or counts anything: the Core's map and rollup are shown as given
 * (docs/81), and a refusal is the Core's code and its own explanation.
 */
import type { AgentHeaderView } from "../../shared/conversation-types.ts";
import { PROJECT_COLORS, PROJECT_ICONS, type ProjectInfo, type ProjectRollupInfo } from "../../shared/project-types.ts";

const kebab = (s: string) => s.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);

/** The CSS colour of a palette role; an unknown role falls back to the accent (the Core only stores known ones). */
export function colorVar(role: string): string {
  return `var(--mb-color-${kebab((PROJECT_COLORS as readonly string[]).includes(role) ? role : "accent")})`;
}

export const iconName = (icon: string): (typeof PROJECT_ICONS)[number] => ((PROJECT_ICONS as readonly string[]).includes(icon) ? icon : "folder") as (typeof PROJECT_ICONS)[number];

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** One line for a project's counts: tasks, attention, approvals, pull requests. Only what the Core's rollup holds. */
export function rollupWords(r: ProjectRollupInfo): string {
  const parts = [plural(r.members, "task", "tasks")];
  if (r.attentionTasks > 0) parts.push(`${r.attentionTasks} ${r.attentionTasks === 1 ? "needs" : "need"} attention`);
  else if (r.members > 0) parts.push("none need attention");
  if (r.pendingApprovals > 0) parts.push(plural(r.pendingApprovals, "approval waiting", "approvals waiting"));
  if (r.pullRequests > 0) parts.push(plural(r.pullRequests, "pull request", "pull requests"));
  return parts.join(" · ");
}

/** The CI line for the pull requests of a project, or null when no pull request exists (nothing is invented). */
export function ciWords(r: ProjectRollupInfo): string | null {
  if (r.pullRequests === 0) return null;
  const parts: string[] = [];
  if (r.ciPassing > 0) parts.push(`${r.ciPassing} passing`);
  if (r.ciFailing > 0) parts.push(`${r.ciFailing} failing`);
  if (r.ciPending > 0) parts.push(`${r.ciPending} pending`);
  const unknown = r.pullRequests - r.ciPassing - r.ciFailing - r.ciPending;
  if (unknown > 0) parts.push(`${unknown} with no CI result recorded`);
  return parts.join(", ");
}

/** The text of a pull request's CI for one member, or null when the log holds none. */
export function memberCiWords(pr: { hasCi: boolean; checksPassed: number; checksFailed: number; checksPending: number }): string {
  if (!pr.hasCi) return "no CI result recorded";
  const parts: string[] = [];
  if (pr.checksFailed > 0) parts.push(`${pr.checksFailed} failed`);
  if (pr.checksPending > 0) parts.push(`${pr.checksPending} pending`);
  if (pr.checksPassed > 0) parts.push(`${pr.checksPassed} passed`);
  return parts.join(", ") || "no checks";
}

export interface Refusal {
  /** The Core's typed code, "" when the error carried none. */
  code: string;
  /** The Core's own sentence. */
  detail: string;
}

/** The Core's refusal inside an IPC error: `Error invoking remote method 'x': Error: CODE: sentence`. */
export function parseRefusal(e: unknown): Refusal {
  const raw = e instanceof Error ? e.message : String(e);
  const text = raw.replace(/^Error invoking remote method '[^']*': /, "").replace(/^Error: /, "");
  const m = /^([A-Z][A-Z0-9_]{2,}): ([\s\S]*)$/.exec(text);
  return m ? { code: m[1]!, detail: m[2]! } : { code: "", detail: text };
}

/** What each typed refusal of a project command means for the person, in Modbit's words; the Core's detail follows it. */
const REFUSAL_WORDS: Record<string, string> = {
  TASK_NOT_TOP_LEVEL: "Only a top-level task can join a project.",
  TASK_IS_DRAFT: "A draft cannot join a project yet.",
  WORKSPACE_MISMATCH: "A project holds tasks of its own workspace only.",
  IN_ANOTHER_PROJECT: "A task is in one project at a time.",
  NOT_A_MEMBER: "That task is not in this project.",
  PROJECT_ARCHIVED: "This project is archived.",
  PROJECT_FULL: "This project is full.",
  PROJECT_NAME_TAKEN: "That name is already used in this workspace.",
  PROJECT_NAME_INVALID: "That name will not do.",
  PROJECT_COLOR_INVALID: "That colour is not in the palette.",
  PROJECT_ICON_INVALID: "That icon is not in the set.",
  PROJECT_LIMIT: "This workspace has as many projects as it can hold.",
  UNKNOWN_PROJECT: "That project no longer exists.",
  UNKNOWN_TASK: "That task no longer exists.",
  STALE_LEASE: "This window lost its place in the session. Try again.",
};

/** A refusal as one sentence for the list's note: the Core's code decides the lead, its detail follows. */
export function refusalSentence(what: string, e: unknown): string {
  const r = parseRefusal(e);
  const lead = REFUSAL_WORDS[r.code];
  return `${what}: ${lead ? `${lead} ` : ""}${r.detail}`.trim();
}

/** The entries of the "Project" menu on a task row: one per live project and, for a member, the way out. Every entry is offered; the Core refuses what is not allowed and says why. */
export interface ProjectMenuEntry {
  id: string;
  label: string;
  kind: "add" | "remove";
  projectId: string;
  checked: boolean;
}

export function projectMenuFor(h: AgentHeaderView, projects: readonly ProjectInfo[]): ProjectMenuEntry[] {
  const live = projects.filter((p) => !p.archived);
  const current = h.projectId ? projects.find((p) => p.projectId === h.projectId) : undefined;
  const out: ProjectMenuEntry[] = live.map((p) => ({ id: `add:${p.projectId}`, label: `Add to ${p.name}`, kind: "add", projectId: p.projectId, checked: p.projectId === h.projectId }));
  if (current) out.push({ id: `remove:${current.projectId}`, label: `Remove from ${current.name}`, kind: "remove", projectId: current.projectId, checked: false });
  return out;
}

/** The workspaces a new project can be bound to: those of the tasks in the list and of the projects already made, the task's own first. */
export function workspaceChoices(headers: readonly AgentHeaderView[], projects: readonly ProjectInfo[], preferred?: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  const add = (root: string) => {
    if (root && !seen.has(root)) {
      seen.add(root);
      out.push(root);
    }
  };
  if (preferred) add(preferred);
  for (const h of headers) if (!h.subagent) add(h.workspaceRoot);
  for (const p of projects) add(p.workspaceRoot);
  return out;
}

/** A fresh command id (32 hex characters) so a retried command acts once. */
export function newCommandId(): string {
  const b = new Uint8Array(16);
  crypto.getRandomValues(b);
  return Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
}

/** A view id: short, unique enough for a per-viewer list. */
export function newViewId(): string {
  return `v${newCommandId().slice(0, 12)}`;
}
