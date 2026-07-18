//! GAC (Generational Arena Checkpoint) — Loop Memory Management
//!
//! Prevents arena leaks in loops by rolling back arena_ptr to checkpoint
//! after each iteration.
//!
//! DRAFT spec: Loops calling child channels repeatedly would leak memory
//! unless arena is reset to loop entry checkpoint on each iteration.

use std::cell::RefCell;

/// Loop frame with checkpoint management
/// Saves arena pointer at loop entry, resets after each iteration
pub struct LoopFrame {
    /// Saved arena_ptr at loop entry (checkpoint)
    checkpoint_ptr: *mut u8,
    /// Current iteration counter (0-indexed)
    iteration: u64,
    /// Local storage for loop-local variables
    local_storage: Vec<u8>,
    /// Checkpoint is valid
    valid: bool,
}

impl LoopFrame {
    /// Create new loop frame with checkpoint
    ///
    /// # Arguments
    /// - current_arena_ptr: Current PSSA arena pointer (to save as checkpoint)
    /// - local_storage_size: Size of loop-local variable area
    pub fn new(current_arena_ptr: *mut u8, local_storage_size: usize) -> Self {
        LoopFrame {
            checkpoint_ptr: current_arena_ptr,
            iteration: 0,
            local_storage: vec![0u8; local_storage_size],
            valid: true,
        }
    }

    /// Get checkpoint pointer (read-only)
    #[inline]
    pub fn checkpoint(&self) -> *mut u8 {
        self.checkpoint_ptr
    }

    /// Advance to next iteration (reset arena to checkpoint)
    ///
    /// Called at loop back-edge to prepare for next iteration.
    /// Returns iteration count that just completed (0 = first iteration).
    pub fn next_iteration(&mut self, current_arena_ptr: &mut *mut u8) -> u64 {
        if !self.valid {
            panic!("LoopFrame already completed or invalidated");
        }

        // Reset arena pointer to loop entry checkpoint
        // This prevents memory leak in loop body
        *current_arena_ptr = self.checkpoint_ptr;

        let completed_iteration = self.iteration;
        self.iteration += 1;
        completed_iteration
    }

    /// Get current iteration count
    #[inline]
    pub fn current_iteration(&self) -> u64 {
        self.iteration
    }

    /// Get local storage pointer (for storing loop-local variables)
    #[inline]
    pub fn local_storage_ptr(&mut self) -> *mut u8 {
        self.local_storage.as_mut_ptr()
    }

    /// Allocate from loop-local storage
    /// Returns pointer if successful, error if insufficient space
    pub fn allocate_local(&mut self, size: usize) -> Result<*mut u8, String> {
        if size > self.local_storage.len() {
            return Err("Loop local storage overflow".to_string());
        }

        Ok(self.local_storage.as_mut_ptr())
    }

    /// Mark loop frame as complete (no more iterations)
    pub fn complete(&mut self) {
        self.valid = false;
    }

    /// Get local storage size
    #[inline]
    pub fn local_storage_size(&self) -> usize {
        self.local_storage.len()
    }

    /// Is this loop frame still valid?
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.valid
    }
}

thread_local! {
    /// Stack of active loop frames (thread-local)
    /// Used for nested loops
    static LOOP_FRAMES: RefCell<Vec<LoopFrame>> = RefCell::new(Vec::new());
}

/// Push a new loop frame onto thread-local stack
pub fn push_loop_frame(frame: LoopFrame) {
    LOOP_FRAMES.with(|frames| frames.borrow_mut().push(frame));
}

/// Pop the current loop frame
pub fn pop_loop_frame() -> Option<LoopFrame> {
    LOOP_FRAMES.with(|frames| frames.borrow_mut().pop())
}

/// Access current loop frame mutably
pub fn current_loop_frame_mut<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut LoopFrame) -> Result<R, String>,
{
    LOOP_FRAMES.with(|frames| {
        let mut frms = frames.borrow_mut();
        frms.last_mut()
            .ok_or("No active loop frame".to_string())
            .and_then(|frame| f(frame))
    })
}

/// Access current loop frame immutably
pub fn current_loop_frame<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&LoopFrame) -> Result<R, String>,
{
    LOOP_FRAMES.with(|frames| {
        let frms = frames.borrow();
        frms.last()
            .ok_or("No active loop frame".to_string())
            .and_then(|frame| f(frame))
    })
}

/// Get depth of loop nesting
pub fn loop_depth() -> usize {
    LOOP_FRAMES.with(|frames| frames.borrow().len())
}

