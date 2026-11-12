/**
 * The plan of the open task as the Core holds it (REQ-PX-200; DR-PX-2026-11-11-021 MWS-06): the work graph's steps with
 * their state (GetWorkGraph) and the plan version in force (GetPlan), read through the two typed bridge reads and read again
 * whenever the task's log moves. Nothing is taken from the transcript's text, and nothing is kept between reads: a reload or
 * a Core restart finds the same card because the Core serves it.
 */
import { useEffect, useRef, useState } from "react";
import type { PlanInfo, WorkGraphInfo } from "../../shared/conversation-types.ts";

export interface PlanData {
  graph: WorkGraphInfo | null;
  plan: PlanInfo | null;
  /** The task the answers are for (a card is never drawn from another task's graph). */
  taskId: string | null;
}

const NONE: PlanData = { graph: null, plan: null, taskId: null };

/** `logKey` changes when the task's log does (the transcript's last offset): each change is one read, and a read in flight is followed by one more. */
export function usePlan(taskId: string | null, connected: boolean, logKey: string): PlanData {
  const [data, setData] = useState<PlanData>(NONE);
  const want = useRef(taskId);
  want.current = taskId;
  const inFlight = useRef(false);
  const again = useRef(false);

  useEffect(() => setData(NONE), [taskId]);
  useEffect(() => {
    if (!taskId || !connected) return;
    const load = async (): Promise<void> => {
      if (inFlight.current) {
        again.current = true;
        return;
      }
      inFlight.current = true;
      try {
        const [graph, plan] = await Promise.all([window.modbit.workGraph(taskId), window.modbit.plan(taskId)]);
        if (want.current === taskId) setData({ graph, plan, taskId });
      } catch {
        // An unreachable Core: the last answer stays, as everywhere else.
      } finally {
        inFlight.current = false;
        if (again.current) {
          again.current = false;
          if (want.current === taskId) void load();
        }
      }
    };
    void load();
  }, [taskId, connected, logKey]);
  return data.taskId === taskId ? data : NONE;
}
