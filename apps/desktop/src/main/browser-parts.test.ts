import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { CERT_HOLD_MS, CertHold, CertTrustStore, type CertInfo } from "./browser-cert.ts";
import { indexDom, mergeAx, type AxNode, type DomNode } from "./browser-ax.ts";
import { NoticeBatcher, type Notice } from "./browser-observer.ts";
import { PageFeedback, Ring } from "./browser-feedback.ts";

const cert = (over: Partial<CertInfo> = {}): CertInfo => ({ id: "c1", hostPort: "127.0.0.1:8443", url: "https://127.0.0.1:8443/", error: "net::ERR_CERT_AUTHORITY_INVALID", issuer: "CN=fixture", subject: "CN=localhost", validStart: 1, validExpiry: 4_000_000_000, fingerprint: "sha256/AAAA", ...over });

test("PX-073: a certificate error is held for 60 s, settles once, and a hold that runs out is a rejection", async () => {
  assert.equal(CERT_HOLD_MS, 60_000);
  const held = new CertHold(cert(), 30);
  assert.equal(held.isSettled, false);
  assert.equal(await held.settled, "timeout");
  assert.equal(held.decide("trust"), false, "a settled hold stays settled");
  const trusted = new CertHold(cert(), 5_000);
  assert.equal(trusted.decide("trust"), true);
  assert.equal(await trusted.settled, "trust");
  assert.equal(trusted.decide("reject"), false);
});

