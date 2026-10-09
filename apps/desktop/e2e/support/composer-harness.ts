/**
 * Helpers for the composer specs (REQ-PX-054..056): a scripted
 * OpenAI-compatible model that answers by the number of requests so far (a
 * follow-up turn is a new request with no new tool result), whose stream a
 * test holds open with a Gate, and which keeps every request body so a spec
 * can assert what really reached the model. It stands in for the provider
 * only; the Core, its log, the preload bridge and the renderer are real.
 */
import { expect, type Page } from "@playwright/test";
import { createServer, type Server } from "node:http";
import { Gate, type Part } from "./stream-model.ts";

export { Gate };
export type { Part };

export interface RequestBody {
  model?: string;
  reasoning_effort?: string;
  messages?: { role: string; content?: unknown }[];
  [k: string]: unknown;
}
export type SeqReply = { parts: Part[] } | ((ctx: { index: number; body: RequestBody }) => { parts: Part[] } | Promise<{ parts: Part[] }>);

export interface SeqModel {
  server: Server;
  url: string;
  requests: () => number;
  bodies: () => RequestBody[];
  /** The whole body of request `i` as one string, for substring assertions. */
  text: (i: number) => string;
}

/** Request N (0-based) is answered by `script[N]`; past the end the reply is a short text. */
export function sequencedModel(script: SeqReply[]): Promise<SeqModel> {
  const seen: RequestBody[] = [];
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      let raw = "";
      req.on("data", (c) => (raw += c));
      req.on("end", () => {
        const body = JSON.parse(raw || "{}") as RequestBody;
        const index = seen.length;
        seen.push(body);
        let closed = false;
        res.on("close", () => (closed = true));
        void (async () => {
          const entry = script[index];
          const reply = entry === undefined ? { parts: [`Answer ${index + 1}.`] } : typeof entry === "function" ? await entry({ index, body }) : entry;
          if (closed) return;
          res.writeHead(200, { "content-type": "text/event-stream" });
          const send = (o: unknown) => {
            if (!closed) res.write(`data: ${JSON.stringify(o)}\n\n`);
          };
          let calls = 0;
          for (const part of reply.parts) {
            if (closed) return;
            if (typeof part === "string") send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: part }, finish_reason: null }] });
            else if ("call" in part) {
              send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { tool_calls: [{ index: calls, id: `call_${index}_${calls}`, type: "function", function: { name: part.call.name, arguments: JSON.stringify(part.call.args) } }] }, finish_reason: null }] });
              calls++;
            } else if ("wait" in part) await Promise.race([part.wait.opened, new Promise<void>((r) => res.on("close", () => r()))]);
            else await new Promise((r) => setTimeout(r, part.pauseMs));
          }
          if (closed) return;
          send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: calls > 0 ? "tool_calls" : "stop" }], usage: { prompt_tokens: 10, completion_tokens: 5 } });
          res.write("data: [DONE]\n\n");
          res.end();
        })();
      });
    });
    server.listen(0, "127.0.0.1", () =>
      resolveServer({
        server,
        url: `http://127.0.0.1:${(server.address() as { port: number }).port}`,
        requests: () => seen.length,
        bodies: () => seen,
        text: (i) => JSON.stringify(seen[i] ?? {}),
      }),
    );
  });
}

/** Creates a task from the Fleet form, starts it and opens its conversation from the agent list. */
export async function startAndOpen(page: Page, goal: string, repo: string): Promise<string> {
  const taskId = await createTask(page, goal, repo);
  const card = page.getByTestId("task-card").filter({ hasText: goal }).first();
  await card.getByTestId("task-start").click();
  await openConversation(page, taskId);
  return taskId;
}

/** Creates a task (not started) from the Fleet form and returns its id. */
export async function createTask(page: Page, goal: string, repo: string): Promise<string> {
  await page.getByTestId("goal").fill(goal);
  await page.getByTestId("workspace").fill(repo);
  await page.getByTestId("run").click();
  const card = page.getByTestId("task-card").filter({ hasText: goal }).first();
  await expect(card).toBeVisible();
  return (await card.getAttribute("data-task-id"))!;
}

export async function openConversation(page: Page, taskId: string): Promise<void> {
  const row = page.locator(`[data-testid="agent-row"][data-agent-id="${taskId}"] [data-testid="agent-open"]`);
  await expect(row).toBeVisible({ timeout: 30_000 });
  await row.click();
  await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", taskId);
  await expect(page.getByTestId("conversation")).toHaveAttribute("data-loaded", "true");
}

/** The Core's own picture of the task's queue, over the typed bridge (the ground truth the tray must match). */
export const coreQueue = (page: Page, taskId: string) => page.evaluate((t) => window.modbit.composerQueue(t).then((q) => q.items.filter((i) => i.state === "QUEUED").map((i) => ({ id: i.inputId, text: i.text, mode: i.mode, edited: i.edited }))), taskId);

/** The user-visible messages of the transcript, in order. */
export const userMessages = (page: Page, taskId: string) => page.evaluate((t) => window.modbit.transcript(t, { density: "DETAILED" }).then((p) => p.rows.filter((r) => r.kind === "USER_MESSAGE").map((r) => r.text)), taskId);

/** Types into the composer and presses a key (the message box is a textarea). */
export async function typeAndPress(page: Page, text: string, key: string): Promise<void> {
  const box = page.getByTestId("composer-input");
  await box.fill(text);
  await box.press(key);
  // A send is complete when the composer has handed the words to the Core and cleared the box; the next message is typed after that.
  if (key === "Enter") await expect(box).toHaveValue("", { timeout: 15_000 });
}
