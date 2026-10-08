/**
 * @modbit/ui — Modbit's React primitives and the registries behind them
 * (REQ-PX-044): Button, IconButton, Menu, Dialog, TrayHost, Tooltip, Tabs,
 * List/Row, StatusDot, Badge, Kbd, plus the layer stack (Escape closes the
 * innermost layer), scoped key bindings, the typed command registry, the tray
 * store and the palette ranking. The renderer holds no authority (docs/81):
 * nothing here reaches the preload bridge. Canonical owner: desktop (docs/12).
 */
export * from "./logic.ts";
export { Button, IconButton, type ButtonProps, type ButtonSize, type ButtonVariant, type IconButtonProps } from "./button.tsx";
export { Badge, Kbd, STATUS_TEXT, StatusDot, type BadgeTone, type Status, type StatusDotProps } from "./status.tsx";
export { Tooltip, type TooltipProps } from "./tooltip.tsx";
export { Menu, type MenuItem, type MenuProps } from "./menu.tsx";
export { Dialog, type DialogProps } from "./dialog.tsx";
export { Tabs, type TabDef, type TabsProps } from "./tabs.tsx";
export { List, Row, type RowProps } from "./list.tsx";
export { TrayHost } from "./tray.tsx";
export { focusables, useEscapeLayers, useFocusTrap, useKeyDispatch, useKeyScope, useLayer, useOutsidePress } from "./hooks.ts";
export { uiCss } from "./styles.ts";
