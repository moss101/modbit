/**
 * The model-free states of the Automations surface (REQ-PX-086) for design
 * review and the accessibility pass: the empty list, the list with every state
 * and every attention kind, the editor with the Core's issues, the enable
 * approval of a write-capable definition, a repository definition that is not
 * enabled and one whose file changed, the history with every typed reason, a
 * finished dry run with what it would have asked, and the kill confirmation.
 * Fixtures only: no Core and no model are behind anything here, and a gallery
 * state is never evidence that a behaviour works (automations.spec.ts proves
 * those against a real Core).
 */
import { useMemo, useState } from "react";
import { Button } from "@modbit/ui";
import { Automations } from "../automations/automations.tsx";
import { EnableDialog, KillDialog, TestRunDialog } from "../automations/dialogs.tsx";
import { ALL_STATES, ATTENTION, BAD_DEFINITION, EMPTY_LIST, FIXTURE_LIST, HISTORY, REPO_CHANGED, REPO_DEF, WRITER, fixtureCalls } from "./automations-fixtures.ts";
import { GALLERY_NOW } from "./workspace-fixtures.ts";

export function AutomationsGallery({ note }: { note: (text: string) => void }) {
  const [enableOpen, setEnableOpen] = useState(false);
  const [killOpen, setKillOpen] = useState(false);
  const [testOpen, setTestOpen] = useState(false);
  const open = (id: string) => note(`automations: open task ${id.slice(0, 8)}`);
  const calls = useMemo(
    () => ({
      empty: fixtureCalls(EMPTY_LIST, []),
      states: fixtureCalls(FIXTURE_LIST, HISTORY),
      repo: fixtureCalls({ ...EMPTY_LIST, automations: [REPO_DEF], attention: [] }, []),
      changed: fixtureCalls({ ...EMPTY_LIST, automations: [REPO_CHANGED], attention: ATTENTION.filter((a) => a.kind === "SOURCE_CHANGED") }, []),
      history: fixtureCalls({ ...EMPTY_LIST, automations: [ALL_STATES[0]!] }, HISTORY),
    }),
    [],
  );
  const drift = ALL_STATES[0]!;
  return (
    <section aria-labelledby="g-automations" data-testid="gallery-automations">
      <h2 id="g-automations">Automations</h2>
      <p className="meta">Fixtures only: the empty list, every definition state, every attention kind, the editor with the Core&apos;s issues, the approval of a write-capable definition, repository definitions, the history with every typed reason, a dry run&apos;s report and the kill confirmation.</p>

      <h3>Empty list</h3>
      <div className="gallery-card" data-testid="gallery-automations-empty">
        <Automations calls={calls.empty} connected onOpenTask={open} initial={{ labelSuffix: "empty list" }} />
      </div>

      <h3>List with every state and every kind of attention</h3>
      <div className="gallery-card" data-testid="gallery-automations-states">
        <Automations calls={calls.states} connected onOpenTask={open} initial={{ labelSuffix: "every state" }} />
      </div>

      <h3>Editor with the Core&apos;s issues</h3>
      <div className="gallery-card" data-testid="gallery-automations-editor">
        <Automations calls={calls.empty} connected onOpenTask={open} initial={{ editor: { automationId: null, text: BAD_DEFINITION, workspaceRoot: "/work/example" }, workspace: "/work/example", labelSuffix: "editor" }} />
      </div>

      <h3>A repository definition that is not enabled</h3>
      <div className="gallery-card" data-testid="gallery-automations-repo">
        <Automations calls={calls.repo} connected onOpenTask={open} initial={{ selectedId: REPO_DEF.automationId, labelSuffix: "repository, not enabled" }} />
      </div>

      <h3>A repository definition whose file changed after it was approved</h3>
      <div className="gallery-card" data-testid="gallery-automations-changed">
        <Automations calls={calls.changed} connected onOpenTask={open} initial={{ selectedId: REPO_CHANGED.automationId, labelSuffix: "repository, changed" }} />
      </div>

      <h3>History with every typed reason</h3>
      <div className="gallery-card" data-testid="gallery-automations-history">
        <Automations calls={calls.history} connected onOpenTask={open} initial={{ selectedId: drift.automationId, labelSuffix: "history" }} />
      </div>

      <h3>Dialogs</h3>
      <div className="gallery-row">
        <Button data-testid="gallery-open-enable" onClick={() => setEnableOpen(true)}>
          Approval of a write-capable definition
        </Button>
        <Button data-testid="gallery-open-test" onClick={() => setTestOpen(true)}>
          A finished dry run
        </Button>
        <Button data-testid="gallery-open-kill" onClick={() => setKillOpen(true)}>
          Kill confirmation
        </Button>
      </div>
      <EnableDialog snapshot={enableOpen ? WRITER : null} sourceRevision="" calls={calls.states} onClose={() => setEnableOpen(false)} onEnabled={() => setEnableOpen(false)} onReview={() => setEnableOpen(false)} />
      <TestRunDialog
        view={testOpen ? { ...drift, triggers: [{ id: "pr", kind: "event", summary: "forge pull_request (opened)", nextDueMs: 0 }] } : null}
        runs={HISTORY}
        calls={calls.states}
        onClose={() => setTestOpen(false)}
        onRefresh={() => {}}
        onOpenTask={open}
        fixed={{ started: { dispatchKey: HISTORY[13]!.dispatchKey, status: "running", reason: "", detail: "", taskId: HISTORY[13]!.taskId, wouldSkipByFilter: false } }}
      />
      <KillDialog target={killOpen ? { automationId: drift.automationId, name: drift.name } : null} calls={calls.states} onClose={() => setKillOpen(false)} onDone={() => note(`automations: killed at ${new Date(GALLERY_NOW).toISOString()}`)} />
    </section>
  );
}
