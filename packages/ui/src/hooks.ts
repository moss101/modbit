import { useEffect, useRef, type RefObject } from "react";
import { layers } from "./layers.ts";
import { isMacPlatform, keyScopes, type KeyScope } from "./keys.ts";

/** Keeps `onClose` current without re-registering the layer on every render. */
function useLatest<T>(value: T): RefObject<T> {
  const ref = useRef(value);
  ref.current = value;
  return ref;
}

/** While `open`, this component is a layer: Escape closes the innermost one. */
export function useLayer(open: boolean, id: string, modal: boolean, onClose: () => void): void {
  const latest = useLatest(onClose);
  useEffect(() => {
    if (!open) return;
    return layers.push({ id, modal, onClose: () => latest.current() });
  }, [open, id, modal, latest]);
}

/** While `active`, the scope is on the key-scope stack. */
export function useKeyScope(active: boolean, scope: KeyScope | null): void {
  const latest = useLatest(scope);
  useEffect(() => {
    if (!active || !latest.current) return;
    const s = latest.current;
    return keyScopes.push({ id: s.id, blocking: s.blocking, resolve: (e, mac) => latest.current?.resolve(e, mac) ?? null });
  }, [active, latest]);
}

/** The window-level Escape handler: closes the innermost layer and consumes the key. Install once at the root. */
export function useEscapeLayers(): void {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      if (layers.escape()) {
        e.preventDefault();
        e.stopImmediatePropagation();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);
}

const FOCUSABLE = 'a[href], button:not(:disabled), input:not(:disabled):not([type="hidden"]), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])';

export function focusables(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => !el.hasAttribute("hidden") && el.getAttribute("aria-hidden") !== "true");
}

/** Focus moves into the container when it opens, Tab cycles inside it, and focus returns to where it was when it closes. */
export function useFocusTrap(ref: RefObject<HTMLElement | null>, active: boolean, initial?: RefObject<HTMLElement | null>): void {
  useEffect(() => {
    const root = ref.current;
    if (!active || !root) return;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    (initial?.current ?? focusables(root)[0] ?? root).focus({ preventScroll: true });
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Tab") return;
      const items = focusables(root);
      if (items.length === 0) {
        e.preventDefault();
        root.focus();
        return;
      }
      const first = items[0]!;
      const last = items[items.length - 1]!;
      const at = document.activeElement;
      if (e.shiftKey && (at === first || !root.contains(at))) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && (at === last || !root.contains(at))) {
        e.preventDefault();
        first.focus();
      }
    };
    root.addEventListener("keydown", onKey);
    return () => {
      root.removeEventListener("keydown", onKey);
      if (previous && previous.isConnected) previous.focus({ preventScroll: true });
    };
  }, [active, ref, initial]);
}

/** Calls `cb` for a pointer press outside `ref` (and outside `also`, e.g. the trigger). */
export function useOutsidePress(ref: RefObject<HTMLElement | null>, active: boolean, cb: () => void, also?: RefObject<HTMLElement | null>): void {
  const latest = useLatest(cb);
  useEffect(() => {
    if (!active) return;
    const onDown = (e: PointerEvent) => {
      const t = e.target as Node | null;
      if (t && (ref.current?.contains(t) || also?.current?.contains(t))) return;
      latest.current();
    };
    document.addEventListener("pointerdown", onDown, true);
    return () => document.removeEventListener("pointerdown", onDown, true);
  }, [active, ref, also, latest]);
}

/**
 * The window-level key dispatcher (capture phase): the innermost scope with a
 * binding for the key runs it and consumes the key, so nothing below the scope
 * (the page's own handlers, the single-letter shortcuts) also acts. Install
 * once at the root, after `useEscapeLayers`.
 */
export function useKeyDispatch(): void {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented) return;
      if (keyScopes.dispatch(e, isMacPlatform())) {
        e.preventDefault();
        e.stopImmediatePropagation();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);
}
