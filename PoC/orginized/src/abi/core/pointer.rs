//! ABI execution context pointer definitions.
//!
//! This module defines the strongly typed execution pointers used by the
//! Seam ABI.
//!
//! These types describe execution state only. They do not manipulate CPU
//! registers directly. Register synchronization is delegated to the
//! architecture layer.

/// Control Operator Pointer (COP).
///
/// COP identifies the currently active execution context.
///
/// Register binding:
/// - x86-64: rbp
/// - AArch64: x29
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ControlOperatorPtr(*mut u8);

impl ControlOperatorPtr {
    /// Creates a null control pointer.
    #[inline(always)]
    pub const fn null() -> Self {
        Self(core::ptr::null_mut())
    }

    /// Creates a control pointer from a raw address.
    #[inline(always)]
    pub const fn from_raw(ptr: *mut u8) -> Self {
        Self(ptr)
    }

    /// Returns the underlying raw pointer.
    #[inline(always)]
    pub const fn as_raw(self) -> *mut u8 {
        self.0
    }

    /// Returns whether this pointer is null.
    #[inline(always)]
    pub const fn is_null(self) -> bool {
        self.0.is_null()
    }
}

/// Resource Pointer (RP).
///
/// RP preserves the execution state of an aborted operator until the
/// collector completes post-mortem processing.
///
/// Register binding:
/// - x86-64: r15
/// - AArch64: x28
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ResourcePtr(*mut u8);

impl ResourcePtr {
    /// Creates a null resource pointer.
    #[inline(always)]
    pub const fn null() -> Self {
        Self(core::ptr::null_mut())
    }

    /// Creates a resource pointer from a raw address.
    #[inline(always)]
    pub const fn from_raw(ptr: *mut u8) -> Self {
        Self(ptr)
    }

    /// Returns the underlying raw pointer.
    #[inline(always)]
    pub const fn as_raw(self) -> *mut u8 {
        self.0
    }

    /// Returns whether this pointer is null.
    #[inline(always)]
    pub const fn is_null(self) -> bool {
        self.0.is_null()
    }
}