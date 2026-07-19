# Phase 7: Direct Jump Integration

## Design Intent (from DRAFT + README)
Seam defines abort handling as a static control-flow transfer, not runtime exception search.
The design target is constant-time jump to collector with no stack unwinding, using CFP/RFP split and pre-bound :collect edges.

## What the Implementation Confirms
- Compile-time collect boundary table exists.
- Primary direct jump path exists and uses register-level transfer.
- Secondary abort escalation path exists through parent boundary resolution.

## Problems Visible from Implementation
1. RFP handoff inconsistency in dispatch path (High)
   - In dispatch abort handling, direct jump context is set with current CFP as RFP.
   - Linked abort metadata also carries target_rfp, but this value is not used there.
   - This weakens the intent that ghost-frame ownership is explicit and stable.

2. Metadata split across two target models (Medium)
   - linker AbortTarget and direct_jump DirectJumpTarget overlap but are not one canonical contract.
   - This increases risk of drift in future phases.

3. Error path semantics are under-specified (Medium)
   - Secondary-abort escalation failure returns an opaque error path.
   - Policy for when to hard-fail versus continue is not clearly encoded in design docs.

4. Low-level no-return paths still expose reachable-style API shapes (Low)
   - no-return asm sections still return Result-based signatures in some paths, creating semantic noise.

## Why This Matters
The direct-jump subsystem is the core of Seam's deterministic recovery model.
Any ambiguity in RFP source or target contract identity can silently break collector correctness while still passing basic tests.

## Priority Fix Direction
1. Define one canonical abort target contract for all layers.
2. Enforce RFP provenance rule: collector always receives intended ghost frame origin, never an implicit fallback.
3. Document hard policy for secondary-abort escalation failure.
4. Align no-return code paths with no-return API semantics.
