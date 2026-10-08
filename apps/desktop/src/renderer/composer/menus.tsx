/**
 * The composer's two typed menus (REQ-PX-054; docs/65 AFW-D07, AFW-D08): the
 * @ menu of sources the Core serves (workspace files and folders through the
 * typed Files reads, the task's terminals, past conversations through the
 * Core's search, the branch's changes) and the slash menu from `ListSkills`.
 * Both are listboxes the textarea drives with `aria-activedescendant`, so the
 * caret never leaves the message. Every label a skill author or a file name
 * supplies is rendered as text. A source the Core's path policy denies is
 * listed with its reason and cannot be chosen.
 */
import { useEffect, useMemo, useState } from "react";
import { Badge } from "@modbit/ui";
import type { SlashInventoryView } from "../../shared/composer-types.ts";
import type { Mention } from "./model.ts";
import { scopeLabel, slashMenu, type SlashItem } from "./model.ts";

export interface MentionOption {
  id: string;
  group: "Files and folders" | "Terminals" | "Past conversations" | "Branch";
  label: string;
  detail: string;
  /** What choosing it adds to the message; null for an entry that only opens a folder. */
  mention: Mention | null;
  /** Choosing it opens this folder in the menu instead of mentioning it. */
  descend?: string;
  /** Why it cannot be chosen, in the Core's words; null when it can. */
  disabledReason: string | null;
}

export interface MentionSources {
  options: MentionOption[];
  loading: boolean;
}

const errText = (e: unknown): string => ((e as Error)?.message ?? String(e)).replace(/^Error invoking remote method '[^']*': (Error: )?/, "").replace(/^[A-Z_]{3,}: /, "");

