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

  private async spawnOnce(): Promise<void> {
    this.setStatus({ state: "starting", restarts: this.restarts });
    const child = spawn(this.binary, ["--data-dir", this.dataDir], { stdio: ["ignore", "pipe", "inherit"] });
    this.child = child;
    const ready = await new Promise<ReadyLine>((resolve, reject) => {
      const rl = createInterface({ input: child.stdout! });
      rl.on("line", (line) => {
        const r = parseReadyLine(line);
        if (r) resolve(r);
      });
      child.once("exit", (code) => reject(new Error(`modbit-core exited with ${code} before it was ready`)));
      child.once("error", reject);
    }).catch((e: Error) => {
      this.scheduleRestart(e.message);
      return null;
    });
    if (!ready) return;
    try {
      const client = await CoreClient.connect(ready, ClientKind.DESKTOP, this.build);
      this.client = client;
      client.onClose = (reason) => {
        if (this.client === client) this.client = null;
        if (!this.stopped) this.scheduleRestart(`connection lost: ${reason}`);
      };
      this.setStatus({ state: "connected", pid: child.pid ?? 0, endpoint: ready.endpoint, restarts: this.restarts });
      this.events.client(client);
    } catch (e) {
      this.scheduleRestart((e as Error).message);
    }
    child.once("exit", (code, signal) => {
      if (this.child === child) this.child = null;
      if (!this.stopped) this.scheduleRestart(`modbit-core exited (${code ?? signal})`);
    });
  }

  private restartTimer: NodeJS.Timeout | null = null;

  private scheduleRestart(reason: string): void {
    if (this.stopped || this.restartTimer) return;
    if (this.child) {
      this.child.kill();
      this.child = null;
    }
    this.client?.close();
    this.client = null;
    this.restarts += 1;
    if (this.restarts > 20) {
      this.setStatus({ state: "failed", reason, restarts: this.restarts });
      return;
    }
    const retryInMs = Math.min(5000, 250 * 2 ** Math.min(this.restarts, 5));
    this.setStatus({ state: "restarting", reason, restarts: this.restarts, retryInMs });
    this.restartTimer = setTimeout(() => {
      this.restartTimer = null;
      void this.spawnOnce();
    }, retryInMs);
  }

  stop(): void {
    this.stopped = true;
    if (this.restartTimer) clearTimeout(this.restartTimer);
    this.restartTimer = null;
    this.client?.close();
    this.client = null;
    if (this.child) {
      this.child.stdout?.destroy();
      this.child.kill();
      this.child = null;
    }
  }
}
