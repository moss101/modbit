# Feature Completion Audit

Before release, audit every production requirement using this checklist.

- [ ] requirement has canonical owner;
- [ ] no duplicate/legacy production path remains active;
- [ ] domain/API behavior implemented;
- [ ] production registration/routing reaches implementation;
- [ ] persistence/migration exists where state is durable;
- [ ] capability/policy is enforced at authoritative boundary;
- [ ] real effector exists;
- [ ] timeout/cancel/idempotency/failure taxonomy implemented as applicable;
- [ ] crash/restart/reconnect behavior proven where claimed;
- [ ] evidence/observability emitted;
- [ ] user/model projection reflects real state rather than optimistic mock state;
- [ ] unit/property tests pass;
- [ ] integration tests use real wiring;
- [ ] qualification test passes;
- [ ] linked E2E/security/performance gates pass;
- [ ] production package contains the implementation;
- [ ] operations/rollback/runbook impact addressed;
- [ ] manifest contains evidence refs.

If any applicable box is unchecked, status cannot be COMPLETE.
