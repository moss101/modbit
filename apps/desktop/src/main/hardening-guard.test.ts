/**
 * REQ-PX-049 / QUAL-PX-049: the main process's security-boundary surface is pinned. The lifecycle work
 * (window options, quit handling) may not change `webPreferences`, the Content-Security-Policy or the preload
 * bridge, so each is hashed here: a change fails this test, and the person making it has to update the pin on
 * purpose, which is the review the boundary asks for (PX-096 owns the hardening inventory and moves these pins
 * with its own checks). The window options are pinned by key, so an option that loosens the boundary
 * (a frame, `webviewTag`, `nodeIntegrationInSubFrames`, ...) cannot arrive among the size options unseen.
 */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const mainSource = readFileSync(join(here, "main.ts"), "utf8");
// A Windows checkout may carry CRLF; the pins are over the LF form so the same source hashes the same everywhere.
const sha256 = (s: string | Buffer): string => createHash("sha256").update(s.toString("utf8").replace(/\r\n/g, "\n")).digest("hex");

/** The pins. Update one only for a reviewed change to the boundary it names. */
const PINS = {
  // M10.2 (reviewed 2026-11-07): four argument-less update channels (view, check, install, rollback). The renderer
  // chooses nothing: the main process fetches, verifies against the pinned keys, decides and installs only a staged,
  // verified update; a build with no pinned key refuses every update.
  // PX-210 (2026-11-11, for review with its pull request): one channel, `composer:suggest` (a session id and a task id, both
  // validated by main), which forwards the one typed Core command SuggestNextPrompt and returns the suggestion or the Core's
  // refusal code; and two type-only additions (the header's last line of PX-218 rides the existing headers channel; the
  // economics summary gains a count). Nothing generic, no path, no command name and no text crosses the bridge for it.
  // PX-200 (2026-11-12, for review with its pull request): two read channels, `conversation:workGraph` and `conversation:plan`
  // (a task id each, validated by main), which forward the Core's existing reads GetWorkGraph and GetPlan and return the
  // steps with their state and the plan version in force as plain data. Reads only: nothing is leased, written or sent,
  // and no path, command name or free text crosses the bridge for them.
  // PX-203 (2026-11-15, for review with its pull request): ten pane-window methods. The app's window: `pane:popOut` (a session
  // id, a task id, one of four pane kinds, a view state of a terminal id, a relative path and a scroll offset, all validated by
  // main and dropped where malformed, a title and a size), `pane:windows`, `pane:focus`, and three events it hears. A pane
  // window: `pane:context` (main says which pane of which task it shows; the window cannot choose), `pane:state`, `pane:show`
  // (asks the app's window to show the task's review, a call or the waiting approval; nothing is decided by it) and
  // `pane:return`. Main answers a pane window only on the channels its pane needs, for its own task and session
  // (pane-windows.ts PANE_CHANNELS, an allow list); no channel that decides an approval is on it. The window is made by the one
  // factory with the same webPreferences, preload and page (the pins below are unchanged; `windowFactoryDrift` checks the rest).
  // PX-206 (2026-11-16, for review with its pull request): Inbox's record of updates. Four request channels without free
  // text: `inbox:updates` (no argument), `inbox:markRead` (an update id of 32 hex characters, validated by main, which moves
  // the Core's own read marker for that update's task through the existing MarkRead), `inbox:markAllRead` and `inbox:clear`
  // (no argument). Two events it hears: `inbox:changed` (no payload) and `notify:open` (a task id the preload re-validates as
  // 32 hex characters, or null), sent by main's click handler on a notification the OS showed. No renderer call can add an
  // entry to the record: main notes entries from the Core's headers only.
  // PX-207 (2026-11-18, for review with its pull request): no new channel and no new method. Two type changes: the context
  // inspector's answer gains `instructions`, the instruction layers the Core already returns (ContextInspectorView field 180,
  // REQ-PX-107), passed through as plain data; and `customizeOpenSource` accepts the kind `rule`, for which main shows a file
  // only if the Core recorded that source as loaded for the task, a relative source resolved inside the task's folder as the
  // Core holds it (customize-ipc.ts `rulePath`). The renderer still names no path of its own.
  // PX-214 (2026-11-19, for review with its pull request): one channel pair and no other change. `review:fixFinding` (a session id, a
  // task id, a finding id of at most 48 plain characters and the 64-hex digest of the reviewer's result the person was looking at,
  // all validated by main) forwards the one typed Core command FixReviewFinding, which queues one input naming that finding; and
  // `review:dismissFinding` (the same four and a reason of 1..500 characters) forwards DismissReviewFinding, which records who and
  // why. Both take the session's lease and return plain data. The findings themselves ride the existing `review:bundle` channel
  // (a type-only addition of the reviewer's fields). No path, command name or free text beyond that reason crosses the bridge;
  // neither channel is on a pane window's allow list (pane-windows.ts), so only the app's window may fix or dismiss a finding.
  preloadSha256: "3148ef2c63d05e0799edecfc55892843b978d4ea0ea390faf30c21ffa94d2d2a",
  cspSha256: "e8c3fcc4286ba170b02ea95b02189f132b16228341d2002412facfec6eb929d1",
  webPreferencesSha256: "28515878dc43e119f2360de779e5a3733283d120b31a113837f7f7a6cf2356ee",
  windowOptionKeys: ["width", "height", "minWidth", "minHeight", "show", "webPreferences"],
};

