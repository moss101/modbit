/**
 * The Automations surface (REQ-PX-086; docs/68 AUT-A04, AUT-D03, AUT-D05,
 * AUT-E01, AUT-E02), in the page shape of REQ-PX-207 (DR-PX-2026-11-11-021
 * MWS-14): a 45 px head with the page's name, its one primary action (New
 * automation) and the global pause and kill; the list of definitions with
 * their states, triggers and last runs, each with its pause; and beside it a
 * details rail for the chosen one (trigger, where it runs, limits, its
 * definition, recent runs, Run now). The attention items and what clears
 * each, the definition editor with the Core's live validation, the enable
 * approval, the test run and the kill switches are the ones the surface had. The renderer holds no authority: every effect is a typed
 * preload call into the main process, which validates it and asks the Core;
 * every word about validity, state, hash and reach is the Core's.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { Button, Icon, IconButton, StatusDot } from "@modbit/ui";
import type { AutoAttention, AutoDetail, AutoRun, AutoView } from "../../shared/automation-types.ts";
import { bridgeCalls, newCommandId, type AutomationCalls } from "./calls.ts";
import { EnableDialog, KillDialog, TestRunDialog } from "./dialogs.tsx";
import { DefinitionEditor, type EditorTarget } from "./editor.tsx";
import { ATTENTION_META, approvalBlocked, attentionAction, costWords, durationWords, effectWords, nextDueWords, reasonWords, refusalSentence, relativeTo, runKinds, runStatusMeta, runSummary, STARTER_DEFINITION, stateMeta, utcMinute } from "./model.ts";
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
  initial?: AutomationsInitial | undefined;
}

export function Automations({ calls = bridgeCalls, connected, onOpenTask, initial }: AutomationsProps) {
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
    <section className="pg autos" aria-label={`Automations${suffix}`} data-testid="automations" data-loaded={data.loaded} data-rail={selected ? "open" : "closed"}>
      <header className="pg-head">
        <h2 className="pg-title">Automations</h2>
        <span className="pg-grow" />
        <Button size="sm" variant="ghost" aria-pressed={globalPaused} onClick={() => void act(async () => void (await calls.pause("", !globalPaused)))} disabled={busy || !list} data-testid="global-pause" data-paused={globalPaused}>
          {globalPaused ? "Resume all" : "Pause all"}
        </Button>
        <Button size="sm" variant="ghost" onClick={() => setKillTarget({ automationId: "", name: "all automations" })} disabled={!list} data-testid="kill-all">
          Kill all…
        </Button>
        <Button size="sm" variant="primary" onClick={() => openEditor(null)} disabled={!reachable} data-testid="automation-new">
          <Icon name="plus" aria-hidden="true" />
          New automation
        </Button>
      </header>
      <div className="pg-split">
        <div className="pg-body autos-body">
          {globalPaused && (
            <p className="autos-note" role="status" data-testid="global-paused-banner">
              <StatusDot status="warn" label="Paused" showLabel /> Every automation is paused: no trigger starts a run, and a slot that comes due is recorded as skipped.
            </p>
          )}
          <p className="pg-intro">
            An automation starts an ordinary task when a trigger fires: a schedule, a repository event, a signed webhook or a run by hand. It runs as you, within limits you approve, and anything protected still asks. A model cannot create or enable one.
            {list ? <span className="pg-dim"> The Core's clock reads {utcMinute(list.nowMs)}{list.clock === "CONTROLLED" ? " (a controlled clock)" : ""}.</span> : null}
          </p>
          <div className="autos-bar">
            <label className="autos-field autos-field-wide">
              <span>Folder whose .modbit/automations files to read</span>
              <input value={workspace} onChange={(e) => setWorkspace(e.target.value)} placeholder="/path/to/a/trusted/repository" spellCheck={false} data-testid="automations-workspace" />
            </label>
            <Button size="sm" onClick={() => void loadRepository(workspace)} disabled={!reachable || busy} data-testid="repository-load">
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
              <h3 className="pg-h">Needs attention</h3>
              <ul className="pg-list">
                {list.attention.map((a, i) => {
                  const meta = ATTENTION_META[a.kind] ?? { label: a.kind, tone: "warn" as const };
                  const action = attentionAction(a);
                  return (
                    <li key={`${a.kind}:${a.automationId}:${a.dispatchKey}:${i}`} className="pg-row" data-testid="automation-attention" data-kind={a.kind} data-automation-id={a.automationId}>
                      <div className="pg-main">
                        <span className="pg-title-line">
                          <StatusDot status={meta.tone} label={meta.label} showLabel /> <strong>{a.name}</strong>
                        </span>
                        <span className="pg-sub">{a.reason}</span>
                      </div>
                      <div className="pg-actions">
                        <Button size="sm" onClick={() => onAttention(a)} disabled={busy} data-testid="attention-action" data-action={action.kind}>
                          {action.label}
                        </Button>
                      </div>
                    </li>
                  );
                })}
              </ul>
            </section>
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

          {list && views.length === 0 && (
            <div className="autos-empty" data-testid="automations-empty">
              <p className="pg-title-line">
                <StatusDot status="idle" label="Empty" showLabel /> <strong>No automations yet</strong>
              </p>
              <p className="meta">Write a definition with New automation, or read the ones a repository carries in .modbit/automations. A definition from a repository is data until you approve it.</p>
            </div>
          )}

          {views.length > 0 && (
            <ul className="pg-list autos-defs" aria-label={`Automations${suffix}`} data-testid="automation-list" data-count={views.length}>
              {views.map((v) => {
                const meta = stateMeta(v.state);
                const chosen = v.automationId === selectedId;
                return (
                  <li key={v.automationId} className="pg-row autos-row" data-testid="automation-row" data-automation-id={v.automationId} data-name={v.name} data-state={v.state} data-paused={v.paused} data-selected={chosen}>
                    <button type="button" className="autos-select" aria-current={chosen ? "true" : undefined} aria-label={`${v.name}: show its details`} onClick={() => setSelectedId(v.automationId)} data-testid="automation-select">
                      <span className="pg-ico">
                        <Icon name={triggerIcon(v)} aria-hidden="true" />
                      </span>
                      <span className="pg-main">
                        <span className="pg-title-line">{v.name}</span>
                        <span className="pg-sub">
                          {reachWords(v.effects)} ·{" "}
                          <span data-testid="automation-source" data-source={v.sourceKind}>
                            {v.sourceKind === "repository" ? <code>{v.sourcePath}</code> : "saved here"}
                          </span>
                        </span>
                      </span>
                    </button>
                    <span className="autos-cell autos-cell-state">
                      <span data-testid="automation-state" data-state={v.state}>
                        <StatusDot status={meta.tone} label={meta.label} showLabel />
                      </span>
                      {v.paused && v.state !== "PAUSED" && <span className="pg-sub">pause set</span>}
                      {v.contentChanged && (
                        <span className="pg-sub pg-warn" data-testid="row-content-changed">
                          file changed
                        </span>
                      )}
                    </span>
                    <ul className="autos-cell autos-cell-trig" data-testid="automation-triggers" aria-label={`Triggers of ${v.name}`}>
                      {v.triggers.map((t) => (
                        <li key={t.id} data-kind={t.kind} data-next-due={t.nextDueMs} title={t.kind === "schedule" ? nextDueWords(t, nowMs) : undefined}>
                          {t.summary}
                        </li>
                      ))}
                    </ul>
                    <span className="autos-cell autos-cell-last" data-testid="automation-last-run" data-status={v.lastRunStatus}>
                      {v.lastRunMs ? (
                        <>
                          {runStatusMeta(v.lastRunStatus).label} <span className="pg-dim">{relativeTo(v.lastRunMs, nowMs)}</span>
                        </>
                      ) : (
                        <span className="pg-dim">Never run</span>
                      )}
                    </span>
                    <Button size="sm" aria-pressed={v.paused} aria-label={`${v.paused ? "Resume" : "Pause"} ${v.name}`} disabled={busy} onClick={() => void act(async () => void (await calls.pause(v.automationId, !v.paused)))} data-testid="row-pause" data-paused={v.paused}>
                      {v.paused ? "Resume" : "Pause"}
                    </Button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>

        {selected && (
          <Detail
            labelSuffix={suffix}
            view={selected}
            detail={detail}
            runs={selectedRuns}
            nowMs={nowMs}
            busy={busy}
            onClose={() => setSelectedId(null)}
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
      </div>

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

/** The icon of a definition's row: a clock for a schedule, a branch for a repository event, a play mark otherwise. */
function triggerIcon(v: AutoView): "clock" | "git-branch" | "globe" | "terminal" {
  const kinds = new Set(v.triggers.map((t) => t.kind));
  if (kinds.has("schedule")) return "clock";
  if (kinds.has("event")) return "git-branch";
  if (kinds.has("webhook")) return "globe";
  return "terminal";
}

