/**
 * The composer (REQ-PX-054, -055, -056; docs/65 AFW-D01..D14, AFW-E01..E09,
 * AFW-H01, AFW-H02): a rounded container at the bottom of the conversation
 * with an add-context button, the mode chip, the model chip, send (and a stop
 * square while a turn runs), the typed @ and / menus, attachments through the
 * Core's ingest, input history and a local draft, and the trays that dock
 * above it (queue, education, stopped, policy-blocked, background terminals,
 * side question).
 *
 * The renderer holds no authority (docs/81). The mode shown is the Core's
 * field; one the Core has not acknowledged is drawn "unconfirmed". The queue,
 * the interrupt, the send behaviour, the preference and its refusal, the
 * skill inventory and the attachment ingest are typed preload calls whose
 * answers are shown as they come. A draft is a local convenience: it is never
 * sent without the person pressing send.
 */
import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Button, IconButton, Menu, useLayer, type MenuItem } from "@modbit/ui";
import type { AttachmentResult, TaskModeId } from "../../shared/composer-types.ts";
import { MAX_ATTACHMENTS_PER_MESSAGE, MAX_ATTACHMENT_BYTES } from "../../shared/attachments.ts";
import type { TaskCard } from "../model.ts";
import {
  applyTrigger,
  blockedTray,
  coreError,
  educationDue,
  enterAction,
  filePaths,
  HISTORY_START,
  isModelRefusal,
  isPolicyRefusal,
  liveMentions,
  loadStore,
  MODE_CYCLE,
  MODE_DEFS,
  mentionText,
  modeStatus,
  nextMode,
  placeholderFor,
  planSend,
  queueRows,
  reorderTarget,
  runningTerminals,
  saveStore,
  suggestionsFor,
  triggerAt,
  walkHistory,
  withDraft,
  withHistory,
  type ComposerStore,
  type HistoryCursor,
  type Mention,
  type Trigger,
} from "./model.ts";
import { useCatalog, useComposerCore, useSlashInventory } from "./use-composer-core.ts";
import { MentionMenu, SlashMenuView, useMentionSources, useSlashItems, type MentionOption } from "./menus.tsx";
import { ModelPicker, type PickerChoice } from "./model-picker.tsx";
import { EducationTray, PolicyTray, QueueTray, SendBehaviorPanel, SideQuestionPanel, StoppedTray, TerminalsChip, TerminalsTray } from "./trays.tsx";

export interface ComposerProps {
  taskId: string;
  sessionId: string | null;
  connected: boolean;
  card: TaskCard | undefined;
  /** An approval is pending on the task: a resume would not help. */
  approvalPending: boolean;
  /** The text of the conversation's latest user message (the one a Stop interrupts). */
  lastUserText: string;
  hasMessages: boolean;
  onResume: (taskId: string) => void;
  onNewTask: () => void;
  /** Open a terminal in the apps panel's Terminal tab. */
  onOpenTerminal?: ((taskId: string, terminalId: string) => void) | undefined;
}

const newId = (): string => `c${Date.now().toString(36)}${Math.random().toString(36).slice(2, 10)}`;
const isMac = (): boolean => typeof navigator !== "undefined" && /mac/i.test(navigator.platform);
const storage = (): Storage | null => {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
};

interface AttachedChip extends AttachmentResult {
  key: string;
}
interface PolicyState {
  model: string;
  code: string;
  reason: string;
}

