import { test } from "node:test";
import assert from "node:assert/strict";

test("@modbit/ide-adapter-core exports the shared thin client, the supervisor and the conformance suite (PX-001)", async () => {
  const mod = await import("./index.ts");
  for (const name of ["CoreClient", "CoreSupervisor", "parseReadyLine", "FrameDecoder", "encodeFrame", "runConformance", "clientSubject", "scanClientSources", "scanCargoClosure", "THIN_CLIENT_RULE"]) {
    assert.equal(typeof (mod as Record<string, unknown>)[name], name === "THIN_CLIENT_RULE" ? "object" : "function", name);
  }
});

test("the ready line a Core prints is parsed exactly and nothing else is", async () => {
  const { parseReadyLine } = await import("./client.ts");
  assert.deepEqual(parseReadyLine("MODBIT_CORE_READY endpoint=/tmp/x.sock secret=00ff protocol=1.0"), { endpoint: "/tmp/x.sock", bootSecretHex: "00ff", protocol: { major: 1, minor: 0 } });
  assert.equal(parseReadyLine("modbit-core: recovery complete"), null);
  assert.equal(parseReadyLine("MODBIT_CORE_READY endpoint=/tmp/x.sock"), null);
});
