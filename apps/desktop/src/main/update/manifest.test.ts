import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  compareVersions, decideUpdate, generateUpdateKeyPair, keyIdOf, planRollback, rolloutBucket,
  signManifest, UpdateError, verifyArtifact, verifyManifest,
  type InstalledState, type PinnedKey, type UpdatePayload,
} from "./manifest.ts";

const artifactBytes = Buffer.from("modbit-desktop-0.2.0-macos-arm64 package bytes");

function payload(over: Partial<UpdatePayload> = {}): UpdatePayload {
  return {
    version: "0.2.0", channel: "stable", published_at: "2026-09-30T00:00:00Z",
    min_supported_version: "0.1.0", schema_version: 17, min_db_schema_version: 15,
    critical_migration: false, rollout_percent: 100,
    artifacts: [{
      platform: "macos", arch: "arm64", url: "https://updates.example.test/modbit-0.2.0-macos-arm64.zip",
      sha256: createHash("sha256").update(artifactBytes).digest("hex"), size: artifactBytes.byteLength,
    }],
    sbom: [{ name: "modbit-core.cdx.json", sha256: "0".repeat(64) }],
    ...over,
  };
}

const installed = (over: Partial<InstalledState> = {}): InstalledState => ({
  version: "0.1.0", platform: "macos", arch: "arm64", channel: "stable", dbSchemaVersion: 17, installId: "install-a", ...over,
});

function fixture() {
  const kp = generateUpdateKeyPair();
  const pinned: PinnedKey[] = [{ key_id: kp.keyId, public_key_pem: kp.publicKeyPem }];
  return { kp, pinned };
}

test("a manifest signed by a pinned key verifies to exactly the payload that was signed", () => {
  const { kp, pinned } = fixture();
  const signed = signManifest(payload(), kp.privateKeyPem);
  assert.equal(signed.key_id, kp.keyId);
  assert.deepEqual(verifyManifest(signed, pinned), payload());
});

test("any change to the signed bytes, the signature or the key is refused with its own code", () => {
  const { kp, pinned } = fixture();
  const signed = signManifest(payload(), kp.privateKeyPem);
  const code = (fn: () => unknown) => {
    try { fn(); } catch (e) { return (e as UpdateError).code; }
    return "NO_ERROR";
  };
  // the version is edited after signing: a downgrade/forced-update attack
  assert.equal(code(() => verifyManifest({ ...signed, payload: signed.payload.replace("0.2.0", "9.9.9") }, pinned)), "SIGNATURE_INVALID");
  // the rollout is widened after signing
  assert.equal(code(() => verifyManifest({ ...signed, payload: signed.payload.replace('"rollout_percent":100', '"rollout_percent":100 ') }, pinned)), "SIGNATURE_INVALID");
  // a flipped signature byte
  const bad = Buffer.from(signed.signature, "base64");
  bad[0] = bad[0]! ^ 0xff;
  assert.equal(code(() => verifyManifest({ ...signed, signature: bad.toString("base64") }, pinned)), "SIGNATURE_INVALID");
  // signed by a key this build does not pin
  const other = generateUpdateKeyPair();
  assert.equal(code(() => verifyManifest(signManifest(payload(), other.privateKeyPem), pinned)), "SIGNATURE_UNKNOWN_KEY");
  // a pinned key claiming another key's id cannot verify
  const forged = { ...signManifest(payload(), other.privateKeyPem), key_id: kp.keyId };
  assert.equal(code(() => verifyManifest(forged, pinned)), "SIGNATURE_INVALID");
  // a revoked key is refused even with a valid signature
  assert.equal(code(() => verifyManifest(signed, [{ ...pinned[0]!, revoked: true }])), "SIGNATURE_KEY_REVOKED");
  // not a manifest at all
  assert.equal(code(() => verifyManifest({ payload: 1 } as never, pinned)), "MANIFEST_MALFORMED");
});

test("a correctly signed payload that is missing required fields is malformed, not trusted", () => {
  const { kp, pinned } = fixture();
  const signed = signManifest({ version: "0.2.0" } as never, kp.privateKeyPem);
  assert.throws(() => verifyManifest(signed, pinned), (e: UpdateError) => e.code === "MANIFEST_MALFORMED");
  const wide = signManifest(payload({ rollout_percent: 250 }), kp.privateKeyPem);
  assert.throws(() => verifyManifest(wide, pinned), (e: UpdateError) => e.code === "MANIFEST_MALFORMED");
});

test("the downloaded artifact must match the signed size and sha256 before it is used", () => {
  const p = payload();
  assert.equal(verifyArtifact(p, "macos", "arm64", artifactBytes).url, p.artifacts[0]!.url);
  const tampered = Buffer.from(artifactBytes);
  tampered[3] = tampered[3]! ^ 0x01;
  assert.throws(() => verifyArtifact(p, "macos", "arm64", tampered), (e: UpdateError) => e.code === "ARTIFACT_DIGEST_MISMATCH");
  assert.throws(() => verifyArtifact(p, "macos", "arm64", Buffer.concat([artifactBytes, Buffer.from("x")])), (e: UpdateError) => e.code === "ARTIFACT_SIZE_MISMATCH");
  assert.throws(() => verifyArtifact(p, "windows", "x64", artifactBytes), (e: UpdateError) => e.code === "ARTIFACT_MISSING");
});

test("versions order as releases do", () => {
  assert.equal(compareVersions("0.2.0", "0.1.9"), 1);
  assert.equal(compareVersions("0.10.0", "0.9.0"), 1);
  assert.equal(compareVersions("1.0.0", "1.0.0-rc.1"), 1);
  assert.equal(compareVersions("1.0.0-rc.1", "1.0.0-rc.2"), -1);
  assert.equal(compareVersions("2.0.0", "2.0.0"), 0);
  assert.throws(() => compareVersions("banana", "1.0.0"));
});

