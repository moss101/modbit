import { Button, Dialog, Kbd } from "@modbit/ui";
import type { CommandRegistry } from "@modbit/ui/logic";
import { SHORTCUTS } from "../keyboard.ts";
import type { ShellActions } from "./commands.ts";

/**
 * The visible shortcut list (AFW-A16): the registry's chords grouped as the
 * palette groups them, then the PX-024 context keys. Both come from data (the
 * registry and `SHORTCUTS`), so the help cannot drift from the bindings.
 */
export function ShortcutHelp({ open, onClose, registry }: { open: boolean; onClose: () => void; registry: CommandRegistry<ShellActions> }) {
  const bound = registry.all().filter((d) => (d.keys?.length ?? 0) > 0);
  const groups = [...new Set(bound.map((d) => d.group))];
  return (
    <Dialog open={open} title="Keyboard shortcuts" onClose={onClose} size="lg" testId="help">
      {groups.map((g) => (
        <section key={g} aria-label={g}>
          <h3 className="help-group">{g}</h3>
          <ul className="help-list">
            {bound
              .filter((d) => d.group === g)
              .map((d) => (
                <li key={d.id} data-testid="help-shortcut" data-command={d.id}>
                  <span>{d.title}</span>
                  <span>
                    {d.keys!.map((k) => (
                      <Kbd key={k} chord={k} />
                    ))}
                  </span>
                </li>
              ))}
          </ul>
        </section>
      ))}
      <section aria-label="Lists and review">
        <h3 className="help-group">Lists and review</h3>
        <ul className="help-list">
          {SHORTCUTS.map((s) => (
            <li key={s.command + s.keys}>
              <span>{s.what}</span>
              <Kbd>{s.keys}</Kbd>
            </li>
          ))}
        </ul>
      </section>
      <div className="dialog-actions">
        <Button onClick={onClose}>Close</Button>
      </div>
    </Dialog>
  );
}
