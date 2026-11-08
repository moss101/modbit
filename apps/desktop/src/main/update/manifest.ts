/**
 * M10.2, docs/70 "Desktop update": signed update manifests, staged rollout,
 * migration compatibility checked before install, rollback to the previous
 * compatible version. Pure decision and verification logic over `node:crypto`
 * (ed25519, sha256) — no dependency, no network, no clock: the caller supplies
 * the bytes, the pinned keys, the device policy and the install identity.
 * The release tooling (`tools/release/release.mjs`) signs with the same
 * module, so what CI produces is exactly what the app verifies.
 */
import { createHash, createPrivateKey, createPublicKey, generateKeyPairSync, sign, verify } from "node:crypto";

export type Platform = "macos" | "windows" | "linux";

export type UpdateArtifact = { platform: Platform; arch: string; url: string; sha256: string; size: number };

export type UpdatePayload = {
  version: string;
  channel: string;
  published_at: string;
  /** Builds older than this cannot update directly; they must pass through an intermediate release. */
  min_supported_version: string;
  /** Core database schema version this build writes (`SCHEMA_VERSION`). */
  schema_version: number;
  /** Oldest database schema this build can migrate from. */
  min_db_schema_version: number;
  /** A non-additive migration: a backup/checkpoint is taken before install (docs/70). */
  critical_migration: boolean;
  /** Staged rollout: the share of installs, 0..100, that may take this update now. */
  rollout_percent: number;
  artifacts: UpdateArtifact[];
  /** SBOM documents released with the build, by digest. */
  sbom: { name: string; sha256: string }[];
};

/** The exact bytes that were signed travel as a string, so no canonicalization can disagree. */
export type SignedManifest = { payload: string; signature: string; key_id: string };

export type PinnedKey = { key_id: string; public_key_pem: string; revoked?: boolean };

export type UpdateErrorCode =
  | "MANIFEST_MALFORMED"
  | "SIGNATURE_UNKNOWN_KEY"
  | "SIGNATURE_KEY_REVOKED"
  | "SIGNATURE_INVALID"
  | "ARTIFACT_MISSING"
  | "ARTIFACT_SIZE_MISMATCH"
  | "ARTIFACT_DIGEST_MISMATCH";

export class UpdateError extends Error {
  readonly code: UpdateErrorCode;
  constructor(code: UpdateErrorCode, message: string) {
    super(`${code}: ${message}`);
    this.name = "UpdateError";
    this.code = code;
  }
}

export function keyIdOf(publicKeyPem: string): string {
  const der = createPublicKey(publicKeyPem).export({ type: "spki", format: "der" });
  return createHash("sha256").update(der).digest("hex").slice(0, 16);
}

export function generateUpdateKeyPair(): { publicKeyPem: string; privateKeyPem: string; keyId: string } {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const publicKeyPem = publicKey.export({ type: "spki", format: "pem" }).toString();
  const privateKeyPem = privateKey.export({ type: "pkcs8", format: "pem" }).toString();
  return { publicKeyPem, privateKeyPem, keyId: keyIdOf(publicKeyPem) };
}

export function signManifest(payload: UpdatePayload, privateKeyPem: string): SignedManifest {
  const text = JSON.stringify(payload);
  const key = createPrivateKey(privateKeyPem);
  const keyId = keyIdOf(createPublicKey(key).export({ type: "spki", format: "pem" }).toString());
  return { payload: text, signature: sign(null, Buffer.from(text, "utf8"), key).toString("base64"), key_id: keyId };
}

