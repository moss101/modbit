/**
 * The Settings view (AFW-A10): one centred column of grouped sections; the left column becomes its navigation
 * (`settings-nav.tsx`). Every control here does what it says through the existing typed preload calls or the shell's own
 * preferences. A setting that belongs to one task (its run mode, its send behaviour) stays with the task, and this page says so.
 */
import { useEffect, useState, type ReactNode } from "react";
import { Button, Icon, type IconName } from "@modbit/ui";
import { THEME_LABEL, THEME_PREFERENCES, type ThemePreference } from "@modbit/design-tokens";
import type { NotificationKind } from "../notifications.ts";
import { StateLine } from "../shell/state-line.tsx";
import { UpdateIndicator } from "../shell/update-indicator.tsx";
import type { AppState } from "../state/use-app.ts";
import { ProviderForm } from "./provider-form.tsx";
import { localMachine } from "../location.ts";

export interface SettingsEntry {
  id: string;
  group: string;
  label: string;
  icon: IconName;
  keywords: string;
}

/** The groups are doc 10's Settings list: models and execution policy, execution locations, permissions, browser permissions, memory, account and cloud; the app's own preferences follow. */
export const SETTINGS_ENTRIES: readonly SettingsEntry[] = [
  { id: "provider", group: "Agent", label: "Models and providers", icon: "layers", keywords: "model openai anthropic endpoint api key keychain execution policy objective cost balance intelligence" },
  { id: "locations", group: "Agent", label: "Execution locations", icon: "laptop", keywords: "local cloud microvm sandbox worktree handoff remote continue" },
  { id: "permissions", group: "Agent", label: "Permissions", icon: "shield-check", keywords: "policy allowlist approval run mode supervised autonomous send behaviour queue steer computer accessibility screen" },
  { id: "credentials", group: "Agent", label: "Login credentials", icon: "shield-check", keywords: "password secret website origin account" },
  { id: "browser", group: "Agent", label: "Browser permissions", icon: "globe", keywords: "origins allowlist certificates trust" },
  { id: "memory", group: "Agent", label: "Memory, skills and hooks", icon: "blocks", keywords: "customize engineering memory skills hooks extensions" },
  { id: "account", group: "Account", label: "Account and cloud", icon: "globe", keywords: "sign in login oidc tenant organization cloud" },
  { id: "appearance", group: "App", label: "Appearance", icon: "eye", keywords: "theme dark light high contrast system zoom size" },
  { id: "notifications", group: "App", label: "Notifications", icon: "clock", keywords: "alerts attention completion failure quiet hours" },
  { id: "tools", group: "App", label: "Automations and worktrees", icon: "git-branch", keywords: "automations worktrees dashboard cost" },
  { id: "about", group: "About", label: "This build", icon: "file-text", keywords: "version platform languages support shortcuts keyboard update updates" },
];

export const settingsMatches = (e: SettingsEntry, query: string): boolean => {
  const q = query.trim().toLowerCase();
  return q === "" || `${e.label} ${e.group} ${e.keywords}`.toLowerCase().includes(q);
};

function Section({ id, query, children }: { id: string; query: string; children: ReactNode }) {
  const entry = SETTINGS_ENTRIES.find((e) => e.id === id)!;
  if (!settingsMatches(entry, query)) return null;
  return (
    <section className="settings-section" id={`settings-${id}`} aria-labelledby={`settings-${id}-h`} data-testid={`settings-section-${id}`}>
      <h3 className="settings-heading" id={`settings-${id}-h`}>
        {entry.label}
      </h3>
      {children}
    </section>
  );
}

function Row({ title, detail, children }: { title: ReactNode; detail?: ReactNode; children?: ReactNode }) {
  return (
    <div className="settings-row">
      <div className="settings-row-text">
        <div>{title}</div>
        {detail && <div className="meta">{detail}</div>}
      </div>
      {children && <div className="settings-row-control">{children}</div>}
    </div>
  );
}

function Toggle({ testId, label, checked, onChange }: { testId: string; label: string; checked: boolean; onChange: (on: boolean) => void }) {
  return <input type="checkbox" className="settings-toggle" role="switch" aria-label={label} data-testid={testId} checked={checked} onChange={(e) => onChange(e.target.checked)} />;
}

