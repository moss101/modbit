/**
 * M10.2, docs/70 "Desktop update": the client half of the updater. It fetches a
 * signed manifest from the feed, verifies it against the keys pinned in this
 * build, decides with `decideUpdate`, downloads the artifact for this platform
 * and refuses it unless its size and sha256 match the signed manifest, takes the
 * backup a critical migration requires, and stages the result. Installing is a
 * separate step (`applyPending`) run by a helper after this process exits, so
 * the files being replaced are not in use.
 *
 * Everything that touches the world is injected (`fetchBytes`, the Core
 * subcommands, the installer), so the real HTTP feed, the real Core database
 * and the real packaged app are exercised by the tests and the packaged E2E.
 */
import { createHash, randomUUID } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  UpdateError,
  compareVersions,
  decideUpdate,
  planRollback,
  verifyArtifact,
  verifyManifest,
  type DevicePolicy,
  type InstalledRelease,
  type InstalledState,
  type Platform,
  type PinnedKey,
  type RollbackPlan,
  type SignedManifest,
  type UpdateArtifact,
  type UpdateDecision,
  type UpdatePayload,
} from "./manifest.ts";

export type UpdateStatus =
  | { state: "idle" }
  | { state: "up-to-date"; checkedAtMs: number }
  | { state: "held"; reason: string; version: string; checkedAtMs: number }
  | { state: "refused"; code: string; detail: string; checkedAtMs: number }
  | { state: "staged"; version: string; requiresBackup: boolean; mandatory: boolean }
  | { state: "error"; code: string; detail: string; checkedAtMs: number };

/** A verified artifact on disk waiting for the next restart to be installed. */
export type PendingUpdate = {
  version: string;
  platform: Platform;
  arch: string;
  file: string;
  sha256: string;
  size: number;
  schema_version: number;
  /** Set when a critical migration took a backup first. */
  backup_ref?: string;
  /** What the app was before this update, so the history can offer it for rollback. */
  from: { version: string; schema_version: number };
  /** A rollback to an earlier release; `restore_backup` is the pre-migration database to put back. */
  rollback?: { restore_backup?: string };
};

export type UpdateClientDeps = {
  feedUrl: string;
  /** `<userData>/update`: downloads, pending.json, releases.json, backups, install-id. */
  stateDir: string;
  pinned: PinnedKey[];
  installed: () => Promise<Omit<InstalledState, "installId">>;
  policy: () => Promise<DevicePolicy>;
  /** The Core's `backup` subcommand: returns the schema version of the copy. */
  backup: (destFile: string) => Promise<number>;
  fetchBytes?: (url: string) => Promise<Uint8Array>;
  now?: () => number;
};

async function defaultFetchBytes(url: string): Promise<Uint8Array> {
  const res = await fetch(url);
  if (!res.ok) throw new UpdateError("ARTIFACT_MISSING", `GET ${url} -> ${res.status}`);
  return new Uint8Array(await res.arrayBuffer());
}

function readJson<T>(file: string): T | null {
  try {
    return existsSync(file) ? (JSON.parse(readFileSync(file, "utf8")) as T) : null;
  } catch {
    return null;
  }
}

/** Write then rename, so a kill mid-write leaves the previous file whole. */
function writeJsonAtomic(file: string, value: unknown): void {
  const tmp = `${file}.tmp`;
  writeFileSync(tmp, JSON.stringify(value, null, 2));
  renameSync(tmp, file);
}

export class UpdateClient {
  private status: UpdateStatus = { state: "idle" };
  private readonly fetchBytes: (url: string) => Promise<Uint8Array>;
  private readonly now: () => number;

  private readonly deps: UpdateClientDeps;

  constructor(deps: UpdateClientDeps) {
    this.deps = deps;
    this.fetchBytes = deps.fetchBytes ?? defaultFetchBytes;
    this.now = deps.now ?? Date.now;
    mkdirSync(join(deps.stateDir, "downloads"), { recursive: true });
    mkdirSync(join(deps.stateDir, "backups"), { recursive: true });
    this.discardPartials();
  }

  getStatus(): UpdateStatus {
    const pending = this.pending();
    return pending && this.status.state !== "staged"
      ? { state: "staged", version: pending.version, requiresBackup: pending.backup_ref !== undefined, mandatory: false }
      : this.status;
  }

