/**
 * M10.2: the update client against a real HTTP feed — real ed25519 manifests,
 * real artifact bytes, real files in a state directory. The backup is the one
 * injected dependency here (the Core's own `backup` subcommand is proven by
 * the Core's tests and the packaged E2E); everything else is the production path.
 */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, before, describe, it } from "node:test";
import { UpdateClient, type PendingUpdate } from "./client.ts";
import { UpdateError, generateUpdateKeyPair, signManifest, type Platform, type UpdatePayload } from "./manifest.ts";

const key = generateUpdateKeyPair();
const pinned = [{ key_id: key.keyId, public_key_pem: key.publicKeyPem }];
const platform: Platform = "macos";
const arch = "arm64";

function sha(b: Buffer): string {
  return createHash("sha256").update(b).digest("hex");
}

function payload(version: string, bytes: Buffer, over: Partial<UpdatePayload> = {}): UpdatePayload {
  return {
    version,
    channel: "stable",
    published_at: "2026-10-02T00:00:00Z",
    min_supported_version: "0.0.1",
    schema_version: 17,
    min_db_schema_version: 1,
    critical_migration: false,
    rollout_percent: 100,
    artifacts: [{ platform, arch, url: `Modbit-${version}.zip`, sha256: sha(bytes), size: bytes.length }],
    sbom: [],
    ...over,
  };
}