function isPayload(v: unknown): v is UpdatePayload {
  if (typeof v !== "object" || v === null) return false;
  const p = v as Record<string, unknown>;
  const str = (k: string) => typeof p[k] === "string" && (p[k] as string).length > 0;
  const num = (k: string) => typeof p[k] === "number" && Number.isFinite(p[k]);
  return (
    str("version") && str("channel") && str("published_at") && str("min_supported_version") &&
    num("schema_version") && num("min_db_schema_version") && typeof p.critical_migration === "boolean" &&
    num("rollout_percent") && (p.rollout_percent as number) >= 0 && (p.rollout_percent as number) <= 100 &&
    Array.isArray(p.artifacts) && Array.isArray(p.sbom) &&
    (p.artifacts as unknown[]).every((a) => {
      const x = a as Record<string, unknown>;
      return typeof x?.url === "string" && typeof x.sha256 === "string" && typeof x.size === "number" &&
        typeof x.platform === "string" && typeof x.arch === "string";
    })
  );
}

/** Verify the signature against the pinned keys, then parse. Anything else is refused, never guessed. */
export function verifyManifest(signed: SignedManifest, pinned: PinnedKey[]): UpdatePayload {
  if (typeof signed?.payload !== "string" || typeof signed.signature !== "string" || typeof signed.key_id !== "string") {
    throw new UpdateError("MANIFEST_MALFORMED", "the manifest is not {payload, signature, key_id}");
  }
  const key = pinned.find((k) => k.key_id === signed.key_id);
  if (!key) throw new UpdateError("SIGNATURE_UNKNOWN_KEY", `key ${signed.key_id} is not pinned in this build`);
  if (key.revoked) throw new UpdateError("SIGNATURE_KEY_REVOKED", `key ${signed.key_id} is revoked`);
  const ok = verify(null, Buffer.from(signed.payload, "utf8"), createPublicKey(key.public_key_pem), Buffer.from(signed.signature, "base64"));
  if (!ok) throw new UpdateError("SIGNATURE_INVALID", "the signature does not match the payload");
  let parsed: unknown;
  try {
    parsed = JSON.parse(signed.payload);
  } catch {
    throw new UpdateError("MANIFEST_MALFORMED", "the signed payload is not JSON");
  }
  if (!isPayload(parsed)) throw new UpdateError("MANIFEST_MALFORMED", "the signed payload is missing required fields");
  return parsed;
}

/** The artifact for this platform, checked against its declared size and sha256 before anything runs. */
export function verifyArtifact(payload: UpdatePayload, platform: Platform, arch: string, bytes: Uint8Array): UpdateArtifact {
  const artifact = payload.artifacts.find((a) => a.platform === platform && a.arch === arch);
  if (!artifact) throw new UpdateError("ARTIFACT_MISSING", `no ${platform}/${arch} artifact in ${payload.version}`);
  if (bytes.byteLength !== artifact.size) {
    throw new UpdateError("ARTIFACT_SIZE_MISMATCH", `${bytes.byteLength} bytes, the manifest says ${artifact.size}`);
  }
  const digest = createHash("sha256").update(bytes).digest("hex");
  if (digest !== artifact.sha256) throw new UpdateError("ARTIFACT_DIGEST_MISMATCH", `sha256 ${digest}, the manifest says ${artifact.sha256}`);
  return artifact;
}

/** `MAJOR.MINOR.PATCH[-pre]`; a release outranks its own pre-release. */
export function compareVersions(a: string, b: string): number {
  const parse = (v: string) => {
    const m = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/.exec(v);
    if (!m) throw new UpdateError("MANIFEST_MALFORMED", `not a version: ${v}`);
    return { n: [Number(m[1]), Number(m[2]), Number(m[3])], pre: m[4] };
  };
  const x = parse(a);
  const y = parse(b);
  for (let i = 0; i < 3; i++) if (x.n[i] !== y.n[i]) return x.n[i]! < y.n[i]! ? -1 : 1;
  if (x.pre === y.pre) return 0;
  if (x.pre === undefined) return 1;
  if (y.pre === undefined) return -1;
  return x.pre < y.pre ? -1 : 1;
}

/** A stable 0..99 bucket per install and version: the same install gets the same answer while a rollout widens. */
export function rolloutBucket(installId: string, version: string): number {
  const h = createHash("sha256").update(`${installId}\u0000${version}`).digest();
  return h.readUInt32BE(0) % 100;
}

