/**
 * Helpers for the latency-budget specs (perf-budgets.spec.ts): the distribution
 * of a measurement, a scripted OpenAI-compatible model that speaks in paced
 * pieces and is routed by task goal, and the retained result file.
 *
 * The result files are measurements, not dossier evidence: they say what one
 * run on one machine saw, with the environment named, so a budget that is not
 * met is on record as not met.
 */
import { createServer, type Server } from "node:http";
import { mkdirSync, writeFileSync } from "node:fs";
import { cpus, platform, release, totalmem } from "node:os";
import { join } from "node:path";
import type { TestInfo } from "@playwright/test";

export interface Dist {
  n: number;
  min: number;
  p50: number;
  p95: number;
  p99: number;
  max: number;
  mean: number;
}

/** Nearest-rank percentile of an ascending list. */
function pct(sorted: readonly number[], q: number): number {
  if (sorted.length === 0) return NaN;
  const rank = Math.ceil(q * sorted.length);
  return sorted[Math.min(sorted.length - 1, Math.max(0, rank - 1))]!;
}

export function dist(samples: readonly number[]): Dist {
  const s = [...samples].sort((a, b) => a - b);
  const sum = s.reduce((a, b) => a + b, 0);
  const round = (x: number): number => Math.round(x * 100) / 100;
  return { n: s.length, min: round(s[0] ?? NaN), p50: round(pct(s, 0.5)), p95: round(pct(s, 0.95)), p99: round(pct(s, 0.99)), max: round(s[s.length - 1] ?? NaN), mean: round(sum / Math.max(1, s.length)) };
}

export type Part = string | { call: { name: string; args: unknown } } | { pauseMs: number } | { wait: Promise<void> };

/** A scripted model: `pick(goal, results)` is the reply to a request carrying `results` tool results for the task whose goal is `goal`. Stands in for the provider only. */
export function pacedModel(pick: (goal: string, results: number) => Part[]): Promise<{ server: Server; url: string }> {
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", () => {
        const parsed = JSON.parse(body || "{}") as { messages?: { role: string; content?: unknown }[] };
        const msgs = parsed.messages ?? [];
        const results = msgs.filter((m) => m.role === "tool").length;
        let goal = "";
        for (const m of msgs) {
          if (m.role !== "user") continue;
          const text = typeof m.content === "string" ? m.content : Array.isArray(m.content) ? m.content.map((c: { text?: string }) => c.text ?? "").join("\n") : "";
          const line = text.split("\n").find((l) => l.startsWith("Task goal: "));
          if (line) {
            goal = line.slice("Task goal: ".length);
            break;
          }
        }
        let closed = false;
        res.on("close", () => (closed = true));
        res.writeHead(200, { "content-type": "text/event-stream" });
        const send = (o: unknown) => {
          if (!closed) res.write(`data: ${JSON.stringify(o)}\n\n`);
        };
        void (async () => {
          let calls = 0;
          for (const part of pick(goal, results)) {
            if (closed) return;
            if (typeof part === "string") send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: part }, finish_reason: null }] });
            else if ("call" in part) {
              send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { tool_calls: [{ index: calls, id: `call_${results}_${calls}`, type: "function", function: { name: part.call.name, arguments: JSON.stringify(part.call.args) } }] }, finish_reason: null }] });
              calls++;
            } else if ("wait" in part) await Promise.race([part.wait, new Promise<void>((r) => res.on("close", () => r()))]);
            else await new Promise((r) => setTimeout(r, part.pauseMs));
          }
          if (closed) return;
          send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: calls > 0 ? "tool_calls" : "stop" }], usage: { prompt_tokens: 10, completion_tokens: 5 } });
          res.write("data: [DONE]\n\n");
          res.end();
        })();
      });
    });
    server.listen(0, "127.0.0.1", () => resolveServer({ server, url: `http://127.0.0.1:${(server.address() as { port: number }).port}` }));
  });
}

/** The machine a measurement was taken on, so a number is never read without it. */
export function environment(): Record<string, unknown> {
  const c = cpus();
  return {
    platform: platform(),
    os: release(),
    cpu: c[0]?.model ?? "unknown",
    cores: c.length,
    memoryGiB: Math.round((totalmem() / 2 ** 30) * 10) / 10,
    ci: process.env.CI === "true" || process.env.GITHUB_ACTIONS === "true",
    runner: process.env.RUNNER_NAME ?? process.env.RUNNER_OS ?? "local",
    app: "unpackaged build (apps/desktop/dist) under Playwright; the packaged-app run is PX-025's (M10.4)",
    clock: "Core event timestamp (recorded_at, ms) and the renderer's performance.timeOrigin+now on the same host",
  };
}

export interface Budget {
  /** What the dossier says, with its location. */
  statement: string;
  /** The numeric bound in ms, or null where the dossier names none. */
  limitMs: number | null;
  /** Which percentile is bounded. */
  percentile: "p95" | null;
}

/**
 * Retain one measurement: `<out>/<row>/<run>/result.json` and `summary.md`. `out` is
 * MODBIT_PERF_OUT (benchmarks/live-results/perf when a person keeps a run) or the
 * test's own output directory.
 */
export function retain(info: TestInfo, row: string, result: Record<string, unknown>): string {
  const root = process.env.MODBIT_PERF_OUT ?? join(info.outputPath(), "perf");
  const run = process.env.MODBIT_PERF_RUN ?? new Date().toISOString().replace(/[:.]/g, "-");
  const dir = join(root, row, run);
  mkdirSync(dir, { recursive: true });
  const json = { row, run, recordedAt: new Date().toISOString(), environment: environment(), ...result };
  writeFileSync(join(dir, "result.json"), `${JSON.stringify(json, null, 2)}\n`);
  const lines = [`# ${row}`, "", `Run ${run} on ${String((json.environment as { platform: string }).platform)} (${String((json.environment as { cpu: string }).cpu)}, ${String((json.environment as { cores: number }).cores)} cores).`, ""];
  for (const [name, d] of Object.entries(result.distributions as Record<string, Dist>)) lines.push(d.n === 0 ? `- ${name}: none` : `- ${name}: n=${d.n}, p50 ${d.p50} ms, p95 ${d.p95} ms, p99 ${d.p99} ms, max ${d.max} ms`);
  const verdict = result.verdict as { met: boolean | null; text: string } | undefined;
  if (verdict) lines.push("", `Budget: ${verdict.met === null ? "no numeric budget in the dossier" : verdict.met ? "MET" : "NOT MET"}. ${verdict.text}`);
  writeFileSync(join(dir, "summary.md"), `${lines.join("\n")}\n`);
  // The numbers are also in the test log, where a CI run keeps them.
  console.log(`[perf] ${row} ${JSON.stringify({ run, distributions: result.distributions, verdict })}`);
  return dir;
}
