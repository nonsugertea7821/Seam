# Fork Executor

## Design Intent (from DRAFT + README)
Fork execution should be deterministic, phase-structured, and recoverable.
The five-phase model exists to isolate failure domains and make concurrency semantics auditable.

## What the Implementation Confirms
- Five-phase pipeline exists: setup, dispatch, barriers, collect, join.
- Abort handoff to direct jump is wired in dispatch.
- Basic result aggregation and join checks are implemented.

## Problems Visible from Implementation
1. Setup phase allocates by path-state entries, not by logical path (High)
   - If one path has multiple access specs, setup can over-allocate and distort phase assumptions.

2. Dispatch executes a shared pseudo-code blob for each path-state (High)
   - Current design intent implies per-path execution semantics.
   - Shared blob replay per state can duplicate or misattribute effects.

3. Abort error handling is effectively ignored at call site (Medium)
   - dispatch maps abort errors but drops result in-place.
   - This makes failure provenance unclear.

4. Collect phase is mostly placeholder semantics (Medium)
   - Current collect marks completion but does not enforce richer recovery/accounting contracts.

## Why This Matters
Executor is the runtime face of path typing.
If phase accounting is inconsistent, the runtime can appear correct in simple tests while violating core path-level guarantees.

## Priority Fix Direction
1. Normalize executor accounting to logical paths.
2. Make dispatch semantics explicitly per-path and deterministic.
3. Propagate abort handoff failures as first-class execution errors.
4. Strengthen collect phase invariants beyond completion flags.