test("PX-073: a trust is remembered per workspace by host and exact certificate, listed, and cleared", () => {
  const dir = mkdtempSync(join(tmpdir(), "modbit-cert-"));
  try {
    const file = join(dir, "trust.json");
    const store = new CertTrustStore(file);
    assert.equal(store.has("/repo", "127.0.0.1:8443", "sha256/AAAA"), false);
    store.add("/repo", cert());
    assert.equal(store.has("/repo", "127.0.0.1:8443", "sha256/AAAA"), true);
    assert.equal(store.has("/other", "127.0.0.1:8443", "sha256/AAAA"), false, "per workspace");
    assert.equal(store.has("/repo", "127.0.0.1:8443", "sha256/BBBB"), false, "a different certificate asks again");
    assert.equal(store.has("/repo", "127.0.0.1:9", "sha256/AAAA"), false, "per host and port");
    assert.equal(store.list("/repo").length, 1);
    // Survives a restart.
    assert.equal(new CertTrustStore(file).has("/repo", "127.0.0.1:8443", "sha256/AAAA"), true);
    // A new certificate on the same host replaces the old trust.
    store.add("/repo", cert({ fingerprint: "sha256/BBBB" }));
    assert.deepEqual(store.list().map((t) => t.fingerprint), ["sha256/BBBB"]);
    assert.equal(store.clear("/other"), 0);
    assert.equal(store.clear("/repo"), 1);
    assert.equal(store.has("/repo", "127.0.0.1:8443", "sha256/BBBB"), false, "cleared means cleared");
    assert.equal(new CertTrustStore(file).list().length, 0, "and stays cleared after a restart");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("PX-122: a page that mutates thousands of times a second leaves as a handful of bounded notices with a monotonic counter", async () => {
  const out: Notice[] = [];
  const b = new NoticeBatcher((n) => out.push(n), 40);
  const t0 = Date.now();
  while (Date.now() - t0 < 200) {
    for (let i = 0; i < 500; i++) b.report({ added: 1, coalesced: 3 });
    await new Promise((r) => setTimeout(r, 1));
  }
  await new Promise((r) => setTimeout(r, 120));
  b.stop();
  assert.ok(b.reports > 1000, `${b.reports} reports`);
  assert.ok(out.length <= 8, `${out.length} notices for ${b.reports} reports in 200 ms`);
  assert.ok(out.length >= 2);
  assert.deepEqual(out.map((n) => n.change_seq), [...out.map((n) => n.change_seq)].sort((a, c) => a - c), "the counter only goes up");
  assert.equal(out.reduce((s, n) => s + n.added, 0), b.reports, "nothing is lost, only folded");
  assert.equal(out.at(-1)!.change_seq, b.changeSeq);
  // A navigation outranks a mutation in the kind.
  const o2: Notice[] = [];
  const b2 = new NoticeBatcher((n) => o2.push(n), 10);
  b2.report({ added: 1 });
  b2.report({ kind: "navigation" });
  b2.flush();
  assert.equal(o2[0]!.kind, "navigation");
});

test("PX-121: the console and network buffers are bounded, redacted, sequence-numbered and say what fell off", () => {
  const f = new PageFeedback();
  f.secrets = () => ["BROKER-VALUE-9"];
  f.ingest("Runtime.consoleAPICalled", { type: "error", args: [{ type: "string", value: "boom token=abcdefabcdef BROKER-VALUE-9" }], stackTrace: { callFrames: [{ url: "http://x.test/app.js?api_key=K", lineNumber: 4 }] } });
  f.ingest("Runtime.exceptionThrown", { exceptionDetails: { text: "Uncaught", exception: { description: "TypeError: x is not a function" }, url: "http://x.test/a.js", lineNumber: 9 } });
  f.ingest("Log.entryAdded", { entry: { level: "warning", text: "mixed content", url: "http://x.test/" } });
  const c = f.console.read(0, 100);
  assert.equal(c.entries.length, 3);
  assert.equal(c.entries[0]!.level, "error");
  assert.ok(!c.entries[0]!.text.includes("abcdefabcdef") && !c.entries[0]!.text.includes("BROKER-VALUE-9"), c.entries[0]!.text);
  assert.ok(!(c.entries[0]!.url ?? "").includes("api_key=K"), c.entries[0]!.url ?? "");
  assert.equal(c.entries[1]!.source, "exception");
  assert.equal(c.entries[0]!.line, 5);
  // Resuming after a sequence number.
  assert.deepEqual(f.console.read(c.entries[1]!.seq, 100).entries.map((e) => e.level), ["warning"]);
  // The network: a failed request, a redirect, a finished one; credentials never kept.
  f.ingest("Network.requestWillBeSent", { requestId: "1", type: "XHR", request: { url: "https://ada:pw@api.test/items?token=zzz&x=1", method: "POST", headers: { Authorization: "Bearer secret" } } });
  assert.equal(f.inflight, 1);
  f.ingest("Network.loadingFailed", { requestId: "1", errorText: "net::ERR_CONNECTION_REFUSED" });
  f.ingest("Network.requestWillBeSent", { requestId: "2", type: "Document", request: { url: "http://x.test/", method: "GET" } });
  f.ingest("Network.responseReceived", { requestId: "2", response: { status: 200 } });
  f.ingest("Network.loadingFinished", { requestId: "2" });
  const n = f.network.read(0, 100).entries;
  assert.equal(n.length, 2);
  assert.equal(n[0]!.url, "https://[REDACTED]@api.test/items?token=[REDACTED]&x=1");
  assert.equal(n[0]!.failed, "net::ERR_CONNECTION_REFUSED");
  assert.equal(n[1]!.status, 200);
  assert.equal(f.inflight, 0);
  assert.ok(!JSON.stringify(n).includes("Bearer") && !JSON.stringify(n).includes("secret"), "headers are never recorded");
  // A bounded ring: the oldest fall off and the read says so.
  const r = new Ring<{ seq: number }>(3);
  for (let i = 0; i < 10; i++) r.push((seq) => ({ seq }));
  const read = r.read(0, 100);
  assert.deepEqual(read.entries.map((e) => e.seq), [8, 9, 10]);
  assert.equal(read.dropped, 7);
});

test("PX-122/123: the DOM walk gives controls a stable identity, their form, their destination and their type; the merge keeps the landmark path through ignored wrappers", () => {
  const el = (backend: number, name: string, attrs: string[], children: DomNode[] = [], extra: Partial<DomNode> = {}): DomNode => ({ nodeType: 1, nodeName: name.toUpperCase(), localName: name, backendNodeId: backend, attributes: attrs, children, ...extra });
  const doc: DomNode = {
    nodeType: 9,
    nodeName: "#document",
    backendNodeId: 1,
    documentURL: "https://app.test/login",
    baseURL: "https://app.test/login",
    children: [
      el(2, "html", [], [
        el(3, "body", [], [
          el(4, "form", ["action", "/session", "method", "POST"], [
            el(5, "input", ["id", "email", "type", "email", "autocomplete", "username", "required", ""]),
            el(6, "input", ["name", "pw", "type", "password", "autocomplete", "current-password", "placeholder", "secret"]),
            el(7, "button", ["type", "submit"]),
            el(8, "button", ["type", "button", "data-testid", "help"]),
          ]),
          el(9, "a", ["href", "../out?next=1"]),
          el(10, "iframe", ["src", "/inner"], [], { frameId: "FRAME2" }),
        ]),
      ]),
    ],
  };
  const idx = indexDom(doc, "TOP");
  const email = idx.byBackend.get(5)!;
  assert.deepEqual([email.ident, email.type, email.autocomplete, email.required, email.formKey, email.formAction, email.formMethod], ["id:email", "email", "username", true, "idx:0", "https://app.test/session", "post"]);
  assert.equal(idx.byBackend.get(6)!.ident, "name:pw");
  assert.equal(idx.byBackend.get(6)!.placeholder, "secret");
  assert.equal(idx.byBackend.get(7)!.submit, true);
  assert.equal(idx.byBackend.get(8)!.submit, false);
  assert.equal(idx.byBackend.get(8)!.ident, "testid:help", "a test id outranks nothing else here");
  assert.equal(idx.byBackend.get(9)!.href, "https://app.test/out?next=1");
  assert.deepEqual(idx.owners.get("FRAME2"), { ownerBackendId: 10, inFrameId: "TOP" });
  const ax: AxNode[] = [
    { nodeId: "1", role: { value: "RootWebArea" }, name: { value: "Login" }, backendDOMNodeId: 1 },
    { nodeId: "2", role: { value: "main" }, parentId: "1" },
    { nodeId: "3", role: { value: "generic" }, parentId: "2", ignored: true },
    { nodeId: "4", role: { value: "form" }, name: { value: "Login" }, parentId: "3", backendDOMNodeId: 4 },
    { nodeId: "5", role: { value: "textbox" }, name: { value: "Email" }, parentId: "4", backendDOMNodeId: 5, properties: [{ name: "required", value: { value: true } }, { name: "invalid", value: { value: "false" } }] },
    { nodeId: "6", role: { value: "button" }, name: { value: "Sign in" }, parentId: "4", backendDOMNodeId: 7 },
    { nodeId: "7", role: { value: "link" }, name: { value: "Out" }, parentId: "2", backendDOMNodeId: 9, properties: [{ name: "url", value: { value: "https://app.test/out?next=1" } }] },
    { nodeId: "8", role: { value: "checkbox" }, name: { value: "Remember" }, parentId: "4", properties: [{ name: "checked", value: { value: "mixed" } }, { name: "expanded", value: { value: false } }] },
  ];
  const merged = mergeAx(ax, { prefix: "", limit: 100, dom: idx });
  const byName = Object.fromEntries(merged.nodes.map((n) => [n.name || n.role, n]));
  assert.equal(byName["Login"]!.parent, "2", "the ignored wrapper is skipped: the form hangs under main");
  assert.equal(byName["Email"]!.dom_ident, "id:email");
  assert.equal(byName["Email"]!.form_key, "idx:0");
  assert.equal(byName["Email"]!.required, true);
  assert.equal(byName["Email"]!.invalid, undefined, "invalid=false is not invalid");
  assert.equal(byName["Sign in"]!.submit, true);
  assert.equal(byName["Out"]!.href, "https://app.test/out?next=1");
  assert.equal(byName["Remember"]!.checked, "mixed");
  assert.equal(byName["Remember"]!.expanded, false);
  // A sub-frame: ids are prefixed, the root hangs under the iframe's AX node and is an `iframe`.
  const sub = mergeAx([{ nodeId: "1", role: { value: "RootWebArea" }, name: { value: "" } }, { nodeId: "2", role: { value: "button" }, name: { value: "InnerBtn" }, parentId: "1", backendDOMNodeId: 5 }], { prefix: "f1:", frame: "f1", limit: 50, dom: { byBackend: new Map(), owners: new Map(), walked: 0 }, rootParent: "7", rootRole: "iframe", rootName: "http://other.test" });
  assert.deepEqual(sub.nodes.map((n) => [n.id, n.parent, n.role, n.frame]), [["f1:1", "7", "iframe", "f1"], ["f1:2", "f1:1", "button", "f1"]]);
  // The bound: the host says it cut the tree.
  assert.equal(mergeAx(ax, { prefix: "", limit: 3, dom: idx }).truncated, true);
});
