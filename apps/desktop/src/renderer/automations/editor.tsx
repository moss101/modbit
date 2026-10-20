/**
 * The definition editor (AUT-D03, REQ-PX-086): a JSON document and the Core's
 * own verdict on it. Every edit is sent to ValidateAutomation (debounced here
 * for load only: nothing depends on the timing, the verdict always belongs to
 * the exact text on screen and Save is possible only for text the Core has
 * called valid). The issues, the canonical hash, the capability lists and the
 * next due time of each schedule are the Core's words, never recomputed in the
 * renderer. Saving an update makes a new version, which needs a new approval.
 */
import { useEffect, useId, useRef, useState } from "react";
import { Button, StatusDot } from "@modbit/ui";
import type { AutoValidation, AutoView } from "../../shared/automation-types.ts";
import { newCommandId, type AutomationCalls } from "./calls.ts";
import { effectWords, nextDueWords, refusalSentence } from "./model.ts";

const DEBOUNCE_MS = 200;

export interface EditorTarget {
  /** null = a new definition. */
  automationId: string | null;
  text: string;
  workspaceRoot: string;
  /** A repository-supplied definition is read here, never edited: the file is the source. */
  readOnlyReason?: string | undefined;
  version?: number | undefined;
}

export interface EditorProps {
  calls: AutomationCalls;
  target: EditorTarget;
  nowMs: number;
  onSaved: (view: AutoView, created: boolean) => void;
  onClose: () => void;
  /** The gallery pins a verdict instead of asking the Core. */
  fixedVerdict?: AutoValidation | undefined;
}

type Verdict = { forText: string; result: AutoValidation | null; error: string | null };

