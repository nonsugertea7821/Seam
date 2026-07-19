# src Responsibility Refactor Plan

Date: 2026-07-19
Scope: PoC/seam-bootstrap/src

## Goals

1. Clarify module boundaries by domain (runtime / compile / execution).
2. Remove duplicate implementations that can diverge over time.
3. Keep backward compatibility for existing imports during migration.
4. Document an incremental plan that can be implemented without a large-bang rewrite.

## Current Findings

### 1) Over-exposed crate root

`src/lib.rs` currently exports many modules and symbols flatly, which makes domain boundaries hard to read.

Risk:
- New code tends to depend on whichever symbol is easiest to import.
- Runtime internals and compile-time concerns are mixed at API level.

### 2) Duplicate SARMEntry definition

`SARMEntry` existed in both:
- `src/abort.rs`
- `src/sarm.rs`

Risk:
- Struct layout drift can cause ABI/logic mismatch.
- Same concept maintained in two places increases maintenance cost.

### 3) Responsibility concentration in core runtime files

`src/context.rs` currently mixes:
- CFP/RFP state management
- frame allocation orchestration
- direct jump integration
- debugger hooks

This is not immediately broken, but it increases change coupling.

## Changes Applied In This Iteration

### A. Duplicate type unification

- Removed local `SARMEntry` from `src/abort.rs`.
- `abort` now imports and uses canonical `SARMEntry` from `src/sarm.rs`.

Effect:
- Single source of truth for static abort register metadata.
- Lower ABI drift risk.

### B. Logical package split at crate API boundary

Added grouped re-export namespaces in `src/lib.rs`:

- `runtime`
  - context / abort / cfp_rfp / direct_jump / sarm / signal_handler / debugger / pssa
- `compile`
  - ast / compiler / codegen / contract / effect
- `execution`
  - channel / fork / linker / resource / shadow_buffer / sync / transaction

Notes:
- Existing flat exports are preserved for compatibility.
- This is a non-breaking first step toward stricter boundaries.

## Recommended Next Steps (Incremental)

1. Context decomposition (small internal split)
- Extract debugger-specific helper methods from `context.rs` to a dedicated extension module.
- Keep `ExecutionContext` public API unchanged.

2. Runtime package physical split
- Move runtime files into `src/runtime/` gradually using `mod` forwarding.
- Start with low-risk files: `sarm`, `direct_jump`, `signal_handler`.

3. Execution flow isolation
- In `linker.rs`, isolate phase execution state machine from pseudo-code interpreter glue.
- Introduce internal submodules: `phases`, `interpreter_bridge`, `result_aggregation`.

4. Boundary enforcement
- Add `pub(crate)` where external visibility is not required.
- Reduce direct cross-domain imports (compile -> runtime should be metadata-only where possible).

5. Regression safety
- Add a focused test matrix for:
  - abort path + collector dispatch
  - SARM lookup/serialization
  - direct jump binding resolution

## Migration Policy

- Keep old import paths available for at least one stabilization cycle.
- Prefer adding grouped namespace imports in new code:
  - `seam_bootstrap::runtime::*`
  - `seam_bootstrap::compile::*`
  - `seam_bootstrap::execution::*`
- Remove legacy flat re-exports only after downstream usage audit.
