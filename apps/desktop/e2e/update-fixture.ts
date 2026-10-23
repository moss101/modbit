/**
 * M10.2: what the packaged-update E2E needs from the real world — a real signing
 * key made by the release tool, real packages of two versions built by
 * electron-builder with that key pinned, and a feed served over real HTTP whose
 * manifests are signed by the real release CLI. Nothing here is a stand-in for
 * the updater; it only builds the situations the updater is put in.
 */
import { execFileSync } from "node:child_process";
import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(appDir, "..", "..");
const releaseTool = join(repoRoot, "tools", "release", "release.mjs");

export type Keys = { publicFile: string; privateFile: string; privatePem: string };

export function makeKeys(dir: string, name = "primary"): Keys {
  const publicFile = join(dir, `${name}-keys.json`);
  const privateFile = join(dir, `${name}-private.pem`);
  execFileSync("node", [releaseTool, "keygen", "--public-out", publicFile, "--private-out", privateFile], { encoding: "utf8" });
  return { publicFile, privateFile, privatePem: readFileSync(privateFile, "utf8") };
}

/** One real package of `version`, with `keys` pinned, as the zip electron-builder makes for macOS. */
export function buildZip(version: string, keys: Keys, outDir: string): string {
  const env = { ...process.env, MODBIT_UPDATE_KEYS_FILE: keys.publicFile };
  execFileSync("node", ["build.mjs"], { cwd: appDir, env, stdio: "inherit" });
  execFileSync("node", ["prepare-package.mjs"], { cwd: appDir, env, stdio: "inherit" });
  execFileSync(
    "pnpm",
    ["exec", "electron-builder", "--config", "electron-builder.yml", "--mac", "zip", `-c.extraMetadata.version=${version}`, `-c.directories.output=${outDir}`],
    { cwd: appDir, env, stdio: "inherit" },
  );
  const zip = readdirSync(outDir).find((n) => n.endsWith(".zip"));
  if (!zip) throw new Error(`electron-builder produced no zip in ${outDir}`);
  return join(outDir, zip);
}

/** `ditto -x -k` the zip: the .app a person would have after installing it. */
export function extractApp(zip: string, into: string): { app: string; exe: string } {
  mkdirSync(into, { recursive: true });
  execFileSync("ditto", ["-x", "-k", zip, into]);
  const app = join(into, readdirSync(into).find((n) => n.endsWith(".app"))!);
  return { app, exe: join(app, "Contents", "MacOS", "Modbit") };
}

export function bundleVersion(app: string): string {
  const plist = readFileSync(join(app, "Contents", "Info.plist"), "utf8");
  return /<key>CFBundleShortVersionString<\/key>\s*<string>([^<]+)<\/string>/.exec(plist)![1]!;
}

type Route = { body: Buffer; stallAfter?: number };

/** A feed over real HTTP. `stall` makes a route send part of its body and then hold the connection open. */
export class FeedServer {
  routes = new Map<string, Route>();
  stalled = new Set<ServerResponse>();
  requests: string[] = [];
  private server!: Server;
  url = "";

  async start(): Promise<void> {
    this.server = createServer((req: IncomingMessage, res: ServerResponse) => {
      const path = (req.url ?? "").replace(/^\//, "");
      this.requests.push(path);
      const r = this.routes.get(path);
      if (!r) return void res.writeHead(404).end();
      if (r.stallAfter !== undefined) {
        res.writeHead(200, { "content-length": r.body.length });
        res.write(r.body.subarray(0, r.stallAfter));
        this.stalled.add(res); // never ended: the client is killed mid-download
        return;
      }
      res.writeHead(200, { "content-length": r.body.length }).end(r.body);
    });
    await new Promise<void>((ok) => this.server.listen(0, "127.0.0.1", ok));
    this.url = `http://127.0.0.1:${(this.server.address() as { port: number }).port}`;
  }

  set(path: string, body: Buffer | string, opts: { stallAfter?: number } = {}): void {
    this.routes.set(path, { body: Buffer.from(body), ...opts });
  }

  releaseStalled(): void {
    for (const r of this.stalled) r.destroy();
    this.stalled.clear();
  }

  stop(): Promise<void> {
    this.releaseStalled();
    this.server.closeAllConnections();
    return new Promise((ok) => this.server.close(() => ok()));
  }
}

export type ManifestOptions = {
  version: string;
  zip: string;
  artifactName?: string;
  keys?: Keys;
  rolloutPercent?: number;
  schemaVersion?: number;
  minDbSchemaVersion?: number;
  minSupportedVersion?: string;
};

/** A manifest signed by the real release CLI; returns the signed JSON text. */
export function signedManifest(feedUrl: string, dir: string, o: ManifestOptions & { keys: Keys }): string {
  const out = join(dir, `manifest-${o.version}-${Math.random().toString(36).slice(2)}.json`);
  const name = o.artifactName ?? `Modbit-${o.version}.zip`;
  const args = [
    releaseTool, "manifest", "--version", o.version, "--channel", "stable",
    "--artifact", `macos:${process.arch}:${feedUrl}/${name}|${o.zip}`,
    "--rollout-percent", String(o.rolloutPercent ?? 100),
    "--out", out,
  ];
  if (o.schemaVersion !== undefined) args.push("--schema-version", String(o.schemaVersion));
  if (o.minDbSchemaVersion !== undefined) args.push("--min-db-schema-version", String(o.minDbSchemaVersion));
  if (o.minSupportedVersion !== undefined) args.push("--min-supported-version", o.minSupportedVersion);
  execFileSync("node", args, { env: { ...process.env, MODBIT_UPDATE_SIGNING_KEY: o.keys.privatePem }, encoding: "utf8" });
  return readFileSync(out, "utf8");
}

export function scratch(prefix: string): string {
  return mkdtempSync(join(tmpdir(), prefix));
}

export function exists(p: string): boolean {
  return existsSync(p);
}

export function writeText(p: string, text: string): void {
  mkdirSync(dirname(p), { recursive: true });
  writeFileSync(p, text);
}
