/**
 * Spawns and supervises the local `modbit-core` process for this desktop
 * profile (docs/30: main is the only process permitted to connect; docs/33:
 * one Core per profile). On Core exit it respawns with backoff and reports
 * `restarting` / `connected` so the renderer can show the PX-023 degraded and
 * recovery states instead of inventing progress.
 */
import { spawn, type ChildProcess } from "node:child_process";
import { createInterface } from "node:readline";
import { ClientKind, CoreClient, parseReadyLine, type ReadyLine } from "./protocol-client.js";

export type CoreStatus =
  | { state: "starting"; restarts: number }
  | { state: "connected"; pid: number; endpoint: string; restarts: number }
  | { state: "restarting"; reason: string; restarts: number; retryInMs: number }
  | { state: "failed"; reason: string; restarts: number };

export interface SupervisorEvents {
  status(s: CoreStatus): void;
  /** A fresh client is available; callers re-subscribe from their cursor. */
  client(c: CoreClient): void;
}

export class CoreSupervisor {
  private child: ChildProcess | null = null;
  private client: CoreClient | null = null;
  private restarts = 0;
  private stopped = false;
  status: CoreStatus = { state: "starting", restarts: 0 };

  constructor(
    private readonly binary: string,
    private readonly dataDir: string,
    private readonly events: SupervisorEvents,
    private readonly build: string,
  ) {}

  current(): CoreClient | null {
    return this.client;
  }

  async start(): Promise<void> {
    this.stopped = false;
    await this.spawnOnce();
  }

  private setStatus(s: CoreStatus): void {
    this.status = s;
    this.events.status(s);
  }

  /** Incremented by every restart; a spawn that finishes after a newer one started stands down. */
  private spawnGeneration = 0;

  private async spawnOnce(): Promise<void> {
    const generation = ++this.spawnGeneration;
    this.setStatus({ state: "starting", restarts: this.restarts });
    // stdin is the lifetime tether: the Core exits when this process goes away,
    // so a killed or crashed desktop never leaves a Core holding the profile lock.
    const child = spawn(this.binary, ["--data-dir", this.dataDir, "--tether-stdin"], { stdio: ["pipe", "pipe", "pipe"] });
    this.child = child;
    // Keep the Core's last stderr lines so a degraded state names the cause
    // (docs/39 PX-023): the user sees why the Core stopped, not just that it did.
    const stderrTail: string[] = [];
    createInterface({ input: child.stderr! }).on("line", (line) => {
      process.stderr.write(`${line}\n`);
      stderrTail.push(line);
      if (stderrTail.length > 20) stderrTail.shift();
    });
    const tail = () => (stderrTail.length ? `: ${stderrTail.slice(-3).join(" | ")}` : "");
    const ready = await new Promise<ReadyLine>((resolve, reject) => {
      const rl = createInterface({ input: child.stdout! });
      rl.on("line", (line) => {
        const r = parseReadyLine(line);
        if (r) resolve(r);
      });
      child.once("exit", (code) => reject(new Error(`modbit-core exited with ${code} before it was ready${tail()}`)));
      child.once("error", reject);
    }).catch((e: Error) => {
      this.scheduleRestart(e.message);
      return null;
    });
    if (!ready) return;
    if (generation !== this.spawnGeneration) {
      // A newer restart superseded this spawn while it was starting.
      child.kill("SIGKILL");
      return;
    }
    try {
      const client = await CoreClient.connect(ready, ClientKind.DESKTOP, this.build);
      this.client = client;
      client.onClose = (reason) => {
        if (this.client === client) this.client = null;
        // An INVALID_CURSOR close means our subscription cursor was beyond the
        // Core's log (REQ-EV-0010): reconnecting re-reads the snapshot, so the
        // renderer rehydrates instead of skipping events.
        if (!this.stopped) this.scheduleRestart(`connection lost: ${reason}`);
      };
      this.setStatus({ state: "connected", pid: child.pid ?? 0, endpoint: ready.endpoint, restarts: this.restarts });
      this.events.client(client);
    } catch (e) {
      this.scheduleRestart((e as Error).message);
    }
    child.once("exit", (code, signal) => {
      if (this.child === child) this.child = null;
      if (!this.stopped) this.scheduleRestart(`modbit-core exited (${code ?? signal})${tail()}`);
    });
  }

  private restartTimer: NodeJS.Timeout | null = null;
  /** The previous Core, until it has actually exited (it holds the profile lock until then). */
  private dying: ChildProcess | null = null;

  /**
   * Wait for the previous Core to exit before spawning the next one: a Core
   * that is still shutting down owns the profile lock, and a replacement
   * spawned too early is refused and would only feed the restart loop.
   */
  private async reapDying(): Promise<void> {
    const child = this.dying;
    this.dying = null;
    if (!child || child.exitCode !== null || child.signalCode !== null) return;
    child.kill("SIGKILL");
    await new Promise<void>((resolve) => {
      const done = () => {
        clearTimeout(t);
        resolve();
      };
      const t = setTimeout(done, 2000);
      child.once("exit", done);
    });
  }

  private scheduleRestart(reason: string): void {
    if (this.stopped || this.restartTimer) return;
    process.stderr.write(`modbit-desktop: core restart scheduled: ${reason}\n`);
    this.restarts += 1;
    if (this.restarts > 20) {
      this.setStatus({ state: "failed", reason, restarts: this.restarts });
      return;
    }
    // Claim the restart slot first: closing the client below fires onClose
    // synchronously, which re-enters here and must find the slot taken (two
    // slots spawned two Cores, the loser of the profile lock exited, and
    // the restart loop chased a Core it no longer tracked).
    const retryInMs = Math.min(5000, 250 * 2 ** Math.min(this.restarts, 5));
    this.spawnGeneration += 1;
    this.restartTimer = setTimeout(() => {
      this.restartTimer = null;
      void this.reapDying().then(() => (this.stopped ? undefined : this.spawnOnce()));
    }, retryInMs);
    if (this.child) {
      this.child.kill();
      this.dying = this.child;
      this.child = null;
    }
    const client = this.client;
    this.client = null;
    client?.close();
    this.setStatus({ state: "restarting", reason, restarts: this.restarts, retryInMs });
  }

  stop(): void {
    this.stopped = true;
    if (this.restartTimer) clearTimeout(this.restartTimer);
    this.restartTimer = null;
    this.client?.close();
    this.client = null;
    if (this.child) {
      const child = this.child;
      this.child = null;
      child.stdin?.end();
      child.stdout?.destroy();
      child.kill();
      // The Core is crash-safe (every accepted command is durable before its
      // ack), so a quitting desktop escalates to SIGKILL almost at once: a Core
      // that outlives the desktop would hold the profile lock against the next one.
      const escalate = setTimeout(() => {
        if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
      }, 250);
      escalate.unref();
    }
  }
}
