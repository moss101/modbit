/**
 * Get started (DR-PX-2026-11-11-021 MWS-12; REQ-PX-022, docs/39): the first-run steps under home's tray, while one is incomplete.
 * Every step does something real: the provider step holds the provider form and its live test call; the repository step trusts
 * one folder through the Core and then offers the starter tasks the Core detects for it; the first-task step puts the cursor in
 * the box above. A step is done when the Core or this app says so, never by a click on the step itself. Nothing here is a
 * checklist the person ticks.
 */
import { useState } from "react";
import { Icon } from "@modbit/ui";
import { ProviderForm } from "../settings/provider-form.tsx";
import type { AppState } from "../state/use-app.ts";
import { stepProgress, type StepFacts } from "./model.ts";

export function Welcome({ app, onWriteTask }: { app: AppState; onWriteTask: () => void }) {
  const [repoOpen, setRepoOpen] = useState(false);
  const { core, submitting, provider, providerResult, trustRoot, setTrustRoot, trusted, trustBusy, trustError, starters, trustRepository, runStarter, model } = app;
  const facts: StepFacts = { provider: provider?.configured === true, repository: trusted !== null, firstTask: model.tasks.size > 0 };
  const progress = stepProgress(facts);
  // A ring, filled with a check once the step is done: what is done is said in words too (data-done and the accessible text).
  const mark = (done: boolean) => (
    <span className="gs-mark" data-done={done ? "true" : "false"}>
      {done && <Icon name="check" size={12} />}
      <span className="mb-sr-only">{done ? "Done: " : "To do: "}</span>
    </span>
  );
  return (
    <section className="welcome home-block" data-testid="welcome" aria-labelledby="welcome-title" data-needs-provider={facts.provider ? "false" : "true"}>
      <div className="gs-head">
        <h2 id="welcome-title" className="home-label">
          Get started
        </h2>
        <span className="cmp-spacer" />
        <span className="meta" data-testid="welcome-progress">
          {progress.done} of {progress.total}
        </span>
        <span className="gs-meter" role="progressbar" aria-label="Setup done" aria-valuemin={0} aria-valuemax={progress.total} aria-valuenow={progress.done}>
          <i style={{ width: `${(progress.done / progress.total) * 100}%` }} />
        </span>
      </div>
      <ol className="steps">
        <li className="mb-sr-only" data-testid="welcome-step-core" data-done="true">
          Core running (pid {core.state === "connected" ? core.pid : "…"})
        </li>
        <li className="gs" data-testid="welcome-step-provider" data-done={facts.provider ? "true" : "false"} data-active={progress.next === "provider" ? "true" : "false"}>
          <div className="gs-row">
            {mark(facts.provider)}
            <span className="gs-title">Connect a model provider</span>
            {facts.provider && (
              <span data-testid="provider-ready" className="meta gs-detail">
                Ready: {provider?.endpoints.join(", ")}.
              </span>
            )}
          </div>
          {facts.provider && providerResult?.ok && (
            <p className="meta gs-body" role="status" data-testid="provider-result" data-ok="true">
              {providerResult.text}
            </p>
          )}
          {!facts.provider && (
            <div className="gs-body">
              <ProviderForm app={app} />
            </div>
          )}
        </li>
        <li className="gs" data-testid="welcome-step-repository" data-done={facts.repository ? "true" : "false"} data-active={progress.next === "repository" ? "true" : "false"}>
          {facts.repository ? (
            <div className="gs-row">
              {mark(true)}
              <span className="gs-title">Trust a repository</span>
              <span className="meta gs-detail" data-testid="repository-trusted">
                Trusted: {trusted}. Indexing runs in the background; you can start now.
              </span>
            </div>
          ) : (
            <details className="welcome-repo" data-testid="welcome-repo-toggle" onToggle={(e) => setRepoOpen(e.currentTarget.open)}>
              <summary className="gs-row">
                {mark(false)}
                <span className="gs-title">Trust a repository</span>
                <span className="meta gs-detail">and get starter tasks for it</span>
              </summary>
              {/* Rendered only while open: what a closed details hides has its transitions paused, and they would never finish. */}
              {repoOpen && (
                <form
                  className="gs-body"
                  onSubmit={(ev) => {
                    ev.preventDefault();
                    void trustRepository();
                  }}
                  aria-label="Repository trust"
                >
                  <label htmlFor="trust-root" className="meta">
                    Local repository (a Git checkout on this machine)
                  </label>
                  <input id="trust-root" data-testid="trust-root" value={trustRoot} onChange={(e) => setTrustRoot(e.target.value)} placeholder="/path/to/repo" disabled={!facts.provider} />
                  <button type="submit" data-testid="trust-confirm" disabled={!facts.provider || trustBusy || !trustRoot.trim()} title={facts.provider ? undefined : "Connect a model provider first"}>
                    {trustBusy ? "Trusting…" : "Trust this repository"}
                  </button>
                  <div className="meta">Trust is scoped to this folder only: Modbit may read it, index it and run its checks. Nothing else on this machine is in scope.</div>
                  {trustError && (
                    <div className="meta" role="alert" data-testid="trust-error">
                      {trustError}
                    </div>
                  )}
                </form>
              )}
            </details>
          )}
        </li>
        <li className="gs" data-testid="welcome-step-first-task" data-done={facts.firstTask ? "true" : "false"} data-active={progress.next === "firstTask" ? "true" : "false"}>
          {facts.firstTask ? (
            <div className="gs-row">
              {mark(true)}
              <span className="gs-title">Start your first task</span>
            </div>
          ) : (
            <button type="button" className="gs-row gs-button" data-testid="welcome-write-task" onClick={onWriteTask}>
              {mark(false)}
              <span className="gs-title">Start your first task</span>
              <span className="meta gs-detail">Describe it in the box above</span>
            </button>
          )}
        </li>
      </ol>
      {trusted && starters && (
        <div className="gs-starters" data-testid="starter-tasks">
          <p className="meta">
            Starter tasks for {starters.stacks.join(", ")}. Pick one, or write your own above.
          </p>
          <ul>
            {starters.tasks.map((t) => (
              <li key={t.id}>
                <button type="button" className="gs-starter" data-testid="starter-task" data-starter-id={t.id} disabled={submitting} onClick={() => void runStarter(t.goalText)}>
                  <Icon name="arrow-right" size={14} />
                  <span className="gs-title">{t.title}</span>
                  <span className="meta gs-detail">{t.goalText}</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
