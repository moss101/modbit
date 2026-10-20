/**
 * The model-free states of projects, worktrees and the apply-back modal
 * (REQ-PX-063, 064, 068) for design review and for the accessibility pass.
 * Fixtures only: no Core and no model are behind anything here, and a gallery
 * state is never evidence that a behaviour works (projects.spec.ts and
 * worktrees.spec.ts prove the behaviour against a real Core).
 */
import { useRef, useState } from "react";
import type { ApplyAckInfo, ApplyConflictInfo, ProjectInfo, WorktreeInfo, WorktreeListInfo } from "../../shared/project-types.ts";
import { AgentListView } from "../agents/agent-list-view.tsx";
import { DEFAULT_LIST_PREFS } from "../agents/prefs.ts";
import { NewProjectDialog, RenameProjectDialog, SaveViewDialog } from "../projects/project-dialogs.tsx";
import { ProjectDetail } from "../projects/project-detail.tsx";
import type { ProjectsState } from "../projects/use-projects.ts";
import { ApproveStep, ConflictStep, DoneStep } from "../worktrees/apply-modal.tsx";
import { stepOf } from "../worktrees/apply-model.ts";
import { RemoveBody, type RemoveState } from "../worktrees/remove-dialog.tsx";
import { WorktreesPanel } from "../worktrees/worktrees-panel.tsx";
import type { WorktreesState } from "../worktrees/use-worktrees.ts";
import { AGENT_HEADER_FIXTURES, GALLERY_NOW } from "./workspace-fixtures.ts";
import { Button } from "@modbit/ui";

const roll = (over: Partial<ProjectInfo["rollup"]> = {}): ProjectInfo["rollup"] => ({ members: 0, byStatus: [], attentionTasks: 0, attentionItems: 0, pendingApprovals: 0, unread: 0, pullRequests: 0, ciPassing: 0, ciFailing: 0, ciPending: 0, ...over });

const member = (i: number, over: Partial<ProjectInfo["members"][number]> = {}): ProjectInfo["members"][number] => {
  const h = AGENT_HEADER_FIXTURES[i]!;
  return { taskId: h.taskId, sessionId: "fixture", title: h.title, statusClass: h.statusClass, statusLabel: h.statusLabel, unread: h.unread, pendingApproval: h.pendingApproval, attentionItems: h.attentionItems, taskState: h.taskState, archived: h.archived, addedAtMs: GALLERY_NOW - 60_000, updatedAtMs: h.updatedAtMs, pullRequest: null, ...over };
};

const RELEASE: ProjectInfo = {
  projectId: "fixture-project-1",
  name: "Release 2",
  color: "warn",
  icon: "rocket",
  workspaceRoot: "/work/example",
  archived: false,
  createdAtMs: GALLERY_NOW - 86_400_000,
  updatedAtMs: GALLERY_NOW - 3_600_000,
  createdOffset: "3",
  lastOffset: "20",
  rollup: roll({
    members: 3,
    byStatus: [
      { statusClass: "NEEDS_ATTENTION", label: "Needs attention", count: 1 },
      { statusClass: "READY_FOR_REVIEW_UNSEEN", label: "Ready for review", count: 1 },
      { statusClass: "RUNNING", label: "Running", count: 1 },
    ],
    attentionTasks: 1,
    attentionItems: 1,
    pendingApprovals: 1,
    unread: 1,
    pullRequests: 2,
    ciPassing: 1,
    ciFailing: 1,
  }),
  members: [
    member(0),
    member(2, { pullRequest: { number: "42", url: "https://example.invalid/o/r/pull/42", head: "task/export", state: "open", hasCi: true, checksPassed: 3, checksFailed: 0, checksPending: 0 } }),
    member(4, { pullRequest: { number: "43", url: "https://example.invalid/o/r/pull/43", head: "task/parser", state: "", hasCi: true, checksPassed: 1, checksFailed: 2, checksPending: 0 } }),
  ],
};
const OLD: ProjectInfo = { ...RELEASE, projectId: "fixture-project-2", name: "Old experiments", color: "info", icon: "leaf", archived: true, rollup: roll({ members: 0 }), members: [] };
const EMPTY: ProjectInfo = { ...RELEASE, projectId: "fixture-project-3", name: "Docs", color: "ok", icon: "book", rollup: roll(), members: [] };

