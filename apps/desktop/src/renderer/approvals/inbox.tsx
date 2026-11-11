/**
 * Inbox (the Approvals page, renamed by DR-PX-2026-11-11-021 MWS-04): what needs the person across every task, as the Core's
 * attention items and the notifications derived from them. Opening a row goes
 * to the task, where the decision is made with its exact intent; nothing is
 * decided from this list.
 */
import { Icon } from "@modbit/ui";
import type { AppState } from "../state/use-app.ts";

export function Inbox({ app, onOpenTask }: { app: AppState; onOpenTask: (taskId: string) => void }) {
  const { attentionItems, notifications, delivered, model, open } = app;
  const titleOf = (id: string) => model.tasks.get(id)?.goalText ?? id.slice(0, 8);
  const empty = attentionItems.length === 0 && notifications.length === 0;
  return (
    <div className="page" data-testid="inbox">
      <div className="page-col">
        <h2 className="page-title">Inbox</h2>
        {empty && (
          <p className="meta" data-testid="inbox-empty">
            Nothing needs you right now.
          </p>
        )}
        {attentionItems.length > 0 && (
          <section className="attention" data-testid="attention" aria-label="attention items" tabIndex={-1}>
            <ul className="page-list">
              {attentionItems.map((i) => (
                <li key={`${i.kind}:${i.taskId}:${i.reference}`}>
                  <button type="button" className="page-row" data-testid="attention-item" data-kind={i.kind} data-task-id={i.taskId} onClick={() => onOpenTask(i.taskId)}>
                    <Icon name="shield-check" size={14} />
                    <span className="page-row-main">
                      <span className="page-row-title">{titleOf(i.taskId)}</span>
                      <span className="meta">
                        {i.kind} · {i.reason} · {i.action}
                      </span>
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )}
        {notifications.length > 0 && (
          <section className="notifications" data-testid="notifications" aria-label="notifications" aria-live="polite">
            <h3 className="page-heading">Notifications</h3>
            <ul className="page-list">
              {notifications.map((n) => (
                <li key={n.id} className="page-note" data-testid="notification" data-kind={n.kind} data-id={n.id} data-coalesced={n.coalesced} data-count={n.count ?? 1} data-delivered={delivered.includes(`${n.id}:${n.reason}`) ? "true" : "false"}>
                  <span className="page-row-main">
                    <span className="page-row-title">{n.title}</span>
                    <span className="meta">
                      {n.reason} · {n.nextAction}
                    </span>
                  </span>
                  <button type="button" data-testid="notification-open" onClick={() => open(n.deepLink)}>
                    Open {n.deepLink.screen}
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )}
      </div>
    </div>
  );
}
