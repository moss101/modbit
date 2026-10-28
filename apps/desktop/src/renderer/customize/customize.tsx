/**
 * The Customize view (REQ-PX-106): three lists over the registries the Core
 * already has - skills, hooks and the extensions a session loaded with their
 * tool servers - and only the actions the Core already offers on them: trust
 * a skill by the content hash this view shows and revoke it, switch a hook on
 * or off at its scope, release a quarantined extension for the exact manifest
 * shown or unload it, import another agent's configuration (it arrives
 * quarantined), and open a source folder. There is no catalog, no install from
 * a network location and no update flow here (those belong to an unratified
 * record). Everything shown is the Core's answer; nothing here holds a
 * registry or decides trust, scope or eligibility.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { Badge, Button } from "@modbit/ui";
import type { CustomizeHooksView, ImportReportInfo, MemoryView, SkillsView } from "../../shared/customize-types.ts";
import { parseRefusal } from "../projects/project-model.ts";
import { MEMORY_SHARE_SCOPES, budgetText, extensionState, memoryWhere, offersShare, shareWords, lastResult, offersRevoke, offersTrust, refusalWords, scopeWords, serverWords, shareText, shortHash, trustWords, whyLocked } from "./customize-model.ts";

export interface CustomizeProps {
  connected: boolean;
  sessionId: string | null;
  /** The task whose hooks are in view (a hook is resolved for a task); null when the session has none. */
  taskId: string | null;
  onClose: () => void;
}

