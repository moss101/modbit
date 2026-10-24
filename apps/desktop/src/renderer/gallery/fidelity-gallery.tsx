/**
 * The fidelity specimen sheets (DR-PX-2026-10-24-016 section 6, PX-142): Modbit's real primitives and native controls laid out
 * as the specimens of ZCode's reference capture (evidence/zcode-ui/reference), with the same labels, so the fidelity spec can probe
 * both with one function and compare element by element. Present only in a build made with MODBIT_GALLERY=1 and reached at
 * `#fidelity/<sheet>/<theme>`; a QA aid that never counts as proof of a behaviour (AFW-K02, check G11).
 */
import { useEffect, type ReactNode } from "react";
import { Badge, Button, Dialog, Icon, IconButton, Kbd, Menu, Tabs, Tooltip, type BadgeTone, type ButtonSize, type ButtonVariant, type MenuItem } from "@modbit/ui";

/** [ZCode name, Modbit name]: ZCode's default, outline, secondary, ghost and destructive buttons. */
const VARIANTS: readonly (readonly [string, ButtonVariant])[] = [["default", "primary"], ["outline", "secondary"], ["secondary", "subtle"], ["ghost", "ghost"], ["destructive", "danger"]];
const SIZES: readonly (readonly [string, ButtonSize])[] = [["xs", "xs"], ["sm", "sm"], ["default", "md"], ["lg", "lg"]];
/** ZCode's icon-xs, icon-sm, icon and icon-lg and the pixel size of the glyph each draws. */
const ICON_SIZES: readonly (readonly [string, ButtonSize, number])[] = [["icon-xs", "xs", 10], ["icon-sm", "sm", 12], ["icon", "md", 16], ["icon-lg", "lg", 16]];
const BADGES: readonly (readonly [string, BadgeTone])[] = [["default", "accent"], ["secondary", "neutral"], ["destructive", "danger"], ["outline", "outline"]];

const noop = () => {};

function Buttons() {
  return (
    <>
      <section data-zdl="button-matrix" className="zdl-sheet">
        {VARIANTS.map(([z, v]) => (
          <div key={z} className="zdl-row" data-row={z}>
            {SIZES.map(([zs, s]) => (
              <Button key={zs} variant={v} size={s}>
                {z}/{zs}
              </Button>
            ))}
            <Button variant={v}>
              <Icon name="plus" data-icon="inline-start" /> With icon
            </Button>
            <Button variant={v} disabled>
              disabled
            </Button>
          </div>
        ))}
        <div className="zdl-row" data-row="icon-ghost">
          {ICON_SIZES.map(([z, s, px]) => (
            <IconButton key={z} label={z} variant="ghost" size={s} icon={<Icon name="settings" size={px} />} />
          ))}
        </div>
        <div className="zdl-row" data-row="icon-outline">
          {ICON_SIZES.map(([z, s, px]) => (
            <IconButton key={z} label={z} variant="secondary" size={s} icon={<Icon name="trash-2" size={px} />} />
          ))}
        </div>
      </section>
      <section data-zdl="badge" className="zdl-sheet">
        <div className="zdl-row">
          {BADGES.map(([z, tone]) => (
            <Badge key={z} tone={tone}>
              {z}
            </Badge>
          ))}
        </div>
      </section>
    </>
  );
}

function Fields() {
  return (
    <>
      <section data-zdl="fields" className="zdl-sheet zdl-fields">
        <label className="mb-label" htmlFor="zdl-input">
          Label
        </label>
        <input id="zdl-input" placeholder="Placeholder" />
        <input defaultValue="Filled value" aria-label="Filled" />
        <input disabled defaultValue="Disabled" aria-label="Disabled" />
        <input aria-invalid="true" defaultValue="Invalid" aria-label="Invalid" />
        <textarea placeholder="Textarea placeholder" aria-label="Textarea" />
        <div className="zdl-row">
          <input type="checkbox" defaultChecked aria-label="checked" />
          <input type="checkbox" aria-label="unchecked" />
        </div>
      </section>
      <section data-zdl="tabs" className="zdl-sheet">
        <div style={{ width: 448 }}>
          <Tabs label="Specimen" idPrefix="zdl" selected="one" onSelect={noop} tabs={[{ id: "one", label: "One" }, { id: "two", label: "Two" }, { id: "three", label: "Three", disabled: true }]}>
            Tab content one
          </Tabs>
        </div>
      </section>
    </>
  );
}

