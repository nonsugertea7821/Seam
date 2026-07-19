//! Debugger Integration — Breakpoint framework and ghost frame inspection
//!
//! Provides debugger support for abort paths by:
//! - Enabling breakpoints at abort entry/collector entry
//! - Inspecting ghost frame state during execution
//! - Stepping through collector execution
//! - Conditional breakpoints based on frame state

use std::collections::BTreeMap;

/// Breakpoint condition for conditional breaks
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BreakpointCondition {
    /// Always break (no condition)
    Unconditional,
    /// Break if abort path equals resource_id
    AbortedResource(u32),
    /// Break after N hits (hit count)
    HitCount(usize),
}

impl BreakpointCondition {
    pub fn matches(&self, resource_id: u32, hit_count: usize) -> bool {
        match self {
            BreakpointCondition::Unconditional => true,
            BreakpointCondition::AbortedResource(res_id) => res_id == &resource_id,
            BreakpointCondition::HitCount(threshold) => hit_count >= *threshold,
        }
    }
}

/// Breakpoint location in abort flow
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BreakpointLocation {
    /// Break when abort() is called
    OnAbort,
    /// Break when collector entry point is reached
    OnCollectorEntry,
    /// Break when ghost frame is accessed
    OnGhostFrameAccess,
}

/// Single breakpoint with optional condition
#[derive(Clone, Debug)]
pub struct Breakpoint {
    /// Unique breakpoint ID
    pub id: u32,
    /// Where to break
    pub location: BreakpointLocation,
    /// Conditional break (resource ID or hit count)
    pub condition: BreakpointCondition,
    /// Number of times this breakpoint has been hit
    pub hit_count: usize,
    /// Whether breakpoint is enabled
    pub enabled: bool,
}

impl Breakpoint {
    pub fn new(id: u32, location: BreakpointLocation) -> Self {
        Breakpoint {
            id,
            location,
            condition: BreakpointCondition::Unconditional,
            hit_count: 0,
            enabled: true,
        }
    }

    pub fn with_condition(mut self, condition: BreakpointCondition) -> Self {
        self.condition = condition;
        self
    }

    pub fn with_hit_count(mut self, threshold: usize) -> Self {
        self.condition = BreakpointCondition::HitCount(threshold);
        self
    }

    /// Check if breakpoint should trigger on this event
    pub fn should_break(&mut self, resource_id: u32) -> bool {
        if !self.enabled {
            return false;
        }

        self.hit_count += 1;
        self.condition.matches(resource_id, self.hit_count)
    }

    pub fn reset(&mut self) {
        self.hit_count = 0;
    }
}

/// Ghost frame state snapshot for inspection
#[derive(Clone, Debug)]
pub struct GhostFrameSnapshot {
    /// Resource frame pointer (RFP)
    pub rfp: usize,
    /// Control frame pointer (CFP) at abort time
    pub cfp_at_abort: usize,
    /// Resource ID that was being accessed when abort occurred
    pub aborted_resource: u32,
    /// Current execution phase
    pub phase: u32,
}

impl GhostFrameSnapshot {
    pub fn new(rfp: usize, cfp: usize, resource: u32, phase: u32) -> Self {
        GhostFrameSnapshot {
            rfp,
            cfp_at_abort: cfp,
            aborted_resource: resource,
            phase,
        }
    }
}

/// Debugger context for managing breakpoints and inspections
pub struct DebuggerContext {
    /// Map of breakpoint ID → Breakpoint
    breakpoints: BTreeMap<u32, Breakpoint>,
    /// Next breakpoint ID to assign
    next_bp_id: u32,
    /// Last ghost frame snapshot
    last_ghost_frame: Option<GhostFrameSnapshot>,
    /// Whether debugger is enabled
    enabled: bool,
}

impl DebuggerContext {
    /// Create new debugger context
    pub fn new() -> Self {
        DebuggerContext {
            breakpoints: BTreeMap::new(),
            next_bp_id: 1,
            last_ghost_frame: None,
            enabled: true,
        }
    }

    /// Enable debugger
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable debugger
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Check if debugger is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set a breakpoint at location
    pub fn set_breakpoint(&mut self, location: BreakpointLocation) -> u32 {
        let id = self.next_bp_id;
        self.next_bp_id += 1;
        let bp = Breakpoint::new(id, location);
        self.breakpoints.insert(id, bp);
        id
    }

