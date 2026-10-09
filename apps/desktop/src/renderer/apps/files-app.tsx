import { useCallback, useEffect, useState } from "react";
import type { WorkspaceEntry } from "../../preload/preload.ts";
import { formatBytes, refusalText, withheldText } from "./files-model.ts";

type Dir = { entries: WorkspaceEntry[]; truncated: boolean } | { error: string };

/**
 * The Files app (REQ-PX-048, AFW-I01): a read-only browser of the task's
 * workspace. Every listing and every file is a typed Core read through the
 * Workspace File Service's path policy; the renderer never composes a path the
 * Core has not listed, and a refusal (outside the workspace, protected, binary,
 * too large) is shown in the Core's terms instead of an empty pane.
 */
export function FilesApp({ taskId, refreshKey }: { taskId: string; refreshKey: string }) {
  const [dirs, setDirs] = useState<Record<string, Dir>>({});
  const [open, setOpen] = useState<Record<string, boolean>>({ "": true });
  const [file, setFile] = useState<{ path: string; text: string | null; note: string | null; language: string } | null>(null);

  const loadDir = useCallback(
    async (path: string) => {
      try {
        const l = await window.modbit.listWorkspaceDir(taskId, path);
        setDirs((d) => ({ ...d, [path]: { entries: l.entries, truncated: l.truncated } }));
      } catch (e) {
        setDirs((d) => ({ ...d, [path]: { error: refusalText((e as Error).message) } }));
      }
    },
    [taskId],
  );
  // A different task starts at its own root; the open directories are read again when the task moves.
  useEffect(() => {
    setDirs({});
    setOpen({ "": true });
    setFile(null);
  }, [taskId]);
  useEffect(() => {
    for (const [p, isOpen] of Object.entries(open)) if (isOpen) void loadDir(p);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [taskId, refreshKey]);

  const toggle = (path: string) => {
    const next = !open[path];
    setOpen((o) => ({ ...o, [path]: next }));
    if (next && !dirs[path]) void loadDir(path);
  };
  const show = async (path: string) => {
    try {
      const v = await window.modbit.readWorkspaceFile(taskId, path);
      setFile({ path: v.path, text: v.status === "TEXT" ? v.text : null, note: withheldText(v.status, v.size), language: v.language });
    } catch (e) {
      setFile({ path, text: null, note: refusalText((e as Error).message), language: "" });
    }
  };

  const tree = (path: string, depth: number) => {
    const d = dirs[path];
    if (!d) return <li className="meta">Reading…</li>;
    if ("error" in d) {
      return (
        <li className="meta" role="status" data-testid="files-error">
          {d.error}
        </li>
      );
    }
    return (
      <>
        {d.entries.map((e) => (
          <li key={e.path} style={{ paddingLeft: depth * 12 }} data-testid="files-entry" data-path={e.path} data-kind={e.kind}>
            {e.kind === "dir" && !e.protected ? (
              <>
                <button type="button" className="files-entry" aria-expanded={open[e.path] === true} data-testid="files-dir" onClick={() => toggle(e.path)}>
                  {open[e.path] ? "▾" : "▸"} {e.name}
                </button>
                {open[e.path] && <ul className="files-tree">{tree(e.path, depth + 1)}</ul>}
              </>
            ) : e.kind === "file" && !e.protected ? (
              <button type="button" className="files-entry" data-testid="files-file" aria-pressed={file?.path === e.path} onClick={() => void show(e.path)}>
                {e.name} <span className="meta">{formatBytes(e.size)}</span>
              </button>
            ) : (
              <span className="files-entry meta" data-testid="files-withheld" title={e.protected ? "protected by the workspace's path policy" : e.kind}>
                {e.name} {e.protected ? "(protected, not opened)" : `(${e.kind})`}
              </span>
            )}
          </li>
        ))}
        {d.truncated && <li className="meta">The listing is cut at its bound; open a subdirectory to see the rest.</li>}
      </>
    );
  };

  return (
    <div className="files-app" data-testid="app-files">
      <ul className="files-tree" aria-label="Workspace files" data-testid="files-tree">
        {tree("", 0)}
      </ul>
      {file && (
        <section aria-label={`File ${file.path}`} data-testid="files-viewer" data-path={file.path}>
          <p className="meta">
            {file.path}
            {file.language ? ` · ${file.language}` : ""} · read-only
          </p>
          {file.note ? (
            <p className="meta" role="status" data-testid="files-note">
              {file.note}
            </p>
          ) : (
            <pre className="codeview" data-testid="files-text">
              {file.text}
            </pre>
          )}
        </section>
      )}
    </div>
  );
}
