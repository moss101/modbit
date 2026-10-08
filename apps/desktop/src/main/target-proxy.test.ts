import { test } from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import net from "node:net";
import { parseRule } from "./browser-policy.ts";
import { TargetProxy, type Denial, type Resolver } from "./target-proxy.ts";

async function origin(): Promise<{ server: http.Server; port: number; hits: string[] }> {
  const hits: string[] = [];
  const server = http.createServer((req, res) => {
    hits.push(`${req.headers.host}${req.url}`);
    res.writeHead(200, { "content-type": "text/plain" }).end("origin says hi");
  });
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
  return { server, port: (server.address() as net.AddressInfo).port, hits };
}

/** A request the way a browser sends it to a forward proxy: the absolute URI on the proxy's socket. */
function viaProxy(proxyPort: number, url: string): Promise<{ status: number; body: string; blocked: string | undefined }> {
  return new Promise((resolve, reject) => {
    const u = new URL(url);
    const req = http.request({ host: "127.0.0.1", port: proxyPort, method: "GET", path: url, headers: { host: u.host }, agent: false }, (res) => {
      let body = "";
      res.on("data", (c) => (body += c));
      res.on("end", () => resolve({ status: res.statusCode ?? 0, body, blocked: res.headers["x-modbit-blocked"] as string | undefined }));
    });
    req.on("error", reject);
    req.end();
  });
}

test("PX-120: a request to the local machine is refused at the proxy and never reaches the server; naming the address lets it through", async () => {
  const o = await origin();
  const denials: Denial[] = [];
  let rules = [parseRule("10.0.0.0/8")!];
  const proxy = new TargetProxy({ rules: () => rules, onDenied: (d) => denials.push(d) });
  const port = await proxy.listen();
  try {
    const refused = await viaProxy(port, `http://127.0.0.1:${o.port}/secret`);
    assert.equal(refused.status, 403);
    assert.equal(refused.blocked, "TARGET_NOT_ALLOWED");
    assert.deepEqual(o.hits, [], "the server never saw the request");
    assert.equal(denials.length, 1);
    assert.equal(denials[0]!.class, "loopback");
    assert.equal(denials[0]!.url, `http://127.0.0.1:${o.port}/secret`);
    // *.localhost resolves to loopback by definition; the hostname alone says nothing to a check on names.
    const viaName = await viaProxy(port, `http://rebind.localhost:${o.port}/secret`);
    assert.equal(viaName.status, 403);
    assert.deepEqual(o.hits, []);
    // The person names the address.
    rules = [parseRule("127.0.0.1")!];
    const ok = await viaProxy(port, `http://127.0.0.1:${o.port}/page`);
    assert.equal(ok.status, 200);
    assert.equal(ok.body, "origin says hi");
    assert.deepEqual(o.hits, [`127.0.0.1:${o.port}/page`]);
  } finally {
    proxy.close();
    o.server.close();
  }
});

test("PX-120: the address checked is the address connected to — one resolution, one connection, no second look", async () => {
  const o = await origin();
  const denials: Denial[] = [];
  const connected: string[] = [];
  let calls = 0;
  // An attacker's resolver: public the first time it is asked, loopback every time after.
  const resolve: Resolver = async () => {
    calls += 1;
    return calls === 1 ? [{ address: "127.0.0.1", family: 4 }] : [{ address: "10.0.0.9", family: 4 }];
  };
  const proxy = new TargetProxy({ rules: () => [parseRule("rebind.test")!], resolve, onDenied: (d) => denials.push(d), onConnect: (_h, a) => connected.push(a) });
  const port = await proxy.listen();
  try {
    // The host is named, so the address it resolves to is accepted - once, and the connection goes to that address.
    const r = await viaProxy(port, `http://rebind.test:${o.port}/x`);
    assert.equal(r.status, 200);
    assert.equal(calls, 1, "resolved once");
    assert.deepEqual(connected, ["127.0.0.1"], "connected to the address that was checked");
  } finally {
    proxy.close();
    o.server.close();
  }
});

test("PX-120: of several answers, only an allowed address is connected to; a name with none is refused", async () => {
  const denials: Denial[] = [];
  const answers: Record<string, { address: string; family: number }[]> = {
    "mixed.test": [{ address: "10.0.0.5", family: 4 }, { address: "93.184.216.34", family: 4 }],
    "private.test": [{ address: "10.0.0.5", family: 4 }, { address: "192.168.1.9", family: 4 }],
  };
  const proxy = new TargetProxy({ rules: () => [], resolve: async (h) => answers[h] ?? [], onDenied: (d) => denials.push(d) });
  try {
    const mixed = await proxy.pin("mixed.test", 80);
    assert.deepEqual(mixed, { ok: true, address: "93.184.216.34", family: 4 }, "the private answer is never the one connected to");
    const priv = await proxy.pin("private.test", 80);
    assert.equal(priv.ok, false);
    assert.equal(priv.ok === false && priv.denial?.class, "private");
    assert.equal(denials.length, 1);
    // A name that does not resolve is a failed request, not a policy refusal.
    const gone = await proxy.pin("nowhere.test", 80);
    assert.deepEqual(gone, { ok: false });
    assert.equal(denials.length, 1);
  } finally {
    proxy.close();
  }
});

test("PX-120: a CONNECT tunnel to a denied address is refused with 403 and an allowed one is piped", async () => {
  const echo = net.createServer((s) => s.on("data", (d) => s.write(`echo:${d}`)));
  await new Promise<void>((r) => echo.listen(0, "127.0.0.1", r));
  const echoPort = (echo.address() as net.AddressInfo).port;
  const denials: Denial[] = [];
  let rules = [] as ReturnType<typeof parseRule>[];
  const proxy = new TargetProxy({ rules: () => rules.filter(Boolean) as never, onDenied: (d) => denials.push(d) });
  const port = await proxy.listen();
  const connect = (target: string): Promise<{ head: string; sock: net.Socket }> =>
    new Promise((resolve, reject) => {
      const s = net.connect(port, "127.0.0.1", () => s.write(`CONNECT ${target} HTTP/1.1\r\nHost: ${target}\r\n\r\n`));
      let buf = "";
      s.on("data", (d) => {
        buf += d.toString();
        if (buf.includes("\r\n\r\n")) resolve({ head: buf.split("\r\n")[0]!, sock: s });
      });
      s.on("error", reject);
    });
  try {
    const refused = await connect(`127.0.0.1:${echoPort}`);
    assert.match(refused.head, /403/);
    refused.sock.destroy();
    assert.equal(denials[0]?.class, "loopback");
    rules = [parseRule("127.0.0.1")];
    const ok = await connect(`127.0.0.1:${echoPort}`);
    assert.match(ok.head, /200/);
    const reply = await new Promise<string>((resolve) => {
      ok.sock.once("data", (d) => resolve(d.toString()));
      ok.sock.write("ping");
    });
    assert.equal(reply, "echo:ping");
    ok.sock.destroy();
  } finally {
    proxy.close();
    echo.close();
  }
});