    /// Remove breakpoint by ID
    pub fn remove_breakpoint(&mut self, bp_id: u32) -> Result<(), &'static str> {
        if self.breakpoints.remove(&bp_id).is_some() {
            Ok(())
        } else {
            Err("Breakpoint not found")
        }
    }

    /// Enable breakpoint
    pub fn enable_breakpoint(&mut self, bp_id: u32) -> Result<(), &'static str> {
        if let Some(bp) = self.breakpoints.get_mut(&bp_id) {
            bp.enabled = true;
            Ok(())
        } else {
            Err("Breakpoint not found")
        }
    }

    /// Disable breakpoint
    pub fn disable_breakpoint(&mut self, bp_id: u32) -> Result<(), &'static str> {
        if let Some(bp) = self.breakpoints.get_mut(&bp_id) {
            bp.enabled = false;
            Ok(())
        } else {
            Err("Breakpoint not found")
        }
    }

    /// Get breakpoint by ID
    pub fn get_breakpoint(&self, bp_id: u32) -> Option<&Breakpoint> {
        self.breakpoints.get(&bp_id)
    }

    /// Check if should break at location
    pub fn should_break_at(&mut self, location: BreakpointLocation, resource_id: u32) -> bool {
        if !self.enabled {
            return false;
        }

        for bp in self.breakpoints.values_mut() {
            if bp.location == location && bp.should_break(resource_id) {
                return true;
            }
        }
        false
    }

    /// Record abort-time ghost frame snapshot from raw frame pointers.
    pub fn record_abort_ghost_frame(&mut self, rfp: usize, cfp: usize, resource_id: u32, phase: u32) {
        self.record_ghost_frame(GhostFrameSnapshot::new(rfp, cfp, resource_id, phase));
    }

    /// Check whether a breakpoint should trigger at abort entry.
    pub fn should_break_on_abort(&mut self, resource_id: u32) -> bool {
        self.should_break_at(BreakpointLocation::OnAbort, resource_id)
    }

    /// Check whether a breakpoint should trigger at collector entry.
    pub fn should_break_on_collector_entry(&mut self, resource_id: u32) -> bool {
        self.should_break_at(BreakpointLocation::OnCollectorEntry, resource_id)
    }

    /// Record ghost frame snapshot
    pub fn record_ghost_frame(&mut self, snapshot: GhostFrameSnapshot) {
        self.last_ghost_frame = Some(snapshot);
    }

    /// Get last ghost frame snapshot
    pub fn get_ghost_frame(&self) -> Option<&GhostFrameSnapshot> {
        self.last_ghost_frame.as_ref()
    }

    /// Clear all breakpoints
    pub fn clear_all_breakpoints(&mut self) {
        self.breakpoints.clear();
    }

    /// Get count of active breakpoints
    pub fn breakpoint_count(&self) -> usize {
        self.breakpoints.len()
    }

    /// Get count of enabled breakpoints
    pub fn enabled_breakpoint_count(&self) -> usize {
        self.breakpoints.values().filter(|bp| bp.enabled).count()
    }

    /// List all breakpoints
    pub fn list_breakpoints(&self) -> Vec<&Breakpoint> {
        self.breakpoints.values().collect()
    }

    /// Reset hit counts on all breakpoints
    pub fn reset_all_hit_counts(&mut self) {
        for bp in self.breakpoints.values_mut() {
            bp.reset();
        }
    }
}

