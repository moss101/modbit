// Writes the two files a package pins about its own updates (M10.2): where the
// feed is, and which public keys may sign what the app installs. With no
// MODBIT_UPDATE_KEYS_FILE the key list is empty and the packaged app verifies
// nothing and refuses every update (fail closed); the private key is never
// here, only the public half the owner generated with `release.mjs keygen`.
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";

const out = "package-resources";
mkdirSync(out, { recursive: true });
const keysFile = process.env.MODBIT_UPDATE_KEYS_FILE;
if (keysFile) {
  const { keys } = JSON.parse(readFileSync(keysFile, "utf8"));
  if (!Array.isArray(keys) || keys.some((k) => typeof k?.key_id !== "string" || typeof k?.public_key_pem !== "string" || "private_key_pem" in k)) {
    throw new Error(`${keysFile} is not {keys: [{key_id, public_key_pem}]} (and must hold no private key)`);
  }
  copyFileSync(keysFile, `${out}/update-keys.json`);
} else {
  writeFileSync(`${out}/update-keys.json`, '{ "keys": [] }\n');
}
const config = { channel: process.env.MODBIT_UPDATE_CHANNEL ?? "stable", ...(process.env.MODBIT_UPDATE_FEED_URL ? { feed_url: process.env.MODBIT_UPDATE_FEED_URL } : {}) };
writeFileSync(`${out}/update-config.json`, `${JSON.stringify(config, null, 2)}\n`);
console.log(`package-resources written (${keysFile ? "keys pinned from " + keysFile : "no keys pinned: updates fail closed"})`);