export function Composer(props: ComposerProps) {
  const { taskId, sessionId, connected, card, approvalPending, lastUserText, hasMessages, onResume, onNewTask, onOpenTerminal } = props;
  const core = useComposerCore(taskId, connected);
  const [store, setStore] = useState<ComposerStore>(() => loadStore(storage()));
  const initial = store.drafts[taskId];
  const [text, setText] = useState(initial?.text ?? "");
  const [mentions, setMentions] = useState<Mention[]>(initial?.mentions ?? []);
  const [skill, setSkill] = useState<string | null>(initial?.skill ?? null);
  const [caret, setCaret] = useState(0);
  const [attached, setAttached] = useState<AttachedChip[]>([]);
  const [attachNotes, setAttachNotes] = useState<string[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [requestedMode, setRequestedMode] = useState<TaskModeId | null>(null);
  const [modeNote, setModeNote] = useState("");
  const [dismissed, setDismissed] = useState<string | null>(null);
  const [activeIdx, setActiveIdx] = useState(0);
  const [history, setHistory] = useState<HistoryCursor>(HISTORY_START);
  const [stopped, setStopped] = useState<{ text: string } | null>(null);
  const [policy, setPolicy] = useState<PolicyState | null>(null);
  const [policyNote, setPolicyNote] = useState("");
  const [education, setEducation] = useState(false);
  const [behaviorOpen, setBehaviorOpen] = useState(false);
  const [sideOpen, setSideOpen] = useState(false);
  const [terminalsOpen, setTerminalsOpen] = useState(false);
  const [killing, setKilling] = useState<string | null>(null);
  const [pickerTick, setPickerTick] = useState(0);
  const [pickerWanted, setPickerWanted] = useState(false);
  const [announce, setAnnounce] = useState("");
  const area = useRef<HTMLTextAreaElement>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  // One id per message text: a retry of the same send replays in the Core; edited text is a different message.
  const inputIdRef = useRef<{ id: string; text: string }>({ id: newId(), text: "" });
  const sending = useRef(false);
  const lastSent = useRef<string>("");
  const lastSentId = useRef<string>("");
  const modeChain = useRef<Promise<void>>(Promise.resolve());
  const listId = useId();

  // ---- the Core's facts
  const posture = core.posture;
  const queue = core.queue;
  const state = card?.state ?? "";
  const running = state === "Running" || queue?.runAlive === true;
  const ended = state === "Completed" || state === "Failed" || state === "Cancelled";
  const startable = !ended && !running && (state === "Created" || state === "Queued" || (state === "Waiting" && card?.waitReason !== "Capacity" && !(card?.waitReason === "Approval" && approvalPending)));
  const coreMode: TaskModeId = posture?.mode ?? "AGENT";
  const shownMode: TaskModeId = requestedMode ?? coreMode;
  const status = modeStatus(requestedMode, posture);
  const rows = useMemo(() => queueRows(queue?.items ?? []), [queue]);
  const running_terminals = useMemo(() => runningTerminals(core.terminals), [core.terminals]);
  const catalogState = useCatalog(taskId, pickerWanted, `${posture?.preference.offset ?? ""}:${pickerTick}`);

  // ---- the draft is a local convenience that survives navigation and a reload
  const persist = useCallback((next: ComposerStore) => {
    setStore(next);
    saveStore(storage(), next);
  }, []);
  const storeRef = useRef(store);
  storeRef.current = store;
  useEffect(() => {
    persist(withDraft(storeRef.current, taskId, { text, mentions, skill }));
  }, [text, mentions, skill, taskId, persist]);

  // ---- the menus
  const trigger: Trigger | null = useMemo(() => triggerAt(text, caret), [text, caret]);
  const triggerKey = trigger ? `${trigger.kind}:${trigger.start}` : null;
  const menuOpen = trigger !== null && dismissed !== triggerKey;
  const mentionOpen = menuOpen && trigger!.kind === "mention";
  const slashOpen = menuOpen && trigger!.kind === "slash";
  const mentionSources = useMentionSources({ taskId, sessionId, query: mentionOpen ? trigger!.query : "", active: mentionOpen, hasWorkspace: !!card, hasChanges: state !== "" && state !== "Created" && state !== "Queued" });
  const slashInv = useSlashInventory(taskId, slashOpen);
  const slash = useSlashItems(slashInv, slashOpen ? trigger!.query : "", !startable);
  const optionCount = mentionOpen ? mentionSources.options.length : slashOpen ? slash.items.length : 0;
  useEffect(() => setActiveIdx(0), [triggerKey, trigger?.query]);
  useEffect(() => setActiveIdx((a) => Math.min(a, Math.max(0, optionCount - 1))), [optionCount]);
  useLayer(menuOpen, "composer-menu", false, () => setDismissed(triggerKey));

  // ---- helpers
  const refreshSoon = core.refreshNow;
  // The caret is placed once the new text is in the box and before the next keystroke can land (a frame later would put it back where it was).
  const pendingCaret = useRef<number | null>(null);
  const setTextAndCaret = (next: string, at: number) => {
    pendingCaret.current = at;
    setText(next);
    setCaret(at);
  };
  useLayoutEffect(() => {
    const at = pendingCaret.current;
    const ta = area.current;
    if (at === null || !ta || ta.value.length < at) return;
    pendingCaret.current = null;
    ta.focus();
    ta.setSelectionRange(at, at);
  });
  const live = useMemo(() => liveMentions(text, mentions), [text, mentions]);

  const chooseMention = (o: MentionOption) => {
    if (!trigger || o.disabledReason) return;
    if (o.descend) {
      const r = applyTrigger(text, trigger, `@${o.descend}`);
      setTextAndCaret(r.text, r.caret);
      return;
    }
    if (!o.mention) return;
    const r = applyTrigger(text, trigger, `${mentionText(o.mention)} `);
    setMentions((m) => (m.some((x) => x.value === o.mention!.value) ? m : [...m, o.mention!]));
    setTextAndCaret(r.text, r.caret);
  };
  const chooseSlash = (it: { entry: { id: string; kind: string }; disabledReason: string | null }) => {
    if (!trigger || it.disabledReason || it.entry.kind !== "SKILL") return;
    setSkill(it.entry.id);
    const r = applyTrigger(text, trigger, "");
    setTextAndCaret(r.text, r.caret);
  };

  // ---- modes: a request until the Core acknowledges it
  const applyMode = (target: TaskModeId) => {
    if (!sessionId) return;
    setRequestedMode(target);
    setModeNote("");
    modeChain.current = modeChain.current.then(async () => {
      try {
        await window.modbit.composerSetMode(sessionId, taskId, target);
        setAnnounce(`${MODE_DEFS[target].label} mode`);
      } catch (e) {
        setModeNote(`The Core did not accept the ${MODE_DEFS[target].label} mode: ${coreError(e).message}`);
      } finally {
        const p = await window.modbit.composerPosture(taskId).catch(() => null);
        if (p) core.setPosture(p);
        setRequestedMode((r) => (r === target ? null : r));
        refreshSoon();
      }
    });
  };

  // ---- sending
  const clearAfterSend = (sent: string) => {
    lastSent.current = sent;
    lastSentId.current = inputIdRef.current.id;
    persist(withDraft(withHistory(storeRef.current, taskId, sent), taskId, null));
    setText("");
    setMentions([]);
    setSkill(null);
    setAttached([]);
    setAttachNotes([]);
    setHistory(HISTORY_START);
    setCaret(0);
    setStopped(null);
    inputIdRef.current = { id: newId(), text: "" };
  };
  const send = async (alternate: boolean) => {
    const body = text.trim();
    if (!sessionId || !body || sending.current || ended) return;
    if (policy) {
      // The tray is up: Enter says why and keeps every word (AFW-H02).
      setError(`${policy.model} is not available: ${policy.reason}`);
      return;
    }
    const plan = planSend({ running, startable, behavior: core.behavior }, alternate);
    sending.current = true;
    setBusy(true);
    setError("");
    try {
      const paths = filePaths(live);
      if (paths.length > 0) await window.modbit.setTaskSelection(sessionId, taskId, { paths, reviewHunks: [], source: "desktop" });
      if (inputIdRef.current.text !== body) inputIdRef.current = { id: newId(), text: body };
      await window.modbit.composerQueueInput(sessionId, taskId, body, plan.mode, inputIdRef.current.id);
      if (plan.start) await window.modbit.startTask(sessionId, taskId, skill ? { skills: [skill] } : {});
      clearAfterSend(body);
      if (educationDue(storeRef.current, plan.queues)) setEducation(true);
      refreshSoon();
    } catch (e) {
      const c = coreError(e);
      if (isPolicyRefusal(c.code)) setPolicy({ model: posture?.preference.pinModel || "The selected model", code: c.code, reason: c.message });
      else setError(c.code ? `${c.message} (${c.code.toLowerCase().replace(/_/g, " ")})` : c.message);
    } finally {
      sending.current = false;
      setBusy(false);
    }
  };

  const stop = async () => {
    if (!sessionId || busy) return;
    setBusy(true);
    setError("");
    try {
      await window.modbit.composerInterrupt(sessionId, taskId, `stop-${taskId.slice(0, 8)}-${Date.now().toString(36)}`);
      setStopped({ text: lastUserText });
      refreshSoon();
    } catch (e) {
      setError(coreError(e).message);
    } finally {
      setBusy(false);
    }
  };

  // ---- the model chip and picker
  const choose = async (c: PickerChoice) => {
    if (!sessionId) return;
    setError("");
    try {
      if (c.kind === "objective") {
        await window.modbit.composerSetPreference(sessionId, taskId, { objective: c.objective!, ...(posture?.preference.pinModel ? { clearPin: true } : {}) });
        setPolicy(null);
      } else if (c.row) {
        const r = c.row;
        await window.modbit.composerSetPreference(sessionId, taskId, { pin: { endpoint: r.endpoint, model: r.model }, ...(r.variant.effort ? { effort: r.variant.effort } : {}), ...(r.variant.serviceTier ? { serviceTier: r.variant.serviceTier } : {}) });
        setPolicy(null);
      }
      setPickerTick((t) => t + 1);
      refreshSoon();
    } catch (e) {
      const err = coreError(e);
      if (c.row && isPolicyRefusal(err.code)) {
        const t = blockedTray(c.row.name, { code: err.code, reason: err.message });
        setPolicy({ model: c.row.name, code: t.code, reason: t.body });
      } else setError(err.message);
    }
  };
  const switchToAuto = async () => {
    if (!sessionId) return;
    try {
      await window.modbit.composerSetPreference(sessionId, taskId, { clearPin: true, objective: "BALANCE" });
      setPolicy(null);
      setPolicyNote("");
      setPickerTick((t) => t + 1);
      refreshSoon();
    } catch (e) {
      setPolicyNote(coreError(e).message);
    }
  };

  // ---- attachments (ingested by the Core when added; they stay with the task)
  const attach = async (files: File[]) => {
    if (!sessionId || files.length === 0) return;
    const notes: string[] = [];
    for (const f of files) {
      if (attached.length + notes.length >= MAX_ATTACHMENTS_PER_MESSAGE) {
        notes.push(`At most ${MAX_ATTACHMENTS_PER_MESSAGE} attachments per message.`);
        break;
      }
      if (f.size > MAX_ATTACHMENT_BYTES) {
        notes.push(`${f.name}: the file is larger than 3 MB, the most one attachment may be.`);
        continue;
      }
      try {
        const r = await window.modbit.attachFile(sessionId, taskId, f);
        setAttached((a) => (a.some((x) => x.attachmentId === r.attachmentId) ? a : [...a, { ...r, key: newId() }]));
      } catch (e) {
        const c = coreError(e);
        notes.push(`${f.name}: ${c.message}`);
      }
    }
    setAttachNotes(notes);
    refreshSoon();
  };

  // ---- keys
  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (menuOpen && optionCount > 0) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        setActiveIdx((a) => (a + (e.key === "ArrowDown" ? 1 : optionCount - 1)) % optionCount);
        return;
      }
      if ((e.key === "Enter" && !e.shiftKey) || e.key === "Tab") {
        if (e.key === "Tab" && e.shiftKey) {
          // fall through to the mode cycle below
        } else {
          e.preventDefault();
          if (mentionOpen) {
            const o = mentionSources.options[activeIdx];
            if (o) chooseMention(o);
          } else {
            const it = slash.items[activeIdx];
            if (it) chooseSlash(it);
          }
          return;
        }
      }
    }
    if (e.key === "Tab" && e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      applyMode(nextMode(shownMode));
      return;
    }
    // Alt+Up and Alt+Down always walk the history. Plain Up does too in an empty box or while a recalled prompt is showing, and plain Down while one is: a draft being written keeps its arrows.
    const ta = e.currentTarget;
    const onFirstLine = !ta.value.slice(0, ta.selectionStart).includes("\n");
    const onLastLine = !ta.value.slice(ta.selectionEnd).includes("\n");
    const plainEdge = !e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey && !menuOpen && ((e.key === "ArrowUp" && onFirstLine && (ta.value === "" || history.index !== -1)) || (e.key === "ArrowDown" && onLastLine && history.index !== -1));
    if ((e.altKey && !e.ctrlKey && !e.metaKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) || plainEdge) {
      e.preventDefault();
      e.stopPropagation();
      const list = store.history[taskId] ?? [];
      const r = walkHistory(list, history, e.key === "ArrowUp" ? "older" : "newer", text);
      if (r) {
        setHistory(r.cursor);
        setTextAndCaret(r.text, r.text.length);
      }
      return;
    }
    if (e.key === "Escape" && !e.defaultPrevented) {
      // Nothing of the composer's is open: leave the message box for the conversation, so the keyboard is never trapped.
      e.preventDefault();
      e.nativeEvent.stopPropagation();
      document.querySelector<HTMLElement>('[data-testid="conv-scroll"]')?.focus();
      return;
    }
    const act = enterAction(e.nativeEvent, isMac());
    if (act === "send" || act === "send-alternate") {
      e.preventDefault();
      void send(act === "send-alternate");
    }
  };

  const hasText = text.trim().length > 0;
  const placeholder = placeholderFor({ mode: shownMode, state, running, hasMessages });
  const tint = shownMode === "AGENT" ? undefined : shownMode.toLowerCase();

  // A model-refusal at run time (the policy blocks the model the task runs) is a failed turn: the words are restored and the tray names the cause.
  const diag = card?.diagnostic;
  useEffect(() => {
    if (!diag || !isModelRefusal(diag.code) || policy) return;
    if (lastSent.current && text === "") {
      setText(lastSent.current);
      // The failed turn never took the message: it is back in the box, so it is not also left waiting in the queue.
      if (sessionId && queue?.items.some((i) => i.inputId === lastSentId.current && i.state === "QUEUED")) void window.modbit.composerRemoveQueued(sessionId, taskId, lastSentId.current).then(refreshSoon, refreshSoon);
      lastSent.current = "";
    }
    setPolicy({ model: posture?.preference.pinModel || "The model this task runs", code: diag.code, reason: diag.detail || diag.userAction });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [diag?.code, diag?.detail]);

  const addItems: MenuItem[] = [
    { id: "attach", label: "Attach a file…", onSelect: () => fileInput.current?.click() },
    { id: "mention-file", label: "Mention a file or folder", onSelect: () => insertAt("@") },
    { id: "mention-terminal", label: "Mention a terminal", onSelect: () => insertAt("@terminal") },
    { id: "mention-conversation", label: "Mention a past conversation", onSelect: () => insertAt("@") },
    { id: "mention-changes", label: "Mention the changes on this branch", onSelect: () => insertAt("@changes") },
    { id: "side", label: "Ask a side question", separatorBefore: true, onSelect: () => setSideOpen(true) },
    { id: "behavior", label: "Send behavior…", onSelect: () => setBehaviorOpen(true) },
    ...MODE_CYCLE.map((m, i): MenuItem => ({ id: `mode-${m.toLowerCase()}`, label: `${MODE_DEFS[m].label} mode`, radio: true, checked: shownMode === m, separatorBefore: i === 0, onSelect: () => applyMode(m) })),
  ];
  function insertAt(s: string) {
    const ta = area.current;
    const at = ta?.selectionStart ?? text.length;
    const before = text.slice(0, at);
    const lead = before.length > 0 && !/\s$/.test(before) ? " " : "";
    const next = before + lead + s + text.slice(at);
    setTextAndCaret(next, (before + lead + s).length);
    setDismissed(null);
  }
  const modeItems: MenuItem[] = MODE_CYCLE.map((m) => ({ id: m.toLowerCase(), label: MODE_DEFS[m].label, radio: true, checked: shownMode === m, onSelect: () => applyMode(m) }));

  if (ended) {
    return (
      <section className="conv-composer cmp" aria-label="Composer" data-testid="composer-region" data-state="ended">
        <p className="meta" data-testid="composer-ended">
          This task has {state === "Completed" ? "completed" : state === "Failed" ? "failed" : "been cancelled"}, so it takes no more messages.
        </p>
        <Button size="sm" onClick={onNewTask} data-testid="conv-new-task">
          New task
        </Button>
      </section>
    );
  }

  return (
    <section
      className="conv-composer cmp"
      aria-label="Composer"
      data-testid="composer-region"
      data-mode={shownMode}
      data-running={running}
      onDragOver={(e) => {
        if (e.dataTransfer.types.includes("Files")) e.preventDefault();
      }}
      onDrop={(e) => {
        if (e.dataTransfer.files.length > 0) {
          e.preventDefault();
          void attach([...e.dataTransfer.files]);
        }
      }}
    >
      <div className="cmp-dock" data-testid="composer-dock">
        {policy && <PolicyTray model={policy.model} code={policy.code} reason={policy.reason} note={policyNote} onAuto={() => void switchToAuto()} onCopy={() => void navigator.clipboard.writeText(policy.reason).then(() => setPolicyNote("Copied.")).catch(() => setPolicyNote("Copying is unavailable here."))} onDismiss={() => setPolicy(null)} />}
        {!policy && stopped && <StoppedTray text={stopped.text} canContinue={startable} onEdit={() => { setTextAndCaret(stopped.text, stopped.text.length); setStopped(null); }} onContinue={() => { setStopped(null); onResume(taskId); }} onDismiss={() => setStopped(null)} />}
        {!policy && !stopped && education && <EducationTray onKeep={() => { persist({ ...storeRef.current, educationAck: true }); setEducation(false); }} onSettings={() => { persist({ ...storeRef.current, educationAck: true }); setEducation(false); setBehaviorOpen(true); }} />}
        {behaviorOpen && core.behavior && (
          <SendBehaviorPanel
            behavior={core.behavior}
            onClose={() => setBehaviorOpen(false)}
            onSet={async (w, n) => {
              if (!sessionId) return;
              try {
                core.setBehavior(await window.modbit.composerSetSendBehavior(sessionId, taskId, w, n));
              } catch (e) {
                setError(coreError(e).message);
              }
            }}
          />
        )}
        {sideOpen && (
          <SideQuestionPanel
            onClose={() => setSideOpen(false)}
            onAsk={async (q) => {
              try {
                const a = await window.modbit.composerSideQuestion(taskId, q);
                return { text: a.text, tokens: a.outputTokens };
              } catch (e) {
                return coreError(e).message;
              }
            }}
            onAdd={(t) => {
              setTextAndCaret(text ? `${text}\n${t}` : t, (text ? text.length + 1 : 0) + t.length);
              setSideOpen(false);
            }}
          />
        )}
        <QueueTray
          rows={rows}
          behavior={core.behavior}
          runAlive={queue?.runAlive ?? false}
          busy={busy}
          onLeave={() => area.current?.focus()}
          onSendNow={(id) => {
            if (!sessionId) return;
            setBusy(true);
            window.modbit
              .composerSendNow(sessionId, taskId, id, `now-${id}`)
              .catch((e) => setError(coreError(e).message))
              .finally(() => {
                setBusy(false);
                refreshSoon();
              });
          }}
          onEdit={async (id, change) => {
            if (!sessionId) return false;
            try {
              await window.modbit.composerEditQueued(sessionId, taskId, id, change);
              refreshSoon();
              return true;
            } catch (e) {
              setError(coreError(e).message);
              return false;
            }
          }}
          onDelete={(id) => {
            if (!sessionId) return;
            window.modbit
              .composerRemoveQueued(sessionId, taskId, id)
              .catch((e) => setError(coreError(e).message))
              .finally(refreshSoon);
          }}
          onMove={(id, dir) => {
            if (!sessionId) return;
            const t = reorderTarget(rows, id, dir);
            if (!t) return;
            window.modbit
              .composerReorderQueued(sessionId, taskId, t.inputId, t.before)
              .catch((e) => setError(coreError(e).message))
              .finally(refreshSoon);
          }}
          onClear={() => {
            if (!sessionId) return;
            void Promise.all(rows.map((r) => window.modbit.composerRemoveQueued(sessionId, taskId, r.inputId)))
              .catch((e) => setError(coreError(e).message))
              .finally(refreshSoon);
          }}
        />
        {terminalsOpen && running_terminals.length > 0 && (
          <TerminalsTray
            terminals={running_terminals}
            killing={killing}
            onOpen={(id) => onOpenTerminal?.(taskId, id)}
            onKill={async (id) => {
              if (!sessionId) return;
              setKilling(id);
              try {
                await window.modbit.terminalKill(sessionId, taskId, id);
              } catch (e) {
                setError(coreError(e).message);
              } finally {
                setKilling(null);
                refreshSoon();
              }
            }}
          />
        )}
      </div>

      <div className="cmp-chips">
        <TerminalsChip count={running_terminals.length} open={terminalsOpen} onToggle={() => setTerminalsOpen((o) => !o)} />
        {startable && (
          <Button size="sm" variant="primary" onClick={() => onResume(taskId)} data-testid="conv-start">
            {state === "Waiting" ? "Resume" : "Start"}
          </Button>
        )}
        <Button size="sm" onClick={onNewTask} data-testid="conv-new-task">
          New task
        </Button>
        {core.stale && (
          <span className="meta" role="status" data-testid="composer-stale">
            The Core is not answering; what is shown may be out of date.
          </span>
        )}
      </div>

      <div className="cmp-box" data-testid="composer" data-mode={shownMode} data-tint={tint} data-mode-status={status}>
        {(live.length > 0 || skill || attached.length > 0) && (
          <div className="cmp-attach-row" data-testid="composer-chips">
            {skill && (
              <span className="cmp-token" data-testid="skill-chip">
                /{skill}
                <button type="button" aria-label={`Remove skill ${skill}`} onClick={() => setSkill(null)}>
                  ×
                </button>
              </span>
            )}
            {live.map((m) => (
              <span key={m.value} className="cmp-token" data-testid="mention-chip" data-kind={m.kind}>
                {m.kind === "file" ? "file" : m.kind === "folder" ? "folder" : m.kind === "terminal" ? "terminal" : m.kind === "conversation" ? "conversation" : "branch"}: {m.label}
              </span>
            ))}
            {attached.map((a) => (
              <span key={a.key} className="cmp-token cmp-attachment" data-testid="attachment-chip" data-kind={a.kind} data-mime={a.mime}>
                {a.thumbnail ? <img src={a.thumbnail} alt={`Thumbnail of ${a.name}`} width={20} height={20} data-testid="attachment-thumb" /> : null}
                {a.name}
                <span className="meta"> · stays with the task</span>
              </span>
            ))}
          </div>
        )}
        {attachNotes.length > 0 && (
          <div role="alert" data-testid="attach-notes">
            <ul className="cmp-notes">
              {attachNotes.map((n) => (
                <li key={n}>{n}</li>
              ))}
            </ul>
          </div>
        )}
        <div className="cmp-row">
          <Menu label="Add to the message" placement="up" triggerTestId="add-context" triggerClassName="cmp-plus" trigger={<span aria-hidden="true">+</span>} items={addItems} />
          {shownMode !== "AGENT" && (
            <span className="cmp-mode" data-testid="mode-chip" data-mode={shownMode}>
              <Menu label={`Mode: ${MODE_DEFS[shownMode].label}. Choose another`} placement="up" triggerTestId="mode-chip-button" triggerClassName="cmp-chip cmp-mode-btn" trigger={<><span aria-hidden="true">{MODE_DEFS[shownMode].icon}</span> <span data-testid="mode-label">{MODE_DEFS[shownMode].label}</span>{status === "unconfirmed" && <span className="cmp-dim" data-testid="mode-unconfirmed"> unconfirmed</span>}</>} items={modeItems} />
              <IconButton label={`Leave ${MODE_DEFS[shownMode].label} mode`} icon={<span>×</span>} size="sm" onClick={() => applyMode("AGENT")} data-testid="mode-clear" />
            </span>
          )}
          <div className="cmp-input">
            <textarea
              ref={area}
              className="cmp-textarea"
              data-testid="composer-input"
              aria-label="Message"
              aria-autocomplete={menuOpen ? "list" : undefined}
              aria-controls={menuOpen && optionCount > 0 ? listId : undefined}
              aria-activedescendant={menuOpen && optionCount > 0 ? `${listId}-o${activeIdx}` : undefined}
              placeholder={placeholder}
              rows={Math.min(8, Math.max(1, text.split("\n").length))}
              maxLength={20_000}
              value={text}
              disabled={!sessionId || !connected}
              onChange={(e) => {
                setText(e.target.value);
                setCaret(e.target.selectionStart);
                setHistory(HISTORY_START);
                if (error) setError("");
              }}
              onSelect={(e) => setCaret(e.currentTarget.selectionStart)}
              onKeyUp={(e) => setCaret(e.currentTarget.selectionStart)}
              onClick={(e) => setCaret(e.currentTarget.selectionStart)}
              onKeyDown={onKeyDown}
              onPaste={(e) => {
                const files = [...e.clipboardData.files];
                if (files.length > 0) {
                  e.preventDefault();
                  void attach(files);
                }
              }}
            />
            {mentionOpen && <MentionMenu id={listId} options={mentionSources.options} loading={mentionSources.loading} active={activeIdx} onActive={setActiveIdx} onChoose={chooseMention} />}
            {slashOpen && <SlashMenuView id={listId} items={slash.items} dividerAt={slash.dividerAt} active={activeIdx} onActive={setActiveIdx} onChoose={chooseSlash} loaded={slashInv !== null} />}
          </div>
          <ModelPicker posture={posture} catalog={catalogState.catalog} catalogError={catalogState.error} disabled={!sessionId} onOpen={() => setPickerWanted(true)} onChoose={choose} />
          {running && (
            <button type="button" className="cmp-stop" aria-label="Stop the turn" title="Stop: ends the turn at a safe point" onClick={() => void stop()} disabled={busy} data-testid="composer-stop">
              <span aria-hidden="true" />
            </button>
          )}
          <Button variant="primary" className="cmp-send" disabled={!hasText || busy || !sessionId || !connected} onClick={() => void send(false)} data-testid="composer-send" aria-label={running ? "Queue the message" : "Send"} data-tint={shownMode === "ASK" && hasText ? "ask" : undefined}>
            {running ? "Queue" : "Send"}
          </Button>
        </div>
        {error && (
          <p className="cmp-reason" role="alert" data-testid="composer-error">
            {error}
          </p>
        )}
        {modeNote && (
          <p className="cmp-reason" role="alert" data-testid="mode-note">
            {modeNote}
          </p>
        )}
      </div>
      {!hasText && !running && (
        <div className="cmp-suggest" data-testid="composer-suggestions">
          {suggestionsFor(shownMode).map((s) => (
            <button key={s.mode} type="button" className="cmp-chip" onClick={() => applyMode(s.mode)} data-testid={`suggest-${s.mode.toLowerCase()}`}>
              {s.label}
              <span className="meta"> Shift+Tab</span>
            </button>
          ))}
        </div>
      )}
      <input ref={fileInput} type="file" multiple hidden tabIndex={-1} aria-hidden="true" data-testid="attach-input" onChange={(e) => { const files = [...(e.target.files ?? [])]; e.target.value = ""; void attach(files); }} />
      <p className="mb-sr-only" role="status" aria-live="polite" data-testid="composer-live">
        {announce}
      </p>
    </section>
  );
}
