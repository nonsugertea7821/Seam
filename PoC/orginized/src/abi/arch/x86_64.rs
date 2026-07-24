//! x86-64 architecture primitives for ABI state manipulation.
//!
//! This module provides raw CPU operations required by the ABI layer.
//! It does not define the meaning of registers.
//! Register ownership and semantics are defined by abi/core.

use core::arch::asm;

/// Moves a value into r14.
///
/// The ABI layer binds this register externally as required.
#[inline(always)]
pub unsafe fn write_r14(value: *mut u8) {
    asm!(
        "mov r14, {}",
        in(reg) value,
        options(nostack, preserves_flags)
    );
}

/// Moves a value into r15.
#[inline(always)]
pub unsafe fn write_r15(value: *mut u8) {
    asm!(
        "mov r15, {}",
        in(reg) value,
        options(nostack, preserves_flags)
    );
}

/// Moves a value into rbp.
#[inline(always)]
pub unsafe fn write_rbp(value: *mut u8) {
    asm!(
        "mov rbp, {}",
        in(reg) value,
        options(nostack, preserves_flags)
    );
}

/// Performs an indirect control transfer.
///
/// The target address must be valid executable code.
#[inline(always)]
pub unsafe fn jump(target: *mut u8) -> ! {
    asm!(
        "jmp {}",
        in(reg) target,
        options(noreturn, nostack)
    );
}