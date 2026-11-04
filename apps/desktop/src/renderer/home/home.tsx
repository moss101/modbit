/**
 * The home surface (the centre region with no task open): one composer in the
 * middle of the window. Send creates the task, starts it and opens its
 * conversation; Alt+Enter creates it without starting it. Setup that is still
 * missing sits above the composer. The renderer holds no authority: creating,
 * trusting and starting are the typed preload calls of the controller.
 */
import { Icon } from "@modbit/ui";
import { StateLine } from "../shell/state-line.tsx";
import type { AppState } from "../state/use-app.ts";
import { Welcome } from "./welcome.tsx";

export function Home({ app }: { app: AppState }) {
  const { core, goal, setGoal, issueUrl, setIssueUrl, workspaceRoot, setWorkspaceRoot, isolate, setIsolate, submitting, provider, submit, composer, onboarding } = app;
  const connected = core.state === "connected";
  const empty = !goal.trim() && !issueUrl.trim();
  return (
    <div className="home" data-testid="home">
      <div className="home-col">
        {onboarding && provider !== null && !provider.configured && <Welcome app={app} />}
        <form className="home-form" aria-label="New Task" onSubmit={(e) => void submit(e, true)}>
          <div className="cmp-box home-box">
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
                if (e.key !== "Enter" || e.shiftKey || e.nativeEvent.isComposing) return;
                e.preventDefault();
                if (!empty && connected) void submit(e, !e.altKey);
              }}
              placeholder="What should the agent do?"
              disabled={!connected}
            />
            <div className="cmp-bar">
              <span className="home-repo">
                <Icon name="folder" size={14} />
                <label htmlFor="workspace" className="mb-sr-only">
                  Repository (a local Git checkout; empty for a Work space)
                </label>
                <input id="workspace" data-testid="workspace" value={workspaceRoot} onChange={(e) => setWorkspaceRoot(e.target.value)} placeholder="Repository path" disabled={!connected} spellCheck={false} />
              </span>
              <span className="cmp-spacer" />
              <button type="submit" className="cmp-send" data-testid="run" aria-label={submitting ? "Creating the task" : "Run"} disabled={!connected || submitting || empty}>
                <Icon name="arrow-up" size={14} />
              </button>
            </div>
          </div>
          <div className="cmp-foot">
            <span className="cmp-foot-item" data-testid="home-location">
              <Icon name="laptop" size={14} /> This Mac
            </span>
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
            <p className="meta home-note" role="status" data-testid="composer-no-provider">
              No provider is set up: a task can be created but will not start until one is.
            </p>
          )}
          {!connected && <p className="meta home-note">Task creation is disabled until the Core is connected.</p>}
          <StateLine state={composer} testid="composer-state" />
        </form>
        {onboarding && provider !== null && provider.configured && <Welcome app={app} />}
      </div>
    </div>
  );
}
