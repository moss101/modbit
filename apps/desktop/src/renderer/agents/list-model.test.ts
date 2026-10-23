import { test } from "node:test";
import assert from "node:assert/strict";
import { STATUS_CLASSES, type AgentHeaderView, type StatusClass } from "../../shared/conversation-types.ts";
import { CLASS_META, MAX_VIEWS, NO_FILTERS, NO_PROJECT, PIN_CAP, PIN_CAP_MESSAGE, SECTION_HEIGHT, ROW_HEIGHT, classRank, deleteView, filterCount, flatten, groupHeaders, layoutOf, listItems, nearestSibling, passes, pin, relativeAge, saveView, timeBucket, unpin, viewMatches, visibleRange, visibleTaskIds, type ListItem, type StoredView } from "./list-model.ts";
import type { ProjectInfo } from "../../shared/project-types.ts";
import { DEFAULT_LIST_PREFS, parseListPrefs, serializeListPrefs } from "./prefs.ts";
import { highlightRuns } from "./snippets.ts";

const NOW = new Date(2026, 9, 8, 15, 0, 0).getTime();
let n = 0;
function h(over: Partial<AgentHeaderView> = {}): AgentHeaderView {
  n++;
  return { taskId: `t${String(n).padStart(3, "0")}`, sessionId: "s", workspaceRoot: "/w/alpha", title: `Task ${n}`, subtitle: "alpha", createdAtMs: NOW - n * 1000, updatedAtMs: NOW - n * 1000, statusClass: "RUNNING", statusLabel: "Running", unread: false, pendingApproval: false, pendingPlan: false, contextPercent: 0, filesChanged: 0, linesAdded: 0, linesRemoved: 0, lastCheckpointAtMs: 0, subagent: false, archived: false, executionLocation: "local", origin: "desktop", taskState: "Running", lastOffset: "1", readOffset: "0", attentionItems: 0, ...over };
}

test("every status class has a glyph and words of its own, in the Core's precedence order", () => {
  assert.deepEqual([...STATUS_CLASSES].sort(), Object.keys(CLASS_META).sort());
  const words = Object.values(CLASS_META).map((m) => m.short);
  assert.equal(new Set(words).size, words.length, "no two classes share their words");
  assert.ok(classRank("NEEDS_ATTENTION") < classRank("FAILED"));
  assert.ok(classRank("FAILED") < classRank("READY_FOR_REVIEW_UNSEEN"));
  assert.ok(classRank("READY_FOR_REVIEW_UNSEEN") < classRank("READY_FOR_REVIEW_SEEN"));
  assert.ok(classRank("READY_FOR_REVIEW_SEEN") < classRank("RUNNING"));
  assert.ok(classRank("RUNNING") < classRank("COMPLETED"));
  assert.ok(classRank("COMPLETED") < classRank("DRAFT"));
});

test("single source: the class a header carries is what is shown, whatever its other fields say", () => {
  // A header that claims COMPLETED while carrying an unread flag, a pending approval and attention items is still COMPLETED here.
  const contradictory = h({ statusClass: "COMPLETED", unread: true, pendingApproval: true, attentionItems: 3, taskState: "Failed" });
  const sections = groupHeaders([contradictory], "status", [], NOW);
  assert.equal(sections[0]!.key, "status:COMPLETED");
  assert.equal(CLASS_META[contradictory.statusClass].short, "Completed");
});

test("status grouping orders sections by the Core's precedence, not by recency", () => {
  const rows: AgentHeaderView[] = (["DRAFT", "RUNNING", "NEEDS_ATTENTION", "COMPLETED", "FAILED"] as StatusClass[]).map((c, i) => h({ statusClass: c, updatedAtMs: NOW - i }));
  assert.deepEqual(groupHeaders(rows, "status", [], NOW).map((s) => s.key), ["status:NEEDS_ATTENTION", "status:FAILED", "status:RUNNING", "status:COMPLETED", "status:DRAFT"]);
});

test("repository grouping is the default shape: newest activity first inside and between sections; same-named folders stay apart", () => {
  const a1 = h({ workspaceRoot: "/w/alpha", subtitle: "alpha", updatedAtMs: NOW - 10 });
  const b1 = h({ workspaceRoot: "/w/beta", subtitle: "beta", updatedAtMs: NOW - 5 });
  const a2 = h({ workspaceRoot: "/w/alpha", subtitle: "alpha", updatedAtMs: NOW - 1 });
  const s = groupHeaders([a1, b1, a2], "repository", [], NOW);
  assert.deepEqual(s.map((x) => x.label), ["alpha", "beta"]);
  assert.deepEqual(s[0]!.rows.map((r) => r.taskId), [a2.taskId, a1.taskId]);
  const x = h({ workspaceRoot: "/one/app", subtitle: "app" });
  const y = h({ workspaceRoot: "/two/app", subtitle: "app" });
  const labels = groupHeaders([x, y], "repository", [], NOW).map((z) => z.label);
  assert.equal(new Set(labels).size, 2);
});

