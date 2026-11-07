# Graph audit of the desktop rows, 2026-11-07

Requested by the owner on 2026-11-07 ("Yes, correct the graph"), after the comparison of the app with its documents found that the graph could be trusted in neither direction. Main at d4c5458a.

## Method and limits

- Five read-only audits under docs/84, on a clean worktree of d4c5458a. `reports/` holds each one in full. They read code and tests and ran nothing.
- 29 unfinished rows: the 26 open rows owned by `desktop`, plus PX-042, PX-066 and PX-103, which the desktop rows stand on. 32 rows at COMPLETE: the 31 owned by `desktop`, plus M8.8. The COMPLETE rows were checked at three depths, and report E says which got which.
- The coordinator spot-checked one claim for each of PX-047, 054, 058, 094, 095, 096, 100, 103, 104, 128 and 043 in the code, and they held, except one clause about which CI job sets a variable (PX-100), which was dropped. The other rows' claims rest on their report.
- `repro.log`, from `repro.spec.ts.txt`, is one local run on macOS arm64 of the renderer as merged, with a real Core and a scripted model. It is the only thing run, and it covers only the contradictions marked reproduced below.

## The graph tool's rule decides most of this

`graph.py set` refuses any state above NOT_STARTED while a prerequisite is not COMPLETE (docs/93), and every open desktop row has such a prerequisite. PX-041 is the root of most of it, held at REAL_TESTING by a live-gateway run and a render budget. So none of the 28 NOT_STARTED rows here can move, and each keeps its status with a dated audit note. That status now means "not closable", not "no code". The audit found code for 24 of them.

## Unfinished rows: class and first missing link

The note on each row in the graph carries the evidence paths and other gaps.

| Class | Rows |
|---|---|
| PRODUCTION-WORKING | none |
| IMPLEMENTED-PARTIAL | PX-025, 042, 046, 047, 048, 054, 055, 056, 058, 060, 062, 064, 066, 068, 074, 080, 086, 093, 094, 096, 100, 103, 104, 127, 128 |
| SCAFFOLDED | PX-031 |
| DOCUMENTED-ONLY | PX-079, 095, 097 |

The earlier look's "17 look built" was right that code exists and wrong as a closure claim: every row has at least one unproven or missing clause. No packaged build exists, so every clause asking for a packaged app is unmet. PX-080 is partial only by reuse of other rows, and PX-048's agent-terminal clause on its own is BROKEN-DRIFTED.

## COMPLETE rows contradicted

| Row | Clause | Basis | Graph action |
|---|---|---|---|
| PX-197 | Escape must not deny an effect the person meant to step back from | reproduced, T1 | moved back to REAL_TESTING |
| PX-197 | A step is in the past tense only once it happened | reproduced, T1: "Ran git clean -fd" on a denied command | moved back to REAL_TESTING |
| PX-196 | The details show each fact for the open task | reproduced, T2: drafting and 0/0 after a reload, awaiting human and 0/1 live | moved back to REAL_TESTING |
| IMP-EV-0152 | UI reload derives all task state from Core APIs | reproduced, T2 | kept COMPLETE with a note: moving it takes ALPHA and BETA off READY and raises 66 integrity findings |
| PX-043 | A client writing to an agent-owned terminal is rejected by the Core | code read; a spec asserts that typing works | kept COMPLETE with a note: moving it fails G4 (PX-099, PX-132 are COMPLETE), and it needs an owner decision |

PX-195 stands with a note: its "every fact is reachable" clause shares the reload defect. M8.8 stands: the desktop's rendering of the stream is outside its scope.

## Not done

- Nothing was fixed. The reopened rows wait on product changes.
- **No row owns the desktop's rendering of the cloud browser stream.** It needs a new row or a Decision Record adding it to PX-128.
- Rows whose specs #169 rewrote still cite runs from before it: IMP-EV-0037, 0143, 0151, 0152, 0275 and PX-010, 022, 023, 024. Run 41983983308 passed on the rewritten specs. Nobody checked spec by spec that they still prove each clause, so that run is not appended.
- Packaged clauses on rows already COMPLETE (PX-022, 024, 044, 045, 049, 106, 141, 142) ran on an unpackaged build. No record decides whether that counts as production-equivalent.
- Report E's section 3 lists smaller findings with no row, not reproduced here, for example `y` and `x` doing nothing after a reload.
- The non-desktop open rows were not audited.
