# px041-px047-stream-render

Run 2026-11-07T19-08-04-879Z on darwin (Apple M5 Pro, 18 cores).

- event timestamp to rendered frame, every delta: n=816, p50 11.7 ms, p95 111.2 ms, p99 132.6 ms, max 597.2 ms
- event timestamp to rendered frame, first delta of a message: n=204, p50 16.4 ms, p95 38.6 ms, p99 44.9 ms, max 69.5 ms
- event timestamp to main-process receipt (Core append, its connection, socket): n=816, p50 1 ms, p95 101 ms, p99 125 ms, max 584 ms
- main-process receipt to renderer receipt (main thread, IPC): n=816, p50 0.5 ms, p95 15.1 ms, p99 23.5 ms, max 35.4 ms
- event timestamp to rendered frame, 2nd delta of a message: n=204, p50 6.9 ms, p95 18.6 ms, p99 28.5 ms, max 31.2 ms
- event timestamp to rendered frame, 3rd delta of a message: n=204, p50 6.1 ms, p95 30.6 ms, p99 37.3 ms, max 38 ms
- event timestamp to rendered frame, 4th delta of a message: n=204, p50 99.2 ms, p95 127.9 ms, p99 196.5 ms, max 597.2 ms
- diagnostic: one transcript read of the last task, IPC round trip: n=20, p50 17.1 ms, p95 32.3 ms, p99 128.3 ms, max 128.3 ms
- diagnostic: one agent-headers read, IPC round trip: n=20, p50 11.9 ms, p95 24.5 ms, p99 79.2 ms, max 79.2 ms
- diagnostic: renderer long tasks (>50 ms main-thread work): none
- renderer receipt to DOM showing it: n=816, p50 1.4 ms, p95 8.9 ms, p99 17.4 ms, max 22.6 ms
- DOM showing it to the next frame: n=816, p50 2.5 ms, p95 8.7 ms, p99 15.8 ms, max 20.5 ms

Budget: NOT MET. recorded on the "ci" profile; asserted only on the reference profile