test("time buckets are local calendar days and deterministic for a given now", () => {
  const day = 86_400_000;
  assert.equal(timeBucket(NOW - 60_000, NOW), "today");
  assert.equal(timeBucket(new Date(2026, 9, 7, 23, 0).getTime(), NOW), "yesterday");
  assert.equal(timeBucket(NOW - 4 * day, NOW), "week");
  assert.equal(timeBucket(NOW - 30 * day, NOW), "older");
  const s = groupHeaders([h({ updatedAtMs: NOW - 30 * day }), h({ updatedAtMs: NOW - 1000 })], "time", [], NOW);
  assert.deepEqual(s.map((x) => x.label), ["Today", "Older"]);
});

test("pinned tasks form the first section, in pin order, whatever the grouping, and leave the other sections", () => {
  const rows = [h(), h(), h()];
  const s = groupHeaders(rows, "status", [rows[2]!.taskId, rows[0]!.taskId], NOW);
  assert.equal(s[0]!.pinned, true);
  assert.deepEqual(s[0]!.rows.map((r) => r.taskId), [rows[2]!.taskId, rows[0]!.taskId]);
  assert.equal(s.flatMap((x) => x.rows).length, 3);
  // A pin whose task is gone is ignored.
  assert.equal(groupHeaders(rows, "status", ["gone"], NOW)[0]!.pinned, undefined);
});

test("a pin beyond the cap changes nothing and says why", () => {
  let pins: string[] = [];
  for (let i = 0; i < PIN_CAP; i++) pins = pin(pins, `p${i}`).pins;
  const over = pin(pins, "extra");
  assert.deepEqual(over.pins, pins);
  assert.equal(over.message, PIN_CAP_MESSAGE);
  assert.equal(pin(pins, "p0").message, null, "pinning a pinned task is not an error");
  assert.equal(unpin(pins, "p0").length, PIN_CAP - 1);
});

test("filters: chips widen within a family and narrow across families; archived rows are hidden unless asked for", () => {
  const run = h({ statusClass: "RUNNING" });
  const failed = h({ statusClass: "FAILED", executionLocation: "cloud", origin: "cli" });
  const unread = h({ statusClass: "READY_FOR_REVIEW_UNSEEN", unread: true });
  const arch = h({ statusClass: "ARCHIVED", archived: true });
  const all = [run, failed, unread, arch];
  const ids = (f = NO_FILTERS) => all.filter((x) => passes(x, f)).map((x) => x.taskId);
  assert.deepEqual(ids(), [run.taskId, failed.taskId, unread.taskId]);
  assert.deepEqual(ids({ ...NO_FILTERS, states: ["running", "failed"] }), [run.taskId, failed.taskId]);
  assert.deepEqual(ids({ ...NO_FILTERS, states: ["failed"], locations: ["cloud"] }), [failed.taskId]);
  assert.deepEqual(ids({ ...NO_FILTERS, states: ["failed"], locations: ["local"] }), []);
  assert.deepEqual(ids({ ...NO_FILTERS, states: ["unread"] }), [unread.taskId]);
  assert.deepEqual(ids({ ...NO_FILTERS, states: ["done"] }), [unread.taskId]);
  assert.deepEqual(ids({ ...NO_FILTERS, origins: ["cli"] }), [failed.taskId]);
  assert.equal(ids({ ...NO_FILTERS, showArchived: true }).length, 4);
  assert.equal(filterCount({ states: ["running"], locations: ["local"], origins: [], projects: [], showArchived: true }), 3);
});

test("an archive hands the selection to the nearest remaining sibling: next, else previous, else nothing", () => {
  assert.equal(nearestSibling(["a", "b", "c"], "b"), "c");
  assert.equal(nearestSibling(["a", "b", "c"], "c"), "b");
  assert.equal(nearestSibling(["a"], "a"), null);
  assert.equal(nearestSibling(["a", "b"], "zzz"), null);
});

