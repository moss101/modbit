/**
 * The model chip and its picker (REQ-PX-056; docs/65 AFW-D13, AFW-D14,
 * AFW-H02). The chip shows the model's name with a dimmer variant; the picker
 * opens upward from it with a search field, the objective profile (Auto) row,
 * and the catalog's models, one row per variant the Core lists. The renderer
 * computes no combination and holds no policy: a row is selectable because
 * the Core said a pin of it is allowed, and choosing any row is the Core's
 * `SetExecutionPreference`, whose typed refusal (a blocked model) is shown as
 * the Core wrote it. There is no billing or plan control here.
 */
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { Badge, useLayer, useOutsidePress } from "@modbit/ui";
import { OBJECTIVES, type ModelCatalogView, type ObjectiveId, type PostureView } from "../../shared/composer-types.ts";
import { modelChip, pickerRows, type PickerRow } from "./model.ts";

export interface PickerChoice {
  /** Auto with an objective (a pin is cleared), or a pin of a model variant. */
  kind: "objective" | "pin";
  objective?: ObjectiveId;
  row?: PickerRow;
}

export function ModelPicker({ posture, catalog, catalogError, disabled, onChoose, onOpen }: { posture: PostureView | null; catalog: ModelCatalogView | null; catalogError: string | null; disabled: boolean; onChoose: (c: PickerChoice) => Promise<void> | void; onOpen: () => void }) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const id = useId();
  const popRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const chip = modelChip(posture, catalog);
  const rows = useMemo(() => pickerRows(catalog, posture, query), [catalog, posture, query]);
  const current = posture?.preference.objective || catalog?.defaultObjective || "BALANCE";
  const pinned = !!posture?.preference.pinModel;
  const close = (restore: boolean) => {
    setOpen(false);
    setQuery("");
    if (restore) triggerRef.current?.focus({ preventScroll: true });
  };
  useLayer(open, `model-picker:${id}`, false, () => close(true));
  useOutsidePress(popRef, open, () => close(false), triggerRef);
  useEffect(() => {
    if (open) searchRef.current?.focus({ preventScroll: true });
  }, [open]);
  useEffect(() => {
    setActive((a) => Math.min(a, Math.max(0, rows.length - 1)));
  }, [rows.length]);

  const choose = async (c: PickerChoice) => {
    close(true);
    await onChoose(c);
  };
  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => (rows.length === 0 ? 0 : (a + (e.key === "ArrowDown" ? 1 : rows.length - 1)) % rows.length));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const r = rows[active];
      if (r) void choose({ kind: "pin", row: r });
    }
  };

  return (
    <span className="cmp-picker-host">
      <button ref={triggerRef} type="button" className="cmp-chip cmp-model" data-testid="model-chip" aria-haspopup="dialog" aria-expanded={open} aria-label={`Model: ${chip.name}${chip.variant ? `, ${chip.variant}` : ""}`} title={chip.routing ? `Routing: ${chip.routing}` : undefined} disabled={disabled} onClick={() => (open ? close(false) : (onOpen(), setOpen(true)))}>
        <span data-testid="model-chip-name">{chip.name}</span>
        {chip.variant && (
          <span className="cmp-dim" data-testid="model-chip-variant">
            {chip.variant}
          </span>
        )}
      </button>
      {open && (
        <div ref={popRef} className="cmp-picker" role="dialog" aria-label="Choose a model" data-testid="model-picker">
          <input ref={searchRef} type="search" className="cmp-picker-search" aria-label="Search models" placeholder="Search models" value={query} onChange={(e) => setQuery(e.target.value)} onKeyDown={onKey} aria-controls={`${id}-list`} aria-activedescendant={rows[active] ? `${id}-r${active}` : undefined} data-testid="model-search" />
          <div className="cmp-auto" role="group" aria-label="Auto: the router optimises for">
            <p className="cmp-auto-title">
              <strong>Auto</strong> <span className="meta">{pinned ? "off, a model is pinned" : "on"}</span>
            </p>
            <div className="cmp-objectives">
              {OBJECTIVES.map((o) => (
                <button key={o} type="button" className="cmp-seg" aria-pressed={!pinned && current === o} data-testid={`objective-${o.toLowerCase()}`} onClick={() => void choose({ kind: "objective", objective: o })}>
                  {o.charAt(0) + o.slice(1).toLowerCase()}
                </button>
              ))}
            </div>
            <p className="meta" data-testid="routing-note">
              {posture?.routing.outcome === "DIRECT" ? `Routing is direct${posture.routing.reasonCode ? `: ${posture.routing.reasonCode.toLowerCase().replace(/_/g, " ")}` : ""}. The profile is recorded, and the task runs the model it started with.` : posture?.routing.outcome ? `Routing: ${posture.routing.outcome.toLowerCase()}${posture.routing.floorMode ? `, quality floor ${posture.routing.floorMode}` : ""}.` : "The router has not evaluated this task yet."}
            </p>
          </div>
          <hr className="cmp-sep" />
          {catalogError && (
            <p className="cmp-reason" role="alert">
              The model list could not be read: {catalogError}
            </p>
          )}
          {rows.length > 0 && (
            <ul id={`${id}-list`} role="listbox" aria-label="Models" className="cmp-list cmp-models">
              {rows.map((r, i) => (
                <li key={r.id} id={`${id}-r${i}`} role="option" aria-selected={r.selected} aria-disabled={r.blocked ? true : undefined} className="cmp-option" data-active={i === active} data-testid="model-row" data-row-id={r.id} data-blocked={r.blocked !== null} onMouseMove={() => setActive(i)} onMouseDown={(e) => e.preventDefault()} onClick={() => void choose({ kind: "pin", row: r })}>
                  <span className="cmp-check" aria-hidden="true">
                    {r.selected ? "✓" : ""}
                  </span>
                  <span className="cmp-option-label">{r.name}</span>
                  {r.variant.label && <span className="cmp-dim">{r.variant.label}</span>}
                  {r.raisesCost && <Badge tone="warn">costs more</Badge>}
                  {r.blocked && <Badge tone="danger">{r.blocked.code.toLowerCase().replace(/_/g, " ")}</Badge>}
                </li>
              ))}
            </ul>
          )}
          {rows.length === 0 && !catalogError && (
            <p className="meta cmp-empty" role="status">
              {catalog ? "No model matches." : "Reading the model list…"}
            </p>
          )}
        </div>
      )}
    </span>
  );
}
