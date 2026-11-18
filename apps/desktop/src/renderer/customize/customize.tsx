/**
 * Customize (REQ-PX-106; REQ-PX-207, DR-PX-2026-11-11-021 MWS-14's page shape): a 45 px head with the page's name and one
 * search across its sections, a section list at the left, and the sections as lists of rows over the registries the Core
 * already has — skills, the instruction layers it loaded (Rules), hooks, the extensions a session loaded with their tool
 * servers, and memory — with only the actions the Core already offers on them: review a skill and trust it by the content hash
 * the review shows, revoke it, switch a hook on or off at its scope, release a quarantined extension for the exact manifest
 * shown or unload it, import another agent's configuration (it arrives quarantined), keep, share or delete a memory, and open a
 * source in the file manager. Rules is read-only: it lists the layers the Core recorded as loaded for the task's last turn
 * (`contextInspector(...).instructions`), apart from the files it found and did not load, and offers nothing but Open source.
 *
 * There is no catalog, no install from a network location and no update flow here. Everything shown is the Core's answer; the
 * search filters what the page already read and keeps no index; nothing here holds a registry or decides trust, scope or
 * eligibility.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button, Dialog, Icon, IconButton, type IconName } from "@modbit/ui";
import type { CustomizeHooksView, ImportReportInfo, MemoryView, SkillRow, SkillsView } from "../../shared/customize-types.ts";
import { parseRefusal } from "../projects/project-model.ts";
import {
  CUSTOMIZE_SECTIONS,
  MEMORY_SHARE_SCOPES,
  baseName,
  budgetText,
  customizeHits,
  extensionState,
  hookName,
  hookWhen,
  invocationWords,
  lastResult,
  layerWords,
  memoryWhere,
  offersRevoke,
  offersShare,
  offersTrust,
  refusalWords,
  ruleSize,
  ruleWhen,
  rulesOf,
  scopeWords,
  serverWords,
  shareText,
  shareWords,
  shortHash,
  signatureWords,
  skillKey,
  skillScopeWords,
  skillTag,
  trustWords,
  whyLocked,
  type Hit,
  type RuleLayer,
  type SectionId,
} from "./customize-model.ts";

export interface CustomizeProps {
  connected: boolean;
  sessionId: string | null;
  /** The task whose hooks, memory and rules are in view (each is resolved for a task); null when the session has none. */
  taskId: string | null;
  /** That task's title, for the words that say whose rules these are. */
  taskTitle?: string | null | undefined;
}

const SECTION_ICON: Record<SectionId, IconName> = { skills: "blocks", rules: "file-text", hooks: "terminal", extensions: "layers", memory: "bookmark" };