/** The options of the @ menu for what is typed, read through the typed preload calls. Stale answers are dropped. */
export function useMentionSources(args: { taskId: string; sessionId: string | null; query: string; active: boolean; hasWorkspace: boolean; hasChanges: boolean }): MentionSources {
  const { taskId, sessionId, query, active, hasWorkspace, hasChanges } = args;
  const [options, setOptions] = useState<MentionOption[]>([]);
  const [loading, setLoading] = useState(false);
  useEffect(() => {
    if (!active) return;
    let cancelled = false;
    setLoading(true);
    const slash = query.lastIndexOf("/");
    const dir = slash >= 0 ? query.slice(0, slash) : "";
    const name = (slash >= 0 ? query.slice(slash + 1) : query).toLowerCase();
    const q = query.toLowerCase();
    void (async () => {
      const out: MentionOption[] = [];
      // Files and folders: the Core lists the directory; a path its policy protects is listed by name and never opened.
      if (hasWorkspace) {
        try {
          const l = await window.modbit.listWorkspaceDir(taskId, dir);
          if (slash >= 0) out.push({ id: `dir:${dir}`, group: "Files and folders", label: `${dir}/`, detail: "this folder", mention: { kind: "folder", value: `${dir}/`, label: `${dir}/` }, disabledReason: null });
          for (const e of l.entries.filter((x) => x.name.toLowerCase().includes(name)).slice(0, 40)) {
            if (e.kind === "dir") out.push({ id: `dir:${e.path}`, group: "Files and folders", label: `${e.name}/`, detail: "folder, press Enter to open it", mention: null, descend: `${e.path}/`, disabledReason: e.protected ? "Protected by the workspace path policy: it is listed by name and never opened." : null });
            else if (e.kind === "file") out.push({ id: `file:${e.path}`, group: "Files and folders", label: e.name, detail: e.path, mention: { kind: "file", value: e.path, label: e.path }, disabledReason: e.protected ? "Protected by the workspace path policy: it cannot be attached." : null });
          }
        } catch (e) {
          out.push({ id: "files-unavailable", group: "Files and folders", label: "Files are unavailable", detail: errText(e), mention: null, disabledReason: errText(e) });
        }
      } else {
        out.push({ id: "files-unavailable", group: "Files and folders", label: "Files are unavailable", detail: "The task has no workspace yet", mention: null, disabledReason: "The task has not started, so it has no workspace to browse." });
      }
      try {
        const t = await window.modbit.terminalList(taskId);
        for (const x of t.terminals.filter((y) => y.taskId === taskId && (q === "" || y.title.toLowerCase().includes(q) || "terminal".includes(q)))) {
          out.push({ id: `term:${x.terminalId}`, group: "Terminals", label: x.title || "terminal", detail: x.state, mention: { kind: "terminal", value: `terminal:${x.terminalId}`, label: x.title || "terminal" }, disabledReason: null });
        }
      } catch {
        // No terminal list: the group is simply absent.
      }
      if (sessionId && q.length >= 2) {
        try {
          const r = await window.modbit.searchConversations(sessionId, query, { limit: 5, maxSnippets: 0 });
          for (const h of r.hits.filter((x) => x.taskId !== taskId)) out.push({ id: `conv:${h.taskId}`, group: "Past conversations", label: h.title || "Untitled conversation", detail: h.statusLabel, mention: { kind: "conversation", value: `task:${h.taskId}`, label: h.title || "conversation" }, disabledReason: null });
        } catch {
          // Search unavailable: the group is absent.
        }
      }
      if ("changes".includes(q) || "diff".includes(q) || "branch".includes(q)) {
        out.push({ id: "changes", group: "Branch", label: "Changes on this branch", detail: hasChanges ? "the task's diff against its base" : "no changes yet", mention: hasChanges ? { kind: "changes", value: "changes", label: "branch changes" } : null, disabledReason: hasChanges ? null : "The task has not changed anything on its branch yet." });
      }
      if (!cancelled) {
        setOptions(out);
        setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [taskId, sessionId, query, active, hasWorkspace, hasChanges]);
  return { options, loading };
}

const GROUPS: MentionOption["group"][] = ["Files and folders", "Terminals", "Past conversations", "Branch"];

export function MentionMenu({ id, options, loading, active, onActive, onChoose }: { id: string; options: readonly MentionOption[]; loading: boolean; active: number; onActive: (i: number) => void; onChoose: (o: MentionOption) => void }) {
  return (
    <div className="cmp-menu" data-testid="mention-menu" data-loading={loading}>
      {options.length > 0 && (
      <ul id={id} role="listbox" aria-label="Add to the message" className="cmp-list">
        {GROUPS.flatMap((g) => {
          const inGroup = options.map((o, i) => ({ o, i })).filter(({ o }) => o.group === g);
          if (inGroup.length === 0) return [];
          return [
            <li key={`h-${g}`} role="presentation" className="cmp-group">
              {g}
            </li>,
            ...inGroup.map(({ o, i }) => (
              <li key={o.id} id={`${id}-o${i}`} role="option" aria-selected={i === active} aria-disabled={o.disabledReason ? true : undefined} className="cmp-option" data-active={i === active} data-testid="mention-option" data-option-id={o.id} onMouseMove={() => onActive(i)} onMouseDown={(e) => e.preventDefault()} onClick={() => onChoose(o)}>
                <span className="cmp-option-label">{o.label}</span>
                <span className="meta cmp-option-detail">{o.disabledReason ?? o.detail}</span>
              </li>
            )),
          ];
        })}
      </ul>
      )}
      {options.length === 0 && (
        <p className="meta cmp-empty" role="status">
          {loading ? "Looking…" : "Nothing matches."}
        </p>
      )}
    </div>
  );
}

/** The visible slash items and the divider, for the listbox and for the key handler's index. */
export function useSlashItems(inv: SlashInventoryView | null, query: string, running: boolean) {
  return useMemo(() => (inv ? slashMenu(inv, query, { running }) : { items: [] as SlashItem[], dividerAt: 0 }), [inv, query, running]);
}

export function SlashMenuView({ id, items, dividerAt, active, onActive, onChoose, loaded }: { id: string; items: readonly SlashItem[]; dividerAt: number; active: number; onActive: (i: number) => void; onChoose: (it: SlashItem) => void; loaded: boolean }) {
  const current = items[active];
  return (
    <div className="cmp-menu cmp-slash" data-testid="slash-menu">
      {items.length > 0 && (
      <ul id={id} role="listbox" aria-label="Commands and skills" className="cmp-list">
        {items.flatMap((it, i) => {
          const row = (
            <li key={`${it.entry.kind}:${it.entry.id}`} id={`${id}-o${i}`} role="option" aria-selected={i === active} aria-disabled={it.disabledReason ? true : undefined} className="cmp-option" data-active={i === active} data-testid="slash-option" data-entry-id={it.entry.id} data-enabled={!it.disabledReason} onMouseMove={() => onActive(i)} onMouseDown={(e) => e.preventDefault()} onClick={() => onChoose(it)}>
              <span className="cmp-option-label">/{it.entry.displayName}</span>
              <Badge tone="neutral">{scopeLabel(it.entry.scope)}</Badge>
              {it.disabledReason !== null && <Badge tone="warn">{it.entry.enabled ? "not now" : it.entry.trust.toLowerCase().replace(/_/g, " ")}</Badge>}
            </li>
          );
          return i === dividerAt && dividerAt > 0 ? [<li key="divider" role="presentation" className="cmp-divider" data-testid="slash-divider" />, row] : [row];
        })}
      </ul>
      )}
      {items.length === 0 && (
        <p className="meta cmp-empty" role="status">
          {loaded ? "No command or skill matches." : "Looking…"}
        </p>
      )}
      {current && (
        <aside className="cmp-detail" data-testid="slash-detail" aria-label="Details of the highlighted entry">
          <strong>/{current.entry.displayName}</strong>
          <p className="meta">{`${current.entry.kind.toLowerCase()} · ${scopeLabel(current.entry.scope)} · ${current.entry.trust.toLowerCase().replace(/_/g, " ")}`}</p>
          {current.entry.description && <p data-testid="slash-description">{current.entry.description}</p>}
          {current.disabledReason && (
            <p className="cmp-reason" data-testid="slash-reason">
              {current.disabledReason}
            </p>
          )}
        </aside>
      )}
    </div>
  );
}
