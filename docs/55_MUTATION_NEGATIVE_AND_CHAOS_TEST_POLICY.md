# Mutation, Negative and Chaos Test Policy

## Purpose

Positive tests can pass while security/recovery checks are ineffective. Critical gates need tests that prove the test itself detects broken behavior.

## Mutation requirements

For policy, effect idempotency, checkpoint fencing, context freshness, tenant isolation, secret redaction and path protection, periodically introduce controlled mutations such as inverted predicate, skipped fence, stale read, duplicate dispatch or removed redaction and prove the suite fails.

## Negative fixtures

Maintain fixtures for ambiguous edits, invalid/stale IDs, duplicate requests, oversized outputs/media, malformed provider/tool events, hostile web/doc content, symlink/path escapes, cross-tenant handles, expired capabilities and corrupted artifacts.

## Chaos

Nightly/staging may inject process kills, network loss, latency, partial responses and resource exhaustion. Chaos tests must have bounded blast radius and deterministic post-run invariant checks.

## EPR controls that must be challenged

For the matching EPR-FI scenarios in doc 61, remove hard eligibility, risk monotonicity, critic capability denial, revision/epoch fencing, usage deduplication, budget inheritance, replay egress/credential isolation and policy evidence checks one control at a time. The associated test must fail. Restore each control for the real-system positive proof. Never count prompt statements, mock tool denial or disabled security in the production proof as isolation evidence.

v1.1 negative proofs also challenge mean-only cheap eligibility, missing stats/gate versions, runtime-generated branches, conflated assurance/acceptance and initial-success misattribution. Reviewer tests must demonstrate allowed bounded scratch evidence writes/processes as well as denied canonical/persistent/external effects and actual cleanup. All-tools-denied mocks cannot qualify EPR-018. Independent gate-safety mutations must block rollout despite better router scores.