const NOTIFY_WORDS: Record<NotificationKind, [string, string]> = {
  attention: ["Needs attention", "An approval, a question or a blocked task"],
  completion: ["Finished", "A task is ready for review"],
  failure: ["Failed", "A task stopped on an error"],
};

export interface SettingsPageProps {
  app: AppState;
  query: string;
  theme: ThemePreference;
  onTheme: (t: ThemePreference) => void;
  zoom: number;
  onZoom: (delta: -1 | 0 | 1) => void;
  onCustomize: () => void;
  onAutomations: () => void;
  onWorktrees: () => void;
  onDashboard: (() => void) | null;
  onShortcuts: () => void;
  onComputerPermissions: () => void;
}

export function SettingsPage({ app, query, theme, onTheme, zoom, onZoom, onCustomize, onAutomations, onWorktrees, onDashboard, onShortcuts, onComputerPermissions }: SettingsPageProps) {
  const { core, provider, credentials, credentialForm, setCredentialForm, credentialError, setCredentialError, refreshCredentials, prefs, settings, savePrefs, platform, languages } = app;
  const connected = core.state === "connected";
  const [changingProvider, setChangingProvider] = useState(false);
  const [browser, setBrowser] = useState<{ origins: string[]; targets: string[]; trusts: number } | null>(null);
  useEffect(() => {
    if (!connected) return;
    let live = true;
    void Promise.all([window.modbit.browserPolicy(), window.modbit.browserCertificateTrusts()])
      .then(([p, t]) => live && setBrowser({ origins: p.allowOrigins, targets: p.allowTargets, trusts: t.length }))
      .catch(() => live && setBrowser(null));
    return () => {
      live = false;
    };
  }, [connected]);
  const noMatch = !SETTINGS_ENTRIES.some((e) => settingsMatches(e, query));
  const providerWords = provider === null ? "Reading the provider's state…" : provider.configured ? `${provider.endpoints.join(", ") || provider.provider} is set up. ${provider.stored ? "The key is in the OS keychain." : "The key is held for this session only."}` : "No provider is set up, so tasks can be created but will not start.";
  return (
    <div className="page settings-page" data-testid="settings-scroll">
      <section className="page-col settings" data-testid="settings" aria-label="Settings" tabIndex={-1}>
        <h2 className="page-title">Settings</h2>
        <StateLine state={settings} testid="settings-state" />
        {noMatch && (
          <p className="meta" data-testid="settings-no-match">
            No setting matches “{query.trim()}”.
          </p>
        )}

        <Section id="provider" query={query}>
          <div className="settings-card">
            <Row title={provider?.configured ? "Connected" : "Not connected"} detail={<span data-testid="settings-provider-status">{providerWords} The Core holds keys in memory only and never shows one again.</span>}>
              {provider !== null && provider.configured && (
                <Button size="sm" variant="outline" onClick={() => setChangingProvider((o) => !o)} aria-expanded={changingProvider} data-testid="settings-provider-change">
                  {changingProvider ? "Keep this key" : "Change"}
                </Button>
              )}
            </Row>
            {provider !== null && (!provider.configured || changingProvider) && (
              <div className="settings-card-body">
                <ProviderForm app={app} />
              </div>
            )}
          </div>
        </Section>

        <Section id="locations" query={query}>
          <div className="settings-card" data-testid="settings-locations">
            <Row title={localMachine()} detail="Available. A new task runs here unless you choose otherwise, and never takes its location from the task you last viewed. It can work in your checkout or in a worktree of its own." />
            <Row title="Isolated cloud" detail={<span data-testid="settings-cloud-state">Not available from this app yet: the desktop has no cloud client (PX-128). The command line can hand a task over with <code>modbit cloud handoff</code>.</span>} />
          </div>
        </Section>

        <Section id="permissions" query={query}>
          <div className="settings-card">
            <Row title="Run mode, allowlist and send behaviour" detail="These belong to each task. Open a task and use Run mode under its composer; its objective and model are in the composer's model menu." />
            <Row title="Computer control" detail="What this Mac allows the agent to observe and control.">
              <Button size="sm" variant="outline" onClick={onComputerPermissions} data-testid="settings-computer-permissions">
                Review
              </Button>
            </Row>
          </div>
        </Section>

        <Section id="credentials" query={query}>
          <div className="settings-card settings-card-body" data-testid="credentials">
            <p className="meta settings-note">Each is bound to one origin. The agent fills them by handle and never sees a value.</p>
            {credentials.length === 0 ? (
              <div className="meta" data-testid="credentials-empty">
                None yet.
              </div>
            ) : (
              <ul className="settings-list" data-testid="credential-list">
                {credentials.map((c) => (
                  <li key={c.handle} className="meta settings-list-row" data-testid="credential" data-handle={c.handle} data-origin={c.origin}>
                    <span>
                      <code>{c.handle}</code> · {c.label} · {c.origin} · {c.username || "(no account name)"} · {c.persisted ? "in the OS keychain" : "held for this session only"}
                    </span>
                    <Button size="sm" variant="ghost" data-testid="credential-remove" onClick={() => void window.modbit.removeCredential(c.handle).then(refreshCredentials)}>
                      Forget
                    </Button>
                  </li>
                ))}
              </ul>
            )}
            <form
              className="settings-form"
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
              </label>
              <label className="meta">
                Origin <input data-testid="credential-origin" placeholder="https://host[:port]" value={credentialForm.origin} onChange={(e) => setCredentialForm({ ...credentialForm, origin: e.target.value })} />
              </label>
              <label className="meta">
                Account <input data-testid="credential-username" value={credentialForm.username} onChange={(e) => setCredentialForm({ ...credentialForm, username: e.target.value })} />
              </label>
              <label className="meta">
                Secret <input data-testid="credential-secret" type="password" value={credentialForm.secret} onChange={(e) => setCredentialForm({ ...credentialForm, secret: e.target.value })} />
              </label>
              <div>
                <Button size="sm" type="submit" data-testid="credential-add" disabled={!connected || !credentialForm.label.trim() || !credentialForm.origin.trim() || !credentialForm.secret}>
                  Add credential
                </Button>
              </div>
              {credentialError && (
                <div className="meta" role="alert" data-testid="credential-error">
                  {credentialError}
                </div>
              )}
            </form>
          </div>
        </Section>

        <Section id="browser" query={query}>
          <div className="settings-card" data-testid="settings-browser">
            <Row title="Sites the browser may open" detail={browser === null ? "Unavailable until the Core is connected." : browser.origins.length + browser.targets.length === 0 ? "None added beyond the defaults." : [...browser.origins, ...browser.targets].join(", ")} />
            <Row title="Trusted certificates" detail={browser === null ? "Unavailable until the Core is connected." : `${browser.trusts} kept; each was decided in the browser panel.`} />
          </div>
        </Section>

        <Section id="memory" query={query}>
          <div className="settings-card">
            <Row title="Memory, skills, hooks and extensions" detail="What the agent remembers and what it may run.">
              <Button size="sm" variant="outline" onClick={onCustomize} data-testid="settings-open-customize">
                Open
              </Button>
            </Row>
          </div>
        </Section>

        <Section id="account" query={query}>
          <div className="settings-card" data-testid="settings-account">
            <Row title="Signed in" detail={<span data-testid="settings-account-state">Nobody: this app has no sign-in yet (PX-128, PX-129), and it works without one on this Mac.</span>} />
          </div>
        </Section>

        <Section id="appearance" query={query}>
          <div className="settings-card">
            <Row title="Theme">
              <span className="settings-seg" role="radiogroup" aria-label="Theme">
                {THEME_PREFERENCES.map((t) => (
                  <button key={t} type="button" role="radio" aria-checked={theme === t} className="settings-seg-btn" data-testid={`settings-theme-${t}`} onClick={() => onTheme(t)}>
                    {THEME_LABEL[t]}
                  </button>
                ))}
              </span>
            </Row>
            <Row title="Zoom" detail={<span data-testid="settings-zoom">{Math.round(zoom * 100)}%</span>}>
              <Button size="sm" variant="outline" aria-label="Zoom out" onClick={() => onZoom(-1)} data-testid="settings-zoom-out">
                −
              </Button>
              <Button size="sm" variant="outline" aria-label="Zoom in" onClick={() => onZoom(1)} data-testid="settings-zoom-in">
                +
              </Button>
              <Button size="sm" variant="ghost" onClick={() => onZoom(0)} data-testid="settings-zoom-reset">
                Reset
              </Button>
            </Row>
          </div>
        </Section>

        <Section id="notifications" query={query}>
          <div className="settings-card" data-testid="notification-preferences">
            <p className="meta settings-note settings-card-pad">OS notifications are off until you opt in. Approvals always lists what needs you.</p>
            {(["attention", "completion", "failure"] as NotificationKind[]).map((k) => (
              <Row key={k} title={NOTIFY_WORDS[k][0]} detail={NOTIFY_WORDS[k][1]}>
                <Toggle testId={`notify-${k}`} label={`Notify: ${NOTIFY_WORDS[k][0]}`} checked={prefs.os[k]} onChange={(on) => savePrefs({ ...prefs, os: { ...prefs.os, [k]: on } })} />
              </Row>
            ))}
            <Row
              title="Quiet hours"
              detail={
                prefs.quietHours ? (
                  <span className="settings-hours">
                    From <input type="number" min={0} max={23} aria-label="Quiet hours start" data-testid="notify-quiet-start" value={prefs.quietHours.start} onChange={(e) => savePrefs({ ...prefs, quietHours: { start: Number(e.target.value), end: prefs.quietHours!.end } })} /> to <input type="number" min={0} max={23} aria-label="Quiet hours end" data-testid="notify-quiet-end" value={prefs.quietHours.end} onChange={(e) => savePrefs({ ...prefs, quietHours: { start: prefs.quietHours!.start, end: Number(e.target.value) } })} />
                  </span>
                ) : (
                  "Nothing reaches the OS during these hours."
                )
              }
            >
              <Toggle testId="notify-quiet" label="Quiet hours" checked={prefs.quietHours !== null} onChange={(on) => savePrefs({ ...prefs, quietHours: on ? { start: 22, end: 7 } : null })} />
            </Row>
          </div>
        </Section>

        <Section id="tools" query={query}>
          <div className="settings-card">
            {(
              [
                ["Automations", "Tasks that run on a schedule or a trigger", onAutomations, "settings-open-automations"],
                ["Worktrees", "The worktrees your tasks created", onWorktrees, "settings-open-worktrees"],
                ["Dashboard", onDashboard ? "Cost and activity across tasks" : "Available once a task exists", onDashboard, "settings-open-dashboard"],
              ] as const
            ).map(([title, detail, open, testId]) => (
              <Row key={title} title={title} detail={detail}>
                <Button size="sm" variant="ghost" disabled={open === null} onClick={() => open?.()} data-testid={testId} aria-label={`Open ${title}`}>
                  <Icon name="chevron-right" size={14} />
                </Button>
              </Row>
            ))}
          </div>
        </Section>

        <Section id="about" query={query}>
          <div className="settings-card" data-testid="about-build">
            {platform && <Row title="Platform" detail={<span data-testid="platform-state" data-state={platform.state}>{platform.statement}</span>} />}
            {languages.length > 0 && (
              <Row
                title="Languages"
                detail={
                  <span data-testid="language-labels" title={languages.map((l) => `${l.language}: ${l.label}`).join("\n")}>
                    {languages.filter((l) => l.tier === "ALPHA_BASELINE").map((l) => l.language).join(", ")} at the Alpha baseline (Tier C plus compile and test evidence; structural intelligence not yet claimed); everything else unsupported.
                  </span>
                }
              />
            )}
            <Row title="Updates" detail={<UpdateIndicator />} />
            <Row title="Keyboard shortcuts">
              <Button size="sm" variant="outline" onClick={onShortcuts} data-testid="settings-shortcuts">
                Show
              </Button>
            </Row>
          </div>
        </Section>
      </section>
    </div>
  );
}
