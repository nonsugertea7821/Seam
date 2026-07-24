//! ABI control transfer.
//!
//! This module provides the primitive operation for moving execution between
//! ABI entries while preserving the required ABI execution state.
//!
//! The ABI layer does not interpret the meaning of the destination.
//! A destination may represent any valid ABI entry.
//!
//! This module is responsible only for:
//!
//! - binding ABI state to architecture-defined registers.
//! - transferring control to the target entry.
//!
//! This module does not:
//!
//! - resolve execution paths.
//! - determine collectors.
//! - restore arena state.
//! - manage resources.
//! - perform runtime cleanup.
//!
//! Those responsibilities belong to higher layers.

use crate::abi::{
    ArenaCheckpoint,
    OperatorPtr,
    ResourcePtr,
};

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
compile_error!("Seam ABI transfer is only supported on x86_64 and aarch64.");

/// ABI state required for a control transfer.
///
/// This structure represents the state contract between the runtime and an
/// ABI entry.
///
/// The ABI layer does not interpret the contents of each field. It only places
/// them into the architecture-defined locations before transferring control.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct TransferContext {
    /// Target ABI entry.
    pub entry: OperatorPtr,

    /// Resource state pointer.
    ///
    /// Bound to the ABI resource register.
    pub resource: ResourcePtr,

    /// Saved arena allocation boundary.
    ///
    /// Bound to the ABI checkpoint register.
    pub checkpoint: ArenaCheckpoint,
}

/// Transfers execution to an ABI entry.
///
/// This operation:
///
/// 1. Places ABI state into reserved registers.
/// 2. Transfers instruction control to the target entry.
///
/// The operation never returns.
///
/// # Safety
///
/// The caller must guarantee that:
///
/// - `entry` points to valid executable code.
/// - `resource` remains valid while required by the target entry.
/// - `checkpoint` is valid for the current ABI execution lifetime.
/// - The target entry satisfies the ABI calling convention.
#[inline(always)]
pub unsafe fn transfer(
    context: TransferContext,
) -> ! {
    bind_resource(context.resource);
    bind_checkpoint(context.checkpoint);
    bind_entry(context.entry);

    jump(context.entry);
}

/// Binds the resource pointer to the ABI register.
///
/// Register ownership is defined by the ABI contract.
#[inline(always)]
unsafe fn bind_resource(
    resource: ResourcePtr,
) {
    #[cfg(target_arch = "x86_64")]
    {
        crate::abi::arch::x86_64::write_r15(
            resource.as_raw(),
        );
    }

    #[cfg(target_arch = "aarch64")]
    {
        crate::abi::arch::aarch64::write_x28(
            resource.as_raw(),
        );
    }
}

/// Binds the checkpoint pointer to the ABI register.
///
/// The ABI layer only preserves the value. Restoration semantics are handled
/// by higher-level runtime logic.
#[inline(always)]
unsafe fn bind_checkpoint(
    checkpoint: ArenaCheckpoint,
) {
    #[cfg(target_arch = "x86_64")]
    {
        crate::abi::arch::x86_64::write_r14(
            checkpoint.as_ptr(),
        );
    }

    #[cfg(target_arch = "aarch64")]
    {
        crate::abi::arch::aarch64::write_x27(
            checkpoint.as_ptr(),
        );
    }
}

/// Binds the target entry to the ABI control register.
///
/// The target is an execution entry, not a Seam-specific collector.
#[inline(always)]
unsafe fn bind_entry(
    entry: OperatorPtr,
) {
    #[cfg(target_arch = "x86_64")]
    {
        crate::abi::arch::x86_64::write_rbp(
            entry.as_raw(),
        );
    }

    #[cfg(target_arch = "aarch64")]
    {
        crate::abi::arch::aarch64::write_x29(
            entry.as_raw(),
        );
    }
}

/// Performs the architecture-specific control transfer.
///
/// This operation does not preserve a return address.
#[inline(always)]
unsafe fn jump(
    entry: OperatorPtr,
) -> ! {
    #[cfg(target_arch = "x86_64")]
    {
        crate::abi::arch::x86_64::jump(
            entry.as_raw(),
        );
    }

    #[cfg(target_arch = "aarch64")]
    {
        crate::abi::arch::aarch64::branch(
            entry.as_raw(),
        );
    }
}