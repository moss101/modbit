/**
 * Inbox's Updates (REQ-PX-206; DR-PX-2026-11-11-021 MWS-13 and section 9): the persisted record of what happened to the
 * person's tasks while they looked elsewhere: a task ready for review, a finished child task, an automation's run, a failure.
 * The record is kept by this process under the data directory, capped, and cleared by the person (section 7 of the record:
 * "PX-206 adds a store in the main process"). It is a log of notices, never a second copy of the Core's state:
 *
 * - every entry is made here from the Core's own agent headers (the status class the Core computed), never from what a
 *   renderer says; no renderer call can add one;
 * - an entry holds ids, its kind, the task's title, a time and the log offset it is about: no transcript text, no reason in
 *   the Core's words, no model output (section 9: "The updates record holds titles and ids, never transcript text");
 * - whether an entry is read is not stored here. It is the Core's read marker for the task (`MarkRead`, persisted on the
 *   task's log): an entry is read once the Core's read offset for its task has reached the entry's offset. Opening an entry
 *   moves that marker through the Core, so a read state survives a restart of the app or of the Core and agrees with the
 *   task list's unread dot.
 */
import { randomBytes } from "node:crypto";
import { existsSync, readFileSync, renameSync, writeFileSync } from "node:fs";

/** How many entries the record keeps; the oldest go first. */
export const UPDATES_CAP = 200;
/** A title longer than this is cut: the record names a task, it does not hold its goal. */
export const TITLE_MAX = 120;
/** How many tasks the record remembers the last notice of (so a restart does not notice the same state twice). */
const LAST_MAX = 2000;

export const UPDATE_KINDS = ["READY_FOR_REVIEW", "CHILD_FINISHED", "AUTOMATION_RAN", "FAILED"] as const;
export type UpdateKind = (typeof UPDATE_KINDS)[number];

/** One entry, exactly these fields: the record's shape is asserted by the qualification. */
export interface UpdateEntry {
  id: string;
  kind: UpdateKind;
  taskId: string;
  title: string;
  atMs: number;
  /** The Core's log offset the notice is about (decimal); the entry is read once the task's read offset reaches it. */
  offset: string;
}
export const ENTRY_KEYS = ["id", "kind", "taskId", "title", "atMs", "offset"] as const;

/** What the record reads of one Core agent header. */
export interface HeaderFacts {
  taskId: string;
  title: string;
  statusClass: string;
  subagent: boolean;
  origin: string;
  lastOffset: string;
  readOffset: string;
  updatedAtMs: number;
}

const HEX32 = /^[0-9a-f]{32}$/;
const OFFSET = /^\d{1,19}$/;

/** The notice a header warrants now, from the Core's status class (null: none). A finished child or automation run is named as such. */
export function kindOf(h: Pick<HeaderFacts, "statusClass" | "subagent" | "origin">): UpdateKind | null {
  const ready = h.statusClass === "READY_FOR_REVIEW_UNSEEN" || h.statusClass === "READY_FOR_REVIEW_SEEN";
  const done = ready || h.statusClass === "COMPLETED";
  if (h.statusClass === "FAILED") return "FAILED";
  if (!done) return null;
  if (h.subagent || h.origin === "subagent") return "CHILD_FINISHED";
  if (h.origin === "automation") return "AUTOMATION_RAN";
  return ready ? "READY_FOR_REVIEW" : null;
}

function clipTitle(t: string): string {
  const s = t.replace(/\s+/g, " ").trim();
  if (s.length <= TITLE_MAX) return s;
  return `${[...s].slice(0, TITLE_MAX - 1).join("")}…`;
}

/** An entry read back from disk, rebuilt field by field (anything else in the file is dropped), or null when it is not one. */
export function cleanEntry(v: unknown): UpdateEntry | null {
  if (!v || typeof v !== "object" || Array.isArray(v)) return null;
  const o = v as Record<string, unknown>;
  if (typeof o.id !== "string" || !HEX32.test(o.id)) return null;
  if (typeof o.kind !== "string" || !(UPDATE_KINDS as readonly string[]).includes(o.kind)) return null;
  if (typeof o.taskId !== "string" || !HEX32.test(o.taskId)) return null;
  if (typeof o.title !== "string") return null;
  if (typeof o.atMs !== "number" || !Number.isFinite(o.atMs) || o.atMs < 0) return null;
  if (typeof o.offset !== "string" || !OFFSET.test(o.offset)) return null;
  return { id: o.id, kind: o.kind as UpdateKind, taskId: o.taskId, title: clipTitle(o.title), atMs: Math.floor(o.atMs), offset: o.offset };
}

export interface UpdateWithRead extends UpdateEntry {
  read: boolean;
}

