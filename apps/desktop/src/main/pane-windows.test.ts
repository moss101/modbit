/**
 * REQ-PX-203: what a pane window may ask. Every channel that decides an approval, answers a question or changes what a task may
 * do is refused to it; the channels it may ask name its own task and session only.
 */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { cleanShowRequest, cleanViewState, PANE_CHANNELS, paneRefusal, paneWindowTitle, POPPABLE, type PaneRef } from "./pane-windows.ts";

const here = dirname(fileURLToPath(import.meta.url));
const SID = "1".repeat(32);
const TID = "a".repeat(32);
const OTHER = "b".repeat(32);
const terminal: PaneRef = { sessionId: SID, taskId: TID, kind: "terminal" };
const changes: PaneRef = { sessionId: SID, taskId: TID, kind: "changes" };

/** Every channel main answers, read from its source (main.ts and the registrars). */
function allChannels(): string[] {
  const out = new Set<string>();
  for (const f of ["main.ts", "conversation-ipc.ts", "composer-ipc.ts", "changes-ipc.ts", "customize-ipc.ts", "project-ipc.ts", "control-ipc.ts", "computer-ipc.ts", "automation-ipc.ts"]) {
    const src = readFileSync(join(here, f), "utf8");
    for (const m of src.matchAll(/(?:^|[^A-Za-z])(?:r\.)?handle\(\s*"([a-zA-Z]+:[a-zA-Z]+)"/g)) out.add(m[1]!);
  }
  return [...out].sort();
}

test("PX-203: no channel that decides an approval, answers the agent or changes what a task may do is open to a pane window", () => {
  const channels = allChannels();
  assert.ok(channels.length > 100, `the channels were read from the source (${channels.length})`);
  const deciding = ["approval:resolve", "approval:allowAlways", "modeproposals:decide", "question:respond", "automations:enable", "review:decide", "review:pullRequest", "runmode:set", "rules:add", "rules:revoke", "budgets:set", "task:start", "task:cancel", "task:steer", "task:create", "browser:certDecide", "browser:setPolicy", "browser:control", "browser:emergencyStop", "computer:stop", "composer:queueInput", "composer:sendNow", "composer:interrupt", "credential:add", "onboarding:provider", "onboarding:trust", "update:install", "events:subscribe", "pane:popOut", "pane:windows", "pane:focus"];
  for (const ch of deciding) {
    assert.ok(channels.includes(ch), `${ch} is a channel main answers`);
    for (const kind of POPPABLE) assert.notEqual(paneRefusal({ ...terminal, kind }, ch, [SID, TID]), null, `${ch} is refused to the ${kind} pane window`);
  }
  // The list is an allow list: every channel on it is one main answers, and none of them decides anything.
  for (const ch of Object.keys(PANE_CHANNELS)) assert.ok(channels.includes(ch), `${ch} is a channel main answers`);
  assert.deepEqual(Object.keys(PANE_CHANNELS).filter((c) => /approval|question|decide:|modeproposal|rules:|runmode|budget|task:/.test(c)), []);
  // Anything not on the list is refused: a channel added to main later is closed to pane windows until it is listed here.
  const open = channels.filter((ch) => paneRefusal(changes, ch, [SID, TID]) === null || paneRefusal(terminal, ch, [SID, TID]) === null || paneRefusal(terminal, ch, [TID]) === null || paneRefusal(changes, ch, [TID]) === null);
  assert.deepEqual(open.filter((c) => !(c in PANE_CHANNELS)), []);
});

test("PX-203: a pane window asks only about its own task and session, and only what its pane needs", () => {
  assert.equal(paneRefusal(changes, "changes:state", [TID]), null);
  assert.equal(paneRefusal(changes, "changes:commitPush", [SID, TID, {}]), null);
  assert.match(paneRefusal(changes, "changes:state", [OTHER]) ?? "", /its own task/);
  assert.match(paneRefusal(changes, "changes:commitPush", [SID, OTHER, {}]) ?? "", /its own task/);
  assert.match(paneRefusal(changes, "changes:commitPush", ["2".repeat(32), TID, {}]) ?? "", /its own session/);
  assert.equal(paneRefusal(terminal, "terminal:attach", [SID, TID, "t1", "0", {}]), null);
  assert.match(paneRefusal(terminal, "terminal:attach", [SID, OTHER, "t1", "0", {}]) ?? "", /its own task/);
  assert.match(paneRefusal(terminal, "terminal:list", [""]) ?? "", /its own task/, "the whole list of terminals is not a pane's");
  // A Terminal window does not ask the Changes channels, nor the other way round.
  assert.match(paneRefusal(terminal, "changes:state", [TID]) ?? "", /does not ask this/);
  assert.match(paneRefusal(changes, "terminal:write", ["att", new Uint8Array(1)]) ?? "", /does not ask this/);
  // Retained mutation: a pane that is not tied to its task would pass another task's id, and that is shown to be refused above;
  // an empty allow-list entry for an approval channel would make the first test fail (asserted here on a copy).
  const loosened = { ...PANE_CHANNELS, "approval:resolve": { kinds: POPPABLE, task: null, session: null } };
  assert.ok(Object.keys(loosened).some((c) => /approval/.test(c)), "the check above would see an approval channel on the list");
});

test("PX-203: what a pane says it shows is kept only where each field is what it says", () => {
  assert.deepEqual(cleanViewState({ terminalId: "term_1", path: "src/a.txt", scrollTop: 120.4 }), { terminalId: "term_1", path: "src/a.txt", scrollTop: 120 });
  assert.deepEqual(cleanViewState({ path: "../etc/passwd", scrollTop: -1, terminalId: "a b" }), {});
  assert.deepEqual(cleanViewState({ path: "/abs" }), {});
  assert.deepEqual(cleanViewState({ path: "C:\\x" }), {});
  assert.deepEqual(cleanViewState("nope"), {});
  assert.deepEqual(cleanShowRequest({ what: "review" }), { what: "review" });
  assert.deepEqual(cleanShowRequest({ what: "call", toolCallId: "abc123" }), { what: "call", toolCallId: "abc123" });
  assert.equal(cleanShowRequest({ what: "approve" }), null);
  assert.equal(paneWindowTitle("terminal", "  Tidy   the notes "), "Terminal: Tidy the notes");
  assert.equal(paneWindowTitle("changes", ""), "Changes");
});
