# Runtime Linking

## Design Intent (from DRAFT + README)
Linking should preserve static path contracts and produce execution-ready structure without re-interpreting language semantics at runtime.
The boundary between linker and executor must be clean.

## What the Implementation Confirms
- Compiled fork metadata is transformed into linked fork structure.
- Resource accesses and barriers are propagated.
- Executor creation is exposed from linker module.

## Problems Visible from Implementation
1. Responsibility drift inside one module (High)
   - linker module now contains linking, interpreter, and executor orchestration.
   - This blurs the design boundary that should keep static transformation distinct from runtime coordination.

2. Path-state cardinality may diverge from path count semantics (High)
   - Linked path states are generated per access entry.
   - Several downstream operations assume path-count semantics.
   - This can create mismatched setup/join behavior under richer contracts.

3. Documentation-level contract is weaker than code coupling (Medium)
   - Public narrative says clear separation, but implementation coupling suggests transitional architecture.

## Why This Matters
Seam depends on static guarantees flowing cleanly into runtime.
When linking and execution contracts mix, failures become harder to classify as compile-time versus runtime design defects.

## Priority Fix Direction
1. Separate linker core from executor/interpreter orchestration at module boundary.
2. Define canonical unit for phase accounting: per path vs per access.
3. Publish explicit invariants for linked-fork shape and validation checks.
