/**
 * The dialogs of the project surfaces (REQ-PX-064): a new project, a rename
 * (with colour and icon), and saving the list's current query as a view. Each
 * asks for a few typed fields and hands them back; the Core validates the
 * name, colour and icon and its typed refusal is shown here as the Core said
 * it, in the dialog, so the person can correct it without losing what they
 * typed.
 */
import { useEffect, useId, useRef, useState } from "react";
import { Button, Dialog } from "@modbit/ui";
import { PROJECT_COLORS, PROJECT_ICONS, type ProjectInfo } from "../../shared/project-types.ts";
import { ProjectGlyph } from "./project-glyph.tsx";
import { refusalSentence } from "./project-model.ts";

const COLOR_WORDS: Record<string, string> = { accent: "Accent", ok: "Green", warn: "Amber", danger: "Red", info: "Blue", modeAsk: "Teal", modePlan: "Violet", modeDebug: "Orange" };
const ICON_WORDS: Record<string, string> = { folder: "Folder", star: "Star", flag: "Flag", bolt: "Bolt", book: "Book", bug: "Bug", rocket: "Rocket", leaf: "Leaf" };

const baseName = (p: string): string => p.split(/[\\/]+/).filter(Boolean).pop() ?? p;

function Appearance({ color, icon, onColor, onIcon }: { color: string; icon: string; onColor: (c: string) => void; onIcon: (i: string) => void }) {
  return (
    <div className="proj-appearance">
      <label className="proj-field">
        <span>Colour</span>
        <select value={color} onChange={(e) => onColor(e.target.value)} data-testid="project-color">
          {PROJECT_COLORS.map((c) => (
            <option key={c} value={c}>
              {COLOR_WORDS[c] ?? c}
            </option>
          ))}
        </select>
      </label>
      <label className="proj-field">
        <span>Icon</span>
        <select value={icon} onChange={(e) => onIcon(e.target.value)} data-testid="project-icon">
          {PROJECT_ICONS.map((i) => (
            <option key={i} value={i}>
              {ICON_WORDS[i] ?? i}
            </option>
          ))}
        </select>
      </label>
      <span className="proj-preview" aria-hidden="true">
        <ProjectGlyph icon={icon} color={color} size={20} />
      </span>
    </div>
  );
}

export interface NewProjectDialogProps {
  open: boolean;
  workspaces: readonly string[];
  /** Resolves when the Core accepted it; rejects with the Core's refusal. */
  onCreate: (input: { name: string; color: string; icon: string; workspaceRoot: string }) => Promise<void>;
  onClose: () => void;
}

