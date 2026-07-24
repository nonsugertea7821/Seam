//! ABI error definitions.
//!
//! This module defines failures exposed by the Seam VM ABI boundary.
//!
//! ABI errors describe semantic failures rather than operating-system error
//! codes. Platform implementations translate native failures into these
//! variants before returning control to the runtime.

use std::{error::Error, fmt};

/// Represents an ABI operation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiError {
    /// The native system page size could not be determined.
    InvalidPageSize,

    /// The requested arena capacity exceeded representable limits.
    CapacityOverflow,

    /// The complete virtual memory mapping size exceeded representable limits.
    MappingOverflow,

    /// The operating system failed to allocate virtual memory.
    MemoryAllocationFailed,

    /// A guard page could not be configured.
    GuardPageProtectionFailed,

    /// The operating system failed to release virtual memory.
    MemoryDeallocationFailed,

    /// The original operation failed and cleanup also failed.
    ///
    /// The primary error represents the original failure.
    /// The secondary error represents the failed recovery operation.
    MultipleFailures {
        /// Original operation failure.
        primary: Box<AbiError>,

        /// Recovery or cleanup failure.
        secondary: Box<AbiError>,
    },
}

impl AbiError {
    /// Returns the original failure when multiple failures are present.
    #[inline(always)]
    pub fn primary(&self) -> &Self {
        match self {
            Self::MultipleFailures { primary, .. } => primary,
            _ => self,
        }
    }

    /// Returns whether this error contains multiple failures.
    #[inline(always)]
    pub const fn is_composite(&self) -> bool {
        matches!(self, Self::MultipleFailures { .. })
    }
}

impl fmt::Display for AbiError {
    fn fmt(
        &self,
        formatter: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            Self::InvalidPageSize =>
                write!(formatter, "invalid native page size"),

            Self::CapacityOverflow =>
                write!(formatter, "arena capacity overflow"),

            Self::MappingOverflow =>
                write!(formatter, "virtual memory mapping overflow"),

            Self::MemoryAllocationFailed =>
                write!(formatter, "virtual memory allocation failed"),

            Self::GuardPageProtectionFailed =>
                write!(formatter, "guard page protection failed"),

            Self::MemoryDeallocationFailed =>
                write!(formatter, "virtual memory deallocation failed"),

            Self::MultipleFailures {
                primary,
                secondary,
            } =>
                write!(
                    formatter,
                    "multiple ABI failures: primary=({}), secondary=({})",
                    primary,
                    secondary,
                ),
        }
    }
}

impl Error for AbiError {}