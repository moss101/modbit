/**
 * Helpers for the approval-stack, context and checkpoint specs (REQ-PX-057,
 * 058, 060, 062): start a task from the Fleet composer against the real Core
 * and a scripted model, open its conversation, and read what the Core
 * recorded. No helper here waits on a clock for a state: every wait is on
 * something the Core or the renderer shows.
 */
import { expect, type Page } from "@playwright/test";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { createServer, type Server } from "node:http";

/** Creates a task from the Fleet composer, starts it and opens its conversation from the agent list. */
export async function startAndOpen(page: Page, goal: string, repo: string): Promise<string> {
  await page.getByTestId("goal").fill(goal);
  await page.getByTestId("workspace").fill(repo);
  await page.getByTestId("run").click();
  const card = page.getByTestId("task-card").filter({ hasText: goal }).first();
  await expect(card).toBeVisible();
  const taskId = (await card.getAttribute("data-task-id"))!;
  await card.getByTestId("task-start").click();
  await openConversation(page, taskId);
  return taskId;
}

/** Creates a task but does not start it (the Fleet card stays; the conversation can be opened later). */
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

/** The session the renderer holds (the preload bridge's own answer). */
export async function sessionOf(page: Page): Promise<string> {
  const s = await page.evaluate(() => window.modbit.localState());
  if (!s.sessionId) throw new Error("no session");
  return s.sessionId;
}

/** The sha256 of a file's bytes on disk (hash compare, never a string compare of content). */
export function sha256File(path: string): string {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

export type ModelPart = string | { call: { name: string; args: unknown } };

/** The goal a request belongs to (`Task goal: ...` in the Core's first user message). */
export function goalOf(body: { messages?: { role: string; content?: unknown }[] }): string {
  for (const m of body.messages ?? []) {
    if (m.role !== "user") continue;
    const text = typeof m.content === "string" ? m.content : Array.isArray(m.content) ? m.content.map((c: { text?: string }) => c.text ?? "").join("\n") : "";
    const line = text.split("\n").find((l) => l.startsWith("Task goal: "));
    if (line) return line.slice("Task goal: ".length);
  }
  return "";
}

/**
 * A scripted OpenAI-compatible model that answers by the task's goal and by how many tool results the request already
 * carries, so several tasks can run against one model, each with its own script. It stands in for the provider only.
 */
export function routedModel(pick: (goal: string, results: number, bodyText: string) => ModelPart[] | Promise<ModelPart[]>, usageFor: (results: number) => number = () => 10): Promise<{ server: Server; url: string; requests: () => number }> {
  let count = 0;
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", async () => {
        count++;
        const parsed = JSON.parse(body || "{}") as { messages?: { role: string; content?: unknown }[] };
        const results = (parsed.messages ?? []).filter((m) => m.role === "tool").length;
        const parts = await pick(goalOf(parsed), results, body);
        res.writeHead(200, { "content-type": "text/event-stream" });
        const send = (o: unknown) => res.write(`data: ${JSON.stringify(o)}\n\n`);
        let calls = 0;
        for (const part of parts) {
          if (typeof part === "string") send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: part }, finish_reason: null }] });
          else {
            send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { tool_calls: [{ index: calls, id: `call_${results}_${calls}`, type: "function", function: { name: part.call.name, arguments: JSON.stringify(part.call.args) } }] }, finish_reason: null }] });
            calls++;
          }
        }
        send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: calls > 0 ? "tool_calls" : "stop" }], usage: { prompt_tokens: usageFor(results), completion_tokens: 5 } });
        res.write("data: [DONE]\n\n");
        res.end();
      });
    });
    server.listen(0, "127.0.0.1", () => resolveServer({ server, url: `http://127.0.0.1:${(server.address() as { port: number }).port}`, requests: () => count }));
  });
}
