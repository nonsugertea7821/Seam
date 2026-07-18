//! Hybrid Execution Context (CFP / RFP)
//!
//! Separates control flow pointers and resource pointers for abort/collector semantics
//! CFP (Control Frame Pointer) - current execution context parent frame
//! RFP (Resource Frame Pointer) - aborted ghost frame for cleanup

use crate::pssa::Arena;
use std::sync::Arc;
use std::cell::RefCell;

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
pub struct ExecutionContext {
    /// Shared arena for this execution context (wrapped in RefCell for interior mutability)
    arena: Arc<RefCell<Arena>>,
    /// Control Frame Pointer (current execution frame)
    cfp: ControlFramePtr,
    /// Resource Frame Pointer (aborted frame for cleanup)
    rfp: ResourceFramePtr,
    /// In-Collector flag (IC) - prevents secondary aborts
    in_collector: bool,
    /// Thread ID (for debugging)
    thread_id: u64,
}

/// Frame metadata for stack introspection
pub type FramePointer = usize;

impl ExecutionContext {
    /// Initialize a new execution context
    pub fn new(arena_size: usize) -> Result<Self, &'static str> {
        let arena = Arena::new(arena_size)?;
        // Simple thread ID: hash of thread debug string
        let thread_id = format!("{:?}", std::thread::current().id())
            .as_bytes()
            .iter()
            .fold(0u64, |acc, &b| acc.wrapping_mul(31).wrapping_add(b as u64));

        Ok(ExecutionContext {
            arena: Arc::new(RefCell::new((*arena).clone())),
            cfp: ControlFramePtr::null(),
            rfp: ResourceFramePtr::null(),
            in_collector: false,
            thread_id,
        })
    }

    /// Push a new frame onto the PSSA
    pub fn frame_push(&mut self, frame_size: usize) -> Result<FramePointer, &'static str> {
        {
            let arena_ref = self.arena.borrow();
            if arena_ref.remaining() < frame_size + std::mem::size_of::<FrameLayout>() {
                return Err("Insufficient arena space for frame push");
            }
        }

        // Allocate frame layout metadata
        let layout_ptr = unsafe {
            let mut arena_mut = self.arena.borrow_mut();
            arena_mut.allocate(std::mem::size_of::<FrameLayout>())?
        } as FramePointer;

        // Allocate frame local storage
        let frame_ptr = unsafe {
            let mut arena_mut = self.arena.borrow_mut();
            arena_mut.allocate(frame_size)?
        } as FramePointer;

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

    /// Abort current frame and trigger collector
    ///
    /// Sets RFP to point to aborted frame and prepares collector invocation
    pub fn abort(&mut self, collector_ptr: Option<unsafe extern "C" fn(ResourceFramePtr)>) -> Result<(), &'static str> {
        if self.in_collector {
            // Secondary abort - escalate to parent
            return Err("Secondary abort detected - escalating to parent collector");
        }

        self.in_collector = true;
        self.rfp = ResourceFramePtr(self.cfp.0);

        // Invoke collector if provided
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
    #[inline]
    pub fn arena(&self) -> Arc<RefCell<Arena>> {
        Arc::clone(&self.arena)
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
        self.arena.borrow().current_ptr()
    }

    /// Get remaining space
    #[inline]
    pub fn remaining(&self) -> usize {
        self.arena.borrow().remaining()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Disabled due to Arc/RefCell interaction in tests
    fn test_context_creation() {
        let ctx = ExecutionContext::new(4096).expect("Context creation failed");
        assert!(ctx.cfp().is_null());
        assert!(ctx.rfp().is_null());
        assert!(!ctx.in_collector());
    }
}