/// Clear all loop frames (typically at function exit or error)
pub fn clear_all_loops() {
    LOOP_FRAMES.with(|frames| frames.borrow_mut().clear());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loop_frame_creation() {
        let base_ptr = 0x1000 as *mut u8;
        let frame = LoopFrame::new(base_ptr, 256);

        assert_eq!(frame.iteration, 0);
        assert_eq!(frame.local_storage_size(), 256);
        assert_eq!(frame.checkpoint(), base_ptr);
        assert!(frame.is_valid());
    }

    #[test]
    fn test_checkpoint_rollback() {
        let base_ptr = 0x1000 as *mut u8;
        let mut frame = LoopFrame::new(base_ptr, 256);

        // Simulate arena advance
        let mut arena_ptr = base_ptr.wrapping_add(512);

        // First iteration
        frame.next_iteration(&mut arena_ptr);
        assert_eq!(arena_ptr, base_ptr); // Rolled back to checkpoint
        assert_eq!(frame.current_iteration(), 1);

        // Advance arena again
        arena_ptr = base_ptr.wrapping_add(300);
        frame.next_iteration(&mut arena_ptr);
        assert_eq!(arena_ptr, base_ptr); // Rolled back again
        assert_eq!(frame.current_iteration(), 2);
    }

    #[test]
    fn test_iteration_counter() {
        let ptr = 0x1000 as *mut u8;
        let mut frame = LoopFrame::new(ptr, 256);

        for expected in 0..10 {
            let mut dummy = ptr;
            let actual = frame.next_iteration(&mut dummy);
            assert_eq!(actual, expected);
        }

        assert_eq!(frame.current_iteration(), 10);
    }

    #[test]
    fn test_loop_frame_stack() {
        clear_all_loops();
        assert_eq!(loop_depth(), 0);

        let frame = LoopFrame::new(0x1000 as *mut u8, 256);
        push_loop_frame(frame);
        assert_eq!(loop_depth(), 1);

        pop_loop_frame().expect("pop failed");
        assert_eq!(loop_depth(), 0);
    }

    #[test]
    fn test_nested_loops() {
        clear_all_loops();

        let frame1 = LoopFrame::new(0x1000 as *mut u8, 256);
        push_loop_frame(frame1);
        assert_eq!(loop_depth(), 1);

        let frame2 = LoopFrame::new(0x2000 as *mut u8, 256);
        push_loop_frame(frame2);
        assert_eq!(loop_depth(), 2);

        pop_loop_frame();
        assert_eq!(loop_depth(), 1);

        pop_loop_frame();
        assert_eq!(loop_depth(), 0);
    }

    #[test]
    fn test_local_storage_allocation() {
        let ptr = 0x1000 as *mut u8;
        let mut frame = LoopFrame::new(ptr, 256);

        let local_ptr = frame.allocate_local(128).expect("allocation failed");
        assert_eq!(local_ptr, frame.local_storage_ptr());

        // Overflow
        let result = frame.allocate_local(512);
        assert!(result.is_err());
    }

    #[test]
    fn test_frame_completion() {
        let ptr = 0x1000 as *mut u8;
        let mut frame = LoopFrame::new(ptr, 256);

        assert!(frame.is_valid());
        frame.complete();
        assert!(!frame.is_valid());
    }

    #[test]
    fn test_current_loop_frame_mut() {
        clear_all_loops();
        let frame = LoopFrame::new(0x1000 as *mut u8, 256);
        push_loop_frame(frame);

        current_loop_frame_mut(|f| {
            let mut dummy = 0x1000 as *mut u8;
            f.next_iteration(&mut dummy);
            Ok(())
        }).expect("mutation failed");

        let iteration = current_loop_frame(|f| {
            Ok(f.current_iteration())
        }).expect("access failed");

        assert_eq!(iteration, 1);

        pop_loop_frame();
    }

    #[test]
    fn test_loop_memory_leak_prevention() {
        // Simulate loop allocating memory in PSSA
        let base = 0x1000 as *mut u8;
        let mut frame = LoopFrame::new(base, 256);

        let mut arena = base;

        for _ in 0..100 {
            // Simulate channel allocation in loop
            arena = arena.wrapping_add(64);

            // Checkpoint rollback
            frame.next_iteration(&mut arena);

            // After rollback, arena should be at checkpoint
            assert_eq!(arena, base);
        }

        // Total memory used is O(1), not O(iterations * 64)
    }
}
