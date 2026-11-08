/**
 * M10.2: the process that replaces the app. The running app cannot replace
 * its own files (Windows locks them, macOS would swap a bundle under itself),
 * so after staging and verifying an update the app spawns this helper — the
 * same Electron binary run as plain Node (`ELECTRON_RUN_AS_NODE=1`) — and
 * quits. The helper waits for the app to exit, optionally puts a database
 * backup back (a rollback across a migration), swaps the install, relaunches,
 * and writes `apply-result.json` for the next start to report.
 *
 * A failed install leaves the previous app in place: the swap moves the
 * current bundle aside first and moves it back if the new one cannot be put
 * in position. The artifact was verified against the signed manifest before
 * this job was written and is verified again here, from the bytes on disk.
 */
import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";

export type ApplyJob = {
  parentPid: number;
  platform: "macos" | "windows" | "linux";
  artifact: { file: string; sha256: string; size: number };
  version: string;
  /** macOS: the `.app` bundle; Linux: the AppImage; Windows: the installed `Modbit.exe`. */
  target: string;
  resultFile: string;
  /** Start the app again when done. Off for a managed or headless install that restarts it itself. */
  relaunch: boolean;
  /** A rollback across a migration: put this backup back as the Core database. */
  restoreDb?: { backup: string; dbFile: string };
};

export type ApplyResult = { ok: boolean; version: string; error?: string; atMs: number };

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

async function waitForExit(pid: number, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      process.kill(pid, 0);
    } catch {
      return;
    }
    await sleep(100);
  }
  throw new Error(`the app (pid ${pid}) did not exit within ${timeoutMs} ms`);
}

function verifyBytes(a: ApplyJob["artifact"]): void {
  const bytes = readFileSync(a.file);
  if (bytes.byteLength !== a.size) throw new Error(`artifact is ${bytes.byteLength} bytes, expected ${a.size}`);
  const digest = createHash("sha256").update(bytes).digest("hex");
  if (digest !== a.sha256) throw new Error(`artifact sha256 ${digest} does not match ${a.sha256}`);
}

function restoreDatabase(r: NonNullable<ApplyJob["restoreDb"]>): void {
  // The post-update database is kept beside the restored one, never destroyed.
  if (existsSync(r.dbFile)) renameSync(r.dbFile, `${r.dbFile}.before-rollback-${Date.now()}`);
  for (const ext of ["-wal", "-shm"]) rmSync(`${r.dbFile}${ext}`, { force: true });
  copyFileSync(r.backup, r.dbFile);
}

function run(cmd: string, args: string[]): void {
  const res = spawnSync(cmd, args, { encoding: "utf8" });
  if (res.status !== 0) throw new Error(`${cmd} ${args.join(" ")} exited ${res.status}: ${res.stderr || res.stdout}`);
}

/** The bundle inside an extracted zip, whose own version must be the one the manifest promised. */
function findApp(dir: string): string {
  const app = readdirSync(dir).find((n) => n.endsWith(".app"));
  if (!app) throw new Error("the artifact contains no .app bundle");
  return join(dir, app);
}

function bundleVersion(app: string): string {
  const plist = readFileSync(join(app, "Contents", "Info.plist"), "utf8");
  const m = /<key>CFBundleShortVersionString<\/key>\s*<string>([^<]+)<\/string>/.exec(plist);
  if (!m) throw new Error("the bundle's Info.plist has no CFBundleShortVersionString");
  return m[1]!;
}

function swap(current: string, next: string): string {
  const aside = `${current}.old-${Date.now()}`;
  renameSync(current, aside);
  try {
    renameSync(next, current);
  } catch (e) {
    renameSync(aside, current); // the previous app stays the app
    throw e;
  }
  return aside;
}

function installMac(job: ApplyJob): void {
  const staging = join(dirname(job.artifact.file), `extract-${job.version}`);
  rmSync(staging, { recursive: true, force: true });
  mkdirSync(staging, { recursive: true });
  run("ditto", ["-x", "-k", job.artifact.file, staging]);
  const next = findApp(staging);
  if (bundleVersion(next) !== job.version) throw new Error(`the bundle is ${bundleVersion(next)}, the manifest promised ${job.version}`);
  // The new bundle must be on the same volume as the current one for an atomic rename.
  const beside = `${job.target}.new-${Date.now()}`;
  run("ditto", [next, beside]);
  swap(job.target, beside);
  rmSync(staging, { recursive: true, force: true });
}

function installLinux(job: ApplyJob): void {
  const beside = `${job.target}.new-${Date.now()}`;
  copyFileSync(job.artifact.file, beside);
  chmodSync(beside, 0o755);
  swap(job.target, beside);
}

function installWindows(job: ApplyJob): void {
  // The NSIS installer replaces the per-user install in place; /S is silent.
  run(job.artifact.file, ["/S"]);
  if (!existsSync(job.target)) throw new Error(`${job.target} is missing after the installer ran`);
}

function relaunch(job: ApplyJob): void {
  if (!job.relaunch) return;
  // The app's own executable with the environment it ran under: its data
  // directory and settings carry over, which `open` would drop.
  const launch = { cmd: job.platform === "macos" ? join(job.target, "Contents", "MacOS", basename(job.target, ".app")) : job.target, args: [] as string[] };
  // The relaunched app must be the app, not another helper.
  const { ELECTRON_RUN_AS_NODE: _runAsNode, MODBIT_APPLY_HELPER: _helper, ...env } = process.env;
  spawn(launch.cmd, launch.args, { detached: true, stdio: "ignore", env }).unref();
}

export async function applyJob(job: ApplyJob): Promise<ApplyResult> {
  try {
    await waitForExit(job.parentPid, 60_000);
    verifyBytes(job.artifact);
    if (job.restoreDb) restoreDatabase(job.restoreDb);
    if (job.platform === "macos") installMac(job);
    else if (job.platform === "linux") installLinux(job);
    else installWindows(job);
    const ok: ApplyResult = { ok: true, version: job.version, atMs: Date.now() };
    writeFileSync(job.resultFile, JSON.stringify(ok));
    relaunch(job);
    return ok;
  } catch (e) {
    const failed: ApplyResult = { ok: false, version: job.version, error: e instanceof Error ? e.message : String(e), atMs: Date.now() };
    writeFileSync(job.resultFile, JSON.stringify(failed));
    // The previous app is still there; start it so the user is not left with nothing.
    try {
      relaunch(job);
    } catch {
      /* the result file says what happened */
    }
    return failed;
  }
}

/** Entry point when run as `electron apply-helper.cjs <job.json>`. */
export async function main(argv: string[]): Promise<number> {
  const jobFile = argv[0];
  if (!jobFile) return 2;
  const job = JSON.parse(readFileSync(jobFile, "utf8")) as ApplyJob;
  return (await applyJob(job)).ok ? 0 : 1;
}

if (process.env.MODBIT_APPLY_HELPER === "1") {
  void main(process.argv.slice(2)).then((code) => process.exit(code));
}
