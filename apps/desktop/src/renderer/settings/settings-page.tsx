/**
 * The Settings view (AFW-A10): one centred column of grouped sections; the left column becomes its navigation
 * (`settings-nav.tsx`). Every control here does what it says through the existing typed preload calls or the shell's own
 * preferences. A setting that belongs to one task (its run mode, its send behaviour) stays with the task, and this page says so.
 */
import { useEffect, useState, type ReactNode } from "react";
import { Button, Icon, Kbd, isMacPlatform, type IconName } from "@modbit/ui";
import { DENSITIES, DENSITY_LABEL, THEME_LABEL, THEME_PREFERENCES, type Density, type ThemePreference } from "@modbit/design-tokens";
import type { NotificationKind } from "../notifications.ts";
import { StateLine } from "../shell/state-line.tsx";
import { UpdateIndicator } from "../shell/update-indicator.tsx";
import type { AppState } from "../state/use-app.ts";
import { ProviderForm } from "./provider-form.tsx";
import { localMachine } from "../location.ts";
import { GROUPINGS, ROW_DETAILS, type Grouping, type RowDetails } from "../agents/list-model.ts";
import { currentListPrefs, subscribeListPrefs, updateListPrefs } from "../agents/prefs.ts";
import { shellRegistry } from "../shell/commands.ts";
import { SCOPES, SCOPE_WORDS, shortcutSheet } from "./shortcut-sheet.ts";
import { ACCESS_LEVELS, type AccessLevel } from "../../shared/computer-types.ts";
import { ACCESS_WORDS } from "../computer/model.ts";
import { useStandingAccess } from "../computer/use-standing-access.ts";

/** What this platform has earned (docs/76), in words; the recorded state stays on `data-state` and the record's sentence in the tooltip. */
function platformWords(p: { state: string; statement: string }): string {
  // The record's sentence starts with the platform and its architecture ("macOS arm64: …").
  const where = p.statement.includes(":") ? p.statement.slice(0, p.statement.indexOf(":")) : "This platform";
  if (p.state === "RELEASE_GRADE") return `${where}: the packaged app passed its full end-to-end tests on this platform.`;
  if (p.state === "CI_COMPATIBLE") return `${where}: built and tested in CI. That is a development guarantee, not a support claim.`;
  return `${where}: not built or tested on this platform.`;
}

/** The words for a row's second line (MWS-04's Row details), as Settings says them. */
const ROW_DETAIL_WORDS: Record<RowDetails, string> = { auto: "When needed", always: "Always", never: "Never" };

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
  { id: "appearance", group: "App", label: "Appearance", icon: "eye", keywords: "theme dark light high contrast system zoom size density comfortable compact rows task list second line group grouping project status environment date" },
  { id: "notifications", group: "App", label: "Notifications", icon: "clock", keywords: "alerts attention completion failure quiet hours" },
  { id: "shortcuts", group: "App", label: "Shortcuts", icon: "circle-question-mark", keywords: "keyboard keys shortcuts chords bindings" },
  { id: "tools", group: "App", label: "Automations and worktrees", icon: "git-branch", keywords: "automations worktrees dashboard cost" },
  { id: "about", group: "About", label: "This build", icon: "file-text", keywords: "version platform languages support update updates" },
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
  /** Comfortable or Compact (MWS-03): one setting, separate from zoom. */
  density: Density;
  onDensity: (d: Density) => void;
  onCustomize: () => void;
  onAutomations: () => void;
  onWorktrees: () => void;
  onDashboard: (() => void) | null;
  onShortcuts: () => void;
  onComputerPermissions: () => void;
}

