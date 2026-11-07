/**
 * The home surface (the centre region with no task open): one composer in the
 * middle of the window. The mode and the model are chosen here and reach the Core
 * before the task starts. Send creates the task, starts it and opens its
 * conversation; Alt+Enter creates it without starting it. Setup that is still
 * missing sits above the composer. The renderer holds no authority: creating,
 * trusting and starting are the typed preload calls of the controller, and a mode
 * or a model the Core refuses leaves the task unstarted.
 */
import { useState } from "react";
import { Icon, Menu, type MenuItem } from "@modbit/ui";
import { MODE_ICON } from "../composer/composer.tsx";
import { MODE_CYCLE, MODE_DEFS, draftPosture, nextMode } from "../composer/model.ts";
import { ModelPicker, type PickerChoice } from "../composer/model-picker.tsx";
import { useCatalog } from "../composer/use-composer-core.ts";
import { CLOUD_UNAVAILABLE, localMachine } from "../location.ts";
import { StateLine } from "../shell/state-line.tsx";
import type { AppState } from "../state/use-app.ts";
import { Welcome } from "./welcome.tsx";

export function Home({ app }: { app: AppState }) {
  const { core, goal, setGoal, issueUrl, setIssueUrl, workspaceRoot, setWorkspaceRoot, isolate, setIsolate, newMode, setNewMode, newModel, setNewModel, submitting, provider, submit, composer, onboarding } = app;
  const connected = core.state === "connected";
  const empty = !goal.trim() && !issueUrl.trim();
  const [pickerWanted, setPickerWanted] = useState(false);
  const [modelNote, setModelNote] = useState("");
  // A key set up on this screen changes which rows have a credential.
  const catalog = useCatalog("", pickerWanted && connected, String(provider?.configured));
  const machine = localMachine();
  const modeItems: MenuItem[] = MODE_CYCLE.map((m) => ({ id: m.toLowerCase(), label: MODE_DEFS[m].label, radio: true, checked: newMode === m, onSelect: () => setNewMode(m) }));
  // Local is the only location this app can run a task in; it is chosen by being the only one that can be, and never inherited (AFW-B09).
  const locationItems: MenuItem[] = [
    { id: "location-local", label: machine, radio: true, checked: true, onSelect: () => {} },
    { id: "location-cloud", label: `Isolated cloud: ${CLOUD_UNAVAILABLE}`, radio: true, checked: false, disabled: true, onSelect: () => {} },
  ];
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
  return (
    <div className="home" data-testid="home">
      <div className="home-col">
        {onboarding && provider !== null && !provider.configured && <Welcome app={app} />}
        <form className="home-form" aria-label="New Task" onSubmit={(e) => void submit(e, true)}>
          <div className="cmp-box home-box" data-mode={newMode} data-tint={newMode === "AGENT" ? undefined : newMode.toLowerCase()}>
            <label htmlFor="goal" className="mb-sr-only">
              Goal
            </label>
            <textarea
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
              placeholder="What should the agent do?"
              disabled={!connected}
            />
            <div className="cmp-bar">
              <span className="cmp-mode" data-testid="home-mode" data-mode={newMode}>
                <Menu label={`Mode: ${MODE_DEFS[newMode].label}. Choose another`} placement="down" triggerTestId="home-mode-button" triggerClassName="cmp-chip cmp-mode-btn" trigger={<><Icon name={MODE_ICON[newMode]} size={14} /> <span>{MODE_DEFS[newMode].label}</span><Icon name="chevron-down" size={10} /></>} items={modeItems} />
              </span>
              <ModelPicker posture={draftPosture(newMode, newModel)} catalog={catalog.catalog} catalogError={catalog.error} disabled={!connected} onOpen={() => setPickerWanted(true)} onChoose={chooseModel} />
              <span className="home-repo">
                <Icon name="folder" size={14} />
                <label htmlFor="workspace" className="mb-sr-only">
                  Repository (a local Git checkout; empty for a Work space)
                </label>
                <input id="workspace" data-testid="workspace" value={workspaceRoot} onChange={(e) => setWorkspaceRoot(e.target.value)} placeholder="Repository path" disabled={!connected} spellCheck={false} />
              </span>
              <button type="submit" className="cmp-send" data-testid="run" aria-label={submitting ? "Creating the task" : "Run"} disabled={!connected || submitting || empty}>
                <Icon name="arrow-up" size={14} />
              </button>
            </div>
            {modelNote && (
              <p className="cmp-reason" role="alert" data-testid="home-model-note">
                {modelNote}
              </p>
            )}
          </div>
          <div className="cmp-foot">
            <Menu label={`Where the task runs: ${machine}. Choose another`} placement="down" triggerTestId="home-location" triggerClassName="cmp-chip" trigger={<><Icon name="laptop" size={14} /> <span>{machine}</span><Icon name="chevron-down" size={10} /></>} items={locationItems} />
            <label className="cmp-foot-item" htmlFor="isolate" title="Your checkout stays untouched until you apply the result">
              <input id="isolate" type="checkbox" data-testid="isolate" checked={isolate} onChange={(e) => setIsolate(e.target.checked)} disabled={!connected || workspaceRoot.trim() === ""} />
              Own worktree
            </label>
            <span className="cmp-spacer" />
            <label htmlFor="issue-url" className="mb-sr-only">
              From a GitHub issue (its text enters as untrusted context)
            </label>
            <input id="issue-url" className="home-issue" data-testid="issue-url" value={issueUrl} onChange={(e) => setIssueUrl(e.target.value)} placeholder="Or a GitHub issue URL" title="Its text enters as untrusted context" disabled={!connected} spellCheck={false} />
          </div>
          {provider !== null && !provider.configured && (
            <p className="mb-sr-only" role="status" data-testid="composer-no-provider">
              No provider is set up: a task can be created but will not start until one is.
            </p>
          )}
          {!connected && <p className="meta home-note">Task creation is disabled until the Core is connected.</p>}
          {/* Without a provider the setup card above already says it; the state line stays for the accessibility tree. */}
          <div className={provider !== null && !provider.configured ? "mb-sr-only" : undefined}>
            <StateLine state={composer} testid="composer-state" />
          </div>
        </form>
        {onboarding && provider !== null && provider.configured && <Welcome app={app} />}
      </div>
    </div>
  );
}