function Surfaces() {
  return (
    <section data-zdl="surfaces" className="zdl-sheet" style={{ width: 768 }}>
      <div className="mb-card">
        <div className="mb-card-header">
          <div className="mb-card-title">Card title</div>
          <div className="mb-card-description">Card description text</div>
        </div>
        <div className="mb-card-content">Card content in text-ui-base</div>
        <div className="mb-card-footer">
          <Button size="sm" variant="primary">Action</Button>
        </div>
      </div>
      <div className="zdl-row">
        <span className="mb-kbd-group">
          <Kbd>⌘</Kbd>
          <Kbd>K</Kbd>
        </span>
      </div>
    </section>
  );
}

const MENU_ITEMS: readonly MenuItem[] = [
  { id: "new", label: "New task", shortcut: "mod+n", onSelect: noop },
  { id: "rename", label: "Rename", onSelect: noop },
  { id: "disabled", label: "Disabled", disabled: true, onSelect: noop },
  { id: "checked", label: "Checked item", checked: true, onSelect: noop },
  { id: "delete", label: "Delete", tone: "destructive", separatorBefore: true, onSelect: noop },
];

function MenuSheet() {
  return (
    <section data-zdl="menu" className="zdl-sheet" style={{ minHeight: 360 }}>
      <Menu label="Menu" triggerVariant="secondary" trigger={<>Menu <Icon name="chevron-down" /></>} items={MENU_ITEMS} />
    </section>
  );
}

function SelectSheet() {
  return (
    <section data-zdl="select" className="zdl-sheet">
      <select aria-label="Model" defaultValue="b" style={{ width: 224 }}>
        <option value="a">Alpha</option>
        <option value="b">Beta</option>
        <option value="c">Gamma</option>
      </select>
    </section>
  );
}

function TooltipSheet() {
  return (
    <section data-zdl="tooltip" className="zdl-sheet" style={{ padding: 48 }}>
      <Tooltip text="Tooltip text" shortcut="mod+k">
        <IconButton label="Settings" variant="ghost" icon={<Icon name="settings" />} />
      </Tooltip>
    </section>
  );
}

function DialogSheet() {
  return (
    <Dialog open title="Dialog title" description="Dialog description text that explains the action." onClose={noop} size="md" testId="zdl-dialog">
      <input placeholder="Field" aria-label="Field" />
      <div className="dialog-actions">
        <Button variant="secondary">Cancel</Button>
        <Button variant="primary">Confirm</Button>
      </div>
    </Dialog>
  );
}

function AlertDialogSheet() {
  return (
    <Dialog open role="alertdialog" title="Delete this task?" description="This cannot be undone." onClose={noop} size="sm" testId="zdl-alert-dialog">
      <div className="dialog-actions">
        <Button variant="secondary">Cancel</Button>
        <Button variant="primary">Delete</Button>
      </div>
    </Dialog>
  );
}

const SHEETS: Record<string, () => ReactNode> = {
  buttons: Buttons,
  fields: Fields,
  surfaces: Surfaces,
  menu: MenuSheet,
  select: SelectSheet,
  tooltip: TooltipSheet,
  dialog: DialogSheet,
  "alert-dialog": AlertDialogSheet,
};

export const FIDELITY_SHEETS = Object.keys(SHEETS);

/** `#fidelity/<sheet>/<theme>`: one sheet on the page surface ZCode's specimens sit on, in light or dark. */
export function FidelityGallery() {
  const [, sheet = "buttons", theme = "light"] = location.hash.slice(1).split("/");
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);
  const Sheet = SHEETS[sheet] ?? Buttons;
  return (
    <div className="zdl-page" data-testid="fidelity" data-sheet={sheet} data-theme-name={theme}>
      <Sheet />
    </div>
  );
}
