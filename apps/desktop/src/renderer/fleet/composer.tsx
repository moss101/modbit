import { StateLine } from "../shell/state-line.tsx";
import type { AppState } from "../state/use-app.ts";

export function Composer({ app }: { app: AppState }) {
  const { core, goal, setGoal, issueUrl, setIssueUrl, workspaceRoot, setWorkspaceRoot, submitting, provider, trusted, submit, composer } = app;
  return (
    <form className="composer" onSubmit={submit} aria-label="New Task">
      <h2 style={{ margin: 0, fontSize: 14 }}>New Task</h2>
      <label htmlFor="goal" className="meta">Goal</label>
      <textarea id="goal" data-testid="goal" value={goal} onChange={(e) => setGoal(e.target.value)} placeholder="What should the agent achieve?" disabled={core.state !== "connected"} />
      <label htmlFor="issue-url" className="meta">Or from an issue (a GitHub issue URL; its text enters as untrusted context)</label>
      <input id="issue-url" data-testid="issue-url" value={issueUrl} onChange={(e) => setIssueUrl(e.target.value)} placeholder="https://github.com/owner/repo/issues/123" disabled={core.state !== "connected"} />
      <label htmlFor="workspace" className="meta">Workspace root (a local Git checkout; empty for a Work space)</label>
      <input id="workspace" data-testid="workspace" value={workspaceRoot} onChange={(e) => setWorkspaceRoot(e.target.value)} placeholder="/path/to/repo" disabled={core.state !== "connected"} />
      <div className="meta">Execution: local_trusted · Origin: desktop · Running here trusts this repository, scoped to it, if it is not trusted yet.</div>
      {provider !== null && !provider.configured && (
        <div className="meta" role="status" data-testid="composer-no-provider">
          No provider is set up: a task can be created but will not start until step 2 above is done.
        </div>
      )}
      <button type="submit" data-testid="run" disabled={core.state !== "connected" || submitting || (!goal.trim() && !issueUrl.trim())}>
        {submitting ? "Creating…" : "Run"}
      </button>
      {core.state !== "connected" && <div className="meta">Task creation is disabled until the Core is connected.</div>}
      <StateLine state={composer} testid="composer-state" />
    </form>
  );
}
