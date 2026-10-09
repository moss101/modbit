/**
 * Helpers for the apps-panel specs (REQ-PX-048): a scripted OpenAI-compatible
 * model whose replies can wait on the test (so a task stays Running while the
 * test uses the panel), terminal commands that are cross-platform (the test
 * runner's own Node on a PTY, so no `sh` or `stty` is assumed), and the
 * renderer-side collectors the IPC-level specs use.
 */
import type { Page } from "@playwright/test";
import { createServer, type Server } from "node:http";

export interface ModelReply {
  text?: string;
  calls?: { name: string; args: unknown }[];
}
export interface RequestContext {
  /** How many tool results the request carries: the step of the script. */
  results: number;
  body: { messages?: { role: string; content?: unknown }[] };
}
export type Step = ModelReply | ((ctx: RequestContext) => ModelReply | Promise<ModelReply>);

/** The terminal handles the tool results in a request name, in the order they were started. */
export function handlesIn(body: RequestContext["body"]): string[] {
  const out: string[] = [];
  for (const m of body.messages ?? []) {
    if (m.role !== "tool" || typeof m.content !== "string") continue;
    const hit = /"session_id"\s*:\s*"([0-9A-Za-z_-]+)"/.exec(m.content);
    if (hit) out.push(hit[1]!);
  }
  return out;
}

export function toolResults(body: RequestContext["body"]): string[] {
  return (body.messages ?? []).filter((m) => m.role === "tool").map((m) => (typeof m.content === "string" ? m.content : JSON.stringify(m.content)));
}

/** A scripted model: step N answers the request that carries N tool results; a step may be a function that waits (a gate) before it answers. */
export function steppedModel(script: Step[]): Promise<{ server: Server; url: string }> {
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      let raw = "";
      req.on("data", (c) => (raw += c));
      req.on("end", () => {
        void (async () => {
          const body = JSON.parse(raw || "{}") as RequestContext["body"];
          const results = (body.messages ?? []).filter((m) => m.role === "tool").length;
          const step = script[results];
          const reply: ModelReply = step === undefined ? { text: "Nothing further." } : typeof step === "function" ? await step({ results, body }) : step;
          res.writeHead(200, { "content-type": "text/event-stream" });
          const send = (o: unknown) => res.write(`data: ${JSON.stringify(o)}\n\n`);
          if (reply.text) send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: reply.text }, finish_reason: null }] });
          (reply.calls ?? []).forEach((c, i) => send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { tool_calls: [{ index: i, id: `call_${results}_${i}`, type: "function", function: { name: c.name, arguments: JSON.stringify(c.args) } }] }, finish_reason: null }] }));
          send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: reply.calls?.length ? "tool_calls" : "stop" }], usage: { prompt_tokens: 10, completion_tokens: 5 } });
          res.write("data: [DONE]\n\n");
          res.end();
        })();
      });
    });
    server.listen(0, "127.0.0.1", () => resolveServer({ server, url: `http://127.0.0.1:${(server.address() as { port: number }).port}` }));
  });
}

/** A promise the test resolves later, to hold the model's reply (and so the task) where the test wants it. */
export function gate(): { wait: Promise<void>; open: () => void } {
  let open!: () => void;
  const wait = new Promise<void>((r) => (open = r));
  return { wait, open };
}

/**
 * Waits until the task has `count` terminals. When it never does, the failure carries what the page shows
 * (the conversation holds the Core's tool results and any pending approval), so a terminal that was
 * refused or failed to spawn says why instead of only that the count stayed at zero.
 */
export async function expectTerminals(page: Page, taskId: string, count: number): Promise<void> {
  const deadline = Date.now() + 60_000;
  let seen = 0;
  while (Date.now() < deadline) {
    seen = await page.evaluate((t) => window.modbit.terminalList(t).then((l) => l.terminals.length), taskId);
    if (seen === count) return;
    await new Promise((r) => setTimeout(r, 250));
  }
  const shown = (await page.locator("body").innerText().catch(() => "")).replace(/\s+/g, " ").slice(0, 3000);
  throw new Error(`the task has ${seen} terminals, expected ${count}; the page shows: ${shown}`);
}

