# WIN-08 packaged suite — per-spec results (against the INSTALLED build)

| Spec | Result | Duration |
|---|---|---|
| approvals.spec.ts :: PX-058: a pending effect docks a card with the exact intent and the Core's reason; a stale | passed | 11.2s |
| approvals.spec.ts :: PX-058: three pending approvals form a deck, oldest first, each decided on its own; a bare | passed | 15.6s |
| approvals.spec.ts :: PX-058: an effect that cannot be undone asks twice, and a key typed into a text field neve | passed | 8.1s |
| approvals.spec.ts :: PX-057/058: Allowlist plus Always on a card makes a durable rule through the Core; the sam | passed | 10.1s |
| approvals.spec.ts :: PX-057: the widest preset needs its warning and still asks for the always-ask classes; eve | passed | 8.4s |
| approvals.spec.ts :: PX-057/058: the main process rejects invalid arguments before the Core sees them | passed | 1.7s |
| conversation.spec.ts :: PX-047: a streamed answer appears incrementally, is never shown as final while open, only  | passed | 4.5s |
| conversation.spec.ts :: PX-047: a reload in the middle of a stream resumes it: the partial text comes back and the | passed | 5.9s |
| conversation.spec.ts :: PX-047: killing the Core mid-stream shows the response as stopped, with its cause, never a | passed | 5.6s |
| conversation.spec.ts :: PX-047: scrolling up freezes follow, the pill counts later messages, the jump returns to t | passed | 5.4s |
| conversation.spec.ts :: PX-047: tool steps fold by density without changing the rows; a question is answered inlin | passed | 14.1s |
| packaged-update.spec.ts :: a build with no pinned key verifies nothing and offers no update | skipped | 0.0s |
| packaged-update.spec.ts :: hostile feeds are refused with their own reason and nothing is staged | skipped | 0.0s |
| packaged-update.spec.ts :: the device policy's minimum_version is honored from the Core's own policy reader | skipped | 0.0s |
| packaged-update.spec.ts :: a SIGKILL during the download leaves nothing that looks verified, and the next check succe | skipped | 0.0s |
| packaged-update.spec.ts :: an install that cannot complete leaves the previous app in place and says why | skipped | 0.0s |
| packaged-update.spec.ts :: a signed update installs, the new version runs, and a rollback restores the old one | skipped | 0.0s |
| packaged.spec.ts :: the packaged app starts the Core it carries and reports it connected | passed | 1.3s |
| perf-budgets.spec.ts :: PERF PX-041/PX-047: a streamed delta renders within 100 ms of its event timestamp at p95,  | passed | 126.4s |
| perf-budgets.spec.ts :: PERF PX-046: the agent list reflects a Core event within 150 ms at p95 with a 200-task lis | passed | 77.5s |
| perf-budgets.spec.ts :: PERF PX-047: a long transcript scrolls; the frame distribution is recorded (the dossier na | passed | 248.7s |
| supervision.spec.ts :: home: the mode and the pin reach the Core before the first model call; a blocked model is  | passed | 6.3s |
| supervision.spec.ts :: home: a mode the Core refuses leaves the task created, unstarted and saying why; no model  | passed | 2.5s |
| supervision.spec.ts :: home: the location is a selector that names this machine and says why cloud cannot be chos | passed | 2.3s |
| supervision.spec.ts :: a stopped task says what is wrong and what to do on its row and in its conversation, with  | passed | 14.5s |
| supervision.spec.ts :: a state nothing else on the screen says stands above the composer: a Core that dies while  | passed | 3.3s |
| supervision.spec.ts :: MOD-UX-001: each state a task can be in is a first-class view, holding the Core's class, o | passed | 10.8s |
| supervision.spec.ts :: PX-196/197: Escape with the details open closes them, not the card; a confirmation keeps i | passed | 4.9s |
| supervision.spec.ts :: PX-196: after a reload the details show the facts the Core folded from the log, not defaul | passed | 7.2s |

Total: 29 specs; stats: {'startTime': '2026-11-13T02:49:17.452Z', 'duration': 598176.6529999999, 'expected': 23, 'skipped': 6, 'unexpected': 0, 'flaky': 0}