export function Customize({ connected, sessionId, taskId, taskTitle }: CustomizeProps) {
  const [skills, setSkills] = useState<SkillsView | null>(null);
  const [hooks, setHooks] = useState<CustomizeHooksView | null>(null);
  const [memory, setMemory] = useState<MemoryView | null>(null);
  const [rules, setRules] = useState<RuleLayer[] | null>(null);
  const [rulesError, setRulesError] = useState<string | null>(null);
  const [noteTopic, setNoteTopic] = useState("");
  const [noteText, setNoteText] = useState("");
  const [noteScope, setNoteScope] = useState<"user" | "session">("user");
  const [shareScope, setShareScope] = useState<string>(MEMORY_SHARE_SCOPES[1]);
  const [error, setError] = useState<string | null>(null);
  const [refusal, setRefusal] = useState<{ code: string; detail: string } | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [importName, setImportName] = useState("imported");
  const [imported, setImported] = useState<ImportReportInfo | null>(null);
  const [query, setQuery] = useState("");
  const [section, setSection] = useState<SectionId>("skills");
  const [found, setFound] = useState<{ section: SectionId; key: string } | null>(null);
  const [reviewing, setReviewing] = useState<SkillRow | null>(null);
  const seq = useRef(0);
  const body = useRef<HTMLDivElement>(null);

  const load = useCallback(async () => {
    if (!connected) return;
    const mine = ++seq.current;
    try {
      const [s, h, m] = await Promise.all([window.modbit.customizeSkills(taskId ?? undefined), sessionId ? window.modbit.customizeHooks(sessionId, taskId ?? undefined) : Promise.resolve(null), taskId ? window.modbit.customizeMemory(taskId) : Promise.resolve(null)]);
      if (mine !== seq.current) return;
      setSkills(s);
      setHooks(h);
      setMemory(m);
      setError(null);
    } catch (e) {
      if (mine === seq.current) setError((e as Error).message);
    }
    // Rules are the Core's record for the task; a failure to read them leaves the rest of the page as it is.
    if (!taskId) {
      if (mine === seq.current) {
        setRules([]);
        setRulesError(null);
      }
      return;
    }
    try {
      const v = await window.modbit.contextInspector(taskId);
      if (mine !== seq.current) return;
      setRules(v.instructions ?? []);
      setRulesError(null);
    } catch (e) {
      if (mine === seq.current) setRulesError((e as Error).message);
    }
  }, [connected, sessionId, taskId]);
  useEffect(() => {
    void load();
  }, [load]);

  /** Run one action: a refusal is shown with the Core's own code and detail, and the lists are read again either way. */
  const act = async <T,>(run: () => Promise<T>, done?: (r: T) => string): Promise<T | null> => {
    setBusy(true);
    setRefusal(null);
    setNote(null);
    try {
      const r = await run();
      if (done) setNote(done(r));
      return r;
    } catch (e) {
      setRefusal(parseRefusal(e));
      return null;
    } finally {
      setBusy(false);
      await load();
    }
  };

  const ruleLists = useMemo(() => rulesOf(rules ?? []), [rules]);
  const hits = useMemo(
    () => customizeHits(query, { skills: skills?.skills ?? [], rules: rules ?? [], hooks: taskId ? (hooks?.hooks ?? []) : [], extensions: hooks?.extensions ?? [], memory: memory?.items ?? [] }),
    [query, skills, rules, hooks, memory, taskId],
  );
  const counts: Record<SectionId, number | null> = {
    skills: skills ? skills.skills.length : null,
    rules: rules ? ruleLists.loaded.length : null,
    hooks: hooks && taskId ? hooks.hooks.length : null,
    extensions: hooks ? hooks.extensions.length : null,
    memory: memory ? memory.items.length : null,
  };

  /** Shows a section: the search is put away and the section is brought into view. */
  const goTo = (id: SectionId) => {
    setQuery("");
    setSection(id);
    setFound(null);
    requestAnimationFrame(() => body.current?.querySelector<HTMLElement>(`[data-section="${id}"]`)?.scrollIntoView({ block: "start" }));
  };
  /** Opens a search result: its own section, its own row, focused. */
  const openHit = (hit: Hit) => {
    setQuery("");
    setSection(hit.section);
    setFound({ section: hit.section, key: hit.key });
  };
  useEffect(() => {
    if (!found || query) return;
    const row = [...(body.current?.querySelectorAll<HTMLElement>(`[data-section="${found.section}"] [data-key]`) ?? [])].find((el) => el.getAttribute("data-key") === found.key);
    if (!row) return;
    row.scrollIntoView({ block: "center" });
    row.focus();
  }, [found, query]);

  if (error) {
    return (
      <section className="pg cz" aria-label="Customize" data-testid="customize-unavailable">
        <header className="pg-head">
          <h2 className="pg-title">Customize</h2>
        </header>
        <div className="pg-body">
          <p className="meta" role="status" data-testid="customize-error">
            Customize could not be read: {error}
          </p>
          <Button size="sm" onClick={() => void load()} data-testid="customize-refresh">
            Try again
          </Button>
        </div>
      </section>
    );
  }

  const isFound = (s: SectionId, key: string) => (found && found.section === s && found.key === key ? "true" : undefined);

  return (
    <section className="pg cz" aria-label="Customize" data-testid="customize" data-section={section}>
      <header className="pg-head">
        <h2 className="pg-title">Customize</h2>
        <span className="pg-grow" />
        <label className="cz-search">
          <Icon name="search" aria-hidden="true" />
          <input
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape" && query) {
                e.stopPropagation();
                setQuery("");
              }
            }}
            placeholder="Search skills, rules, hooks and more"
            aria-label="Search skills, rules, hooks, extensions and memory"
            data-testid="customize-search"
          />
        </label>
        <IconButton icon={<Icon name="rotate-ccw" />} label="Read everything again" size="md" onClick={() => void load()} data-testid="customize-refresh" />
      </header>
      <div className="pg-split">
        <nav className="cz-nav" aria-label="Customize sections" data-testid="customize-nav">
          <span className="cz-cap">What the agent can use</span>
          {CUSTOMIZE_SECTIONS.map((s) => (
            <button key={s.id} type="button" className="cz-nav-row" aria-current={!query && section === s.id ? "true" : undefined} onClick={() => goTo(s.id)} data-testid={`customize-nav-${s.id}`}>
              <Icon name={SECTION_ICON[s.id]} aria-hidden="true" />
              <span className="cz-nav-label">{s.label}</span>
              {counts[s.id] !== null && (
                <span className="cz-count" aria-label={`${counts[s.id]} listed`}>
                  {counts[s.id]}
                </span>
              )}
            </button>
          ))}
        </nav>
        <div className="pg-body cz-body" ref={body}>
          {refusal && (
            <div className="changes-refusal" role="alert" data-testid="customize-refusal" data-code={refusal.code}>
              <p>
                <strong>{refusal.code || "Refused"}</strong> {refusalWords(refusal.code)}
              </p>
              {refusal.detail && <p className="meta">{refusal.detail}</p>}
            </div>
          )}
          {note && (
            <p className="meta" role="status" data-testid="customize-note">
              {note}
            </p>
          )}

          {query.trim() ? (
            <section className="cz-results" aria-label="Search results" data-testid="customize-results" data-count={hits.length}>
              <p className="pg-h">{hits.length === 0 ? `Nothing in skills, rules, hooks, extensions or memory matches “${query.trim()}”.` : `${hits.length} ${hits.length === 1 ? "match" : "matches"} for “${query.trim()}”`}</p>
              {hits.length > 0 && (
                <ul className="pg-list">
                  {hits.map((h) => (
                    <li key={`${h.section}:${h.key}`}>
                      <button type="button" className="pg-row pg-row-button" onClick={() => openHit(h)} data-testid="customize-result" data-section={h.section} data-key={h.key}>
                        <span className="pg-ico">
                          <Icon name={SECTION_ICON[h.section]} aria-hidden="true" />
                        </span>
                        <span className="pg-main">
                          <span className="pg-title-line">{h.title}</span>
                          <span className="pg-sub">{h.detail}</span>
                        </span>
                        <span className="pg-aside">{CUSTOMIZE_SECTIONS.find((s) => s.id === h.section)?.label}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </section>
          ) : (
            <>
              {/* ------------------------------------------------------------------ Skills */}
              <section className="cz-sec" aria-labelledby="customize-skills-h" data-testid="customize-skills" data-section="skills">
                <h3 className="pg-h" id="customize-skills-h">
                  Skills
                </h3>
                <p className="pg-intro">Instructions the agent loads when a task calls for them. A skill that is not trusted never reaches the model: review it, then trust that exact version.</p>
                {!skills ? (
                  <p className="meta" role="status">
                    Reading the skills…
                  </p>
                ) : (
                  <>
                    <p className="meta" data-testid="customize-skills-budget">
                      {budgetText(skills)}
                    </p>
                    {skills.skills.length === 0 && <p className="meta">No skill is installed in the profile, the System scope or this task's scopes.</p>}
                    {skills.skills.length > 0 && (
                      <ul className="pg-list" aria-label="Skills">
                        {skills.skills.map((s) => {
                          const tag = skillTag(s);
                          return (
                            <li key={skillKey(s)} className="pg-row" tabIndex={-1} data-key={skillKey(s)} data-found={isFound("skills", skillKey(s))} data-testid="customize-skill" data-name={s.name} data-scope={s.scope} data-trust={s.trust} data-hash={s.contentHash} data-enabled={s.enabled ? "true" : "false"}>
                              <span className="pg-ico">
                                <Icon name="blocks" aria-hidden="true" />
                              </span>
                              <div className="pg-main">
                                <span className="pg-title-line">
                                  <strong>{s.name}</strong> <span className="pg-dim">version {s.version}</span>
                                </span>
                                <span className="pg-sub">
                                  {s.description} · {skillScopeWords(s.scope)}
                                </span>
                                <span className="pg-sub" data-testid="customize-skill-facts">
                                  <span title={s.contentHash}>hash {shortHash(s.contentHash)}</span> · {invocationWords(s.invocation)} · {shareText(s)}
                                  {s.paths.length > 0 ? ` · applies to ${s.paths.join(", ")} (${s.pathsActive ? "in use now" : "not in use now"})` : ""}
                                </span>
                                <span className="pg-sub" data-testid="customize-skill-provenance">
                                  {s.provenanceAuthor || s.provenanceSource || s.provenanceLicense ? `from ${s.provenanceAuthor || "unknown author"}${s.provenanceSource ? `, ${s.provenanceSource}` : ""}${s.provenanceLicense ? `, ${s.provenanceLicense}` : ""}` : "no provenance recorded"} · {s.trustDetail || trustWords(s.trust)}
                                </span>
                              </div>
                              <span className="pg-tag" data-tone={tag.tone} title={trustWords(s.trust)}>
                                {tag.text}
                              </span>
                              <div className="pg-actions">
                                {offersTrust(s) && (
                                  <Button size="sm" variant="primary" disabled={busy} onClick={() => setReviewing(s)} aria-label={`Review ${s.name} before trusting it`} data-testid="customize-skill-review">
                                    Review and trust
                                  </Button>
                                )}
                                {offersRevoke(s) && (
                                  <Button size="sm" disabled={busy} onClick={() => void act(() => window.modbit.customizeSkillRevoke(s.name, s.contentHash), (r) => `Revoked ${r.name}.`)} aria-label={`Revoke trust in ${s.name}`} data-testid="customize-skill-revoke">
                                    Revoke
                                  </Button>
                                )}
                                <Button size="sm" disabled={busy} onClick={() => void act(() => window.modbit.customizeOpenSource("skill", s.name, undefined, taskId ?? undefined))} aria-label={`Open the source of ${s.name}`} data-testid="customize-skill-open">
                                  Open source
                                </Button>
                              </div>
                            </li>
                          );
                        })}
                      </ul>
                    )}
                    {skills.rejected.length > 0 && (
                      <ul className="pg-notes" aria-label="Skills that could not be loaded" data-testid="customize-skills-rejected">
                        {skills.rejected.map((r) => (
                          <li key={r.source} className="meta">
                            {r.source}: {r.reason || r.code}
                          </li>
                        ))}
                      </ul>
                    )}
                  </>
                )}
              </section>

              {/* ------------------------------------------------------------------ Rules (read-only) */}
              <section className="cz-sec" aria-labelledby="customize-rules-h" data-testid="customize-rules" data-section="rules">
                <h3 className="pg-h" id="customize-rules-h">
                  Rules
                </h3>
                <p className="pg-intro">
                  {taskId
                    ? `The instruction files the Core put in front of the model on the last turn of “${taskTitle || "this task"}”, in the order it applies them. They are read-only here: change a file in your editor and the Core reads it again on the next turn.`
                    : "Rules are recorded for a task: open one to see the instruction files the Core loaded for its folder."}
                </p>
                {taskId && rulesError && (
                  <p className="meta" role="status" data-testid="customize-rules-error">
                    The rules could not be read: {rulesError}
                  </p>
                )}
                {taskId && !rulesError && rules === null && (
                  <p className="meta" role="status">
                    Reading the rules…
                  </p>
                )}
                {taskId && rules !== null && ruleLists.loaded.length === 0 && (
                  <p className="meta" data-testid="customize-rules-empty">
                    The Core has loaded no instruction file for this task yet. It records them when a turn runs.
                  </p>
                )}
                {ruleLists.loaded.length > 0 && (
                  <ul className="pg-list" aria-label="Rules" data-testid="customize-rule-list">
                    {ruleLists.loaded.map((r) => (
                      <li key={r.source} className="pg-row" tabIndex={-1} data-key={r.source} data-found={isFound("rules", r.source)} data-testid="customize-rule" data-source={r.source} data-layer={r.layer} data-hash={r.contentHash} data-reason={r.reason}>
                        <span className="pg-ico">
                          <Icon name="file-text" aria-hidden="true" />
                        </span>
                        <div className="pg-main">
                          <span className="pg-title-line">
                            <strong>{baseName(r.source)}</strong> <span className="pg-dim">{layerWords(r.layer)}</span>
                          </span>
                          <span className="pg-sub" data-testid="customize-rule-when">
                            {ruleWhen(r.reason)}
                            {ruleSize(r) ? ` · ${ruleSize(r)}` : ""}
                          </span>
                          <span className="pg-sub pg-mono" title={r.contentHash}>
                            {r.source} · hash {shortHash(r.contentHash)}
                          </span>
                          {r.scannerFindings.length > 0 && (
                            <span className="pg-sub pg-warn" data-testid="customize-rule-findings">
                              Text in this file looks like instructions to the agent ({r.scannerFindings.join(", ")}). The model is told to treat it as data.
                            </span>
                          )}
                        </div>
                        <div className="pg-actions">
                          <Button size="sm" disabled={busy || !sessionId || !taskId} onClick={() => sessionId && taskId && void act(() => window.modbit.customizeOpenSource("rule", r.source, sessionId, taskId))} aria-label={`Open the source of ${r.source}`} data-testid="customize-rule-open">
                            Open source
                          </Button>
                        </div>
                      </li>
                    ))}
                  </ul>
                )}
                {ruleLists.notLoaded.length > 0 && (
                  <details className="pg-details" data-testid="customize-rules-not-loaded">
                    <summary>Files the Core found and did not load ({ruleLists.notLoaded.length})</summary>
                    <ul className="pg-notes">
                      {ruleLists.notLoaded.map((r) => (
                        <li key={r.source} className="meta" data-testid="customize-rule-not-loaded" data-source={r.source}>
                          <span className="pg-mono">{r.source}</span>: {r.notLoadedReason}
                        </li>
                      ))}
                    </ul>
                  </details>
                )}
              </section>

              {/* ------------------------------------------------------------------ Hooks */}
              <section className="cz-sec" aria-labelledby="customize-hooks-h" data-testid="customize-hooks" data-section="hooks">
                <h3 className="pg-h" id="customize-hooks-h">
                  Hooks
                </h3>
                <p className="pg-intro">Commands the Core runs at a step of a task. One declared by a layer above you cannot be switched off here.</p>
                {!taskId && <p className="meta">Hooks are listed for a task; open one to see the hooks in force for it.</p>}
                {hooks && taskId && hooks.hooks.length === 0 && <p className="meta">No hook is in force for this task.</p>}
                {hooks && taskId && hooks.hooks.length > 0 && (
                  <ul className="pg-list" aria-label="Hooks">
                    {hooks.hooks.map((h) => (
                      <li key={h.id} className="pg-row" tabIndex={-1} data-key={h.id} data-found={isFound("hooks", h.id)} data-testid="customize-hook" data-id={h.id} data-scope={h.scope} data-enabled={h.enabled ? "true" : "false"} data-can-disable={h.canDisable ? "true" : "false"}>
                        <span className="pg-ico">
                          <Icon name="terminal" aria-hidden="true" />
                        </span>
                        <div className="pg-main">
                          <span className="pg-title-line">
                            <strong>{hookName(h.id)}</strong> <span className="pg-dim">set by {scopeWords(h.scope)}</span>
                          </span>
                          <span className="pg-sub">{hookWhen(h)}</span>
                          <span className="pg-sub">
                            runs <code>{h.command.join(" ")}</code>
                            {h.tools.length > 0 ? ` · for ${h.tools.join(", ")}` : ""}
                            {h.trustedWorkspaceGate ? " · in force only while you trust this repository" : ""}
                          </span>
                          <span className="pg-sub" data-testid="customize-hook-last">
                            Last result: {lastResult(h)}
                          </span>
                          {whyLocked(h) && (
                            <span className="pg-sub" data-testid="customize-hook-locked">
                              {whyLocked(h)}
                            </span>
                          )}
                        </div>
                        {!h.enabled && (
                          <span className="pg-tag" data-tone="warn">
                            Off
                          </span>
                        )}
                        <div className="pg-actions">
                          <Button size="sm" aria-pressed={h.enabled} disabled={busy || (h.enabled && !h.canDisable) || !sessionId} onClick={() => sessionId && void act(() => window.modbit.customizeHookEnabled(sessionId, taskId, h.id, !h.enabled), () => `${h.enabled ? "Switched off" : "Switched on"} ${hookName(h.id)}.`)} aria-label={`${h.enabled ? "Switch off" : "Switch on"} hook ${h.id}`} data-testid="customize-hook-toggle">
                            {h.enabled ? "Switch off" : "Switch on"}
                          </Button>
                        </div>
                      </li>
                    ))}
                  </ul>
                )}
                {hooks && hooks.refused.length > 0 && (
                  <ul className="pg-notes" aria-label="Hooks that were refused" data-testid="customize-hooks-refused">
                    {hooks.refused.map((r) => (
                      <li key={r} className="meta">
                        {r}
                      </li>
                    ))}
                  </ul>
                )}
              </section>

              {/* ------------------------------------------------------------------ Extensions and their tool servers */}
              <section className="cz-sec" aria-labelledby="customize-ext-h" data-testid="customize-extensions" data-section="extensions">
                <h3 className="pg-h" id="customize-ext-h">
                  Extensions and tool servers
                </h3>
                <p className="pg-intro">What this session loaded, with the tool servers each declares. An extension that is not signed by a key you trust stays quarantined, and its servers never start, until you trust its exact manifest.</p>
                {hooks && hooks.extensions.length === 0 && <p className="meta">No extension is loaded in this session.</p>}
                {hooks && hooks.extensions.length > 0 && (
                  <ul className="pg-list" aria-label="Extensions">
                    {hooks.extensions.map((e) => (
                      <li key={e.extensionId} className="pg-row" tabIndex={-1} data-key={e.extensionId} data-found={isFound("extensions", e.extensionId)} data-testid="customize-extension" data-id={e.extensionId} data-state={e.state} data-digest={e.manifestDigest} data-signature={e.signature}>
                        <span className="pg-ico">
                          <Icon name="layers" aria-hidden="true" />
                        </span>
                        <div className="pg-main">
                          <span className="pg-title-line">
                            <strong>{e.name}</strong> <span className="pg-dim">version {e.version}</span>
                          </span>
                          <span className="pg-sub" data-testid="customize-extension-state">
                            {extensionState(e)} · {signatureWords(e.signature)} · {e.publisher ? `from ${e.publisher}` : "no publisher"}
                            {e.source ? `, ${e.source}` : ""} · manifest {shortHash(e.manifestDigest)}
                          </span>
                          <span className="pg-sub">
                            {e.hooks.length > 0 ? `hooks ${e.hooks.join(", ")}` : "no hooks"} · {e.commands.length > 0 ? `commands ${e.commands.join(", ")}` : "no commands"}
                          </span>
                          {e.servers.length > 0 && (
                            <ul className="pg-notes" aria-label={`Tool servers of ${e.name}`}>
                              {e.servers.map((s) => (
                                <li key={s.name} className="pg-sub" data-testid="customize-server" data-name={s.name} data-state={s.state}>
                                  Tool server {s.name} ({s.transport}): {serverWords(s)}
                                  {s.scopes.length > 0 ? ` · scopes ${s.scopes.join(", ")}` : ""}
                                </li>
                              ))}
                            </ul>
                          )}
                        </div>
                        <span className="pg-tag" data-tone={e.state === "ACTIVE" ? "ok" : "warn"}>
                          {e.state === "ACTIVE" ? "Active" : "Quarantined"}
                        </span>
                        <div className="pg-actions">
                          {e.canTrust && sessionId && (
                            <Button size="sm" variant="primary" disabled={busy} onClick={() => void act(() => window.modbit.customizeExtensionTrust(sessionId, e.extensionId, e.manifestDigest), (r) => `Released ${r.name} for manifest ${shortHash(e.manifestDigest)}.`)} aria-label={`Trust ${e.name} at manifest ${shortHash(e.manifestDigest)}`} data-testid="customize-extension-trust">
                              Trust this manifest
                            </Button>
                          )}
                          {sessionId && (
                            <Button size="sm" disabled={busy} onClick={() => void act(() => window.modbit.customizeExtensionUnload(sessionId, e.extensionId), (r) => `Unloaded ${r.name}; none of its hooks runs again.`)} aria-label={`Unload ${e.name}`} data-testid="customize-extension-unload">
                              Unload
                            </Button>
                          )}
                          <Button size="sm" disabled={busy || !sessionId} onClick={() => sessionId && void act(() => window.modbit.customizeOpenSource("extension", e.extensionId, sessionId))} aria-label={`Open the source of ${e.name}`} data-testid="customize-extension-open">
                            Open source
                          </Button>
                        </div>
                      </li>
                    ))}
                  </ul>
                )}
                <div className="pg-form">
                  <label htmlFor="customize-import-name" className="meta">
                    Name for the imported configuration
                  </label>
                  <input id="customize-import-name" type="text" maxLength={64} value={importName} onChange={(e) => setImportName(e.target.value)} data-testid="customize-import-name" />
                  <Button
                    size="sm"
                    disabled={busy || !sessionId || !/^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$/.test(importName)}
                    onClick={() => sessionId && void act(() => window.modbit.customizeImport(sessionId, importName), (r) => (r ? `Imported ${r.mapped} items as ${r.name}; it is quarantined until you trust it.` : "Import cancelled.")).then((r) => setImported(r ?? null))}
                    data-testid="customize-import"
                  >
                    Import another agent's configuration…
                  </Button>
                </div>
                {imported && (
                  <div className="meta" data-testid="customize-import-report" data-quarantined={imported.quarantined ? "true" : "false"}>
                    <p>
                      Imported as {imported.name}: {imported.mapped} mapped, {imported.skipped} skipped, {imported.conflicts} conflicts. {imported.quarantined ? `Quarantined: ${imported.quarantined}` : "Active."}
                    </p>
                  </div>
                )}
              </section>

              {/* ------------------------------------------------------------------ Memory */}
              <section className="cz-sec" aria-labelledby="customize-mem-h" data-testid="customize-memory" data-section="memory">
                <h3 className="pg-h" id="customize-mem-h">
                  Memory
                </h3>
                <p className="pg-intro">What is remembered for this task, where it lives and where it came from. Delete removes an item from the store and from every later prompt. Share copies one of yours into a wider scope with personal detail removed.</p>
                {sessionId && taskId && (
                  <div className="pg-form">
                    <label htmlFor="customize-memory-topic" className="meta">
                      Remember (private to you until you share it)
                    </label>
                    <input id="customize-memory-topic" type="text" maxLength={200} placeholder="Topic" value={noteTopic} onChange={(e) => setNoteTopic(e.target.value)} data-testid="customize-memory-topic" />
                    <input type="text" aria-label="What to remember" maxLength={8192} placeholder="What to remember" value={noteText} onChange={(e) => setNoteText(e.target.value)} data-testid="customize-memory-content" />
                    <select aria-label="Keep it for" value={noteScope} onChange={(e) => setNoteScope(e.target.value as "user" | "session")} data-testid="customize-memory-note-scope">
                      <option value="user">you</option>
                      <option value="session">this session</option>
                    </select>
                    <Button
                      size="sm"
                      disabled={busy || noteTopic.trim() === "" || noteText.trim() === ""}
                      onClick={() => void act(() => window.modbit.customizeMemoryRemember(sessionId, taskId, noteScope, noteTopic, noteText), (r) => (r.existed ? "That is already remembered." : `Remembered as a proposal in ${r.scope}.`)).then((r) => r && (setNoteTopic(""), setNoteText("")))}
                      data-testid="customize-memory-remember"
                    >
                      Remember
                    </Button>
                  </div>
                )}
                {memory && memory.items.length === 0 && <p className="meta">Nothing is remembered in this task's scopes.</p>}
                {memory && memory.items.length > 0 && (
                  <label className="meta pg-form">
                    Share into{" "}
                    <select value={shareScope} onChange={(e) => setShareScope(e.target.value)} data-testid="customize-memory-share-scope">
                      {MEMORY_SHARE_SCOPES.map((k) => (
                        <option key={k} value={k}>
                          {k}
                        </option>
                      ))}
                    </select>
                  </label>
                )}
                {memory && memory.items.length > 0 && (
                  <ul className="pg-list" aria-label="Memories">
                    {memory.items.map((m) => (
                      <li key={m.id} className="pg-row" tabIndex={-1} data-key={m.id} data-found={isFound("memory", m.id)} data-testid="customize-memory-item" data-id={m.id} data-scope={m.scope} data-status={m.status}>
                        <span className="pg-ico">
                          <Icon name="bookmark" aria-hidden="true" />
                        </span>
                        <div className="pg-main">
                          <span className="pg-title-line">
                            <strong>{m.topic}</strong> <span className="pg-dim">{m.status}</span>
                          </span>
                          <span className="pg-sub pg-text">{m.content}</span>
                          <span className="pg-sub" data-testid="customize-memory-where">
                            {memoryWhere(m)}
                          </span>
                        </div>
                        <div className="pg-actions">
                          {m.status === "proposed" && sessionId && taskId && (
                            <Button size="sm" variant="primary" disabled={busy} onClick={() => void act(() => window.modbit.customizeMemoryPromote(sessionId, taskId, m.id), (r) => (r.outcome === "curated" ? `Kept ${m.topic}; it can reach later prompts.` : `Not kept (${r.refusalCode || r.outcome}).`))} aria-label={`Keep ${m.topic}`} data-testid="customize-memory-promote">
                              Keep
                            </Button>
                          )}
                          {offersShare(m) && sessionId && taskId && (
                            <Button size="sm" disabled={busy} onClick={() => void act(() => window.modbit.customizeMemoryShare(sessionId, taskId, m.id, shareScope), shareWords)} aria-label={`Share ${m.topic} into ${shareScope}`} data-testid="customize-memory-share">
                              Share
                            </Button>
                          )}
                          {sessionId && taskId && (
                            <Button size="sm" disabled={busy} onClick={() => void act(() => window.modbit.customizeMemoryForget(sessionId, taskId, m.id), () => `Deleted ${m.topic}; it is gone from the store and from the next prompt.`)} aria-label={`Delete ${m.topic}`} data-testid="customize-memory-delete">
                              Delete
                            </Button>
                          )}
                        </div>
                      </li>
                    ))}
                  </ul>
                )}
              </section>
            </>
          )}
        </div>
      </div>
      <SkillReview
        skill={reviewing}
        busy={busy}
        onClose={() => setReviewing(null)}
        onOpenSource={(s) => void act(() => window.modbit.customizeOpenSource("skill", s.name, undefined, taskId ?? undefined))}
        onTrust={(s) => {
          setReviewing(null);
          void act(() => window.modbit.customizeSkillTrust(s.name, s.contentHash, taskId ?? undefined), (r) => `Trusted ${r.name} at ${shortHash(r.contentHash)}.`);
        }}
      />
    </section>
  );
}

/**
 * The review a skill that is not trusted needs before it can reach the model: everything the Core lists of it, with the exact
 * content hash a trust decision is bound to. Trust sends that hash and nothing else; the Core refuses any other.
 */
function SkillReview({ skill: s, busy, onClose, onOpenSource, onTrust }: { skill: SkillRow | null; busy: boolean; onClose: () => void; onOpenSource: (s: SkillRow) => void; onTrust: (s: SkillRow) => void }) {
  return (
    <Dialog open={s !== null} title={s ? `Review ${s.name}` : "Review"} onClose={onClose} testId="customize-skill-review-dialog" description="Trusting lets this exact content reach the model. If the file changes, it is not trusted again until you review it.">
      {s && (
        <div className="cz-review" data-name={s.name} data-hash={s.contentHash}>
          <dl className="cz-facts">
            <dt>What it does</dt>
            <dd>{s.description || "No description"}</dd>
            <dt>Version</dt>
            <dd>{s.version}</dd>
            <dt>Where it comes from</dt>
            <dd>
              {skillScopeWords(s.scope)} · <span className="pg-mono">{s.source}</span>
            </dd>
            <dt>Who can call it</dt>
            <dd>{invocationWords(s.invocation)}</dd>
            {s.paths.length > 0 && (
              <>
                <dt>Applies to</dt>
                <dd>{s.paths.join(", ")}</dd>
              </>
            )}
            <dt>Provenance</dt>
            <dd>{s.provenanceAuthor || s.provenanceSource || s.provenanceLicense ? [s.provenanceAuthor || "unknown author", s.provenanceSource, s.provenanceLicense].filter(Boolean).join(", ") : "none recorded"}</dd>
            <dt>Now</dt>
            <dd>{trustWords(s.trust)}</dd>
            <dt>Content hash</dt>
            <dd className="pg-mono" data-testid="customize-review-hash">
              {s.contentHash}
            </dd>
          </dl>
          <div className="cz-review-actions">
            <Button size="sm" disabled={busy} onClick={() => onOpenSource(s)} aria-label={`Open the source of ${s.name}`} data-testid="customize-review-open">
              Open source
            </Button>
            <span className="pg-grow" />
            <Button size="sm" onClick={onClose} data-testid="customize-review-cancel">
              Cancel
            </Button>
            <Button size="sm" variant="primary" disabled={busy} onClick={() => onTrust(s)} aria-label={`Trust ${s.name} at ${shortHash(s.contentHash)}`} data-testid="customize-skill-trust">
              Trust this version
            </Button>
          </div>
        </div>
      )}
    </Dialog>
  );
}
