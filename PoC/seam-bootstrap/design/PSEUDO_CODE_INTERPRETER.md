# Phase 5C: Pseudo-Code Interpreter

## Design Intent (from DRAFT + README)
Interpreter should be a minimal semantic bridge from compile-time contracts to runtime effects.
Its role is correctness transparency, not optimization.

## What the Implementation Confirms
- Minimal instruction set exists.
- Parse is permissive and non-panicking.
- Abort/success state is represented in PathResult.

## Problems Visible from Implementation
1. Default-success fallback masks malformed or incomplete programs (High)
   - If no explicit success and no abort, result is forced to success.
   - This can hide missing terminators or parser loss.

2. Parse model silently drops unknown instructions (Medium)
   - Good for resilience, bad for contract strictness in a path-typed VM.

3. Resource tracking integration is partial (Medium)
   - ResourceAccessTracker exists but is not strongly integrated into execution-phase validation.

4. Barrier semantics are flattened (Low)
   - Current barrier instruction uses a single fence behavior, while synchronization model may require richer barrier intent.

## Why This Matters
Interpreter behavior defines how much static intent survives into runtime.
Silent success and silent drops both reduce diagnostic quality and weaken trust in path-level guarantees.

## Priority Fix Direction
1. Replace default-success with explicit termination policy.
2. Add strict mode for unknown instruction handling.
3. Integrate resource access tracking into executor-phase checks.
4. Expand barrier instruction semantics to match sync design categories.
