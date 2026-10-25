/**
 * Main's side of the Customize view (REQ-PX-106): typed, argument-validated
 * handlers over the Core's skill, hook and extension commands. Every handler
 * validates what the renderer sent before it reaches the client (REQ-EV-0103)
 * and converts the Core's answer to the plain shapes of
 * shared/customize-types.ts. Nothing is decided or kept here: trust is the
 * Core's content-hash-bound record, a hook switch is the Core's record on the
 * session's log, an extension is released only for the manifest digest the
 * row showed. A path is never taken from the renderer: "open the source" and
 * "import" look the path up in the Core's own listing or ask the person
 * through the OS dialog. There is no handler that browses a catalog,
 * downloads or installs from a network location, or updates anything.
 */
import {
  importAgentConfig,
  listHooks,
  loadExtension,
  setHookEnabled,
  trustExtension,
  trustSkill,
  unloadExtension,
  untrustSkill,
} from "@modbit/ide-adapter-core";
import type { ExtensionRow, HookListView, ImportReportView, SkillList, SkillTrustResult } from "@modbit/surface-protocol";
import type { CustomizeHooksView, ExtensionRowInfo, HookEnabledInfo, ImportReportInfo, SkillRow, SkillsView, SkillTrustInfo } from "../shared/customize-types.ts";
import { bad, validBool } from "./control-args.ts";
import type { Registrar } from "./conversation-ipc.ts";

const HEX64 = /^[0-9a-f]{64}$/;
// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f]/;

/** What main needs of the OS: a folder the person picks and a file manager. Both are the person's act. */
export interface CustomizeHost {
  /** Ask the person for a directory (the OS dialog); null when they cancel. */
  pickDirectory: () => Promise<string | null>;
  /** Show a path the Core listed in the file manager. */
  reveal: (path: string) => void;
}

// --------------------------------------------------------------- validation

/** A skill's name as the Core lists it: printable, bounded, no path separators. */
export function validSkillName(v: unknown): string {
  if (typeof v !== "string" || v.length === 0 || v.length > 128 || CONTROL.test(v) || /[\\/]/.test(v)) return bad("a skill name is the name the Core listed");
  return v;
}

/** The content hash the Core named: 64 lowercase hex digits. Trust by name alone does not exist. */
export function validContentHash(v: unknown): string {
  if (typeof v !== "string" || !HEX64.test(v)) return bad("a trust decision names the 64-hex-digit content hash the Core showed");
  return v;
}

export function validRevokeHash(v: unknown): string {
  if (v === undefined || v === null || v === "") return "";
  return validContentHash(v);
}

/** A hook id as ListHooks names it (`<source>/<name>`). */
export function validHookId(v: unknown): string {
  if (typeof v !== "string" || v.length === 0 || v.length > 256 || CONTROL.test(v)) return bad("a hook id is the id the Core listed");
  return v;
}

export function validExtensionId(v: unknown): string {
  if (typeof v !== "string" || v.length === 0 || v.length > 128 || CONTROL.test(v)) return bad("an extension id is the id the Core listed");
  return v;
}

export function validManifestDigest(v: unknown): string {
  if (typeof v !== "string" || !HEX64.test(v)) return bad("a manifest digest is the 64-hex-digit digest the Core showed");
  return v;
}

export function validImportName(v: unknown): string {
  if (typeof v !== "string" || !/^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$/.test(v)) return bad("an import name is letters, digits, - and _ (at most 64)");
  return v;
}

// --------------------------------------------------------------- conversion

export function skillsView(l: SkillList): SkillsView {
  return {
    skills: l.skills.map(
      (s): SkillRow => ({
        name: s.name,
        version: s.version,
        description: s.description,
        scope: s.scope,
        contentHash: s.contentHash,
        trust: s.trust,
        trustDetail: s.trustDetail,
        enabled: s.enabled,
        invocation: s.invocation,
        paths: [...s.paths],
        pathsActive: s.pathsActive,
        indexTokens: s.indexTokens,
        indexed: s.indexed,
        indexForm: s.indexForm,
        source: s.source,
        provenanceSource: s.provenanceSource,
        provenanceAuthor: s.provenanceAuthor,
        provenanceLicense: s.provenanceLicense,
        lifecycle: s.lifecycle,
      }),
    ),
    rejected: l.rejected.map((r) => ({ source: r.source, code: r.code, reason: r.reason })),
    indexBudgetTokens: l.indexBudgetTokens,
    indexUsedTokens: l.indexUsedTokens,
    indexOmitted: l.indexOmitted,
    systemRoot: l.systemRoot,
  };
}

export function skillTrustInfo(r: SkillTrustResult): SkillTrustInfo {
  return { name: r.name, contentHash: r.contentHash, trust: r.trust, records: r.records };
}

export function extensionInfo(e: ExtensionRow): ExtensionRowInfo {
  return {
    extensionId: e.extensionId,
    name: e.name,
    version: e.version,
    scope: e.scope,
    state: e.state,
    quarantineReason: e.quarantineReason,
    signature: e.signature,
    manifestDigest: e.manifestDigest,
    path: e.path,
    publisher: e.publisher,
    source: e.source,
    hooks: [...e.hooks],
    commands: [...e.commands],
    providers: [...e.providers],
    servers: e.servers.map((s) => ({ name: s.name, transport: s.transport, state: s.state, credentialRequired: s.credentialRequired, credentialHeld: s.credentialHeld, scopes: [...s.scopes] })),
    capabilities: [...e.capabilities],
    canTrust: e.canTrust,
  };
}

