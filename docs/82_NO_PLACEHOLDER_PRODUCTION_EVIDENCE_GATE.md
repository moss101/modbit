# No-Placeholder Production Evidence Gate

> **LOCKED:** a production capability cannot be declared COMPLETE because an interface, UI, fake adapter, mocked unit test, task checkbox or sample output exists.

## Completion state

`DECLARED → IMPLEMENTED → WIRED → E2E_PROVEN → COMPLETE`.

### IMPLEMENTED
A real code path and real effector/storage behavior exists. An interface with `NotImplemented`, canned return value or unreachable module is not IMPLEMENTED.

### WIRED
A production caller can reach it through actual registration/routing/policy paths. A direct unit call to an otherwise unreachable class does not count.

### E2E_PROVEN
A release test exercises the packaged or production-equivalent stack to a real effect: filesystem/Git/process/browser/sandbox/provider/integration/database as applicable. Fault/restart behavior is proven where durability is claimed.

### COMPLETE
Required unit/property/integration/E2E/security/performance gates are green, evidence is retained, operations/rollback exist and the feature is included in the build manifest.

## Allowed test doubles

Small deterministic fakes/mocks are allowed **inside lower-level tests** to inject errors and exercise edge cases. They are never the only proof for a production capability. The release manifest records which real-system test closes each capability.

## CI checks

- registered production tool has a non-test effector;
- no production route resolves to a demo/fake provider or fake sandbox;
- every ADOPT/ADAPT source row resolves to a real qualification test;
- packaged smoke test uses real local filesystem/Git/process/database;
- staging/nightly uses at least one real model-provider path;
- browser capability uses actual Chromium;
- cloud execution gate uses actual Sandbox Gateway + guest;
- restart/resume tests kill real processes rather than simulating state transition only;
- protected effects require real approval/receipt path;
- large output/media tests use actual byte payloads and artifact storage;
- no release evidence is a screenshot of a mock UI as sole proof.

## Evidence record

Every release-gate execution produces: build digest, git revision, environment digest, test ID, run ID, exact capability IDs, provider/sandbox/browser versions where relevant, event/evidence refs, pass/fail, duration and artifact digests. This gives the team proof that “implemented” means the real system worked.
