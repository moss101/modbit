/**
 * Home (DR-PX-2026-11-11-021 MWS-12, REQ-PX-205; the rules of DR-PX-2026-11-04-020 AWS-01 and AWS-10 items 3 and 6 stay): the
 * product's name, then the composer in a tray. The tray holds the text and a bar with add context, the mode, the model and send;
 * attached under it is where the task runs: the location, the folder, its own worktree and the run mode. Under the tray, what is
 * waiting on the person, then the first-run steps while one is incomplete.
 *
 * Send creates the task, starts it and opens its conversation; Alt+Enter creates it without starting it. The mode, the model and
 * the run mode are chosen here and reach the Core after CreateTask and before StartTask, so the first model request is already on
 * them; a choice the Core refuses leaves the task created, unstarted and saying why. Nothing carries over to the next task. The
 * renderer holds no authority: creating, trusting, choosing and starting are the typed preload calls of the controller.
 */
import { useRef, useState } from "react";
import { Icon, Menu, type MenuItem } from "@modbit/ui";
import { MODE_ICON } from "../composer/composer.tsx";
import { MODE_CYCLE, MODE_DEFS, draftPosture, nextMode } from "../composer/model.ts";
import { ModelPicker, type PickerChoice } from "../composer/model-picker.tsx";
import { useCatalog } from "../composer/use-composer-core.ts";
import { RunModeWarning } from "../approvals/run-mode.tsx";
import { DEFAULT_RUN_MODE, MODE_COPY, RUN_MODES, approvesMore, modeLabel } from "../approvals/run-mode-model.ts";
import { useDockApprovals } from "../approvals/use-dock-approvals.ts";
import { CLOUD_UNAVAILABLE, localMachine } from "../location.ts";
import { StateLine } from "../shell/state-line.tsx";
import type { AppState } from "../state/use-app.ts";
import { Waiting } from "./waiting.tsx";
import { Welcome } from "./welcome.tsx";

