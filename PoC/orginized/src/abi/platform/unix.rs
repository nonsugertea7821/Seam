//! Unix platform implementation.
//!
//! This module provides Unix-specific virtual memory operations required by
//! the Seam VM ABI.
//!
//! The platform layer converts operating-system memory primitives into the
//! platform-independent `VirtualMapping` and `OperatorArena` abstractions.

use std::{
    ffi::c_void,
    ptr,
};

use libc::{
    mmap,
    mprotect,
    munmap,
    sysconf,
    MAP_ANONYMOUS,
    MAP_FAILED,
    MAP_PRIVATE,
    PROT_NONE,
    PROT_READ,
    PROT_WRITE,
    _SC_PAGESIZE,
};

use crate::abi::{
    AbiError,
    OperatorArena,
    VirtualMapping,
};

/// Allocates a guarded arena using Unix virtual memory.
///
/// Memory layout:
///
/// +------------+----------------------+------------+
/// | Guard Page |    Operator Arena    | Guard Page |
/// +------------+----------------------+------------+
pub fn alloc(
    capacity: usize,
) -> Result<(OperatorArena, VirtualMapping), AbiError> {
    unsafe {
        let page_size = page_size()?;

        let capacity = align_capacity(
            capacity,
            page_size,
        )?;

        let total_size = capacity
            .checked_add(page_size)
            .and_then(|size| size.checked_add(page_size))
            .ok_or(AbiError::MappingOverflow)?;

        let ptr = mmap(
            ptr::null_mut(),
            total_size,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            -1,
            0,
        );

        if ptr == MAP_FAILED {
            return Err(AbiError::MemoryAllocationFailed);
        }

        let mapping = VirtualMapping::new(
            ptr,
            total_size,
            release,
        );

        let base = ptr.cast::<u8>();
        let arena_base = base.add(page_size);
        let arena_limit = arena_base.add(capacity);

        if let Err(error) = protect_guard(
            base,
            page_size,
        ) {
            return cleanup(mapping, error);
        }

        if let Err(error) = protect_guard(
            arena_limit,
            page_size,
        ) {
            return cleanup(mapping, error);
        }

        let arena = OperatorArena::new(
            arena_base,
            arena_limit,
            capacity,
        );

        Ok((arena, mapping))
    }
}

/// Returns the native Unix page size.
fn page_size() -> Result<usize, AbiError> {
    unsafe {
        let size = sysconf(_SC_PAGESIZE);

        if size <= 0 {
            return Err(AbiError::InvalidPageSize);
        }

        Ok(size as usize)
    }
}

/// Aligns capacity to the system page size.
fn align_capacity(
    capacity: usize,
    page_size: usize,
) -> Result<usize, AbiError> {
    capacity
        .checked_add(page_size - 1)
        .map(|value| value & !(page_size - 1))
        .ok_or(AbiError::CapacityOverflow)
}

/// Applies a no-access guard page.
unsafe fn protect_guard(
    ptr: *mut u8,
    size: usize,
) -> Result<(), AbiError> {
    if mprotect(
        ptr.cast::<c_void>(),
        size,
        PROT_NONE,
    ) != 0
    {
        return Err(AbiError::GuardPageProtectionFailed);
    }

    Ok(())
}

/// Releases a Unix virtual memory mapping.
unsafe fn release(
    ptr: *mut c_void,
    size: usize,
) -> Result<(), AbiError> {
    if munmap(
        ptr,
        size,
    ) != 0
    {
        return Err(AbiError::MemoryDeallocationFailed);
    }

    Ok(())
}

/// Releases a partially initialized mapping while preserving errors.
fn cleanup(
    mapping: VirtualMapping,
    error: AbiError,
) -> Result<(OperatorArena, VirtualMapping), AbiError> {
    match mapping.release() {
        Ok(()) => Err(error),
        Err(cleanup_error) => Err(AbiError::MultipleFailures {
            primary: Box::new(error),
            secondary: Box::new(cleanup_error),
        }),
    }
}