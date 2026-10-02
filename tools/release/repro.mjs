#!/usr/bin/env node
// M10.2, docs/70 "Build reproducibility": the same inputs produce the same
// artifact digests twice. Builds the shipped Rust binaries and the desktop
// bundles two times, each from a clean target directory with identical pinned
// inputs (the locked dependency set, SOURCE_DATE_EPOCH from the commit, path
// prefixes remapped), compares sha256 and writes the evidence. Exit 1 when any
// artifact differs, 2 when the evidence itself cannot be produced.
//
//   node tools/release/repro.mjs --out evidence.json [--work-dir dir]
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const args = process.argv.slice(2);
const flag = (name) => (args.includes(`--${name}`) ? args[args.indexOf(`--${name}`) + 1] : undefined);
const outFile = flag("out");
if (!outFile) {
  console.error("repro: --out is required");
  process.exit(2);
}

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const exe = (n) => (process.platform === "win32" ? `${n}.exe` : n);

function run(cmd, cmdArgs, opts = {}) {
  const r = spawnSync(cmd, cmdArgs, { encoding: "utf8", maxBuffer: 256 * 1024 * 1024, ...opts });
  if (r.error || r.status !== 0) {
    console.error(`repro: ${cmd} ${cmdArgs.join(" ")} failed\n${r.error?.message ?? r.stderr}`);
    process.exit(2);
  }
  return r.stdout.trim();
}

function walk(dir) {
  const out = [];
  for (const n of readdirSync(dir).sort()) {
    const p = join(dir, n);
    if (statSync(p).isDirectory()) out.push(...walk(p));
    else out.push(p);
  }
  return out;
}

/** The committed state being built: a dirty tree is not an input anyone can name. */
const commit = run("git", ["rev-parse", "HEAD"], { cwd: repoRoot });
const dirty = run("git", ["status", "--porcelain", "--untracked-files=no"], { cwd: repoRoot });
const epoch = run("git", ["log", "-1", "--format=%ct"], { cwd: repoRoot });
const cargoHome = process.env.CARGO_HOME ?? join(homedir(), ".cargo");
const work = flag("work-dir") ? resolve(flag("work-dir")) : mkdtempSync(join(tmpdir(), "modbit-repro-"));
mkdirSync(work, { recursive: true });

const buildEnv = {
  ...process.env,
  SOURCE_DATE_EPOCH: epoch,
  // Paths of the checkout and of the cargo registry must not reach the binary.
  RUSTFLAGS: `--remap-path-prefix=${repoRoot}=/modbit-src --remap-path-prefix=${cargoHome}=/cargo ${process.env.RUSTFLAGS ?? ""}`.trim(),
  CARGO_INCREMENTAL: "0",
};

const RUST = [
  { name: "modbit-core", file: exe("modbit-core") },
  { name: "modbit-execd", file: exe("modbit-execd") },
];
const runs = [];
for (const label of ["a", "b"]) {
  const target = join(work, `target-${label}`);
  rmSync(target, { recursive: true, force: true });
  console.error(`repro: build ${label}: cargo build --release --locked (clean target ${target})`);
  run("cargo", ["build", "--release", "--locked", "-p", "modbit-core", "-p", "modbit-execd"], { cwd: repoRoot, env: { ...buildEnv, CARGO_TARGET_DIR: target } });
  const digests = {};
  for (const r of RUST) digests[`rust/${r.name}`] = sha256(readFileSync(join(target, "release", r.file)));

  // The desktop bundles, built into a scratch copy of the app so the two runs share no output.
  const appCopy = join(work, `desktop-${label}`);
  rmSync(appCopy, { recursive: true, force: true });
  cpSync(join(repoRoot, "apps", "desktop"), appCopy, {
    recursive: true,
    filter: (src) => !/[\\/](node_modules|dist|release|package-resources|test-results)([\\/]|$)/.test(relative(join(repoRoot, "apps", "desktop"), src)),
  });
  // Dependencies resolve through the repository's pnpm install; the copy sees them by symlink.
  run("node", ["--no-warnings", "-e", `require("node:fs").symlinkSync(${JSON.stringify(join(repoRoot, "apps", "desktop", "node_modules"))}, ${JSON.stringify(join(appCopy, "node_modules"))}, "dir")`]);
  run("node", ["build.mjs"], { cwd: appCopy, env: buildEnv });
  const dist = join(appCopy, "dist");
  for (const f of walk(dist)) {
    // The source maps name their sources relative to the build directory; the digest of the maps is of the bundle's content, which is what we compare.
    digests[`desktop/${relative(dist, f).split(sep).join("/")}`] = sha256(readFileSync(f));
  }
  runs.push(digests);
}

const names = [...new Set([...Object.keys(runs[0]), ...Object.keys(runs[1])])].sort();
const artifacts = names.map((name) => ({ name, a: runs[0][name] ?? null, b: runs[1][name] ?? null, equal: runs[0][name] !== undefined && runs[0][name] === runs[1][name] }));
const evidence = {
  kind: "reproducible-build",
  commit,
  tree_dirty: dirty.length > 0,
  inputs: {
    source_date_epoch: epoch,
    rustc: run("rustc", ["-Vv"], { cwd: repoRoot }).split("\n").slice(0, 3).join("; "),
    node: process.version,
    cargo_lock_sha256: sha256(readFileSync(join(repoRoot, "Cargo.lock"))),
    pnpm_lock_sha256: sha256(readFileSync(join(repoRoot, "pnpm-lock.yaml"))),
    platform: `${process.platform}-${process.arch}`,
  },
  artifacts,
  reproducible: artifacts.every((a) => a.equal),
};
mkdirSync(dirname(resolve(outFile)), { recursive: true });
writeFileSync(outFile, `${JSON.stringify(evidence, null, 2)}\n`);
for (const a of artifacts) console.log(`${a.equal ? "same     " : "DIFFERENT"} ${a.name} ${a.a?.slice(0, 16)} ${a.b?.slice(0, 16)}`);
console.log(`repro: ${evidence.reproducible ? "REPRODUCIBLE" : "NOT REPRODUCIBLE"} (${artifacts.length} artifacts)${evidence.tree_dirty ? "; the working tree has uncommitted changes, so this evidence names no commit" : ""}; evidence in ${outFile}`);
if (!existsSync(outFile)) process.exit(2);
process.exit(evidence.reproducible ? 0 : 1);