export function Home({ app }: { app: AppState }) {
  const { core, goal, setGoal, issueUrl, setIssueUrl, workspaceRoot, setWorkspaceRoot, isolate, setIsolate, newMode, setNewMode, newModel, setNewModel, newRunMode, setNewRunMode, submitting, provider, submit, composer, onboarding, model, attentionItems, requestNav } = app;
  const connected = core.state === "connected";
  const empty = !goal.trim() && !issueUrl.trim();
  const [pickerWanted, setPickerWanted] = useState(false);
  const [modelNote, setModelNote] = useState("");
  const [issueWanted, setIssueWanted] = useState(false);
  const [pendingRunMode, setPendingRunMode] = useState<string | null>(null);
  const goalRef = useRef<HTMLTextAreaElement>(null);
  const issueRef = useRef<HTMLInputElement>(null);
  // A key set up on this screen changes which rows have a credential.
  const catalog = useCatalog("", pickerWanted && connected, String(provider?.configured));
  // What waits on the person across the session's tasks, as the Core lists it (no task is open on home).
  const waiting = useDockApprovals(model.sessionId, null, connected);
  const machine = localMachine();
  const issueShown = issueWanted || issueUrl !== "";
  const folder = workspaceRoot.trim();
  const runMode = newRunMode?.mode ?? DEFAULT_RUN_MODE;

  const modeItems: MenuItem[] = MODE_CYCLE.map((m) => ({ id: m.toLowerCase(), label: MODE_DEFS[m].label, radio: true, checked: newMode === m, onSelect: () => setNewMode(m) }));
  // Local is the only location this app can run a task in; it is chosen by being the only one that can be, and never inherited
  // (AFW-B09). Cloud stays disabled with its reason until PX-128 builds the desktop's cloud client.
  const locationItems: MenuItem[] = [
    { id: "location-local", label: machine, radio: true, checked: true, onSelect: () => {} },
    { id: "location-cloud", label: `Isolated cloud: ${CLOUD_UNAVAILABLE}`, radio: true, checked: false, disabled: true, onSelect: () => {} },
  ];
  // An issue URL is entered from add context (MWS-12): the Core reads the issue and attaches its text as untrusted context.
  const addItems: MenuItem[] = [
    {
      id: "home-add-issue",
      label: issueShown ? "GitHub issue (added below the text)" : "From a GitHub issue",
      onSelect: () => {
        setIssueWanted(true);
        requestAnimationFrame(() => issueRef.current?.focus());
      },
    },
  ];
  // Who approves the new task's effects. A mode that approves more than the default asks for the acknowledgement first, in the
  // same dialog a task's run-mode panel uses; the Core refuses such a mode without it.
  const runModeItems: MenuItem[] = RUN_MODES.map((m) => ({
    id: `home-runmode-${m.toLowerCase()}`,
    label: modeLabel(m),
    radio: true,
    checked: runMode === m,
    onSelect: () => {
      if (m === runMode) return;
      if (approvesMore(DEFAULT_RUN_MODE, m, RUN_MODES)) setPendingRunMode(m);
      else setNewRunMode(m === DEFAULT_RUN_MODE ? null : { mode: m, acknowledged: false });
    },
  }));
  const chooseModel = (c: PickerChoice) => {
    // The catalog is the Core's: a row it says cannot be pinned is not held for a task that would then refuse to start.
    if (c.row?.blocked) {
      setModelNote(`${c.row.name} is not available: ${c.row.blocked.reason}`);
      return;
    }
    setModelNote("");
    if (c.kind === "objective" && c.objective) setNewModel({ objective: c.objective });
    else if (c.row) setNewModel({ row: c.row });
  };
  const titleOf = (id: string) => model.tasks.get(id)?.goalText ?? "A task";
  const sendWhy = !connected ? "The Core is not connected" : empty ? "Describe a task first" : undefined;
  return (
    <div className="home" data-testid="home">
      <div className="home-col">
        <div className="home-head">
          <h1 className="home-brand" data-testid="home-brand">
            Modbit
          </h1>
        </div>
        <form className="home-form" aria-label="New Task" onSubmit={(e) => void submit(e, true)}>
          <div className="home-tray" data-testid="home-tray">
            <div className="cmp-box home-box" data-testid="home-composer" data-mode={newMode} data-tint={newMode === "AGENT" ? undefined : newMode.toLowerCase()}>
              <label htmlFor="goal" className="mb-sr-only">
                Describe the task
              </label>
              <textarea
                ref={goalRef}
                id="goal"
                data-testid="goal"
                className="cmp-textarea home-goal"
                rows={3}
                value={goal}
                onChange={(e) => setGoal(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Tab" && e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey) {
                    e.preventDefault();
                    setNewMode(nextMode(newMode));
                    return;
                  }
                  if (e.key !== "Enter" || e.shiftKey || e.nativeEvent.isComposing) return;
                  e.preventDefault();
                  if (!empty && connected) void submit(e, !e.altKey);
                }}
                placeholder="Describe a task. Enter sends it, Alt+Enter keeps it to start later."
                disabled={!connected}
              />
              {issueShown && (
                <div className="home-issue-row" data-testid="home-issue">
                  <Icon name="file-text" size={14} />
                  <label htmlFor="issue-url" className="mb-sr-only">
                    From a GitHub issue (its text enters as untrusted context)
                  </label>
                  <input ref={issueRef} id="issue-url" className="home-issue" data-testid="issue-url" value={issueUrl} onChange={(e) => setIssueUrl(e.target.value)} placeholder="GitHub issue URL" title="Its text enters as untrusted context" disabled={!connected} spellCheck={false} />
                  <button
                    type="button"
                    className="home-issue-remove"
                    data-testid="home-issue-remove"
                    aria-label="Remove the issue"
                    onClick={() => {
                      setIssueUrl("");
                      setIssueWanted(false);
                      goalRef.current?.focus();
                    }}
                  >
                    <Icon name="x" size={12} />
                  </button>
                </div>
              )}
              <div className="cmp-bar home-bar" data-testid="home-bar">
                <Menu label="Add context" placement="down" triggerTestId="home-add-context" triggerClassName="cmp-plus" trigger={<Icon name="plus" size={16} />} items={addItems} />
                <span className="cmp-mode" data-testid="home-mode" data-mode={newMode}>
                  <Menu label={`Mode: ${MODE_DEFS[newMode].label}. Choose another`} placement="down" triggerTestId="home-mode-button" triggerClassName="cmp-chip cmp-mode-btn" trigger={<><Icon name={MODE_ICON[newMode]} size={14} /> <span>{MODE_DEFS[newMode].label}</span><Icon name="chevron-down" size={10} /></>} items={modeItems} />
                </span>
                <span className="cmp-spacer" />
                <ModelPicker posture={draftPosture(newMode, newModel)} catalog={catalog.catalog} catalogError={catalog.error} disabled={!connected} onOpen={() => setPickerWanted(true)} onChoose={chooseModel} />
                <button type="submit" className="cmp-send" data-testid="run" aria-label={submitting ? "Creating the task" : "Send: create and start the task"} title={sendWhy} disabled={!connected || submitting || empty}>
                  <Icon name="arrow-up" size={14} />
                </button>
              </div>
              {modelNote && (
                <p className="cmp-reason" role="alert" data-testid="home-model-note">
                  {modelNote}
                </p>
              )}
            </div>
            <div className="home-env" data-testid="home-env" role="group" aria-label="Where the task runs">
              <Menu label={`Where the task runs: ${machine}. Choose another`} placement="down" triggerTestId="home-location" triggerClassName="cmp-chip home-chip" trigger={<><Icon name="laptop" size={14} /> <span>{machine}</span><Icon name="chevron-down" size={10} /></>} items={locationItems} />
              <span className="home-chip home-folder" data-testid="home-folder">
                <Icon name="folder" size={14} />
                <label htmlFor="workspace" className="mb-sr-only">
                  Folder (a local Git checkout; empty for a task without one)
                </label>
                <input id="workspace" data-testid="workspace" value={workspaceRoot} onChange={(e) => setWorkspaceRoot(e.target.value)} placeholder="Folder" disabled={!connected} spellCheck={false} title={folder || "A local Git checkout the task works in"} />
              </span>
              <label className="home-chip home-switch" htmlFor="isolate" title={folder ? "Your checkout stays untouched until you apply the result" : "Choose a folder first: a worktree is made from a checkout"}>
                <input id="isolate" type="checkbox" role="switch" data-testid="isolate" checked={isolate} onChange={(e) => setIsolate(e.target.checked)} disabled={!connected || folder === ""} />
                <span className="home-switch-track" aria-hidden="true" />
                Own worktree
              </label>
              <span className="cmp-spacer" />
              <Menu label={`Run mode: ${modeLabel(runMode)}. ${MODE_COPY[runMode]?.summary ?? ""} Choose another`} placement="down" align="end" triggerTestId="home-runmode" triggerClassName="cmp-chip home-chip" trigger={<><Icon name="shield-check" size={14} /> <span data-testid="home-runmode-label">{modeLabel(runMode)}</span><Icon name="chevron-down" size={10} /></>} items={runModeItems} />
            </div>
          </div>
          {provider !== null && !provider.configured && (
            <p className="mb-sr-only" role="status" data-testid="composer-no-provider">
              No provider is set up: a task can be created but will not start until one is.
            </p>
          )}
          {!connected && <p className="meta home-note">Task creation is disabled until the Core is connected.</p>}
          {/* Without a provider the first-run steps already say it; the state line stays for the accessibility tree. */}
          <div className={provider !== null && !provider.configured ? "mb-sr-only" : undefined}>
            <StateLine state={composer} testid="composer-state" />
          </div>
        </form>
        <Waiting sessionId={model.sessionId} approvals={waiting.approvals} attention={attentionItems} titleOf={titleOf} onOpenTask={(id) => requestNav({ kind: "task", taskId: id })} onDecided={waiting.refresh} />
        {onboarding && <Welcome app={app} onWriteTask={() => goalRef.current?.focus()} />}
      </div>
      <RunModeWarning
        pending={pendingRunMode}
        warning=""
        error={null}
        busy={false}
        onCancel={() => setPendingRunMode(null)}
        onConfirm={(m) => {
          setNewRunMode({ mode: m, acknowledged: true });
          setPendingRunMode(null);
        }}
      />
    </div>
  );
}
