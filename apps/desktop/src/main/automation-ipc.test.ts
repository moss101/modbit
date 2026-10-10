import { test } from "node:test";
import assert from "node:assert/strict";
import { isAbsolute, join } from "node:path";
import { tmpdir } from "node:os";
import { create, toBinary } from "@bufbuild/protobuf";
import { AutomationEnableViewSchema, AutomationRunViewSchema, AutomationTriggerViewSchema, AutomationViewSchema, type CommandAck } from "@modbit/surface-protocol";
import { validAutomationId, validDefinitionJson, validEnableInput, validOptionalAutomationId, validRunInput, validWorkspaceRoot } from "./automation-args.ts";
import { autoView, registerAutomationHandlers, runView } from "./automation-ipc.ts";

const ID = "c".repeat(32);
const H = "a".repeat(64);
const TID = "b".repeat(32);

function harness() {
  const handlers = new Map<string, (...a: unknown[]) => unknown>();
  const commands: { type: string; id: Uint8Array | undefined }[] = [];
  const client = {
    command: (type: string, _payload: Uint8Array, id?: Uint8Array): Promise<CommandAck> => {
      commands.push({ type, id });
      return Promise.reject(new Error(`APPROVAL_MISMATCH: the Core says so (${type})`));
    },
  };
  registerAutomationHandlers({
    handle: (c, fn) => void handlers.set(c, fn),
    client: () => client as never,
    sessionId: (v) => String(v),
    taskId: (v) => { if (typeof v !== "string" || !/^[0-9a-f]{32}$/.test(v)) throw new Error("BAD_ARGUMENT: task"); return v; },
    lease: async () => {},
  });
  return { call: (c: string, ...a: unknown[]) => Promise.resolve().then(() => handlers.get(c)!(...a)), commands, channels: [...handlers.keys()] };
}

test("REQ-PX-086: an automation id is 32 lowercase hex characters; an empty id is the global switch only where the Core allows it", () => {
  assert.equal(validAutomationId(ID), ID);
  for (const v of ["", "xyz", "C".repeat(32), ID + "0", 7, null, undefined]) assert.throws(() => validAutomationId(v), /BAD_ARGUMENT/);
  assert.equal(validOptionalAutomationId(""), "");
  assert.equal(validOptionalAutomationId(undefined), "");
  assert.throws(() => validOptionalAutomationId("nope"), /BAD_ARGUMENT/);
});

test("REQ-PX-086: the approval names the version, the full hash and the lists the dialog showed; nothing is defaulted", () => {
  const ok = { automationId: ID, version: 2, definitionHash: H, effects: "reversible_write", capabilities: ["fs.write"], paths: ["reports/**"], hosts: [] };
  assert.deepEqual(validEnableInput(ok), ok);
  const refused = (what: string, over: Record<string, unknown>) => assert.throws(() => validEnableInput({ ...ok, ...over }), /BAD_ARGUMENT/, what);
  refused("a short hash", { definitionHash: "abc" });
  refused("an upper-case hash", { definitionHash: "A".repeat(64) });
  refused("no hash", { definitionHash: undefined });
  refused("version zero", { version: 0 });
  refused("a version as text", { version: "2" });
  refused("an effect class the Core does not know", { effects: "everything" });
  refused("a list that is not a list", { capabilities: "fs.write" });
  refused("a list entry with a control character", { paths: ["a\nb"] });
  refused("an empty list entry", { hosts: [""] });
  refused("too many entries", { paths: Array.from({ length: 257 }, (_, i) => `p${i}`) });
  refused("a missing list", { hosts: undefined });
  assert.throws(() => validEnableInput(null), /BAD_ARGUMENT/);
  assert.throws(() => validEnableInput([ok]), /BAD_ARGUMENT/);
});

test("REQ-PX-086: a definition is text within the bound; the workspace is an absolute path; a run request is narrow", () => {
  assert.equal(validDefinitionJson("{}"), "{}");
  assert.throws(() => validDefinitionJson({}), /BAD_ARGUMENT/);
  assert.throws(() => validDefinitionJson("x".repeat(256 * 1024 + 1)), /BAD_ARGUMENT/);
  const abs = join(tmpdir(), "repo");
  assert.ok(isAbsolute(abs));
  assert.equal(validWorkspaceRoot(abs), abs);
  for (const v of ["", "   ", "relative/dir", "a\u0000b", 5, "x".repeat(5000)]) assert.throws(() => validWorkspaceRoot(v), /BAD_ARGUMENT/);

  assert.deepEqual(validRunInput({ automationId: ID }), { automationId: ID, triggerId: "", inputsJson: "", test: false, payloadJson: "", source: "", event: "", eventId: "" });
  const t = validRunInput({ automationId: ID, test: true, triggerId: "pr", payloadJson: '{"action":"opened"}', source: "forge" });
  assert.equal(t.test, true);
  assert.equal(t.payloadJson, '{"action":"opened"}');
  for (const bad of [{ automationId: ID, test: "yes" }, { automationId: ID, payloadJson: "{not json" }, { automationId: ID, inputsJson: "[1]" }, { automationId: ID, source: "slack" }, { automationId: ID, event: "x".repeat(129) }, { automationId: "nope" }, null]) {
    assert.throws(() => validRunInput(bad), /BAD_ARGUMENT/);
  }
});

