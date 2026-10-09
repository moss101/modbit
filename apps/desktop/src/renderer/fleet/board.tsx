import { childrenOf } from "../model.ts";
import { taskState } from "../screens.ts";
import { Card } from "./card.tsx";
import { COLUMNS } from "./columns.ts";
import type { AppState } from "../state/use-app.ts";

export function Board({ app }: { app: AppState }) {
  const { setSelectedTaskId, core, model, screen, goal, attentionItems, setReviewing, filter, setFilter, focusedTask, startTask, openBrowser, cols, attention, visible, tasks, fleet } = app;
  return (
    <div>
      <label htmlFor="search" className="sr-only">Filter tasks</label>
      <input id="search" data-testid="search" type="search" value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="Filter tasks (/)" aria-label="Filter tasks by goal" />
      <p className="sr-only" aria-live="polite">{attention} tasks need attention</p>
      {attentionItems.length > 0 && (
        <section className="attention" data-testid="attention" aria-label="attention items" tabIndex={-1}>
          <h2>Attention ({attentionItems.length})</h2>
          <ul>
            {attentionItems.map((i) => (
              <li key={`${i.kind}:${i.taskId}:${i.reference}`} data-testid="attention-item" data-kind={i.kind} data-task-id={i.taskId} tabIndex={-1}>
                <strong>{i.kind}</strong> · {i.reason} · <em>{i.action}</em>
              </li>
            ))}
          </ul>
        </section>
      )}
      {screen === "loading" && <p className="meta" data-testid="fleet-loading">Loading fleet from the Core…</p>}
      {screen === "empty" && <p className="empty" data-testid="fleet-empty">No tasks yet. Create one on the left.</p>}
      <div className="fleet" role="region" aria-label="Fleet board" tabIndex={0} data-testid="fleet" data-screen={screen}>
        {COLUMNS.map((c) => (
          <section className="column" key={c.key} aria-label={`${c.title}, ${cols[c.key].length} task(s)`} data-testid={`column-${c.key}`} tabIndex={-1}>
            <h2>
              {c.title} <span aria-hidden="true">({cols[c.key].length})</span>
            </h2>
            {cols[c.key].length === 0 ? <p className="empty">None</p> : visible(cols[c.key]).map((t) => <Card key={t.taskId} card={t} children={childrenOf(model, t.taskId)} state={taskState({ card: t, core, attention: attentionItems })} sessionId={model.sessionId} onFocus={(id) => {
                  focusedTask.current = id;
                  setSelectedTaskId(id);
                }} onStart={startTask} onReview={(id) => setReviewing(id)} onBrowser={(id) => void openBrowser(id)} />)}
          </section>
        ))}
      </div>
    </div>
  );
}
