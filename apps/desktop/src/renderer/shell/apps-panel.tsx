import { Tabs, type TabDef } from "@modbit/ui";
import type { TerminalViewJson } from "../../preload/preload.ts";
import { BrowserApp } from "../apps/browser-app.tsx";
import { ChangesApp } from "../apps/changes-app.tsx";
import { EvidenceApp } from "../apps/evidence-app.tsx";
import { FilesApp } from "../apps/files-app.tsx";
import { TerminalApp } from "../apps/terminal-app.tsx";
import { APP_LABEL } from "./artifacts.ts";
import { APP_KINDS, type AppKind } from "./prefs.ts";

export interface AppsPanelProps {
  sessionId: string;
  taskId: string;
  taskTitle: string;
  taskState: string;
  artifacts: readonly AppKind[];
  tab: AppKind;
  onTab: (tab: AppKind) => void;
  /** Changes whenever the task moves (its last event offset): the apps read the Core again. */
  refreshKey: string;
  terminals: readonly TerminalViewJson[];
  selectedTerminalId: string | null;
  onSelectTerminal: (terminalId: string) => void;
  onTerminalsChanged: () => void;
  announce: (text: string) => void;
  onOpenReview: () => void;
  onOpenBrowser: () => void;
}

/**
 * The typed apps-panel host (AFW-A01, AFW-I01, REQ-PX-048): a tab per app kind
 * with the tab and the terminal remembered per task. Every app is a view over
 * typed Core reads through the preload bridge; none holds task state, and an
 * app with nothing to show says so.
 */
export function AppsPanel(p: AppsPanelProps) {
  const tabs: TabDef[] = APP_KINDS.map((k) => ({ id: k, label: APP_LABEL[k], ...(k === "terminal" && p.terminals.length > 0 ? { badge: String(p.terminals.length) } : {}) }));
  return (
    <div className="apps-panel" data-testid="apps-panel" data-task-id={p.taskId}>
      <p className="meta apps-panel-task" title={p.taskTitle}>
        {p.taskTitle}
      </p>
      <Tabs label="Apps" tabs={tabs} selected={p.tab} onSelect={(id) => p.onTab(id as AppKind)} idPrefix="apps" testId="apps-tabs">
        {p.tab === "changes" &&
          (p.artifacts.includes("changes") ? (
            <ChangesApp taskId={p.taskId} taskState={p.taskState} refreshKey={p.refreshKey} onOpenReview={p.onOpenReview} />
          ) : (
            <p className="meta" data-testid="app-changes-empty">
              This task has no change set to show yet.
            </p>
          ))}
        {p.tab === "terminal" && <TerminalApp sessionId={p.sessionId} taskId={p.taskId} terminals={p.terminals} selectedId={p.selectedTerminalId} onSelect={p.onSelectTerminal} onChanged={p.onTerminalsChanged} announce={p.announce} />}
        {p.tab === "browser" && <BrowserApp taskId={p.taskId} onOpenBrowser={p.onOpenBrowser} />}
        {p.tab === "files" &&
          (p.artifacts.includes("files") ? (
            <FilesApp taskId={p.taskId} refreshKey={p.refreshKey} />
          ) : (
            <p className="meta" data-testid="app-files-empty">
              The task has not started, so there is no workspace to browse yet.
            </p>
          ))}
        {p.tab === "evidence" &&
          (p.artifacts.includes("evidence") ? (
            <EvidenceApp taskId={p.taskId} refreshKey={p.refreshKey} />
          ) : (
            <p className="meta" data-testid="app-evidence-empty">
              The task has not started, so there is no evidence yet.
            </p>
          ))}
      </Tabs>
    </div>
  );
}