const noop = async () => {
  throw new Error("The gallery has no Core.");
};
const PROJECTS_STATE = (projects: ProjectInfo[]): ProjectsState => ({ projects, loaded: true, error: null, refresh: () => {}, reload: async () => {}, create: noop, rename: noop, archive: noop, addMember: noop, removeMember: noop });

const wt = (i: number, over: Partial<WorktreeInfo> = {}): WorktreeInfo => ({ worktreeId: `fixture-wt-${i}`, taskId: AGENT_HEADER_FIXTURES[i]!.taskId, kind: "TASK", path: `/data/worktrees/fixture-${i}`, originRoot: "/work/example", branch: `modbit/task-fixture-${i}`, baseRevision: "abc123", state: "ACTIVE", disposition: "", dirty: false, unapplied: false, changedFiles: 0, bytes: "184320", createdAtMs: GALLERY_NOW - 5 * 3_600_000, lastActivityMs: GALLERY_NOW - 3_600_000, taskRunning: false, orphan: false, protected: false, removable: false, removableReason: "", setupStatus: "", setupDetail: "", neutralized: [], ...over });

const WORKTREES: WorktreeInfo[] = [
  wt(4, { taskRunning: true, protected: true, removableReason: "its task has not ended", changedFiles: 2, dirty: true }),
  wt(2, { taskRunning: true, dirty: true, unapplied: true, changedFiles: 3, bytes: "5242880", removableReason: "its task has not ended" }),
  wt(6, { dirty: true, unapplied: true, changedFiles: 1, removableReason: "it is dirty and its result is not applied, merged or discarded" }),
  wt(0, { removable: true, removableReason: "its result was applied (the checkout already holds every change) and has not changed since", disposition: "APPLIED" }),
  wt(7, { removable: false, protected: true, removableReason: "younger than the protection window", createdAtMs: GALLERY_NOW - 120_000 }),
  wt(8, { taskId: "", orphan: true, originRoot: "", worktreeId: "fixture-orphan", kind: "", branch: "", path: "/data/worktrees/orphan", removableReason: "not a Git worktree Modbit can inspect" }),
];
const WORKTREE_LIST: WorktreeListInfo = {
  worktrees: WORKTREES,
  count: WORKTREES.length,
  totalBytes: "6000000",
  policy: { maxWorktrees: 25, maxBytes: String(50 * 1024 ** 3), protectMs: 600_000, hysteresisPercent: 80 },
  lastCleanup: { runId: "r1", owner: "core-1", reason: "scheduled", status: "SKIPPED", skippedReason: "another cleanup held the lease (core-1)", scanned: 0, removed: 0, bytesFreed: "0", errors: [], removedIds: [], kept: [], attention: [], startedAtMs: GALLERY_NOW - 3_600_000, finishedAtMs: GALLERY_NOW - 3_600_000, remaining: 0, remainingBytes: "0", holderOwner: "core-1", holderReason: "requested" },
  attention: ["worktree fixture-wt-6 (TASK): it is dirty and its result is not applied, merged or discarded; ApplyWorktree or DiscardWorktree decides it"],
  nextCleanupAtMs: GALLERY_NOW + 6 * 3_600_000,
  lastCompletedAtMs: GALLERY_NOW - 7 * 3_600_000,
};
const WORKTREES_STATE: WorktreesState = { list: WORKTREE_LIST, loaded: true, error: null, refresh: async () => {} };

