/**
 * Landmark names in split view (REQ-PX-204): two tasks side by side each draw a transcript, a composer, notices and trays, and a
 * landmark's name must be unique in the page (axe landmark-unique). A conversation in split view gives its subtree its task's
 * title, and each landmark inside says whose it is ("Composer: Add the release remotes"). With one task nothing is added.
 */
import { createContext, useContext } from "react";

export const LandmarkTaskContext = createContext<string>("");

/** Names a landmark for the task around it: the label alone with one task, "label: task" in split view. */
export function useLandmarkName(): (label: string) => string {
  const task = useContext(LandmarkTaskContext);
  return (label) => (task ? `${label}: ${task}` : label);
}
