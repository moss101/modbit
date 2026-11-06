
import { useState } from "react";
import { ProviderForm } from "../settings/provider-form.tsx";
import type { AppState } from "../state/use-app.ts";

export function Welcome({ app }: { app: AppState }) {
  const [repoOpen, setRepoOpen] = useState(false);
  const { core, submitting, provider, providerResult, trustRoot, setTrustRoot, trusted, trustBusy, trustError, starters, trustRepository, runStarter } = app;
  return (
    <section className="welcome" data-testid="welcome" aria-label="Setup" data-needs-provider={provider?.configured ? "false" : "true"}>
      <ol className="steps">
        <li className="mb-sr-only" data-testid="welcome-step-core" data-done="true">Core running (pid {core.state === "connected" ? core.pid : "…"})</li>
        <li className={provider?.configured && !providerResult?.ok ? "welcome-quiet" : undefined} data-testid="welcome-step-provider" data-done={provider?.configured ? "true" : "false"} data-active={!provider?.configured ? "true" : "false"}>
          <strong className={provider?.configured ? "mb-sr-only" : undefined}>Connect a model provider</strong>{" "}
          {provider?.configured ? (
            <span data-testid="provider-ready" className={providerResult?.ok ? undefined : "mb-sr-only"}>
              Ready: {provider.endpoints.join(", ")}.
              {providerResult?.ok && (
                <span className="meta" role="status" data-testid="provider-result" data-ok="true">
                  {" "}
                  {providerResult.text}
                </span>
              )}
            </span>
          ) : (
            <ProviderForm app={app} />
          )}
        </li>
        <li data-testid="welcome-step-repository" data-done={trusted ? "true" : "false"} data-active={provider?.configured && !trusted ? "true" : "false"}>
          {trusted ? (
            <span className="meta" data-testid="repository-trusted">Trusted: {trusted}. Indexing runs in the background; you can start now.</span>
          ) : (
            <details className="welcome-repo" data-testid="welcome-repo-toggle" onToggle={(e) => setRepoOpen(e.currentTarget.open)}>
            <summary className="meta">Trust a repository first and get starter tasks</summary>
            {/* Rendered only while open: what a closed details hides has its transitions paused, and they would never finish. */}
            {repoOpen && <form
              onSubmit={(ev) => {
                ev.preventDefault();
                void trustRepository();
              }}
              aria-label="Repository trust"
            >
              <label htmlFor="trust-root" className="meta">Local repository (a Git checkout on this machine)</label>
              <input id="trust-root" data-testid="trust-root" value={trustRoot} onChange={(e) => setTrustRoot(e.target.value)} placeholder="/path/to/repo" disabled={!provider?.configured} />
              <button type="submit" data-testid="trust-confirm" disabled={!provider?.configured || trustBusy || !trustRoot.trim()}>
                {trustBusy ? "Trusting…" : "Trust this repository"}
              </button>
              <div className="meta">Trust is scoped to this folder only: Modbit may read it, index it and run its checks. Nothing else on this machine is in scope.</div>
              {trustError && (
                <div className="meta" role="alert" data-testid="trust-error">
                  {trustError}
                </div>
              )}
            </form>}
            </details>
          )}
        </li>
      </ol>
      {trusted && starters && (
        <div data-testid="starter-tasks">
          <strong>First task</strong>
          <span className="meta"> — detected: {starters.stacks.join(", ")}. Pick one, or write your own below.</span>
          <ul>
            {starters.tasks.map((t) => (
              <li key={t.id}>
                <button type="button" data-testid="starter-task" data-starter-id={t.id} disabled={submitting} onClick={() => void runStarter(t.goalText)}>
                  {t.title}
                </button>{" "}
                <span className="meta">{t.goalText}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