function windowOptions(): string {
  const start = mainSource.indexOf("new BrowserWindow({");
  assert.ok(start >= 0, "main.ts constructs the window");
  const end = mainSource.indexOf("\n  });", start);
  return mainSource.slice(start, end);
}

test("PX-049: the preload bridge is byte-identical to the pinned one", () => {
  const bytes = readFileSync(join(here, "..", "preload", "preload.ts"));
  assert.equal(sha256(bytes), PINS.preloadSha256, "preload.ts changed: a new bridge surface is a security-boundary change (update the pin only with its review)");
});

test("PX-049: the Content-Security-Policy is the pinned one", () => {
  const m = /"Content-Security-Policy": \["([^"]+)"\]/.exec(mainSource);
  assert.ok(m, "main.ts sets the Content-Security-Policy header");
  assert.equal(sha256(m[1]!), PINS.cspSha256, `the policy changed: ${m[1]}`);
  assert.match(m[1]!, /default-src 'self'/);
  assert.match(m[1]!, /connect-src 'none'/);
  assert.doesNotMatch(m[1]!, /unsafe-eval|script-src[^;]*unsafe-inline/, "scripts stay same-origin and never eval");
});

test("PX-049: webPreferences is the pinned block (isolated, sandboxed, no Node)", () => {
  const m = /webPreferences: \{[\s\S]*?\n    \},/.exec(windowOptions());
  assert.ok(m, "the window sets webPreferences");
  assert.equal(sha256(m[0]), PINS.webPreferencesSha256, `webPreferences changed:\n${m[0]}`);
  for (const must of ["contextIsolation: true", "nodeIntegration: false", "sandbox: true", "webSecurity: true"]) assert.ok(m[0].includes(must), must);
});

test("PX-049: the window constructor takes the size options and nothing else beside webPreferences", () => {
  const body = windowOptions();
  const keys = [...body.matchAll(/^ {4}([A-Za-z]+)[:,]/gm)].map((k) => k[1]!);
  // `...(bounds ?? WINDOW_DEFAULT)` supplies width and height; they are listed as the keys the spread provides.
  const all = body.includes("...(bounds ?? WINDOW_DEFAULT)") ? ["width", "height", ...keys] : keys;
  assert.deepEqual(
    all.filter((k, i, a) => a.indexOf(k) === i),
    PINS.windowOptionKeys,
    "an option other than the window size, visibility and webPreferences was added to the constructor",
  );
});

/**
 * REQ-PX-203 / QUAL-PX-203: a pane window is made by the same factory as the app's window. Read from main.ts: one constructor,
 * one webPreferences block, one page; the navigation and new-window refusals are registered for every window before the factory
 * tells the roles apart; the constructor's spreads are the known ones (a spread could otherwise carry any option past the key
 * pin above); and every window, the app's and each pane's, comes from that factory. Empty when it holds.
 */
