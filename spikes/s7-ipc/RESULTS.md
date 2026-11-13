# Spike P0.S7 — Results: the bus over local IPC

Date: 2026-11-13 · Windows named pipe, 4-byte length + JSON envelope · tokio 1.54 · raw output `out/report.json`

## Verdict: **pass** — Inference and Execution can run in their own processes. Overhead is ~0.05 ms, 40× under the 2 ms budget.

| Check | Result | Budget / expectation |
|---|---|---|
| Round trip, small command (10,000 sequential) | **0.052 ms** median, 0.095 ms p99, 0.73 ms max | < 2 ms |
| 64 requests in flight | 106,000 messages/s | — |
| Streaming 1,000 token events | first after 0.63 ms, all in 5.8 ms | — |
| Cancel → acknowledgement from a busy child | **0.20 ms** | prompt |
| Child killed from outside → noticed | 0.65 ms | prompt |
| Child crashes (`abort`) → noticed | 134 ms | Windows crash handling runs first |
| Restart → first correct reply | 16 ms | — |
| Spawn + connect + first reply | 17 ms | — |
| 1 MB / 8 MB in one message | 11 ms / 58 ms | why large data travels as `BlobRef` |
| Parent exits abruptly: child left running? | **with Job Object: no · without: yes** | must be "no" |

## Findings for the foundation (Phase 1 M1.BUS.6, M1.OS.2)

1. **Named pipes + length-prefixed JSON envelopes are enough**; no binary format needed for commands and events. Large payloads go by `BlobRef` (8 MB costs 58 ms in a message).
2. **Every child process goes into a Job Object with "kill on close".** Without it, a hung engine survives an app crash — measured.
3. **Never let children inherit handles.** On Windows a child inherits *every* inheritable handle of its parent, not only stdin/stdout: in this spike a captured stdout pipe leaked into a grandchild and blocked the parent forever. The `ProcessSpawner` adapter must start children with an explicit, empty handle list (`PROC_THREAD_ATTRIBUTE_HANDLE_LIST`) and null stdio unless a pipe is wanted.
4. **Cancellation is event-driven.** A worker that polls a cancel flag between `sleep(1 ms)` calls reacts in ~16 ms, because Windows' default timer granularity is 15.6 ms; waking on the cancel message itself takes 0.2 ms.
5. **Crashes take longer to see than kills** (134 ms vs 0.65 ms) because Windows Error Reporting handles the crash first; restart policy must not assume instant detection. Consider disabling the WER dialog for engine processes (`SetErrorMode`/`WerAddExcludedApplication`) in the adapter.
6. The transport is Windows-only code (tokio named pipes, Job Objects) and belongs in OS adapters; Unix domain sockets and process groups at the port stage.

## Bugs found in the spike itself (kept for the record)
- First run: cancel looked slow (polling against the timer) and the orphan test could not fail (the child exited when its pipe closed). Second run: the handle-inheritance leak above hung the measurement. All fixed before the numbers in the table.
