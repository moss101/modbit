# px041-px047-stream-render

Run local-macos-debug-core-ci-profile on darwin (Apple M5 Pro, 18 cores).

- event timestamp to rendered frame, every delta: n=794, p50 14.7 ms, p95 108.7 ms, p99 149 ms, max 209.8 ms
- event timestamp to rendered frame, first delta of a message: n=204, p50 16.8 ms, p95 103.9 ms, p99 107 ms, max 152.9 ms
- event timestamp to main-process receipt (Core append, its connection, socket): n=794, p50 3 ms, p95 103 ms, p99 136 ms, max 206 ms
- main-process receipt to renderer receipt (main thread, IPC): n=794, p50 0.4 ms, p95 11.4 ms, p99 26.2 ms, max 36.1 ms
- event timestamp to rendered frame, 2nd delta of a message: n=204, p50 9.8 ms, p95 68.9 ms, p99 106.4 ms, max 125.8 ms
- event timestamp to rendered frame, 3rd delta of a message: n=204, p50 7.9 ms, p95 105.3 ms, p99 124.2 ms, max 149 ms
- event timestamp to rendered frame, 4th delta of a message: n=182, p50 87.9 ms, p95 140.3 ms, p99 197.9 ms, max 209.8 ms
- diagnostic: one transcript read of the last task, IPC round trip: n=20, p50 35.7 ms, p95 343.8 ms, p99 458.6 ms, max 458.6 ms
- diagnostic: one agent-headers read, IPC round trip: n=20, p50 19.6 ms, p95 290.6 ms, p99 316.7 ms, max 316.7 ms
- diagnostic: renderer long tasks (>50 ms main-thread work): none
- renderer receipt to DOM showing it: n=794, p50 1.1 ms, p95 6.6 ms, p99 14.4 ms, max 19.6 ms
- DOM showing it to the next frame: n=794, p50 3.1 ms, p95 8.7 ms, p99 14.8 ms, max 18.8 ms

Budget: NOT MET. recorded on the "ci" profile; asserted only on the reference profile