export type InstalledState = {
  version: string;
  platform: Platform;
  arch: string;
  channel: string;
  /** The schema version of the Core database on disk. */
  dbSchemaVersion: number;
  installId: string;
};

export type DevicePolicy = { update_channel?: string; minimum_version?: string };

export type UpdateDecision =
  | { action: "none"; reason: "CHANNEL_MISMATCH" | "NOT_NEWER" | "STAGED_NOT_YET" }
  | { action: "refuse"; reason: "REQUIRES_INTERMEDIATE" | "DB_NEWER_THAN_UPDATE" | "DB_TOO_OLD" | "BELOW_DEVICE_MINIMUM"; detail: string }
  | { action: "install"; requiresBackup: boolean; mandatory: boolean };

/**
 * docs/70: what to do with a verified manifest. Migration compatibility is
 * settled here, before any byte is installed: an update that cannot open the
 * database on disk is refused, and a non-additive migration is installed only
 * behind a backup.
 */
export function decideUpdate(now: InstalledState, payload: UpdatePayload, policy: DevicePolicy = {}): UpdateDecision {
  const channel = policy.update_channel ?? now.channel;
  if (payload.channel !== channel) return { action: "none", reason: "CHANNEL_MISMATCH" };
  if (compareVersions(payload.version, now.version) <= 0) return { action: "none", reason: "NOT_NEWER" };
  const floor = policy.minimum_version;
  if (floor !== undefined && compareVersions(payload.version, floor) < 0) {
    return { action: "refuse", reason: "BELOW_DEVICE_MINIMUM", detail: `${payload.version} is below the device minimum ${floor}` };
  }
  const mandatory = floor !== undefined && compareVersions(now.version, floor) < 0;
  if (!mandatory && rolloutBucket(now.installId, payload.version) >= payload.rollout_percent) {
    return { action: "none", reason: "STAGED_NOT_YET" };
  }
  if (compareVersions(now.version, payload.min_supported_version) < 0) {
    return { action: "refuse", reason: "REQUIRES_INTERMEDIATE", detail: `${now.version} is older than ${payload.min_supported_version}; update through an intermediate release` };
  }
  if (now.dbSchemaVersion > payload.schema_version) {
    return { action: "refuse", reason: "DB_NEWER_THAN_UPDATE", detail: `the database is at schema ${now.dbSchemaVersion}, ${payload.version} writes ${payload.schema_version} and would refuse write mode` };
  }
  if (now.dbSchemaVersion < payload.min_db_schema_version) {
    return { action: "refuse", reason: "DB_TOO_OLD", detail: `the database is at schema ${now.dbSchemaVersion}, ${payload.version} migrates from ${payload.min_db_schema_version}` };
  }
  return { action: "install", requiresBackup: payload.critical_migration && now.dbSchemaVersion < payload.schema_version, mandatory };
}

export type InstalledRelease = { version: string; schema_version: number; backup_ref?: string };

export type RollbackPlan =
  | { action: "rollback"; to: InstalledRelease; restoreBackup: boolean }
  | { action: "none"; reason: "NO_PREVIOUS_RELEASE" | "NO_COMPATIBLE_RELEASE" };

/**
 * Rollback to the newest earlier release that can open the database on disk.
 * An earlier release that writes an older schema can run only from the backup
 * taken before the migration; without one it is not compatible.
 */
export function planRollback(history: InstalledRelease[], current: string, dbSchemaVersion: number): RollbackPlan {
  const earlier = history.filter((r) => compareVersions(r.version, current) < 0).sort((a, b) => compareVersions(b.version, a.version));
  if (earlier.length === 0) return { action: "none", reason: "NO_PREVIOUS_RELEASE" };
  for (const r of earlier) {
    if (r.schema_version >= dbSchemaVersion) return { action: "rollback", to: r, restoreBackup: false };
    if (r.backup_ref !== undefined) return { action: "rollback", to: r, restoreBackup: true };
  }
  return { action: "none", reason: "NO_COMPATIBLE_RELEASE" };
}