const CONFLICT: ApplyConflictInfo = {
  planDigest: "d".repeat(64),
  candidateTree: "t".repeat(40),
  checkoutHead: "h".repeat(40),
  candidateRevision: "7",
  paths: [
    { path: "src/parser.rs", change: "MODIFY", state: "CONFLICT", conflict: "BOTH_MODIFIED", protected: false, markersAvailable: true },
    { path: "docs/NOTES.md", change: "MODIFY", state: "CONFLICT", conflict: "BOTH_MODIFIED", protected: false, markersAvailable: true },
    { path: "src/new/export.rs", change: "ADD", state: "CLEAN", conflict: "", protected: false, markersAvailable: false },
    { path: ".github/workflows/ci.yml", change: "MODIFY", state: "CLEAN", conflict: "", protected: true, markersAvailable: false },
  ],
  conflictingPaths: ["docs/NOTES.md", "src/parser.rs"],
  protectedPaths: [".github/workflows/ci.yml"],
  dirtyPaths: ["scratch.txt"],
  options: [
    { option: "CANCEL", destructive: false, needsConfirmation: false, confirmPaths: [], available: true, whyUnavailable: "" },
    { option: "MERGE_MANUALLY", destructive: false, needsConfirmation: true, confirmPaths: [".github/workflows/ci.yml"], available: true, whyUnavailable: "" },
    { option: "STASH", destructive: false, needsConfirmation: true, confirmPaths: [".github/workflows/ci.yml"], available: false, whyUnavailable: "the task's change still conflicts with the committed checkout" },
    { option: "OVERWRITE", destructive: true, needsConfirmation: true, confirmPaths: [".github/workflows/ci.yml", "docs/NOTES.md", "src/parser.rs"], available: true, whyUnavailable: "" },
    { option: "FULL_OVERWRITE", destructive: true, needsConfirmation: true, confirmPaths: [".github/workflows/ci.yml", "docs/NOTES.md", "scratch.txt", "src/parser.rs"], available: true, whyUnavailable: "" },
    { option: "UNDO_AND_APPLY", destructive: false, needsConfirmation: false, confirmPaths: [], available: false, whyUnavailable: "no earlier apply of this worktree is in force" },
  ],
  defaultOption: "CANCEL",
  rememberedOption: "",
};
const ack = (over: Partial<ApplyAckInfo> = {}): ApplyAckInfo => ({ status: "APPLIED", applyId: "ap-1", approvalId: "approval", intentHash: "e".repeat(64), planDigest: "d".repeat(64), candidateRevision: "7", candidateTree: "t", checkoutHeadBefore: "h", conflict: null, appliedPaths: ["src/new/export.rs", "src/parser.rs"], unresolvedPaths: [], effectReceiptIds: ["r1"], detail: "", code: "", option: "OVERWRITE", stashRef: "", replayed: false, divergentPaths: [], ...over });

const REFUSAL: RemoveState = { phase: "refused", refusal: { code: "WORKTREE_NEEDS_DECISION", detail: "it is dirty and its result is not applied, merged or discarded; apply, merge or discard its result first" } };
const RUNNING_REFUSAL: RemoveState = { phase: "refused", refusal: { code: "TASK_RUNNING", detail: "its task has not ended" } };
const CONFIRM: RemoveState = { phase: "confirm", preview: { worktreeId: "fixture-wt-0", removed: false, dryRun: true, branch: "modbit/task-fixture-0", path: "/data/worktrees/fixture-0", bytes: "184320", reason: "its result was applied and has not changed since", loses: ["the worktree directory /data/worktrees/fixture-0 (184320 bytes)", "the branch modbit/task-fixture-0, which belonged to the worktree", "nothing of the result: it was applied and has not changed since"], offset: "0" } };

