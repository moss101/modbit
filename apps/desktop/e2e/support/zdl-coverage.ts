/**
 * The closed coverage table of the fidelity gate (DR-PX-2026-10-24-016 section 6): for every record key of ZCode's reference capture
 * (`spec|slot|variant|size`) either the selector of the Modbit element that is compared with it, or the reason there is none. A
 * reference key missing here, or a key here that the capture does not have, fails the gate.
 */
export interface Cover {
  select: string;
  /** Compare only the first n elements (ZCode renders a hidden duplicate of a tooltip's content). */
  first?: number;
  /** Properties not compared, with the reason: a container's size follows children Modbit's primitive does not have. */
  ignore?: readonly string[];
  /** Skip the pixel comparison, with the reason: the container holds rows ZCode has and Modbit does not. */
  noPixels?: string;
}
export type Entry = Cover | { na: string };
export type SheetCoverage = Record<string, Entry>;

const CHROME = { na: "the harness's own heading or section wrapper, not a component" } as const;
const NO_VARIANT = { na: "variant not adopted: Modbit renders no solid warning or link-styled button" } as const;
const NO_SWITCH = { na: "Modbit has no switch control (the renderer uses native checkboxes)" } as const;
const NATIVE_LIST = { na: "the option list of a native select is drawn by the operating system" } as const;

const VZ = ["default", "outline", "secondary", "ghost", "destructive"] as const;
const SZ: readonly (readonly [string, string])[] = [["xs", "xs"], ["sm", "sm"], ["default", "md"]];

function buttons(): SheetCoverage {
  const c: SheetCoverage = {};
  for (const z of VZ) {
    for (const [zs, s] of [...SZ, ["lg", "lg"] as const]) c[`button-matrix|button|${z}|${zs}`] = { select: `[data-row="${z}"] .mb-btn[data-size="${s}"]` };
  }
  for (const z of ["warning", "link"]) for (const zs of ["xs", "sm", "default", "lg"]) c[`button-matrix|button|${z}|${zs}`] = NO_VARIANT;
  for (const [row, z] of [["icon-ghost", "ghost"], ["icon-outline", "outline"]] as const) {
    c[`button-matrix|button|${z}|icon-xs`] = { select: `[data-row="${row}"] .mb-icon-btn[data-size="xs"]` };
    c[`button-matrix|button|${z}|icon-sm`] = { select: `[data-row="${row}"] .mb-icon-btn[data-size="sm"]` };
    c[`button-matrix|button|${z}|icon`] = { select: `[data-row="${row}"] .mb-icon-btn[data-size="md"]` };
    c[`button-matrix|button|${z}|icon-md`] = { na: "ZCode's icon-md is the 28 px icon with a larger radius; Modbit has one 28 px icon size" };
    c[`button-matrix|button|${z}|icon-lg`] = { select: `[data-row="${row}"] .mb-icon-btn[data-size="lg"]` };
  }
  for (const k of ["button-matrix|section||", "button-matrix|h2||", "badge|section||", "badge|h2||"]) c[k] = CHROME;
  for (const [z, tone] of [["default", "accent"], ["secondary", "neutral"], ["destructive", "danger"], ["outline", "outline"]]) c[`badge|badge|${z}|`] = { select: `[data-zdl="badge"] .mb-badge[data-tone="${tone}"]` };
  c["badge|badge|ghost|"] = { na: "ZCode's ghost and link badges are bare text; Modbit renders none" };
  c["badge|badge|link|"] = c["badge|badge|ghost|"]!;
  return c;
}

