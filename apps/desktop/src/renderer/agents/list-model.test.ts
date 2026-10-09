import { test } from "node:test";
import assert from "node:assert/strict";
import { STATUS_CLASSES, type AgentHeaderView, type StatusClass } from "../../shared/conversation-types.ts";
import { CLASS_META, NO_FILTERS, PIN_CAP, PIN_CAP_MESSAGE, SECTION_HEIGHT, ROW_HEIGHT, classRank, filterCount, flatten, groupHeaders, layoutOf, nearestSibling, passes, pin, relativeAge, timeBucket, unpin, visibleRange, visibleTaskIds } from "./list-model.ts";
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
  assert.equal(filterCount({ states: ["running"], locations: ["local"], origins: [], showArchived: true }), 3);
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
