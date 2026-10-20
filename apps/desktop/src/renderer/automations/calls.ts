/**
 * The Core calls the automations surface makes. The app uses the preload
 * bridge (every call is a typed, argument-validated main-process handler over
 * the Core's own command); the model-free gallery replaces them with fixtures
 * so the same components render every state with no Core behind them.
 */
import type { AutoDetail, AutoEnableInput, AutoKillReport, AutoList, AutoRepoLoad, AutoRun, AutoRunInput, AutoRunStarted, AutoValidation, AutoView } from "../../shared/automation-types.ts";

export interface AutomationCalls {
  list: () => Promise<AutoList>;
  get: (automationId: string) => Promise<AutoDetail>;
  runs: (options: { automationId?: string; limit?: number }) => Promise<AutoRun[]>;
  validate: (definitionJson: string) => Promise<AutoValidation>;
  create: (definitionJson: string, workspaceRoot: string, commandId: string) => Promise<AutoView>;
  update: (automationId: string, definitionJson: string, commandId: string) => Promise<AutoView>;
  loadRepository: (workspaceRoot: string) => Promise<AutoRepoLoad>;
  enable: (approval: AutoEnableInput, commandId: string) => Promise<AutoView>;
  disable: (automationId: string) => Promise<AutoView>;
  pause: (automationId: string, paused: boolean) => Promise<unknown>;
  kill: (automationId: string) => Promise<AutoKillReport>;
  run: (request: AutoRunInput, commandId: string) => Promise<AutoRunStarted>;
  ack: (dispatchKey: string) => Promise<unknown>;
}

export const bridgeCalls: AutomationCalls = {
  list: () => window.modbit.automations(),
  get: (id) => window.modbit.automation(id),
  runs: (o) => window.modbit.automationRuns(o),
  validate: (json) => window.modbit.validateAutomation(json),
  create: (json, root, commandId) => window.modbit.createAutomation(json, root, commandId),
  update: (id, json, commandId) => window.modbit.updateAutomation(id, json, commandId),
  loadRepository: (root) => window.modbit.loadRepositoryAutomations(root),
  enable: (approval, commandId) => window.modbit.enableAutomation(approval, commandId),
  disable: (id) => window.modbit.disableAutomation(id),
  pause: (id, paused) => window.modbit.pauseAutomation(id, paused),
  kill: (id) => window.modbit.killAutomation(id),
  run: (request, commandId) => window.modbit.runAutomation(request, commandId),
  ack: (key) => window.modbit.ackAutomationAttention(key),
};

/** 32 hex characters: the id a retried command keeps so it acts once (a create's id becomes the definition's id). */
export function newCommandId(): string {
  const b = new Uint8Array(16);
  crypto.getRandomValues(b);
  return Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
}
