// AArch64 (ARM64) Architecture Support
//
// Register mapping:
// CFP (Control Frame Pointer) = x29
// RFP (Resource Frame Pointer) = x28
// arena_ptr                     = x27

use crate::context::ResourceFramePtr;

pub mod intrinsics {

    /// Read CFP (x29)
    #[inline]
    pub fn read_cfp() -> usize {
        let cfp: usize;
        unsafe {
            std::arch::asm!(
                "mov {}, x29",
                out(reg) cfp,
                options(nomem, nostack, preserves_flags)
            );
        }
        cfp
    }

    /// Write CFP (x29)
    #[inline]
    pub unsafe fn write_cfp(value: usize) {
        std::arch::asm!(
            "mov x29, {}",
            in(reg) value,
            options(nostack, preserves_flags)
        );
    }

    /// Read RFP (x28)
    #[inline]
    pub fn read_rfp() -> usize {
        let rfp: usize;
        unsafe {
            std::arch::asm!(
                "mov {}, x28",
                out(reg) rfp,
                options(nomem, nostack, preserves_flags)
            );
        }
        rfp
    }

    /// Write RFP (x28)
    #[inline]
    pub unsafe fn write_rfp(value: usize) {
        std::arch::asm!(
            "mov x28, {}",
            in(reg) value,
            options(nostack, preserves_flags)
        );
    }

    /// Read arena_ptr (x27)
    #[inline]
    pub fn read_arena_ptr() -> usize {
        let ptr: usize;
        unsafe {
            std::arch::asm!(
                "mov {}, x27",
                out(reg) ptr,
                options(nomem, nostack, preserves_flags)
            );
        }
        ptr
    }

    /// Write arena_ptr (x27)
    #[inline]
    pub unsafe fn write_arena_ptr(value: usize) {
        std::arch::asm!(
            "mov x27, {}",
            in(reg) value,
            options(nostack, preserves_flags)
        );
    }

    /// Bump allocate: arena_ptr += size
    #[inline]
    pub unsafe fn arena_bump(size: usize) {
        std::arch::asm!(
            "add x27, x27, {}",
            in(reg) size,
            options(nostack)
        );
    }

    /// Data memory barrier (ARM64 equivalent of sfence)
    #[inline]
    pub unsafe fn memory_fence() {
        std::arch::asm!("dmb ish", options(nostack));
    }

    /// Store barrier (DMB ISH ST)
    #[inline]
    pub unsafe fn store_barrier() {
        std::arch::asm!("dmb ishst", options(nostack));
    }
}

/// Restore callee-saved registers from saved area
///
/// On AArch64, callee-saved registers are: x19-x29, sp, lr (x30)
/// Mask bits correspond to register IDs
pub unsafe fn restore_registers_aarch64(saved_area: *const u8, mask: u32) {
    // Placeholder - actual implementation would restore x19-x28 from saved area
    // Bit layout for mask:
    // Bits 0-10: x19-x29
    // Bit 11: sp (x31)
    // Bit 12: lr (x30)
    
    if mask & 0x01 != 0 {
        // Restore x19
        let x19: u64;
        std::arch::asm!("ldr {}, [{}]", out(reg) x19, in(reg) saved_area);
        std::arch::asm!("mov x19, {}", in(reg) x19);
    }
    
    // Additional registers would be restored similarly
    let _ = saved_area;
}

use crate::context::ResourceFramePtr;

/// Direct abort jump with context switch (AArch64)
///
/// mov x0, rfp            ; first arg = RFP (x0)
/// mov x29, new_cfp       ; CFP = parent frame
/// br collector_addr      ; jump to collector
pub unsafe fn abort_direct_jump_aarch64(rfp: ResourceFramePtr, _collector_addr: *const u8) {
    // Placeholder: In real implementation, would directly jump to collector
    let _ = rfp;
}