const fields: SheetCoverage = {
  "fields|section||": CHROME,
  "fields|h2||": CHROME,
  "tabs|section||": CHROME,
  "tabs|h2||": CHROME,
  "fields|label||": { select: '[data-zdl="fields"] .mb-label' },
  "fields|input||": { select: '[data-zdl="fields"] input:not([type="checkbox"])' },
  "fields|textarea||": { select: '[data-zdl="fields"] textarea' },
  "fields|switch||default": NO_SWITCH,
  "fields|switch-thumb||": NO_SWITCH,
  "fields|checkbox||": { select: '[data-zdl="fields"] input[type="checkbox"]' },
  "fields|checkbox-indicator||": { na: "the native checkbox draws its check itself" },
  "fields|checkbox-checked-icon||": { na: "the native checkbox draws its check itself" },
  "fields|checkbox-indeterminate-icon||": { na: "the native checkbox draws its check itself" },
  "tabs|tabs||": { select: '[data-zdl="tabs"] .mb-tabs' },
  "tabs|tabs-list|default|": { select: '[data-zdl="tabs"] .mb-tablist', ignore: ["width"] },
  "tabs|tabs-trigger||": { select: '[data-zdl="tabs"] .mb-tab', ignore: ["width"] },
  "tabs|tabs-content||": { select: '[data-zdl="tabs"] .mb-tabpanel' },
};

const surfaces: SheetCoverage = {
  "surfaces|section||": CHROME,
  "surfaces|h2||": CHROME,
  "surfaces|card||default": { select: ".mb-card" },
  "surfaces|card-header||": { select: ".mb-card-header" },
  "surfaces|card-title||": { select: ".mb-card-title" },
  "surfaces|card-description||": { select: ".mb-card-description" },
  "surfaces|card-content||": { select: ".mb-card-content" },
  "surfaces|card-footer||": { select: ".mb-card-footer" },
  "surfaces|button|default|sm": { select: ".mb-card-footer .mb-btn" },
  "surfaces|alert||": { na: "Modbit's banners and trays use the state-surface tokens (ZDX-02), not a bordered alert box" },
  "surfaces|alert-title||": { na: "part of the alert, which Modbit does not render as a box" },
  "surfaces|alert-description||": { na: "part of the alert, which Modbit does not render as a box" },
  "surfaces|progress||": { na: "Modbit has no progress bar primitive" },
  "surfaces|progress-indicator||": { na: "Modbit has no progress bar primitive" },
  "surfaces|separator||": { na: "Modbit separates menu rows only (compared in the menu sheet)" },
  "surfaces|kbd-group||": { select: ".mb-kbd-group" },
  "surfaces|kbd||": { select: ".mb-kbd-group .mb-kbd" },
  "surfaces|button-group||": { na: "Modbit has no button group primitive" },
  "surfaces|button|outline|default": { na: "the buttons of ZCode's button group" },
};

const menu: SheetCoverage = {
  "|dropdown-menu-trigger|outline|default": { select: '.mb-btn[aria-haspopup="menu"]', ignore: ["width"] },
  "dropdown-menu|dropdown-menu-content||": { select: ".mb-popover", ignore: ["width", "height", "text"], noPixels: "ZCode's menu holds a heading and a submenu row that Modbit's does not" },
  "dropdown-menu|dropdown-menu-label||": { na: "Modbit menus have no heading rows" },
  "dropdown-menu|dropdown-menu-item|default|": { select: '.mb-menu-item[role="menuitem"]:not([data-tone])', ignore: ["width"] },
  "dropdown-menu|dropdown-menu-shortcut||": { select: ".mb-menu-shortcut" },
  "dropdown-menu|dropdown-menu-checkbox-item||": { select: '.mb-menu-item[role="menuitemcheckbox"]', ignore: ["width"] },
  "dropdown-menu|dropdown-menu-checkbox-item-indicator||": { select: ".mb-menu-check" },
  "dropdown-menu|dropdown-menu-separator||": { select: ".mb-menu-sep", ignore: ["width"] },
  "dropdown-menu|dropdown-menu-sub-trigger||": { na: "Modbit menus have no submenus" },
  "dropdown-menu|dropdown-menu-item|destructive|": { select: '.mb-menu-item[data-tone="destructive"]', ignore: ["width"] },
};

