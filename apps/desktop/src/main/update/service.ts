/**
 * M10.2: Electron wiring for the updater. The decision logic is `manifest.ts`,
 * the staging logic `client.ts`, the file replacement `apply-helper.ts`; this
 * file supplies what only the running app knows — its version, its install
 * location, its Core binary for the offline `schema-info`, `backup` and
 * `device-policy` subcommands — and spawns the helper.
 *
 * Fail closed: with no pinned key in this build (a checkout, or a package built
 * without `MODBIT_UPDATE_KEYS_FILE`) the service reports updates as disabled
 * and never fetches or installs anything.
 */
import { execFile, spawn } from "node:child_process";
import { existsSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { promisify } from "node:util";
import { UpdateClient, type PendingUpdate, type UpdateStatus } from "./client.ts";
import type { ApplyJob, ApplyResult } from "./apply-helper.ts";
import type { DevicePolicy, InstalledState, PinnedKey, Platform } from "./manifest.ts";

const execFileAsync = promisify(execFile);

export type UpdateView = {
  enabled: boolean;
  /** Why updates are off, when they are. */
  disabledReason?: string;
  version: string;
  channel: string;
  feedUrl?: string;
  status: UpdateStatus;
  /** What the last install attempt did, shown once after restart. */
  lastApply?: ApplyResult;
  canRollback: boolean;
};

export type UpdateServiceOptions = {
  version: string;
  platform: NodeJS.Platform;
  arch: string;
  isPackaged: boolean;
  resourcesPath: string;
  execPath: string;
  /** `<userData>/update` is derived from this. */
  dataDir: string;
  coreBin: string;
  /** Quit the app so the helper can replace it. */
  quit: () => void;
  env?: NodeJS.ProcessEnv;
};

const PLATFORMS: Partial<Record<NodeJS.Platform, Platform>> = { darwin: "macos", win32: "windows", linux: "linux" };

export function readUpdateConfig(resourcesPath: string, env: NodeJS.ProcessEnv): { feedUrl?: string; channel: string; pinned: PinnedKey[] } {
  const read = <T>(name: string): T | null => {
    try {
      return JSON.parse(readFileSync(join(resourcesPath, name), "utf8")) as T;
    } catch {
      return null;
    }
  };
  const config = read<{ feed_url?: string; channel?: string }>("update-config.json");
  const keys = read<{ keys?: PinnedKey[] }>("update-keys.json")?.keys;
  // The feed URL may be overridden for staging and tests. It cannot weaken
  // anything: whatever it serves must still verify against the pinned keys.
  const feedUrl = env.MODBIT_UPDATE_FEED ?? config?.feed_url;
  return { ...(feedUrl ? { feedUrl } : {}), channel: config?.channel ?? "stable", pinned: Array.isArray(keys) ? keys : [] };
}

export class UpdateService {
  readonly client: UpdateClient | null;
  private readonly opts: UpdateServiceOptions;
  private readonly cfg: ReturnType<typeof readUpdateConfig>;
  private readonly stateDir: string;
  private lastApply: ApplyResult | undefined;
  private timer: NodeJS.Timeout | undefined;

  constructor(opts: UpdateServiceOptions) {
    this.opts = opts;
    this.stateDir = join(opts.dataDir, "update");
    this.cfg = readUpdateConfig(opts.resourcesPath, opts.env ?? process.env);
    const platform = PLATFORMS[opts.platform];
    this.client =
      platform && this.cfg.feedUrl && this.cfg.pinned.length > 0
        ? new UpdateClient({
            feedUrl: this.cfg.feedUrl,
            stateDir: this.stateDir,
            pinned: this.cfg.pinned,
            installed: () => this.installed(platform),
            policy: () => this.devicePolicy(),
            backup: (dest) => this.backup(dest),
          })
        : null;
    this.collectApplyResult();
  }

  private disabledReason(): string | undefined {
    if (!PLATFORMS[this.opts.platform]) return `no update artifacts exist for ${this.opts.platform}`;
    if (this.cfg.pinned.length === 0) return "this build pins no update signing key, so it verifies nothing and refuses every update";
    if (!this.cfg.feedUrl) return "no update feed is configured";
    return undefined;
  }

  view(): UpdateView {
    const reason = this.disabledReason();
    return {
      enabled: this.client !== null,
      ...(reason ? { disabledReason: reason } : {}),
      version: this.opts.version,
      channel: this.cfg.channel,
      ...(this.cfg.feedUrl ? { feedUrl: this.cfg.feedUrl } : {}),
      status: this.client?.getStatus() ?? { state: "idle" },
      ...(this.lastApply ? { lastApply: this.lastApply } : {}),
      canRollback: (this.client?.history().length ?? 0) > 1,
    };
  }

  /** Check the feed once now and every `everyMs` after; staging only, never installing. */
  start(everyMs = 6 * 60 * 60 * 1000, firstMs = 5_000): void {
    if (!this.client) return;
    const tick = () => void this.client!.checkAndStage();
    setTimeout(tick, firstMs).unref();
    this.timer = setInterval(tick, everyMs);
    this.timer.unref();
  }

  stop(): void {
    if (this.timer) clearInterval(this.timer);
  }

  async check(): Promise<UpdateView> {
    await this.client?.checkAndStage();
    return this.view();
  }

  /** Install the staged update: verify it again from disk, record it, hand over to the helper and quit. */
  async installPending(): Promise<UpdateView> {
    const client = this.client;
    const pending = client?.pending();
    if (!client || !pending) return this.view();
    client.verifyStaged(pending);
    this.spawnHelper(pending);
    client.commitInstall(pending);
    this.opts.quit();
    return this.view();
  }

  async rollback(): Promise<UpdateView> {
    const client = this.client;
    if (!client) return this.view();
    const s = await client.stageRollback();
    if (s.state === "staged") return this.installPending();
    return this.view();
  }

  private target(): string {
    if (this.opts.platform === "darwin") return dirname(dirname(dirname(this.opts.execPath))); // …/Modbit.app/Contents/MacOS/Modbit
    if (this.opts.platform === "linux") return (this.opts.env ?? process.env).APPIMAGE ?? this.opts.execPath;
    return this.opts.execPath;
  }

  private spawnHelper(p: PendingUpdate): void {
    const platform = PLATFORMS[this.opts.platform]!;
    const job: ApplyJob = {
      parentPid: process.pid,
      platform,
      artifact: { file: p.file, sha256: p.sha256, size: p.size },
      version: p.version,
      target: this.target(),
      resultFile: join(this.stateDir, "apply-result.json"),
      relaunch: (this.opts.env ?? process.env).MODBIT_UPDATE_NO_RELAUNCH !== "1",
      ...(p.rollback?.restore_backup ? { restoreDb: { backup: p.rollback.restore_backup, dbFile: join(this.opts.dataDir, "core.db") } } : {}),
    };
    const jobFile = join(this.stateDir, "apply-job.json");
    writeFileSync(jobFile, JSON.stringify(job));
    const helper = this.opts.isPackaged ? join(this.opts.resourcesPath, "apply-helper.cjs") : join(__dirname, "apply-helper.cjs");
    const child = spawn(this.opts.execPath, [helper, jobFile], {
      detached: true,
      stdio: "ignore",
      env: { ...(this.opts.env ?? process.env), ELECTRON_RUN_AS_NODE: "1", MODBIT_APPLY_HELPER: "1" },
    });
    child.unref();
  }

  /** What the previous run's helper did: shown once, then cleared; a good install lets go of the bundle it replaced. */
  private collectApplyResult(): void {
    const file = join(this.stateDir, "apply-result.json");
    if (!existsSync(file)) return;
    try {
      this.lastApply = JSON.parse(readFileSync(file, "utf8")) as ApplyResult;
    } catch {
      this.lastApply = { ok: false, version: "unknown", error: "apply-result.json is unreadable", atMs: Date.now() };
    }
    rmSync(file, { force: true });
    if (this.lastApply.ok && (this.opts.platform === "darwin" || this.opts.platform === "linux")) {
      const target = this.target();
      const dir = dirname(target);
      for (const n of existsSync(dir) ? readdirSync(dir) : []) {
        if (n.startsWith(`${basename(target)}.old-`)) rmSync(join(dir, n), { recursive: true, force: true });
      }
    }
  }

  private async installed(platform: Platform): Promise<Omit<InstalledState, "installId">> {
    const info = JSON.parse((await this.core(["schema-info", "--data-dir", this.opts.dataDir])).trim()) as {
      build_schema_version: number;
      on_disk_schema_version: number | null;
    };
    return {
      version: this.opts.version,
      platform,
      arch: this.opts.arch,
      channel: this.cfg.channel,
      dbSchemaVersion: info.on_disk_schema_version ?? info.build_schema_version,
    };
  }

  private async devicePolicy(): Promise<DevicePolicy> {
    const raw = JSON.parse((await this.core(["device-policy"])).trim()) as { update_channel: string | null; minimum_version: string | null };
    return {
      ...(raw.update_channel ? { update_channel: raw.update_channel } : {}),
      ...(raw.minimum_version ? { minimum_version: raw.minimum_version } : {}),
    };
  }

  private async backup(dest: string): Promise<number> {
    const out = JSON.parse((await this.core(["backup", "--data-dir", this.opts.dataDir, "--to", dest])).trim()) as { schema_version: number };
    return out.schema_version;
  }

  private async core(args: string[]): Promise<string> {
    const { stdout } = await execFileAsync(this.opts.coreBin, args, { env: this.opts.env ?? process.env, timeout: 60_000 });
    return stdout;
  }
}
