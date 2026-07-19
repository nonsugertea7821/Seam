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
- `src/runtime/abort.rs`
- `src/runtime/sarm.rs`

Risk:
- Struct layout drift can cause ABI/logic mismatch.
- Same concept maintained in two places increases maintenance cost.

### 3) Responsibility concentration in core runtime files

`src/runtime/context.rs` currently mixes:
- CFP/RFP state management
- frame allocation orchestration
- direct jump integration
- debugger hooks

This is not immediately broken, but it increases change coupling.

## Changes Applied In This Iteration

### A. Duplicate type unification

- Removed local `SARMEntry` from `src/runtime/abort.rs`.
- `abort` now imports and uses canonical `SARMEntry` from `src/runtime/sarm.rs`.

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

### C. Context debugger concern extraction

- Added internal helper module: `src/runtime/context_debug.rs`.
- Moved debug-specific helper logic (`record_ghost_frame`, abort/collector breakpoint checks)
  out of direct implementation detail in `src/runtime/context.rs`.
- `ExecutionContext` public API remains unchanged; `context` now delegates to helper functions.

Effect:
- `context.rs` becomes more focused on frame/context lifecycle and abort orchestration.
- Debugger behavior can evolve independently with lower risk of touching frame management code.

### D. Physical directory split by responsibility group

Moved source files into grouped directories:

- `src/runtime/`
  - pssa / context / context_debug / abort / cfp_rfp / shadow_arena / sarm / gac / direct_jump / signal_handler / debugger
- `src/compile/`
  - ast / compiler / codegen / contract / effect
- `src/execution/`
  - channel / resource / shadow_buffer / transaction / fork / sync / linker

Compatibility handling:
- `src/lib.rs` keeps existing module names (`crate::context`, `crate::linker`, ...)
  and maps them to new file locations via `#[path = "..."]`.
- Existing imports remain valid while the physical layout is now domain-oriented.

## Recommended Next Steps (Incremental)

1. Context decomposition (next internal split)
- Extract direct-jump specific helper logic from `src/runtime/context.rs` into a dedicated helper module.
- Keep `ExecutionContext` public API unchanged.

2. Runtime package visibility tightening
- Apply `pub(crate)` to runtime internals that should not be external API.
- Keep public re-exports only for stable surface.

3. Execution flow isolation
- In `src/execution/linker.rs`, isolate phase execution state machine from pseudo-code interpreter glue.
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
