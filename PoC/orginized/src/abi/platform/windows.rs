//! Windows platform implementation.
//!
//! This module provides Windows-specific virtual memory operations required
//! by the Seam VM ABI.
//!
//! The platform layer is responsible only for constructing and releasing
//! virtual memory resources. Runtime allocation behavior is implemented by
//! `OperatorArena`.

use std::{
    ffi::c_void,
    mem::zeroed,
    ptr,
};

use windows_sys::Win32::{
    System::{
        Memory::{
            VirtualAlloc,
            VirtualFree,
            VirtualProtect,
            MEM_COMMIT,
            MEM_RELEASE,
            MEM_RESERVE,
            PAGE_NOACCESS,
            PAGE_READWRITE,
        },
        SystemInformation::{
            GetSystemInfo,
            SYSTEM_INFO,
        },
    },
};

use crate::abi::{
    AbiError,
    OperatorArena,
    VirtualMapping,
};

/// Allocates a guarded arena.
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

        let capacity = align_capacity(capacity, page_size)?;

        let total_size = capacity
            .checked_add(page_size)
            .and_then(|size| size.checked_add(page_size))
            .ok_or(AbiError::MappingOverflow)?;

        let ptr = VirtualAlloc(
            ptr::null_mut(),
            total_size,
            MEM_RESERVE | MEM_COMMIT,
            PAGE_READWRITE,
        );

        if ptr.is_null() {
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

/// Returns the native Windows page size.
fn page_size() -> Result<usize, AbiError> {
    unsafe {
        let mut info: SYSTEM_INFO = zeroed();

        GetSystemInfo(&mut info);

        let size = info.dwPageSize as usize;

        if size == 0 {
            return Err(AbiError::InvalidPageSize);
        }

        Ok(size)
    }
}

/// Aligns arena capacity to the native page size.
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
    let mut old = 0;

    if VirtualProtect(
        ptr.cast::<c_void>(),
        size,
        PAGE_NOACCESS,
        &mut old,
    ) == 0
    {
        return Err(AbiError::GuardPageProtectionFailed);
    }

    Ok(())
}

/// Releases a Windows virtual memory mapping.
unsafe fn release(
    ptr: *mut c_void,
    _: usize,
) -> Result<(), AbiError> {
    if VirtualFree(
        ptr,
        0,
        MEM_RELEASE,
    ) == 0
    {
        return Err(AbiError::MemoryDeallocationFailed);
    }

    Ok(())
}

/// Releases a partially initialized mapping and preserves the original error.
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