import { useCallback, useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { useLayer, useOutsidePress } from "./hooks.ts";
import { Kbd } from "./status.tsx";
import { navTarget, nextEnabled, typeAheadTarget } from "./nav.ts";

export interface MenuItem {
  id: string;
  label: string;
  /** A chord hint such as `mod+k`; display only: the command registry binds it. */
  shortcut?: string | undefined;
  disabled?: boolean | undefined;
  /** Renders as a checkable item (menuitemradio when `radio`). */
  checked?: boolean | undefined;
  radio?: boolean | undefined;
  /** A separator is drawn before this item. */
  separatorBefore?: boolean | undefined;
  onSelect: () => void;
}

export interface MenuProps {
  /** The accessible name of the menu (and of the trigger when it has no text). */
  label: string;
  trigger: ReactNode;
  items: readonly MenuItem[];
  placement?: "down" | "up" | undefined;
  align?: "start" | "end" | undefined;
  /** Test hook / styling hook for the trigger. */
  triggerTestId?: string | undefined;
  triggerClassName?: string | undefined;
}

/**
 * A menu button: Enter, Space or ArrowDown opens it on the first item (ArrowUp
 * on the last); arrows, Home, End and type-ahead move; Enter or Space selects;
 * Escape (the layer stack) and Tab close it and focus returns to the trigger.
 */
export function Menu({ label, trigger, items, placement = "down", align = "start", triggerTestId, triggerClassName }: MenuProps) {
  const id = useId();
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popRef = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<(HTMLElement | null)[]>([]);
  const disabled = items.map((i) => i.disabled === true);

  const close = useCallback((restoreFocus: boolean) => {
    setOpen(false);
    if (restoreFocus) triggerRef.current?.focus({ preventScroll: true });
  }, []);
  useLayer(open, `menu:${id}`, false, () => close(true));
  useOutsidePress(popRef, open, () => close(false), triggerRef);

  const openAt = (where: "first" | "last") => {
    const first = nextEnabled(-1, 1, disabled);
    const last = nextEnabled(0, -1, disabled);
    setActive(where === "first" ? Math.max(0, first) : Math.max(0, last));
    setOpen(true);
  };
  useEffect(() => {
    if (open) itemRefs.current[active]?.focus({ preventScroll: true });
  }, [open, active]);

  const onTriggerKey = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      openAt("first");
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      openAt("last");
    }
  };

  const onMenuKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Tab") {
      close(false);
      return;
    }
    const target = navTarget(e.key, active, items.length, "vertical");
    if (target !== null) {
      e.preventDefault();
      if (e.key === "Home") setActive(nextEnabled(-1, 1, disabled));
      else if (e.key === "End") setActive(nextEnabled(0, -1, disabled));
      else setActive(nextEnabled(active, e.key === "ArrowDown" ? 1 : -1, disabled));
      return;
    }
    if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey && e.key !== " ") {
      const i = typeAheadTarget(e.key, active, items.map((x) => x.label), disabled);
      if (i !== null) {
        e.preventDefault();
        setActive(i);
      }
    }
  };

  const select = (item: MenuItem) => {
    if (item.disabled) return;
    close(true);
    item.onSelect();
  };

  return (
    <span className="mb-menu-host">
      <button ref={triggerRef} type="button" className={`mb-btn${triggerClassName ? ` ${triggerClassName}` : ""}`} data-variant="ghost" data-size="md" data-testid={triggerTestId} aria-haspopup="menu" aria-expanded={open} aria-controls={open ? id : undefined} aria-label={label} onClick={() => (open ? close(false) : openAt("first"))} onKeyDown={onTriggerKey}>
        {trigger}
      </button>
      {open && (
        <div ref={popRef} id={id} role="menu" aria-label={label} className="mb-popover mb-menu" data-placement={placement} data-align={align} onKeyDown={onMenuKey}>
          {items.map((item, i) => (
            <div key={item.id} role="none">
              {item.separatorBefore && i > 0 && <div role="separator" className="mb-menu-sep" />}
              <div
                ref={(el) => {
                  itemRefs.current[i] = el;
                }}
                role={item.radio ? "menuitemradio" : item.checked !== undefined ? "menuitemcheckbox" : "menuitem"}
                {...(item.checked === undefined ? {} : { "aria-checked": item.checked })}
                aria-disabled={item.disabled ? true : undefined}
                tabIndex={i === active ? 0 : -1}
                className="mb-menu-item"
                data-testid={`menu-item-${item.id}`}
                onClick={() => select(item)}
                onMouseMove={() => !item.disabled && setActive(i)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    select(item);
                  }
                }}
              >
                <span className="mb-menu-check" aria-hidden="true">
                  {item.checked ? "✓" : ""}
                </span>
                <span className="mb-menu-label">{item.label}</span>
                {item.shortcut && <Kbd chord={item.shortcut} />}
              </div>
            </div>
          ))}
        </div>
      )}
    </span>
  );
}
