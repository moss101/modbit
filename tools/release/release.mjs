#!/usr/bin/env node
// M10.2, docs/70: release tooling. Signs and verifies with the same module the
// desktop app verifies with (apps/desktop/src/main/update/manifest.ts), so what
// CI produces is exactly what the app accepts. Node built-ins only.
//
//   keygen    --public-out <update-keys.json> --private-out <file outside the repo>
//   schema-version                       the Core database schema this tree writes
//   checksums <dir> [--out SHA256SUMS]   sha256 of every file, sorted, sha256sum format
//   manifest  --version V --channel C --artifact platform:arch:url:path ... --out F [options]
//   verify    <manifest.json> --keys <update-keys.json> [--artifact-dir dir]
//
// The signing key comes from the MODBIT_UPDATE_SIGNING_KEY environment variable
// (the CI secret store) or --private-key-file; never from an argument, never
// from the repository.
import { createHash } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { generateUpdateKeyPair, keyIdOf, signManifest, UpdateError, verifyArtifact, verifyManifest } from "../../apps/desktop/src/main/update/manifest.ts";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");

function fail(message) {
  console.error(`release: ${message}`);
  process.exit(1);
}

function parseArgs(argv) {
  const positional = [];
  const flags = new Map();
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a.startsWith("--")) {
      const name = a.slice(2);
      const value = argv[i + 1];
      if (value === undefined || value.startsWith("--")) {
        flags.set(name, [...(flags.get(name) ?? []), true]);
      } else {
        flags.set(name, [...(flags.get(name) ?? []), value]);
        i++;
      }
    } else {
      positional.push(a);
    }
  }
  return { positional, flags };
}

const one = (flags, name, fallback) => {
  const v = flags.get(name);
  if (v === undefined) return fallback;
  if (v.length > 1) fail(`--${name} given more than once`);
  return v[0];
};
const required = (flags, name) => {
  const v = one(flags, name);
  if (v === undefined || v === true) fail(`--${name} is required`);
  return v;
};

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function insideRepo(path) {
  const rel = relative(repoRoot, resolve(path));
  return !(rel === ".." || rel.startsWith(`..${sep}`) || isAbsolute(rel));
}

function walk(dir) {
  const out = [];
  for (const name of readdirSync(dir).sort()) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) out.push(...walk(p));
    else out.push(p);
  }
  return out;
}

function coreSchemaVersions() {
  const src = readFileSync(join(repoRoot, "crates/event-store/src/schema.rs"), "utf8");
  const start = src.indexOf("pub const MIGRATIONS");
  if (start < 0) fail("crates/event-store/src/schema.rs has no MIGRATIONS table");
  const end = src.indexOf("pub const SCHEMA_VERSION", start);
  const versions = [...src.slice(start, end < 0 ? undefined : end).matchAll(/^\s*version:\s*(\d+),/gm)].map((m) => Number(m[1]));
  if (versions.length === 0) fail("no migration versions found in schema.rs");
  return { oldest: Math.min(...versions), newest: Math.max(...versions) };
}

function loadPinned(path) {
  const parsed = JSON.parse(readFileSync(path, "utf8"));
  if (!Array.isArray(parsed.keys)) fail(`${path} is not {"keys": [...]}`);
  return parsed.keys;
}

function signingKey(flags) {
  const file = one(flags, "private-key-file");
  if (file !== undefined) {
    if (insideRepo(file)) fail("the signing key must not live inside the repository");
    return readFileSync(file, "utf8");
  }
  const env = process.env.MODBIT_UPDATE_SIGNING_KEY;
  if (!env) fail("no signing key: set MODBIT_UPDATE_SIGNING_KEY (the CI secret) or pass --private-key-file");
  return env.replace(/\\n/g, "\n");
}

const [command, ...rest] = process.argv.slice(2);
const { positional, flags } = parseArgs(rest);

