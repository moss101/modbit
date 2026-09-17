/**
 * M7.8 (docs/22 "Credentials"): the credential broker. A login credential
 * the person adds lives in Electron main's custody — the secret encrypted
 * by `safeStorage` (Keychain / DPAPI / libsecret) and only its ciphertext
 * on disk, under the app's data directory with owner-only permissions —
 * bound to one origin. What leaves this module is a handle (`cred_…`), a
 * label, the origin and the account name: the Core and the model see
 * those; the renderer sees those; the value is read back only by
 * `BrowserHost` to fill a field of a page at the bound origin, and it is
 * never logged, returned or put on the wire.
 */
import { randomBytes } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";

export type CredentialHandle = { handle: string; label: string; origin: string; username: string; persisted: boolean };
type StoredCredential = { handle: string; label: string; origin: string; username: string; secretCiphertext: string };

/** `scheme://host[:port]`, lower-case, of an http(s) URL; null otherwise. */
export function originOf(url: string): string | null {
  try {
    const u = new URL(url);
    if (u.protocol !== "http:" && u.protocol !== "https:") return null;
    return `${u.protocol}//${u.host}`.toLowerCase();
  } catch {
    return null;
  }
}

export type SecretCrypto = { available(): boolean; encrypt(plain: string): string; decrypt(cipher: string): string };

export class CredentialStore {
  /** Ciphertext records on disk; secrets in memory only when the OS offers no encryption. */
  private readonly memoryOnly = new Map<string, string>();
  private readonly memoryMeta = new Map<string, { label: string; origin: string; username: string }>();
  private readonly file: string;
  private readonly crypto: SecretCrypto;

  constructor(file: string, crypto: SecretCrypto) {
    this.file = file;
    this.crypto = crypto;
  }

  private load(): StoredCredential[] {
    try {
      return existsSync(this.file) ? (JSON.parse(readFileSync(this.file, "utf8")) as StoredCredential[]) : [];
    } catch {
      return [];
    }
  }

  private save(records: StoredCredential[]): void {
    writeFileSync(this.file, JSON.stringify(records), { mode: 0o600 });
  }

  /** The handles: never a secret. */
  list(): CredentialHandle[] {
    const stored = this.load().map((r) => ({ handle: r.handle, label: r.label, origin: r.origin, username: r.username, persisted: true }));
    const memory = [...this.memoryOnly.keys()].map((handle) => {
      const meta = this.memoryMeta.get(handle)!;
      return { handle, ...meta, persisted: false };
    });
    return [...stored, ...memory];
  }

  /** Add a credential bound to `origin` (an http(s) origin, exactly). The
   *  secret crosses this call once. */
  add(label: string, origin: string, username: string, secret: string): CredentialHandle {
    const bound = originOf(origin);
    if (bound === null || bound !== origin.trim().toLowerCase()) throw new Error(`BAD_ORIGIN: ${origin} is not an http(s) origin (scheme://host[:port])`);
    if (!label.trim()) throw new Error("BAD_LABEL: a label is required");
    if (!secret) throw new Error("BAD_SECRET: a secret is required");
    const handle = `cred_${randomBytes(6).toString("hex")}`;
    if (this.crypto.available()) {
      const records = this.load();
      records.push({ handle, label: label.trim(), origin: bound, username, secretCiphertext: this.crypto.encrypt(secret) });
      this.save(records);
      return { handle, label: label.trim(), origin: bound, username, persisted: true };
    }
    this.memoryOnly.set(handle, secret);
    this.memoryMeta.set(handle, { label: label.trim(), origin: bound, username });
    return { handle, label: label.trim(), origin: bound, username, persisted: false };
  }

  /** Forget a credential. */
  remove(handle: string): boolean {
    if (this.memoryOnly.delete(handle)) {
      this.memoryMeta.delete(handle);
      return true;
    }
    const records = this.load();
    const kept = records.filter((r) => r.handle !== handle);
    if (kept.length === records.length) return false;
    this.save(kept);
    return true;
  }

  /** The handle's metadata, or null. */
  get(handle: string): CredentialHandle | null {
    return this.list().find((c) => c.handle === handle) ?? null;
  }

  /** The secret, for the host's fill only — and only when the page's origin
   *  is the bound one. Null when unknown, undecryptable or unbound. */
  secretFor(handle: string, pageOrigin: string): string | null {
    const memory = this.memoryOnly.get(handle);
    if (memory !== undefined) return this.memoryMeta.get(handle)?.origin === pageOrigin ? memory : null;
    const rec = this.load().find((r) => r.handle === handle);
    if (!rec || rec.origin !== pageOrigin) return null;
    try {
      return this.crypto.available() ? this.crypto.decrypt(rec.secretCiphertext) : null;
    } catch {
      return null;
    }
  }
}
