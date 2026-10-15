/**
 * An ack belongs to the command it names. The client used to hand each ack to the oldest pending command; when an ack is not the
 * oldest command's (or an extra one arrives), every later command decodes another command's result: a lease generation read from
 * somebody else's reply, or a UTF-8 error in a string that was never one (seen on the Windows desktop E2E: `SetBrowserControl`
 * answered with a result that left the host's lease generation where it was while `ListAutomationRuns` failed to decode).
 *
 * The peer here is a real socket speaking the frame protocol that answers on purpose out of order, with an ack for no command, and
 * with an ack that arrives in pieces; the real Core's answers are covered by the conformance suite and the desktop E2E.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync } from "node:fs";
import { createServer, type Server, type Socket } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ClientKind, CoreClient, FrameDecoder, encodeFrame, hex } from "./client.ts";
import { CommandStatus, type CommandEnvelope } from "@modbit/surface-protocol";

interface Peer {
  endpoint: string;
  server: Server;
  /** Commands as they arrived, with the answer function for each. */
  commands: { env: CommandEnvelope; socket: Socket }[];
  ack(env: CommandEnvelope, result: number[], opts?: { split?: boolean; id?: Uint8Array }): void;
}

async function peer(): Promise<Peer> {
  const dir = mkdtempSync(join(tmpdir(), "modbit-ack-"));
  const endpoint = process.platform === "win32" ? `\\\\.\\pipe\\modbit-ack-${process.pid}-${Date.now()}` : join(dir, "core.sock");
  const commands: Peer["commands"] = [];
  const server = createServer((socket) => {
    const decoder = new FrameDecoder();
    socket.on("data", (chunk: Buffer) => {
      for (const f of decoder.push(chunk)) {
        if (f.body.case === "clientHello") socket.write(encodeFrame({ body: { case: "helloAck", value: { compatible: true } } }));
        else if (f.body.case === "command") commands.push({ env: f.body.value, socket });
      }
    });
  });
  await new Promise<void>((resolve) => server.listen(endpoint, resolve));
  return {
    endpoint,
    server,
    commands,
    ack(env, result, opts = {}) {
      const socket = commands.find((c) => c.env === env)!.socket;
      const bytes = encodeFrame({ body: { case: "commandAck", value: { commandId: { value: opts.id ?? env.commandId!.value }, status: CommandStatus.ACCEPTED, result: new Uint8Array(result) } } });
      if (opts.split) {
        socket.write(bytes.subarray(0, 7));
        setTimeout(() => socket.write(bytes.subarray(7)), 20);
      } else socket.write(bytes);
    },
  };
}

async function until(what: string, probe: () => boolean): Promise<void> {
  for (let i = 0; i < 400; i++) {
    if (probe()) return;
    await new Promise((r) => setTimeout(r, 10));
  }
  throw new Error(`timed out waiting for ${what}`);
}

async function connected(p: Peer): Promise<CoreClient> {
  return CoreClient.connect({ endpoint: p.endpoint, bootSecretHex: "00", protocol: { major: 1, minor: 0 } }, ClientKind.CLI, "ack-test");
}

test("acks answered out of order reach the commands they name, not the oldest", async () => {
  const p = await peer();
  const c = await connected(p);
  try {
    const a = c.command("TypeA", new Uint8Array());
    const b = c.command("TypeB", new Uint8Array());
    const d = c.command("TypeD", new Uint8Array());
    await until("three commands", () => p.commands.length === 3);
    const [ea, eb, ed] = p.commands.map((x) => x.env);
    p.ack(ed!, [3]);
    p.ack(eb!, [2]);
    p.ack(ea!, [1], { split: true });
    assert.deepEqual([...(await a).result], [1]);
    assert.deepEqual([...(await b).result], [2]);
    assert.deepEqual([...(await d).result], [3]);
  } finally {
    c.close();
    p.server.close();
  }
});

test("an ack that names no pending command is given to none of them", async () => {
  const p = await peer();
  const c = await connected(p);
  try {
    const a = c.command("TypeA", new Uint8Array());
    await until("the command", () => p.commands.length === 1);
    const stray = new Uint8Array(16).fill(7);
    p.ack(p.commands[0]!.env, [9], { id: stray });
    p.ack(p.commands[0]!.env, [1]);
    assert.deepEqual([...(await a).result], [1], `the stray ack (${hex(stray)}) must not answer the command`);
    // The next command is not shifted by the stray either.
    const b = c.command("TypeB", new Uint8Array());
    await until("the second command", () => p.commands.length === 2);
    p.ack(p.commands[1]!.env, [2]);
    assert.deepEqual([...(await b).result], [2]);
  } finally {
    c.close();
    p.server.close();
  }
});
