import { test } from "node:test";
import assert from "node:assert/strict";
import { CommandRegistry, type CommandDef } from "@modbit/ui/logic";
import { shellCommandDefs, shellRegistry, type ShellActions } from "../shell/commands.ts";
import { commandFor, rowKeyAction, type Command } from "../keyboard.ts";
import { shortcutSheet, type ShortcutSheet } from "./shortcut-sheet.ts";

const resolveWith = (r: CommandRegistry<ShellActions>) => (e: Parameters<CommandRegistry<ShellActions>["forEvent"]>[0], mac: boolean) => r.forEvent(e, mac)?.id;
const sheet = (mac: boolean): ShortcutSheet => shortcutSheet(shellRegistry.all() as CommandDef<never>[], resolveWith(shellRegistry), mac);

for (const mac of [true, false]) {
  test(`REQ-PX-208: the Shortcuts section is the registry, binding for binding (${mac ? "macOS" : "Windows and Linux"})`, () => {
    const s = sheet(mac);
    assert.deepEqual(s.unbound, [], "every chord the registry declares is answered by the registry");
    const listed = s.rows.filter((r) => r.scope === "anywhere" && !r.id.startsWith("key-")).map((r) => `${r.id}=${r.probes.map((p) => p.chord).join(",")}`).sort();
    const declared = shellCommandDefs()
      .filter((d) => (d.keys?.length ?? 0) > 0)
      .map((d) => `${d.id}=${d.keys!.join(",")}`)
      .sort();
    assert.deepEqual(listed, declared);
  });

  test(`REQ-PX-208: every key listed is answered by the map that dispatches it, and every key that map answers is listed (${mac ? "macOS" : "Windows and Linux"})`, () => {
    const s = sheet(mac);
    for (const r of s.rows) {
      assert.ok(r.probes.length > 0, r.id);
      for (const p of r.probes) {
        if (r.id.startsWith("row:")) assert.equal(rowKeyAction(p), r.id.slice(4), `${r.id} ${p.chord}`);
        else if (r.id.startsWith("key-")) {
          const cmd = r.id.split(":")[1] as Command;
          const ctx = { editable: false, screen: r.scope === "review" ? ("review" as const) : ("fleet" as const), confirming: r.scope === "confirming", isMac: mac };
          assert.equal(commandFor(p, ctx), cmd, `${r.id} ${p.chord}`);
          if (r.scope !== "confirming") assert.equal(shellRegistry.forEvent(p, mac), undefined, `${p.chord} is the registry's, not ${r.id}`);
        } else assert.equal(shellRegistry.forEvent(p, mac)?.id, r.id, `${r.id} ${p.chord}`);
      }
    }
    const commands = new Set(s.rows.filter((r) => r.id.startsWith("key-")).map((r) => r.id.split(":")[1]));
    // Every command of the key map that has a key of its own is listed (newTask's chord is the registry's).
    for (const c of ["search", "help", "jumpAttention", "jumpRunning", "approve", "deny", "cancelTask", "steerTask", "next", "previous", "open", "back", "hunkNext", "hunkPrevious", "hunkToggle", "toggleSplit", "jumpFailing", "confirm"]) assert.ok(commands.has(c), c);
    assert.ok(!commands.has("newTask"), "mod+N is listed once, as the registry's New task");
    const rowActions = new Set(s.rows.filter((r) => r.scope === "row").map((r) => r.id));
    for (const a of ["down", "up", "first", "last", "pageDown", "pageUp", "preview", "pin", "archive"]) assert.ok(rowActions.has(`row:${a}`), a);
  });
}

test("REQ-PX-208: a chord the registry declares but does not answer is not listed, and is reported (mutation)", () => {
  const reg = new CommandRegistry<ShellActions>();
  reg.register({ id: "x.bound", title: "Bound", group: "G", keys: ["mod+shift+m"], run: () => {} });
  // A resolver that has lost the chord: the key map no longer answers it.
  const s = shortcutSheet(reg.all() as CommandDef<never>[], () => undefined, true);
  assert.deepEqual(s.unbound, ["x.bound: mod+shift+m"]);
  assert.equal(s.rows.some((r) => r.id === "x.bound"), false);
});

test("REQ-PX-208: the list's key handler and the Shortcuts section read one row key map", () => {
  const k = (key: string, mods: Partial<{ ctrlKey: boolean; metaKey: boolean; altKey: boolean }> = {}) => ({ key, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...mods });
  assert.equal(rowKeyAction(k(" ")), "preview");
  assert.equal(rowKeyAction(k("p")), "pin");
  assert.equal(rowKeyAction(k("E")), "archive");
  assert.equal(rowKeyAction(k("p", { metaKey: true })), null);
  assert.equal(rowKeyAction(k("ArrowDown", { metaKey: true })), "down");
});
