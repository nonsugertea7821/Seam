//! Path-bounded Shadow Stack Arena (PSSA) Implementation
//!
//! PSSA is a thread-local contiguous virtual memory region isolated from native OS stack.
//! Allocation uses bump-allocation strategy with O(1) cost.
//!
//! Memory Management (mmap-based):
//! - Arena::new() allocates memory via OS virtual memory (VirtualAlloc/mmap)
//! - Guard pages at bottom and top prevent buffer overruns
//! - Returned in Arc for thread-safe sharing
//! - Thread-safe arena_ptr uses atomic operations
//! - Only ONE deallocation happens when Arc refcount reaches 0

use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::cell::RefCell;

// Platform-specific virtual memory allocation
#[cfg(windows)]
mod vm {
    use std::ffi::c_void;
    
    extern "system" {
        fn VirtualAlloc(lpAddress: *const c_void, dwSize: usize, flAllocationType: u32, flProtect: u32) -> *mut c_void;
        fn VirtualFree(lpAddress: *mut c_void, dwSize: usize, dwFreeType: u32) -> i32;
    }
    
    const MEM_COMMIT: u32 = 0x1000;
    const MEM_RESERVE: u32 = 0x2000;
    const MEM_RELEASE: u32 = 0x8000;
    const PAGE_READWRITE: u32 = 0x04;
    const PAGE_NOACCESS: u32 = 0x01;
    
    pub unsafe fn allocate_virtual(size: usize) -> *mut u8 {
        let page_size = 4096;
        // Allocate with guard pages at top and bottom
        let total_size = size + (2 * page_size);
        
        let base = VirtualAlloc(
            std::ptr::null(),
            total_size,
            MEM_RESERVE,
            PAGE_NOACCESS,
        ) as *mut u8;
        
        if base.is_null() {
            return std::ptr::null_mut();
        }
        
        // Commit the main arena region (skip bottom guard page)
        let commit_base = base.add(page_size);
        VirtualAlloc(
            commit_base as *const c_void,
            size,
            MEM_COMMIT,
            PAGE_READWRITE,
        );
        
        // Return pointer to usable region (after bottom guard page)
        commit_base
    }
    
    pub unsafe fn deallocate_virtual(ptr: *mut u8, size: usize) {
        if ptr.is_null() {
            return;
        }
        let page_size = 4096;
        let base = ptr.sub(page_size);
        let total_size = size + (2 * page_size);
        
        VirtualFree(base as *mut c_void, total_size, MEM_RELEASE);
    }
}

#[cfg(unix)]
mod vm {
    use libc::{mmap, munmap, mprotect, PROT_READ, PROT_WRITE, PROT_NONE, MAP_PRIVATE, MAP_ANONYMOUS};
    
    pub unsafe fn allocate_virtual(size: usize) -> *mut u8 {
        let page_size = 4096;
        // Allocate with guard pages at top and bottom
        let total_size = size + (2 * page_size);
        
        let base = mmap(
            std::ptr::null_mut(),
            total_size,
            PROT_NONE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            -1,
            0,
        ) as *mut u8;
        
        if base as isize == -1 {
            return std::ptr::null_mut();
        }
        
        // Enable main arena region (skip bottom guard page)
        let commit_base = base.add(page_size);
        mprotect(
            commit_base as *mut libc::c_void,
            size,
            PROT_READ | PROT_WRITE,
        );
        
        // Return pointer to usable region (after bottom guard page)
        commit_base
    }
    
    pub unsafe fn deallocate_virtual(ptr: *mut u8, size: usize) {
        if ptr.is_null() {
            return;
        }
        let page_size = 4096;
        let base = ptr.sub(page_size);
        let total_size = size + (2 * page_size);
        
        munmap(base as *mut libc::c_void, total_size);
    }
}

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
/// Virtual memory-based with guard pages:
/// - base and max_size are immutable after creation
/// - arena_ptr uses atomic operations for thread-safe bumping
/// - Only Arc manages the arena lifetime — single deallocation
/// - Guard pages prevent buffer overruns (top and bottom)
pub struct Arena {
    /// Base address of the arena (immutable after creation)
    /// Points to first usable byte (after bottom guard page)
    base: *mut u8,
    /// Current allocation pointer (atomic for thread safety)
    arena_ptr: AtomicUsize,
    /// Maximum arena size (static upper bound, immutable)
    /// Does NOT include guard pages in this count
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
    /// Allocates using OS virtual memory with guard pages:
    /// - Bottom guard page (PROT_NONE) prevents underflow
    /// - Main arena region (PROT_READ|PROT_WRITE)
    /// - Top guard page (PROT_NONE) prevents overflow
    ///
    /// # Arguments
    /// - max_size: Desired arena capacity (excludes guard pages)
    ///
    /// # Returns
    /// - Ok(Arc<Arena>): Successfully allocated arena
    /// - Err: Allocation failed or size too large
    pub fn new(max_size: usize) -> Result<Arc<Arena>, &'static str> {
        if max_size == 0 {
            return Err("Arena size must be non-zero");
        }
        
        if max_size > (1usize << 32) {
            return Err("Arena size too large (max 4GB)");
        }

        // Allocate with virtual memory (includes guard pages)
        let base = unsafe { vm::allocate_virtual(max_size) };
        
        if base.is_null() {
            return Err("Failed to allocate virtual memory for arena");
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
        // Safety: base and max_size were set during allocation
        if !self.base.is_null() {
            unsafe {
                vm::deallocate_virtual(self.base, self.max_size);
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
