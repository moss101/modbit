# Existing-Code Feature Audit Protocol

Run this protocol whenever a task touches code that already exists.

## Step 1 — identify requirement surface

List every task requirement and its observable behaviors. Do not start with filenames.

## Step 2 — trace actual production path

For each behavior trace:

`caller → transport/command → canonical owner → policy → persistence → effector → result/event/evidence → user/model projection`.

Record exact files and symbols. Stop at the first missing or fake boundary.

## Step 3 — classify

- **PRODUCTION-WORKING:** all applicable depth layers exist and real tests pass.
- **IMPLEMENTED-PARTIAL:** meaningful real behavior exists but one or more required layers/gates are missing.
- **SCAFFOLDED:** types/interfaces/routes exist but behavior is absent or fake.
- **DOCUMENTED-ONLY:** no material code path.
- **BROKEN-DRIFTED:** implementation exists but violates current contracts or fails.
- **NOT-FOUND:** no relevant implementation.

## Step 4 — search for duplicate generations

Before adding code, find old/parallel modules providing the same semantic operation. Choose the canonical owner and remove/retire duplicate paths as part of the task when safe.

## Step 5 — inspect tests skeptically

Identify whether tests call production registration/routing and real effectors. Tests built entirely on mocks cannot establish feature completion.

## Step 6 — produce the audit note

The task record must contain: current state, evidence paths/symbols, missing links, duplicate/drift risks, proposed in-place changes, tests to add/run, and migration/removal steps if old code conflicts.