test("REQ-PX-086: every automation handler rejects malformed arguments before anything reaches the Core, and a Core refusal passes through with its own code", async () => {
  const h = harness();
  const refused = (what: string, c: string, ...a: unknown[]) => assert.rejects(h.call(c, ...a), /BAD_ARGUMENT/, what);
  await refused("get with a bad id", "automations:get", "nope");
  await refused("validate with a non-string", "automations:validate", 5);
  await refused("create with a relative workspace", "automations:create", "{}", "repo", "");
  await refused("create with a bad command id", "automations:create", "{}", join(tmpdir(), "r"), "zz");
  await refused("update with a bad id", "automations:update", "x", "{}");
  await refused("enable with a short hash", "automations:enable", { automationId: ID, version: 1, definitionHash: "abc", effects: "read_only", capabilities: [], paths: [], hosts: [] });
  await refused("enable with nothing", "automations:enable", undefined);
  await refused("pause with a non-boolean", "automations:pause", ID, "yes");
  await refused("pause with a bad id", "automations:pause", "x", true);
  await refused("kill with a bad id", "automations:kill", "x");
  await refused("ack with a bad key", "automations:ack", "../..");
  await refused("run with a bad payload", "automations:run", { automationId: ID, payloadJson: "{" });
  await refused("runs with a bad limit", "automations:runs", { limit: 9999 });
  await refused("runs with a bad task id", "automations:runs", { taskId: "x" });
  assert.equal(h.commands.length, 0, "nothing reached the Core");

  // A valid request is forwarded, and the Core's refusal reaches the caller with its code unchanged.
  await assert.rejects(h.call("automations:enable", { automationId: ID, version: 1, definitionHash: H, effects: "read_only", capabilities: [], paths: [], hosts: [] }), /APPROVAL_MISMATCH/);
  assert.deepEqual(h.commands.map((c) => c.type), ["EnableAutomation"]);
  await assert.rejects(h.call("automations:create", "{}", join(tmpdir(), "r"), "d".repeat(32)), /APPROVAL_MISMATCH/);
  assert.deepEqual(Array.from(h.commands[1]!.id ?? []), Array.from(Buffer.from("d".repeat(32), "hex")), "a retry keeps its command id");
  for (const c of ["automations:list", "automations:get", "automations:runs", "automations:validate", "automations:create", "automations:update", "automations:loadRepository", "automations:enable", "automations:disable", "automations:pause", "automations:kill", "automations:run", "automations:ack", "automations:taskHeaders"]) assert.ok(h.channels.includes(c), c);
});

test("REQ-PX-086: the Core's view becomes plain shapes without losing a field the dialog shows", () => {
  const v = create(AutomationViewSchema, {
    automationId: ID,
    name: "nightly",
    currentVersion: 3,
    definitionHash: H,
    state: "NEEDS_APPROVAL",
    effects: "reversible_write",
    capabilities: ["fs.write"],
    paths: ["reports/**"],
    hosts: ["api.github.com"],
    sourceKind: "repository",
    sourcePath: ".modbit/automations/nightly.json",
    needsListedApproval: true,
    contentChanged: true,
    lastRunMs: 1_800_000_000_000n,
    limitDeadlineMinutes: 30n,
    triggers: [create(AutomationTriggerViewSchema, { id: "t", kind: "schedule", summary: "cron 0 3 * * * (UTC)", nextDueMs: 1_800_000_123_000n })],
    enabled: create(AutomationEnableViewSchema, { version: 2, definitionHash: "e".repeat(64), approvedMs: 5n, repoRevision: "abc" }),
  });
  const plain = autoView(v);
  assert.equal(plain.definitionHash, H);
  assert.equal(plain.currentVersion, 3);
  assert.deepEqual(plain.capabilities, ["fs.write"]);
  assert.deepEqual(plain.paths, ["reports/**"]);
  assert.deepEqual(plain.hosts, ["api.github.com"]);
  assert.equal(plain.contentChanged, true);
  assert.equal(plain.lastRunMs, 1_800_000_000_000);
  assert.equal(plain.triggers[0]?.nextDueMs, 1_800_000_123_000);
  assert.equal(plain.enabled?.repoRevision, "abc");
  assert.ok(toBinary(AutomationViewSchema, v).length > 0);

  const r = runView(create(AutomationRunViewSchema, { dispatchKey: "k", status: "skipped", reason: "DUPLICATE", taskId: TID, costMinor: 7n, findings: 2, outputsJson: '{"dry_run":true}' }));
  assert.deepEqual({ status: r.status, reason: r.reason, taskId: r.taskId, costMinor: r.costMinor, findings: r.findings }, { status: "skipped", reason: "DUPLICATE", taskId: TID, costMinor: 7, findings: 2 });
});
