import type { NotificationKind } from "../notifications.ts";
import { StateLine } from "../shell/state-line.tsx";
import type { AppState } from "../state/use-app.ts";

export function SettingsPanel({ app }: { app: AppState }) {
  const { core, error, provider, credentials, credentialForm, setCredentialForm, credentialError, setCredentialError, refreshCredentials, prefs, submit, attention, settings, notifications, savePrefs } = app;
  return (
    <section className="settings" data-testid="settings" aria-label="Settings" tabIndex={-1}>
      <h2 style={{ margin: 0, fontSize: 14 }}>Settings</h2>
      <div className="meta">
        Credentials: {provider === null ? "loading…" : provider.configured ? `${provider.stored ? "provider key in the OS keychain" : "provider key held for this session only"} (${provider.provider})` : "no provider key"}; the Core holds keys in memory only and never shows one again.
      </div>
      <StateLine state={settings} testid="settings-state" />
      <fieldset data-testid="credentials">
        <legend className="meta">Login credentials (bound to one origin; the agent fills them by handle and never sees a value)</legend>
        {credentials.length === 0 ? (
          <div className="meta" data-testid="credentials-empty">None yet.</div>
        ) : (
          <ul data-testid="credential-list">
            {credentials.map((c) => (
              <li key={c.handle} className="meta" data-testid="credential" data-handle={c.handle} data-origin={c.origin}>
                <code>{c.handle}</code> · {c.label} · {c.origin} · {c.username || "(no account name)"} · {c.persisted ? "in the OS keychain" : "held for this session only"}{" "}
                <button type="button" data-testid="credential-remove" onClick={() => void window.modbit.removeCredential(c.handle).then(refreshCredentials)}>
                  Forget
                </button>
              </li>
            ))}
          </ul>
        )}
        <form
          data-testid="credential-form"
          onSubmit={(e) => {
            e.preventDefault();
            setCredentialError(null);
            const f = credentialForm;
            void window.modbit
              .addCredential(f.label, f.origin, f.username, f.secret)
              .then(() => {
                setCredentialForm({ label: "", origin: "", username: "", secret: "" });
                refreshCredentials();
              })
              .catch((err: Error) => setCredentialError(err.message));
          }}
        >
          <label className="meta">
            Label <input data-testid="credential-label" value={credentialForm.label} onChange={(e) => setCredentialForm({ ...credentialForm, label: e.target.value })} />
          </label>{" "}
          <label className="meta">
            Origin <input data-testid="credential-origin" placeholder="https://host[:port]" value={credentialForm.origin} onChange={(e) => setCredentialForm({ ...credentialForm, origin: e.target.value })} />
          </label>{" "}
          <label className="meta">
            Account <input data-testid="credential-username" value={credentialForm.username} onChange={(e) => setCredentialForm({ ...credentialForm, username: e.target.value })} />
          </label>{" "}
          <label className="meta">
            Secret <input data-testid="credential-secret" type="password" value={credentialForm.secret} onChange={(e) => setCredentialForm({ ...credentialForm, secret: e.target.value })} />
          </label>{" "}
          <button type="submit" data-testid="credential-add" disabled={core.state !== "connected" || !credentialForm.label.trim() || !credentialForm.origin.trim() || !credentialForm.secret}>
            Add credential
          </button>
          {credentialError && (
            <div className="meta" role="alert" data-testid="credential-error">
              {credentialError}
            </div>
          )}
        </form>
      </fieldset>
      <fieldset data-testid="notification-preferences">
        <legend className="meta">OS notifications (off until you opt in; the in-app list is always on)</legend>
        {(["attention", "completion", "failure"] as NotificationKind[]).map((k) => (
          <label key={k} className="meta">
            <input type="checkbox" data-testid={`notify-${k}`} checked={prefs.os[k]} onChange={(e) => savePrefs({ ...prefs, os: { ...prefs.os, [k]: e.target.checked } })} /> {k}
          </label>
        ))}
        <label className="meta">
          <input type="checkbox" data-testid="notify-quiet" checked={prefs.quietHours !== null} onChange={(e) => savePrefs({ ...prefs, quietHours: e.target.checked ? { start: 22, end: 7 } : null })} /> quiet hours
        </label>
        {prefs.quietHours && (
          <span className="meta">
            {" from "}
            <input type="number" min={0} max={23} data-testid="notify-quiet-start" value={prefs.quietHours.start} onChange={(e) => savePrefs({ ...prefs, quietHours: { start: Number(e.target.value), end: prefs.quietHours!.end } })} />
            {" to "}
            <input type="number" min={0} max={23} data-testid="notify-quiet-end" value={prefs.quietHours.end} onChange={(e) => savePrefs({ ...prefs, quietHours: { start: prefs.quietHours!.start, end: Number(e.target.value) } })} />
          </span>
        )}
      </fieldset>
    </section>
  );
}
