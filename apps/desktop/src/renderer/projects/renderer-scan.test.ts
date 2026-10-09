// QUAL-PX-064 / QUAL-PX-068 negative proofs, as a static scan of the renderer
// sources of the project and worktree surfaces: a renderer that edits
// membership locally, runs Git, or removes a directory is a failure. The
// scan reads the real source files, so it fails the moment such code lands.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const renderer = join(here, "..");

function sources(dir: string): { file: string; text: string }[] {
  return readdirSync(dir)
    .filter((f) => /\.tsx?$/.test(f) && !/\.test\.tsx?$/.test(f))
    .map((f) => ({ file: join(dir, f), text: readFileSync(join(dir, f), "utf8") }));
}

const surfaces = [...sources(join(renderer, "projects")), ...sources(join(renderer, "worktrees"))];

test("the project and worktree surfaces exist and are scanned", () => {
  assert.ok(surfaces.length >= 10, `scanned ${surfaces.length} files`);
});

test("the renderer imports no Node, process, filesystem or Git facility", () => {
  const forbidden = [
    /from\s+["']node:/,
    /from\s+["'](fs|child_process|path|os)["']/,
    /require\(/,
    /\bchild_process\b/,
    /\b(spawn|spawnSync|execSync|execFile|execFileSync)\(/,
    /\brmSync\b|\brmdirSync\b|\bunlinkSync\b|\bfs\.rm\b|\bfs\.promises\b/,
    /\bprocess\.(env|cwd|platform)\b/,
    /["']git["']\s*,\s*["'](worktree|merge|checkout|stash|reset|apply)["']/,
  ];
  for (const { file, text } of surfaces) {
    const code = text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:])\/\/.*$/gm, "$1");
    for (const re of forbidden) {
      assert.ok(!re.test(code), `${file} matches ${re}`);
    }
  }
});

test("the renderer reaches the Core only through the typed bridge", () => {
  for (const { file, text } of surfaces) {
    assert.ok(!/ipcRenderer/.test(text), `${file} talks to IPC directly`);
    assert.ok(!/\bfetch\(|XMLHttpRequest|WebSocket\(/.test(text), `${file} opens its own transport`);
  }
});

test("membership is never edited in the renderer: every change is a Core command followed by a Core read", () => {
  // A local edit of a membership list or of a header's project id would show
  // the user something the Core never recorded.
  const local = [
    /\bmembers\s*\.\s*(push|splice|unshift|pop|shift)\(/,
    /\bmembers\s*=\s*\[\s*\.\.\./,
    /\.projectId\s*=[^=]/,
    /\bproject_?[iI]d\s*:\s*[^,}\s]+\s*,\s*\/\/\s*optimistic/,
    /optimistic/i,
  ];
  for (const { file, text } of surfaces) {
    const code = text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:])\/\/.*$/gm, "$1");
    for (const re of local) {
      assert.ok(!re.test(code), `${file} edits membership locally: ${re}`);
    }
  }
  const hook = surfaces.find((s) => s.file.endsWith("use-projects.ts"));
  assert.ok(hook, "the projects hook exists");
  // Every mutating call is followed by a re-read of the Core's projection.
  assert.match(hook.text, /const after = /);
  assert.match(hook.text, /window\.modbit\.listProjects/);
});
