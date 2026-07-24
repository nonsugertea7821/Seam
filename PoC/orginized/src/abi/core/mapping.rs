//! Virtual memory mapping abstraction.
//!
//! A `VirtualMapping` represents ownership of an operating-system virtual
//! memory allocation.
//!
//! The purpose of this type is to separate:
//!
//! - native memory resource ownership,
//! - execution allocation state.
//!
//! `VirtualMapping` owns the lifetime of the OS resource.
//! `OperatorArena` only manages allocation boundaries inside that resource.
//!
//! Ownership contract:
//!
//! - exactly one owner exists at a time.
//! - releasing consumes the mapping.
//! - failed construction never exposes partial ownership.

use std::ffi::c_void;

use crate::abi::AbiError;

/// Represents an owned operating-system virtual memory mapping.
///
/// This type does not manage allocation state.
/// It only manages resource lifetime.
#[derive(Debug)]
pub struct VirtualMapping {
    /// Base address returned by the platform allocator.
    ptr: *mut c_void,

    /// Total mapped size.
    size: usize,

    /// Platform-specific release operation.
    release: unsafe fn(
        *mut c_void,
        usize,
    ) -> Result<(), AbiError>,
}

impl VirtualMapping {
    /// Creates a virtual memory mapping owner.
    ///
    /// # Safety
    ///
    /// The pointer must represent a valid mapping created by the matching
    /// platform allocator.
    ///
    /// The release function must release that mapping exactly once.
    #[inline(always)]
    pub(crate) unsafe fn new(
        ptr: *mut c_void,
        size: usize,
        release: unsafe fn(
            *mut c_void,
            usize,
        ) -> Result<(), AbiError>,
    ) -> Self {
        Self {
            ptr,
            size,
            release,
        }
    }

    /// Returns the mapping base address.
    #[inline(always)]
    pub const fn as_ptr(
        &self,
    ) -> *mut c_void {
        self.ptr
    }

    /// Returns the mapping size.
    #[inline(always)]
    pub const fn size(
        &self,
    ) -> usize {
        self.size
    }

    /// Releases the owned mapping.
    ///
    /// Ownership is consumed by this operation.
    pub fn release(
        self,
    ) -> Result<(), AbiError> {
        let ptr = self.ptr;
        let size = self.size;
        let release = self.release;

        std::mem::forget(self);

        unsafe {
            release(ptr, size)
        }
    }
}