import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  CDP_DENIED_DOMAINS,
  CDP_DENIED_METHODS,
  CdpDenied,
  addressAllowed,
  cdpDenied,
  checkOrigin,
  checkUrlLiteral,
  classifyIp,
  classifyName,
  guardCdp,
  parseOriginRule,
  parsePolicy,
  parseRule,
  redactText,
  redactUrl,
} from "./browser-policy.ts";

const rules = (...r: string[]) => r.map((x) => parseRule(x)!);

test("PX-120: addresses are classified, IPv4-embedded IPv6 and every spelling of the metadata endpoint included", () => {
  const cases: [string, string][] = [
    ["127.0.0.1", "loopback"],
    ["127.9.9.9", "loopback"],
    ["::1", "loopback"],
    ["10.1.2.3", "private"],
    ["172.16.0.1", "private"],
    ["172.31.255.255", "private"],
    ["172.32.0.1", "public"],
    ["192.168.1.1", "private"],
    ["169.254.169.254", "metadata"],
    ["169.254.1.1", "link_local"],
    ["fe80::1", "link_local"],
    ["fd00:ec2::254", "metadata"],
    ["fc00::1", "unique_local"],
    ["fd12:3456::1", "unique_local"],
    ["100.64.0.1", "carrier_grade"],
    ["100.128.0.1", "public"],
    ["224.0.0.1", "multicast"],
    ["ff02::1", "multicast"],
    ["0.0.0.0", "unspecified"],
    ["::", "unspecified"],
    ["::ffff:127.0.0.1", "loopback"],
    ["::ffff:7f00:1", "loopback"],
    ["::ffff:169.254.169.254", "metadata"],
    ["64:ff9b::a00:1", "private"],
    ["2002:c0a8:0101::1", "private"],
    ["8.8.8.8", "public"],
    ["2606:4700:4700::1111", "public"],
    ["not an address", "reserved"],
  ];
  for (const [ip, want] of cases) assert.equal(classifyIp(ip), want, ip);
  assert.equal(classifyName("localhost"), "loopback");
  assert.equal(classifyName("app.localhost"), "loopback");
  assert.equal(classifyName("metadata.google.internal"), "metadata");
  assert.equal(classifyName("example.com"), "public");
});

test("PX-120: a literal destination is refused unless the policy names the host, the address or the range; every spelling of an address is the address", () => {
  const none = rules();
  for (const url of ["http://127.0.0.1:3000/", "http://localhost/", "http://[::1]:8080/", "http://10.0.0.5/", "http://192.168.0.1/admin", "http://169.254.169.254/latest/meta-data/", "http://metadata.google.internal/", "http://[::ffff:169.254.169.254]/", "http://2130706433/", "http://0x7f.1/", "http://0177.0.0.1/", "http://foo.localhost:3000/", "http://[fd00:ec2::254]/"]) {
    const v = checkUrlLiteral(none, url);
    assert.equal(v.ok, false, url);
    assert.equal(v.ok === false && v.code, "TARGET_NOT_ALLOWED", url);
  }
  assert.equal(checkUrlLiteral(none, "https://example.com/").ok, true);
  assert.equal(checkUrlLiteral(none, "http://93.184.216.34/").ok, true);
  // Named by host, by address, by address with a port, by range.
  assert.equal(checkUrlLiteral(rules("localhost"), "http://localhost:3000/").ok, true);
  assert.equal(checkUrlLiteral(rules("127.0.0.1"), "http://127.0.0.1:3000/").ok, true);
  assert.equal(checkUrlLiteral(rules("127.0.0.1"), "http://localhost:3000/").ok, false, "naming the address does not name the other spelling of it");
  assert.equal(checkUrlLiteral(rules("127.0.0.1:3000"), "http://127.0.0.1:3000/").ok, true);
  assert.equal(checkUrlLiteral(rules("127.0.0.1:3000"), "http://127.0.0.1:3001/").ok, false);
  assert.equal(checkUrlLiteral(rules("10.0.0.0/8"), "http://10.20.30.40/").ok, true);
  assert.equal(checkUrlLiteral(rules("10.0.0.0/8"), "http://11.0.0.1/").ok, true, "public anyway");
  assert.equal(checkUrlLiteral(rules("10.0.0.0/8"), "http://192.168.0.1/").ok, false);
  assert.equal(checkUrlLiteral(rules("*.localhost"), "http://app.localhost/").ok, true);
  // The metadata endpoint is not a range a policy names by accident.
  assert.equal(checkUrlLiteral(rules("10.0.0.0/8", "192.168.0.0/16", "127.0.0.1"), "http://169.254.169.254/").ok, false);
});

