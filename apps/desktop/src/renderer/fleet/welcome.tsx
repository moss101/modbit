
import type { AppState } from "../state/use-app.ts";

export function Welcome({ app }: { app: AppState }) {
  const { core, error, goal, submitting, provider, providerKind, setProviderKind, providerKey, setProviderKey, providerUrl, setProviderUrl, providerBusy, providerResult, trustRoot, setTrustRoot, trusted, trustBusy, trustError, starters, confirm, submit, setupProvider, trustRepository, runStarter, tasks } = app;
  return (
    <section className="welcome" data-testid="welcome" aria-label="Welcome">
      <h2 style={{ margin: 0, fontSize: 16 }}>Welcome to Modbit</h2>
      <p className="meta">Three steps to a first reviewed change. Nothing here is stored outside the Core and your OS keychain.</p>
      <ol className="steps">
        <li data-testid="welcome-step-core" data-done="true">Core running (pid {core.state === "connected" ? core.pid : "…"})</li>
        <li data-testid="welcome-step-provider" data-done={provider?.configured ? "true" : "false"} data-active={!provider?.configured ? "true" : "false"}>
          <strong>Provider.</strong>{" "}
          {provider?.configured ? (
            <span data-testid="provider-ready">
              Ready: {provider.endpoints.join(", ")}.
              {providerResult?.ok && (
                <span className="meta" role="status" data-testid="provider-result" data-ok="true">
                  {" "}
                  {providerResult.text}
                </span>
              )}
            </span>
          ) : (
            <form
              onSubmit={(ev) => {
                ev.preventDefault();
                void setupProvider();
              }}
              aria-label="Provider setup"
            >
              <label htmlFor="provider-kind" className="meta">Provider</label>
              <select id="provider-kind" data-testid="provider-kind" value={providerKind} onChange={(e) => setProviderKind(e.target.value as "openai" | "anthropic")}>
                <option value="openai">OpenAI (or a compatible endpoint)</option>
                <option value="anthropic">Anthropic</option>
              </select>
              <label htmlFor="provider-key" className="meta">API key (goes to the OS keychain through the app; never shown again)</label>
              <input id="provider-key" data-testid="provider-key" type="password" autoComplete="off" value={providerKey} onChange={(e) => setProviderKey(e.target.value)} />
              <label htmlFor="provider-url" className="meta">Endpoint URL (optional; a compatible endpoint)</label>
              <input id="provider-url" data-testid="provider-url" value={providerUrl} onChange={(e) => setProviderUrl(e.target.value)} placeholder="https://" />
              <button type="submit" data-testid="provider-test" disabled={providerBusy}>
                {providerBusy ? "Testing…" : "Test and save"}
              </button>
              {providerResult && (
                <div className="meta" role="status" data-testid="provider-result" data-ok={providerResult.ok ? "true" : "false"}>
                  {providerResult.text}
                </div>
              )}
            </form>
          )}
        </li>
        <li data-testid="welcome-step-repository" data-done={trusted ? "true" : "false"} data-active={provider?.configured && !trusted ? "true" : "false"}>
          <strong>Repository.</strong>{" "}
          {trusted ? (
            <span data-testid="repository-trusted">Trusted: {trusted}. Indexing runs in the background; you can start now.</span>
          ) : (
            <form
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
            </form>
          )}
        </li>
      </ol>
      {trusted && starters && (
        <div data-testid="starter-tasks">
          <strong>First task</strong>
          <span className="meta"> — detected: {starters.stacks.join(", ")}. Pick one, or write your own goal below.</span>
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
