#!/usr/bin/env node
// M10.2: gather what one UNSIGNED desktop package build produced into one
// directory a CI job can upload, and refuse anything that could pass for a
// release. Reads the package's own build-info.json (written by
// apps/desktop/prepare-package.mjs) and fails when it says signed, when an
// artifact name lacks "UNSIGNED", or when nothing was built. Writes:
//   <out>/<artifact>-UNSIGNED.<ext>   the distributables electron-builder made
//   <out>/build-info.json             what the package says it is
//   <out>/modbit-desktop-<platform>-<arch>-UNSIGNED.package.cdx.json
//                                     the package SBOM (release.mjs package-sbom)
//   <out>/SHA256SUMS-UNSIGNED         sha256 of every file above
//
//   node tools/release/unsigned-bundle.mjs --out target/unsigned [--sbom-dir target/sbom]
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { packagedExecutable } from "../../apps/desktop/e2e/packaged-app.ts";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const releaseDir = join(repoRoot, "apps", "desktop", "release");
const args = process.argv.slice(2);
const flag = (name) => (args.includes(`--${name}`) ? args[args.indexOf(`--${name}`) + 1] : undefined);
const fail = (m) => {
  console.error(`unsigned-bundle: ${m}`);
  process.exit(1);
};
const out = flag("out");
if (!out) fail("--out is required");
const outDir = resolve(out);
mkdirSync(outDir, { recursive: true });

const { exe, resources } = packagedExecutable(releaseDir);
const info = JSON.parse(readFileSync(join(resources, "build-info.json"), "utf8"));
if (info.signed !== false || info.label !== "UNSIGNED") fail(`the package says label=${info.label} signed=${info.signed}; this tool gathers UNSIGNED test builds only`);

const DISTRIBUTABLE = /\.(dmg|zip|exe|AppImage|tar\.gz)$/;
const built = readdirSync(releaseDir).filter((n) => statSync(join(releaseDir, n)).isFile() && DISTRIBUTABLE.test(n));
if (built.length === 0) fail(`no distributable in ${releaseDir}; run \`pnpm --filter @modbit/desktop package\` first`);
const unlabelled = built.filter((n) => !n.includes("UNSIGNED"));
if (unlabelled.length > 0) fail(`artifact names without UNSIGNED: ${unlabelled.join(", ")}`);
for (const n of built) copyFileSync(join(releaseDir, n), join(outDir, n));
copyFileSync(join(resources, "build-info.json"), join(outDir, "build-info.json"));

const tag = `${process.platform === "darwin" ? "mac" : process.platform === "win32" ? "win" : "linux"}-${process.arch}`;
const sbom = join(outDir, `modbit-desktop-${info.version}-${tag}-UNSIGNED.package.cdx.json`);
const release = join(repoRoot, "tools", "release", "release.mjs");
const sbomArgs = [release, "package-sbom", "--resources", resources, "--executable", exe, "--out", sbom, ...built.flatMap((n) => ["--artifact", join(outDir, n)])];
if (flag("sbom-dir")) sbomArgs.push("--sbom-dir", resolve(flag("sbom-dir")));
const run = (a) => {
  const r = spawnSync(process.execPath, a, { stdio: "inherit" });
  if (r.status !== 0) fail(`node ${a.slice(0, 2).join(" ")} exited ${r.status}`);
};
run(sbomArgs);
run([release, "checksums", outDir, "--out", "SHA256SUMS-UNSIGNED"]);
if (!existsSync(join(outDir, "SHA256SUMS-UNSIGNED"))) fail("no checksums written");
console.log(`unsigned-bundle: ${built.length} UNSIGNED artifact(s), the package SBOM and SHA256SUMS-UNSIGNED in ${outDir}`);