test("collapsed sections hide their rows and the visible order follows", () => {
  const rows = [h({ workspaceRoot: "/w/a", subtitle: "a" }), h({ workspaceRoot: "/w/b", subtitle: "b" })];
  const sections = groupHeaders(rows, "repository", [], NOW);
  const open = flatten(sections, new Set());
  assert.equal(open.length, 4);
  const closed = flatten(sections, new Set([sections[0]!.key]));
  assert.equal(closed.length, 3);
  assert.deepEqual(visibleTaskIds(closed), [sections[1]!.rows[0]!.taskId]);
});

test("a thousand rows window to the viewport: the slice is small, contiguous and follows the scroll", () => {
  const rows = Array.from({ length: 1000 }, (_, i) => h({ workspaceRoot: `/w/r${i % 10}`, subtitle: `r${i % 10}` }));
  const items = flatten(groupHeaders(rows, "repository", [], NOW), new Set());
  const layout = layoutOf(items);
  assert.equal(items.length, 1010);
  assert.equal(layout.total, 10 * SECTION_HEIGHT + 1000 * ROW_HEIGHT);
  const top = visibleRange(layout, items.length, 0, 600);
  assert.equal(top.start, 0);
  assert.ok(top.end - top.start < 30);
  const mid = visibleRange(layout, items.length, layout.total / 2, 600);
  assert.ok(mid.start > 400 && mid.end - mid.start < 40);
  const first = layout.offsets[mid.start]!;
  assert.ok(first <= layout.total / 2);
  const end = visibleRange(layout, items.length, layout.total, 600);
  assert.equal(end.end, items.length);
  assert.deepEqual(visibleRange(layout, 0, 0, 600), { start: 0, end: 0 });
});

test("relative age is short words, never a clock", () => {
  assert.equal(relativeAge(NOW - 5_000, NOW), "now");
  assert.equal(relativeAge(NOW - 5 * 60_000, NOW), "5m");
  assert.equal(relativeAge(NOW - 3 * 3_600_000, NOW), "3h");
  assert.equal(relativeAge(NOW - 3 * 86_400_000, NOW), "3d");
  assert.equal(relativeAge(0, NOW), "");
});

test("list preferences: a hostile or stale stored value is cut back to what is valid; the pin cap holds", () => {
  assert.deepEqual(parseListPrefs(null), DEFAULT_LIST_PREFS);
  assert.deepEqual(parseListPrefs("{not json"), DEFAULT_LIST_PREFS);
  const many = Array.from({ length: 50 }, (_, i) => `p${i}`);
  const p = parseListPrefs(JSON.stringify({ grouping: "nonsense", filters: { states: ["running", "<script>"], showArchived: "yes" }, pins: many, collapsed: { status: ["x"], evil: ["y"] }, subtitle: ["origin", 7] }));
  assert.equal(p.grouping, "repository");
  assert.deepEqual(p.filters.states, ["running"]);
  assert.equal(p.filters.showArchived, false);
  assert.equal(p.pins.length, PIN_CAP);
  assert.deepEqual(Object.keys(p.collapsed), ["status"]);
  assert.deepEqual(p.subtitle, ["origin"]);
  assert.deepEqual(parseListPrefs(serializeListPrefs(p)), p);
});

test("search snippets highlight by UTF-8 byte ranges and drop ranges that are out of order or split a character", () => {
  assert.deepEqual(highlightRuns("hello world", [{ start: 6, end: 11 }]), [{ text: "hello ", match: false }, { text: "world", match: true }]);
  // "é" is two bytes: the range 1..2 splits it and is dropped; 0..2 covers it.
  assert.deepEqual(highlightRuns("éa", [{ start: 1, end: 2 }]), [{ text: "éa", match: false }]);
  assert.deepEqual(highlightRuns("éa", [{ start: 0, end: 2 }]), [{ text: "é", match: true }, { text: "a", match: false }]);
  assert.deepEqual(highlightRuns("abc", [{ start: 2, end: 3 }, { start: 0, end: 1 }]).filter((r) => r.match).map((r) => r.text), ["c"]);
  assert.deepEqual(highlightRuns("abc", [{ start: 0, end: 99 }]), [{ text: "abc", match: false }]);
});

// ------------------------------------------------------------ PX-064: projects

