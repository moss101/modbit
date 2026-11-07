// Writes the two files a package pins about its own updates (M10.2): where the
// feed is, and which public keys may sign what the app installs. With no
// MODBIT_UPDATE_KEYS_FILE the key list is empty and the packaged app verifies
// nothing and refuses every update (fail closed); the private key is never
// here, only the public half the owner generated with `release.mjs keygen`.
//
// It also writes build-info.json, which says what this package is. Every
// package this script prepares is UNSIGNED: no code-signing identity and no
// notarization (both are the owner's, docs/77 section 5 item 4). It is for
// tests only (packaged E2E, render budgets, SBOM), never a release candidate.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";

const out = "package-resources";
mkdirSync(out, { recursive: true });
const keysFile = process.env.MODBIT_UPDATE_KEYS_FILE;
if (keysFile) {
  const { keys } = JSON.parse(readFileSync(keysFile, "utf8"));
  if (!Array.isArray(keys) || keys.some((k) => typeof k?.key_id !== "string" || typeof k?.public_key_pem !== "string" || "private_key_pem" in k)) {
    throw new Error(`${keysFile} is not {keys: [{key_id, public_key_pem}]} (and must hold no private key)`);
  }
  copyFileSync(keysFile, `${out}/update-keys.json`);
} else {
  writeFileSync(`${out}/update-keys.json`, '{ "keys": [] }\n');
}
const config = { channel: process.env.MODBIT_UPDATE_CHANNEL ?? "stable", ...(process.env.MODBIT_UPDATE_FEED_URL ? { feed_url: process.env.MODBIT_UPDATE_FEED_URL } : {}) };
writeFileSync(`${out}/update-config.json`, `${JSON.stringify(config, null, 2)}\n`);

function commitOf() {
  if (process.env.GITHUB_SHA) return process.env.GITHUB_SHA;
  try {
    return execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).trim();
  } catch {
    return "unknown";
  }
}
const pkg = JSON.parse(readFileSync("package.json", "utf8"));
const buildInfo = {
  label: "UNSIGNED",
  signed: false,
  notarized: false,
  purpose: "tests only: packaged E2E, render budgets and SBOM; not a release candidate",
  version: pkg.version,
  electron: pkg.devDependencies?.electron ?? null,
  commit: commitOf(),
  update_keys_pinned: JSON.parse(readFileSync(`${out}/update-keys.json`, "utf8")).keys.length,
};
writeFileSync(`${out}/build-info.json`, `${JSON.stringify(buildInfo, null, 2)}\n`);
console.log(`package-resources written (UNSIGNED; ${keysFile ? "keys pinned from " + keysFile : "no keys pinned: updates fail closed"})`);
