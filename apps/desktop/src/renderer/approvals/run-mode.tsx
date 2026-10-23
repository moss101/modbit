/**
 * The run-mode chooser and the allowlist manager (REQ-PX-057; docs/65
 * AFW-F06..F10), on the Core's existing commands. A run mode is who approves
 * inside the capability envelope; it never widens the envelope. The kinds that
 * always ask are listed in plain words, a move to a mode that approves more
 * opens a warning dialog that must be acknowledged, and the widest preset says
 * it lasts for this session only. Rules are made and revoked through the Core;
 * nothing is stored here. There is no classifier that approves on its own
 * judgement (docs/65 AFW-F06).
 */
import { useCallback, useEffect, useId, useState } from "react";
import { Badge, Button, Dialog } from "@modbit/ui";
import type { AddRuleInput, AllowRuleInfo, RunModeInfo } from "../../shared/control-types.ts";
import { ASK_CLASS_WORDS } from "./model.ts";
import { EXPIRY_CHOICES, MODE_COPY, SCOPE_WORDS, approvesMore, expiryWords, modeLabel, orderRules, patternFromText, ruleRefusal } from "./run-mode-model.ts";

export interface RunModePanelProps {
  open: boolean;
  onClose: () => void;
  info: RunModeInfo | null;
  rules: readonly AllowRuleInfo[];
  /** Records the mode (the Core refuses a move that approves more without the acknowledgement). Rejects with the Core's message. */
  onSetMode: (mode: string, acknowledged: boolean) => Promise<void>;
  onAddRule: (rule: AddRuleInput) => Promise<void>;
  onRevoke: (ruleId: string) => Promise<void>;
  now?: number;
}

const MODES_FALLBACK = ["ASK", "ALLOWLIST", "ALLOWLIST_SANDBOX", "RUN_EVERYTHING"];

