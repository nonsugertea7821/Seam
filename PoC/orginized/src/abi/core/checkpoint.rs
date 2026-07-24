//! Arena checkpoint definition.
//!
//! An `ArenaCheckpoint` represents a saved allocation boundary of an
//! `OperatorArena`.
//!
//! A checkpoint does not own memory and does not modify arena state.
//! It only stores the allocation position required to restore the arena
//! execution state.
//!
//! The validity of a checkpoint is bound to the lifetime of the arena that
//! created it.

/// Immutable snapshot of an arena allocation boundary.
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ArenaCheckpoint {
    /// Saved allocation boundary.
    bump_ptr: *mut u8,
}

impl ArenaCheckpoint {
    /// Creates a checkpoint from an allocation boundary.
    ///
    /// This constructor is restricted to the ABI layer.
    ///
    /// External users should obtain checkpoints through
    /// `OperatorArena::checkpoint()`.
    #[inline(always)]
    pub(crate) const fn new(
        bump_ptr: *mut u8,
    ) -> Self {
        Self {
            bump_ptr,
        }
    }

    /// Returns the saved allocation boundary.
    ///
    /// The returned pointer is only meaningful when used with the arena from
    /// which this checkpoint was created.
    #[inline(always)]
    pub const fn as_ptr(
        self,
    ) -> *mut u8 {
        self.bump_ptr
    }
}