test("PX-120: the connection check is on the address connected to — a name that resolves privately is refused whatever its name says", () => {
  // The name says nothing; the addresses do.
  const r = rules();
  const v = addressAllowed(r, "innocent.example", 80, "127.0.0.1");
  assert.equal(v.ok, false);
  assert.match(v.ok ? "" : v.reason, /innocent\.example resolves to 127\.0\.0\.1/);
  assert.equal(addressAllowed(r, "innocent.example", 80, "93.184.216.34").ok, true);
  // Naming the host names its addresses (the person decided); naming another host does not.
  assert.equal(addressAllowed(rules("innocent.example"), "innocent.example", 80, "10.0.0.7").ok, true);
  assert.equal(addressAllowed(rules("other.example"), "innocent.example", 80, "10.0.0.7").ok, false);
});

test("PX-120: a rule that is not a rule is dropped, never widened; the policy parses defensively", () => {
  for (const bad of ["", " ", "a b", "http://x", "10.0.0.0/33", "*.", "host:99999", "%%%", "10.0.0.0/8/2"]) assert.equal(parseRule(bad), null, bad);
  const p = parsePolicy({ allow_targets: ["127.0.0.1", "nonsense rule", 5, "10.0.0.0/8"], allow_origins: ["https://app.test", "also bad origin"] });
  assert.deepEqual(p.allowTargets, ["127.0.0.1", "10.0.0.0/8"]);
  assert.deepEqual(p.allowOrigins, ["https://app.test"]);
  assert.deepEqual(parsePolicy("garbage").allowTargets, []);
});

test("PX-073: the per-call origin gate — file: never, unknown closed, the allow-list when configured", () => {
  const none = (u: string) => checkOrigin([], [], u);
  assert.deepEqual(none("about:blank"), { ok: true, origin: "about:blank" });
  assert.equal(none("http://127.0.0.1:3000/a").ok, true);
  const file = none("file:///etc/hosts");
  assert.equal(file.ok === false && file.code, "FILE_ORIGIN");
  for (const u of ["chrome-error://chromewebdata/", "data:text/html,hi", "javascript:alert(1)", "blob:http://x/1", "not a url"]) {
    const v = none(u);
    assert.equal(v.ok === false && v.code, "ORIGIN_NOT_ALLOWED", u);
  }
  const allowed = ["https://app.test", "127.0.0.1:3000", "*.corp.test"];
  const rs = allowed.map((a) => parseOriginRule(a)!);
  assert.equal(checkOrigin(rs, allowed, "https://app.test/x").ok, true);
  assert.equal(checkOrigin(rs, allowed, "http://app.test/x").ok, false, "the scheme counts when the rule names one");
  assert.equal(checkOrigin(rs, allowed, "http://127.0.0.1:3000/").ok, true);
  assert.equal(checkOrigin(rs, allowed, "http://127.0.0.1:3001/").ok, false);
  assert.equal(checkOrigin(rs, allowed, "https://a.b.corp.test/").ok, true);
  assert.equal(checkOrigin(rs, allowed, "https://corp.test/").ok, false, "a suffix rule is for subdomains");
  const refused = checkOrigin(rs, allowed, "https://evil.test/");
  assert.equal(refused.ok === false && refused.code, "ORIGIN_NOT_ALLOWED");
  assert.deepEqual(refused.ok === false && refused.allowed, allowed, "the refusal lists what is allowed");
  // file: is refused even when the allow-list would otherwise admit everything.
  assert.equal(checkOrigin(rs, allowed, "file:///x").ok === false && true, true);
});

