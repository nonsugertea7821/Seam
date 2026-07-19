//! Debug-specific helpers for `ExecutionContext`.
//!
//! Keeps breakpoint and ghost-frame logic out of `context.rs` core frame handling.

use crate::debugger::{BreakpointLocation, DebuggerContext, GhostFrameSnapshot};

pub(crate) fn record_ghost_frame(
    debugger: &mut DebuggerContext,
    rfp: usize,
    cfp: usize,
    resource_id: u32,
    phase: u32,
) {
    let snapshot = GhostFrameSnapshot::new(rfp, cfp, resource_id, phase);
    debugger.record_ghost_frame(snapshot);
}

pub(crate) fn should_break_on_abort(
    debugger: &mut DebuggerContext,
    resource_id: u32,
) -> bool {
    debugger.should_break_at(BreakpointLocation::OnAbort, resource_id)
}

pub(crate) fn should_break_on_collector_entry(
    debugger: &mut DebuggerContext,
    resource_id: u32,
) -> bool {
    debugger.should_break_at(BreakpointLocation::OnCollectorEntry, resource_id)
}