export function Customize({ connected, sessionId, taskId, onClose }: CustomizeProps) {
  const [skills, setSkills] = useState<SkillsView | null>(null);
  const [hooks, setHooks] = useState<CustomizeHooksView | null>(null);
  const [memory, setMemory] = useState<MemoryView | null>(null);
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
  const seq = useRef(0);

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

  if (error) {
    return (
      <section className="customize" aria-label="Customize" data-testid="customize-unavailable">
        <p className="meta" role="status" data-testid="customize-error">
          Customize could not be read: {error}
        </p>
        <Button size="sm" onClick={() => void load()} data-testid="customize-refresh">
          Try again
        </Button>
      </section>
    );
  }
  return (
    <section className="customize" aria-label="Customize" data-testid="customize">
      <div className="customize-head">
        <h2>Customize</h2>
        <Button size="sm" onClick={() => void load()} data-testid="customize-refresh">
          Refresh
        </Button>
        <Button size="sm" onClick={onClose} data-testid="customize-close">
          Close
        </Button>
      </div>
      <p className="meta">What this session holds: skills, hooks and installed extensions, as the Core lists them. Nothing is browsed or installed from here.</p>
      {refusal && (
        <div className="changes-refusal" role="alert" data-testid="customize-refusal" data-code={refusal.code}>
          <p>
            <strong>{refusal.code || "REFUSED"}</strong> {refusalWords(refusal.code)}
          </p>
          {refusal.detail && <p className="meta">{refusal.detail}</p>}
        </div>
      )}
      {note && (
        <p className="meta" role="status" data-testid="customize-note">
          {note}
        </p>
      )}

      <section aria-labelledby="customize-skills-h" data-testid="customize-skills">
        <h3 id="customize-skills-h">Skills</h3>
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
            <ul className="customize-list" aria-label="Skills">
              {skills.skills.map((s) => (
                <li key={`${s.scope}/${s.name}`} className="customize-row" data-testid="customize-skill" data-name={s.name} data-scope={s.scope} data-trust={s.trust} data-hash={s.contentHash} data-enabled={s.enabled ? "true" : "false"}>
                  <div>
                    <strong>{s.name}</strong> <span className="meta">v{s.version}</span> <Badge tone="neutral">{s.scope.toLowerCase()}</Badge> <Badge tone={s.enabled ? "ok" : "warn"}>{trustWords(s.trust)}</Badge>
                  </div>
                  <p className="meta">{s.description}</p>
                  <p className="meta" data-testid="customize-skill-facts">
                    <span title={s.contentHash}>hash {shortHash(s.contentHash)}</span> · invocation {s.invocation === "USER_ONLY" ? "by you only" : "you or the model"} · {shareText(s)}
                    {s.paths.length > 0 ? ` · paths ${s.paths.join(", ")} (${s.pathsActive ? "active" : "not active"})` : ""}
                  </p>
                  <p className="meta" data-testid="customize-skill-provenance">
                    {s.provenanceAuthor || s.provenanceSource || s.provenanceLicense ? `from ${s.provenanceAuthor || "unknown author"}${s.provenanceSource ? `, ${s.provenanceSource}` : ""}${s.provenanceLicense ? `, ${s.provenanceLicense}` : ""}` : "no provenance recorded"} · {s.trustDetail}
                  </p>
                  <div className="customize-actions">
                    {offersTrust(s) && (
                      <Button size="sm" variant="primary" disabled={busy} onClick={() => void act(() => window.modbit.customizeSkillTrust(s.name, s.contentHash, taskId ?? undefined), (r) => `Trusted ${r.name} at ${shortHash(r.contentHash)}.`)} aria-label={`Trust ${s.name} at ${shortHash(s.contentHash)}`} data-testid="customize-skill-trust">
                        Trust this version
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
              ))}
            </ul>
            {skills.rejected.length > 0 && (
              <ul className="customize-list" aria-label="Skills that could not be loaded" data-testid="customize-skills-rejected">
                {skills.rejected.map((r) => (
                  <li key={r.source} className="meta">
                    {r.source}: {r.code} {r.reason}
                  </li>
                ))}
              </ul>
            )}
          </>
        )}
      </section>

      <section aria-labelledby="customize-hooks-h" data-testid="customize-hooks">
        <h3 id="customize-hooks-h">Hooks</h3>
        {!taskId && <p className="meta">Hooks are listed for a task; open one to see the hooks in force for it.</p>}
        {hooks && taskId && hooks.hooks.length === 0 && <p className="meta">No hook is in force for this task.</p>}
        {hooks && taskId && (
          <ul className="customize-list" aria-label="Hooks">
            {hooks.hooks.map((h) => (
              <li key={h.id} className="customize-row" data-testid="customize-hook" data-id={h.id} data-scope={h.scope} data-enabled={h.enabled ? "true" : "false"} data-can-disable={h.canDisable ? "true" : "false"}>
                <div>
                  <strong>{h.id.split("/").slice(1).join("/") || h.id}</strong> <span className="meta">{h.point} · {h.mode} · fails {h.failPolicy}</span> <Badge tone="neutral">{scopeWords(h.scope)}</Badge> {!h.enabled && <Badge tone="warn">Off</Badge>}
                </div>
                <p className="meta">
                  <code>{h.command.join(" ")}</code>
                  {h.tools.length > 0 ? ` · tools ${h.tools.join(", ")}` : ""}
                  {h.trustedWorkspaceGate ? " · in force only while you trust this repository" : ""}
                </p>
                <p className="meta" data-testid="customize-hook-last">
                  Last result: {lastResult(h)}
                </p>
                {whyLocked(h) && (
                  <p className="meta" data-testid="customize-hook-locked">
                    {whyLocked(h)}
                  </p>
                )}
                <div className="customize-actions">
                  <Button size="sm" aria-pressed={h.enabled} disabled={busy || (h.enabled && !h.canDisable) || !sessionId} onClick={() => sessionId && void act(() => window.modbit.customizeHookEnabled(sessionId, taskId, h.id, !h.enabled), () => `${h.enabled ? "Switched off" : "Switched on"} ${h.id}.`)} aria-label={`${h.enabled ? "Switch off" : "Switch on"} hook ${h.id}`} data-testid="customize-hook-toggle">
                    {h.enabled ? "Switch off" : "Switch on"}
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        )}
        {hooks && hooks.refused.length > 0 && (
          <ul className="customize-list" aria-label="Hooks that were refused" data-testid="customize-hooks-refused">
            {hooks.refused.map((r) => (
              <li key={r} className="meta">
                {r}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section aria-labelledby="customize-ext-h" data-testid="customize-extensions">
        <h3 id="customize-ext-h">Extensions</h3>
        {hooks && hooks.extensions.length === 0 && <p className="meta">No extension is loaded in this session.</p>}
        <ul className="customize-list" aria-label="Extensions">
          {hooks?.extensions.map((e) => (
            <li key={e.extensionId} className="customize-row" data-testid="customize-extension" data-id={e.extensionId} data-state={e.state} data-digest={e.manifestDigest}>
              <div>
                <strong>{e.name}</strong> <span className="meta">v{e.version}</span> <Badge tone="neutral">{e.scope.toLowerCase()}</Badge> <Badge tone={e.state === "ACTIVE" ? "ok" : "warn"}>{e.state === "ACTIVE" ? "Active" : "Quarantined"}</Badge>
              </div>
              <p className="meta" data-testid="customize-extension-state">
                {extensionState(e)} · signature {e.signature} · {e.publisher ? `from ${e.publisher}` : "no publisher"}
                {e.source ? `, ${e.source}` : ""} · manifest {shortHash(e.manifestDigest)}
              </p>
              <p className="meta">
                {e.hooks.length > 0 ? `hooks ${e.hooks.join(", ")}` : "no hooks"} · {e.commands.length > 0 ? `commands ${e.commands.join(", ")}` : "no commands"}
              </p>
              {e.servers.length > 0 && (
                <ul className="customize-list" aria-label={`Tool servers of ${e.name}`}>
                  {e.servers.map((s) => (
                    <li key={s.name} className="meta" data-testid="customize-server" data-name={s.name} data-state={s.state}>
                      Tool server {s.name} ({s.transport}): {serverWords(s)}
                      {s.scopes.length > 0 ? ` · scopes ${s.scopes.join(", ")}` : ""}
                    </li>
                  ))}
                </ul>
              )}
              <div className="customize-actions">
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
        <div className="customize-import">
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
      <section aria-labelledby="customize-mem-h" data-testid="customize-memory">
        <h3 id="customize-mem-h">Memory</h3>
        <p className="meta">What is remembered for this task, where it lives and where it came from. Delete removes an item from the store and from every later prompt. Share copies one of yours into a wider scope with personal detail removed.</p>
        {sessionId && taskId && (
          <div className="customize-import">
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
          <label className="meta">
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
        <ul className="customize-list" aria-label="Memories">
          {memory?.items.map((m) => (
            <li key={m.id} className="customize-row" data-testid="customize-memory-item" data-id={m.id} data-scope={m.scope} data-status={m.status}>
              <div>
                <strong>{m.topic}</strong> <Badge tone={m.status === "curated" ? "ok" : "neutral"}>{m.status}</Badge>
              </div>
              <p>{m.content}</p>
              <p className="meta" data-testid="customize-memory-where">
                {memoryWhere(m)}
              </p>
              <div className="customize-actions">
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
      </section>
    </section>
  );
}
