//! AArch64 architecture primitives for ABI state manipulation.
//!
//! This module provides raw CPU operations required by the ABI layer.
//! Register semantics are defined outside this module.

use core::arch::asm;

/// Moves a value into x27.
#[inline(always)]
pub unsafe fn write_x27(value: *mut u8) {
    asm!(
        "mov x27, {}",
        in(reg) value,
        options(nostack, preserves_flags)
    );
}

/// Moves a value into x28.
#[inline(always)]
pub unsafe fn write_x28(value: *mut u8) {
    asm!(
        "mov x28, {}",
        in(reg) value,
        options(nostack, preserves_flags)
    );
}

/// Moves a value into x29.
#[inline(always)]
pub unsafe fn write_x29(value: *mut u8) {
    asm!(
        "mov x29, {}",
        in(reg) value,
        options(nostack, preserves_flags)
    );
}

/// Performs an indirect control transfer.
///
/// The target address must be valid executable code.
#[inline(always)]
pub unsafe fn branch(target: *mut u8) -> ! {
    asm!(
        "br {}",
        in(reg) target,
        options(noreturn, nostack)
    );
}