test("PX-073: the CDP deny list covers the domains and methods, only the host's structural methods pass, and it matches the Rust list", () => {
  for (const m of ["Browser.close", "Input.dispatchKeyEvent", "Storage.clearDataForOrigin", "SystemInfo.getInfo", "Target.createTarget", "Tethering.bind", "Network.getAllCookies", "Network.setCookie", "Network.clearBrowserCache", "DOM.setFileInputFiles", "Page.navigate", "Page.getNavigationHistory"]) {
    assert.equal(cdpDenied(m), true, m);
    assert.throws(() => guardCdp(m, false), CdpDenied, m);
  }
  for (const m of ["Accessibility.getFullAXTree", "DOM.getDocument", "Runtime.callFunctionOn", "Page.getFrameTree", "Network.enable", "Page.captureScreenshot"]) {
    assert.equal(cdpDenied(m), false, m);
    assert.doesNotThrow(() => guardCdp(m, false), m);
  }
  // The host's own dispatch may use Input and the auto-attach; nothing else of a denied domain, and never a denied method.
  assert.doesNotThrow(() => guardCdp("Input.dispatchMouseEvent", true));
  assert.doesNotThrow(() => guardCdp("Target.setAutoAttach", true));
  assert.throws(() => guardCdp("Target.createTarget", true), CdpDenied);
  assert.throws(() => guardCdp("Browser.close", true), CdpDenied);
  assert.throws(() => guardCdp("Network.setCookie", true), CdpDenied);
  // One list in two languages: the Rust source names exactly the domains and methods listed here.
  const rust = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "../../../../crates/browser/src/refusal.rs"), "utf8");
  const listOf = (name: string): string[] => {
    const m = new RegExp(`pub const ${name}: &\\[&str\\] = &\\[([\\s\\S]*?)\\];`).exec(rust);
    assert.ok(m, `${name} is in refusal.rs`);
    return [...m[1]!.matchAll(/"([^"]+)"/g)].map((x) => x[1]!);
  };
  assert.deepEqual([...CDP_DENIED_DOMAINS].sort(), listOf("CDP_DENIED_DOMAINS").sort());
  assert.deepEqual([...CDP_DENIED_METHODS].sort(), listOf("CDP_DENIED_METHODS").sort());
});

test("PX-121: URLs and console lines lose credentials, tokens and held secrets before they leave the host", () => {
  assert.equal(redactUrl("https://ada:hunter2@api.test/v1?id=7&token=abc&page=2#access_token=zzz&x=1"), "https://[REDACTED]@api.test/v1?id=7&token=[REDACTED]&page=2#access_token=[REDACTED]&x=1");
  assert.equal(redactUrl("http://127.0.0.1:3000/a?q=hello"), "http://127.0.0.1:3000/a?q=hello");
  const line = redactText("fetch failed Authorization: Bearer eyJhbGciOi.payload.sig for https://u:p@x.test/a?api_key=K1 key=sk-live-1234567890abcdef and BROKER-VALUE-9 too", ["BROKER-VALUE-9"]);
  for (const leak of ["eyJhbGciOi", "sk-live", "BROKER-VALUE-9", "u:p@", "api_key=K1"]) assert.ok(!line.includes(leak), `${leak} in ${line}`);
  assert.ok(line.includes("fetch failed"));
  assert.equal(redactText("Uncaught TypeError: x is not a function"), "Uncaught TypeError: x is not a function");
  assert.ok(redactText("y ".repeat(600)).length <= 501);
});