  /** A download killed mid-way leaves only `.part` files, never a file that looks verified. */
  private discardPartials(): void {
    const dir = join(this.deps.stateDir, "downloads");
    for (const name of readdirSync(dir)) if (name.endsWith(".part")) rmSync(join(dir, name), { force: true });
  }

  /** Stable per install, so a staged rollout gives the same answer until it widens. */
  installId(): string {
    const f = join(this.deps.stateDir, "install-id");
    const existing = existsSync(f) ? readFileSync(f, "utf8").trim() : "";
    if (existing) return existing;
    const id = randomUUID();
    writeFileSync(f, id, { mode: 0o600 });
    return id;
  }

  pending(): PendingUpdate | null {
    return readJson<PendingUpdate>(join(this.deps.stateDir, "pending.json"));
  }

  history(): InstalledRelease[] {
    return readJson<InstalledRelease[]>(join(this.deps.stateDir, "releases.json")) ?? [];
  }

  /**
   * Fetch, verify, decide, download, verify the artifact, back up if the
   * migration is critical, and stage. Never installs. Anything that fails
   * verification is refused and leaves nothing staged.
   */
  async checkAndStage(): Promise<UpdateStatus> {
    const checkedAtMs = this.now();
    try {
      const raw = await this.fetchBytes(`${this.deps.feedUrl.replace(/\/$/, "")}/manifest.json`);
      let signed: SignedManifest;
      try {
        signed = JSON.parse(Buffer.from(raw).toString("utf8")) as SignedManifest;
      } catch {
        throw new UpdateError("MANIFEST_MALFORMED", "the feed's manifest.json is not JSON");
      }
      const payload = verifyManifest(signed, this.deps.pinned);
      const installed: InstalledState = { ...(await this.deps.installed()), installId: this.installId() };
      const decision: UpdateDecision = decideUpdate(installed, payload, await this.deps.policy());
      if (decision.action === "none") {
        return (this.status = decision.reason === "STAGED_NOT_YET"
          ? { state: "held", reason: decision.reason, version: payload.version, checkedAtMs }
          : { state: "up-to-date", checkedAtMs });
      }
      if (decision.action === "refuse") {
        return (this.status = { state: "refused", code: decision.reason, detail: decision.detail, checkedAtMs });
      }
      const pending = await this.download(payload, installed, decision.requiresBackup);
      writeJsonAtomic(join(this.deps.stateDir, "pending.json"), pending);
      return (this.status = { state: "staged", version: pending.version, requiresBackup: decision.requiresBackup, mandatory: decision.mandatory });
    } catch (e) {
      const code = e instanceof UpdateError ? e.code : "UPDATE_FAILED";
      return (this.status = { state: "error", code, detail: e instanceof Error ? e.message : String(e), checkedAtMs });
    }
  }

  private async download(payload: UpdatePayload, installed: InstalledState, requiresBackup: boolean, rollback?: PendingUpdate["rollback"]): Promise<PendingUpdate> {
    const artifact: UpdateArtifact | undefined = payload.artifacts.find((a) => a.platform === installed.platform && a.arch === installed.arch);
    if (!artifact) throw new UpdateError("ARTIFACT_MISSING", `no ${installed.platform}/${installed.arch} artifact in ${payload.version}`);
    const url = /^[a-z]+:\/\//i.test(artifact.url) ? artifact.url : `${this.deps.feedUrl.replace(/\/$/, "")}/${artifact.url}`;
    const bytes = await this.fetchBytes(url);
    verifyArtifact(payload, installed.platform, installed.arch, bytes);
    const name = `${payload.version}-${installed.platform}-${installed.arch}${artifact.url.includes(".") ? artifact.url.slice(artifact.url.lastIndexOf(".")) : ""}`;
    const final = join(this.deps.stateDir, "downloads", name);
    const part = `${final}.part`;
    writeFileSync(part, bytes);
    renameSync(part, final);
    let backup_ref: string | undefined;
    if (requiresBackup) {
      // Before anything is installed: the database as it is now, so a rollback
      // to a release that writes an older schema has a database it can open.
      const dest = join(this.deps.stateDir, "backups", `pre-${payload.version}.db`);
      rmSync(dest, { force: true });
      await this.deps.backup(dest);
      backup_ref = dest;
    }
    return {
      version: payload.version,
      platform: installed.platform,
      arch: installed.arch,
      file: final,
      sha256: artifact.sha256,
      size: artifact.size,
      schema_version: payload.schema_version,
      ...(backup_ref ? { backup_ref } : {}),
      from: { version: installed.version, schema_version: installed.dbSchemaVersion },
      ...(rollback ? { rollback } : {}),
    };
  }