/** A definition's reach in a few words, for its row. */
function reachWords(effects: string): string {
  if (effects === "read_only") return "Reads only";
  if (effects === "reversible_write") return "Writes in its own worktree";
  if (effects === "protected_write") return "May ask to write to protected paths";
  if (effects === "external_side_effect") return "May ask for effects beyond this machine";
  return effects.replace(/_/g, " ");
}

// ------------------------------------------------------------------ the details rail

interface DetailProps {
  labelSuffix?: string;
  view: AutoView;
  detail: AutoDetail | null;
  runs: AutoRun[];
  nowMs: number;
  busy: boolean;
  onClose: () => void;
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

/**
 * The chosen automation in a rail beside the list (MWS-14, in the shape of the task's details rail, MWS-09): its name and state,
 * the actions the Core offers on it, then what fires it, where it runs, its limits, its definition and its recent runs. Every
 * value is the Core's answer for this definition; each action is the call the surface made before (enable, disable, test run,
 * run now, kill).
 */
export function Detail({ labelSuffix = "", view: v, detail, runs, nowMs, busy, onClose, onEdit, onEnable, onDisable, onTest, onRun, onKill, onReload, onOpenTask, onAck }: DetailProps) {
  const meta = stateMeta(v.state);
  const blocked = approvalBlocked(v);
  const kinds = runKinds(v);
  const repo = v.sourceKind === "repository";
  const live = v.state === "ENABLED" || v.state === "PAUSED";
  const folder = v.workspaceRoot.split(/[\\/]/).filter(Boolean).pop() ?? v.workspaceRoot;
  return (
    <aside className="autos-rail" aria-label={`Automation ${v.name}${labelSuffix}`} data-testid="automation-detail" data-automation-id={v.automationId} data-state={v.state} data-version={v.currentVersion}>
      <div className="rail-head autos-rail-head">
        <h3 className="autos-rail-title">{v.name}</h3>
        <IconButton icon={<Icon name="x" />} label="Close the details" size="sm" onClick={onClose} data-testid="automation-rail-close" />
      </div>
      <p className="autos-rail-state">
        <span data-testid="detail-state" data-state={v.state}>
          <StatusDot status={meta.tone} label={meta.label} showLabel />
        </span>
        <span className="pg-dim"> · version {v.currentVersion}</span>
        {repo && <span className="pg-dim"> · from the repository</span>}
      </p>
      {v.description && <p className="autos-rail-text">{v.description}</p>}
      <p className="autos-rail-text pg-dim">{meta.words}</p>
      {v.disabledReason && (
        <p className="autos-rail-text" data-testid="detail-disabled-reason" data-reason={v.disabledReason}>
          Why: {reasonWords(v.disabledReason)}
          {v.disabledDetail ? ` (${v.disabledDetail})` : ""}
        </p>
      )}
      {repo && (
        <p className="autos-note" role="note" data-testid="repository-note" data-enabled={live}>
          Read from <code>{v.sourcePath}</code> and shown as data. {live ? "It is enabled for the exact bytes you approved." : "It is NOT enabled: reading a file enables nothing. It runs only after you approve this exact version."} A change to the file makes a new version that needs a new approval.
        </p>
      )}
      {v.contentChanged && (
        <p className="autos-note" role="alert" data-testid="content-changed">
          <StatusDot status="warn" label="Changed" showLabel /> The file on disk no longer has the bytes that were approved. The definition will not run until you read it again and approve the new version.
        </p>
      )}
      <div className="autos-actions" role="group" aria-label={`Actions for ${v.name}`}>
        {kinds.manual && (
          <Button size="sm" variant="primary" onClick={onRun} disabled={busy || !live} title={!live ? "Approve this version before it can run" : undefined} data-testid="detail-run">
            Run now
          </Button>
        )}
        {!live && (
          <Button size="sm" variant={kinds.manual ? "secondary" : "primary"} onClick={onEnable} disabled={busy || blocked !== null} title={blocked ?? undefined} data-testid="detail-enable">
            Review and approve…
          </Button>
        )}
        {live && (
          <Button size="sm" onClick={onDisable} disabled={busy} data-testid="detail-disable">
            Disable
          </Button>
        )}
        <Button size="sm" onClick={onTest} disabled={busy || (repo && !live)} title={repo && !live ? "A repository definition is data until you approve it" : undefined} data-testid="detail-test">
          Test run…
        </Button>
        <Button size="sm" onClick={onEdit} data-testid="detail-edit">
          {repo ? "View definition" : "Edit"}
        </Button>
        {repo && (
          <Button size="sm" onClick={onReload} disabled={busy} data-testid="detail-reload">
            Reload from the repository
          </Button>
        )}
        <Button size="sm" variant="danger" onClick={onKill} data-testid="detail-kill">
          Kill…
        </Button>
      </div>
      {blocked && !live && (
        <p className="autos-rail-text pg-dim" data-testid="detail-enable-blocked">
          {blocked}
        </p>
      )}

      <section className="rail-sec" aria-label="Trigger">
        <h4 className="rail-h">Trigger</h4>
        <ul className="rail-list">
          {v.triggers.map((t) => (
            <li key={t.id} className="rail-row" data-testid="detail-trigger" data-kind={t.kind}>
              <span className="rail-ico">
                <Icon name={t.kind === "schedule" ? "clock" : t.kind === "event" ? "git-branch" : t.kind === "webhook" ? "globe" : "terminal"} aria-hidden="true" />
              </span>
              <span className="rail-x" title={t.summary}>
                {t.summary}
              </span>
              {t.kind === "schedule" && (
                <span className="rail-aside" title={nextDueWords(t, nowMs)}>
                  {t.nextDueMs ? `next ${relativeTo(t.nextDueMs, nowMs)}` : "no slot due"}
                </span>
              )}
              {t.kind === "manual" && <span className="rail-aside">by hand</span>}
            </li>
          ))}
        </ul>
      </section>

      <section className="rail-sec" aria-label="Runs in">
        <h4 className="rail-h">Runs in</h4>
        <div className="rail-row">
          <span className="rail-ico">
            <Icon name="folder" aria-hidden="true" />
          </span>
          <span className="rail-x" title={v.workspaceRoot}>
            {folder}
          </span>
          <span className="rail-aside">its own worktree</span>
        </div>
        <div className="rail-row">
          <span className="rail-ico">
            <Icon name="eye" aria-hidden="true" />
          </span>
          <span className="rail-x">Runs as</span>
          <span className="rail-aside" data-testid="detail-principal">
            {v.principal}
          </span>
        </div>
      </section>

      <section className="rail-sec" aria-label="Limits">
        <h4 className="rail-h">Limits</h4>
        <div className="rail-row">
          <span className="rail-ico">
            <Icon name="shield-check" aria-hidden="true" />
          </span>
          <span className="rail-x">May do</span>
          <span className="rail-aside" data-testid="detail-effects" data-effects={v.effects} title={effectWords(v.effects)}>
            {reachWords(v.effects)}
          </span>
        </div>
        {v.needsListedApproval && (
          <p className="autos-rail-text pg-dim" data-testid="detail-reach">
            Tools {v.capabilities.join(", ") || "none"}; paths {v.paths.join(", ") || "none"}; hosts {v.hosts.join(", ") || "none"}.
          </p>
        )}
        {v.limitMaxCostMinor > 0 && (
          <div className="rail-row">
            <span className="rail-x">Cost limit</span>
            <span className="rail-aside">{costWords(v.limitMaxCostMinor)} a run</span>
          </div>
        )}
        {v.limitDeadlineMinutes > 0 && (
          <div className="rail-row">
            <span className="rail-x">Time limit</span>
            <span className="rail-aside">{v.limitDeadlineMinutes} min a run</span>
          </div>
        )}
        {v.limitApprovalWaitMinutes > 0 && (
          <div className="rail-row">
            <span className="rail-x">Waits for an approval</span>
            <span className="rail-aside">{v.limitApprovalWaitMinutes} min</span>
          </div>
        )}
        <p className="autos-rail-text" data-testid="detail-approved">
          {v.enabled ? (
            <>
              Approved: version {v.enabled.version}, hash <code>{v.enabled.definitionHash.slice(0, 12)}…</code>, by {v.enabled.approver || "the owner"}, {utcMinute(v.enabled.approvedMs)}
              {v.enabled.repoRevision ? `, repository revision ${v.enabled.repoRevision.slice(0, 12)}` : ""}
            </>
          ) : (
            "No version is approved."
          )}
        </p>
        <p className="autos-rail-text pg-dim">
          {v.active} running, {v.queued} queued, {v.consecutiveFailures} {v.consecutiveFailures === 1 ? "failure" : "failures"} in a row
        </p>
      </section>

      <section className="rail-sec" aria-label="Definition">
        <h4 className="rail-h">Definition</h4>
        <p className="autos-rail-text">
          Hash{" "}
          <code className="autos-hash" data-testid="detail-hash">
            {v.definitionHash}
          </code>
        </p>
        {detail && detail.versions.length > 0 && (
          <details className="autos-versions" data-testid="detail-versions">
            <summary>Versions ({detail.versions.length})</summary>
            <ul className="rail-list">
              {detail.versions.map((ver) => (
                <li key={ver.version} className="autos-rail-text" data-testid="version-row" data-version={ver.version} data-approved={ver.approved}>
                  version {ver.version} · <code>{ver.definitionHash.slice(0, 12)}…</code> · {ver.sourceKind === "repository" ? `file ${ver.sourcePath}` : "saved here"} · {utcMinute(ver.createdMs)} · {ver.approved ? "the approved version" : "not approved"}
                </li>
              ))}
            </ul>
          </details>
        )}
      </section>

      <section className="rail-sec" aria-label="Recent runs">
        <h4 className="rail-h">
          Recent runs<span className="rail-h-aside">{runs.length > 0 ? runs.length : ""}</span>
        </h4>
        <History runs={runs} nowMs={nowMs} onOpenTask={onOpenTask} onAck={onAck} />
      </section>
    </aside>
  );
}

// ----------------------------------------------------------------- history

/** A definition's runs, newest first, each with its status, its typed reason in words, what fired it, its cost and its task. */
export function History({ runs, nowMs, onOpenTask, onAck }: { runs: readonly AutoRun[]; nowMs: number; onOpenTask: (taskId: string) => void; onAck?: ((dispatchKey: string) => void) | undefined }) {
  if (runs.length === 0) {
    return (
      <p className="autos-rail-text pg-dim" data-testid="history-empty">
        No run has been recorded for this definition yet.
      </p>
    );
  }
  return (
    <ul className="rail-list autos-runs" aria-label="Run history" data-testid="automation-runs" data-count={runs.length}>
      {runs.map((r) => {
        const meta = runStatusMeta(r.status);
        const duration = durationWords(r);
        return (
          <li key={r.dispatchKey} className="autos-run" data-testid="run-row" data-status={r.status} data-reason={r.reason} data-trigger-kind={r.triggerKind} data-test={r.test} data-catch-up={r.catchUp} data-dispatch-key={r.dispatchKey} data-task-id={r.taskId} data-findings={r.findings}>
            <div className="rail-row">
              <span data-testid="run-status" data-status={r.status}>
                <StatusDot status={meta.tone} label={meta.label} showLabel />
              </span>
              <span className="rail-aside" title={utcMinute(r.firedMs)}>
                {relativeTo(r.firedMs, nowMs)}
              </span>
            </div>
            {r.reason ? (
              <p className="autos-rail-text">
                <span data-testid="run-reason" data-code={r.reason}>
                  {reasonWords(r.reason)}
                </span>
                {r.detail && <span className="pg-dim"> {r.detail}</span>}
              </p>
            ) : (
              r.detail && <p className="autos-rail-text pg-dim">{r.detail}</p>
            )}
            {r.findings > 0 && (
              <p className="autos-finding" data-testid="run-findings" data-count={r.findings}>
                {r.findings} instruction-shaped {r.findings === 1 ? "passage" : "passages"} found in the trigger payload (kept as data, never obeyed)
              </p>
            )}
            <p className="autos-rail-text pg-dim" data-testid="run-trigger">
              {utcMinute(r.firedMs)} · {runSummary(r)} · <span className="autos-event">{r.eventId}</span>
              {r.catchUp && (
                <span data-testid="run-catch-up"> · one run for the whole missed window</span>
              )}
            </p>
            <p className="autos-rail-text pg-dim">
              <span data-testid="run-cost">{costWords(r.costMinor)}</span>
              {duration ? ` · ${duration}` : ""}
            </p>
            {(r.taskId || (onAck && !r.acknowledged && (r.status === "failed" || r.reason === "APPROVAL_EXPIRED" || r.reason === "BUDGET"))) && (
              <div className="autos-actions">
                {r.taskId && (
                  <Button size="sm" onClick={() => onOpenTask(r.taskId)} aria-label={`Open the task of the run fired ${utcMinute(r.firedMs)}`} data-testid="run-open-task">
                    Open task
                  </Button>
                )}
                {onAck && !r.acknowledged && (r.status === "failed" || r.reason === "APPROVAL_EXPIRED" || r.reason === "BUDGET") && (
                  <Button size="sm" onClick={() => onAck(r.dispatchKey)} data-testid="run-ack">
                    Acknowledge
                  </Button>
                )}
              </div>
            )}
          </li>
        );
      })}
    </ul>
  );
}
