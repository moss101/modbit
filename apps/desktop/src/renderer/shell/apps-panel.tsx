import { Button, Tabs, type TabDef } from "@modbit/ui";
import { APP_LABEL, APP_PENDING } from "./artifacts.ts";
import { APP_KINDS, type AppKind } from "./prefs.ts";

export interface AppsPanelProps {
  taskId: string;
  taskTitle: string;
  artifacts: readonly AppKind[];
  tab: AppKind;
  onTab: (tab: AppKind) => void;
  onOpenReview: () => void;
  onOpenBrowser: () => void;
}

/**
 * The typed apps-panel host (AFW-A01, AFW-I01): a tab per app kind with the
 * task's state remembered per task. Changes and Browser lead into the existing
 * Review and Browser screens; Terminal, Files and Evidence are the extension
 * points PX-048 fills, and say so instead of showing anything made up.
 */
export function AppsPanel({ taskId, taskTitle, artifacts, tab, onTab, onOpenReview, onOpenBrowser }: AppsPanelProps) {
  const tabs: TabDef[] = APP_KINDS.map((k) => ({ id: k, label: APP_LABEL[k] }));
  return (
    <div className="apps-panel" data-testid="apps-panel" data-task-id={taskId}>
      <p className="meta apps-panel-task" title={taskTitle}>
        {taskTitle}
      </p>
      <Tabs label="Apps" tabs={tabs} selected={tab} onSelect={(id) => onTab(id as AppKind)} idPrefix="apps" testId="apps-tabs">
        {tab === "changes" &&
          (artifacts.includes("changes") ? (
            <div data-testid="app-changes">
              <p>This task has a change set ready to review.</p>
              <Button variant="primary" data-testid="panel-open-review" onClick={onOpenReview}>
                Open review
              </Button>
            </div>
          ) : (
            <p className="meta" data-testid="app-changes-empty">
              This task has no change set to show yet.
            </p>
          ))}
        {tab === "browser" &&
          (artifacts.includes("browser") ? (
            <div data-testid="app-browser">
              <p>This task has a browser session.</p>
              <Button variant="primary" data-testid="panel-open-browser" onClick={onOpenBrowser}>
                Open browser
              </Button>
            </div>
          ) : (
            <p className="meta" data-testid="app-browser-empty">
              This task has no browser session yet.
            </p>
          ))}
        {tab !== "changes" && tab !== "browser" && (
          <p className="meta" data-testid={`app-${tab}-pending`}>
            {APP_PENDING[tab]}
          </p>
        )}
      </Tabs>
    </div>
  );
}