/** `node -e <script>` as a `shell.start` call on a PTY, using the runner's own Node. */
export function nodeTerminal(script: string): { name: string; args: unknown } {
  return { name: "shell.start", args: { argv: [process.execPath, "-e", script], inherit_env: true, pty: true, timeout_ms: 600_000 } };
}

/** Echoes each typed line back as `got:<line>`. */
export const ECHO_LOOP = "process.stdout.write('READY\\n'); process.stdin.setEncoding('utf8'); process.stdin.on('data', (d) => process.stdout.write('got:' + d.trim() + '\\n')); setInterval(() => {}, 1000)";
/** Prints the PTY's size at start and on every resize. */
export const SIZE_REPORTER = "const p = () => process.stdout.write('SIZE ' + process.stdout.columns + ' ' + process.stdout.rows + '\\n'); p(); process.stdout.on('resize', p); setInterval(() => {}, 1000)";
/** About 400 KiB of numbered 1 KiB lines, then stays alive. */
export const BIG_OUTPUT = "const l = 'x'.repeat(1019) + '\\n'; for (let i = 0; i < 400; i++) process.stdout.write(String(i).padStart(4, '0') + l); setInterval(() => {}, 1000)";
/** Prints a line and exits with code 3. */
// The exit waits a moment: a Windows pseudo-console can lose the last output of a process that exits at once.
export const EXIT_THREE = "console.log('finished'); setTimeout(() => process.exit(3), 500)";

/**
 * In the renderer: attach to a terminal over the bridge and read its output up to the head it had when asked,
 * acknowledging as it goes. Returns the bytes and whether the stream stayed within its window.
 */
export async function readTerminal(page: Page, a: { sessionId: string; taskId: string; terminalId: string; after: string; windowBytes: number }): Promise<{ bytes: number[]; head: string; maxInFlight: number; window: string }> {
  return page.evaluate(async ({ sessionId, taskId, terminalId, after, windowBytes }) => {
    const listed = await window.modbit.terminalList(taskId);
    const head = BigInt(listed.terminals.find((t) => t.terminalId === terminalId)!.bytesSoFar);
    const chunks: { cursor: bigint; data: Uint8Array }[] = [];
    let attachId = "";
    let done: () => void = () => undefined;
    const finished = new Promise<void>((r) => (done = r));
    let acked = BigInt(after);
    let maxInFlight = 0n;
    const queue: { attachId: string; cursor: string; data: Uint8Array; kind: string }[] = [];
    const off = window.modbit.onTerminalFrame((f) => {
      if (f.kind === "output") queue.push({ attachId: f.attachId, cursor: f.cursor, data: f.data, kind: f.kind });
      void drain();
    });
    let draining = false;
    async function drain() {
      if (draining || !attachId) return;
      draining = true;
      try {
        while (queue.length) {
          const f = queue.shift()!;
          if (f.attachId !== attachId) continue;
          const at = BigInt(f.cursor);
          chunks.push({ cursor: at, data: f.data });
          const end = at + BigInt(f.data.length);
          if (end - acked > maxInFlight) maxInFlight = end - acked;
          acked = end;
          await window.modbit.terminalAck(attachId, end.toString());
          if (end >= head) done();
        }
      } finally {
        draining = false;
      }
    }
    const att = await window.modbit.terminalAttach(sessionId, taskId, terminalId, after, { windowBytes });
    attachId = att.attachId;
    if (BigInt(after) >= head) done();
    void drain();
    await finished;
    off();
    await window.modbit.terminalDetach(attachId);
    const all: number[] = [];
    for (const c of chunks) for (const b of c.data) all.push(b);
    return { bytes: all, head: head.toString(), maxInFlight: maxInFlight.toString(), window: att.windowBytes };
  }, a).then((r) => ({ bytes: r.bytes, head: r.head, maxInFlight: Number(r.maxInFlight), window: r.window }));
}
