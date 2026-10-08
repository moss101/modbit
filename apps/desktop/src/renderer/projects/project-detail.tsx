/**
 * A project's page (REQ-PX-064): its record, the rollup the Core computed
 * from the members' own headers, and its tasks. The page is a read of the
 * Core's projection; the only things it does are Core commands: rename,
 * archive and its undo, and taking a task out. Counts are never summed here
 * and a pull request or CI line appears only when the log holds it.
 */
import { useId, useState } from "react";
import { Badge, Button, StatusDot } from "@modbit/ui";
import type { ProjectInfo } from "../../shared/project-types.ts";
import { CLASS_META } from "../agents/list-model.ts";
import { archiveProject } from "./archive-project.ts";
import { RenameProjectDialog } from "./project-dialogs.tsx";
import { ProjectGlyph } from "./project-glyph.tsx";
import { ciWords, memberCiWords, refusalSentence, rollupWords } from "./project-model.ts";
import type { ProjectsState } from "./use-projects.ts";

export interface ProjectDetailProps {
  project: ProjectInfo | null;
  loaded: boolean;
  sessionId: string | null;
  projects: ProjectsState;
  onOpenTask: (taskId: string) => void;
  announce: (message: string) => void;
}

export function ProjectDetail({ project, loaded, sessionId, projects, onOpenTask, announce }: ProjectDetailProps) {
  const [renaming, setRenaming] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const titleId = useId();
  if (!project) {
    return (
      <section className="proj-page" aria-label="Project" data-testid="project-page" data-state={loaded ? "missing" : "loading"}>
        <p className="meta" role="status">
          {loaded ? "That project no longer exists." : "Reading the project…"}
        </p>
      </section>
    );
  }
  const r = project.rollup;
  const ci = ciWords(r);
  const remove = async (taskId: string, title: string) => {
    if (!sessionId) return;
    try {
      await projects.removeMember(sessionId, project.projectId, taskId);
      setNote(null);
      announce(`Removed “${title}” from “${project.name}”.`);
    } catch (e) {
      setNote(refusalSentence(`Could not remove “${title}”`, e));
    }
  };
  return (
    <section className="proj-page" aria-labelledby={titleId} data-testid="project-page" data-project-id={project.projectId} data-archived={project.archived} data-state="ready">
      <header className="proj-head">
        <ProjectGlyph icon={project.icon} color={project.color} size={24} />
        <h2 id={titleId} className="proj-title" data-testid="project-title">
          {project.name}
        </h2>
        {project.archived && <Badge tone="warn">Archived</Badge>}
        <span className="proj-actions">
          {!project.archived && (
            <Button size="sm" onClick={() => setRenaming(true)} data-testid="project-page-rename">
              Rename
            </Button>
          )}
          <Button
            size="sm"
            variant={project.archived ? "primary" : "secondary"}
            data-testid={project.archived ? "project-page-restore" : "project-page-archive"}
            onClick={() => {
              if (sessionId) void archiveProject(projects, sessionId, project.projectId, project.name, !project.archived, { onError: setNote, onChanged: () => setNote(null) });
            }}
          >
            {project.archived ? "Restore project" : "Archive project"}
          </Button>
        </span>
      </header>
      <p className="meta proj-workspace" data-testid="project-workspace-root">
        Workspace: {project.workspaceRoot}
      </p>
      {note && (
        <p className="proj-problem" role="status" data-testid="project-note">
          {note}
        </p>
      )}
      <section className="proj-rollup" aria-label={`Rollup of ${project.name}`} data-testid="project-rollup">
        <p data-testid="project-rollup-words">{rollupWords(r)}</p>
        <ul className="proj-counts" aria-label="Tasks by status">
          {r.byStatus.map((s) => (
            <li key={s.statusClass} data-testid="project-count" data-class={s.statusClass} data-count={s.count}>
              <StatusDot status={CLASS_META[s.statusClass].status} label={s.label} />
              <span>
                {s.count} {s.label.toLowerCase()}
              </span>
            </li>
          ))}
        </ul>
        <dl className="proj-facts">
          <div>
            <dt>Needs attention</dt>
            <dd data-testid="project-attention">{r.attentionTasks === 0 ? "none" : `${r.attentionTasks} ${r.attentionTasks === 1 ? "task" : "tasks"}, ${r.attentionItems} ${r.attentionItems === 1 ? "item" : "items"}`}</dd>
          </div>
          <div>
            <dt>Approvals waiting</dt>
            <dd data-testid="project-approvals">{r.pendingApprovals}</dd>
          </div>
          <div>
            <dt>Unread</dt>
            <dd data-testid="project-unread">{r.unread}</dd>
          </div>
          <div>
            <dt>Pull requests</dt>
            <dd data-testid="project-prs">{r.pullRequests === 0 ? "none recorded" : `${r.pullRequests}${ci ? ` (${ci})` : ""}`}</dd>
          </div>
        </dl>
      </section>
      <section aria-label={`Tasks of ${project.name}`} data-testid="project-members">
        <h3 className="proj-h2">Tasks</h3>
        {project.members.length === 0 ? (
          <p className="meta empty" data-testid="project-empty">
            No task is in this project. Use a task&apos;s project menu in the list, or drag a task onto the project.
          </p>
        ) : (
          <ul className="proj-members">
            {project.members.map((m) => (
              <li key={m.taskId} className="proj-member" data-testid="project-member" data-task-id={m.taskId} data-class={m.statusClass}>
                <StatusDot status={CLASS_META[m.statusClass].status} label={m.statusLabel || CLASS_META[m.statusClass].short} />
                <button type="button" className="proj-member-open" onClick={() => onOpenTask(m.taskId)} data-testid="project-member-open">
                  {m.title || "Untitled task"}
                </button>
                <span className="meta" data-testid="project-member-status">
                  {m.statusLabel || CLASS_META[m.statusClass].short}
                  {m.attentionItems > 0 ? ` · ${m.attentionItems} ${m.attentionItems === 1 ? "item needs" : "items need"} you` : ""}
                  {m.pullRequest ? ` · PR #${m.pullRequest.number} (${memberCiWords(m.pullRequest)})` : ""}
                </span>
                {!project.archived && (
                  <Button size="sm" onClick={() => void remove(m.taskId, m.title || "the task")} data-testid="project-member-remove" aria-label={`Remove ${m.title || "the task"} from the project`}>
                    Remove
                  </Button>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>
      <RenameProjectDialog
        project={renaming ? project : null}
        onClose={() => setRenaming(false)}
        onRename={async (patch) => {
          if (!sessionId) return;
          await projects.rename(sessionId, project.projectId, patch);
          setRenaming(false);
        }}
      />
    </section>
  );
}
