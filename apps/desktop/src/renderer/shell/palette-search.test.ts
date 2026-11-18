/**
 * REQ-PX-209 (DR-PX-2026-11-11-021 MWS-15): the palette's file walk and its matching, and the conversation results taken from
 * the Core's hits. The walk is driven over a fake listing here to pin its rules (bound, protected, skipped, refused); the real
 * listing is proven by e2e/px-209-search.spec.ts against the Core.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import type { WorkspaceEntry } from "../../preload/preload.ts";
import type { ConversationHitView } from "../../shared/conversation-types.ts";
import { conversationMatches, matchFiles, walkFolder } from "./palette-search.ts";

const f = (path: string, extra: Partial<WorkspaceEntry> = {}): WorkspaceEntry => ({ path, name: path.slice(path.lastIndexOf("/") + 1), kind: "file", size: "1", protected: false, ...extra });
const d = (path: string, extra: Partial<WorkspaceEntry> = {}): WorkspaceEntry => f(path, { kind: "dir", ...extra });

function tree(dirs: Record<string, WorkspaceEntry[]>, refuse: string[] = []) {
  const asked: string[] = [];
  const list = async (_t: string, path: string) => {
    asked.push(path);
    if (refuse.includes(path)) throw new Error(`Error invoking remote method 'files:list': Error: OUTSIDE_ROOT: refused ${path}`);
    return { entries: dirs[path] ?? [], truncated: false };
  };
  return { list, asked };
}

test("the walk keeps the files the Core lists, enters folders breadth first, and never enters a protected folder, .git or node_modules", async () => {
  const t = tree({
    "": [f("README.md"), d("src"), d("secrets", { protected: true }), d(".git"), d("node_modules"), f(".env", { protected: true })],
    src: [f("src/main.ts"), d("src/util")],
    "src/util": [f("src/util/strings.ts")],
  });
  const w = await walkFolder("t", t.list, () => {}, () => false);
  assert.deepEqual(w.paths, ["README.md", "src/main.ts", "src/util/strings.ts"]);
  assert.deepEqual(t.asked, ["", "src", "src/util"]);
  assert.equal(w.done, true);
  assert.equal(w.partial, false);
  assert.equal(w.error, null);
});

test("the walk stops at its bound and says so; a cut listing says so too", async () => {
  const t = tree({ "": [d("a"), d("b"), d("c")], a: [f("a/1")], b: [f("b/1")], c: [f("c/1")] });
  const w = await walkFolder("t", t.list, () => {}, () => false, { dirs: 2, files: 100 });
  assert.equal(w.partial, true);
  assert.deepEqual(w.paths, ["a/1"]);
  const cut = await walkFolder("t", async () => ({ entries: [f("x")], truncated: true }), () => {}, () => false);
  assert.equal(cut.partial, true);
});

test("a refused root is an error to show; a refused sub-folder leaves the rest searched", async () => {
  const root = await walkFolder("t", tree({}, [""]).list, () => {}, () => false);
  assert.match(root.error ?? "", /OUTSIDE_ROOT/);
  assert.deepEqual(root.paths, []);
  const sub = await walkFolder("t", tree({ "": [d("a"), f("b.txt")] }, ["a"]).list, () => {}, () => false);
  assert.deepEqual(sub.paths, ["b.txt"]);
  assert.equal(sub.partial, true);
  assert.equal(sub.error, null);
});

test("a cancelled walk stops asking", async () => {
  let stop = false;
  const t = tree({ "": [d("a"), d("b")], a: [], b: [] });
  await walkFolder("t", t.list, () => (stop = true), () => stop);
  assert.deepEqual(t.asked, [""]);
});

test("files are found by name: a name that starts with the text, then one that holds it, then a path that holds it; scattered letters do not match", () => {
  const paths = ["docs/notes.md", "src/notes/index.ts", "src/footnotes.ts", "notes.txt", "src/other.ts", "n/o/t/e/s"];
  assert.deepEqual(
    matchFiles("notes", paths).map((m) => m.path),
    ["notes.txt", "docs/notes.md", "src/footnotes.ts", "src/notes/index.ts"],
  );
  assert.deepEqual(matchFiles("NOTES.TXT", paths).map((m) => [m.path, m.at, m.length]), [["notes.txt", 0, 9]]);
  const [hit] = matchFiles("foot", paths);
  assert.equal(hit!.path.slice(hit!.at, hit!.at + hit!.length), "foot");
  assert.deepEqual(matchFiles("   ", paths), []);
  assert.equal(matchFiles("s", paths, 2).length, 2);
});

test("a conversation result is the hit's first snippet with its task; a hit on the title alone is not one", () => {
  const hit = (taskId: string, snippets: number): ConversationHitView => ({
    taskId,
    title: `task ${taskId}`,
    statusClass: "RUNNING",
    statusLabel: "Working",
    archived: false,
    titleMatched: snippets === 0,
    matchedRows: snippets,
    score: 1,
    lastOffset: "1",
    snippets: Array.from({ length: snippets }, (_, i) => ({ rowId: `r${i}`, kind: "ASSISTANT_MESSAGE", source: "BODY", offset: "1", turnId: "t", text: `text ${i}`, cutBefore: i > 0, cutAfter: false, matches: [{ start: 0, end: 4 }], score: 1 })),
  });
  const m = conversationMatches([hit("a", 0), hit("b", 2), hit("c", 1)]);
  assert.deepEqual(m.map((x) => [x.taskId, x.rowId, x.text, x.taskTitle]), [["b", "r0", "text 0", "task b"], ["c", "r0", "text 0", "task c"]]);
  assert.equal(conversationMatches([hit("b", 1), hit("c", 1)], 1).length, 1);
});
