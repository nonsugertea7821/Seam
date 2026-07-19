//! Hybrid Execution Context (CFP / RFP)
//!
//! Phase 1 + Phase 6 Integration:
//! - Phase 1: Memory management with PSSA arena and frame pointers
//! - Phase 6: Physical register bindings for O(1) direct jump abort mechanism
//!
//! Separates control flow pointers and resource pointers for abort/collector semantics
//! CFP (Control Frame Pointer) - current execution context parent frame (physical register: rbp/x29)
//! RFP (Resource Frame Pointer) - aborted ghost frame for cleanup (physical register: r15/x28)

use crate::pssa::Arena;
use crate::cfp_rfp::{HybridContextSwitch, set_hybrid_context, get_hybrid_context};
use crate::direct_jump;
use crate::debugger::DebuggerContext;
use std::sync::Arc;

/// Control Frame Pointer - points to parent frame in control flow
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlFramePtr(pub usize);

/// Resource Frame Pointer - points to ghost frame for abort cleanup
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceFramePtr(pub usize);

impl ControlFramePtr {
    pub fn null() -> Self {
        ControlFramePtr(0)
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }
}

impl ResourceFramePtr {
    pub fn null() -> Self {
        ResourceFramePtr(0)
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }
}

/// Represents the physical frame layout in PSSA
#[repr(C)]
pub struct FrameLayout {
    /// Frame identifier (for debugging)
    pub frame_id: u32,
    /// Parent CFP saved for unwinding
    pub parent_cfp: ControlFramePtr,
    /// Size of this frame's local variables
    pub frame_size: usize,
    /// Collector function pointer (if any)
    pub collector_ptr: Option<unsafe extern "C" fn(rfp: ResourceFramePtr)>,
}

/// Execution context managing PSSA and frame pointers
/// 
/// Integrates Phase 1 (memory management) with Phase 6 (physical register bindings):
/// - Maintains hybrid CFP/RFP state
/// - Enables O(1) abort via direct jump (no stack unwinding)
/// - Tracks frame layout and collector paths
pub struct ExecutionContext {
    /// Shared arena for this execution context (Arc for safe sharing)
    /// Note: Arena::arena_ptr uses AtomicUsize for thread-safe allocation
    arena: Arc<Arena>,
    /// Control Frame Pointer (current execution frame) — physical register rbp/x29
    cfp: ControlFramePtr,
    /// Resource Frame Pointer (aborted frame for cleanup) — physical register r15/x28
    rfp: ResourceFramePtr,
    /// In-Collector flag (IC) - prevents secondary aborts
    in_collector: bool,
    /// Thread ID (for debugging)
    thread_id: u64,
    /// Phase 6: Direct jump context for abort mechanism
    direct_jump_context: Option<HybridContextSwitch>,
    /// Phase 9: Debugger context for breakpoints and inspections
    debugger: DebuggerContext,
}

/// Frame metadata for stack introspection
pub type FramePointer = usize;

impl ExecutionContext {
    /// Initialize a new execution context
    pub fn new(arena_size: usize) -> Result<Self, &'static str> {
        let arena = Arena::new(arena_size)?;  // Arc<Arena>
        // Simple thread ID: hash of thread debug string
        let thread_id = format!("{:?}", std::thread::current().id())
            .as_bytes()
            .iter()
            .fold(0u64, |acc, &b| acc.wrapping_mul(31).wrapping_add(b as u64));

        let ctx = ExecutionContext {
            arena,  // Move the Arc directly, no cloning needed
            cfp: ControlFramePtr::null(),
            rfp: ResourceFramePtr::null(),
            in_collector: false,
            thread_id,
            direct_jump_context: None,
            debugger: DebuggerContext::new(),  // Phase 9: Initialize debugger
        };
        
        // Phase 6 Integration: Initialize thread-local hybrid context
        set_hybrid_context(ctx.cfp.0, ctx.rfp.0);
        