export function DefinitionEditor({ calls, target, nowMs, onSaved, onClose, fixedVerdict }: EditorProps) {
  const [text, setText] = useState(target.text);
  const [root, setRoot] = useState(target.workspaceRoot);
  const [id, setId] = useState<string | null>(target.automationId);
  const [verdict, setVerdict] = useState<Verdict>(fixedVerdict ? { forText: target.text, result: fixedVerdict, error: null } : { forText: "\u0000", result: null, error: null });
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState<{ version: number; created: boolean } | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const seq = useRef(0);
  const cmd = useRef({ key: "", id: newCommandId() });
  const statusId = useId();
  const readOnly = target.readOnlyReason !== undefined;

  useEffect(() => {
    if (fixedVerdict) return;
    const mine = ++seq.current;
    const t = setTimeout(() => {
      calls
        .validate(text)
        .then((result) => mine === seq.current && setVerdict({ forText: text, result, error: null }))
        .catch((e: Error) => mine === seq.current && setVerdict({ forText: text, result: null, error: e.message.replace(/^Error invoking remote method '[^']*': (Error: )?/, "") }));
    }, DEBOUNCE_MS);
    return () => clearTimeout(t);
    // `calls` is fixed for the life of the editor.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text]);

  const current = verdict.forText === text;
  const state: "checking" | "valid" | "invalid" | "unavailable" = !current ? "checking" : verdict.error ? "unavailable" : verdict.result?.ok ? "valid" : "invalid";
  const result = current ? verdict.result : null;
  const canSave = !readOnly && state === "valid" && !saving && (id !== null || root.trim().length > 0);

  const save = async () => {
    if (!canSave) return;
    setSaving(true);
    setSaveError(null);
    // A retry of the same request keeps its command id (so it acts once); a different request has a new one.
    const key = `${id ?? ""}\u0000${root}\u0000${text}`;
    if (cmd.current.key !== key) cmd.current = { key, id: newCommandId() };
    try {
      const created = id === null;
      const view = created ? await calls.create(text, root.trim(), cmd.current.id) : await calls.update(id!, text, cmd.current.id);
      setId(view.automationId);
      setSaved({ version: view.currentVersion, created });
      onSaved(view, created);
    } catch (e) {
      setSaveError(refusalSentence((e as Error).message));
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="autos-editor" aria-label={id ? "Edit automation" : "New automation"} data-testid="automation-editor" data-editing={id ?? "new"}>
      <div className="autos-editor-grid">
        <div className="autos-editor-left">
          {id === null ? (
            <label className="autos-field">
              <span>Workspace folder the runs act in</span>
              <input value={root} onChange={(e) => setRoot(e.target.value)} placeholder="/path/to/a/trusted/repository" spellCheck={false} data-testid="automation-workspace" />
              <span className="meta">It must be a folder you have trusted. The Core checks this when you enable the definition.</span>
            </label>
          ) : (
            <p className="meta" data-testid="automation-workspace-fixed">
              Workspace: <code>{root}</code>
              {target.version ? ` · editing version ${target.version}` : ""}
            </p>
          )}
          {readOnly && (
            <p className="autos-note" data-testid="editor-readonly" role="note">
              {target.readOnlyReason}
            </p>
          )}
          <label className="autos-field">
            <span>Definition (JSON)</span>
            <textarea
              className="autos-json"
              value={text}
              onChange={(e) => setText(e.target.value)}
              readOnly={readOnly}
              spellCheck={false}
              aria-describedby={statusId}
              aria-invalid={state === "invalid"}
              data-testid="automation-definition"
              rows={18}
            />
          </label>
          <div className="autos-actions">
            <Button variant="primary" disabled={!canSave} onClick={() => void save()} data-testid="automation-save">
              {id === null ? "Save definition" : "Save as a new version"}
            </Button>
            <Button onClick={onClose} data-testid="automation-editor-close">
              {saved ? "Close" : "Cancel"}
            </Button>
            {id !== null && <span className="meta">A change makes a new version. A new version has to be approved again before any trigger can start it.</span>}
          </div>
          {saveError && (
            <p className="autos-note" role="alert" data-testid="editor-save-error">
              {saveError}
            </p>
          )}
          {saved && (
            <p className="autos-note" role="status" data-testid="editor-saved" data-version={saved.version}>
              {saved.created ? `Saved as version ${saved.version}. It is not enabled: approve it to let a trigger start it.` : `Saved as version ${saved.version}. This version is not approved yet: approve it again before it can run.`}
            </p>
          )}
        </div>
        <div className="autos-editor-right" id={statusId} data-testid="validation" data-state={state}>
          <h3 className="autos-h3">What the Core says</h3>
          <p className="autos-verdict" role="status" aria-live="polite" data-testid="validation-status" data-state={state}>
            {state === "checking" && <><StatusDot status="running" label="Checking" showLabel /> asking the Core…</>}
            {state === "valid" && <><StatusDot status="ok" label="Valid" showLabel /> the Core accepts this definition.</>}
            {state === "invalid" && <><StatusDot status="danger" label="Not valid" showLabel /> {result?.issues.length ?? 0} {(result?.issues.length ?? 0) === 1 ? "issue" : "issues"}. It cannot be saved until the Core accepts it.</>}
            {state === "unavailable" && <><StatusDot status="warn" label="Unavailable" showLabel /> the Core could not check this: {verdict.error}</>}
          </p>
          {result && result.issues.length > 0 && (
            <ul className="autos-issues" aria-label="Issues" data-testid="validation-issues">
              {result.issues.map((i, k) => (
                <li key={`${i.path}:${i.code}:${k}`} data-testid="validation-issue" data-code={i.code} data-path={i.path}>
                  <code className="autos-issue-path">{i.path || "(document)"}</code> <strong className="autos-issue-code">{i.code}</strong>
                  <span>{i.message}</span>
                </li>
              ))}
            </ul>
          )}
          {result?.ok && (
            <dl className="autos-facts" data-testid="validation-facts">
              <dt>Name</dt>
              <dd data-testid="validation-name">{result.name}</dd>
              <dt>Canonical hash</dt>
              <dd>
                <code className="autos-hash" data-testid="validation-hash">
                  {result.definitionHash}
                </code>
              </dd>
              <dt>Reach</dt>
              <dd data-testid="validation-effects" data-effects={result.effects}>
                {effectWords(result.effects)}
              </dd>
              {result.needsListedApproval && (
                <>
                  <dt>Approval will list</dt>
                  <dd data-testid="validation-lists">
                    capabilities: {result.capabilities.join(", ") || "none"}; paths: {result.paths.join(", ") || "none"}; hosts: {result.hosts.join(", ") || "none"}
                  </dd>
                </>
              )}
              <dt>Triggers</dt>
              <dd>
                <ul className="autos-triggers" data-testid="validation-triggers">
                  {result.triggers.map((t) => (
                    <li key={t.id} data-testid="validation-trigger" data-kind={t.kind} data-next-due={t.nextDueMs}>
                      <strong>{t.id}</strong> · {t.summary}
                      {t.kind === "schedule" && <span className="meta"> · {nextDueWords(t, nowMs)}</span>}
                    </li>
                  ))}
                </ul>
              </dd>
            </dl>
          )}
        </div>
      </div>
    </section>
  );
}
