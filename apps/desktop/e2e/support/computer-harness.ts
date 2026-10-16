/**
 * Helpers for the native-control specs (PX-074): the real Electron app, the real
 * Core process and the repository's fixture actuator (`modbit-actuator-fixture`,
 * a real process speaking the same authenticated RPC as the macOS helper over an
 * in-memory desktop, selected through the Core's own configuration:
 * `MODBIT_COMPUTER_ACTUATOR` and `MODBIT_COMPUTER_ACTUATOR_ARGS`). Only the model
 * is scripted. Nothing here waits on a clock for a state; a wait is on something
 * the Core, the actuator or the renderer shows.
 */
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import type { Page } from "@playwright/test";
import { expect } from "@playwright/test";
import { coreBin } from "./ui-harness.ts";
import { type ModelPart } from "./conversation-harness.ts";
import type { Gate } from "./stream-model.ts";

/** A part of a scripted reply: text, a tool call, or a held stream the test opens when it has set what it needs. */
export type Part = ModelPart | { wait: Gate };

export const fixtureBin = join(dirname(coreBin), process.platform === "win32" ? "modbit-actuator-fixture.exe" : "modbit-actuator-fixture");

export const FORM = "org.example.fixture.form";
export const LOGIN = "org.example.fixture.login";

export interface Permissions {
  accessibility: boolean;
  screen_recording: boolean;
}

export interface ComputerRig {
  /** The Core's data directory. */
  dataDir: string;
  /** The actuator's files: its permissions file (the stand-in for System Settings), its in-flight marker and its pid. */
  dir: string;
  permissionsFile: string;
  hangMarker: string;
  env: Record<string, string>;
}

export function setPermissions(rig: ComputerRig, p: Permissions): void {
  writeFileSync(rig.permissionsFile, JSON.stringify(p));
}

/** A rig whose Core launches the fixture actuator; `permissions` is what the operating system "has granted" at start. */
export function computerRig(opts: { permissions?: Permissions; desktop?: unknown; env?: Record<string, string> } = {}): ComputerRig {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-computer-"));
  const dir = join(dataDir, "actuator");
  mkdirSync(dir, { recursive: true });
  const permissionsFile = join(dir, "permissions.json");
  const hangMarker = join(dir, "in-flight");
  const args = ["--permissions-file", permissionsFile, "--hang-marker", hangMarker, "--pid-file", join(dir, "actuator.pid")];
  if (opts.desktop) {
    const desktop = join(dir, "desktop.json");
    writeFileSync(desktop, JSON.stringify(opts.desktop));
    args.push("--desktop", desktop);
  }
  const rig: ComputerRig = {
    dataDir,
    dir,
    permissionsFile,
    hangMarker,
    env: { MODBIT_COMPUTER_ACTUATOR: fixtureBin, MODBIT_COMPUTER_ACTUATOR_ARGS: JSON.stringify(args), MODBIT_SUPPRESS_OPEN_SETTINGS: "1", ...(opts.env ?? {}) },
  };
  setPermissions(rig, opts.permissions ?? { accessibility: true, screen_recording: true });
  return rig;
}

// ------------------------------------------------------------ the scripted model

/** What the model has been shown so far, parsed from the tool results of the request. */
export interface Seen {
  texts: string[];
  snapshot: string;
  frame: string;
  tree: string;
  control: string;
}

function parseToolText(text: string): Record<string, unknown> {
  const i = text.indexOf("{");
  if (i < 0) return {};
  try {
    return JSON.parse(text.slice(i)) as Record<string, unknown>;
  } catch {
    // The result may carry trailing text after the object; read the first balanced object.
    let depth = 0;
    for (let k = i; k < text.length; k++) {
      if (text[k] === "{") depth++;
      else if (text[k] === "}" && --depth === 0) {
        try {
          return JSON.parse(text.slice(i, k + 1)) as Record<string, unknown>;
        } catch {
          return {};
        }
      }
    }
    return {};
  }
}

export function seenOf(body: { messages?: { role: string; content?: unknown }[] }): Seen {
  const seen: Seen = { texts: [], snapshot: "", frame: "", tree: "", control: "" };
  for (const m of body.messages ?? []) {
    if (m.role !== "tool") continue;
    const text = typeof m.content === "string" ? m.content : "";
    const j = parseToolText(text);
    if (typeof j.snapshot === "string") seen.snapshot = j.snapshot;
    if (typeof j.frame === "string") seen.frame = j.frame;
    if (typeof j.tree === "string") seen.tree = j.tree;
    if (typeof j.control_id === "string") seen.control = j.control_id;
    seen.texts.push(text);
  }
  return seen;
}

