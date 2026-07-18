// x86-64 Architecture Support
//
// Register mapping:
// CFP (Control Frame Pointer) = rbp
// RFP (Resource Frame Pointer) = r15
// arena_ptr                     = r14

use crate::context::ResourceFramePtr;

pub mod intrinsics {

    /// Read CFP (rbp)
    #[inline]
    pub fn read_cfp() -> usize {
        let cfp: usize;
        unsafe {
            std::arch::asm!(
                "mov {}, rbp",
                out(reg) cfp,
                options(nomem, nostack, preserves_flags)
            );
        }
        cfp
    }

    /// Write CFP (rbp)
    #[inline]
    pub unsafe fn write_cfp(value: usize) {
        std::arch::asm!(
            "mov rbp, {}",
            in(reg) value,
            options(nostack, preserves_flags)
        );
    }

    /// Read RFP (r15)
    #[inline]
    pub fn read_rfp() -> usize {
        let rfp: usize;
        unsafe {
            std::arch::asm!(
                "mov {}, r15",
                out(reg) rfp,
                options(nomem, nostack, preserves_flags)
            );
        }
        rfp
    }

    /// Write RFP (r15)
    #[inline]
    pub unsafe fn write_rfp(value: usize) {
        std::arch::asm!(
            "mov r15, {}",
            in(reg) value,
            options(nostack, preserves_flags)
        );
    }

    /// Read arena_ptr (r14)
    #[inline]
    pub fn read_arena_ptr() -> usize {
        let ptr: usize;
        unsafe {
            std::arch::asm!(
                "mov {}, r14",
                out(reg) ptr,
                options(nomem, nostack, preserves_flags)
            );
        }
        ptr
    }

    /// Write arena_ptr (r14)
    #[inline]
    pub unsafe fn write_arena_ptr(value: usize) {
        std::arch::asm!(
            "mov r14, {}",
            in(reg) value,
            options(nostack, preserves_flags)
        );
    }

    /// Bump allocate: arena_ptr += size
    /// Equivalent to: add qword ptr [r14], size
    #[inline]
    pub unsafe fn arena_bump(size: usize) {
        std::arch::asm!(
            "add qword ptr [r14], {}",
            in(reg) size,
            options(nostack)
        );
    }

    /// Memory fence for 2PST (Phase 2 commit)
    #[inline]
    pub unsafe fn memory_fence() {
        std::arch::asm!("sfence", options(nostack));
    }

    /// Store barrier
    #[inline]
    pub unsafe fn store_barrier() {
        std::arch::asm!("sfence", options(nostack));
    }
}

/// Restore callee-saved registers from saved area
/// 
/// On x86-64, callee-saved registers are: rbx, rbp, r12-r15
/// Mask bits correspond to register IDs
pub unsafe fn restore_registers_x86_64(saved_area: *const u8, mask: u32) {
    // This is a placeholder - actual implementation would restore from stack
    // Bit layout for mask:
    // 0: rbx, 1: rbp (r5), 2-5: r12-r15
    
    if mask & 0x01 != 0 {
        // Restore rbx
        let rbx: u64;
        std::arch::asm!("mov {}, qword ptr [{}]", out(reg) rbx, in(reg) saved_area);
        std::arch::asm!("mov rbx, {}", in(reg) rbx);
    }
    
    // Additional registers would be restored similarly
    let _ = saved_area;
}

/// Direct abort jump with context switch (x86-64)
/// 
/// This would typically be inline asm in real implementation:
/// mov rdi, rfp           ; first arg = RFP
/// mov rbp, new_cfp       ; CFP = parent frame
/// jmp collector_addr     ; jump to collector
pub unsafe fn abort_direct_jump_x86_64(rfp: ResourceFramePtr, collector_addr: *const u8) {
    std::arch::asm!(
        "mov rdi, {}",      // RFP as first argument
        "jmp {}",           // Jump to collector
        in(reg) rfp.0,
        in(reg) collector_addr,
        options(noreturn)
    );
}