        Ok(ctx)
    }

    /// Push a new frame onto the PSSA
    pub fn frame_push(&mut self, frame_size: usize) -> Result<FramePointer, &'static str> {
        // Check space availability
        if self.arena.remaining() < frame_size + std::mem::size_of::<FrameLayout>() {
            return Err("Insufficient arena space for frame push");
        }

        // Allocate frame layout metadata
        let layout_ptr = self.arena
            .allocate(std::mem::size_of::<FrameLayout>())? as FramePointer;

        // Allocate frame local storage
        let frame_ptr = self.arena
            .allocate(frame_size)? as FramePointer;

        // Initialize frame layout
        unsafe {
            let layout = &mut *(layout_ptr as *mut FrameLayout);
            *layout = FrameLayout {
                frame_id: 0,
                parent_cfp: self.cfp,
                frame_size,
                collector_ptr: None,
            };
        }

        // Update CFP
        self.cfp = ControlFramePtr(layout_ptr);

        Ok(frame_ptr)
    }

    /// Phase 9: Get mutable reference to debugger context
    pub fn debugger_mut(&mut self) -> &mut DebuggerContext {
        &mut self.debugger
    }

    /// Phase 9: Get immutable reference to debugger context
    pub fn debugger(&self) -> &DebuggerContext {
        &self.debugger
    }

    /// Phase 9: Record ghost frame snapshot when abort occurs
    pub fn record_ghost_frame(&mut self, resource_id: u32, phase: u32) {
        self.debugger.record_abort_ghost_frame(self.rfp.0, self.cfp.0, resource_id, phase);
    }

    /// Phase 9: Check if should break on abort event
    pub fn should_break_on_abort(&mut self, resource_id: u32) -> bool {
        self.debugger.should_break_on_abort(resource_id)
    }

    /// Phase 9: Check if should break on collector entry
    pub fn should_break_on_collector_entry(&mut self, resource_id: u32) -> bool {
        self.debugger.should_break_on_collector_entry(resource_id)
    }

    /// Abort current frame and trigger collector
    ///
    /// Phase 1 + Phase 6 Integration:
    /// - Sets RFP to point to aborted frame (ghost frame for cleanup)
    /// - Prepares direct jump context for O(1) abort
    /// - May trigger direct jump to collector (DWARF-free exception handling)
    ///
    /// # Phase 6 Direct Jump Mechanism
    /// If `direct_jump_context` is configured, executes O(1) abort:
    /// - x86-64: mov rbp, target_cfp; mov r15, target_rfp; jmp collector_ip
    /// - AArch64: mov x29, target_cfp; mov x28, target_rfp; br collector_ip
    pub fn abort(&mut self, collector_ptr: Option<unsafe extern "C" fn(ResourceFramePtr)>) -> Result<(), &'static str> {
        let was_in_collector = self.in_collector;
        self.in_collector = true;
        self.rfp = ResourceFramePtr(self.cfp.0);
        
        // Phase 9: Record ghost frame and check for breakpoint
        self.record_ghost_frame(0, 5);  // resource_id=0, phase=5 (dispatch)
        if self.should_break_on_abort(0) {
            // Phase 9: Would trigger debugger break here
            // For now, just record that breakpoint was hit
        }
        
        // Update thread-local hybrid context for Phase 6 direct jump
        set_hybrid_context(self.cfp.0, self.rfp.0);

        if was_in_collector && self.direct_jump_context.is_none() {
            self.in_collector = false;
            return Err("Secondary abort detected - no direct jump context configured");
        }

        if let Some(ref direct_jump) = self.direct_jump_context {
            unsafe {
                if was_in_collector {
                    let secondary_jump_result = direct_jump::with_collect_bindings(|bindings| {
                        bindings.execute_secondary_abort_jump(
                            direct_jump.get_collector_channel_id(),
                            self.rfp.0 as *mut u8,
                        )
                    });

                    if let Err(err) = secondary_jump_result {
                        self.in_collector = false;
                        let _ = err;
                        return Err("Secondary abort detected - escalating to parent collector failed");
                    }

                    unreachable!("secondary abort jump does not return");
                }

                // O(1) abort with direct jump (no stack unwinding)
                direct_jump.execute_direct_jump();
                // This never returns (noreturn assembly)
            }
        }

        // Fallback: Traditional collector invocation (if direct jump not configured)
        if let Some(collector) = collector_ptr {
            unsafe {
                collector(self.rfp);
            }
        }

        self.in_collector = false;
        Ok(())
    }

    /// Get current CFP
    #[inline]
    pub fn cfp(&self) -> ControlFramePtr {
        self.cfp
    }

    /// Get current RFP
    #[inline]
    pub fn rfp(&self) -> ResourceFramePtr {
        self.rfp
    }

    /// Get reference to arena
    pub fn arena(&self) -> Arc<Arena> {
        Arc::clone(&self.arena)
    }
    
    /// Phase 6: Set direct jump context for abort mechanism
    /// 
    /// Configures O(1) abort via direct jump instead of traditional unwinding
    /// 
    /// # Arguments
    /// - target_cfp: New control frame (where collector executes)
    /// - target_rfp: Ghost frame (aborted context for cleanup access)
    /// - collector_ip: Entry point of collector function
    /// - collector_channel_id: Collector channel identity for parent-boundary resolution
    pub fn set_direct_jump_context(
        &mut self,
        target_cfp: *mut u8,
        target_rfp: *mut u8,
        collector_ip: *const u8,
        collector_channel_id: u32,
    ) {
        self.direct_jump_context = Some(HybridContextSwitch::new(
            target_cfp,
            target_rfp,
            collector_ip,
            collector_channel_id,
        ));
        // Update thread-local hybrid context
        set_hybrid_context(target_cfp as usize, target_rfp as usize);
    }
    
    /// Phase 6: Clear direct jump context
    pub fn clear_direct_jump_context(&mut self) {
        self.direct_jump_context = None;
    }
    
    /// Phase 6: Get current hybrid context (CFP/RFP values)
    pub fn get_hybrid_context(&self) -> Option<(usize, usize)> {
        get_hybrid_context()
    }

    /// Check if currently in collector
    #[inline]
    pub fn in_collector(&self) -> bool {
        self.in_collector
    }

    /// Get thread ID
    #[inline]
    pub fn thread_id(&self) -> u64 {
        self.thread_id
    }

    /// Get total allocated space
    #[inline]
    pub fn allocated(&self) -> usize {
        self.arena.current_ptr()
    }

    /// Get remaining space
    #[inline]
    pub fn remaining(&self) -> usize {
        self.arena.remaining()
    }
    
    /// Phase 6: Check if direct jump context is configured
    #[inline]
    pub fn has_direct_jump_context(&self) -> bool {
        self.direct_jump_context.is_some()
    }

    /// Phase 8: Register signal handler for this execution context
    /// 
    /// Enables OS signals (SIGTERM, SIGABRT, SIGINT) to trigger direct jump abort
    /// Must be called once per context when signal handling is desired
    pub fn register_signal_handler(&self) -> Result<(), &'static str> {
        use crate::signal_handler::{SignalHandler, SignalAbortTarget};
        
        // Register the signal handlers (once per thread)
        SignalHandler::register_signal_handlers()?;
        
        // Create abort target from current direct jump context
        if let Some(ref direct_jump) = self.direct_jump_context {
            let abort_target = SignalAbortTarget::new(
                direct_jump.get_collector_ip(),
                direct_jump.get_collector_channel_id(),
                direct_jump.get_target_cfp(),
                direct_jump.get_target_rfp(),
            );
            SignalHandler::set_abort_target(abort_target);
            Ok(())
        } else {
            Err("Direct jump context must be configured before registering signal handler")
        }
    }
    
    /// Phase 8: Unregister signal handler
    pub fn unregister_signal_handler(&self) -> Result<(), &'static str> {
        use crate::signal_handler::SignalHandler;
        
        SignalHandler::clear_abort_target();
        SignalHandler::unregister_signal_handlers()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_direct_jump_integration() {
        let mut ctx = ExecutionContext::new(8192).unwrap();
        
        // Verify no direct jump context initially
        assert!(!ctx.has_direct_jump_context());
        
        // Set up direct jump context
        let target_cfp = 0x10000 as *mut u8;
        let target_rfp = 0x20000 as *mut u8;
        let collector_ip = 0x30000 as *const u8;
        
        ctx.set_direct_jump_context(target_cfp, target_rfp, collector_ip, 7);
        assert!(ctx.has_direct_jump_context());
        
        // Verify hybrid context is updated
        if let Some((cfp, rfp)) = ctx.get_hybrid_context() {
            assert_eq!(cfp, target_cfp as usize);
            assert_eq!(rfp, target_rfp as usize);
        }
        
        // Clear context
        ctx.clear_direct_jump_context();
        assert!(!ctx.has_direct_jump_context());
    }
    
    #[test]
    fn test_context_creation() {
        let ctx = ExecutionContext::new(4096).expect("Context creation failed");
        assert!(ctx.cfp().is_null());
        assert!(ctx.rfp().is_null());
        assert!(!ctx.in_collector());
    }
}
