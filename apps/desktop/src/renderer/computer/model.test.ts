import { test } from "node:test";
import assert from "node:assert/strict";
import type { ComputerCardView, ComputerPresenceEntry, ComputerPresenceInfo } from "../../shared/computer-types.ts";
import { alwaysOffered, announce, allReady, cardStatus, holdsMachine, permissionsNeeded, refusalWords, rowStatus } from "./model.ts";
import { alwaysAvailable } from "../approvals/model.ts";
import type { DockApprovalView } from "../../shared/control-types.ts";

const entry = (over: Partial<ComputerPresenceEntry> = {}): ComputerPresenceEntry => ({ taskId: "t1", controlId: "c1", application: "Form App", bundleId: "b", scope: "APP", state: "CONTROLLING", grantExpiresMs: 0, latch: "", latchToolCallId: "", actions: 0, screenshots: 0, observations: 0, ...over });
const info = (over: Partial<ComputerPresenceInfo> = {}): ComputerPresenceInfo => ({ configured: true, attached: true, actuatorName: "a", actuatorVersion: "1", platform: "fixture", detail: "", accessibility: "GRANTED", screenRecording: "GRANTED", controls: [], ...over });
const card = (over: Partial<ComputerCardView> = {}): ComputerCardView => ({ toolCallId: "c", tool: "computer.press", state: "SUCCEEDED", failureCode: "", role: "act", application: "", bundleId: "", scope: "", target: null, action: "", argumentsText: "", modality: "", outcome: "", verified: null, durationMs: 0, approval: null, observation: null, latched: false, userAborted: false, ...over });

test("CUC-E05: a permission row is checking until the helper says, allow when off, ready when on", () => {
  assert.equal(rowStatus("UNKNOWN", true), "checking");
  assert.equal(rowStatus("GRANTED", false), "checking", "no helper, nothing is ready");
  assert.equal(rowStatus("DENIED", true), "allow");
  assert.equal(rowStatus("GRANTED", true), "ready");
  assert.equal(permissionsNeeded(info({ accessibility: "DENIED" })), true);
  assert.equal(permissionsNeeded(info({ attached: false, accessibility: "DENIED" })), false);
  assert.equal(allReady(info()), true);
  assert.equal(allReady(info({ screenRecording: "UNKNOWN" })), false);
});

test("CUC-H01: only an agent in control or paused holds the machine; a stop or a latch is shown but has no Stop", () => {
  assert.equal(holdsMachine(entry()), true);
  assert.equal(holdsMachine(entry({ state: "PARKED" })), true);
  for (const s of ["STOPPED", "USER_ABORTED", "LATCHED"] as const) assert.equal(holdsMachine(entry({ state: s })), false);
});

test("CUC-H01: control acquired, stopped and released are announced once each", () => {
  const title = () => "the task";
  assert.match(announce([], [entry()], title), /acquired.*the task.*Form App/);
  assert.equal(announce([entry()], [entry()], title), "");
  assert.match(announce([entry()], [entry({ state: "USER_ABORTED" })], title), /stopped by you/);
  assert.match(announce([entry()], [], title), /released/);
  assert.match(announce([entry()], [entry({ state: "LATCHED" })], title), /unknown outcome/);
  assert.match(announce([entry()], [entry({ state: "PARKED" })], title), /paused/);
});

test("CUC-D01: every refusal is said in plain words with its typed code; a skipped, latched or stopped call is not a failure of the same kind", () => {
  assert.equal(cardStatus(card()).tone, "ok");
  assert.match(cardStatus(card({ state: "FAILED", failureCode: "TARGET_STALE" })).label, /target stale/);
  assert.match(cardStatus(card({ state: "FAILED", failureCode: "TARGET_STALE" })).detail, /Nothing was done/);
  assert.equal(cardStatus(card({ state: "FAILED", failureCode: "APPROVAL_DENIED" })).label, "Skipped");
  assert.match(cardStatus(card({ state: "FAILED", failureCode: "USER_ABORTED", userAborted: true })).label, /USER_ABORTED/);
  assert.equal(cardStatus(card({ state: "UNKNOWN_OUTCOME" })).label, "Outcome unknown");
  assert.equal(cardStatus(card({ latched: true })).tone, "warn");
  assert.match(refusalWords("SOMETHING_NEW").headline, /something new/);
});

test("CUC-C01: no always, allowlist or run-mode path exists for a native-control approval", () => {
  assert.equal(alwaysOffered("computer.press"), false);
  const base = { approvalId: "a", taskId: "t", toolCallId: "c", toolName: "computer.press", effectClass: "ExternalSideEffect", intentHash: "h".repeat(64), requestedAtMs: 0, expiresAtMs: 0, reason: { code: "MODE_ASK", runMode: "ALLOWLIST", askClasses: [], contained: false, capabilities: [], executionProfile: "" }, intent: { found: true, argv: ["x", "y"], command: "", cwd: "", paths: [], hosts: [], escalation: "", argsPreview: "", truncated: false } } as DockApprovalView;
  assert.equal(alwaysAvailable(base).ok, false, "even with a command-shaped argv and a permissive mode");
  assert.equal(alwaysAvailable({ ...base, toolName: "shell.exec", computer: null, effectClass: "ProtectedWrite" }).ok, true, "other effects are unchanged");
});