test("a staged rollout gives each install a stable answer that only widens, and approximates the share", () => {
  const ids = Array.from({ length: 4000 }, (_, i) => `install-${i}`);
  const at = (pct: number) => ids.filter((id) => rolloutBucket(id, "0.2.0") < pct);
  const p10 = at(10);
  const p50 = at(50);
  assert.ok(p10.every((id) => p50.includes(id)), "an install admitted at 10% stays admitted at 50%");
  assert.ok(Math.abs(p10.length / ids.length - 0.1) < 0.03, `10% admitted ${p10.length}/4000`);
  assert.ok(Math.abs(p50.length / ids.length - 0.5) < 0.04, `50% admitted ${p50.length}/4000`);
  assert.equal(at(0).length, 0);
  assert.equal(at(100).length, ids.length);
  const moved = ids.filter((id) => rolloutBucket(id, "0.2.0") !== rolloutBucket(id, "0.3.0")).length;
  assert.ok(moved > ids.length * 0.9, `the bucket depends on the version, so one install is not always first in line (${moved}/4000 moved)`);
  const held = ids.find((id) => rolloutBucket(id, "0.2.0") >= 10)!;
  assert.deepEqual(decideUpdate(installed({ installId: held }), payload({ rollout_percent: 10 })), { action: "none", reason: "STAGED_NOT_YET" });
  const admitted = ids.find((id) => rolloutBucket(id, "0.2.0") < 10)!;
  assert.equal(decideUpdate(installed({ installId: admitted }), payload({ rollout_percent: 10 })).action, "install");
});

test("channel, version and device-policy floors decide before anything is installed", () => {
  assert.deepEqual(decideUpdate(installed(), payload({ channel: "beta" })), { action: "none", reason: "CHANNEL_MISMATCH" });
  // device policy pins the channel: a beta manifest is offered to a device that pins beta
  assert.equal(decideUpdate(installed(), payload({ channel: "beta" }), { update_channel: "beta" }).action, "install");
  assert.deepEqual(decideUpdate(installed({ version: "0.2.0" }), payload()), { action: "none", reason: "NOT_NEWER" });
  assert.deepEqual(decideUpdate(installed({ version: "0.3.0" }), payload()), { action: "none", reason: "NOT_NEWER" });
  // an update below the device's minimum_version is refused; a device below it must update whatever the rollout says
  assert.equal(decideUpdate(installed(), payload(), { minimum_version: "0.5.0" }).action, "refuse");
  const forced = decideUpdate(installed({ installId: "held" }), payload({ rollout_percent: 0 }), { minimum_version: "0.2.0" });
  assert.deepEqual(forced, { action: "install", requiresBackup: false, mandatory: true });
  assert.equal(decideUpdate(installed({ version: "0.0.5" }), payload({ min_supported_version: "0.1.0" })).action, "refuse");
});

test("migration compatibility is settled before install: a database the update cannot open is refused, a critical migration is backed up", () => {
  const newer = decideUpdate(installed({ dbSchemaVersion: 18 }), payload({ schema_version: 17 }));
  assert.equal(newer.action, "refuse");
  assert.equal(newer.action === "refuse" && newer.reason, "DB_NEWER_THAN_UPDATE");
  const old = decideUpdate(installed({ dbSchemaVersion: 12 }), payload({ min_db_schema_version: 15 }));
  assert.equal(old.action === "refuse" && old.reason, "DB_TOO_OLD");
  assert.deepEqual(decideUpdate(installed({ dbSchemaVersion: 16 }), payload({ critical_migration: true })), { action: "install", requiresBackup: true, mandatory: false });
  assert.deepEqual(decideUpdate(installed({ dbSchemaVersion: 16 }), payload({ critical_migration: false })), { action: "install", requiresBackup: false, mandatory: false });
  // nothing to migrate, nothing to back up
  assert.deepEqual(decideUpdate(installed({ dbSchemaVersion: 17 }), payload({ critical_migration: true })), { action: "install", requiresBackup: false, mandatory: false });
});

test("rollback goes to the newest earlier release that can open the database, from the backup when it cannot", () => {
  const history = [
    { version: "0.1.0", schema_version: 15, backup_ref: "backup-0.1.0-pre-migration" },
    { version: "0.1.5", schema_version: 17 },
    { version: "0.2.0", schema_version: 17 },
  ];
  const same = planRollback(history, "0.2.0", 17);
  assert.equal(same.action === "rollback" && same.to.version, "0.1.5");
  assert.equal(same.action === "rollback" && same.restoreBackup, false);
  // the database was migrated past 0.1.0's schema: only its pre-migration backup can run it
  const migrated = planRollback([history[0]!, history[2]!], "0.2.0", 17);
  assert.equal(migrated.action === "rollback" && migrated.to.version, "0.1.0");
  assert.equal(migrated.action === "rollback" && migrated.restoreBackup, true);
  // no backup, an older schema: not compatible — refused, not attempted
  assert.deepEqual(planRollback([{ version: "0.1.0", schema_version: 15 }, history[2]!], "0.2.0", 17), { action: "none", reason: "NO_COMPATIBLE_RELEASE" });
  assert.deepEqual(planRollback([history[2]!], "0.2.0", 17), { action: "none", reason: "NO_PREVIOUS_RELEASE" });
});

test("the key id is the digest of the public key, so a pinned list is auditable", () => {
  const { kp } = fixture();
  assert.equal(keyIdOf(kp.publicKeyPem), kp.keyId);
  assert.equal(kp.keyId.length, 16);
});
