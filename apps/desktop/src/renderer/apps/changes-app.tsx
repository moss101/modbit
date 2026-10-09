import { useCallback, useEffect, useRef, useState } from "react";
import { Badge, Button } from "@modbit/ui";
import type { CodeView, ReviewBundleView, ReviewFileView } from "../../preload/preload.ts";
import { changeSummary, isReviewable, lineKind, rangesContain } from "./changes-model.ts";

/**
 * The Changes app (REQ-PX-048, PX-104's reachability half): the task's live,
 * revision-bound diff in every task state, not only when it is ready for
 * review. The file list and hunks are the Core's review bundle; a file's full
 * text with its changed lines marked is the Core's code view (`codeView`,
 * through the same typed bridge). Decisions stay where the Core accepts them:
 * `DecideReview` takes a decision only on a task that is ready for review, so
 * earlier than that this panel shows the diff and says so; the per-hunk
 * controls are the existing Review screen's, one button away.
 */
export function ChangesApp({ taskId, taskState, refreshKey, onOpenReview }: { taskId: string; taskState: string; refreshKey: string; onOpenReview: () => void }) {
  const [bundle, setBundle] = useState<ReviewBundleView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [path, setPath] = useState<string | null>(null);
  const [full, setFull] = useState<CodeView | null>(null);
  const [fullError, setFullError] = useState<string | null>(null);
  const seq = useRef(0);

  const load = useCallback(async () => {
    const mine = ++seq.current;
    try {
      const b = await window.modbit.reviewBundle(taskId);
      if (mine !== seq.current) return;
      setBundle(b);
      setError(null);
    } catch (e) {
      if (mine === seq.current) setError((e as Error).message);
    }
  }, [taskId]);
  // The diff follows the task: it is read again whenever the task moves (the caller's refresh key) and on demand.
  useEffect(() => {
    void load();
  }, [load, refreshKey]);
  useEffect(() => {
    setPath(null);
    setFull(null);
  }, [taskId]);

  const file: ReviewFileView | undefined = bundle?.files.find((f) => f.path === path) ?? bundle?.files[0];
  const showFull = async () => {
    if (!file) return;
    setFullError(null);
    try {
      setFull(await window.modbit.codeView(taskId, file.path, file.fileRevision));
    } catch (e) {
      setFull(null);
      setFullError((e as Error).message);
    }
  };

  if (error) {
    return (
      <p className="meta" role="status" data-testid="changes-error">
        The change set could not be read: {error}
      </p>
    );
  }
  if (!bundle) {
    return (
      <p className="meta" role="status" data-testid="changes-loading">
        Reading the change set…
      </p>
    );
  }
  return (
    <div className="changes-app" data-testid="app-changes" data-task-state={taskState} data-workspace-revision={bundle.workspaceRevision}>
      <div className="changes-head">
        <span data-testid="changes-summary">{changeSummary(bundle.files)}</span>
        <Badge tone={isReviewable(taskState) ? "ok" : "info"}>{taskState}</Badge>
        <Button size="sm" onClick={() => void load()} data-testid="changes-refresh">
          Refresh
        </Button>
        <Button size="sm" variant="primary" onClick={onOpenReview} data-testid="panel-open-review">
          {isReviewable(taskState) ? "Open review" : "Open review (read-only)"}
        </Button>
      </div>
      <p className="meta" data-testid="changes-note">
        {isReviewable(taskState)
          ? "This task is ready for review: accept or return it, hunk by hunk, in the review."
          : "The task is still working. This is its change set as of workspace revision " + bundle.workspaceRevision + "; a decision on a hunk is taken once the task is ready for review."}
      </p>
      {bundle.files.length === 0 ? (
        <p className="meta empty" data-testid="changes-empty">
          No file has changed yet.
        </p>
      ) : (
        <>
          <ul className="changes-files" aria-label="Changed files" data-testid="changes-files">
            {bundle.files.map((f) => (
              <li key={f.path}>
                <button type="button" className="changes-file" aria-pressed={f.path === file?.path} data-testid="changes-file" data-path={f.path} onClick={() => { setPath(f.path); setFull(null); setFullError(null); }}>
                  <span className="changes-status">{f.status}</span> {f.path}
                  <span className="meta"> · {f.binary ? "binary" : `${f.hunks.length} ${f.hunks.length === 1 ? "hunk" : "hunks"}`}</span>
                </button>
              </li>
            ))}
          </ul>
          {file && (
            <section aria-label={`Diff of ${file.path}`} data-testid="changes-diff" data-path={file.path}>
              {file.binary ? (
                <p className="meta">Binary file: its change is not shown as text.</p>
              ) : (
                file.hunks.map((h) => (
                  <div key={h.index} className="hunk" data-testid="changes-hunk">
                    <div className="hunk-head">{h.header}</div>
                    <pre>
                      {h.lines.map((l, i) => (
                        <span key={i} className={lineKind(l)}>
                          {l}
                          {"\n"}
                        </span>
                      ))}
                    </pre>
                  </div>
                ))
              )}
              {!file.binary && file.status !== "D" && (
                <Button size="sm" onClick={() => void showFull()} data-testid="changes-show-full">
                  Show the full file
                </Button>
              )}
              {fullError && (
                <p className="meta" role="status" data-testid="changes-full-error">
                  {fullError}
                </p>
              )}
              {full && (
                <div data-testid="changes-full" data-stale={full.stale ? "true" : "false"} data-file-revision={full.fileRevision}>
                  <p className="meta">
                    {full.path} · {full.syntaxLanguage} · file revision {full.fileRevision.slice(0, 12)}
                    {full.stale ? " · the file changed since the diff was read; refresh" : ""}
                  </p>
                  {full.text === "" ? (
                    <p className="meta">The file is too large to show inline; its content is held by the Core as {full.contentRef.slice(0, 12)}.</p>
                  ) : (
                    <pre className="codeview">
                      {full.text.split("\n").map((line, i) => (
                        <span key={i} className={rangesContain(full.changedRanges, i + 1) ? "add" : "ctx"} data-changed={rangesContain(full.changedRanges, i + 1) ? "true" : "false"}>
                          {String(i + 1).padStart(4, " ")}  {line}
                          {"\n"}
                        </span>
                      ))}
                    </pre>
                  )}
                </div>
              )}
            </section>
          )}
        </>
      )}
    </div>
  );
}