export function ProjectsGallery() {
  const [newOpen, setNewOpen] = useState(false);
  const [renaming, setRenaming] = useState<ProjectInfo | null>(null);
  const [saving, setSaving] = useState(false);
  const [log, setLog] = useState<string[]>([]);
  const note = (t: string) => setLog((l) => [...l.slice(-5), t]);
  const cancel = useRef<HTMLButtonElement>(null);
  const projects = [RELEASE, EMPTY, OLD];
  const headers = AGENT_HEADER_FIXTURES.map((h, i) => (i === 0 || i === 2 || i === 4 ? { ...h, projectId: RELEASE.projectId } : h));
  return (
    <section aria-labelledby="g-projects" data-testid="gallery-projects">
      <h2 id="g-projects">Projects, worktrees and apply-back</h2>
      <p className="meta">Fixtures only: no Core is behind any of this, and nothing here proves a behaviour.</p>

      <h3>Agent list with the Projects group, a project grouping and a stored view</h3>
      <div className="gallery-shell-box" style={{ width: 320, height: 560 }} data-testid="gallery-projects-list">
        <AgentListView
          headers={headers}
          loaded
          error={null}
          selectedId={null}
          pins={[]}
          grouping="repository"
          filters={{ ...DEFAULT_LIST_PREFS.filters, showArchived: true }}
          subtitleFields={DEFAULT_LIST_PREFS.subtitle}
          collapsed={new Set()}
          nowMs={GALLERY_NOW}
          query=""
          search={{ state: "idle", results: null, error: null }}
          note="Could not add “Plan the release notes” to “Release 2”: A draft cannot join a project yet. a draft task cannot join a project; start it first"
          onQuery={() => {}}
          onSelect={() => {}}
          onPin={() => {}}
          onUnpin={() => {}}
          onArchive={() => {}}
          onToggleSection={() => {}}
          onGrouping={() => {}}
          onFilters={() => {}}
          onSubtitleFields={() => {}}
          projects={projects}
          sort="recent"
          onSort={() => {}}
          viewActions={{ views: [{ id: "v1", name: "Release by title", grouping: "project", sort: "title", filters: { ...DEFAULT_LIST_PREFS.filters, projects: [RELEASE.projectId] } }], onApply: (v) => note(`view ${v.name}`), onSaveRequest: () => setSaving(true), onDelete: () => {} }}
          projectActions={{ selectedId: RELEASE.projectId, onNew: () => setNewOpen(true), onOpen: (id) => note(`open ${id}`), onRename: (id) => setRenaming(projects.find((p) => p.projectId === id) ?? null), onArchive: () => {}, onRestore: () => {}, onAdd: () => {}, onRemove: () => {} }}
        />
      </div>
      <div className="gallery-shell-box" style={{ width: 320, height: 420 }} data-testid="gallery-projects-grouped">
        <AgentListView
          headers={headers}
          loaded
          error={null}
          selectedId={null}
          pins={[]}
          grouping="project"
          filters={DEFAULT_LIST_PREFS.filters}
          subtitleFields={DEFAULT_LIST_PREFS.subtitle}
          collapsed={new Set()}
          nowMs={GALLERY_NOW}
          query=""
          search={{ state: "idle", results: null, error: null }}
          note={null}
          onQuery={() => {}}
          onSelect={() => {}}
          onPin={() => {}}
          onUnpin={() => {}}
          onArchive={() => {}}
          onToggleSection={() => {}}
          onGrouping={() => {}}
          onFilters={() => {}}
          onSubtitleFields={() => {}}
          projects={projects}
          sort="status"
          onSort={() => {}}
        />
      </div>
      <p className="meta" role="status" data-testid="gallery-projects-log">
        {log.join(" · ")}
      </p>
      <div className="gallery-row">
        <Button size="sm" data-testid="gallery-open-new-project" onClick={() => setNewOpen(true)}>
          New project dialog
        </Button>
        <Button size="sm" data-testid="gallery-open-save-view" onClick={() => setSaving(true)}>
          Save view dialog
        </Button>
      </div>
      <NewProjectDialog open={newOpen} workspaces={["/work/example", "/work/other"]} onCreate={async () => note("create")} onClose={() => setNewOpen(false)} />
      <RenameProjectDialog project={renaming} onRename={async () => note("rename")} onClose={() => setRenaming(null)} />
      <SaveViewDialog open={saving} onSave={() => null} onClose={() => setSaving(false)} />

      <h3>Project page: a project with members, pull requests and CI, and an empty one</h3>
      <div className="gallery-card" data-testid="gallery-project-page">
        <ProjectDetail project={RELEASE} loaded sessionId={null} projects={PROJECTS_STATE(projects)} onOpenTask={() => {}} announce={() => {}} />
      </div>
      <div className="gallery-card" data-testid="gallery-project-empty">
        <ProjectDetail project={EMPTY} loaded sessionId={null} projects={PROJECTS_STATE(projects)} onOpenTask={() => {}} announce={() => {}} />
      </div>
      <div className="gallery-card" data-testid="gallery-project-archived">
        <ProjectDetail project={OLD} loaded sessionId={null} projects={PROJECTS_STATE(projects)} onOpenTask={() => {}} announce={() => {}} />
      </div>

      <h3>Worktree list</h3>
      <div className="gallery-card" data-testid="gallery-worktrees">
        <WorktreesPanel state={WORKTREES_STATE} sessionId={null} titleOf={(id) => AGENT_HEADER_FIXTURES.find((h) => h.taskId === id)?.title ?? "another task"} taskStateOf={(id) => AGENT_HEADER_FIXTURES.find((h) => h.taskId === id)?.taskState} nowMs={GALLERY_NOW} onOpenTask={() => {}} announce={() => {}} />
      </div>

      <h3>Removal: refused for a running one, refused for an unapplied one, and what a removal would take</h3>
      <div className="gallery-card" data-testid="gallery-remove-running">
        <RemoveBody worktree={WORKTREES[0]!} owner="Fix the flaky parser test" state={RUNNING_REFUSAL} onCancel={() => {}} onConfirm={() => {}} />
      </div>
      <div className="gallery-card" data-testid="gallery-remove-unapplied">
        <RemoveBody worktree={WORKTREES[2]!} owner="Rename the config key" state={REFUSAL} onCancel={() => {}} onConfirm={() => {}} />
      </div>
      <div className="gallery-card" data-testid="gallery-remove-confirm">
        <RemoveBody worktree={WORKTREES[3]!} owner="Needs attention" state={CONFIRM} cancelRef={cancel} onCancel={() => {}} onConfirm={() => {}} />
      </div>

      <h3>Apply-back modal: the conflict plan, an approval, the result, and a stop</h3>
      <div className="gallery-card apply" data-testid="gallery-apply-conflict">
        <ConflictStep step={{ kind: "conflict", ack: ack({ status: "CONFLICT", conflict: CONFLICT }), conflict: CONFLICT, refusal: "Overwrite conflicting files needs the files typed to confirm it: docs/NOTES.md, src/parser.rs" }} onChoose={() => {}} onCancel={() => {}} />
      </div>
      <div className="gallery-card apply" data-testid="gallery-apply-approve">
        <ApproveStep step={{ kind: "approve", flow: "apply", ack: ack({ status: "APPROVAL_PENDING", option: "MERGE_MANUALLY", detail: "decide the approval, then call ApplyWorktree again with the same arguments" }), request: {} }} worktree={WORKTREES[2]!} onDecide={() => {}} />
      </div>
      <div className="gallery-card apply" data-testid="gallery-apply-done">
        {(() => {
          const s = stepOf(ack({ unresolvedPaths: ["src/parser.rs"], stashRef: "refs/modbit/stash/1" }), "apply", {});
          return s.kind === "done" ? <DoneStep step={s} onUndo={() => {}} onReapply={() => {}} /> : null;
        })()}
      </div>
    </section>
  );
}
