/**
 * M10.2 (docs/36 license gate, docs/70 SBOM): the policy engine and the SBOM
 * check in the release tool, run as the real CLI over SBOM documents. The
 * generation of real SBOMs from cargo and pnpm is proven in CI's supply-chain job,
 * which has the generators installed.
 */
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "..", "..");
const cli = join(repoRoot, "tools", "release", "release.mjs");

function run(args: string[], env: Record<string, string> = {}) {
  const r = spawnSync(process.execPath, [cli, ...args], { encoding: "utf8", env: { ...process.env, MODBIT_UPDATE_SIGNING_KEY: "", ...env } });
  return { code: r.status, out: r.stdout, err: r.stderr };
}

type Comp = { name: string; version: string; licenses?: unknown[] };
const lic = (id: string) => [{ license: { id } }];
const expr = (e: string) => [{ expression: e }];
const sbom = (dir: string, name: string, components: Comp[]) => {
  const p = join(dir, name);
  writeFileSync(p, JSON.stringify({ bomFormat: "CycloneDX", specVersion: "1.5", components }));
  return p;
};

test("the license gate passes permissive and OR-alternatives, and fails copyleft, unknown and unlicensed third-party components", () => {
  const dir = mkdtempSync(join(tmpdir(), "modbit-sbom-"));
  try {
    const ok = sbom(dir, "ok.json", [
      { name: "serde", version: "1.0.0", licenses: expr("MIT OR Apache-2.0") },
      { name: "ring-like", version: "1.0.0", licenses: expr("Apache-2.0 AND ISC") },
      { name: "choice", version: "1.0.0", licenses: expr("GPL-3.0-only OR MIT") }, // MIT is available: allowed
      { name: "modbit-domain", version: "0.0.0", licenses: lic("UNLICENSED") }, // first party, recognized from the workspace
    ]);
    assert.equal(run(["licenses", ok]).code, 0, run(["licenses", ok]).err);

    for (const [name, licenses, expected] of [
      ["gpl", lic("GPL-3.0-only"), /GPL-3\.0-only/],
      ["agpl-and", expr("MIT AND AGPL-3.0-only"), /AGPL-3\.0-only/],
      ["unknown", lic("LicenseRef-vendor-eula"), /LicenseRef-vendor-eula/],
      ["third-party-unlicensed", lic("UNLICENSED"), /UNLICENSED/],
      ["undeclared", undefined, /declares no license/],
    ] as const) {
      const bad = sbom(dir, `${name}.json`, [{ name: `pkg-${name}`, version: "1.0.0", ...(licenses ? { licenses } : {}) }]);
      const r = run(["licenses", bad]);
      assert.notEqual(r.code, 0, `${name} must fail`);
      assert.match(r.err, expected, name);
      assert.match(r.err, new RegExp(`pkg-${name}`));
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("an SBOM with no components, a non-CycloneDX document, or an unnamed component is refused", () => {
  const dir = mkdtempSync(join(tmpdir(), "modbit-sbom-"));
  try {
    assert.match(run(["licenses", sbom(dir, "empty.json", [])]).err, /lists no components/);
    writeFileSync(join(dir, "spdx.json"), JSON.stringify({ spdxVersion: "SPDX-2.3", components: [{ name: "x", version: "1" }] }));
    assert.match(run(["licenses", join(dir, "spdx.json")]).err, /not a CycloneDX document/);
    assert.match(run(["licenses", sbom(dir, "unnamed.json", [{ name: "", version: "1" }])]).err, /without a name and version/);
    writeFileSync(join(dir, "junk.json"), "{{");
    assert.match(run(["licenses", join(dir, "junk.json")]).err, /not JSON/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("verify checks every SBOM against the digest the signed manifest carries, so a swapped SBOM fails", () => {
  const dir = mkdtempSync(join(tmpdir(), "modbit-sbom-"));
  try {
    const keys = join(dir, "keys.json");
    const priv = join(dir, "secret", "key.pem");
    assert.equal(run(["keygen", "--public-out", keys, "--private-out", priv]).code, 0);
    const art = join(dir, "app.zip");
    writeFileSync(art, "artifact bytes");
    const sbomDir = join(dir, "sboms");
    mkdirSync(sbomDir);
    const good = sbom(sbomDir, "core.cdx.json", [{ name: "serde", version: "1.0.0", licenses: lic("MIT") }]);
    const manifest = join(dir, "manifest.json");
    const made = run(
      ["manifest", "--version", "1.0.0", "--artifact", `macos:arm64:https://feed.example.test/app.zip|${art}`, "--sbom", good, "--out", manifest],
      { MODBIT_UPDATE_SIGNING_KEY: readFileSync(priv, "utf8") },
    );
    assert.equal(made.code, 0, made.err);

    const ok = run(["verify", manifest, "--keys", keys, "--sbom-dir", sbomDir]);
    assert.equal(ok.code, 0, ok.err);
    assert.match(ok.out, /1 SBOMs match their signed digests/);

    // The same name with different content: the manifest's signed digest no longer matches.
    sbom(sbomDir, "core.cdx.json", [{ name: "evil", version: "6.6.6", licenses: lic("MIT") }]);
    const swapped = run(["verify", manifest, "--keys", keys, "--sbom-dir", sbomDir]);
    assert.notEqual(swapped.code, 0);
    assert.match(swapped.err, /does not match the digest the manifest signs/);

    // A listed SBOM that is missing is a failure, not a skip.
    rmSync(join(sbomDir, "core.cdx.json"));
    assert.match(run(["verify", manifest, "--keys", keys, "--sbom-dir", sbomDir]).err, /missing from/);

    // A manifest that lists no SBOM cannot satisfy --sbom-dir.
    const bare = join(dir, "bare.json");
    assert.equal(run(["manifest", "--version", "1.0.0", "--artifact", `macos:arm64:https://feed.example.test/app.zip|${art}`, "--out", bare], { MODBIT_UPDATE_SIGNING_KEY: readFileSync(priv, "utf8") }).code, 0);
    assert.match(run(["verify", bare, "--keys", keys, "--sbom-dir", sbomDir]).err, /lists no SBOM/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
