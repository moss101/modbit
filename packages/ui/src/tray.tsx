import { useMemo, useState, useSyncExternalStore, type ReactNode } from "react";
import { Button } from "./button.tsx";
import { useKeyScope, useLayer } from "./hooks.ts";
import { bindingsScope } from "./keys.ts";
import { Kbd } from "./status.tsx";
import { trays as defaultStore, type TrayDef, type TrayStore } from "./trays.ts";

const TONE_LABEL = { info: "Notice", warn: "Warning", error: "Error", attention: "Needs you" } as const;

/**
 * The tray host (AFW-H01): docked above the composer, never a modal. The
 * active tray is shown in full and owns its actions' chords while it is
 * active; the others are one-line rows that bring themselves forward. Trays
 * stay until resolved or dismissed, wherever the person navigates.
 */
export function TrayHost({ store = defaultStore as TrayStore<ReactNode> }: { store?: TrayStore<ReactNode> | undefined }) {
  const snap = useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
  const active = snap.trays.find((t) => t.id === snap.activeId) ?? null;
  const others = snap.trays.filter((t) => t.id !== snap.activeId);
  return (
    <section className="mb-tray-host" aria-label="Notices" data-testid="tray-host" data-count={snap.trays.length}>
      {others.map((t) => (
        <button key={t.id} type="button" className="mb-tray-chip" data-tone={t.tone} data-testid="tray-pending" data-tray-id={t.id} onClick={() => store.activate(t.id)}>
          <span className="mb-tray-chip-tone">{TONE_LABEL[t.tone]}:</span> {t.title}
        </button>
      ))}
      {active && <Tray key={active.id} def={active} store={store} />}
    </section>
  );
}

function Tray({ def, store }: { def: TrayDef<ReactNode>; store: TrayStore<ReactNode> }) {
  const [focused, setFocused] = useState(false);
  const dismissible = def.dismissible !== false;
  // The active tray's chords live on the scope stack for as long as it is active.
  const scope = useMemo(() => {
    const bindings: Record<string, () => void> = {};
    for (const a of def.actions ?? []) if (a.keys) bindings[a.keys] = a.run;
    return bindingsScope(`tray:${def.id}`, bindings, false);
  }, [def]);
  useKeyScope(true, scope);
  // Escape dismisses the tray only while focus is inside it, and only after any inner layer.
  useLayer(focused && dismissible, `tray:${def.id}`, false, () => store.dismiss(def.id));
  return (
    <div
      role={def.tone === "error" ? "alert" : "status"}
      className="mb-tray"
      data-tone={def.tone}
      data-testid="tray"
      data-tray-id={def.id}
      data-active="true"
      onFocus={() => setFocused(true)}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setFocused(false);
      }}
    >
      <div className="mb-tray-head">
        <strong className="mb-tray-title">
          <span className="mb-tray-tone">{TONE_LABEL[def.tone]}: </span>
          {def.title}
        </strong>
        {dismissible && (
          <Button size="sm" variant="ghost" data-testid="tray-dismiss" onClick={() => store.dismiss(def.id)} aria-label={`Dismiss: ${def.title}`}>
            Dismiss
          </Button>
        )}
      </div>
      {def.body !== undefined && <div className="mb-tray-body">{def.body}</div>}
      {(def.actions?.length ?? 0) > 0 && (
        <div className="mb-tray-actions">
          {def.actions!.map((a) => (
            <Button key={a.id} size="sm" variant={a.primary ? "primary" : "secondary"} data-testid={`tray-action-${a.id}`} onClick={a.run}>
              {a.label} {a.keys && <Kbd chord={a.keys} />}
            </Button>
          ))}
        </div>
      )}
    </div>
  );
}
