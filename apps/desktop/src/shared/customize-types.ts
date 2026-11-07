/**
 * The plain shapes the Customize view (REQ-PX-106) crosses the preload bridge
 * as. Main converts the Core's protobuf messages into these; the renderer
 * shows them and decides none of it: scope, trust, provenance, the budget
 * share, which hook a person may switch off and which extension manifest may
 * be released are the Core's answers (docs/81). There is no field for a
 * catalog, a network location to install from or an update.
 */

export interface SkillRow {
  name: string;
  version: string;
  description: string;
  /** SYSTEM | PROJECT | EXTENSION | USER */
  scope: string;
  contentHash: string;
  /** SYSTEM | SIGNED | TRUSTED_BY_OWNER | DEV_FLAG | UNTRUSTED | CHANGED_SINCE_TRUSTED | REVOKED | FORBIDDEN */
  trust: string;
  trustDetail: string;
  enabled: boolean;
  /** BOTH | USER_ONLY */
  invocation: string;
  paths: string[];
  pathsActive: boolean;
  indexTokens: number;
  indexed: boolean;
  indexForm: string;
  source: string;
  provenanceSource: string;
  provenanceAuthor: string;
  provenanceLicense: string;
  lifecycle: string;
}

export interface SkillsView {
  skills: SkillRow[];
  rejected: { source: string; code: string; reason: string }[];
  indexBudgetTokens: number;
  indexUsedTokens: number;
  indexOmitted: number;
  systemRoot: string;
}

export interface SkillTrustInfo {
  name: string;
  contentHash: string;
  trust: string;
  records: number;
}

export interface HookRow {
  id: string;
  source: string;
  point: string;
  mode: string;
  failPolicy: string;
  timeoutMs: string;
  tools: string[];
  command: string[];
  /** DEVICE | ADMIN | PROJECT | USER | EXTENSION */
  scope: string;
  enabled: boolean;
  canDisable: boolean;
  trustedWorkspaceGate: boolean;
  lastOutcome: string;
  lastDetail: string;
  lastDurationMs: string;
  invocations: number;
}

export interface ToolServerInfo {
  name: string;
  transport: string;
  /** CONFIGURED | INERT: what the Core knows without starting the server; not a liveness probe. */
  state: string;
  credentialRequired: boolean;
  credentialHeld: boolean;
  scopes: string[];
}

export interface ExtensionRowInfo {
  extensionId: string;
  name: string;
  version: string;
  scope: string;
  /** ACTIVE | QUARANTINED */
  state: string;
  quarantineReason: string;
  signature: string;
  manifestDigest: string;
  path: string;
  publisher: string;
  source: string;
  hooks: string[];
  commands: string[];
  providers: string[];
  servers: ToolServerInfo[];
  capabilities: string[];
  canTrust: boolean;
}

export interface CustomizeHooksView {
  hooks: HookRow[];
  refused: string[];
  extensions: ExtensionRowInfo[];
}

export interface HookEnabledInfo {
  hookId: string;
  enabled: boolean;
  scope: string;
  offset: string;
}

export interface ImportReportInfo {
  name: string;
  extensionPath: string;
  mapped: number;
  skipped: number;
  conflicts: number;
  manifestDigest: string;
  signature: string;
  /** What loading it did: the extension arrives quarantined, "" when it is active. */
  quarantined: string;
  items: { kind: string; source: string; target: string; status: string; reason: string }[];
}

/** One memory item as the Memory view shows it (PX-188): source, scope and status are the Core's answers. */
export interface MemoryRowInfo {
  id: string;
  /** kind:id of the scope the item lives in. */
  scope: string;
  recordType: string;
  topic: string;
  content: string;
  /** user_stated | agent_observed | transcript_summary | ... */
  source: string;
  author: string;
  sensitivity: string;
  /** proposed | curated | superseded | expired */
  status: string;
  validated: boolean;
}

export interface MemoryView {
  items: MemoryRowInfo[];
  scopes: string[];
}

export interface MemoryForgetInfo {
  memoryId: string;
  changed: boolean;
  status: string;
}

export interface MemoryShareInfo {
  memoryId: string;
  sharedMemoryId: string;
  scope: string;
  /** shared | refused | unknown */
  outcome: string;
  refusalCode: string;
  refusalDetail: string;
  existed: boolean;
}

export interface MemoryRememberInfo {
  memoryId: string;
  status: string;
  scope: string;
  existed: boolean;
}

export interface MemoryPromoteInfo {
  memoryId: string;
  /** curated | refused | unknown */
  outcome: string;
  refusalCode: string;
  refusalDetail: string;
}