export function hooksView(l: HookListView): CustomizeHooksView {
  return {
    hooks: l.hooks.map((h) => ({
      id: h.id,
      source: h.source,
      point: h.point,
      mode: h.mode,
      failPolicy: h.failPolicy,
      timeoutMs: h.timeoutMs.toString(),
      tools: [...h.tools],
      command: [...h.command],
      scope: h.scope,
      enabled: h.enabled,
      canDisable: h.canDisable,
      trustedWorkspaceGate: h.trustedWorkspaceGate,
      lastOutcome: h.lastOutcome,
      lastDetail: h.lastDetail,
      lastDurationMs: h.lastDurationMs.toString(),
      invocations: h.invocations,
    })),
    refused: [...l.refused],
    extensions: l.extensionRows.map(extensionInfo),
  };
}

export function importInfo(r: ImportReportView, quarantined: string): ImportReportInfo {
  return {
    name: r.name,
    extensionPath: r.extensionPath,
    mapped: r.mapped,
    skipped: r.skipped,
    conflicts: r.conflicts,
    manifestDigest: r.manifestDigest,
    signature: r.signature,
    quarantined,
    items: r.items.map((i) => ({ kind: i.kind, source: i.source, target: i.target, status: i.status, reason: i.reason })),
  };
}

// ------------------------------------------------------------- the handlers

const optionalTask = (r: Registrar, v: unknown): string | undefined => (v === undefined || v === null || v === "" ? undefined : r.taskId(v));

/** The commands of the Customize view. */
export function registerCustomizeHandlers(r: Registrar, host: CustomizeHost): void {
  r.handle("customize:skills", async (...a) => {
    const tid = optionalTask(r, a[0]);
    return skillsView(await r.client().listSkills(tid));
  });
  r.handle("customize:skillTrust", async (...a) => {
    const name = validSkillName(a[0]);
    const hash = validContentHash(a[1]);
    const tid = optionalTask(r, a[2]);
    return skillTrustInfo(await trustSkill(r.client(), name, hash, tid));
  });
  r.handle("customize:skillRevoke", async (...a) => {
    const name = validSkillName(a[0]);
    const hash = validRevokeHash(a[1]);
    return skillTrustInfo(await untrustSkill(r.client(), name, hash));
  });
  r.handle("customize:hooks", async (...a) => {
    const sid = r.sessionId(a[0]);
    const tid = optionalTask(r, a[1]);
    return hooksView(await listHooks(r.client(), tid ? { taskId: tid } : { sessionId: sid }));
  });
  r.handle("customize:hookEnabled", async (...a) => {
    const sid = r.sessionId(a[0]);
    const tid = r.taskId(a[1]);
    const hookId = validHookId(a[2]);
    const enabled = validBool(a[3], "enabled");
    const c = r.client();
    await r.lease(c, sid);
    const s = await setHookEnabled(c, sid, tid, hookId, enabled);
    return { hookId: s.hookId, enabled: s.enabled, scope: s.scope, offset: s.offset.toString() } satisfies HookEnabledInfo;
  });
  r.handle("customize:extensionTrust", async (...a) => {
    const sid = r.sessionId(a[0]);
    const extId = validExtensionId(a[1]);
    const digest = validManifestDigest(a[2]);
    const c = r.client();
    await r.lease(c, sid);
    const v = await trustExtension(c, sid, extId, digest);
    return { extensionId: v.extensionId, name: v.name, quarantined: v.quarantined };
  });
  r.handle("customize:extensionUnload", async (...a) => {
    const sid = r.sessionId(a[0]);
    const extId = validExtensionId(a[1]);
    const c = r.client();
    await r.lease(c, sid);
    const v = await unloadExtension(c, sid, extId);
    return { extensionId: v.extensionId, name: v.name, removed: [...v.removed] };
  });
  // Import: the person picks a directory in the OS dialog (a renderer cannot name one); the Core's importer writes one unsigned extension and loading it quarantines it.
  r.handle("customize:import", async (...a): Promise<ImportReportInfo | null> => {
    const sid = r.sessionId(a[0]);
    const name = validImportName(a[1]);
    const dir = await host.pickDirectory();
    if (dir === null) return null;
    const c = r.client();
    await r.lease(c, sid);
    const report = await importAgentConfig(c, sid, dir, name, false);
    const loaded = await loadExtension(c, sid, report.extensionPath, report.manifestDigest);
    return importInfo(report, loaded.quarantined);
  });
  // Open the source of a row: the path comes from the Core's own listing, looked up by the row's key, never from the renderer.
  r.handle("customize:openSource", async (...a) => {
    const kind = a[0];
    if (kind !== "skill" && kind !== "extension") bad("kind must be skill or extension");
    let path: string | undefined;
    if (kind === "skill") {
      const name = validSkillName(a[1]);
      const tid = optionalTask(r, a[3]);
      path = (await r.client().listSkills(tid)).skills.find((s) => s.name === name)?.source;
    } else {
      const sid = r.sessionId(a[2]);
      const extId = validExtensionId(a[1]);
      path = (await listHooks(r.client(), { sessionId: sid })).extensionRows.find((e) => e.extensionId === extId)?.path;
    }
    if (!path) return bad("the Core lists no such source");
    host.reveal(path);
    return { opened: path };
  });
}
