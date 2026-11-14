/**
 * The Activity pane (REQ-PX-202; DR-PX-2026-11-11-021 MWS-10): what the task has done, in the order of the Core's log, with a
 * filter by kind and a field to find text. It lists what this window was already given (the transcript rows the conversation
 * read and the events main pushed: apps/delivered.ts) and makes no read of its own, so opening it, filtering and finding ask the
 * Core for nothing.
 */
import { useMemo, useState } from "react";
import { Icon } from "@modbit/ui";
import { PaneActions } from "../shell/pane.tsx";
import { ACTIVITY_KINDS, KIND_LABEL, activityOf, clock, filterActivity, kindCounts, type ActivityKind } from "./activity-model.ts";
import { useDelivered } from "./delivered.ts";

export function ActivityApp({ taskId }: { taskId: string }) {
  const d = useDelivered(taskId);
  const entries = useMemo(() => activityOf(d.rows, d.events), [d]);
  const counts = useMemo(() => kindCounts(entries), [entries]);
  const [picked, setPicked] = useState<ActivityKind | "all">("all");
  const [query, setQuery] = useState("");
  // A kind whose last entry left (a re-read folded it) has no filter any more: the list shows everything again.
  const kind = picked !== "all" && !counts[picked] ? "all" : picked;
  const shown = useMemo(() => filterActivity(entries, kind, query), [entries, kind, query]);
  return (
    <div className="activity-app" data-testid="app-activity" data-entries={entries.length} data-shown={shown.length} data-kind={kind} data-as-of={d.asOfOffset}>
      <PaneActions className="activity-tools">
        <label className="activity-find">
          <Icon name="search" size={14} />
          <input type="search" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Find in activity" aria-label="Find in activity" data-testid="activity-find" />
        </label>
      </PaneActions>
      {entries.length === 0 ? (
        <p className="meta" data-testid="activity-empty">
          Nothing of this task has reached this window yet. What it does is listed here as it happens, and its conversation's steps once the conversation has been opened.
        </p>
      ) : (
        <>
          <div className="activity-kinds" role="group" aria-label="Filter activity by kind" data-testid="activity-kinds">
            <button type="button" className="activity-kind" aria-pressed={kind === "all"} onClick={() => setPicked("all")} data-testid="activity-kind-all">
              All <span className="activity-count">{entries.length}</span>
            </button>
            {ACTIVITY_KINDS.filter((k) => counts[k]).map((k) => (
              <button key={k} type="button" className="activity-kind" aria-pressed={kind === k} onClick={() => setPicked(k)} data-testid={`activity-kind-${k}`}>
                {KIND_LABEL[k]} <span className="activity-count">{counts[k]}</span>
              </button>
            ))}
          </div>
          <p className="mb-sr-only" role="status" data-testid="activity-shown">
            {shown.length} of {entries.length} shown
          </p>
          {shown.length === 0 ? (
            <p className="meta" data-testid="activity-none">
              Nothing matches.
            </p>
          ) : (
            <ol className="activity-list" aria-label="Activity, oldest first" data-testid="activity-list">
              {shown.map((e) => (
                <li key={e.key} className="activity-entry" data-testid="activity-entry" data-key={e.key} data-kind={e.kind} data-source={e.source} data-offset={e.offset} data-tone={e.tone}>
                  <span className="activity-text" title={e.text}>
                    {e.text}
                  </span>
                  {e.atMs > 0 && (
                    <time className="activity-time" dateTime={new Date(e.atMs).toISOString()}>
                      {clock(e.atMs)}
                    </time>
                  )}
                </li>
              ))}
            </ol>
          )}
        </>
      )}
    </div>
  );
}
