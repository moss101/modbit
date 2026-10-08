/**
 * Typed commands of the automations surface (REQ-PX-086; docs/68): the
 * definitions, the owner's enable approval, the kill switches, manual and test
 * runs and the run history, over the one SurfaceProtocol client. Every
 * function is a thin typed call: the Core validates, versions, approves and
 * runs, and these return its answer unchanged. None of them needs the session
 * lease (an automation is the owner's, not a session's); the Core holds the
 * `automation.manage` capability check, never this file.
 */
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import {
  AckAutomationAttentionSchema,
  AutomationDetailSchema,
  AutomationListSchema,
  AutomationRunListSchema,
  AutomationRunStartedSchema,
  AutomationValidationSchema,
  AutomationViewSchema,
  CreateAutomationSchema,
  DisableAutomationSchema,
  EnableAutomationSchema,
  GetAutomationSchema,
  KillAutomationSchema,
  KillReportSchema,
  ListAutomationRunsSchema,
  ListAutomationsSchema,
  LoadRepositoryAutomationsSchema,
  PauseAutomationSchema,
  RepositoryAutomationsSchema,
  RunAutomationSchema,
  UpdateAutomationSchema,
  ValidateAutomationSchema,
  type AutomationDetail,
  type AutomationList,
  type AutomationRunList,
  type AutomationRunStarted,
  type AutomationValidation,
  type AutomationView,
  type KillReport,
  type RepositoryAutomations,
} from "@modbit/surface-protocol";
import type { CoreClient } from "./client.ts";

export async function listAutomations(c: CoreClient): Promise<AutomationList> {
  const ack = await c.command("ListAutomations", toBinary(ListAutomationsSchema, create(ListAutomationsSchema, {})));
  return fromBinary(AutomationListSchema, ack.result);
}

export async function getAutomation(c: CoreClient, automationId: string): Promise<AutomationDetail> {
  const ack = await c.command("GetAutomation", toBinary(GetAutomationSchema, create(GetAutomationSchema, { automationId })));
  return fromBinary(AutomationDetailSchema, ack.result);
}

export async function listAutomationRuns(c: CoreClient, o: { automationId?: string; taskId?: string; limit?: number }): Promise<AutomationRunList> {
  const ack = await c.command("ListAutomationRuns", toBinary(ListAutomationRunsSchema, create(ListAutomationRunsSchema, { automationId: o.automationId ?? "", taskId: o.taskId ?? "", limit: o.limit ?? 0 })));
  return fromBinary(AutomationRunListSchema, ack.result);
}

/** The editor's live validation: the Core's own parser and validator, nothing stored. */
export async function validateAutomation(c: CoreClient, definitionJson: string): Promise<AutomationValidation> {
  const ack = await c.command("ValidateAutomation", toBinary(ValidateAutomationSchema, create(ValidateAutomationSchema, { definitionJson })));
  return fromBinary(AutomationValidationSchema, ack.result);
}

export async function createAutomation(c: CoreClient, definitionJson: string, workspaceRoot: string, commandId?: Uint8Array): Promise<AutomationView> {
  const ack = await c.command("CreateAutomation", toBinary(CreateAutomationSchema, create(CreateAutomationSchema, { definitionJson, workspaceRoot })), commandId);
  return fromBinary(AutomationViewSchema, ack.result);
}

export async function updateAutomation(c: CoreClient, automationId: string, definitionJson: string, commandId?: Uint8Array): Promise<AutomationView> {
  const ack = await c.command("UpdateAutomation", toBinary(UpdateAutomationSchema, create(UpdateAutomationSchema, { automationId, definitionJson })), commandId);
  return fromBinary(AutomationViewSchema, ack.result);
}

export async function loadRepositoryAutomations(c: CoreClient, workspaceRoot: string): Promise<RepositoryAutomations> {
  const ack = await c.command("LoadRepositoryAutomations", toBinary(LoadRepositoryAutomationsSchema, create(LoadRepositoryAutomationsSchema, { workspaceRoot })));
  return fromBinary(RepositoryAutomationsSchema, ack.result);
}

/** The owner's approval: exactly the version, hash and lists the person was shown. */
export async function enableAutomation(
  c: CoreClient,
  a: { automationId: string; version: number; definitionHash: string; effects: string; capabilities: string[]; paths: string[]; hosts: string[] },
  commandId?: Uint8Array,
): Promise<AutomationView> {
  const ack = await c.command("EnableAutomation", toBinary(EnableAutomationSchema, create(EnableAutomationSchema, a)), commandId);
  return fromBinary(AutomationViewSchema, ack.result);
}

export async function disableAutomation(c: CoreClient, automationId: string, note: string): Promise<AutomationView> {
  const ack = await c.command("DisableAutomation", toBinary(DisableAutomationSchema, create(DisableAutomationSchema, { automationId, note })));
  return fromBinary(AutomationViewSchema, ack.result);
}

/** Pause or resume one definition (the Core answers with its view) or, with an empty id, every definition (it answers with the list). */
export async function pauseAutomation(c: CoreClient, automationId: string, paused: boolean, note: string): Promise<{ view: AutomationView | null; list: AutomationList | null }> {
  const ack = await c.command("PauseAutomation", toBinary(PauseAutomationSchema, create(PauseAutomationSchema, { automationId, paused, note })));
  return automationId === "" ? { view: null, list: fromBinary(AutomationListSchema, ack.result) } : { view: fromBinary(AutomationViewSchema, ack.result), list: null };
}

export async function killAutomation(c: CoreClient, automationId: string, note: string): Promise<KillReport> {
  const ack = await c.command("KillAutomation", toBinary(KillAutomationSchema, create(KillAutomationSchema, { automationId, note })));
  return fromBinary(KillReportSchema, ack.result);
}

export async function runAutomation(
  c: CoreClient,
  a: { automationId: string; triggerId?: string; inputsJson?: string; test?: boolean; payloadJson?: string; source?: string; event?: string; eventId?: string },
  commandId?: Uint8Array,
): Promise<AutomationRunStarted> {
  const ack = await c.command(
    "RunAutomation",
    toBinary(RunAutomationSchema, create(RunAutomationSchema, { automationId: a.automationId, triggerId: a.triggerId ?? "", inputsJson: a.inputsJson ?? "", test: a.test ?? false, payloadJson: a.payloadJson ?? "", source: a.source ?? "", event: a.event ?? "", eventId: a.eventId ?? "" })),
    commandId,
  );
  return fromBinary(AutomationRunStartedSchema, ack.result);
}

export async function ackAutomationAttention(c: CoreClient, dispatchKey: string): Promise<void> {
  await c.command("AckAutomationAttention", toBinary(AckAutomationAttentionSchema, create(AckAutomationAttentionSchema, { dispatchKey })));
}
