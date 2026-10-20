/**
 * The Automations surface (REQ-PX-086; docs/68 AUT-A04, AUT-D03, AUT-D05,
 * AUT-E01, AUT-E02): the list of definitions with their states, triggers and
 * next due times, the global and per-definition pause, the attention items and
 * what clears each, the definition editor with the Core's live validation, the
 * enable approval, the test run, the run history with typed reasons and the
 * kill switches. The renderer holds no authority: every effect is a typed
 * preload call into the main process, which validates it and asks the Core;
 * every word about validity, state, hash and reach is the Core's.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { Badge, Button, StatusDot } from "@modbit/ui";
import type { AutoAttention, AutoDetail, AutoRun, AutoView } from "../../shared/automation-types.ts";
import { bridgeCalls, newCommandId, type AutomationCalls } from "./calls.ts";
import { EnableDialog, KillDialog, TestRunDialog } from "./dialogs.tsx";
import { DefinitionEditor, type EditorTarget } from "./editor.tsx";
import { ATTENTION_META, approvalBlocked, attentionAction, costWords, durationWords, nextDueWords, reasonWords, refusalSentence, relativeTo, runKinds, runStatusMeta, runSummary, STARTER_DEFINITION, stateMeta, utcMinute } from "./model.ts";
import { useAutomations } from "./use-automations.ts";

const WORKSPACE_KEY = "modbit.automations.workspace";

function loadWorkspace(): string {
  try {
    return localStorage.getItem(WORKSPACE_KEY) ?? "";
  } catch {
    return "";
  }
}

function pretty(json: string): string {
  try {
    return JSON.stringify(JSON.parse(json), null, 2);
  } catch {
    return json;
  }
}

/** What the gallery opens a surface on; the app never passes it. */
export interface AutomationsInitial {
  selectedId?: string;
  editor?: EditorTarget;
  workspace?: string;
  /** Distinguishes the landmarks of several surfaces on one page (the gallery); the app has one. */
  labelSuffix?: string;
}

export interface AutomationsProps {
  calls?: AutomationCalls;
  connected: boolean;
  /** Jumps to a task's conversation (the agent list selects it). */
  onOpenTask: (taskId: string) => void;
  onClose?: (() => void) | undefined;
  initial?: AutomationsInitial | undefined;
}

