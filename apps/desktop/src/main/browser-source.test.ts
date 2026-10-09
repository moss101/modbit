import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const src = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "browser.ts"), "utf8");

test("PX-073: every DevTools-protocol command of the host goes through the one funnel that applies the deny list", () => {
  const direct = src.match(/\.sendCommand\(/g) ?? [];
  assert.equal(direct.length, 2, "only the funnel's two forms (with and without a session) call the debugger");
  const funnel = src.slice(src.indexOf("private async send("), src.indexOf("/** Attach the DevTools bridge"));
  assert.ok(funnel.includes("guardCdp(method"), "the funnel guards before it sends");
  assert.equal((funnel.match(/\.sendCommand\(/g) ?? []).length, 2);
  // Input and Target are reached only with the structural flag.
  for (const m of src.matchAll(/this\.send\(h, "(Input\.[A-Za-z]+|Target\.[A-Za-z]+)"[^;]*\);/g)) assert.ok(m[0].includes("structural: true"), m[0]);
});

test("PX-073: navigation never takes focus - the host never focuses the page's web contents", () => {
  assert.ok(!/webContents\.focus\(\)|view\.focus\(\)/.test(src));
});
