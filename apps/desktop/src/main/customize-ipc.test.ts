import assert from "node:assert/strict";
import { test } from "node:test";
import { create } from "@bufbuild/protobuf";
import { ExtensionRowSchema, HookListViewSchema, HookViewSchema, ImportReportViewSchema, SkillListSchema, SkillViewSchema, ToolServerRowSchema } from "@modbit/surface-protocol";
import { extensionInfo, hooksView, importInfo, registerCustomizeHandlers, skillsView, validContentHash, validHookId, validImportName, validManifestDigest, validSkillName } from "./customize-ipc.ts";
import type { Registrar } from "./conversation-ipc.ts";

const SID = "0123456789abcdef0123456789abcdef";
const TID = "fedcba9876543210fedcba9876543210";
const HASH = "a".repeat(64);

/** A registrar whose Core client explodes when touched: a handler that reaches it has not finished validating. */
function harness() {
  const handlers = new Map<string, (...a: unknown[]) => unknown>();
  let picked = 0;
  const r: Registrar = {
    handle: (channel, fn) => void handlers.set(channel, fn),
    client: () => {
      throw new Error("CLIENT_TOUCHED");
    },
    sessionId: (v) => {
      if (v !== SID) throw new Error("BAD_ARGUMENT: session id must be 32 hex chars");
      return SID;
    },
    taskId: (v) => {
      if (v !== TID) throw new Error("BAD_ARGUMENT: task id must be 32 hex chars");
      return TID;
    },
    lease: () => Promise.resolve(),
  };
  registerCustomizeHandlers(r, {
    pickDirectory: () => {
      picked += 1;
      return Promise.resolve(null);
    },
    reveal: () => {
      throw new Error("REVEAL_TOUCHED");
    },
  });
  const call = async (channel: string, ...args: unknown[]): Promise<string> => {
    const fn = handlers.get(channel);
    assert.ok(fn, `${channel} is registered`);
    try {
      await fn(...args);
      return "accepted";
    } catch (e) {
      return (e as Error).message;
    }
  };
  return { call, channels: () => [...handlers.keys()], picked: () => picked };
}

test("PX-106: the Customize view registers nine typed channels and none installs, browses a catalog or updates", () => {
  const channels = harness().channels().sort();
  assert.deepEqual(channels, ["customize:extensionTrust", "customize:extensionUnload", "customize:hookEnabled", "customize:hooks", "customize:import", "customize:memory", "customize:memoryForget", "customize:memoryPromote", "customize:memoryRemember", "customize:memoryShare", "customize:openSource", "customize:skillRevoke", "customize:skillTrust", "customize:skills"]);
  assert.equal(channels.filter((c) => /catalog|market|install|download|update|browse/i.test(c)).length, 0);
});

test("PX-106: trust is bound to the content hash; a malformed or missing one is refused in main before the Core is asked", async () => {
  const h = harness();
  const refused = async (channel: string, ...args: unknown[]) => assert.match(await h.call(channel, ...args), /BAD_ARGUMENT/, `${channel} ${JSON.stringify(args)}`);
  // Trust by name only does not exist: no hash, a short one, a made-up one are all refused.
  await refused("customize:skillTrust", "alpha");
  await refused("customize:skillTrust", "alpha", "");
  await refused("customize:skillTrust", "alpha", "latest");
  await refused("customize:skillTrust", "alpha", HASH.toUpperCase());
  await refused("customize:skillTrust", "../alpha", HASH);
  await refused("customize:skillTrust", "", HASH);
  await refused("customize:skillTrust", "a\u0000b", HASH);
  await refused("customize:skillTrust", "alpha", HASH, "not a task");
  await refused("customize:skillRevoke", "alpha", "nope");
  // A valid trust reaches the client (which here explodes): validation is not what stopped it.
  assert.match(await h.call("customize:skillTrust", "alpha", HASH), /CLIENT_TOUCHED/);
  assert.match(await h.call("customize:skillRevoke", "alpha"), /CLIENT_TOUCHED/, "revoking every hash of a name needs no hash");
});

