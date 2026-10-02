import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { decideUpdate, verifyManifest, type InstalledState } from "./manifest.ts";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..", "..", "..", "..", "..");
const cli = join(repoRoot, "tools", "release", "release.mjs");

function run(args: string[], env: Record<string, string> = {}) {
  const r = spawnSync(process.execPath, [cli, ...args], { encoding: "utf8", env: { ...process.env, MODBIT_UPDATE_SIGNING_KEY: "", ...env } });
  return { code: r.status, out: r.stdout, err: r.stderr };
}

function scratch() {
  return mkdtempSync(join(tmpdir(), "modbit-release-"));
}

test("release tooling signs with a key that never enters the repository, and what it signs the app-side module accepts (M10.2)", () => {
  const dir = scratch();
  try {
    // the private key is refused inside the repository
    const inRepo = run(["keygen", "--public-out", join(dir, "keys.json"), "--private-out", join(repoRoot, "leaked-update-key.pem")]);
    assert.notEqual(inRepo.code, 0);
    assert.match(inRepo.err, /inside the repository/);

    const publicOut = join(dir, "update-keys.json");
    const privateOut = join(dir, "secret", "update-key.pem");
    const gen = run(["keygen", "--public-out", publicOut, "--private-out", privateOut]);
    assert.equal(gen.code, 0, gen.err);
    if (process.platform !== "win32") assert.equal(statSync(privateOut).mode & 0o777, 0o600, "the private key file is owner-only");
    // an existing key is never overwritten
    assert.notEqual(run(["keygen", "--public-out", publicOut, "--private-out", privateOut]).code, 0);

    const artifacts = join(dir, "dist");
    mkdirSync(artifacts);
    const bytes = Buffer.from("packaged modbit desktop, macos arm64");
    writeFileSync(join(artifacts, "modbit-0.2.0-macos-arm64.zip"), bytes);
    const sbom = join(dir, "modbit-core.cdx.json");
    writeFileSync(sbom, JSON.stringify({ bomFormat: "CycloneDX", specVersion: "1.6", components: [] }));

    const manifestPath = join(dir, "manifest.json");
    const build = [
      "manifest", "--version", "0.2.0", "--channel", "stable", "--rollout-percent", "25", "--critical-migration",
      "--artifact", `macos:arm64:https://updates.example.test/modbit-0.2.0-macos-arm64.zip|${join(artifacts, "modbit-0.2.0-macos-arm64.zip")}`,
      "--sbom", sbom, "--out", manifestPath,
    ];
    // no signing key, no manifest
    const noKey = run(build);
    assert.notEqual(noKey.code, 0);
    assert.match(noKey.err, /no signing key/);

    const built = run(build, { MODBIT_UPDATE_SIGNING_KEY: readFileSync(privateOut, "utf8") });
    assert.equal(built.code, 0, built.err);

    const ok = run(["verify", manifestPath, "--keys", publicOut, "--artifact-dir", artifacts]);
    assert.equal(ok.code, 0, ok.err);
    assert.match(ok.out, /verifies; 1 artifacts match/);

    // the app-side module reads the same file and the same pinned keys the tool wrote
    const pinned = JSON.parse(readFileSync(publicOut, "utf8")).keys;
    const payload = verifyManifest(JSON.parse(readFileSync(manifestPath, "utf8")), pinned);
    assert.equal(payload.version, "0.2.0");
    assert.equal(payload.rollout_percent, 25);
    assert.equal(payload.critical_migration, true);
    assert.equal(payload.artifacts[0]!.sha256, createHash("sha256").update(bytes).digest("hex"));
    assert.equal(payload.sbom[0]!.name, "modbit-core.cdx.json");
    assert.equal(payload.schema_version, Number(run(["schema-version"]).out.trim()), "the manifest carries the schema version of this tree");
    const now: InstalledState = { version: "0.1.0", platform: "macos", arch: "arm64", channel: "stable", dbSchemaVersion: payload.schema_version - 1, installId: "x" };
    assert.equal(decideUpdate({ ...now, installId: "always-admitted" }, { ...payload, rollout_percent: 100 }).action, "install");

    // a tampered artifact, a tampered manifest and an unpinned key each fail verification with their own code
    writeFileSync(join(artifacts, "modbit-0.2.0-macos-arm64.zip"), Buffer.concat([bytes, Buffer.from("!")]));
    const badArtifact = run(["verify", manifestPath, "--keys", publicOut, "--artifact-dir", artifacts]);
    assert.notEqual(badArtifact.code, 0);
    assert.match(badArtifact.err, /ARTIFACT_SIZE_MISMATCH/);
    writeFileSync(join(artifacts, "modbit-0.2.0-macos-arm64.zip"), Buffer.from("packaged modbit desktop, macos arm64".replace("arm64", "arm65")));
    assert.match(run(["verify", manifestPath, "--keys", publicOut, "--artifact-dir", artifacts]).err, /ARTIFACT_DIGEST_MISMATCH/);

    const signed = JSON.parse(readFileSync(manifestPath, "utf8"));
    writeFileSync(join(dir, "edited.json"), JSON.stringify({ ...signed, payload: signed.payload.replace("0.2.0", "0.9.0") }));
    assert.match(run(["verify", join(dir, "edited.json"), "--keys", publicOut]).err, /SIGNATURE_INVALID/);

    const otherKeys = join(dir, "other-keys.json");
    assert.equal(run(["keygen", "--public-out", otherKeys, "--private-out", join(dir, "secret", "other.pem")]).code, 0);
    assert.match(run(["verify", manifestPath, "--keys", otherKeys]).err, /SIGNATURE_UNKNOWN_KEY/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("checksums are sha256sum-format lines over every file, sorted, without the sums file itself", () => {
  const dir = scratch();
  try {
    mkdirSync(join(dir, "sub"));
    writeFileSync(join(dir, "b.bin"), "b");
    writeFileSync(join(dir, "sub", "a.bin"), "a");
    const r = run(["checksums", dir]);
    assert.equal(r.code, 0, r.err);
    const sums = readFileSync(join(dir, "SHA256SUMS"), "utf8").trimEnd().split("\n");
    const digest = (s: string) => createHash("sha256").update(s).digest("hex");
    assert.deepEqual(sums, [`${digest("b")}  b.bin`, `${digest("a")}  sub/a.bin`]);
    // a second run over the same tree is byte-identical: the sums file does not sum itself
    assert.equal(run(["checksums", dir]).code, 0);
    assert.deepEqual(readFileSync(join(dir, "SHA256SUMS"), "utf8").trimEnd().split("\n"), sums);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
