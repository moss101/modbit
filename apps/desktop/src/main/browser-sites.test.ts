import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { SiteStore, canonicalOrigin, siteOrigin } from "./browser-sites.ts";

const dir = () => mkdtempSync(join(tmpdir(), "px216-"));

test("PX-216: an origin is scheme, host and port as a URL prints it, and only http(s) has one", () => {
  assert.equal(siteOrigin("https://Example.com/a/b?c#d"), "https://example.com");
  assert.equal(siteOrigin("http://127.0.0.1:4100/x"), "http://127.0.0.1:4100");
  assert.equal(siteOrigin("http://127.0.0.1:80/"), "http://127.0.0.1");
  for (const bad of ["file:///etc/passwd", "javascript:alert(1)", "about:blank", "data:text/html,hi", "not a url", ""]) assert.equal(siteOrigin(bad), null, bad);
  // A path, a query or a fragment is not an origin; the stored form is the printed one.
  assert.equal(canonicalOrigin("http://127.0.0.1:4100"), "http://127.0.0.1:4100");
  assert.equal(canonicalOrigin("http://127.0.0.1:4100/"), "http://127.0.0.1:4100");
  assert.equal(canonicalOrigin("http://127.0.0.1:4100/path"), null);
  assert.equal(canonicalOrigin("http://127.0.0.1:*"), null, "an any-port pattern is a startup form only");
  assert.equal(canonicalOrigin("http://127.0.0.1:*", true), "http://127.0.0.1:*");
});

test("PX-216: a decision belongs to one origin: another port, scheme or subdomain is undecided", () => {
  const s = new SiteStore(null);
  s.set("http://127.0.0.1:4100", "allow", 1);
  assert.equal(s.get("http://127.0.0.1:4100")?.decision, "allow");
  for (const other of ["http://127.0.0.1:4101", "https://127.0.0.1:4100", "http://localhost:4100", "http://a.127.0.0.1:4100"]) assert.equal(s.get(other), undefined, other);
});

test("PX-216: a decision replaces the one before it, survives a restart, and removing it makes the origin ask again", () => {
  const file = join(dir(), "browser-sites.json");
  const a = new SiteStore(file);
  a.set("http://site.test:9", "allow", 10);
  a.set("http://site.test:9", "deny", 20);
  a.set("http://other.test:9", "allow", 30);
  const b = new SiteStore(file);
  assert.deepEqual(b.list().map((e) => [e.origin, e.decision]), [["http://site.test:9", "deny"], ["http://other.test:9", "allow"]]);
  assert.equal(b.remove("http://site.test:9"), true);
  assert.equal(b.remove("http://site.test:9"), false, "removing what is not there says so");
  assert.equal(new SiteStore(file).get("http://site.test:9"), undefined);
});

test("PX-216: a file that is not the person's list decides nothing, and an entry that is not an origin is dropped", () => {
  const file = join(dir(), "browser-sites.json");
  writeFileSync(file, "{not json");
  assert.deepEqual(new SiteStore(file).list(), []);
  writeFileSync(file, JSON.stringify({ sites: [{ origin: "http://ok.test", decision: "allow" }, { origin: "http://bad.test/path", decision: "allow" }, { origin: "http://x.test", decision: "maybe" }, { origin: "file:///", decision: "allow" }, 7, null] }));
  assert.deepEqual(new SiteStore(file).list().map((e) => e.origin), ["http://ok.test"]);
  assert.equal(readFileSync(file, "utf8").includes("bad.test"), true, "reading never rewrites the file");
});

test("PX-216: a startup allow names an origin or every port of a host, cannot be removed, and a person's Deny wins over it", () => {
  const s = new SiteStore(null, ["http://127.0.0.1:*", "https://fixed.test:8443", "not an origin", "http://path.test/x"]);
  assert.equal(s.get("http://127.0.0.1:5000")?.decision, "allow");
  assert.equal(s.get("http://127.0.0.1")?.decision, "allow");
  assert.equal(s.get("http://127.0.0.10:5000"), undefined, "a longer host is not the pattern's host");
  assert.equal(s.get("https://fixed.test:8443")?.source, "startup");
  assert.equal(s.get("https://fixed.test:9"), undefined);
  assert.equal(s.get("http://path.test"), undefined);
  assert.equal(s.remove("http://127.0.0.1:5000"), false);
  s.set("http://127.0.0.1:5000", "deny", 1);
  assert.equal(s.get("http://127.0.0.1:5000")?.decision, "deny");
  assert.equal(s.get("http://127.0.0.1:5001")?.decision, "allow");
});

test("PX-216: only an http(s) origin can be decided", () => {
  const s = new SiteStore(null);
  assert.throws(() => s.set("file:///etc", "allow", 1), /BAD_ARGUMENT/);
  assert.throws(() => s.set("nonsense", "deny", 1), /BAD_ARGUMENT/);
});
