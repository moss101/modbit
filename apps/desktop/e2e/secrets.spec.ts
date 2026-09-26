/**
 * Platform conformance: keychain and secret storage (PX-030, docs/76; the
 * custody rule of PX-022 and docs/23). On every CI platform the real app
 * sets up a provider through a live test call; then:
 *
 * - where the OS offers a keychain (Keychain on macOS, DPAPI on Windows, a
 *   secret service on Linux), only ciphertext reaches disk — the key's text
 *   and its base64 appear nowhere in the profile, the record is owner-only on
 *   Unix — and a restarted app hands the key back to its Core without the
 *   user entering it again;
 * - where it does not (a Linux runner with no secret service, whose
 *   `basic_text` fallback is a fixed key, not custody), nothing is persisted,
 *   the user is told so, and a restarted app has no provider.
 *
 * Which branch ran, and on which backend, is annotated on the result.
 */
import { _electron as electron, expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { existsSync, mkdtempSync, readdirSync, readFileSync, statSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const coreBin = process.env.MODBIT_CORE_BIN ?? resolve(appDir, "..", "..", "target", "debug", process.platform === "win32" ? "modbit-core.exe" : "modbit-core");

/** An OpenAI-compatible endpoint that answers the setup probe and records
 *  the credentials it was sent. */
function provider(): Promise<{ server: Server; url: string; seen: string[] }> {
  const seen: string[] = [];
  return new Promise((resolveServer) => {
    const server = createServer((req, res) => {
      seen.push(req.headers.authorization ?? "");
      req.on("data", () => {});
      req.on("end", () => {
        res.writeHead(200, { "content-type": "text/event-stream" });
        const send = (o: unknown) => res.write(`data: ${JSON.stringify(o)}\n\n`);
        send({ id: "c", model: "scripted", choices: [{ index: 0, delta: { content: "ok" }, finish_reason: null }] });
        send({ id: "c", model: "scripted", choices: [{ index: 0, delta: {}, finish_reason: "stop" }], usage: { prompt_tokens: 3, completion_tokens: 1 } });
        res.write("data: [DONE]\n\n");
        res.end();
      });
    });
    server.listen(0, "127.0.0.1", () => resolveServer({ server, url: `http://127.0.0.1:${(server.address() as { port: number }).port}`, seen }));
  });
}

async function launch(dataDir: string): Promise<{ app: ElectronApplication; page: Page }> {
  const app = await electron.launch({
    args: [join(appDir, "dist", "main", "main.cjs")],
    env: { ...process.env, MODBIT_DATA_DIR: dataDir, MODBIT_CORE_BIN: coreBin, OPENAI_API_KEY: "", ANTHROPIC_API_KEY: "", MODBIT_OPENAI_BASE_URL: "", MODBIT_ANTHROPIC_BASE_URL: "" },
  });
  const page = await app.firstWindow();
  app.process().stderr?.on("data", (d: Buffer) => process.stderr.write(`[electron] ${d}`));
  await expect(page.getByTestId("core-status")).toContainText("Core connected", { timeout: 60_000 });
  return { app, page };
}

async function closeApp(app: ElectronApplication): Promise<void> {
  const proc = app.process();
  await Promise.race([app.close(), new Promise<void>((r) => setTimeout(r, 15_000))]);
  if (proc.exitCode === null) proc.kill("SIGKILL");
}

/** Every file under `dir`, recursively. */
function files(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? files(join(dir, e.name)) : e.isFile() ? [join(dir, e.name)] : []));
}

test("keychain custody: ciphertext only at rest and restored after a restart, or nothing kept where the OS has no keychain", async ({}, testInfo) => {
  const key = `sk-px030-${Math.random().toString(36).slice(2)}-${Date.now()}`;
  const { server, url, seen } = await provider();
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-secrets-"));
  try {
    const first = await launch(dataDir);
    await first.page.getByTestId("provider-key").fill(key);
    await first.page.getByTestId("provider-url").fill(url);
    await first.page.getByTestId("provider-test").click();
    await expect(first.page.getByTestId("provider-result")).toHaveAttribute("data-ok", "true", { timeout: 30_000 });
    expect(seen.some((h) => h === `Bearer ${key}`), "the live test call carried the key").toBe(true);
    const status = await first.page.evaluate(() => window.modbit.providerStatus());
    testInfo.annotations.push({ type: "keychain", description: `${process.platform}: backend=${status.keychainBackend} usable=${status.keychainAvailable}` });
    const record = join(dataDir, "provider.enc");
    if (status.keychainAvailable) {
      await expect(first.page.getByTestId("provider-result")).toContainText("OS keychain");
      expect(existsSync(record)).toBe(true);
      const stored = JSON.parse(readFileSync(record, "utf8")) as { keyCiphertext: string };
      expect(stored.keyCiphertext.length).toBeGreaterThan(0);
      if (process.platform !== "win32") expect(statSync(record).mode & 0o777).toBe(0o600);
    } else {
      await expect(first.page.getByTestId("provider-result")).toContainText("was not saved");
      expect(existsSync(record)).toBe(false);
    }
    await closeApp(first.app);
    // Nothing in the profile holds the key's text or its base64, whatever the branch.
    const b64 = Buffer.from(key).toString("base64");
    for (const f of files(dataDir)) {
      const bytes = readFileSync(f);
      expect(bytes.includes(key) || bytes.includes(b64), `${f} holds the key`).toBe(false);
    }
    // A restarted app hands a kept key back to its Core; a key never kept is gone.
    const second = await launch(dataDir);
    try {
      if (status.keychainAvailable) {
        await expect.poll(async () => (await second.page.evaluate(() => window.modbit.providerStatus())).configured, { timeout: 30_000 }).toBe(true);
      } else {
        const after = await second.page.evaluate(() => window.modbit.providerStatus());
        expect(after.configured).toBe(false);
        expect(after.stored).toBe(false);
      }
    } finally {
      await closeApp(second.app);
    }
  } finally {
    server.close();
  }
});
