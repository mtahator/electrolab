# ADR-009: Local IPC transport — named pipes, length-prefixed JSON envelopes, Job Objects

Status: **accepted**
Date: 2026-11-13
Plan reference: Draft 2.2 §9.1–§9.4 (envelopes, transports), §14.1 and §14.8 (Inference and Execution in separate processes), §27; tasks P0.S7, P1.M1.BUS.6, P1.M1.OS.2

## Context
Inference (llama.cpp) and Execution (user code) must run outside the app process for crash isolation, while the bus keeps one contract for in-process and out-of-process engines.

## Decision
1. **Transport:** one Windows named pipe per engine process (`\\.\pipe\ep-<app-pid>-<engine>-<n>`, first-instance only, current user), frames of a 4-byte little-endian length plus a JSON `Envelope`. Unix domain sockets at the port stage, behind the same `Transport` trait.
2. **Large data never travels in messages:** `BlobRef` only (8 MB in one message costs 58 ms).
3. **Every child process is put in a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`** (measured: without it a hung child survives the app).
4. **Children inherit no handles:** the `ProcessSpawner` adapter passes an explicit empty handle list and null stdio unless a pipe is wanted (measured: an inherited pipe hung the parent).
5. **Cancellation is a message the worker wakes on**, never a flag polled between sleeps (0.2 ms vs ~16 ms on Windows' default timer).
6. **Restart policy:** detect EOF on the pipe, restart with back-off; allow for ~150 ms crash detection; consider suppressing Windows Error Reporting for engine processes.

## Alternatives considered
| Option | Why not |
|---|---|
| TCP on localhost | Firewall prompts, port management, other local users can connect |
| gRPC / HTTP/2 locally | Extra weight for no measured gain; kept for the network transport (T2) |
| Binary encoding (postcard, bincode) | JSON already at 0.05 ms per round trip; JSON matches the generated schemas and is debuggable |
| Shared memory | Complexity; large data already goes through the blob store |

## Consequences
- Out-of-process engines cost ~0.05 ms per command — no reason to keep them in-process for speed.
- OS-specific code (pipes, Job Objects, handle lists) lives in `ep-os-windows` and the transport adapter only.

## Evidence
`spikes/s7-ipc/RESULTS.md`, `spikes/s7-ipc/out/report.json`.
