# Phase 8: Signal Integration

## Design Intent (from DRAFT + README)
Signals should not create a second exception model.
They must enter the same abort-to-collector path, preserving O(1) transfer and deterministic recovery semantics.

## What the Implementation Confirms
- Thread-local abort target registration is implemented.
- Signal handler can jump directly into collector path.
- Context-level registration wiring exists.

## Problems Visible from Implementation
1. Async-signal-safety boundary is weak (High)
   - Handler fallback uses process-exit calls and mixed platform branches.
   - The design intent says minimal handler work, but current behavior still includes nontrivial runtime branching.

2. Registration model is snapshot-based, not lifecycle-bound (High)
   - Abort target is captured at registration time.
   - If execution context changes after registration, handler state may become stale.

3. collector_channel_id is carried but not consumed in handler path (Medium)
   - Boundary identity is part of the contract, but signal dispatch does not use it for policy decisions.

4. API portability concerns are visible (Medium)
   - signal registration uses simple signal APIs rather than stricter action-style configuration.
   - This makes behavior less predictable across platforms.

## Why This Matters
Signal entry is where external nondeterminism touches Seam.
If this edge is not strict, deterministic recovery can degrade under real process conditions even when normal abort tests pass.

## Priority Fix Direction
1. Tighten signal-safe handler contract and reduce handler-side branching.
2. Bind signal target lifecycle to active execution context updates.
3. Either consume collector boundary identity in policy or remove it from this layer.
4. Standardize platform registration behavior with explicit semantics documentation.