const KNOWN_SPREADS = ["...(bounds ?? WINDOW_DEFAULT)", '...(process.platform === "darwin" && role.kind === "app" ? { titleBarStyle: "hiddenInset" as const, trafficLightPosition: { x: 14, y: 13 } } : {})', '...(role.kind === "pane" ? { title: role.title } : {})'];
export function windowFactoryDrift(source: string): string[] {
  // A Windows checkout may carry CRLF; the guard reads the LF form, as the pins above do.
  const src = source.replace(/\r\n/g, "\n");
  const bad: string[] = [];
  const count = (re: RegExp) => [...src.matchAll(re)].length;
  if (count(/new BrowserWindow\(/g) !== 1) bad.push(`main.ts constructs a window in ${count(/new BrowserWindow\(/g)} places, not one`);
  if (count(/webPreferences\s*:/g) !== 1) bad.push(`main.ts has ${count(/webPreferences\s*:/g)} webPreferences blocks, not one`);
  const start = src.indexOf("function createAppWindow(");
  if (start < 0) return [...bad, "main.ts has no window factory"];
  const end = src.indexOf("\n}\n", start);
  const body = src.slice(start, end);
  if (!body.includes("new BrowserWindow(")) bad.push("the window is not constructed in the factory");
  const branch = body.indexOf('if (role.kind === "pane")');
  for (const [what, needle] of [
    ["navigation is refused", 'w.webContents.on("will-navigate", (e) => e.preventDefault());'],
    ["new windows are refused", 'w.webContents.setWindowOpenHandler(() => ({ action: "deny" }));'],
  ] as const) {
    const at = body.indexOf(needle);
    if (at < 0) bad.push(`${what}: not in the factory`);
    else if (branch >= 0 && at > branch) bad.push(`${what}: only for some windows (after the roles are told apart)`);
  }
  const ctor = body.slice(body.indexOf("new BrowserWindow({"), body.indexOf("\n  });", body.indexOf("new BrowserWindow({")));
  const spreads = [...ctor.matchAll(/^ {4}(\.\.\..*?),?$/gm)].map((m) => m[1]!.replace(/,$/, ""));
  for (const sp of spreads) if (!KNOWN_SPREADS.includes(sp)) bad.push(`an unknown spread in the window's options: ${sp}`);
  const pages = [...body.matchAll(/w\.load(File|URL)\(([^;]*)\);/g)].map((m) => `${m[1]}:${m[2]!.replace(/, \{ hash: "pane" \}/, "")}`);
  if (pages.length === 0 || new Set(pages).size !== 1 || !pages[0]!.startsWith('File:join(__dirname, "..", "renderer", "index.html")')) bad.push(`the windows do not all load the app's own page: ${pages.join(" | ")}`);
  if (!/createAppWindow\(\{ kind: "pane",/.test(src)) bad.push("a pane window is not made by the factory");
  if (count(/createAppWindow\(\{ kind: "app" \}/g) < 2) bad.push("the app's window (and its replacement after a crash) is not made by the factory");
  if (/partition\s*:|session\s*:\s*session\./.test(ctor)) bad.push("a window names its own session or partition (the policy is the default session's)");
  if (!/session\.defaultSession\.webRequest\.onHeadersReceived\(/.test(src)) bad.push("the Content-Security-Policy is not set on the default session");
  return bad;
}

test("PX-203: every window, the app's and each pane's, comes from the one factory with the one set of web preferences", () => {
  assert.deepEqual(windowFactoryDrift(mainSource), []);
});

test("PX-203: the factory guard fails on a looser window (retained mutations)", () => {
  const lf = mainSource.replace(/\r\n/g, "\n");
  const mutate = (from: string, to: string): string => {
    assert.ok(lf.includes(from), `the mutation's anchor is in main.ts: ${from.slice(0, 60)}`);
    return lf.replace(from, to);
  };
  // A second constructor for pane windows with Node in the page.
  const second = mutate('    void w.loadFile(join(__dirname, "..", "renderer", "index.html"), { hash: "pane" });', '    const loose = new BrowserWindow({ webPreferences: { nodeIntegration: true, contextIsolation: false } });\n    void loose.loadFile(join(__dirname, "..", "renderer", "index.html"), { hash: "pane" });');
  assert.ok(windowFactoryDrift(second).some((d) => d.includes("constructs a window in 2 places")));
  assert.ok(windowFactoryDrift(second).some((d) => d.includes("webPreferences blocks")));
  // Options for pane windows only, carried by a spread past the key pin.
  const spread = mutate('    ...(role.kind === "pane" ? { title: role.title } : {}),', '    ...(role.kind === "pane" ? { title: role.title, webPreferences: { sandbox: false } } : {}),');
  assert.ok(windowFactoryDrift(spread).some((d) => d.includes("unknown spread")));
  // Navigation refused for the app's window only.
  const nav = mutate('  w.webContents.on("will-navigate", (e) => e.preventDefault());\n', "").replace("  win = w;\n", '  win = w;\n  w.webContents.on("will-navigate", (e) => e.preventDefault());\n');
  assert.ok(windowFactoryDrift(nav).some((d) => d.startsWith("navigation is refused: only for some windows")));
  // A pane window that loads some other page.
  const page = mutate('void w.loadFile(join(__dirname, "..", "renderer", "index.html"), { hash: "pane" });', 'void w.loadURL("https://example.invalid/pane");');
  assert.ok(windowFactoryDrift(page).some((d) => d.includes("do not all load the app's own page")));
  // A pane window in a session of its own, where the policy header is not set.
  const part = mutate("    show: true,\n    webPreferences: {", "    show: true,\n    webPreferences: {\n      partition: \"pane\",");
  assert.ok(windowFactoryDrift(part).some((d) => d.includes("its own session or partition")));
});