export function Automations({ calls = bridgeCalls, connected, onOpenTask, onClose, initial }: AutomationsProps) {
  const data = useAutomations(calls, connected);
  const suffix = initial?.labelSuffix ? ` (${initial.labelSuffix})` : "";
  const { list, runs } = data;
  const [selectedId, setSelectedId] = useState<string | null>(initial?.selectedId ?? null);
  const [detail, setDetail] = useState<AutoDetail | null>(null);
  const [editor, setEditor] = useState<EditorTarget | null>(initial?.editor ?? null);
  const [enableSnap, setEnableSnap] = useState<{ view: AutoView; revision: string } | null>(null);
  const [testFor, setTestFor] = useState<AutoView | null>(null);
  const [killTarget, setKillTarget] = useState<{ automationId: string; name: string } | null>(null);
  const [notice, setNotice] = useState<{ text: string; tone: "info" | "error" } | null>(null);
  const [workspace, setWorkspace] = useState(initial?.workspace ?? loadWorkspace());
  const [repoProblems, setRepoProblems] = useState<{ path: string; code: string; message: string }[]>([]);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    try {
      if (workspace) localStorage.setItem(WORKSPACE_KEY, workspace);
    } catch {
      // A profile that refuses storage still works; the folder is just not remembered.
    }
  }, [workspace]);

  const views = list?.automations ?? [];
  const selected = views.find((v) => v.automationId === selectedId) ?? null;
  const nowMs = list?.nowMs ?? Date.now();
  const selectedRuns = useMemo(() => (selected ? runs.filter((r) => r.automationId === selected.automationId) : []), [runs, selected]);

  // The version table of the selected definition, read again whenever its version or hash moves.
  const detailKey = selected ? `${selected.automationId}:${selected.currentVersion}:${selected.definitionHash}:${selected.state}` : "";
  useEffect(() => {
    if (!selected) {
      setDetail(null);
      return;
    }
    let live = true;
    calls
      .get(selected.automationId)
      .then((d) => live && setDetail(d))
      .catch(() => live && setDetail(null));
    return () => {
      live = false;
    };
    // `calls` is fixed for the life of the surface.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detailKey]);

  const say = useCallback((text: string, tone: "info" | "error" = "info") => setNotice({ text, tone }), []);
  const fail = useCallback((e: unknown) => say(refusalSentence((e as Error).message), "error"), [say]);
  const act = useCallback(
    async (fn: () => Promise<void>) => {
      setBusy(true);
      try {
        await fn();
      } catch (e) {
        fail(e);
      } finally {
        setBusy(false);
        void data.refresh();
      }
    },
    [data, fail],
  );

  const openEnable = useCallback(
    async (id: string) => {
      try {
        const d = await calls.get(id);
        if (!d.view) return;
        const rev = d.versions.find((v) => v.version === d.view!.currentVersion)?.sourceRevision ?? "";
        setSelectedId(id);
        setEnableSnap({ view: d.view, revision: rev });
      } catch (e) {
        fail(e);
      }
    },
    [calls, fail],
  );

  const openEditor = (v: AutoView | null) => {
    if (v === null) setEditor({ automationId: null, text: STARTER_DEFINITION, workspaceRoot: workspace });
    else if (v.sourceKind === "repository") setEditor({ automationId: v.automationId, text: pretty(v.definitionJson), workspaceRoot: v.workspaceRoot, version: v.currentVersion, readOnlyReason: `This definition is supplied by ${v.sourcePath}. It is changed in that file and loaded again; it is shown here as data.` });
    else setEditor({ automationId: v.automationId, text: pretty(v.definitionJson), workspaceRoot: v.workspaceRoot, version: v.currentVersion });
  };

  const loadRepository = (root: string) =>
    act(async () => {
      if (!root.trim()) {
        say("Enter the workspace folder whose .modbit/automations files should be read.", "error");
        return;
      }
      const res = await calls.loadRepository(root.trim());
      setRepoProblems(res.problems);
      const first = res.loaded[0];
      if (first) setSelectedId(first.automationId);
      say(`Read the repository: ${res.loaded.length} definition${res.loaded.length === 1 ? "" : "s"} loaded as data, ${res.problems.length} problem${res.problems.length === 1 ? "" : "s"}. Nothing is enabled until you approve it.`);
    });

  const onAttention = (a: AutoAttention) => {
    const action = attentionAction(a);
    if (action.kind === "acknowledge") void act(async () => void (await calls.ack(a.dispatchKey)));
    else if (action.kind === "review") void openEnable(a.automationId);
    else if (a.taskId) onOpenTask(a.taskId);
  };

  const manualRun = (v: AutoView) =>
    act(async () => {
      const res = await calls.run({ automationId: v.automationId }, newCommandId());
      if (res.status === "running" || res.status === "queued" || res.status === "pending") say(`Run started (${res.status}).`);
      else say(`The run was recorded as ${res.status}${res.reason ? ` (${res.reason})` : ""}: ${res.detail}`, res.status === "failed" ? "error" : "info");
    });

  const globalPaused = list?.globalPaused ?? false;
  const reachable = connected && (list !== null || data.error === null);

  return (
    <section className="autos" aria-label={`Automations${suffix}`} data-testid="automations" data-loaded={data.loaded}>
      <header className="autos-head">
        <h2 className="autos-title">Automations</h2>
        <span className="autos-spacer" />
        <Button size="sm" aria-pressed={globalPaused} onClick={() => void act(async () => void (await calls.pause("", !globalPaused)))} disabled={busy || !list} data-testid="global-pause" data-paused={globalPaused}>
          {globalPaused ? "Resume all automations" : "Pause all automations"}
        </Button>
        <Button size="sm" variant="danger" onClick={() => setKillTarget({ automationId: "", name: "all automations" })} disabled={!list} data-testid="kill-all">
          Kill all…
        </Button>
        {onClose && (
          <Button size="sm" onClick={onClose} data-testid="automations-close">
            Close
          </Button>
        )}
      </header>
      {globalPaused && (
        <p className="autos-note" role="status" data-testid="global-paused-banner">
          <StatusDot status="warn" label="Paused" showLabel /> Every automation is paused: no trigger starts a run, and a slot that comes due is recorded as skipped.
        </p>
      )}
      <p className="meta">
        An automation is a saved definition that makes the Core start an ordinary task when a trigger fires. It runs as you, within a ceiling you approve, and anything protected still asks. Nothing here can be created or enabled by a model.
        {list ? ` Clock: ${list.clock.toLowerCase()}, ${utcMinute(list.nowMs)}.` : ""}
      </p>
      <div className="autos-bar">
        <label className="autos-field autos-field-wide">
          <span>Workspace folder</span>
          <input value={workspace} onChange={(e) => setWorkspace(e.target.value)} placeholder="/path/to/a/trusted/repository" spellCheck={false} data-testid="automations-workspace" />
        </label>
        <Button variant="primary" onClick={() => openEditor(null)} disabled={!reachable} data-testid="automation-new">
          New automation
        </Button>
        <Button onClick={() => void loadRepository(workspace)} disabled={!reachable || busy} data-testid="repository-load">
          Load definitions from the repository
        </Button>
      </div>
      {notice && (
        <p className="autos-note" role={notice.tone === "error" ? "alert" : "status"} data-testid="automations-notice" data-tone={notice.tone}>
          {notice.text}
        </p>
      )}
      {repoProblems.length > 0 && (
        <ul className="autos-issues" aria-label="Repository files that could not be loaded" data-testid="repo-problems">
          {repoProblems.map((p, i) => (
            <li key={i} data-testid="repo-problem" data-code={p.code}>
              <code className="autos-issue-path">{p.path}</code> <strong className="autos-issue-code">{p.code}</strong> <span>{p.message}</span>
            </li>
          ))}
        </ul>
      )}
      {data.error && !list && (
        <p className="autos-note" role="alert" data-testid="automations-error">
          The automations are unavailable: {data.error}
        </p>
      )}
      {!data.loaded && !data.error && (
        <p className="meta" role="status" data-testid="automations-loading">
          Loading automations from the Core…
        </p>
      )}

      {list && list.attention.length > 0 && (
        <section className="autos-attention" aria-label={`Needs attention${suffix}`} data-testid="automation-attention-list" data-count={list.attention.length}>
          <h3 className="autos-h3">Needs attention</h3>
          <ul className="autos-plain">
            {list.attention.map((a, i) => {
              const meta = ATTENTION_META[a.kind] ?? { label: a.kind, tone: "warn" as const };
              const action = attentionAction(a);
              return (
                <li key={`${a.kind}:${a.automationId}:${a.dispatchKey}:${i}`} className="autos-attention-item" data-testid="automation-attention" data-kind={a.kind} data-automation-id={a.automationId}>
                  <StatusDot status={meta.tone} label={meta.label} showLabel /> <strong>{a.name}</strong>
                  <span className="meta"> {a.reason}</span>
                  <Button size="sm" onClick={() => onAttention(a)} disabled={busy} data-testid="attention-action" data-action={action.kind}>
                    {action.label}
                  </Button>
                </li>
              );
            })}
          </ul>
        </section>
      )}

      {list && views.length === 0 && (
        <div className="gallery-card autos-empty" data-testid="automations-empty">
          <p>
            <StatusDot status="idle" label="Empty" showLabel /> <strong>No automations yet</strong>
          </p>
          <p className="meta">Write a definition (a schedule, a repository event, a signed webhook or a manual run), or read the ones a repository carries in .modbit/automations. A definition from a repository is data until you approve it.</p>
        </div>
      )}

      {views.length > 0 && (
        <div className="autos-table-wrap">
          <table className="autos-table" data-testid="automation-list" data-count={views.length}>
            <caption className="mb-sr-only">Automations</caption>
            <thead>
              <tr>
                <th scope="col">Name</th>
                <th scope="col">State</th>
                <th scope="col">Triggers</th>
                <th scope="col">Runs as</th>
                <th scope="col">Source</th>
                <th scope="col">Last run</th>
                <th scope="col">Pause</th>
              </tr>
            </thead>
            <tbody>
              {views.map((v) => {
                const meta = stateMeta(v.state);
                return (
                  <tr key={v.automationId} data-testid="automation-row" data-automation-id={v.automationId} data-name={v.name} data-state={v.state} data-paused={v.paused} data-selected={v.automationId === selectedId}>
                    <th scope="row">
                      <button type="button" className="autos-link" aria-current={v.automationId === selectedId ? "true" : undefined} onClick={() => setSelectedId(v.automationId)} data-testid="automation-select">
                        {v.name}
                      </button>
                      {v.description && <div className="meta">{v.description}</div>}
                    </th>
                    <td>
                      <span data-testid="automation-state" data-state={v.state}>
                        <StatusDot status={meta.tone} label={meta.label} showLabel />
                      </span>
                      {v.paused && v.state !== "PAUSED" && <div className="meta">pause set</div>}
                      {v.contentChanged && (
                        <div className="meta" data-testid="row-content-changed">
                          file changed
                        </div>
                      )}
                    </td>
                    <td>
                      <ul className="autos-plain" data-testid="automation-triggers">
                        {v.triggers.map((t) => (
                          <li key={t.id} data-kind={t.kind} data-next-due={t.nextDueMs}>
                            {t.summary}
                            {t.kind === "schedule" && <div className="meta">{nextDueWords(t, nowMs)}</div>}
                          </li>
                        ))}
                      </ul>
                    </td>
                    <td data-testid="automation-principal">{v.principal}</td>
                    <td data-testid="automation-source" data-source={v.sourceKind}>
                      {v.sourceKind === "repository" ? <code>{v.sourcePath}</code> : "local"}
                    </td>
                    <td data-testid="automation-last-run" data-status={v.lastRunStatus}>
                      {v.lastRunMs ? (
                        <>
                          {runStatusMeta(v.lastRunStatus).label}
                          <div className="meta">{relativeTo(v.lastRunMs, nowMs)}</div>
                        </>
                      ) : (
                        <span className="meta">never</span>
                      )}
                    </td>
                    <td>
                      <Button size="sm" aria-pressed={v.paused} aria-label={`${v.paused ? "Resume" : "Pause"} ${v.name}`} disabled={busy} onClick={() => void act(async () => void (await calls.pause(v.automationId, !v.paused)))} data-testid="row-pause" data-paused={v.paused}>
                        {v.paused ? "Resume" : "Pause"}
                      </Button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      {selected && (
        <Detail
          labelSuffix={suffix}
          view={selected}
          detail={detail}
          runs={selectedRuns}
          nowMs={nowMs}
          busy={busy}
          onEdit={() => openEditor(selected)}
          onEnable={() => void openEnable(selected.automationId)}
          onDisable={() => void act(async () => void (await calls.disable(selected.automationId)))}
          onTest={() => setTestFor(selected)}
          onRun={() => void manualRun(selected)}
          onKill={() => setKillTarget({ automationId: selected.automationId, name: selected.name })}
          onReload={() => void loadRepository(selected.workspaceRoot)}
          onOpenTask={onOpenTask}
          onAck={(key) => void act(async () => void (await calls.ack(key)))}
        />
      )}

      {editor && (
        <DefinitionEditor
          key={`${editor.automationId ?? "new"}:${editor.text.length}:${editor.version ?? 0}`}
          calls={calls}
          target={editor}
          nowMs={nowMs}
          onSaved={(view) => {
            setSelectedId(view.automationId);
            void data.refresh();
          }}
          onClose={() => setEditor(null)}
        />
      )}

      <EnableDialog
        snapshot={enableSnap?.view ?? null}
        sourceRevision={enableSnap?.revision ?? ""}
        calls={calls}
        onClose={() => setEnableSnap(null)}
        onEnabled={(view) => {
          setEnableSnap(null);
          say(`“${view.name}” version ${view.currentVersion} is approved and ${view.state === "ENABLED" ? "enabled" : view.state.toLowerCase()}.`);
          void data.refresh();
        }}
        onReview={() => enableSnap && void openEnable(enableSnap.view.automationId)}
      />
      <TestRunDialog view={testFor} runs={runs} calls={calls} onClose={() => setTestFor(null)} onRefresh={() => void data.refresh()} onOpenTask={(id) => { setTestFor(null); onOpenTask(id); }} />
      <KillDialog target={killTarget} calls={calls} onClose={() => setKillTarget(null)} onDone={() => void data.refresh()} />
    </section>
  );
}

// ---------------------------------------------------------------- the detail

interface DetailProps {
  labelSuffix?: string;
  view: AutoView;
  detail: AutoDetail | null;
  runs: AutoRun[];
  nowMs: number;
  busy: boolean;
  onEdit: () => void;
  onEnable: () => void;
  onDisable: () => void;
  onTest: () => void;
  onRun: () => void;
  onKill: () => void;
  onReload: () => void;
  onOpenTask: (taskId: string) => void;
  onAck: (dispatchKey: string) => void;
}

export function Detail({ labelSuffix = "", view: v, detail, runs, nowMs, busy, onEdit, onEnable, onDisable, onTest, onRun, onKill, onReload, onOpenTask, onAck }: DetailProps) {
  const meta = stateMeta(v.state);
  const blocked = approvalBlocked(v);
  const kinds = runKinds(v);
  const repo = v.sourceKind === "repository";
  const live = v.state === "ENABLED" || v.state === "PAUSED";
  return (
    <section className="autos-detail" aria-label={`Automation ${v.name}${labelSuffix}`} data-testid="automation-detail" data-automation-id={v.automationId} data-state={v.state} data-version={v.currentVersion}>
      <div className="autos-detail-head">
        <h3 className="autos-h3">{v.name}</h3>
        <span data-testid="detail-state" data-state={v.state}>
          <StatusDot status={meta.tone} label={meta.label} showLabel />
        </span>
        <Badge tone="neutral">version {v.currentVersion}</Badge>
        {repo && <Badge tone="info">from the repository</Badge>}
      </div>
      <p className="meta">{meta.words}</p>
      {v.disabledReason && (
        <p className="meta" data-testid="detail-disabled-reason" data-reason={v.disabledReason}>
          Why: {v.disabledReason}
          {v.disabledDetail ? ` — ${v.disabledDetail}` : ""}
        </p>
      )}
      {repo && (
        <p className="autos-note" role="note" data-testid="repository-note" data-enabled={live}>
          This definition is read from <code>{v.sourcePath}</code> and is shown as data. {live ? "It is enabled for the exact bytes you approved." : "It is NOT enabled: reading a file enables nothing. It runs only after you approve this exact version."} A change to the file makes a new version that needs a new approval.
        </p>
      )}
      {v.contentChanged && (
        <p className="autos-note" role="alert" data-testid="content-changed">
          <StatusDot status="warn" label="Changed" showLabel /> The file on disk no longer has the bytes that were approved. The definition will not run until you read it again and approve the new version.
        </p>
      )}
      <dl className="autos-facts" data-testid="detail-facts">
        <dt>Definition hash</dt>
        <dd>
          <code className="autos-hash" data-testid="detail-hash">
            {v.definitionHash}
          </code>
        </dd>
        <dt>Runs as</dt>
        <dd>{v.principal}</dd>
        <dt>Workspace</dt>
        <dd>
          <code>{v.workspaceRoot}</code>
        </dd>
        <dt>Reach</dt>
        <dd data-testid="detail-effects" data-effects={v.effects}>
          {v.effects}
          {v.needsListedApproval ? ` · capabilities: ${v.capabilities.join(", ") || "none"}; paths: ${v.paths.join(", ") || "none"}; hosts: ${v.hosts.join(", ") || "none"}` : " · reads only"}
        </dd>
        <dt>Approved</dt>
        <dd data-testid="detail-approved">
          {v.enabled ? (
            <>
              version {v.enabled.version}, hash <code>{v.enabled.definitionHash.slice(0, 12)}…</code>, by {v.enabled.approver || "the owner"}, {utcMinute(v.enabled.approvedMs)}
              {v.enabled.repoRevision ? `, repository revision ${v.enabled.repoRevision.slice(0, 12)}` : ""}
            </>
          ) : (
            "no version is approved"
          )}
        </dd>
        <dt>Activity</dt>
        <dd>
          {v.active} running, {v.queued} queued, {v.consecutiveFailures} consecutive {v.consecutiveFailures === 1 ? "failure" : "failures"}
        </dd>
      </dl>
      <div className="autos-actions" role="group" aria-label={`Actions for ${v.name}`}>
        <Button onClick={onEdit} data-testid="detail-edit">
          {repo ? "View definition" : "Edit"}
        </Button>
        {!live && (
          <Button variant="primary" onClick={onEnable} disabled={busy || blocked !== null} title={blocked ?? undefined} data-testid="detail-enable">
            Review and approve…
          </Button>
        )}
        {live && (
          <Button onClick={onDisable} disabled={busy} data-testid="detail-disable">
            Disable
          </Button>
        )}
        <Button onClick={onTest} disabled={busy || (repo && !live)} title={repo && !live ? "A repository definition is data until you approve it" : undefined} data-testid="detail-test">
          Test run…
        </Button>
        {kinds.manual && (
          <Button onClick={onRun} disabled={busy || !live} title={!live ? "Approve this version before it can run" : undefined} data-testid="detail-run">
            Run now
          </Button>
        )}
        {repo && (
          <Button onClick={onReload} disabled={busy} data-testid="detail-reload">
            Reload from repository
          </Button>
        )}
        <Button variant="danger" onClick={onKill} data-testid="detail-kill">
          Kill…
        </Button>
      </div>
      {blocked && !live && (
        <p className="meta" data-testid="detail-enable-blocked">
          {blocked}
        </p>
      )}

      {detail && detail.versions.length > 0 && (
        <details className="autos-versions" data-testid="detail-versions">
          <summary>Versions ({detail.versions.length})</summary>
          <ul className="autos-plain">
            {detail.versions.map((ver) => (
              <li key={ver.version} data-testid="version-row" data-version={ver.version} data-approved={ver.approved}>
                version {ver.version} · <code>{ver.definitionHash.slice(0, 12)}…</code> · {ver.sourceKind === "repository" ? `file ${ver.sourcePath}` : "saved here"} · {utcMinute(ver.createdMs)} · {ver.approved ? "the approved version" : "not approved"}
              </li>
            ))}
          </ul>
        </details>
      )}

      <h3 className="autos-h3">Run history</h3>
      <History runs={runs} nowMs={nowMs} onOpenTask={onOpenTask} onAck={onAck} />
    </section>
  );
}

// ----------------------------------------------------------------- history

export function History({ runs, nowMs, onOpenTask, onAck }: { runs: readonly AutoRun[]; nowMs: number; onOpenTask: (taskId: string) => void; onAck?: ((dispatchKey: string) => void) | undefined }) {
  if (runs.length === 0) {
    return (
      <p className="meta" data-testid="history-empty">
        No run has been recorded for this definition yet.
      </p>
    );
  }
  return (
    <div className="autos-table-wrap">
      <table className="autos-table" data-testid="automation-runs" data-count={runs.length}>
        <caption className="mb-sr-only">Run history</caption>
        <thead>
          <tr>
            <th scope="col">Fired (UTC)</th>
            <th scope="col">Status</th>
            <th scope="col">Why</th>
            <th scope="col">Trigger</th>
            <th scope="col">Event id</th>
            <th scope="col">Cost</th>
            <th scope="col">Duration</th>
            <th scope="col">Task</th>
          </tr>
        </thead>
        <tbody>
          {runs.map((r) => {
            const meta = runStatusMeta(r.status);
            return (
              <tr key={r.dispatchKey} data-testid="run-row" data-status={r.status} data-reason={r.reason} data-trigger-kind={r.triggerKind} data-test={r.test} data-catch-up={r.catchUp} data-dispatch-key={r.dispatchKey} data-task-id={r.taskId} data-findings={r.findings}>
                <td>
                  {utcMinute(r.firedMs)}
                  <div className="meta">{relativeTo(r.firedMs, nowMs)}</div>
                </td>
                <td>
                  <span data-testid="run-status" data-status={r.status}>
                    <StatusDot status={meta.tone} label={meta.label} showLabel />
                  </span>
                </td>
                <td>
                  {r.reason ? (
                    <>
                      <strong data-testid="run-reason">{r.reason}</strong>
                      <div className="meta">{reasonWords(r.reason)}</div>
                      {r.detail && <div className="meta">{r.detail}</div>}
                    </>
                  ) : (
                    <span className="meta">{r.detail}</span>
                  )}
                  {r.findings > 0 && (
                    <div className="autos-finding" data-testid="run-findings" data-count={r.findings}>
                      {r.findings} instruction-shaped {r.findings === 1 ? "passage" : "passages"} found in the trigger payload (kept as data, never obeyed)
                    </div>
                  )}
                </td>
                <td data-testid="run-trigger">
                  {runSummary(r)}
                  {r.catchUp && (
                    <div className="meta" data-testid="run-catch-up">
                      one run for the whole missed window
                    </div>
                  )}
                </td>
                <td>
                  <code className="autos-event">{r.eventId}</code>
                </td>
                <td data-testid="run-cost">{costWords(r.costMinor)}</td>
                <td>{durationWords(r) || <span className="meta">—</span>}</td>
                <td>
                  {r.taskId ? (
                    <Button size="sm" onClick={() => onOpenTask(r.taskId)} aria-label={`Open the task of the run fired ${utcMinute(r.firedMs)}`} data-testid="run-open-task">
                      Open task
                    </Button>
                  ) : (
                    <span className="meta">no task</span>
                  )}
                  {onAck && !r.acknowledged && (r.status === "failed" || r.reason === "APPROVAL_EXPIRED" || r.reason === "BUDGET") && (
                    <Button size="sm" onClick={() => onAck(r.dispatchKey)} data-testid="run-ack">
                      Acknowledge
                    </Button>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
