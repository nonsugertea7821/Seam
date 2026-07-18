//! Path-bounded Shadow Stack Arena (PSSA) Implementation
//!
//! PSSA is a thread-local contiguous virtual memory region isolated from native OS stack.
//! Allocation uses bump-allocation strategy with O(1) cost.

use std::alloc::{alloc, dealloc, Layout};
use std::sync::Arc;
use std::cell::RefCell;

/// Arena pointer represents the current allocation frontier in PSSA
pub type ArenaPtr = usize;

/// Represents a single frame checkpoint (GAC - Generational Arena Checkpoint)
#[derive(Debug, Clone, Copy)]
pub struct FrameCheckpoint {
    pub ptr: ArenaPtr,
    pub size: usize,
}

/// Thread-local PSSA (Path-bounded Shadow Stack Arena)
pub struct Arena {
    /// Base address of the arena
    base: *mut u8,
    /// Current allocation pointer (arena_ptr)
    arena_ptr: ArenaPtr,
    /// Maximum arena size (static upper bound)
    max_size: usize,
    /// Active checkpoints for loop frames (GAC)
    checkpoints: RefCell<Vec<FrameCheckpoint>>,
}

// Manual Clone for Arena - creates independent copy
impl Clone for Arena {
    fn clone(&self) -> Self {
        Arena {
            base: self.base,
            arena_ptr: self.arena_ptr,
            max_size: self.max_size,
            checkpoints: RefCell::new(self.checkpoints.borrow().clone()),
        }
    }
}

impl Arena {
    /// Initialize a new PSSA with the given maximum size
    ///
    /// # Safety
    /// - The size must be reasonable and within system memory limits
    /// - Only one Arena should be active per thread
    pub fn new(max_size: usize) -> Result<Arc<Arena>, &'static str> {
        if max_size == 0 {
            return Err("Arena size must be non-zero");
        }

        // Allocate with mmap semantics (PROT_NONE first would be ideal, but we use standard alloc)
        let layout = Layout::from_size_align(max_size, 16).map_err(|_| "Invalid layout")?;

        let base = unsafe { alloc(layout) };
        if base.is_null() {
            return Err("Failed to allocate arena memory");
        }

        Ok(Arc::new(Arena {
            base,
            arena_ptr: 0,
            max_size,
            checkpoints: RefCell::new(Vec::new()),
        }))
    }

    /// Allocate space in the arena (bump allocation)
    ///
    /// Returns the absolute address of allocated memory
    /// # Safety
    /// Caller must ensure:
    /// - Size does not exceed remaining arena space
    /// - Allocated memory is properly initialized
    pub unsafe fn allocate(&mut self, size: usize) -> Result<*mut u8, &'static str> {
        if self.arena_ptr + size > self.max_size {
            return Err("Arena overflow: insufficient space for allocation");
        }

        let ptr = self.base.add(self.arena_ptr);
        self.arena_ptr += size;

        Ok(ptr)
    }

    /// Save current arena pointer as a checkpoint (for loop frames)
    pub fn checkpoint_save(&self) -> FrameCheckpoint {
        FrameCheckpoint {
            ptr: self.arena_ptr,
            size: 0, // Will be set on rollback
        }
    }

    /// Rollback arena pointer to the given checkpoint (GAC - Generational Arena Checkpoint)
    /// This is critical for loops to prevent arena leaks
    pub fn checkpoint_rollback(&mut self, checkpoint: FrameCheckpoint) {
        if checkpoint.ptr <= self.max_size {
            self.arena_ptr = checkpoint.ptr;
        }
    }

    /// Get current arena pointer position
    #[inline]
    pub fn current_ptr(&self) -> ArenaPtr {
        self.arena_ptr
    }

    /// Get base address
    #[inline]
    pub fn base_address(&self) -> *mut u8 {
        self.base
    }

    /// Get maximum size
    #[inline]
    pub fn max_size(&self) -> usize {
        self.max_size
    }

    /// Remaining available space
    #[inline]
    pub fn remaining(&self) -> usize {
        self.max_size.saturating_sub(self.arena_ptr)
    }

    /// Clear entire arena (use with caution - typically at VM shutdown only)
    pub fn clear(&mut self) {
        self.arena_ptr = 0;
        self.checkpoints.borrow_mut().clear();
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        // Safety: Layout must match allocation in new()
        if !self.base.is_null() {
            let layout = Layout::from_size_align(self.max_size, 16)
                .expect("Arena layout should be valid");
            unsafe {
                dealloc(self.base, layout);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_creation() {
        let arena_result = Arena::new(4096);
        assert!(arena_result.is_ok());
    }

    #[test]
    fn test_arena_size_validation() {
        let arena = Arena::new(0);
        assert!(arena.is_err());
        
        let arena = Arena::new(1024);
        assert!(arena.is_ok());
    }
}
