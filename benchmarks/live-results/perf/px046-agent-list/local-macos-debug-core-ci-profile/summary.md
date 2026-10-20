# px046-agent-list

Run local-macos-debug-core-ci-profile on darwin (Apple M5 Pro, 18 cores).

- event timestamp to rendered frame, a task appears: n=220, p50 91.6 ms, p95 122.6 ms, p99 146.7 ms, max 155.1 ms
- event timestamp to rendered frame, a task is archived: n=220, p50 20.2 ms, p95 75.2 ms, p99 78.3 ms, max 114.7 ms
- event timestamp to rendered frame, both: n=440, p50 69.2 ms, p95 112.8 ms, p99 136.2 ms, max 155.1 ms
- event timestamp to main-process receipt (Core append, its connection, socket): n=440, p50 1 ms, p95 2 ms, p99 3 ms, max 5 ms
- main-process receipt to renderer receipt (main thread, IPC): n=440, p50 0.3 ms, p95 17.7 ms, p99 45.6 ms, max 52.1 ms
- renderer receipt to the row present or gone: n=440, p50 66.3 ms, p95 106.9 ms, p99 129.2 ms, max 140.5 ms
- that to the next frame: n=440, p50 1 ms, p95 11 ms, p99 15.2 ms, max 25.4 ms

Budget: MET. recorded on the "ci" profile; asserted only on the reference profile
