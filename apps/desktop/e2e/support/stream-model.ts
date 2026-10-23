/**
 * A scripted OpenAI-compatible model whose stream the test controls (PX-047):
 * text goes out in pieces, and a `Gate` holds the stream open until the test
 * releases it, so "partial text is on screen and the stream is still open" is
 * a state the test creates, not a timing it hopes for. Like the repository's
 * other scripted model it stands in for the provider only: the Core, its
 * event log and the renderer are real.
 */
import { createServer, type Server } from "node:http";

export class Gate {
  private release!: () => void;
  readonly opened = new Promise<void>((r) => (this.release = r));
  open(): void {
    this.release();
  }
}

export type Part = string | { call: { name: string; args: unknown } } | { wait: Gate } | { pauseMs: number };
export interface Reply {
  parts: Part[];
}

export interface StreamModel {
  server: Server;
  url: string;
  /** Requests answered so far. */
  requests: () => number;
}

/** The reply is chosen by the number of tool results already in the request (the repository's scripted-model convention). */
export function streamingModel(script: Reply[]): Promise<StreamModel> {
  let count = 0;
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", () => {
        count++;
        const parsed = JSON.parse(body || "{}") as { messages?: { role: string }[] };
        const results = (parsed.messages ?? []).filter((m) => m.role === "tool").length;
        const reply = script[results] ?? { parts: ["Nothing further."] };
        let closed = false;
        res.on("close", () => (closed = true));
        res.writeHead(200, { "content-type": "text/event-stream" });
        const send = (o: unknown) => {
          if (!closed) res.write(`data: ${JSON.stringify(o)}\n\n`);
        };
        void (async () => {
          let calls = 0;
          for (const part of reply.parts) {
            if (closed) return;
            if (typeof part === "string") send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: part }, finish_reason: null }] });
            else if ("call" in part) {
              send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { tool_calls: [{ index: calls, id: `call_${results}_${calls}`, type: "function", function: { name: part.call.name, arguments: JSON.stringify(part.call.args) } }] }, finish_reason: null }] });
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
    server.listen(0, "127.0.0.1", () => resolveServer({ server, url: `http://127.0.0.1:${(server.address() as { port: number }).port}`, requests: () => count }));
  });
}
