# Phase 9: Debugger Integration

## Design Intent (from DRAFT + README)
Debugger should expose abort/collector causality without changing runtime semantics.
It must help validate CFP/RFP recovery behavior, not become a second control-flow authority.

## What the Implementation Confirms
- Breakpoint model includes abort, collector-entry, and ghost-frame concepts.
- Ghost frame snapshot storage exists.
- ExecutionContext is wired to debugger context.

## Problems Visible from Implementation
1. Recorded abort metadata is currently hardcoded in main abort path (High)
   - abort path records resource_id=0 and phase=5 constants.
   - This weakens root-cause diagnostics and design intent of path-aware observability.

2. Collector-entry breakpoint path is not fully instrumented end-to-end (Medium)
   - API exists, but collector-entry event emission is not clearly wired in runtime flow.

3. Ghost-frame-access breakpoint concept is largely declarative (Medium)
   - Breakpoint type exists, but no robust runtime hook path is visible for actual access events.

4. Enabled-by-default behavior may conflict with stated low-overhead intent (Low)
   - Debugger context starts enabled by default.

## Why This Matters
Abort debugging is not optional in Seam; it is how we verify that deterministic recovery really matches design.
Weak event fidelity reduces confidence in collector correctness and escalation behavior.

## Priority Fix Direction
1. Record real abort context metadata (resource, phase, boundary identity) at source points.
2. Add explicit collector-entry event hooks in runtime transitions.
3. Implement concrete ghost-frame-access instrumentation points.
4. Revisit default enablement policy against overhead goals.
