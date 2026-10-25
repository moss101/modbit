/**
 * Typed commands of the Customize view (REQ-PX-106): a view over the
 * registries the Core already has. Skills are listed with ListSkills and
 * trusted by hash with TrustSkill / UntrustSkill (PX-105); hooks and the
 * session's extensions come from ListHooks; a hook is switched with
 * SetHookEnabled; an extension is released with TrustExtension, removed with
 * UnloadExtension, and another agent's configuration comes in through
 * ImportAgentConfig + LoadExtension (quarantined until trusted). Nothing here
 * decides: the Core refuses a trust that is not bound to the current hash, a
 * switch of a hook a higher layer declared, and a release of a manifest other
 * than the one that was shown. There is no catalog, no install from a network
 * location and no update call.
 */
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import {
  ExtensionLoadedViewSchema,
  ExtensionUnloadedViewSchema,
  HookEnabledSetSchema,
  HookListViewSchema,
  ImportAgentConfigSchema,
  ImportReportViewSchema,
  ListHooksSchema,
  LoadExtensionSchema,
  SetHookEnabledSchema,
  SkillTrustResultSchema,
  TrustExtensionSchema,
  TrustSkillSchema,
  UnloadExtensionSchema,
  UntrustSkillSchema,
  type ExtensionLoadedView,
  type ExtensionUnloadedView,
  type HookEnabledSet,
  type HookListView,
  type ImportReportView,
  type SkillTrustResult,
} from "@modbit/surface-protocol";
import { unhex, type CoreClient } from "./client.ts";

const id = (hex: string) => ({ value: unhex(hex) });
const lease = (c: CoreClient, sessionId: string): bigint | undefined => c.leaseGeneration(sessionId);

/** Trust a skill: the decision names the content hash the person reviewed (a changed file is untrusted again). */
export async function trustSkill(c: CoreClient, name: string, contentHash: string, taskId?: string): Promise<SkillTrustResult> {
  const payload = toBinary(TrustSkillSchema, create(TrustSkillSchema, { name, contentHash, ...(taskId ? { taskId: id(taskId) } : {}) }));
  return fromBinary(SkillTrustResultSchema, (await c.command("TrustSkill", payload)).result);
}

/** Withdraw trust from one hash of a skill, or from every hash of the name when `contentHash` is empty. */
export async function untrustSkill(c: CoreClient, name: string, contentHash: string): Promise<SkillTrustResult> {
  const payload = toBinary(UntrustSkillSchema, create(UntrustSkillSchema, { name, contentHash }));
  return fromBinary(SkillTrustResultSchema, (await c.command("UntrustSkill", payload)).result);
}

/** The hooks in force for a task (with the ones switched off) and the session's extensions as structured rows; with no task, the extensions only. */
export async function listHooks(c: CoreClient, a: { taskId?: string; sessionId?: string }): Promise<HookListView> {
  const payload = toBinary(ListHooksSchema, create(ListHooksSchema, { ...(a.taskId ? { taskId: id(a.taskId) } : {}), ...(a.sessionId ? { sessionId: id(a.sessionId) } : {}) }));
  return fromBinary(HookListViewSchema, (await c.command("ListHooks", payload)).result);
}

export async function setHookEnabled(c: CoreClient, sessionId: string, taskId: string, hookId: string, enabled: boolean): Promise<HookEnabledSet> {
  const payload = toBinary(SetHookEnabledSchema, create(SetHookEnabledSchema, { sessionId: id(sessionId), taskId: id(taskId), hookId, enabled }));
  return fromBinary(HookEnabledSetSchema, (await c.command("SetHookEnabled", payload, undefined, lease(c, sessionId), sessionId)).result);
}

/** Release a quarantined extension: this exact manifest, which the person has seen. */
export async function trustExtension(c: CoreClient, sessionId: string, extensionId: string, manifestDigest: string): Promise<ExtensionLoadedView> {
  const payload = toBinary(TrustExtensionSchema, create(TrustExtensionSchema, { sessionId: id(sessionId), extensionId, manifestDigest }));
  return fromBinary(ExtensionLoadedViewSchema, (await c.command("TrustExtension", payload, undefined, lease(c, sessionId), sessionId)).result);
}

/** Unloading removes the extension's handlers at once: none runs again. */
export async function unloadExtension(c: CoreClient, sessionId: string, extensionId: string): Promise<ExtensionUnloadedView> {
  const payload = toBinary(UnloadExtensionSchema, create(UnloadExtensionSchema, { sessionId: id(sessionId), extensionId }));
  return fromBinary(ExtensionUnloadedViewSchema, (await c.command("UnloadExtension", payload, undefined, lease(c, sessionId), sessionId)).result);
}

export async function loadExtension(c: CoreClient, sessionId: string, path: string, expectedDigest: string): Promise<ExtensionLoadedView> {
  const payload = toBinary(LoadExtensionSchema, create(LoadExtensionSchema, { sessionId: id(sessionId), path, expectedDigest }));
  return fromBinary(ExtensionLoadedViewSchema, (await c.command("LoadExtension", payload, undefined, lease(c, sessionId), sessionId)).result);
}

/** The existing importer: another agent's configuration becomes one unsigned extension, labelled item by item; loading it quarantines it. */
export async function importAgentConfig(c: CoreClient, sessionId: string, sourceRoot: string, name: string, replace: boolean): Promise<ImportReportView> {
  const payload = toBinary(ImportAgentConfigSchema, create(ImportAgentConfigSchema, { sessionId: id(sessionId), sourceRoot, name, replace }));
  return fromBinary(ImportReportViewSchema, (await c.command("ImportAgentConfig", payload, undefined, lease(c, sessionId), sessionId)).result);
}
