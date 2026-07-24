//! Operator arena definition.
//!
//! An `OperatorArena` represents the temporary allocation state used during
//! Seam operator execution.
//!
//! The arena does not own memory resources. The platform layer owns the
//! underlying virtual memory mapping, while this type only tracks the active
//! allocation boundary inside that region.
//!
//! The purpose of this arena is to provide:
//!
//! - constant-time sequential allocation,
//! - constant-time allocation rollback through checkpoints,
//! - temporary execution storage without heap lifecycle management.
//!
//! Invariants:
//!
//! - base_ptr <= bump_ptr <= limit_ptr
//! - capacity == limit_ptr - base_ptr
//! - successful allocations advance bump_ptr monotonically
//! - failed allocations do not modify bump_ptr
//! - checkpoints are valid only within the lifetime of the originating arena

use crate::abi::ArenaCheckpoint;

/// Contiguous bump allocation arena used by the Seam ABI.
///
/// `OperatorArena` represents allocation state only.
/// It does not own the underlying virtual memory resource.
#[repr(C, align(64))]
#[derive(Debug)]
pub struct OperatorArena {
    /// First usable address.
    pub base_ptr: *mut u8,

    /// First address after the usable region.
    pub limit_ptr: *mut u8,

    /// Current allocation boundary.
    pub bump_ptr: *mut u8,

    /// Total usable capacity in bytes.
    pub capacity: usize,
}

impl OperatorArena {
    /// Creates an arena over an existing memory region.
    ///
    /// # Safety
    ///
    /// The caller must guarantee:
    ///
    /// - `base_ptr..limit_ptr` is a valid writable memory region.
    /// - `capacity == limit_ptr - base_ptr`.
    #[inline(always)]
    pub unsafe fn new(
        base_ptr: *mut u8,
        limit_ptr: *mut u8,
        capacity: usize,
    ) -> Self {
        debug_assert!(
            base_ptr as usize <= limit_ptr as usize
        );

        debug_assert_eq!(
            limit_ptr as usize - base_ptr as usize,
            capacity
        );

        Self {
            base_ptr,
            limit_ptr,
            bump_ptr: base_ptr,
            capacity,
        }
    }

    /// Allocates a region using bump allocation.
    ///
    /// Allocation is constant-time.
    ///
    /// Returns `None` when:
    ///
    /// - alignment is invalid,
    /// - size overflows,
    /// - remaining capacity is insufficient.
    ///
    /// On failure, the arena state remains unchanged.
    ///
    /// # Safety
    ///
    /// `align` must be a non-zero power of two.
    #[inline(always)]
    pub unsafe fn alloc(
        &mut self,
        size: usize,
        align: usize,
    ) -> Option<*mut u8> {
        if align == 0 || !align.is_power_of_two() {
            return None;
        }

        let current = self.bump_ptr as usize;

        let aligned = current
            .checked_add(align - 1)?
            & !(align - 1);

        let next = aligned.checked_add(size)?;

        if next > self.limit_ptr as usize {
            return None;
        }

        self.bump_ptr = next as *mut u8;

        Some(aligned as *mut u8)
    }

    /// Saves the current allocation boundary.
    ///
    /// The checkpoint stores only the information required to restore this
    /// allocation state.
    #[inline(always)]
    pub const fn checkpoint(
        &self,
    ) -> ArenaCheckpoint {
        ArenaCheckpoint::new(self.bump_ptr)
    }

    /// Restores a previous allocation boundary.
    ///
    /// # Safety
    ///
    /// The checkpoint must originate from this arena lifetime.
    #[inline(always)]
    pub unsafe fn restore(
        &mut self,
        checkpoint: ArenaCheckpoint,
    ) {
        let ptr = checkpoint.as_ptr();

        debug_assert!(
            ptr as usize >= self.base_ptr as usize
        );

        debug_assert!(
            ptr as usize <= self.limit_ptr as usize
        );

        self.bump_ptr = ptr;
    }

    /// Resets the arena to the initial allocation boundary.
    #[inline(always)]
    pub fn reset(&mut self) {
        self.bump_ptr = self.base_ptr;
    }

    /// Returns the number of allocated bytes.
    #[inline(always)]
    pub fn used(
        &self,
    ) -> usize {
        self.bump_ptr as usize
            - self.base_ptr as usize
    }

    /// Returns the remaining allocation capacity.
    #[inline(always)]
    pub fn remaining(
        &self,
    ) -> usize {
        self.limit_ptr as usize
            - self.bump_ptr as usize
    }

    /// Returns whether the arena contains no allocations.
    #[inline(always)]
    pub fn is_empty(
        &self,
    ) -> bool {
        self.bump_ptr == self.base_ptr
    }

    /// Returns whether the arena has no remaining capacity.
    #[inline(always)]
    pub fn is_full(
        &self,
    ) -> bool {
        self.bump_ptr == self.limit_ptr
    }

    /// Validates the arena boundary invariant.
    #[inline(always)]
    pub fn validate(
        &self,
    ) -> bool {
        let base = self.base_ptr as usize;
        let current = self.bump_ptr as usize;
        let limit = self.limit_ptr as usize;

        base <= current
            && current <= limit
            && limit - base == self.capacity
    }
}