/** The entries newest first, each with the Core's read state: read once the task's read offset has reached the entry's offset. A task the Core no longer lists has nothing left to read. */
export function withReadState(entries: readonly UpdateEntry[], headers: readonly Pick<HeaderFacts, "taskId" | "readOffset">[]): UpdateWithRead[] {
  const readOf = new Map(headers.map((h) => [h.taskId, h.readOffset] as const));
  return [...entries]
    .sort((a, b) => b.atMs - a.atMs || (BigInt(b.offset) > BigInt(a.offset) ? 1 : BigInt(b.offset) < BigInt(a.offset) ? -1 : 0))
    .map((e) => {
      const r = readOf.get(e.taskId);
      return { ...e, read: r === undefined || !OFFSET.test(r) ? true : BigInt(e.offset) <= BigInt(r) };
    });
}

/** Where a click on an operating-system notification goes: the task its id names (`task:<id>`), else Inbox. */
export function notificationTarget(id: string): { taskId: string | null } {
  const m = /^task:([0-9a-f]{32})$/.exec(id);
  return { taskId: m ? m[1]! : null };
}

export class UpdatesRecord {
  private list: UpdateEntry[] = [];
  /** The notice last made for each task (absent: the task is in no state worth one), so a state is noticed once. */
  private last = new Map<string, UpdateKind>();

  private readonly file: string;
  readonly cap: number;
  private readonly now: () => number;

  constructor(file: string, cap: number = UPDATES_CAP, now: () => number = Date.now) {
    this.file = file;
    this.cap = cap;
    this.now = now;
    this.load();
  }

  /** Reads the file; a missing or unreadable one is an empty record (the record is a convenience log, the Core holds the facts). */
  load(): void {
    this.list = [];
    this.last = new Map();
    if (!existsSync(this.file)) return;
    try {
      const raw = JSON.parse(readFileSync(this.file, "utf8")) as { entries?: unknown; last?: unknown };
      const entries = Array.isArray(raw.entries) ? raw.entries.map(cleanEntry).filter((e): e is UpdateEntry => e !== null) : [];
      const seen = new Set<string>();
      this.list = entries.filter((e) => (seen.has(e.id) ? false : (seen.add(e.id), true)));
      if (raw.last && typeof raw.last === "object" && !Array.isArray(raw.last)) {
        for (const [k, v] of Object.entries(raw.last as Record<string, unknown>)) if (HEX32.test(k) && typeof v === "string" && (UPDATE_KINDS as readonly string[]).includes(v)) this.last.set(k, v as UpdateKind);
      }
      this.trim();
    } catch {
      this.list = [];
      this.last = new Map();
    }
  }

  /** Written whole to a temporary file and moved into place, so a crash leaves the old record or the new one. */
  save(): void {
    const body = JSON.stringify({ version: 1, cap: this.cap, entries: this.list, last: Object.fromEntries(this.last) });
    const tmp = `${this.file}.${process.pid}.tmp`;
    writeFileSync(tmp, body, { mode: 0o600 });
    renameSync(tmp, this.file);
  }

  private trim(): void {
    if (this.list.length > this.cap) this.list.splice(0, this.list.length - this.cap);
    while (this.last.size > LAST_MAX) this.last.delete(this.last.keys().next().value as string);
  }

  /** Notes what the Core's headers say now; true when an entry was added. A state already noticed for a task is not noticed again until the task leaves it. */
  observe(headers: readonly HeaderFacts[]): boolean {
    let added = false;
    let changed = false;
    for (const h of headers) {
      if (!HEX32.test(h.taskId) || !OFFSET.test(h.lastOffset)) continue;
      const kind = kindOf(h);
      const prev = this.last.get(h.taskId);
      if (kind === null) {
        if (prev !== undefined) {
          this.last.delete(h.taskId);
          changed = true;
        }
        continue;
      }
      if (prev === kind) continue;
      this.list.push({ id: randomBytes(16).toString("hex"), kind, taskId: h.taskId, title: clipTitle(h.title), atMs: h.updatedAtMs > 0 ? h.updatedAtMs : this.now(), offset: h.lastOffset });
      this.last.delete(h.taskId);
      this.last.set(h.taskId, kind);
      added = true;
      changed = true;
    }
    if (changed) this.trim();
    return added || changed;
  }

  entries(): UpdateEntry[] {
    return this.list.map((e) => ({ ...e }));
  }

  get(id: string): UpdateEntry | undefined {
    const e = this.list.find((x) => x.id === id);
    return e ? { ...e } : undefined;
  }

  /** The person clears the record. What each task was last noticed for is kept, so a cleared notice does not come back. */
  clear(): number {
    const n = this.list.length;
    this.list = [];
    return n;
  }
}