/** `<snapshot>/<id>` of the element named `name` in the latest tree the model read. */
export function elementOf(seen: Seen, name: string): string {
  const needle = `| "${name}" |`;
  const line = seen.tree.split("\n").find((l) => l.includes(needle));
  const id = line?.trim().split(/\s+/)[0];
  if (!id) throw new Error(`no element ${name} in\n${seen.tree}`);
  return id;
}

export type Step = (seen: Seen) => Part[];
export const call = (name: string, args: unknown): Part[] => [{ call: { name, args } }];
export const say = (text: string, ...rest: Part[]): Part[] => [text, ...rest];
export const plan = (): Step => () => call("plan.update", { outcome: "operate a native application", expected_files: [], protected_effects: ["computer"] });
export const start = (app: string, reason = "the test needs it"): Step => () => call("computer.start", { application: app, reason });
export const finish = (): Step => () => call("task.complete", { summary: "done", self_review: { findings: [] } });

/**
 * A scripted OpenAI-compatible model whose reply is `steps[n]`, `n` being the
 * number of tool results already in the request. It stands in for the provider
 * only; every tool call it makes goes through the Core's real pipeline.
 */
export function computerModel(steps: Step[]): Promise<{ server: Server; url: string; requests: () => number; bodies: () => string[] }> {
  let count = 0;
  const bodies: string[] = [];
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", async () => {
        count++;
        bodies.push(body);
        const parsed = JSON.parse(body || "{}") as { messages?: { role: string; content?: unknown }[] };
        const results = (parsed.messages ?? []).filter((m) => m.role === "tool").length;
        const step = steps[results];
        let parts: Part[];
        try {
          parts = step ? step(seenOf(parsed)) : ["Nothing further."];
        } catch (e) {
          // A script that cannot be played is the test's bug: say so loudly instead of leaving the Core waiting on a stream.
          process.stderr.write(`[computer-model] step ${results} failed: ${(e as Error).message}\n`);
          res.writeHead(500, { "content-type": "text/plain" });
          res.end(`script error at step ${results}`);
          return;
        }
        res.writeHead(200, { "content-type": "text/event-stream" });
        const send = (o: unknown) => res.write(`data: ${JSON.stringify(o)}\n\n`);
        let closed = false;
        res.on("close", () => (closed = true));
        let calls = 0;
        for (const part of parts) {
          if (closed) return;
          if ("wait" in (typeof part === "string" ? {} : part)) await Promise.race([(part as { wait: Gate }).wait.opened, new Promise<void>((r) => res.on("close", () => r()))]);
          else if (typeof part === "string") send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: part }, finish_reason: null }] });
          else {
            const c = (part as { call: { name: string; args: unknown } }).call;
            send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { tool_calls: [{ index: calls, id: `call_${results}_${calls}`, type: "function", function: { name: c.name, arguments: JSON.stringify(c.args) } }] }, finish_reason: null }] });
            calls++;
          }
        }
        if (closed) return;
        send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: calls > 0 ? "tool_calls" : "stop" }], usage: { prompt_tokens: 10, completion_tokens: 5 } });
        res.write("data: [DONE]\n\n");
        res.end();
      });
    });
    server.listen(0, "127.0.0.1", () => resolveServer({ server, url: `http://127.0.0.1:${(server.address() as { port: number }).port}`, requests: () => count, bodies: () => bodies }));
  });
}

// ------------------------------------------------------------------ the surface

/** Approves the card on screen the way a person at the keyboard does: Enter, then Enter again at the second step. */
export async function approveWithKeyboard(page: Page, tool: string): Promise<void> {
  await expect(page.getByTestId("approval-tool")).toHaveText(tool, { timeout: 90_000 });
  await page.getByTestId("approval-card").focus();
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("approval-confirm")).toBeVisible();
  await page.keyboard.press("Enter");
}

/** Approves with the pointer: Approve, then Confirm. */
export async function approveWithPointer(page: Page, tool: string): Promise<void> {
  await expect(page.getByTestId("approval-tool")).toHaveText(tool, { timeout: 90_000 });
  await page.getByTestId("approval-run").click();
  await page.getByTestId("approval-confirm-run").click();
}

export const cardOf = (page: Page, n: number) => page.locator(`[data-testid="computer-card"][data-n="${n}"]`);