test("PX-106: hook and extension actions name what the Core listed", async () => {
  const h = harness();
  const refused = async (channel: string, ...args: unknown[]) => assert.match(await h.call(channel, ...args), /BAD_ARGUMENT/, `${channel} ${JSON.stringify(args)}`);
  await refused("customize:hookEnabled", "nope", TID, "config:user/x", false);
  await refused("customize:hookEnabled", SID, "nope", "config:user/x", false);
  await refused("customize:hookEnabled", SID, TID, "", false);
  await refused("customize:hookEnabled", SID, TID, "config:user/x", "no");
  await refused("customize:hookEnabled", SID, TID, "x".repeat(300), false);
  assert.match(await h.call("customize:hookEnabled", SID, TID, "config:user/x", false), /CLIENT_TOUCHED/);
  await refused("customize:extensionTrust", SID, "ext-1", "short");
  await refused("customize:extensionTrust", SID, "", HASH);
  await refused("customize:extensionTrust", "nope", "ext-1", HASH);
  assert.match(await h.call("customize:extensionTrust", SID, "ext-1", HASH), /CLIENT_TOUCHED/);
  await refused("customize:extensionUnload", SID, "");
  await refused("customize:hooks", "nope");
  assert.match(await h.call("customize:hooks", SID), /CLIENT_TOUCHED/, "with no task the session's extensions are listed");
  await refused("customize:openSource", "network", "x");
  await refused("customize:openSource", "skill", "../../etc");
  await refused("customize:openSource", "extension", "ext-1", "nope");
});

test("PX-106: an import name is plain, and the folder is asked of the person, never taken from the renderer", async () => {
  const h = harness();
  for (const name of ["", "../x", "a b", "-x", "x".repeat(65), 7]) assert.match(await h.call("customize:import", SID, name), /BAD_ARGUMENT/, String(name));
  assert.equal(h.picked(), 0, "nothing was asked of the person for a bad name");
  assert.equal(await h.call("customize:import", SID, "imported"), "accepted", "a cancelled dialog imports nothing");
  assert.equal(h.picked(), 1);
  // A path in the arguments is not a path anyone looks at.
  assert.equal(await h.call("customize:import", SID, "imported", "/etc"), "accepted");
  assert.equal(h.picked(), 2);
});

test("PX-106: validators keep what is valid", () => {
  assert.equal(validContentHash(HASH), HASH);
  assert.equal(validManifestDigest(HASH), HASH);
  assert.equal(validSkillName("pdf-reader"), "pdf-reader");
  assert.equal(validHookId("config:user/rec"), "config:user/rec");
  assert.equal(validImportName("imported_1"), "imported_1");
});

test("PX-106: the Core's rows become plain shapes with nothing added or dropped", () => {
  const s = skillsView(
    create(SkillListSchema, {
      skills: [create(SkillViewSchema, { name: "alpha", version: "1.0.0", scope: "USER", contentHash: HASH, trust: "UNTRUSTED", trustDetail: "nobody vouches", enabled: false, invocation: "BOTH", indexTokens: 12, indexed: false, indexForm: "NOT_INDEXED", source: "/x/alpha", provenanceAuthor: "me" })],
      indexBudgetTokens: 400,
      indexUsedTokens: 30,
      indexOmitted: 1,
    }),
  );
  assert.equal(s.skills[0]?.trust, "UNTRUSTED");
  assert.equal(s.skills[0]?.contentHash, HASH);
  assert.deepEqual([s.indexBudgetTokens, s.indexUsedTokens, s.indexOmitted], [400, 30, 1]);
  const e = extensionInfo(create(ExtensionRowSchema, { extensionId: "e1", name: "kit", state: "QUARANTINED", canTrust: true, servers: [create(ToolServerRowSchema, { name: "docs", state: "INERT", credentialRequired: true })] }));
  assert.equal(e.servers[0]?.state, "INERT");
  assert.equal(e.canTrust, true);
  const hv = hooksView(create(HookListViewSchema, { hooks: [create(HookViewSchema, { id: "config:user/r", scope: "USER", enabled: false, canDisable: true, timeoutMs: 5000n, lastDurationMs: 7n, invocations: 2, lastOutcome: "OK" })], refused: ["x (y): z"], extensionRows: [create(ExtensionRowSchema, { extensionId: "e1" })] }));
  assert.deepEqual([hv.hooks[0]?.timeoutMs, hv.hooks[0]?.lastDurationMs, hv.hooks[0]?.enabled, hv.extensions.length, hv.refused.length], ["5000", "7", false, 1, 1]);
  const i = importInfo(create(ImportReportViewSchema, { name: "mixed", extensionPath: "/p", mapped: 3, skipped: 1, conflicts: 1, manifestDigest: HASH, signature: "UNSIGNED" }), "unsigned: inert until trusted");
  assert.deepEqual([i.mapped, i.skipped, i.conflicts, i.quarantined], [3, 1, 1, "unsigned: inert until trusted"]);
});