impl Default for DebuggerContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_breakpoint_creation() {
        let bp = Breakpoint::new(1, BreakpointLocation::OnAbort);
        assert_eq!(bp.id, 1);
        assert_eq!(bp.location, BreakpointLocation::OnAbort);
        assert_eq!(bp.hit_count, 0);
        assert!(bp.enabled);
    }

    #[test]
    fn test_breakpoint_with_condition() {
        let bp = Breakpoint::new(1, BreakpointLocation::OnAbort)
            .with_condition(BreakpointCondition::AbortedResource(42));
        assert_eq!(bp.condition, BreakpointCondition::AbortedResource(42));
    }

    #[test]
    fn test_breakpoint_unconditional() {
        let mut bp = Breakpoint::new(1, BreakpointLocation::OnAbort);
        assert!(bp.should_break(42));
        assert_eq!(bp.hit_count, 1);
    }

    #[test]
    fn test_breakpoint_resource_condition() {
        let mut bp = Breakpoint::new(1, BreakpointLocation::OnAbort)
            .with_condition(BreakpointCondition::AbortedResource(42));
        
        // Should not break for different resource
        assert!(!bp.should_break(41));
        
        // Should break for matching resource
        assert!(bp.should_break(42));
        assert_eq!(bp.hit_count, 2);
    }

    #[test]
    fn test_breakpoint_hit_count() {
        let mut bp = Breakpoint::new(1, BreakpointLocation::OnAbort)
            .with_hit_count(3);
        
        assert!(!bp.should_break(1));  // hit_count = 1
        assert!(!bp.should_break(1));  // hit_count = 2
        assert!(bp.should_break(1));   // hit_count = 3
        assert_eq!(bp.hit_count, 3);
    }

    #[test]
    fn test_breakpoint_enabled_disabled() {
        let mut bp = Breakpoint::new(1, BreakpointLocation::OnAbort);
        assert!(bp.should_break(1));
        
        bp.enabled = false;
        assert!(!bp.should_break(1));
        assert_eq!(bp.hit_count, 1);  // Hit count not incremented
    }

    #[test]
    fn test_debugger_context_creation() {
        let ctx = DebuggerContext::new();
        assert!(ctx.is_enabled());
        assert_eq!(ctx.breakpoint_count(), 0);
    }

    #[test]
    fn test_debugger_set_breakpoint() {
        let mut ctx = DebuggerContext::new();
        let bp_id = ctx.set_breakpoint(BreakpointLocation::OnAbort);
        assert_eq!(bp_id, 1);
        assert_eq!(ctx.breakpoint_count(), 1);
        
        let bp = ctx.get_breakpoint(bp_id);
        assert!(bp.is_some());
        assert_eq!(bp.unwrap().id, 1);
    }

    #[test]
    fn test_debugger_remove_breakpoint() {
        let mut ctx = DebuggerContext::new();
        let bp_id = ctx.set_breakpoint(BreakpointLocation::OnAbort);
        assert_eq!(ctx.breakpoint_count(), 1);
        
        ctx.remove_breakpoint(bp_id).unwrap();
        assert_eq!(ctx.breakpoint_count(), 0);
        assert!(ctx.get_breakpoint(bp_id).is_none());
    }

    #[test]
    fn test_debugger_enable_disable_breakpoint() {
        let mut ctx = DebuggerContext::new();
        let bp_id = ctx.set_breakpoint(BreakpointLocation::OnAbort);
        
        ctx.disable_breakpoint(bp_id).unwrap();
        assert!(!ctx.get_breakpoint(bp_id).unwrap().enabled);
        
        ctx.enable_breakpoint(bp_id).unwrap();
        assert!(ctx.get_breakpoint(bp_id).unwrap().enabled);
    }

    #[test]
    fn test_debugger_should_break_at() {
        let mut ctx = DebuggerContext::new();
        ctx.set_breakpoint(BreakpointLocation::OnAbort);
        
        assert!(ctx.should_break_at(BreakpointLocation::OnAbort, 42));
        assert!(!ctx.should_break_at(BreakpointLocation::OnCollectorEntry, 42));
    }

    #[test]
    fn test_debugger_ghost_frame_snapshot() {
        let mut ctx = DebuggerContext::new();
        assert!(ctx.get_ghost_frame().is_none());
        
        let snapshot = GhostFrameSnapshot::new(0x1000, 0x2000, 42, 2);
        ctx.record_ghost_frame(snapshot);
        
        assert!(ctx.get_ghost_frame().is_some());
        let gf = ctx.get_ghost_frame().unwrap();
        assert_eq!(gf.rfp, 0x1000);
        assert_eq!(gf.cfp_at_abort, 0x2000);
        assert_eq!(gf.aborted_resource, 42);
        assert_eq!(gf.phase, 2);
    }

    #[test]
    fn test_debugger_enable_disable() {
        let mut ctx = DebuggerContext::new();
        assert!(ctx.is_enabled());
        
        ctx.disable();
        assert!(!ctx.is_enabled());
        
        ctx.enable();
        assert!(ctx.is_enabled());
    }

    #[test]
    fn test_debugger_clear_all_breakpoints() {
        let mut ctx = DebuggerContext::new();
        ctx.set_breakpoint(BreakpointLocation::OnAbort);
        ctx.set_breakpoint(BreakpointLocation::OnCollectorEntry);
        assert_eq!(ctx.breakpoint_count(), 2);
        
        ctx.clear_all_breakpoints();
        assert_eq!(ctx.breakpoint_count(), 0);
    }

    #[test]
    fn test_debugger_enabled_breakpoint_count() {
        let mut ctx = DebuggerContext::new();
        let bp1 = ctx.set_breakpoint(BreakpointLocation::OnAbort);
        let _bp2 = ctx.set_breakpoint(BreakpointLocation::OnCollectorEntry);
        
        assert_eq!(ctx.enabled_breakpoint_count(), 2);
        
        ctx.disable_breakpoint(bp1).unwrap();
        assert_eq!(ctx.enabled_breakpoint_count(), 1);
    }

    #[test]
    fn test_debugger_reset_hit_counts() {
        let mut ctx = DebuggerContext::new();
        ctx.set_breakpoint(BreakpointLocation::OnAbort);
        
        ctx.should_break_at(BreakpointLocation::OnAbort, 1);
        ctx.should_break_at(BreakpointLocation::OnAbort, 1);
        
        let bp = ctx.get_breakpoint(1).unwrap();
        assert_eq!(bp.hit_count, 2);
        
        ctx.reset_all_hit_counts();
        let bp = ctx.get_breakpoint(1).unwrap();
        assert_eq!(bp.hit_count, 0);
    }

    #[test]
    fn test_debugger_list_breakpoints() {
        let mut ctx = DebuggerContext::new();
        ctx.set_breakpoint(BreakpointLocation::OnAbort);
        ctx.set_breakpoint(BreakpointLocation::OnCollectorEntry);
        ctx.set_breakpoint(BreakpointLocation::OnGhostFrameAccess);
        
        let bps = ctx.list_breakpoints();
        assert_eq!(bps.len(), 3);
        assert_eq!(bps[0].location, BreakpointLocation::OnAbort);
        assert_eq!(bps[1].location, BreakpointLocation::OnCollectorEntry);
        assert_eq!(bps[2].location, BreakpointLocation::OnGhostFrameAccess);
    }
}
