//! Path-bounded Shadow Stack Arena (PSSA) Implementation
//!
//! PSSA is a thread-local contiguous virtual memory region isolated from native OS stack.
//! Allocation uses bump-allocation strategy with O(1) cost.
//!
//! Memory Management:
//! - Arena::new() allocates memory via std::alloc
//! - Returned in Arc for thread-safe sharing
//! - Thread-safe arena_ptr uses atomic operations
//! - Only ONE deallocation happens when Arc refcount reaches 0

use std::alloc::{alloc, dealloc, Layout};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
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
/// 
/// Memory Safety:
/// - base and max_size are immutable after creation
/// - arena_ptr uses atomic operations for thread-safe bumping
/// - Only Arc manages the arena lifetime — single deallocation
pub struct Arena {
    /// Base address of the arena (immutable after creation)
    base: *mut u8,
    /// Current allocation pointer (atomic for thread safety)
    arena_ptr: AtomicUsize,
    /// Maximum arena size (static upper bound, immutable)
    max_size: usize,
    /// Active checkpoints for loop frames (GAC)
    /// Only checkpoints need RefCell for interior mutability
    checkpoints: RefCell<Vec<FrameCheckpoint>>,
}

// Arena does NOT implement Clone — always shared via Arc
// This prevents double-free issues

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
            arena_ptr: AtomicUsize::new(0),
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
    pub fn allocate(&self, size: usize) -> Result<*mut u8, &'static str> {
        let current = self.arena_ptr.load(Ordering::Acquire);
        if current + size > self.max_size {
            return Err("Arena overflow: insufficient space for allocation");
        }

        let ptr = unsafe { self.base.add(current) };
        let new_ptr = current + size;
        
        // Atomic compare-and-swap to ensure thread-safe allocation
        if self.arena_ptr.compare_exchange(
            current,
            new_ptr,
            Ordering::Release,
            Ordering::Acquire,
        ).is_err() {
            // Retry on conflict (another thread allocated concurrently)
            return self.allocate(size);
        }

        Ok(ptr)
    }

    /// Save current arena pointer as a checkpoint (for loop frames)
    pub fn checkpoint_save(&self) -> FrameCheckpoint {
        FrameCheckpoint {
            ptr: self.arena_ptr.load(Ordering::Acquire),
            size: 0, // Will be set on rollback
        }
    }

    /// Rollback arena pointer to the given checkpoint (GAC - Generational Arena Checkpoint)
    /// This is critical for loops to prevent arena leaks
    pub fn checkpoint_rollback(&self, checkpoint: FrameCheckpoint) {
        if checkpoint.ptr <= self.max_size {
            self.arena_ptr.store(checkpoint.ptr, Ordering::Release);
        }
    }

    /// Get current arena pointer position
    #[inline]
    pub fn current_ptr(&self) -> ArenaPtr {
        self.arena_ptr.load(Ordering::Acquire)
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
        self.max_size.saturating_sub(self.arena_ptr.load(Ordering::Acquire))
    }

    /// Clear entire arena (use with caution - typically at VM shutdown only)
    pub fn clear(&self) {
        self.arena_ptr.store(0, Ordering::Release);
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
