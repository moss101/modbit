/**
 * The Fleet screen and its overlays (Review, Browser, Dashboard), exactly as
 * the single-file renderer composed them, now the content of the shell's
 * conversation region. The agent list and conversation proper are PX-046 and
 * PX-047; until then this view is the supervision console (M1.4, M6.6).
 */
import { Browser } from "../browser/browser.tsx";
import { Dashboard } from "../dashboard/dashboard.tsx";
import { Review } from "../review/review.tsx";
import type { AppState } from "../state/use-app.ts";
import { Board } from "./board.tsx";
import { Composer } from "./composer.tsx";
import { SettingsPanel } from "./settings-panel.tsx";
import { StatusRegion } from "./status-region.tsx";

export function FleetView({ app }: { app: AppState }) {
  const { model, reviewing, browsing, dashboardOpen, setDashboardOpen, runFleetCommand, openBrowser } = app;
  return (
    <>
      <StatusRegion app={app} />
      {dashboardOpen && model.sessionId && <Dashboard sessionId={model.sessionId} onClose={() => setDashboardOpen(false)} />}
      {reviewing && model.sessionId && <Review taskId={reviewing} sessionId={model.sessionId} card={model.tasks.get(reviewing) ?? null} onClose={() => void runFleetCommand("back")} />}
      {browsing && !reviewing && <Browser browsing={browsing} card={model.tasks.get(browsing.taskId) ?? null} sessionId={model.sessionId} onReopen={() => void openBrowser(browsing.taskId)} onClose={() => void runFleetCommand("back")} />}
      <div className="fleet-main" hidden={reviewing !== null || browsing !== null}>
        <div className="side">
          <Composer app={app} />
          <SettingsPanel app={app} />
        </div>
        <Board app={app} />
      </div>
    </>
  );
}
