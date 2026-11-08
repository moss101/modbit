# px046-agent-list

Run 2026-11-07T19-08-24-993Z on darwin (Apple M5 Pro, 18 cores).

- event timestamp to rendered frame, a task appears: n=220, p50 45.9 ms, p95 57.3 ms, p99 62.9 ms, max 64.5 ms
- event timestamp to rendered frame, a task is archived: n=220, p50 4.3 ms, p95 45 ms, p99 46.9 ms, max 56.5 ms
- event timestamp to rendered frame, both: n=440, p50 41.1 ms, p95 51.7 ms, p99 59.9 ms, max 64.5 ms
- event timestamp to main-process receipt (Core append, its connection, socket): n=440, p50 1 ms, p95 1 ms, p99 4 ms, max 6 ms
- main-process receipt to renderer receipt (main thread, IPC): n=440, p50 0.6 ms, p95 3.4 ms, p99 10.4 ms, max 12 ms
- renderer receipt to the row present or gone: n=440, p50 39 ms, p95 48.6 ms, p99 58.2 ms, max 62 ms
- that to the next frame: n=440, p50 0.7 ms, p95 7.2 ms, p99 13.8 ms, max 18.8 ms

Budget: MET. recorded on the "ci" profile; asserted only on the reference profile
