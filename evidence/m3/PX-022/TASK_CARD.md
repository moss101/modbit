# Task Card — PX-022 Onboarding to a first useful task within five minutes

## Identity

- Task ID: PX-022
- Milestone: M3 (moved from M2 by DR-M2-002); release: ALPHA
- Requirement: REQ-PX-022; owner label: desktop; subsystem: desktop
- Qualification: `QUAL-PX-022` / `PX-E2E-022` — Playwright drives the packaged app on fresh profiles through provider setup, repository trust and a starter task on a small real repository with a live provider test model; the median time to ReadyForReview with real evidence is within five minutes on reference hardware; the p90 is reported. Negative: an invalid key, a network failure and an untrusted repository each show the named cause and next action; a profile that skips provider setup cannot start a task; no step shows an outcome the Core has not persisted.
- Evidence tier: real-system or production-equivalent
- Decision Record: `docs/decisions/DR-M3-003-benchmark-and-conformance-live-model-halves.md` (the live-model measurement)

## Goal

From first launch of a fresh install to a first task at ReadyForReview with a real diff, a real test run and receipts, through the three steps docs/39 names — provider setup with a live test call, explicit scoped repository trust, a starter task for the detected stack — and the Review opening on the first result.

## Existing-code audit

- classification: ABSENT before this task. The desktop had a Fleet, a New Task composer and a Review screen (M1.4, M2.9), but no welcome, no way to set up a provider (the Core read keys from its environment only), no notion of repository trust anywhere in the product, and no starter tasks. A fresh install had nothing to do until someone set an environment variable.
- production entry points:
  - `services/modbit-core/src/onboarding.rs`, `ConfigureProvider`, `TrustRepository`, `ListStarterTasks` — the Core half. A credential handed to the Core is held in memory only: the command is not journaled, nothing writes it to the log, the object store or a file, and no view echoes it. `ProbeModel` is the live test call. A profile with no provider cannot start a task because the Core has no endpoint to start it on (`NO_PROVIDER`). Trust is a `RepositoryTrusted` event on the session, scoped to exactly the root the user named; a desktop task on an untrusted root does not start (`REPOSITORY_UNTRUSTED`, naming the next action). Starter tasks come from the stack the verification plan detects, or from the files when no build system declares one.
  - `crates/providers/src/gateway.rs` — endpoints can be configured and forgotten at runtime; an endpoint the live test call could not confirm is forgotten, credential included. An environment variable set to nothing registers nothing.
  - `apps/desktop/src/main/main.ts` — the key crosses Electron main once, from the renderer's input to `safeStorage` (the OS keychain's custody: Keychain on macOS, DPAPI on Windows, libsecret on Linux; only the ciphertext touches disk, mode 0600) and to the Core; it is never returned to the renderer, and a restarted Core is handed it again so a restart does not undo setup. When the OS offers no encryption the key is not saved and the user is told.
  - `apps/desktop/src/renderer/index.tsx` — the single-screen welcome with three steps, each a working control with its own empty, busy and error states; the starter gallery; the composer's explicit trust; the Review opening on the first `TaskReadyForReview`.
- proof: on three fresh profiles the real Electron app, driving the real Core against the wire-faithful scripted provider on a small real Git repository with a configured check, goes from first launch through provider setup (a live test call), scoped trust and a starter task to the Review open on a one-hunk diff with a COMPLETION verification run, with a median of 1.2 s and a p90 of 1.2 s on an Apple M5 Pro, against the 300 s budget. On a fresh profile with no provider a task can be created but not started, and the error names `NO_PROVIDER`; an invalid key is refused by the provider and the step says "rejected the key" and offers a retry; an unreachable endpoint says "could not be reached"; a repository that does not exist says so; and once the provider is confirmed the same task starts and reaches ReadyForReview. At the Core, the same rules are proven over the wire, and the credential is shown to be absent from every file under the profile after a run.

## Limitations

- The measurement is against the wire-faithful local provider, not a live test model: the model's own latency is what a live run adds, and DR-M3-003 defers that measurement to `.github/workflows/live-providers.yml`. The 1.2 s median therefore measures the product's own path — app launch, Core start, provider test, trust, indexing, the run, verification and the Review — and not a provider's response time.
- "Reference hardware" is the machine named in the evidence (an Apple M5 Pro); the CI runners repeat the same flow on macOS, Linux and Windows without a timing assertion beyond the five-minute budget.
- The welcome offers a path field for the repository rather than a native folder picker, because a scripted E2E cannot drive an OS dialog; a picker is an additive control for PX-023.
- Trust is per session, as docs/39 scopes it; a new session trusts again. Persisting trust across sessions is a policy question for the settings screen (PX-023).

## Verification

Named tests (run on macOS, Linux and Windows by `.github/workflows/ci.yml`):

- `onboarding: a fresh profile reaches a first reviewed change through provider setup, repository trust and a starter task, within five minutes at the median` (Playwright, packaged app)
- `onboarding: an invalid key, an unreachable endpoint and a missing repository each name their cause; a profile without a provider cannot start a task` (Playwright, packaged app)
- `qual_px_022_provider_setup_and_repository_trust_are_enforced_by_the_core` (the Core half, including that the credential never touches disk)

## Published measurement

Apple M5 Pro, Darwin arm64, three fresh profiles: samples 1.17 s, 1.18 s, 1.23 s; median 1.2 s; p90 1.2 s; budget 300 s.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- commit `26583a5`; hosted CI run 34568027155 green on macOS, Linux and Windows (`ci-run-34568027155.json`)