export function RunModePanel({ open, onClose, info, rules, onSetMode, onAddRule, onRevoke, now = Date.now() }: RunModePanelProps) {
  const modes = info && info.modes.length > 0 ? info.modes : MODES_FALLBACK;
  const [pending, setPending] = useState<string | null>(null);
  const [understood, setUnderstood] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [text, setText] = useState("");
  const [scope, setScope] = useState<AddRuleInput["scope"]>("REPO");
  const [expiry, setExpiry] = useState<(typeof EXPIRY_CHOICES)[number]["id"]>("never");
  const [coversAlwaysAsk, setCovers] = useState(false);
  const [showInactive, setShowInactive] = useState(false);
  const uid = useId();
  const current = info?.mode ?? "ASK";

  useEffect(() => {
    if (!open) {
      setPending(null);
      setUnderstood(false);
      setError(null);
    }
  }, [open]);

  const choose = (mode: string) => {
    if (mode === current || busy) return;
    setError(null);
    if (approvesMore(current, mode, modes)) {
      setUnderstood(false);
      setPending(mode);
      return;
    }
    void apply(mode, false);
  };
  const apply = async (mode: string, ack: boolean) => {
    setBusy(true);
    try {
      await onSetMode(mode, ack);
      setPending(null);
    } catch (e) {
      setError((e as Error).message.replace(/^Error invoking remote method '[^']*': (Error: )?/, ""));
    } finally {
      setBusy(false);
    }
  };
  const addRule = async () => {
    const pattern = patternFromText(text);
    if (pattern.length === 0) {
      setError("Type the command words the rule should cover, for example: git status");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const ms = EXPIRY_CHOICES.find((c) => c.id === expiry)?.ms ?? 0;
      await onAddRule({ pattern, scope, expiresAtMs: ms === 0 ? 0 : Date.now() + ms, coversAlwaysAsk });
      setText("");
      setCovers(false);
    } catch (e) {
      setError(ruleRefusal((e as Error).message));
    } finally {
      setBusy(false);
    }
  };
  const revoke = async (id: string) => {
    setBusy(true);
    setError(null);
    try {
      await onRevoke(id);
    } catch (e) {
      setError(ruleRefusal((e as Error).message));
    } finally {
      setBusy(false);
    }
  };
  const shown = orderRules(rules).filter((r) => showInactive || r.state === "ACTIVE");
  const widest = pending === "RUN_EVERYTHING";

  return (
    <>
      <Dialog open={open} onClose={onClose} title="Run mode and allowlist" size="lg" testId="runmode-dialog">
        <div className="rm">
          <p className="meta">A run mode decides who approves protected effects inside what the task is already allowed to touch. It never widens that.</p>
          <fieldset>
            <legend>Who approves</legend>
            {modes.map((m) => (
              <label key={m} className="rm-mode" data-current={m === current} data-testid={`runmode-option-${m}`}>
                <input type="radio" name={`${uid}-mode`} value={m} checked={m === current} disabled={busy} onChange={() => choose(m)} data-testid={`runmode-radio-${m}`} />
                <span>
                  <strong>{modeLabel(m)}</strong>
                  {m === current ? <> <Badge tone="info">in force</Badge></> : null}
                </span>
                <span className="meta">{MODE_COPY[m]?.summary ?? ""}</span>
              </label>
            ))}
          </fieldset>
          {info?.sessionOnly && (
            <p className="meta" data-testid="runmode-session-only">
              This mode lasts only until the Core restarts; after a restart the last durable mode is back.
            </p>
          )}
          <section aria-label="The kinds of effect that always ask">
            <h3>These always ask, in every mode</h3>
            <ul className="rm-classes" data-testid="runmode-always-ask">
              {(info && info.alwaysAsk.length > 0 ? info.alwaysAsk : Object.keys(ASK_CLASS_WORDS)).map((c) => (
                <li key={c}>
                  <strong>{ASK_CLASS_WORDS[c]?.label ?? c}</strong>: {ASK_CLASS_WORDS[c]?.detail ?? "asks every time"}
                </li>
              ))}
            </ul>
            <p className="meta">A rule can cover one of these only if you say so when you make it, and only for a task or a repository.</p>
          </section>
          <section aria-label="Allowlist rules">
            <h3>Allowlist rules</h3>
            <label className="meta">
              <input type="checkbox" checked={showInactive} onChange={(e) => setShowInactive(e.target.checked)} data-testid="rules-show-inactive" /> Show expired and revoked rules
            </label>
            {shown.length === 0 ? (
              <p className="meta" data-testid="rules-empty">
                No rules apply to this task. Allow a command prefix from an approval card, or make one below.
              </p>
            ) : (
              <ul className="rm-rules" data-testid="rules-list">
                {shown.map((r) => (
                  <li key={r.ruleId} className="rm-rule" data-testid="rule" data-rule-id={r.ruleId} data-state={r.state}>
                    <code data-testid="rule-pattern">{r.pattern.join(" ")}</code>
                    <Badge tone={r.state === "ACTIVE" ? "ok" : "neutral"}>{r.state.toLowerCase()}</Badge>
                    <span className="meta">
                      in {SCOPE_WORDS[r.scope] ?? r.scope.toLowerCase()} · {expiryWords(r.expiresAtMs, now)} · made by {r.createdBy || "you"}
                      {r.coversAlwaysAsk ? " · also covers always-ask effects" : ""}
                    </span>
                    <span className="rm-spacer" />
                    {r.state === "ACTIVE" && (
                      <Button size="sm" onClick={() => void revoke(r.ruleId)} disabled={busy} data-testid="rule-revoke" aria-label={`Revoke the rule ${r.pattern.join(" ")}`}>
                        Revoke
                      </Button>
                    )}
                  </li>
                ))}
              </ul>
            )}
            <form
              className="rm-form"
              onSubmit={(e) => {
                e.preventDefault();
                void addRule();
              }}
              aria-label="Make a rule"
            >
              <label>
                Command prefix (words, in order)
                <input value={text} onChange={(e) => setText(e.target.value)} placeholder="for example: cargo test" disabled={busy} data-testid="rule-pattern-input" />
              </label>
              <label>
                Applies to
                <select value={scope} onChange={(e) => setScope(e.target.value as AddRuleInput["scope"])} disabled={busy} data-testid="rule-scope">
                  <option value="REPO">this repository</option>
                  <option value="TASK">this task</option>
                  <option value="USER">all my tasks</option>
                </select>
              </label>
              <label>
                Lasts
                <select value={expiry} onChange={(e) => setExpiry(e.target.value as typeof expiry)} disabled={busy} data-testid="rule-expiry">
                  {EXPIRY_CHOICES.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.label}
                    </option>
                  ))}
                </select>
              </label>
              <label className="rm-check">
                <input type="checkbox" checked={coversAlwaysAsk} onChange={(e) => setCovers(e.target.checked)} disabled={busy || scope === "USER"} data-testid="rule-covers" />
                <span>Also cover effects that always ask (needs two or more words, and a task or repository scope)</span>
              </label>
              <div className="rm-actions">
                <Button type="submit" variant="primary" size="sm" disabled={busy} data-testid="rule-add">
                  Add rule
                </Button>
              </div>
            </form>
            <p className="meta">A rule is a literal word prefix. It never matches shell operators, redirections, substitutions or pipelines, and a compound command needs every part covered. Rules apply when the mode is Allowlist or higher.</p>
          </section>
          {error && (
            <p className="appr-note" role="alert" data-testid="runmode-error">
              {error}
            </p>
          )}
          <div className="rm-actions">
            <Button onClick={onClose} data-testid="runmode-close">
              Close
            </Button>
          </div>
        </div>
      </Dialog>
      <Dialog open={open && pending !== null} onClose={() => setPending(null)} title={widest ? "Run everything without asking?" : "Let more run without asking?"} role="alertdialog" size="md" testId="runmode-warning">
        <div className="ckpt-dialog">
          <p data-testid="runmode-warning-text">{info?.warning || "This lets the agent act without asking you first."}</p>
          {widest && (
            <p className="appr-warn" data-testid="runmode-warning-widest">
              This is the widest preset. It lasts for this session only and asks again every time you choose it. Effects that always ask still ask.
            </p>
          )}
          <label className="rm-check">
            <input type="checkbox" checked={understood} onChange={(e) => setUnderstood(e.target.checked)} data-testid="runmode-understood" />{" "}
            <span>I understand that a prompt injection or a data leak could run inside this mode without my seeing it.</span>
          </label>
          {error && (
            <p className="appr-note" role="alert" data-testid="runmode-warning-error">
              {error}
            </p>
          )}
          <div className="rm-actions">
            <Button onClick={() => setPending(null)} data-testid="runmode-warning-cancel">
              Cancel
            </Button>
            <Button variant="danger" disabled={!understood || busy} onClick={() => pending && void apply(pending, true)} data-testid="runmode-warning-confirm">
              {pending ? `Use ${modeLabel(pending)}` : "Use"}
            </Button>
          </div>
        </div>
      </Dialog>
    </>
  );
}

