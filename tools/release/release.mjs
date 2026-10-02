#!/usr/bin/env node
// M10.2, docs/70: release tooling. Signs and verifies with the same module the
// desktop app verifies with (apps/desktop/src/main/update/manifest.ts), so what
// CI produces is exactly what the app accepts. Node built-ins only.
//
//   keygen    --public-out <update-keys.json> --private-out <file outside the repo>
//   schema-version                       the Core database schema this tree writes
//   checksums <dir> [--out SHA256SUMS]   sha256 of every file, sorted, sha256sum format
//   manifest  --version V --channel C --artifact platform:arch:url:path ... --out F [options]
//   verify    <manifest.json> --keys <update-keys.json> [--artifact-dir dir] [--sbom-dir dir]
//   sbom      --out-dir D                CycloneDX 1.5 JSON for modbit-core, modbit-execd and the desktop
//   licenses  <sbom.json>...             fail on a license outside tools/release/license-policy.json
//
// The signing key comes from the MODBIT_UPDATE_SIGNING_KEY environment variable
// (the CI secret store) or --private-key-file; never from an argument, never
// from the repository.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
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


/** Run a tool and fail the release with its output, never with a guess. */
function run(cmd, args, opts = {}) {
  const r = spawnSync(cmd, args, { encoding: "utf8", maxBuffer: 256 * 1024 * 1024, ...opts });
  if (r.error) fail(`${cmd}: ${r.error.message}`);
  if (r.status !== 0) fail(`${cmd} ${args.join(" ")} exited ${r.status}\n${r.stderr || r.stdout}`);
  return r.stdout;
}

const SHIPPED_RUST = [
  { dir: "services/modbit-core", name: "modbit-core" },
  { dir: "services/modbit-execd", name: "modbit-execd" },
];

/** `cargo cyclonedx` writes one file per workspace member into the tree; keep the shipped binaries' and leave nothing behind. */
function rustSboms(outDir) {
  const generated = [];
  run("cargo", ["cyclonedx", "--manifest-path", join(repoRoot, "Cargo.toml"), "--format", "json", "--spec-version", "1.5", "--describe", "binaries"]);
  const found = [];
  const scan = (dir) => {
    for (const name of readdirSync(dir)) {
      if (["node_modules", "target", ".git"].includes(name)) continue;
      const p = join(dir, name);
      if (statSync(p).isDirectory()) scan(p);
      else if (name.endsWith("_bin.cdx.json")) found.push(p);
    }
  };
  scan(repoRoot);
  try {
    for (const { dir, name } of SHIPPED_RUST) {
      const src = join(repoRoot, dir, `${name}_bin.cdx.json`);
      if (!existsSync(src)) fail(`cargo cyclonedx produced no SBOM for ${name}`);
      const dest = join(outDir, `${name}.cdx.json`);
      copyFileSync(src, dest);
      generated.push(dest);
    }
  } finally {
    for (const f of found) rmSync(f, { force: true });
  }
  return generated;
}

/** pnpm's own CycloneDX output reads pnpm-lock.yaml; the Electron runtime the packager adds is declared beside it. */
function desktopSbom(outDir) {
  const dest = join(outDir, "desktop.cdx.json");
  run("pnpm", ["sbom", "--filter", "@modbit/desktop", "--sbom-format", "cyclonedx", "--sbom-spec-version", "1.5", "--sbom-type", "application", "--prod", "--out", dest], { cwd: repoRoot });
  const bom = JSON.parse(readFileSync(dest, "utf8"));
  const pkg = JSON.parse(readFileSync(join(repoRoot, "apps/desktop/package.json"), "utf8"));
  const electron = pkg.devDependencies?.electron;
  if (!electron) fail("apps/desktop/package.json pins no electron version");
  bom.components = [
    ...(bom.components ?? []),
    {
      type: "framework",
      "bom-ref": `pkg:npm/electron@${electron}`,
      name: "electron",
      version: electron,
      purl: `pkg:npm/electron@${electron}`,
      scope: "required",
      licenses: [{ license: { id: "MIT" } }],
      description: "The runtime the packager copies into every desktop artifact; Chromium and Node.js are inside it.",
    },
  ];
  writeFileSync(dest, `${JSON.stringify(bom, null, 2)}\n`);
  return [dest];
}

