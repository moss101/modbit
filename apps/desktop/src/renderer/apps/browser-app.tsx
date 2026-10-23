import { useCallback, useEffect, useState } from "react";
import { Badge, Button } from "@modbit/ui";
import type { BrowserViewDescription } from "../../preload/preload.ts";

/**
 * The Browser tab (REQ-PX-048): the task's browser session as the panel shows
 * it, with the control-lease badge and the takeover control. The live page
 * itself is a native view the host places over the Browser screen, so "Open
 * browser" leads there; this tab never embeds page content (it stays out of
 * the renderer, docs/22).
 */
export function BrowserApp({ taskId, onOpenBrowser }: { taskId: string; onOpenBrowser: () => void }) {
  const [views, setViews] = useState<{ browserSessionId: string; url: string; shown: boolean; reclaimed: boolean }[]>([]);
  const [host, setHost] = useState<BrowserViewDescription | null>(null);
  const [busy, setBusy] = useState(false);
  const refresh = useCallback(async () => {
    try {
      const list = await window.modbit.listBrowserViews(taskId);
      setViews(list);
      const first = list[0];
      setHost(first ? await window.modbit.describeBrowser(first.browserSessionId, taskId) : null);
    } catch {
      setViews([]);
      setHost(null);
    }
  }, [taskId]);
  useEffect(() => {
    void refresh();
    return window.modbit.onBrowserState(() => void refresh());
  }, [refresh]);

  const setControl = async (controller: "AGENT" | "USER") => {
    if (!host || busy) return;
    setBusy(true);
    try {
      await window.modbit.setBrowserControl(host.browserSessionId, controller);
      await refresh();
    } finally {
      setBusy(false);
    }
  };
  const user = host?.controller === "USER";
  return (
    <div className="browser-app" data-testid="app-browser">
      {views.length === 0 ? (
        <p className="meta" data-testid="app-browser-empty">
          This task has no browser session yet.
        </p>
      ) : (
        <>
          <p>
            <Badge tone={user ? "accent" : "info"}>{user ? "You hold control" : "Agent holds control"}</Badge>
            <span className="meta" data-testid="browser-tab-lease">
              {" "}
              lease generation {host?.leaseGeneration ?? 0}
            </span>
          </p>
          <p className="meta" data-testid="browser-tab-page">
            {host?.title || "untitled"} · {host?.url || "no page yet"}
          </p>
          <div className="actions">
            {user ? (
              <Button onClick={() => void setControl("AGENT")} disabled={busy} data-testid="browser-tab-return">
                Return control to the agent
              </Button>
            ) : (
              <Button onClick={() => void setControl("USER")} disabled={busy} data-testid="browser-tab-take">
                Take control
              </Button>
            )}
            <Button variant="primary" data-testid="panel-open-browser" onClick={onOpenBrowser}>
              Open browser
            </Button>
          </div>
        </>
      )}
    </div>
  );
}
