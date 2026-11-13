# Spike P0.S7 — The bus over local IPC

**Question (plans/phase-0):** can Execution and Inference run in separate processes with < 2 ms overhead per command, streaming, cancellation, crash detection and restart?
**Fall-back:** in-process only for T1.
**Throw-away code.** Lessons go into ADR-009 and Phase 1 M1.BUS.6 / M1.OS.2.

`src/main.rs` plays both roles: `s7-ipc` (parent: runs every measurement, prints JSON) and `s7-ipc child <pipe>` (engine). Helpers `orphan` and `hang` test what happens to a child when its parent dies.

```
cd spikes/s7-ipc && cargo build --release && target/release/s7-ipc > out/report.json
```

Results: [RESULTS.md](RESULTS.md).