  /** Re-verify the staged file from disk immediately before it is used: staging is not trust. */
  verifyStaged(p: PendingUpdate): void {
    if (!existsSync(p.file)) throw new UpdateError("ARTIFACT_MISSING", `${p.file} is gone`);
    const bytes = readFileSync(p.file);
    if (bytes.byteLength !== p.size) throw new UpdateError("ARTIFACT_SIZE_MISMATCH", `${bytes.byteLength} bytes, staged as ${p.size}`);
    const digest = createHash("sha256").update(bytes).digest("hex");
    if (digest !== p.sha256) throw new UpdateError("ARTIFACT_DIGEST_MISMATCH", `sha256 ${digest}, staged as ${p.sha256}`);
  }

  /**
   * Record that `pending` is about to be installed. The release being left
   * joins the history with its schema and, for an update across a critical
   * migration, the backup taken before it; the release being installed is
   * recorded too, so a later rollback knows what it came from. Call after
   * `verifyStaged`, before the helper replaces the app.
   */
  commitInstall(p: PendingUpdate): void {
    const byVersion = new Map(this.history().map((h) => [h.version, h] as const));
    if (!p.rollback) {
      byVersion.set(p.from.version, { version: p.from.version, schema_version: p.from.schema_version, ...(p.backup_ref ? { backup_ref: p.backup_ref } : {}) });
    }
    byVersion.set(p.version, { ...(byVersion.get(p.version) ?? {}), version: p.version, schema_version: p.schema_version });
    writeJsonAtomic(join(this.deps.stateDir, "releases.json"), [...byVersion.values()]);
    rmSync(join(this.deps.stateDir, "pending.json"), { force: true });
  }

  /** The newest earlier release that can open the database on disk (`planRollback`). */
  async planRollback(): Promise<{ plan: RollbackPlan; current: string; dbSchemaVersion: number }> {
    const installed = await this.deps.installed();
    return { plan: planRollback(this.history(), installed.version, installed.dbSchemaVersion), current: installed.version, dbSchemaVersion: installed.dbSchemaVersion };
  }

  /**
   * Stage a rollback: plan it from the history, fetch that release's own signed
   * manifest from the feed (`releases/<version>/manifest.json`), verify it
   * against the pinned keys, download and verify its artifact. Nothing is
   * installed; a release with no signed manifest on the feed cannot be rolled back to.
   */
  async stageRollback(): Promise<UpdateStatus> {
    const checkedAtMs = this.now();
    try {
      const { plan } = await this.planRollback();
      if (plan.action === "none") return (this.status = { state: "refused", code: plan.reason, detail: "no earlier release can open this database", checkedAtMs });
      const raw = await this.fetchBytes(`${this.deps.feedUrl.replace(/\/$/, "")}/releases/${plan.to.version}/manifest.json`);
      const payload = verifyManifest(JSON.parse(Buffer.from(raw).toString("utf8")) as SignedManifest, this.deps.pinned);
      if (payload.version !== plan.to.version) throw new UpdateError("MANIFEST_MALFORMED", `the feed's ${plan.to.version} manifest is for ${payload.version}`);
      const installed: InstalledState = { ...(await this.deps.installed()), installId: this.installId() };
      const pending = await this.download(payload, installed, false, plan.restoreBackup && plan.to.backup_ref ? { restore_backup: plan.to.backup_ref } : {});
      writeJsonAtomic(join(this.deps.stateDir, "pending.json"), pending);
      return (this.status = { state: "staged", version: pending.version, requiresBackup: false, mandatory: false });
    } catch (e) {
      const code = e instanceof UpdateError ? e.code : "ROLLBACK_FAILED";
      return (this.status = { state: "error", code, detail: e instanceof Error ? e.message : String(e), checkedAtMs });
    }
  }

  /** Newest history entry strictly newer than `version`, for diagnostics. */
  newestKnown(): string | null {
    return this.history().map((h) => h.version).sort(compareVersions).pop() ?? null;
  }
}