/** The conversation's run-mode control: a button that names the mode in force and opens the panel. */
export function RunModeControl({ sessionId, taskId, connected, open, onOpenChange }: { sessionId: string | null; taskId: string; connected: boolean; open: boolean; onOpenChange: (open: boolean) => void }) {
  const [info, setInfo] = useState<RunModeInfo | null>(null);
  const [rules, setRules] = useState<AllowRuleInfo[]>([]);
  const refresh = useCallback(async () => {
    try {
      const [i, r] = await Promise.all([window.modbit.runMode(taskId), window.modbit.allowRules(taskId, true)]);
      setInfo(i);
      setRules(r);
    } catch {
      // Unreachable Core: the last answer stays.
    }
  }, [taskId]);
  useEffect(() => {
    if (connected) void refresh();
  }, [connected, refresh, open]);
  useEffect(() => {
    const off = window.modbit.onEvent((raw) => {
      const e = raw as { taskId: string | null; eventType: string };
      if (e.taskId === taskId && (e.eventType === "RunModeSet" || e.eventType === "AllowRuleAdded" || e.eventType === "AllowRuleRevoked")) void refresh();
    });
    return off;
  }, [taskId, refresh]);
  const label = info ? modeLabel(info.mode) : "…";
  return (
    <>
      <Button size="sm" variant="ghost" className="rm-btn" onClick={() => onOpenChange(true)} data-testid="runmode-button" data-mode={info?.mode ?? ""} aria-haspopup="dialog">
        Run mode: <span data-testid="runmode-current">{label}</span>
      </Button>
      <RunModePanel
        open={open}
        onClose={() => onOpenChange(false)}
        info={info}
        rules={rules}
        onSetMode={async (mode, ack) => {
          if (!sessionId) throw new Error("no session");
          setInfo(await window.modbit.setRunMode(sessionId, taskId, mode, ack));
        }}
        onAddRule={async (rule) => {
          if (!sessionId) throw new Error("no session");
          await window.modbit.addAllowRule(sessionId, taskId, rule);
          await refresh();
        }}
        onRevoke={async (ruleId) => {
          if (!sessionId) throw new Error("no session");
          await window.modbit.revokeAllowRule(sessionId, taskId, ruleId, "revoked in the run-mode settings");
          await refresh();
        }}
      />
    </>
  );
}