const proj = (id: string, name: string, over: Partial<ProjectInfo> = {}): ProjectInfo => ({ projectId: id, name, color: "accent", icon: "folder", workspaceRoot: "/w/alpha", archived: false, createdAtMs: 1, updatedAtMs: 1, createdOffset: "1", lastOffset: "1", rollup: { members: 0, byStatus: [], attentionTasks: 0, attentionItems: 0, pendingApprovals: 0, unread: 0, pullRequests: 0, ciPassing: 0, ciFailing: 0, ciPending: 0 }, members: [], ...over });
const items = (over: Partial<Parameters<typeof listItems>[0]> & { headers: ReturnType<typeof h>[] }) => listItems({ projects: [], grouping: "repository", pins: [], collapsed: new Set(), nowMs: NOW, sort: "recent", showArchived: false, ...over });
const kinds = (xs: ListItem[]) => xs.map((i) => (i.kind === "row" ? `row:${i.header.taskId}` : i.kind === "section" ? `section:${i.label}` : i.kind === "project" ? `project:${i.project.name}` : i.kind));

test("without a project the list is exactly what it was: no Projects group, no new row", () => {
  const rows = [h(), h()];
  const before = flatten(groupHeaders(rows, "repository", [], NOW), new Set());
  assert.deepEqual(items({ headers: rows }), before);
});

test("the Projects group sits after the pins and before the repositories; a member is shown under its project and nowhere else", () => {
  const a = proj("pa", "Alpha");
  const b = proj("pb", "Beta");
  const m1 = h({ projectId: "pa" });
  const m2 = h({ projectId: "pa" });
  const free = h();
  const pinned = h({ projectId: "pb" });
  const out = items({ headers: [m1, m2, free, pinned], projects: [a, b], pins: [pinned.taskId] });
  assert.deepEqual(kinds(out), ["section:Pinned", `row:${pinned.taskId}`, "projects-head", "project:Alpha", `row:${m1.taskId}`, `row:${m2.taskId}`, "project:Beta", "new-project", "section:alpha", `row:${free.taskId}`]);
  // Each task appears once; the pinned member stays pinned and does not appear under its project.
  const rowIds = out.flatMap((i) => (i.kind === "row" ? [i.header.taskId] : []));
  assert.equal(new Set(rowIds).size, rowIds.length);
  const proj_b = out.find((i) => i.kind === "project" && i.project.projectId === "pb");
  assert.equal(proj_b?.kind === "project" ? proj_b.shown : -1, 0);
});

test("collapsing a project hides its tasks and collapsing the group hides the projects, each by its own key", () => {
  const a = proj("pa", "Alpha");
  const m = h({ projectId: "pa" });
  assert.deepEqual(kinds(items({ headers: [m], projects: [a], collapsed: new Set(["project:pa"]) })), ["projects-head", "project:Alpha", "new-project"]);
  assert.deepEqual(kinds(items({ headers: [m], projects: [a], collapsed: new Set(["projects"]) })), ["projects-head"], "the group is closed and the member is not listed a second time");
});

test("an archived project is not a parent unless archived items are asked for; its tasks stay listed where they were", () => {
  const old = proj("pa", "Old", { archived: true });
  const m = h({ projectId: "pa" });
  const hidden = items({ headers: [m], projects: [old] });
  assert.deepEqual(kinds(hidden), ["section:alpha", `row:${m.taskId}`]);
  const shown = items({ headers: [m], projects: [old], showArchived: true });
  assert.deepEqual(kinds(shown), ["projects-head", "project:Old", `row:${m.taskId}`, "new-project"]);
});

test("the Project grouping makes the projects the sections, empty ones included, and tasks of no project last", () => {
  const a = proj("pa", "Alpha");
  const b = proj("pb", "Beta");
  const m = h({ projectId: "pa" });
  const free = h();
  const out = items({ headers: [free, m], projects: [a, b], grouping: "project" });
  assert.deepEqual(kinds(out), ["section:Alpha", `row:${m.taskId}`, "section:Beta", "section:No project", `row:${free.taskId}`]);
  // A task whose project the list does not show is in no project here.
  assert.deepEqual(kinds(items({ headers: [m], projects: [], grouping: "project" })), ["section:No project", `row:${m.taskId}`]);
});

test("project chips filter by the Core's map; 'no project' is its own chip and a chip never matches by name", () => {
  const inA = h({ projectId: "pa" });
  const inB = h({ projectId: "pb" });
  const free = h();
  const ids = (projects: string[]) => [inA, inB, free].filter((x) => passes(x, { ...NO_FILTERS, projects })).map((x) => x.taskId);
  assert.deepEqual(ids(["pa"]), [inA.taskId]);
  assert.deepEqual(ids(["pa", "pb"]), [inA.taskId, inB.taskId]);
  assert.deepEqual(ids([NO_PROJECT]), [free.taskId]);
  assert.deepEqual(ids(["pa", NO_PROJECT]), [inA.taskId, free.taskId]);
  assert.deepEqual(ids([]), [inA.taskId, inB.taskId, free.taskId]);
  assert.equal(filterCount({ ...NO_FILTERS, projects: ["pa", NO_PROJECT] }), 2);
});