export function SettingsPage({ app, query, theme, onTheme, zoom, onZoom, density, onDensity, onCustomize, onAutomations, onWorktrees, onDashboard, onShortcuts, onComputerPermissions }: SettingsPageProps) {
  const { core, provider, credentials, credentialForm, setCredentialForm, credentialError, setCredentialError, refreshCredentials, prefs, settings, savePrefs, platform, languages } = app;
  const connected = core.state === "connected";
  // The task list's own preferences (one copy in this window, agents/prefs.ts): a choice here is the list's at once.
  const [list, setList] = useState(currentListPrefs);
  useEffect(() => subscribeListPrefs(setList), []);
  const [changingProvider, setChangingProvider] = useState(false);
  // PX-215: the person's standing level per application, held and enforced by the Core.
  const access = useStandingAccess(connected);
  const [accessForm, setAccessForm] = useState<{ application: string; level: AccessLevel }>({ application: "", level: "view_only" });
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
            <Row title="Isolated cloud" detail={<span data-testid="settings-cloud-state">Not available from this app yet: the desktop cannot hand a task to the cloud. The command line can hand a task over with <code>modbit cloud handoff</code>.</span>} />
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
          <div className="settings-card" data-testid="settings-computer-access" data-state={access.info === null ? "unknown" : "ready"}>
            <Row title="Standing access for applications" detail="How far the agent may go in one application, until you change it. Nothing is set by default: an application with no level follows each task's own grant. A level only ever narrows what a task may do, and you still approve every action." />
            {access.info !== null && access.info.entries.length === 0 && (
              <Row title="No application has a standing level" detail={<span data-testid="settings-access-none">Every application follows each task's own grant.</span>} />
            )}
            {access.info?.entries.map((e) => (
              <Row key={e.bundleId} title={<span data-testid="settings-access-app">{e.application || e.bundleId}</span>} detail={<span data-testid="settings-access-does"><code>{e.bundleId}</code> · {ACCESS_WORDS[e.level].does}</span>}>
                <select className="settings-select" aria-label={`Standing access for ${e.application || e.bundleId}`} data-testid="settings-access-level" data-bundle={e.bundleId} value={e.level} disabled={access.busy} onChange={(ev) => void access.set(e.bundleId, e.application, ev.target.value as AccessLevel | "")}>
                  <option value="">Not set</option>
                  {ACCESS_LEVELS.map((l) => (
                    <option key={l} value={l}>
                      {ACCESS_WORDS[l].label}
                    </option>
                  ))}
                </select>
              </Row>
            ))}
            <div className="settings-card-body">
              <form
                className="settings-form"
                data-testid="settings-access-form"
                onSubmit={(ev) => {
                  ev.preventDefault();
                  const running = access.info?.running.find((a) => a.bundleId === accessForm.application.trim());
                  void access.set(accessForm.application.trim(), running?.name ?? "", accessForm.level).then((ok) => ok && setAccessForm({ application: "", level: accessForm.level }));
                }}
              >
                <label className="meta">
                  Application
                  <input data-testid="settings-access-application" list="settings-access-running" placeholder="com.example.app" value={accessForm.application} onChange={(ev) => setAccessForm({ ...accessForm, application: ev.target.value })} />
                  <datalist id="settings-access-running">
                    {access.info?.running.map((a) => (
                      <option key={a.bundleId} value={a.bundleId}>
                        {a.name}
                      </option>
                    ))}
                  </datalist>
                </label>
                <label className="meta">
                  Level
                  <select className="settings-select" data-testid="settings-access-new-level" value={accessForm.level} onChange={(ev) => setAccessForm({ ...accessForm, level: ev.target.value as AccessLevel })}>
                    {ACCESS_LEVELS.map((l) => (
                      <option key={l} value={l}>
                        {ACCESS_WORDS[l].label}
                      </option>
                    ))}
                  </select>
                </label>
                <div>
                  <Button size="sm" type="submit" data-testid="settings-access-set" disabled={!connected || access.busy || accessForm.application.trim() === ""}>
                    Set level
                  </Button>
                </div>
              </form>
              <p className="meta settings-note" data-testid="settings-access-words">
                {ACCESS_WORDS[accessForm.level].does}
              </p>
              {access.note && (
                <p className="meta settings-note" role="status" data-testid="settings-access-note">
                  {access.note}
                </p>
              )}
              {access.error && (
                <p className="meta settings-note" role="alert" data-testid="settings-access-error">
                  {access.error}
                </p>
              )}
            </div>
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
                      <span title={`Handle ${c.handle}`}>{c.label}</span> · {c.origin} · {c.username || "no account name"} · {c.persisted ? "in the OS keychain" : "held for this session only"}
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
            <Row title="Signed in" detail={<span data-testid="settings-account-state">Nobody: this app has no sign-in yet, and it works without one on this Mac.</span>} />
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
            <Row title="Density" detail="Compact fits more on screen: smaller text, lower rows and bars. Zoom is separate.">
              <span className="settings-seg" role="radiogroup" aria-label="Density" data-testid="settings-density">
                {DENSITIES.map((d) => (
                  <button key={d} type="button" role="radio" aria-checked={density === d} className="settings-seg-btn" data-testid={`settings-density-${d}`} onClick={() => onDensity(d)}>
                    {DENSITY_LABEL[d]}
                  </button>
                ))}
              </span>
            </Row>
            <Row title="Second line on task rows" detail="When needed: only a task that needs you or has failed shows its reason under the title.">
              <span className="settings-seg" role="radiogroup" aria-label="Second line on task rows" data-testid="settings-row-details">
                {ROW_DETAILS.map((d) => (
                  <button key={d.id} type="button" role="radio" aria-checked={list.rowDetails === d.id} className="settings-seg-btn" data-testid={`settings-row-details-${d.id}`} onClick={() => updateListPrefs((p) => ({ ...p, rowDetails: d.id }))}>
                    {ROW_DETAIL_WORDS[d.id]}
                  </button>
                ))}
              </span>
            </Row>
            <Row title="Group tasks by" detail="The same choice as the list's own options.">
              <select className="settings-select" aria-label="Group tasks by" data-testid="settings-grouping" value={list.grouping} onChange={(e) => updateListPrefs((p) => ({ ...p, grouping: e.target.value as Grouping }))}>
                {GROUPINGS.map((g) => (
                  <option key={g.id} value={g.id}>
                    {g.label}
                  </option>
                ))}
              </select>
            </Row>
          </div>
        </Section>

        <Section id="notifications" query={query}>
          <div className="settings-card" data-testid="notification-preferences">
            <p className="meta settings-note settings-card-pad">OS notifications are off until you opt in. Inbox always lists what needs you.</p>
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

        <Section id="shortcuts" query={query}>
          <Shortcuts onShow={onShortcuts} />
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
            {platform && <Row title="Platform" detail={<span data-testid="platform-state" data-state={platform.state} title={platform.statement}>{platformWords(platform)}</span>} />}
            {languages.length > 0 && (
              <Row
                title="Languages"
                detail={
                  <span data-testid="language-labels" title={languages.map((l) => `${l.language}: ${l.label}`).join("\n")}>
                    {languages.filter((l) => l.tier === "ALPHA_BASELINE").map((l) => l.language).join(", ")} at the Alpha baseline: the agent can edit, build and test them, with no deeper code understanding claimed yet; everything else unsupported.
                  </span>
                }
              />
            )}
            <Row title="Updates" detail={<UpdateIndicator />} />
          </div>
        </Section>
      </section>
    </div>
  );
}