/** What makes a document an SBOM worth signing: CycloneDX, with components, each named and versioned. */
function checkSbom(path) {
  let bom;
  try {
    bom = JSON.parse(readFileSync(path, "utf8"));
  } catch (e) {
    fail(`${path} is not JSON: ${e.message}`);
  }
  if (bom.bomFormat !== "CycloneDX") fail(`${path} is not a CycloneDX document`);
  if (!Array.isArray(bom.components) || bom.components.length === 0) fail(`${path} lists no components`);
  const bad = bom.components.find((c) => !c.name || !c.version);
  if (bad) fail(`${path} has a component without a name and version: ${JSON.stringify(bad).slice(0, 120)}`);
  return bom;
}

function licensesOf(component) {
  const out = [];
  for (const l of component.licenses ?? []) {
    if (l.expression) out.push(...l.expression.split(/\s+(?:AND|OR|WITH)\s+|[()]/i).map((t) => t.trim()).filter(Boolean));
    else if (l.license?.id) out.push(l.license.id);
    else if (l.license?.name) out.push(l.license.name);
  }
  return out;
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
  case "sbom": {
    const outDir = resolve(required(flags, "out-dir"));
    mkdirSync(outDir, { recursive: true });
    const files = [...rustSboms(outDir), ...desktopSbom(outDir)];
    for (const f of files) {
      const bom = checkSbom(f);
      console.log(`${f}: CycloneDX ${bom.specVersion}, ${bom.components.length} components, sha256 ${sha256(readFileSync(f))}`);
    }
    break;
  }
  case "licenses": {
    if (positional.length === 0) fail("licenses <sbom.json>...");
    const policy = JSON.parse(readFileSync(join(repoRoot, "tools/release/license-policy.json"), "utf8"));
    const allowed = new Set(policy.allowed);
    // First party is what this workspace builds: its Cargo packages and its npm packages.
    const firstParty = new Set(JSON.parse(run("cargo", ["metadata", "--format-version", "1", "--no-deps"], { cwd: repoRoot })).packages.map((p) => p.name));
    for (const dir of ["packages", "apps"]) {
      for (const n of existsSync(join(repoRoot, dir)) ? readdirSync(join(repoRoot, dir)) : []) {
        const pj = join(repoRoot, dir, n, "package.json");
        if (existsSync(pj)) firstParty.add(String(JSON.parse(readFileSync(pj, "utf8")).name).replace(/^@[^/]+\//, ""));
      }
    }
    const exceptions = new Map(policy.reviewed.map((r) => [`${r.name}`, r]));
    const findings = [];
    for (const file of positional) {
      for (const c of checkSbom(file).components) {
        if (firstParty.has(c.name)) continue;
        const ids = licensesOf(c);
        if (ids.length === 0) {
          if (!exceptions.has(c.name)) findings.push(`${file}: ${c.name}@${c.version} declares no license`);
          continue;
        }
        // An OR expression is satisfied by any allowed alternative; AND and WITH need every term allowed.
        const expr = (c.licenses ?? []).map((l) => l.expression).find(Boolean);
        const ok = expr && /\sOR\s/i.test(expr) && !/\sAND\s/i.test(expr) ? ids.some((i) => allowed.has(i)) : ids.every((i) => allowed.has(i));
        if (!ok && !exceptions.has(c.name)) findings.push(`${file}: ${c.name}@${c.version} license ${ids.join(" / ")} is not in the policy and has no recorded review`);
      }
    }
    if (findings.length > 0) fail(`license gate (docs/36): copyleft and unknown licenses need legal review before merge\n${findings.join("\n")}`);
    console.log(`licenses: ${positional.length} SBOM(s) pass the policy`);
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
      const sbomDir = one(flags, "sbom-dir");
      if (sbomDir !== undefined) {
        if (payload.sbom.length === 0) fail("the manifest lists no SBOM");
        for (const e of payload.sbom) {
          const p = join(sbomDir, e.name);
          if (!existsSync(p)) fail(`SBOM ${e.name} is listed in the manifest but missing from ${sbomDir}`);
          if (sha256(readFileSync(p)) !== e.sha256) fail(`SBOM ${e.name} does not match the digest the manifest signs`);
          checkSbom(p);
        }
      }
      console.log(`manifest for ${payload.version} verifies${dir ? `; ${payload.artifacts.length} artifacts match their signed size and sha256` : ""}${sbomDir ? `; ${payload.sbom.length} SBOMs match their signed digests` : ""}`);
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
    fail("commands: keygen | schema-version | checksums | sbom | licenses | manifest | verify | key-id");
}