test("sorting inside sections is a stored choice: by status, by title, by recency", () => {
  const a = h({ title: "b-task", statusClass: "COMPLETED", updatedAtMs: NOW - 1 });
  const b = h({ title: "a-task", statusClass: "FAILED", updatedAtMs: NOW - 3 });
  const c = h({ title: "c-task", statusClass: "RUNNING", updatedAtMs: NOW - 2 });
  const order = (sort: "recent" | "status" | "title") => groupHeaders([a, b, c], "repository", [], NOW, { sort })[0]!.rows.map((x) => x.title);
  assert.deepEqual(order("recent"), ["b-task", "c-task", "a-task"]);
  assert.deepEqual(order("status"), ["a-task", "c-task", "b-task"]);
  assert.deepEqual(order("title"), ["a-task", "b-task", "c-task"]);
});

// ------------------------------------------------------------ stored views

test("a stored view is a saved query: saved by unique name, matched by what it stores, replaced by its name, deleted by id", () => {
  let n = 0;
  const id = () => `v${++n}`;
  const now = { grouping: "status" as const, filters: { ...NO_FILTERS, states: ["running"] as ("running" | "failed")[], projects: ["pa"] }, sort: "title" as const };
  const r = saveView([], "  Running in Alpha ", now, id);
  assert.ok(r.ok);
  if (!r.ok) return;
  assert.equal(r.view.name, "Running in Alpha");
  assert.ok(viewMatches(r.view, now));
  assert.equal(viewMatches(r.view, { ...now, sort: "recent" }), false);
  assert.equal(viewMatches(r.view, { ...now, filters: { ...now.filters, projects: [] } }), false);
  assert.equal(viewMatches(r.view, { ...now, filters: { ...now.filters, states: ["running"], projects: ["pa"] } }), true, "chip order does not matter");
  // Saving the same name (any case) replaces that view, keeping its id.
  const again = saveView(r.views, "running in alpha", { ...now, sort: "recent" }, id);
  assert.ok(again.ok);
  if (again.ok) {
    assert.equal(again.views.length, 1);
    assert.equal(again.views[0]!.id, r.view.id);
    assert.equal(again.views[0]!.sort, "recent");
  }
  // A view is a copy: changing the list's filters afterwards does not change it.
  now.filters.states.push("failed");
  assert.deepEqual(r.view.filters.states, ["running"]);
  assert.deepEqual(deleteView(r.views, r.view.id), []);
});

test("a view needs a name and the list holds a bounded number of them", () => {
  const now = { grouping: "repository" as const, filters: NO_FILTERS, sort: "recent" as const };
  assert.equal(saveView([], "   ", now, () => "x").ok, false);
  assert.equal(saveView([], "x".repeat(41), now, () => "x").ok, false);
  let views: StoredView[] = [];
  for (let i = 0; i < MAX_VIEWS; i++) {
    const r = saveView(views, `view ${i}`, now, () => `id${i}`);
    assert.ok(r.ok);
    if (r.ok) views = r.views;
  }
  const over = saveView(views, "one more", now, () => "zz");
  assert.equal(over.ok, false);
  assert.equal(saveView(views, "VIEW 3", now, () => "zz").ok, true, "replacing a view is not adding one");
});

test("stored preferences with views: hostile entries are cut back; valid views round-trip", () => {
  const good = { id: "v1", name: "Mine", grouping: "project", sort: "status", filters: { states: ["running", "<x>"], projects: ["pa", 7], showArchived: true } };
  const p = parseListPrefs(JSON.stringify({ sort: "title", views: [good, { ...good, id: "v2", name: "mine" }, { ...good, id: "v3", grouping: "nope" }, { id: "v4", name: "", grouping: "status", sort: "recent" }, "junk", null] }));
  assert.equal(p.sort, "title");
  assert.equal(p.views.length, 1, "a duplicate name, a bad grouping, no name and junk are dropped");
  assert.deepEqual(p.views[0]!.filters, { states: ["running"], locations: [], origins: [], projects: ["pa"], showArchived: true });
  assert.deepEqual(parseListPrefs(serializeListPrefs(p)), p);
  assert.equal(parseListPrefs(JSON.stringify({ sort: "sideways" })).sort, "recent");
  assert.deepEqual(parseListPrefs(JSON.stringify({ views: "nope" })).views, []);
});