/** A feed served over real HTTP from an in-memory file map. */
class Feed {
  files = new Map<string, Buffer>();
  hits: string[] = [];
  server!: Server;
  url = "";
  async start(): Promise<void> {
    this.server = createServer((req, res) => {
      const path = (req.url ?? "").replace(/^\//, "");
      this.hits.push(path);
      const body = this.files.get(path);
      if (!body) {
        res.writeHead(404).end();
        return;
      }
      res.writeHead(200, { "content-length": body.length }).end(body);
    });
    await new Promise<void>((r) => this.server.listen(0, "127.0.0.1", r));
    const addr = this.server.address();
    assert(addr && typeof addr === "object");
    this.url = `http://127.0.0.1:${addr.port}`;
  }
  publish(p: UpdatePayload, bytes: Buffer, opts: { signWith?: string; also?: string } = {}): void {
    const signed = signManifest(p, opts.signWith ?? key.privateKeyPem);
    this.files.set("manifest.json", Buffer.from(JSON.stringify(signed)));
    this.files.set(`releases/${p.version}/manifest.json`, Buffer.from(JSON.stringify(signed)));
    this.files.set(p.artifacts[0]!.url, bytes);
  }
  stop(): Promise<void> {
    return new Promise((r) => this.server.close(() => r()));
  }
}

function client(feed: Feed, over: { version?: string; db?: number; policy?: { update_channel?: string; minimum_version?: string }; backup?: (d: string) => Promise<number>; dir?: string } = {}) {
  const stateDir = over.dir ?? mkdtempSync(join(tmpdir(), "modbit-update-"));
  const c = new UpdateClient({
    feedUrl: feed.url,
    stateDir,
    pinned,
    installed: async () => ({ version: over.version ?? "0.1.0", platform, arch, channel: "stable", dbSchemaVersion: over.db ?? 17 }),
    policy: async () => over.policy ?? {},
    backup: over.backup ?? (async (dest) => {
      writeFileSync(dest, "database-copy");
      return 17;
    }),
  });
  return { c, stateDir };
}

describe("UpdateClient over a real feed", () => {
  const feed = new Feed();
  before(() => feed.start());
  after(() => feed.stop());

  it("stages a verified newer release and records what it came from", async () => {
    const bytes = Buffer.from("artifact-0.2.0");
    feed.publish(payload("0.2.0", bytes), bytes);
    const { c, stateDir } = client(feed);
    const s = await c.checkAndStage();
    assert.equal(s.state, "staged", JSON.stringify(s));
    const p = c.pending();
    assert(p);
    assert.equal(p.version, "0.2.0");
    assert.deepEqual(p.from, { version: "0.1.0", schema_version: 17 });
    assert.equal(readFileSync(p.file).toString(), "artifact-0.2.0");
    assert.equal(sha(readFileSync(p.file)), p.sha256);
    assert.deepEqual(readdirSync(join(stateDir, "downloads")).filter((n) => n.endsWith(".part")), []);
  });

  it("an artifact whose bytes do not match the signed digest is refused and nothing is staged", async () => {
    const real = Buffer.from("artifact-0.3.0");
    feed.publish(payload("0.3.0", real), real);
    feed.files.set("Modbit-0.3.0.zip", Buffer.from("artifact-0.3.X")); // same length, different bytes
    const { c, stateDir } = client(feed);
    const s = await c.checkAndStage();
    assert.deepEqual([s.state, s.state === "error" && s.code], ["error", "ARTIFACT_DIGEST_MISMATCH"]);
    assert.equal(c.pending(), null);
    assert.deepEqual(readdirSync(join(stateDir, "downloads")), []);
    // A truncated artifact fails on size before the digest.
    feed.files.set("Modbit-0.3.0.zip", real.subarray(0, 5));
    const t = await c.checkAndStage();
    assert.deepEqual([t.state, t.state === "error" && t.code], ["error", "ARTIFACT_SIZE_MISMATCH"]);
  });

  it("a manifest signed by a key this build does not pin, or altered after signing, is refused before any download", async () => {
    const bytes = Buffer.from("artifact-0.4.0");
    const other = generateUpdateKeyPair();
    feed.publish(payload("0.4.0", bytes), bytes, { signWith: other.privateKeyPem });
    feed.hits.length = 0;
    const { c } = client(feed);
    const unknown = await c.checkAndStage();
    assert.deepEqual([unknown.state, unknown.state === "error" && unknown.code], ["error", "SIGNATURE_UNKNOWN_KEY"]);

    feed.publish(payload("0.4.0", bytes), bytes);
    const signed = JSON.parse(feed.files.get("manifest.json")!.toString());
    signed.payload = signed.payload.replace('"rollout_percent":100', '"rollout_percent":101').replace("0.4.0", "9.9.9");
    feed.files.set("manifest.json", Buffer.from(JSON.stringify(signed)));
    feed.hits.length = 0;
    const tampered = await c.checkAndStage();
    assert.deepEqual([tampered.state, tampered.state === "error" && tampered.code], ["error", "SIGNATURE_INVALID"]);
    assert.deepEqual(feed.hits, ["manifest.json"], "no artifact was requested for an unverified manifest");
    assert.equal(c.pending(), null);
  });

  it("a staged rollout holds an install outside the percentage and admits it when widened", async () => {
    const bytes = Buffer.from("artifact-0.5.0");
    const { c } = client(feed);
    feed.publish(payload("0.5.0", bytes, { rollout_percent: 0 }), bytes);
    const held = await c.checkAndStage();
    assert.equal(held.state, "held");
    assert.equal(c.pending(), null);
    feed.publish(payload("0.5.0", bytes, { rollout_percent: 100 }), bytes);
    assert.equal((await c.checkAndStage()).state, "staged");
  });

  it("refuses an update that cannot open the database on disk, and honors the device policy", async () => {
    const bytes = Buffer.from("artifact-0.6.0");
    feed.publish(payload("0.6.0", bytes, { schema_version: 17 }), bytes);
    const newerDb = await client(feed, { db: 18 }).c.checkAndStage();
    assert.deepEqual([newerDb.state, newerDb.state === "refused" && newerDb.code], ["refused", "DB_NEWER_THAN_UPDATE"]);

    const wrongChannel = await client(feed, { policy: { update_channel: "beta" } }).c.checkAndStage();
    assert.equal(wrongChannel.state, "up-to-date");

    const belowFloor = await client(feed, { policy: { minimum_version: "0.9.0" } }).c.checkAndStage();
    assert.deepEqual([belowFloor.state, belowFloor.state === "refused" && belowFloor.code], ["refused", "BELOW_DEVICE_MINIMUM"]);
  });

  it("a critical migration is installed only behind a backup, and a failed backup stages nothing", async () => {
    const bytes = Buffer.from("artifact-0.7.0");
    feed.publish(payload("0.7.0", bytes, { schema_version: 18, critical_migration: true }), bytes);
    const { c, stateDir } = client(feed, { db: 17 });
    const s = await c.checkAndStage();
    assert.equal(s.state, "staged");
    const p = c.pending();
    assert(p?.backup_ref);
    assert.equal(readFileSync(p.backup_ref).toString(), "database-copy");
    assert(p.backup_ref.startsWith(join(stateDir, "backups")));

    const failing = client(feed, { db: 17, backup: async () => { throw new Error("disk full"); } });
    const f = await failing.c.checkAndStage();
    assert.equal(f.state, "error");
    assert.equal(failing.c.pending(), null, "no backup, no staged update");
  });

  it("a download killed part-way leaves no file that looks verified", async () => {
    const dir = mkdtempSync(join(tmpdir(), "modbit-update-kill-"));
    mkdirSync(join(dir, "downloads"), { recursive: true });
    writeFileSync(join(dir, "downloads", "0.8.0-macos-arm64.zip.part"), "half a download");
    const { c } = client(feed, { dir });
    assert.deepEqual(readdirSync(join(dir, "downloads")), []);
    assert.equal(c.pending(), null);
  });

  it("re-verifies the staged file from disk, so tampering after staging is caught before install", async () => {
    const bytes = Buffer.from("artifact-0.9.0");
    feed.publish(payload("0.9.0", bytes), bytes);
    const { c } = client(feed);
    assert.equal((await c.checkAndStage()).state, "staged");
    const p = c.pending() as PendingUpdate;
    c.verifyStaged(p);
    writeFileSync(p.file, Buffer.from("artifact-0.9.X"));
    assert.throws(() => c.verifyStaged(p), (e: unknown) => e instanceof UpdateError && e.code === "ARTIFACT_DIGEST_MISMATCH");
    writeFileSync(p.file, "short");
    assert.throws(() => c.verifyStaged(p), (e: unknown) => e instanceof UpdateError && e.code === "ARTIFACT_SIZE_MISMATCH");
  });

  it("rolls back to an earlier release by its own signed manifest, restoring the pre-migration backup", async () => {
    const v1 = Buffer.from("artifact-1.0.0");
    const v2 = Buffer.from("artifact-1.1.0");
    // 1.0.0 -> 1.1.0 across a critical migration: the history gains 1.0.0 with its backup.
    feed.publish(payload("1.1.0", v2, { schema_version: 18, critical_migration: true }), v2);
    const { c, stateDir } = client(feed, { version: "1.0.0", db: 17 });
    assert.equal((await c.checkAndStage()).state, "staged");
    c.commitInstall(c.pending()!);
    assert.equal(c.pending(), null);
    assert.deepEqual(c.history().map((h) => [h.version, h.schema_version, Boolean(h.backup_ref)]), [["1.0.0", 17, true], ["1.1.0", 18, false]]);

    // The app is now 1.1.0 on a schema-18 database; the feed still serves 1.0.0's signed manifest.
    feed.publish(payload("1.0.0", v1), v1);
    const afterUpdate = new UpdateClient({
      feedUrl: feed.url, stateDir, pinned,
      installed: async () => ({ version: "1.1.0", platform, arch, channel: "stable", dbSchemaVersion: 18 }),
      policy: async () => ({}), backup: async () => 18,
    });
    const s = await afterUpdate.stageRollback();
    assert.equal(s.state, "staged", JSON.stringify(s));
    const p = afterUpdate.pending()!;
    assert.equal(p.version, "1.0.0");
    assert.equal(readFileSync(p.file).toString(), "artifact-1.0.0");
    assert.equal(p.rollback?.restore_backup, join(stateDir, "backups", "pre-1.1.0.db"));
    afterUpdate.commitInstall(p);
    assert.equal(afterUpdate.history().find((h) => h.version === "1.0.0")?.backup_ref !== undefined, true, "rolling back does not lose the backup record");
  });

  it("refuses a rollback when no earlier release can open the database", async () => {
    const { c } = client(feed, { version: "2.0.0", db: 20 });
    const s = await c.stageRollback();
    assert.deepEqual([s.state, s.state === "refused" && s.code], ["refused", "NO_PREVIOUS_RELEASE"]);
    assert(!existsSync(join(c.pending()?.file ?? "/nonexistent")));
  });
});
