import type { AppState } from "../state/use-app.ts";

/** docs/39 step 2: the key goes to main and the Core; the live test call confirms it or names the cause. The renderer keeps nothing. */
export function ProviderForm({ app }: { app: AppState }) {
  const { providerKind, setProviderKind, providerKey, setProviderKey, providerUrl, setProviderUrl, providerBusy, providerResult, setupProvider } = app;
  return (
    <form
      className="provider-form"
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
      <div>
        <button type="submit" data-testid="provider-test" disabled={providerBusy}>
          {providerBusy ? "Testing…" : "Test and save"}
        </button>
      </div>
      {providerResult && (
        <div className="meta" role="status" data-testid="provider-result" data-ok={providerResult.ok ? "true" : "false"}>
          {providerResult.text}
        </div>
      )}
    </form>
  );
}