/**
 * The Shortcuts section (REQ-PX-208): generated from the command registry and the two key maps (settings/shortcut-sheet.ts),
 * each key checked against the map that dispatches it. Nothing here is written by hand, and nothing new is bound.
 */
function Shortcuts({ onShow }: { onShow: () => void }) {
  const mac = isMacPlatform();
  const sheet = shortcutSheet(shellRegistry.all() as never, (e, m) => shellRegistry.forEvent(e, m)?.id, mac);
  const help = shellRegistry.get("help.shortcuts")?.keys?.[0];
  return (
    <div className="settings-shortcuts" data-testid="settings-shortcut-sheet">
      <div className="settings-card">
        <Row title="Show this list anywhere" detail={help ? <>Press <Kbd chord={help} /> in any view.</> : undefined}>
          <Button size="sm" variant="outline" onClick={onShow} data-testid="settings-shortcuts">
            Show
          </Button>
        </Row>
      </div>
      {SCOPES.map((scope) => {
        const rows = sheet.rows.filter((r) => r.scope === scope);
        if (rows.length === 0) return null;
        return (
          <div key={scope} className="settings-shortcut-group" data-testid="settings-shortcut-group" data-scope={scope}>
            <h4 className="settings-subheading">{SCOPE_WORDS[scope].title}</h4>
            <p className="meta settings-note">{SCOPE_WORDS[scope].note}</p>
            <ul className="settings-card settings-shortcut-list" aria-label={SCOPE_WORDS[scope].title}>
              {rows.map((r) => (
                <li key={r.id} className="settings-row" data-testid="settings-shortcut" data-id={r.id} data-scope={r.scope} data-probes={JSON.stringify(r.probes)}>
                  <span className="settings-row-text">{r.what}</span>
                  <span className="settings-row-control settings-keys">
                    {r.probes.map((p) => (
                      <Kbd key={p.chord} chord={p.chord} />
                    ))}
                  </span>
                </li>
              ))}
            </ul>
          </div>
        );
      })}
    </div>
  );
}
