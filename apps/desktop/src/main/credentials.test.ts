import assert from "node:assert/strict";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { CredentialStore, originOf } from "./credentials.ts";

/** A reversible stand-in for safeStorage: the store's rules are what is under test. */
const fake = { available: () => true, encrypt: (p: string) => `enc:${Buffer.from(p).toString("base64")}`, decrypt: (c: string) => Buffer.from(c.slice(4), "base64").toString() };

test("M7.8: a credential binds to an exact http(s) origin; a handle goes out, the secret only to a fill at that origin", () => {
  const store = new CredentialStore(join(mkdtempSync(join(tmpdir(), "modbit-cred-")), "credentials.enc"), fake);
  const c = store.add("Fixture login", "https://app.test:8443", "ada@example.test", "hunter2");
  assert.match(c.handle, /^cred_[0-9a-f]{12}$/);
  assert.deepEqual({ label: c.label, origin: c.origin, username: c.username, persisted: c.persisted }, { label: "Fixture login", origin: "https://app.test:8443", username: "ada@example.test", persisted: true });
  assert.ok(!JSON.stringify(store.list()).includes("hunter2"));
  assert.equal(store.secretFor(c.handle, "https://app.test:8443"), "hunter2");
  assert.equal(store.secretFor(c.handle, "https://evil.test"), null);
  assert.equal(store.secretFor(c.handle, "http://app.test:8443"), null);
  assert.equal(store.secretFor("cred_000000000000", "https://app.test:8443"), null);
  assert.throws(() => store.add("x", "https://app.test/login", "u", "s"), /BAD_ORIGIN/);
  assert.throws(() => store.add("x", "file:///etc", "u", "s"), /BAD_ORIGIN/);
  assert.throws(() => store.add("x", "https://app.test", "u", ""), /BAD_SECRET/);
  assert.equal(store.remove(c.handle), true);
  assert.equal(store.remove(c.handle), false);
  assert.deepEqual(store.list(), []);
});

test("M7.8: without OS encryption the secret stays in memory only, and the handle says so", () => {
  const store = new CredentialStore(join(mkdtempSync(join(tmpdir(), "modbit-cred-")), "credentials.enc"), { ...fake, available: () => false });
  const c = store.add("Local", "http://127.0.0.1:3000", "u", "s3cret-value");
  assert.equal(c.persisted, false);
  assert.deepEqual(store.list(), [{ handle: c.handle, label: "Local", origin: "http://127.0.0.1:3000", username: "u", persisted: false }]);
  assert.equal(store.secretFor(c.handle, "http://127.0.0.1:3000"), "s3cret-value");
});

test("M7.8: an origin is scheme, host and port only", () => {
  assert.equal(originOf("https://App.Test:8443/login?x=1#f"), "https://app.test:8443");
  assert.equal(originOf("http://127.0.0.1:3000/"), "http://127.0.0.1:3000");
  assert.equal(originOf("file:///etc/hosts"), null);
  assert.equal(originOf("nope"), null);
});
