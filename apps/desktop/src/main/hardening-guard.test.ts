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
  preloadSha256: "850a085e967ba2312747dfc4dca7e8ed1119281bfb1dd3bfb9cc7a45c2be696e",
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