switch (command) {
  case "keygen": {
    const publicOut = required(flags, "public-out");
    const privateOut = required(flags, "private-out");
    if (insideRepo(privateOut)) fail("--private-out is inside the repository; the private key never lives in the tree");
    if (existsSync(privateOut)) fail(`${privateOut} exists; refusing to overwrite a key`);
    const kp = generateUpdateKeyPair();
    mkdirSync(dirname(resolve(privateOut)), { recursive: true });
    writeFileSync(privateOut, kp.privateKeyPem, { mode: 0o600 });
    chmodSync(privateOut, 0o600);
    const keys = existsSync(publicOut) ? loadPinned(publicOut) : [];
    keys.push({ key_id: kp.keyId, public_key_pem: kp.publicKeyPem });
    mkdirSync(dirname(resolve(publicOut)), { recursive: true });
    writeFileSync(publicOut, `${JSON.stringify({ keys }, null, 2)}\n`);
    console.log(`key ${kp.keyId}: public key pinned in ${publicOut}; private key written to ${privateOut} (mode 0600). Store it as the MODBIT_UPDATE_SIGNING_KEY secret and remove the file.`);
    break;
  }
  case "schema-version":
    console.log(coreSchemaVersions().newest);
    break;
  case "checksums": {
    const dir = positional[0];
    if (!dir) fail("checksums <dir>");
    const outName = one(flags, "out", "SHA256SUMS");
    const outPath = resolve(dir, outName);
    const lines = walk(resolve(dir))
      .filter((p) => p !== outPath)
      .map((p) => `${sha256(readFileSync(p))}  ${relative(resolve(dir), p).split(sep).join("/")}`);
    writeFileSync(outPath, `${lines.join("\n")}\n`);
    console.log(`${lines.length} checksums written to ${outPath}`);
    break;
  }
  case "manifest": {
    const artifacts = (flags.get("artifact") ?? []).map((spec) => {
      const m = /^(macos|windows|linux):([^:]+):(https?:\/\/[^|]+)\|(.+)$/.exec(spec);
      if (!m) fail(`--artifact must be platform:arch:url|path, got ${spec}`);
      const bytes = readFileSync(m[4]);
      return { platform: m[1], arch: m[2], url: m[3], sha256: sha256(bytes), size: bytes.byteLength };
    });
    if (artifacts.length === 0) fail("at least one --artifact is required");
    const sbom = (flags.get("sbom") ?? []).map((p) => ({ name: p.split(/[\\/]/).pop(), sha256: sha256(readFileSync(p)) }));
    const schemas = coreSchemaVersions();
    const schemaVersion = Number(one(flags, "schema-version", schemas.newest));
    const payload = {
      version: required(flags, "version"),
      channel: one(flags, "channel", "stable"),
      published_at: one(flags, "published-at", new Date().toISOString()),
      min_supported_version: one(flags, "min-supported-version", "0.0.0"),
      schema_version: schemaVersion,
      min_db_schema_version: Number(one(flags, "min-db-schema-version", schemas.oldest)),
      critical_migration: flags.has("critical-migration"),
      rollout_percent: Number(one(flags, "rollout-percent", 100)),
      artifacts,
      sbom,
    };
    const signed = signManifest(payload, signingKey(flags));
    const outPath = required(flags, "out");
    writeFileSync(outPath, `${JSON.stringify(signed, null, 2)}\n`);
    console.log(`signed manifest for ${payload.version} (${payload.channel}, ${artifacts.length} artifacts, key ${signed.key_id}) written to ${outPath}`);
    break;
  }
  case "verify": {
    const manifestPath = positional[0];
    if (!manifestPath) fail("verify <manifest.json> --keys <update-keys.json>");
    try {
      const payload = verifyManifest(JSON.parse(readFileSync(manifestPath, "utf8")), loadPinned(required(flags, "keys")));
      const dir = one(flags, "artifact-dir");
      if (dir !== undefined) {
        for (const a of payload.artifacts) {
          const name = decodeURIComponent(new URL(a.url).pathname.split("/").pop());
          verifyArtifact(payload, a.platform, a.arch, readFileSync(join(dir, name)));
        }
      }
      console.log(`manifest for ${payload.version} verifies${dir ? `; ${payload.artifacts.length} artifacts match their signed size and sha256` : ""}`);
    } catch (e) {
      if (e instanceof UpdateError) fail(e.message);
      throw e;
    }
    break;
  }
  case "key-id": {
    const file = positional[0];
    if (!file) fail("key-id <public-key.pem>");
    console.log(keyIdOf(readFileSync(file, "utf8")));
    break;
  }
  default:
    fail("commands: keygen | schema-version | checksums | manifest | verify | key-id");
}
