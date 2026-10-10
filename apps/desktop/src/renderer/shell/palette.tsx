import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { Badge, Dialog, Kbd } from "@modbit/ui";
import { rank } from "@modbit/ui/logic";

export type PaletteKind = "agent" | "action" | "setting";

export interface PaletteItem {
  id: string;
  kind: PaletteKind;
  title: string;
  /** A second line: the task's state, a group, the reason a command is unavailable. */
  detail?: string | undefined;
  keywords?: readonly string[] | undefined;
  /** A chord hint (display only). */
  chord?: string | undefined;
  /** A reason string disables the item and is shown instead of running it. */
  disabled?: string | undefined;
  run: () => void;
}

const KIND_LABEL: Record<PaletteKind, string> = { agent: "Agent", action: "Action", setting: "Setting" };
const KIND_ORDER: PaletteKind[] = ["agent", "action", "setting"];

/**
 * The command palette (AFW-A15): a modal combobox over agents, actions and
 * settings. Typing filters and ranks; ArrowUp/Down move the active option
 * (aria-activedescendant, focus stays in the field); Enter runs it; Escape
 * (the layer stack) closes it and focus returns to where it was.
 */
export function CommandPalette({ open, items, onClose, announce }: { open: boolean; items: readonly PaletteItem[]; onClose: () => void; announce: (text: string) => void }) {
  const input = useRef<HTMLInputElement>(null);
  const pending = useRef<(() => void) | null>(null);
  // The chosen command runs once the dialog has closed and returned focus, so a command that moves focus wins.
  useEffect(() => {
    if (open || !pending.current) return;
    const run = pending.current;
    pending.current = null;
    run();
  }, [open]);
  return (
    <Dialog open={open} title="Command palette" hideTitle onClose={onClose} initialFocus={input} testId="palette" className="palette">
      <PaletteBody items={items} onClose={onClose} announce={announce} input={input} pending={pending} />
    </Dialog>
  );
}

function PaletteBody({ items, onClose, announce, input, pending }: { items: readonly PaletteItem[]; onClose: () => void; announce: (text: string) => void; input: React.RefObject<HTMLInputElement | null>; pending: React.RefObject<(() => void) | null> }) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const listId = useId();
  const results = useMemo(() => {
    const ranked = rank(query, items);
    // With no query the groups keep their order; with one, the best match leads whatever its kind.
    return query.trim() ? ranked : [...ranked].sort((a, b) => KIND_ORDER.indexOf(a.kind) - KIND_ORDER.indexOf(b.kind));
  }, [query, items]);
  const at = Math.min(active, Math.max(0, results.length - 1));
  const activeItem = results[at];

  const choose = (item: PaletteItem | undefined) => {
    if (!item) return;
    if (item.disabled) {
      announce(`${item.title} is unavailable: ${item.disabled}`);
      return;
    }
    pending.current = item.run;
    onClose();
  };
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive(results.length === 0 ? 0 : (at + 1) % results.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive(results.length === 0 ? 0 : (at - 1 + results.length) % results.length);
    } else if (e.key === "Home") {
      e.preventDefault();
      setActive(0);
    } else if (e.key === "End") {
      e.preventDefault();
      setActive(Math.max(0, results.length - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      choose(activeItem);
    }
  };
  return (
    <>
      <input
        ref={input}
        type="text"
        role="combobox"
        aria-expanded="true"
        aria-controls={listId}
        aria-autocomplete="list"
        aria-activedescendant={activeItem ? `${listId}-${activeItem.id}` : undefined}
        aria-label="Search agents, actions and settings"
        placeholder="Search agents, actions and settings"
        className="palette-input"
        data-testid="palette-input"
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setActive(0);
        }}
        onKeyDown={onKey}
        autoComplete="off"
        spellCheck={false}
      />
      <ul id={listId} role="listbox" aria-label="Results" className="palette-list" data-testid="palette-list">
        {results.map((item, i) => (
          <li
            key={item.id}
            id={`${listId}-${item.id}`}
            role="option"
            aria-selected={i === at}
            aria-disabled={item.disabled ? true : undefined}
            className="palette-option"
            data-testid="palette-option"
            data-kind={item.kind}
            data-id={item.id}
            data-active={i === at}
            onPointerMove={() => setActive(i)}
            onClick={() => choose(item)}
          >
            <Badge tone="neutral">{KIND_LABEL[item.kind]}</Badge>
            <span className="palette-title">{item.title}</span>
            {(item.disabled ?? item.detail) && <span className="meta palette-detail">{item.disabled ? `unavailable: ${item.disabled}` : item.detail}</span>}
            {item.chord && <Kbd chord={item.chord} />}
          </li>
        ))}
      </ul>
      {results.length === 0 && (
        <p className="meta" role="status" data-testid="palette-empty">
          Nothing matches “{query}”.
        </p>
      )}
    </>
  );
}
