/**
 * Component styles for the primitives (DR-PX-2026-10-24-016, PX-142). Every rule expresses the recipe of the matching ZCode
 * component (Apache-2.0, third-party/zcode-ui/NOTICE.md) as measured on its real build (evidence/zcode-ui/reference) over the
 * token layer: ZCode's own `--color-*`, `--text-ui-*`, `--radius-*` and `--shadow-*` names, plus the few Modbit roles where a
 * declared exception keeps AFW-A12 (`--mb-color-border-control` for a control edge, ZDX-03; `--mb-color-text-muted` for secondary
 * text, ZDX-01; `--mb-color-focus` for the focus ring, ZDX-04; the status-surface foregrounds for status text, ZDX-02). No
 * literal colour appears here, so a theme change or the high-contrast mode reaches every primitive. Under prefers-reduced-motion
 * the token stylesheet removes every transition and animation (ZDX-09).
 */
export const uiCss = `
:root { --mb-transition-colors: color, background-color, border-color, outline-color, text-decoration-color, fill, stroke, --tw-gradient-from, --tw-gradient-via, --tw-gradient-to; --mb-ease: cubic-bezier(0.4, 0, 0.2, 1); --mb-shadow-stack: 0 0 #0000, 0 0 #0000, 0 0 #0000, 0 0 #0000; }
.mb-sr-only { position: absolute; width: 1px; height: 1px; margin: -1px; padding: 0; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; border: 0; }

/* Buttons: ZDL button.tsx. A 1 px transparent border on every variant, so the outline variant changes colour and never size. */
.mb-btn { box-sizing: border-box; display: inline-flex; flex-shrink: 0; align-items: center; justify-content: center; gap: 4px; height: var(--mb-control-md); padding: 0 8px; border: 1px solid transparent; border-radius: var(--radius-md); background: transparent; background-clip: padding-box; color: var(--color-foreground); font: inherit; font-size: var(--text-ui-base); font-weight: 400; line-height: inherit; white-space: nowrap; text-decoration: none; cursor: default; user-select: none; outline: none; transition-property: var(--mb-transition-colors); transition-duration: var(--mb-motion-base); transition-timing-function: var(--mb-ease); }
.mb-btn[data-size="xs"] { height: var(--mb-control-xs); border-radius: var(--radius-sm); }
.mb-btn[data-size="sm"] { height: var(--mb-control-sm); line-height: 1.625; }
.mb-btn[data-size="lg"] { height: var(--mb-control-lg); padding: 0 10px; border-radius: var(--radius-lg); }
.mb-btn:has(> [data-icon="inline-start"]) { padding-left: 6px; }
.mb-btn[data-size="lg"]:has(> [data-icon="inline-start"]) { padding-left: 8px; }
.mb-btn > svg { flex-shrink: 0; width: 14px; height: 14px; pointer-events: none; }
.mb-btn[data-size="xs"] > svg { width: 10px; height: 10px; }
.mb-btn[data-size="sm"] > svg { width: 12px; height: 12px; }
.mb-btn[data-size="lg"] > svg { width: 16px; height: 16px; }
.mb-btn[data-variant="primary"] { background: var(--color-primary); color: var(--color-primary-foreground); }
.mb-btn[data-variant="primary"]:hover:not(:disabled) { background: color-mix(in oklab, var(--color-primary) 80%, transparent); }
.mb-btn[data-variant="secondary"] { border-color: var(--mb-color-border-control); }
.mb-btn[data-variant="secondary"]:hover:not(:disabled), .mb-btn[data-variant="secondary"][aria-expanded="true"] { background: color-mix(in oklab, var(--color-input) 50%, transparent); }
.mb-btn[data-variant="subtle"] { background: var(--color-secondary); }
.mb-btn[data-variant="subtle"]:hover:not(:disabled), .mb-btn[data-variant="subtle"][aria-expanded="true"] { background: color-mix(in oklab, var(--color-secondary) 80%, transparent); }
.mb-btn[data-variant="ghost"]:hover:not(:disabled), .mb-btn[data-variant="ghost"][aria-expanded="true"] { background: var(--color-hover); }
.mb-btn[data-variant="danger"] { background: var(--color-destructive); color: var(--mb-color-text-on-danger-solid); }
/* ZDL lightens the destructive fill on hover; under the label that is 4.03:1 in light, so hover moves toward the text colour instead (darker in light, lighter in dark): ZDX-05. */
.mb-btn[data-variant="danger"]:hover:not(:disabled) { background: color-mix(in oklab, var(--color-destructive) 90%, var(--mb-color-text)); }
.mb-btn[aria-pressed="true"]:not([data-variant="primary"]):not([data-variant="danger"]) { background: var(--color-selected); }
.mb-btn:disabled, .mb-btn[aria-disabled="true"] { opacity: 0.5; }
.mb-icon-btn.mb-icon-btn { padding: 0; width: var(--mb-control-md); line-height: 1.625; gap: normal; }
.mb-icon-btn.mb-icon-btn[data-size="xs"] { width: var(--mb-control-xs); }
.mb-icon-btn.mb-icon-btn[data-size="sm"] { width: var(--mb-control-sm); }
.mb-icon-btn.mb-icon-btn[data-size="lg"] { width: var(--mb-control-lg); }
.mb-icon { display: inline-flex; width: 16px; height: 16px; align-items: center; justify-content: center; }
.mb-icon-btn[data-size="xs"] .mb-icon { width: 10px; height: 10px; }
.mb-icon-btn[data-size="sm"] .mb-icon { width: 12px; height: 12px; }
:where(button, a, input, select, textarea, [tabindex]):focus-visible { outline: 2px solid var(--mb-color-focus); outline-offset: 2px; }

/* Native form controls: ZDL input, textarea, select trigger and checkbox, restyled in place so every screen inherits them. A control edge is the 3:1 role (ZDX-03). */
:where(input:not([type="checkbox"]):not([type="radio"]):not([type="range"]):not([type="file"]):not([type="color"]):not([type="button"]):not([type="submit"]), textarea, select) { box-sizing: border-box; height: var(--mb-control-md); min-width: 0; padding: 2px 8px; border: 1px solid var(--mb-color-border-control); border-radius: var(--radius-md); background: var(--color-input); color: var(--color-foreground); font: inherit; font-size: var(--text-ui-base); line-height: 1.625; outline: none; transition-property: var(--mb-transition-colors); transition-duration: var(--mb-motion-base); transition-timing-function: var(--mb-ease); }
:where(input:not([type="checkbox"]):not([type="radio"]):not([type="range"]):not([type="file"]):not([type="color"]):not([type="button"]):not([type="submit"]), textarea):where(:not(:disabled)) { cursor: text; }
:where(select) { padding: 0 4px 0 8px; }
:where(textarea) { height: auto; min-height: 64px; padding: 8px; background: color-mix(in oklab, var(--color-input) 20%, transparent); }
:where(input, textarea)::placeholder { color: var(--mb-color-text-muted); opacity: 1; }
:where(input, textarea, select):disabled { opacity: 0.5; cursor: not-allowed; }
:where(input, textarea, select)[aria-invalid="true"] { border-color: var(--color-destructive); box-shadow: var(--mb-shadow-stack), 0 0 0 2px color-mix(in oklab, var(--color-destructive) 20%, transparent), 0 0 #0000; }
:where(.mb-label) { display: flex; align-items: center; font-weight: 500; line-height: 1; user-select: none; }
input[type="checkbox"], input[type="radio"] { appearance: none; position: relative; box-sizing: border-box; flex-shrink: 0; width: 16px; height: 16px; margin: 0; padding: 0; border: 1px solid var(--mb-color-border-control); background: var(--color-input); color: var(--color-primary-foreground); font: inherit; outline: none; vertical-align: middle; cursor: default; transition-property: var(--mb-transition-colors); transition-duration: var(--mb-motion-base); transition-timing-function: var(--mb-ease); }
input[type="checkbox"] { border-radius: var(--radius-sm); }
input[type="radio"] { border-radius: calc(infinity * 1px); }
input[type="checkbox"]:checked, input[type="radio"]:checked { border-color: var(--color-primary); background: var(--color-primary); }
input[type="checkbox"]:checked::after { content: ""; position: absolute; inset: 0; margin: auto; width: 12px; height: 12px; background: currentColor; -webkit-mask: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M20 6 9 17l-5-5'/%3E%3C/svg%3E") center / contain no-repeat; }
input[type="radio"]:checked::after { content: ""; position: absolute; inset: 0; margin: auto; width: 6px; height: 6px; border-radius: calc(infinity * 1px); background: currentColor; }
input[type="checkbox"]:disabled, input[type="radio"]:disabled { opacity: 0.5; }

.mb-kbd { box-sizing: border-box; display: inline-flex; flex-shrink: 0; align-items: center; justify-content: center; gap: 4px; min-width: 16px; height: 16px; padding: 0 6px; border-radius: var(--radius-md); background: var(--color-tooltip-tag); color: var(--color-tooltip-tag-foreground); font-family: var(--font-mono); font-size: var(--text-ui-xs); font-weight: 500; line-height: 1.5; letter-spacing: normal; white-space: nowrap; user-select: none; pointer-events: none; }
.mb-kbd[data-apple="true"] { font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "SF Pro Display", "Helvetica Neue", sans-serif; }
.mb-kbd-group { display: inline-flex; align-items: center; gap: 4px; }

/* Badge: ZDL badge.tsx. Text only (a count or a word), so it is never colour alone. Status tones use the state surfaces and their AA foregrounds (ZDX-02). */
.mb-badge { box-sizing: border-box; display: inline-flex; flex-shrink: 0; align-items: center; justify-content: center; gap: 4px; width: fit-content; height: 20px; padding: 2px 8px; overflow: hidden; border: 1px solid transparent; border-radius: calc(infinity * 1px); background: transparent; color: var(--color-foreground); font-size: var(--text-ui-xs); font-weight: 500; line-height: 1.5; white-space: nowrap; transition-property: all; transition-duration: var(--mb-motion-base); transition-timing-function: var(--mb-ease); }
.mb-badge[data-tone="accent"] { background: var(--color-primary); color: var(--color-primary-foreground); }
.mb-badge[data-tone="neutral"] { background: var(--color-secondary); }
.mb-badge[data-tone="outline"] { border-color: var(--color-border); background: color-mix(in oklab, var(--color-input) 20%, transparent); }
.mb-badge[data-tone="danger"] { background: color-mix(in oklab, var(--color-destructive) 10%, var(--mb-color-raised)); color: var(--color-destructive-surface-foreground); }
.mb-badge[data-tone="ok"] { background: var(--color-success-surface); color: var(--color-success-surface-foreground); }
.mb-badge[data-tone="warn"] { background: color-mix(in srgb, var(--color-warning) 12%, var(--mb-color-raised)); color: var(--color-warning-surface-foreground); }
.mb-badge[data-tone="info"] { background: var(--color-progress-surface); color: var(--color-progress-surface-foreground); }

.mb-dot { display: inline-flex; align-items: center; gap: var(--mb-space-1); color: var(--mb-color-text-muted); }
.mb-dot-glyph { display: inline-flex; width: 14px; height: 14px; }
.mb-dot-glyph svg { width: 100%; height: 100%; }
.mb-dot[data-status="ok"] .mb-dot-glyph { color: var(--mb-color-ok); }
.mb-dot[data-status="warn"] .mb-dot-glyph { color: var(--mb-color-warn); }
.mb-dot[data-status="danger"] .mb-dot-glyph { color: var(--mb-color-danger); }
.mb-dot[data-status="info"] .mb-dot-glyph, .mb-dot[data-status="running"] .mb-dot-glyph { color: var(--mb-color-info); }
@keyframes mb-spin { to { transform: rotate(360deg); } }
@keyframes mb-shimmer { from { background-position: 200% 0; } to { background-position: -200% 0; } }
.mb-spin { animation: mb-spin 1s linear infinite; }
.mb-shimmer { background: linear-gradient(90deg, var(--mb-color-text-muted) 30%, var(--mb-color-text) 50%, var(--mb-color-text-muted) 70%) 200% 0 / 200% 100%; -webkit-background-clip: text; background-clip: text; color: transparent; animation: mb-shimmer 2s linear infinite; }

/* Menus: ZDL dropdown-menu.tsx (content, item, shortcut, checkbox item, separator). Secondary text is the AA muted role (ZDX-01). */
.mb-popover { position: absolute; z-index: var(--mb-z-popover); box-sizing: border-box; display: flex; flex-direction: column; gap: 2px; min-width: 128px; padding: 4px; background: var(--color-menu); color: var(--color-foreground); border: 1px solid var(--color-popover-border); border-radius: var(--radius-lg); box-shadow: var(--mb-shadow-stack), var(--shadow-md); font-size: var(--text-ui-base); line-height: 1.5; transition-duration: var(--mb-motion-fast); }
.mb-menu-host { position: relative; display: inline-flex; }
.mb-menu[data-placement="down"] { top: calc(100% + 4px); }
.mb-menu[data-placement="up"] { bottom: calc(100% + 4px); }
.mb-menu[data-align="start"] { left: 0; }
.mb-menu[data-align="end"] { right: 0; }
.mb-menu-item { position: relative; display: flex; align-items: center; gap: 8px; padding: 4px 8px; border-radius: var(--radius-md); color: var(--color-foreground); font-size: var(--text-ui-base); line-height: 1.625; cursor: default; outline: none; user-select: none; }
.mb-menu-item:has(.mb-menu-check) { padding-right: 32px; }
.mb-menu-item:hover, .mb-menu-item:focus-visible { background: var(--color-menu-hover); }
.mb-menu-item[data-tone="destructive"] { color: var(--color-destructive-surface-foreground); }
.mb-menu-item[data-tone="destructive"]:hover, .mb-menu-item[data-tone="destructive"]:focus-visible { background: color-mix(in oklab, var(--color-destructive) 10%, transparent); }
.mb-menu-item[aria-disabled="true"] { color: var(--color-foreground-subtlest); }
.mb-menu-item[role="menuitemcheckbox"] { padding-top: 6px; padding-bottom: 6px; line-height: 1.5; }
.mb-menu-item[role="menuitemcheckbox"] .mb-menu-check { line-height: 1.5; }
.mb-menu-label { flex: 1; white-space: nowrap; }
.mb-menu-shortcut { margin-left: auto; color: var(--mb-color-text-muted); font-size: var(--text-ui-xs); line-height: 1.625; letter-spacing: 0.1em; white-space: nowrap; }
.mb-menu-check { position: absolute; right: 8px; display: flex; width: 16px; height: 16px; align-items: center; justify-content: center; pointer-events: none; }
.mb-menu-check svg { color: var(--mb-color-icon-muted); }
.mb-menu-sep { height: 1px; margin: 4px -4px; background: var(--color-border); }

.mb-tooltip-host { position: relative; display: inline-flex; }
.mb-tooltip { position: absolute; width: max-content; z-index: var(--mb-z-tooltip); top: calc(100% + 2px); left: 50%; transform: translateX(-50%); box-sizing: border-box; display: inline-flex; align-items: center; gap: 6px; max-width: 320px; padding: 6px 12px; border: 1px solid var(--color-border); border-radius: var(--radius-lg); background: var(--color-tooltip); color: var(--color-tooltip-foreground); font-size: var(--text-ui-sm); line-height: 1.5; }
.mb-tooltip:has(.mb-kbd) { padding-right: 6px; }
.mb-tooltip .mb-kbd { border-radius: var(--radius-sm); }
.mb-tooltip[hidden] { display: none; }

/* Dialogs: ZDL dialog.tsx and alert-dialog.tsx. The scrim is black at 60 percent with a 4 px blur. */
.mb-scrim { position: fixed; inset: 0; z-index: var(--mb-z-modal); display: flex; align-items: center; justify-content: center; background: rgb(0 0 0 / 0.6); backdrop-filter: blur(4px); transition-duration: var(--mb-motion-fast); }
.mb-dialog { box-sizing: border-box; display: grid; gap: 16px; width: min(448px, calc(100vw - 32px)); max-height: calc(100vh - 64px); overflow: auto; padding: 16px; background: var(--color-popover); color: var(--color-foreground); border: 1px solid var(--color-popover-border); border-radius: var(--radius-2xl); box-shadow: var(--mb-shadow-stack), var(--shadow-md); font-size: var(--text-ui-base); line-height: 1.625; transition-duration: var(--mb-motion-fast); }
.mb-dialog[data-size="sm"] { width: min(384px, calc(100vw - 32px)); }
.mb-dialog[data-size="lg"] { width: min(760px, calc(100vw - 32px)); }
.mb-dialog[role="alertdialog"] { gap: 20px; padding: 20px; border: 0; background: color-mix(in oklab, var(--color-popover) 98%, transparent); box-shadow: var(--mb-shadow-stack), var(--shadow-2xl); }
.mb-dialog-header { display: grid; gap: 4px; }
.dialog-actions { display: flex; justify-content: flex-end; gap: 8px; margin: 0; }
.mb-dialog[role="alertdialog"] .mb-dialog-header { gap: 8px; }
.mb-dialog-title { margin: 0; font-size: var(--text-ui-base); font-weight: 500; line-height: 1.625; }
.mb-dialog[role="alertdialog"] .mb-dialog-title { font-size: var(--text-ui-lg); font-weight: 600; }
.mb-dialog-description { margin: 0; color: var(--mb-color-text-muted); }
.mb-dialog[role="alertdialog"] .mb-dialog-description { padding-top: 2px; line-height: 24px; }

/* Tabs: ZDL tabs.tsx. The selected tab is a raised pill, never colour alone: it also reads aria-selected. */
.mb-tabs { display: flex; flex-direction: column; }
.mb-tablist { box-sizing: border-box; display: inline-flex; align-items: center; justify-content: center; width: fit-content; max-width: 100%; height: 32px; padding: 3px; border-radius: var(--radius-lg); }
.mb-tab { box-sizing: border-box; display: inline-flex; align-items: center; justify-content: center; gap: 6px; height: 25px; min-height: 0; padding: 2px 6px; border: 1px solid transparent; border-radius: var(--radius-md); background: transparent; color: var(--mb-color-text-muted); font: inherit; font-size: var(--text-ui-base); font-weight: 500; line-height: 1.5; white-space: nowrap; cursor: default; transition-property: all; transition-duration: var(--mb-motion-base); transition-timing-function: var(--mb-ease); }
.mb-tab:hover:not(:disabled) { color: var(--color-foreground); }
.mb-tab[aria-selected="true"] { background: var(--color-background); color: var(--color-foreground); }
.mb-tab:disabled { opacity: 0.5; }
.mb-tabpanel { flex: 1; outline: none; }

/* Card: ZDL card.tsx. Decorative edge (the card is not a control), so the ZDL own border applies. */
.mb-card { box-sizing: border-box; display: flex; flex-direction: column; gap: 16px; padding: 16px 0; border: 1px solid var(--color-card-border); border-radius: var(--radius-xl); background: var(--color-card); color: var(--color-foreground); font-size: var(--text-ui-base); line-height: 1.625; }
.mb-card-header { display: grid; gap: 4px; padding: 0 16px; border-radius: var(--radius-xl) var(--radius-xl) 0 0; }
.mb-card-title { font-weight: 500; }
.mb-card-description { color: var(--mb-color-text-muted); }
.mb-card-content { padding: 0 16px; }
.mb-card-footer { display: flex; align-items: center; padding: 0 16px; border-radius: 0 0 var(--radius-xl) var(--radius-xl); }

.mb-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--mb-space-0_5); }
.mb-row { font: inherit; display: flex; align-items: center; gap: var(--mb-space-2); width: 100%; min-height: 30px; padding: var(--mb-space-1) var(--mb-space-2); border: 0; border-radius: var(--radius-md); background: transparent; color: var(--color-foreground); text-align: left; cursor: default; }
.mb-row:hover { background: var(--color-hover); }
.mb-row[aria-current="true"] { background: var(--color-selected); }
.mb-row-text { display: flex; flex-direction: column; min-width: 0; flex: 1; }
.mb-row-title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--text-ui-base); }
.mb-row-subtitle { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--text-ui-sm); color: var(--mb-color-text-muted); }
.mb-row-trailing, .mb-row-leading { display: inline-flex; align-items: center; color: var(--mb-color-text-muted); font-size: var(--text-ui-sm); }
.mb-tray-host { display: flex; flex-direction: column; gap: var(--mb-space-1); padding: 0 var(--mb-space-3); z-index: var(--mb-z-tray); pointer-events: none; }
.mb-tray-host > * { pointer-events: auto; }
.mb-tray-host:empty, .mb-tray-host[data-count="0"] { display: none; }
.mb-tray { background: var(--color-card); border: 1px solid var(--color-card-border); border-radius: var(--radius-xl); padding: var(--mb-space-2) var(--mb-space-3); box-shadow: var(--shadow-sm); }
.mb-tray[data-tone="error"] { background: var(--color-destructive-surface); border-color: var(--color-destructive-surface); }
.mb-tray[data-tone="warn"] { background: var(--color-warning-surface); border-color: var(--color-warning-surface); }
.mb-tray[data-tone="attention"], .mb-tray[data-tone="info"] { background: var(--color-progress-surface); border-color: var(--color-progress-surface); }
.mb-tray-head { display: flex; align-items: center; justify-content: space-between; gap: var(--mb-space-2); }
.mb-tray-body { margin-top: var(--mb-space-1); color: var(--color-foreground); }
.mb-tray-actions { display: flex; gap: var(--mb-space-2); margin-top: var(--mb-space-2); flex-wrap: wrap; }
.mb-tray-chip { font: inherit; font-size: var(--text-ui-sm); text-align: left; padding: var(--mb-space-1) var(--mb-space-3); border: 1px solid var(--mb-color-border-control); border-radius: calc(infinity * 1px); background: var(--color-card); color: var(--color-foreground); cursor: default; }
.mb-tray-chip-tone { color: var(--mb-color-text-muted); }
`;