const select: SheetCoverage = {
  "select-trigger|select-trigger|input|default": { select: "select", ignore: ["text", "lineHeight", "gap", "paddingTop", "paddingBottom"] },
  "select-trigger|select-value||": { na: "the native select draws its value itself" },
  "select-content|select-content||": NATIVE_LIST,
  "select-content|select-scroll-up-button||": NATIVE_LIST,
  "select-content|select-group||": NATIVE_LIST,
  "select-content|select-label||": NATIVE_LIST,
  "select-content|select-item||": NATIVE_LIST,
};

const popover: SheetCoverage = {
  "|popover-trigger|outline|default": { na: "Modbit has no standalone popover primitive; its menus are compared in the menu sheet" },
  "popover|popover-content||": { na: "part of the standalone popover Modbit does not have" },
  "popover|popover-header||": { na: "part of the standalone popover Modbit does not have" },
  "popover|popover-title||": { na: "part of the standalone popover Modbit does not have" },
  "popover|popover-description||": { na: "part of the standalone popover Modbit does not have" },
};

const tooltip: SheetCoverage = {
  "|tooltip-trigger|ghost|icon": { select: ".mb-icon-btn" },
  "tooltip|tooltip-content||": { select: ".mb-tooltip", ignore: ["text"] },
  "tooltip|kbd||": { select: ".mb-tooltip .mb-kbd", first: 1 },
};

const dialog: SheetCoverage = {
  "dialog|dialog-content||": { select: ".mb-dialog" },
  "dialog|dialog-header||": { select: ".mb-dialog-header" },
  "dialog|dialog-title||": { select: ".mb-dialog-title" },
  "dialog|dialog-description||": { select: ".mb-dialog-description" },
  "dialog|input||": { select: ".mb-dialog input" },
  "dialog|dialog-footer||": { select: ".dialog-actions" },
  "dialog|button|outline|default": { select: '.dialog-actions .mb-btn[data-variant="secondary"]' },
  "dialog|button|default|default": { select: '.dialog-actions .mb-btn[data-variant="primary"]' },
  "dialog|dialog-close|ghost|icon-sm": { na: "Modbit dialogs close by Escape or an explicit action; they have no corner close button" },
};

const alertDialog: SheetCoverage = {
  "|alert-dialog-overlay||": { select: ".mb-scrim", ignore: ["text"] },
  "alert-dialog|alert-dialog-content||": { select: ".mb-dialog" },
  "alert-dialog|alert-dialog-header||": { select: ".mb-dialog-header" },
  "alert-dialog|alert-dialog-title||": { select: ".mb-dialog-title" },
  "alert-dialog|alert-dialog-description||": { select: ".mb-dialog-description" },
  "alert-dialog|alert-dialog-footer||": { select: ".dialog-actions" },
  "alert-dialog|alert-dialog-cancel||": { select: '.dialog-actions .mb-btn[data-variant="secondary"]' },
  "alert-dialog|alert-dialog-action||": { select: '.dialog-actions .mb-btn[data-variant="primary"]' },
};

/** Sheets of the reference capture and the sheet of Modbit's gallery that stands for each. */
export const COVERAGE: Record<string, { modbitSheet: string; keys: SheetCoverage }> = {
  buttons: { modbitSheet: "buttons", keys: buttons() },
  fields: { modbitSheet: "fields", keys: fields },
  surfaces: { modbitSheet: "surfaces", keys: surfaces },
  menu: { modbitSheet: "menu", keys: menu },
  select: { modbitSheet: "select", keys: select },
  popover: { modbitSheet: "select", keys: popover },
  tooltip: { modbitSheet: "tooltip", keys: tooltip },
  dialog: { modbitSheet: "dialog", keys: dialog },
  "alert-dialog": { modbitSheet: "alert-dialog", keys: alertDialog },
};