export function NewProjectDialog({ open, workspaces, onCreate, onClose }: NewProjectDialogProps) {
  const [name, setName] = useState("");
  const [workspace, setWorkspace] = useState(workspaces[0] ?? "");
  const [color, setColor] = useState("accent");
  const [icon, setIcon] = useState("folder");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const nameRef = useRef<HTMLInputElement>(null);
  const errId = useId();
  useEffect(() => {
    if (open) {
      setName("");
      setProblem(null);
      setBusy(false);
      setWorkspace((w) => (workspaces.includes(w) ? w : (workspaces[0] ?? "")));
    }
  }, [open, workspaces]);
  const submit = async () => {
    setBusy(true);
    setProblem(null);
    try {
      await onCreate({ name, color, icon, workspaceRoot: workspace });
    } catch (e) {
      setProblem(refusalSentence("Could not create the project", e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog open={open} title="New project" onClose={onClose} testId="project-new-dialog" initialFocus={nameRef}>
      <form
        className="proj-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (!busy) void submit();
        }}
      >
        <p className="meta">A project groups tasks of one workspace. It does not run anything; archiving it leaves its tasks as they are.</p>
        <label className="proj-field">
          <span>Name</span>
          <input ref={nameRef} type="text" value={name} maxLength={80} onChange={(e) => setName(e.target.value)} data-testid="project-name" autoComplete="off" spellCheck={false} aria-describedby={problem ? errId : undefined} />
        </label>
        <label className="proj-field">
          <span>Workspace</span>
          <select value={workspace} onChange={(e) => setWorkspace(e.target.value)} data-testid="project-workspace">
            {workspaces.map((w) => (
              <option key={w} value={w}>
                {baseName(w)} — {w}
              </option>
            ))}
          </select>
        </label>
        <Appearance color={color} icon={icon} onColor={setColor} onIcon={setIcon} />
        {problem && (
          <p id={errId} className="proj-problem" role="alert" data-testid="project-problem">
            {problem}
          </p>
        )}
        <div className="dialog-actions">
          <Button type="button" onClick={onClose} data-testid="project-new-cancel">
            Cancel
          </Button>
          <Button type="submit" variant="primary" disabled={busy || name.trim().length === 0 || workspace === ""} data-testid="project-new-create">
            Create project
          </Button>
        </div>
      </form>
    </Dialog>
  );
}

export interface RenameProjectDialogProps {
  project: ProjectInfo | null;
  onRename: (patch: { name: string; color: string; icon: string }) => Promise<void>;
  onClose: () => void;
}

export function RenameProjectDialog({ project, onRename, onClose }: RenameProjectDialogProps) {
  const [name, setName] = useState("");
  const [color, setColor] = useState("accent");
  const [icon, setIcon] = useState("folder");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const nameRef = useRef<HTMLInputElement>(null);
  const errId = useId();
  useEffect(() => {
    if (project) {
      setName(project.name);
      setColor(project.color);
      setIcon(project.icon);
      setProblem(null);
      setBusy(false);
    }
  }, [project]);
  const submit = async () => {
    setBusy(true);
    setProblem(null);
    try {
      await onRename({ name, color, icon });
    } catch (e) {
      setProblem(refusalSentence("Could not change the project", e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog open={project !== null} title="Rename project" onClose={onClose} testId="project-rename-dialog" initialFocus={nameRef}>
      <form
        className="proj-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (!busy) void submit();
        }}
      >
        <label className="proj-field">
          <span>Name</span>
          <input ref={nameRef} type="text" value={name} maxLength={80} onChange={(e) => setName(e.target.value)} data-testid="project-rename-name" autoComplete="off" spellCheck={false} aria-describedby={problem ? errId : undefined} />
        </label>
        <Appearance color={color} icon={icon} onColor={setColor} onIcon={setIcon} />
        {problem && (
          <p id={errId} className="proj-problem" role="alert" data-testid="project-problem">
            {problem}
          </p>
        )}
        <div className="dialog-actions">
          <Button type="button" onClick={onClose} data-testid="project-rename-cancel">
            Cancel
          </Button>
          <Button type="submit" variant="primary" disabled={busy || name.trim().length === 0} data-testid="project-rename-save">
            Save
          </Button>
        </div>
      </form>
    </Dialog>
  );
}

export interface SaveViewDialogProps {
  open: boolean;
  /** Returns the reason it cannot be saved, or null when it was. */
  onSave: (name: string) => string | null;
  onClose: () => void;
}

export function SaveViewDialog({ open, onSave, onClose }: SaveViewDialogProps) {
  const [name, setName] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const ref = useRef<HTMLInputElement>(null);
  const errId = useId();
  useEffect(() => {
    if (open) {
      setName("");
      setProblem(null);
    }
  }, [open]);
  return (
    <Dialog open={open} title="Save view" onClose={onClose} testId="view-save-dialog" initialFocus={ref} size="sm">
      <form
        className="proj-form"
        onSubmit={(e) => {
          e.preventDefault();
          const why = onSave(name);
          if (why) setProblem(why);
        }}
      >
        <p className="meta">A view keeps the list&apos;s grouping, filters and sort under a name. It is kept on this computer, for you.</p>
        <label className="proj-field">
          <span>Name</span>
          <input ref={ref} type="text" value={name} maxLength={40} onChange={(e) => setName(e.target.value)} data-testid="view-name" autoComplete="off" spellCheck={false} aria-describedby={problem ? errId : undefined} />
        </label>
        {problem && (
          <p id={errId} className="proj-problem" role="alert" data-testid="view-problem">
            {problem}
          </p>
        )}
        <div className="dialog-actions">
          <Button type="button" onClick={onClose} data-testid="view-save-cancel">
            Cancel
          </Button>
          <Button type="submit" variant="primary" disabled={name.trim().length === 0} data-testid="view-save">
            Save view
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
