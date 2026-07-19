//! CFP/RFP Hybrid Context — Physical Register Bindings
//! 
//! Manages physical CPU registers for abort/collector semantics:
//! - CFP (Control Frame Pointer): Current execution context
//! - RFP (Resource Frame Pointer): Aborted frame for cleanup
//! 
//! Physical register bindings:
//! - x86-64: CFP=rbp, RFP=r15, arena_ptr=r14
//! - AArch64: CFP=x29, RFP=x28, arena_ptr=x27

use std::cell::Cell;

/// Physical register bindings per architecture
#[cfg(target_arch = "x86_64")]
pub struct PhysicalRegisters {
    /// rbp: Control Frame Pointer (current execution context parent)
    pub cfp: *mut u8,
    /// r15: Resource Frame Pointer (abort target frame)
    pub rfp: *mut u8,
    /// r14: Arena pointer (PSSA allocation frontier)
    pub arena_ptr: *mut u8,
}

#[cfg(target_arch = "aarch64")]
pub struct PhysicalRegisters {
    /// x29: Control Frame Pointer (current execution context parent)
    pub cfp: *mut u8,
    /// x28: Resource Frame Pointer (abort target frame)
    pub rfp: *mut u8,
    /// x27: Arena pointer (PSSA allocation frontier)
    pub arena_ptr: *mut u8,
}

/// Hybrid context switching for abort — simultaneous CFP/RFP modification
/// 
/// This enables O(1) abort with direct jump without stack unwinding.
/// The core mechanism of Seam language's abort semantics.
pub struct HybridContextSwitch {
    /// Target CFP for control transfer
    target_cfp: *mut u8,
    /// Target RFP for resource cleanup
    target_rfp: *mut u8,
    /// Collector entry point instruction pointer
    collector_ip: *const u8,
    /// Collector channel identity used for parent-boundary resolution
    collector_channel_id: u32,
}

impl HybridContextSwitch {
    /// Create context switch for abort → collector path
    /// 
    /// # Arguments
    /// - target_cfp: New control frame (where collector executes)
    /// - target_rfp: Ghost frame (aborted context for cleanup access)
    /// - collector_ip: Entry point of collector function
    pub fn new(
        target_cfp: *mut u8,
        target_rfp: *mut u8,
        collector_ip: *const u8,
        collector_channel_id: u32,
    ) -> Self {
        HybridContextSwitch {
            target_cfp,
            target_rfp,
            collector_ip,
            collector_channel_id,
        }
    }

    /// Execute direct jump with simultaneous CFP/RFP switch
    /// 
    /// This is the core abort mechanism — no stack unwinding, no dynamic dispatch.
    /// DRAFT spec: "ダイレクトジャンプ" with O(1) register modification
    /// 
    /// # Safety
    /// Caller must ensure:
    /// - CFP/RFP point to valid frame addresses in PSSA
    /// - collector_ip is a valid code address
    /// - No other code is accessing CFP/RFP during this operation
    #[inline(never)]
    pub unsafe fn execute_direct_jump(&self) {
        #[cfg(target_arch = "x86_64")]
        {
            // x86-64: 3 instruction sequence for O(1) abort
            // mov rbp, cfp_value   — set new control context
            // mov r15, rfp_value   — set ghost frame for cleanup
            // jmp collector_ip     — jump without CALL (no return address)
            core::arch::asm!(
                "mov rbp, {cfp}",
                "mov r15, {rfp}",
                "jmp {collector}",
                cfp = in(reg) self.target_cfp,
                rfp = in(reg) self.target_rfp,
                collector = in(reg) self.collector_ip,
                options(noreturn)
            );
        }

        #[cfg(target_arch = "aarch64")]
        {
            // AArch64: 3 instruction sequence for O(1) abort
            // mov x29, cfp_value   — set new control context
            // mov x28, rfp_value   — set ghost frame for cleanup
            // br collector_ip      — branch without BL (no return address)
            core::arch::asm!(
                "mov x29, {cfp}",
                "mov x28, {rfp}",
                "br {collector}",
                cfp = in(reg) self.target_cfp,
                rfp = in(reg) self.target_rfp,
                collector = in(reg) self.collector_ip,
                options(noreturn)
            );
        }
    }

    /// Get target CFP (for inspection, not direct register access)
    pub fn get_target_cfp(&self) -> *mut u8 {
        self.target_cfp
    }

    /// Get target RFP (for inspection, not direct register access)
    pub fn get_target_rfp(&self) -> *mut u8 {
        self.target_rfp
    }

    /// Get collector IP (for inspection, not direct register access)
    pub fn get_collector_ip(&self) -> *const u8 {
        self.collector_ip
    }

    /// Get collector channel identity.
    pub fn get_collector_channel_id(&self) -> u32 {
        self.collector_channel_id
    }
}

thread_local! {
    static HYBRID_CONTEXT: Cell<Option<(usize, usize)>> = Cell::new(None);
}

/// Set current CFP/RFP values in thread-local storage
pub fn set_hybrid_context(cfp: usize, rfp: usize) {
    HYBRID_CONTEXT.with(|ctx| ctx.set(Some((cfp, rfp))));
}

/// Get current CFP/RFP values
pub fn get_hybrid_context() -> Option<(usize, usize)> {
    HYBRID_CONTEXT.with(|ctx| ctx.get())
}

/// Clear hybrid context (typically at channel completion)
pub fn clear_hybrid_context() {
    HYBRID_CONTEXT.with(|ctx| ctx.set(None));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hybrid_context_creation() {
        let cfp = std::ptr::null_mut();
        let rfp = std::ptr::null_mut();
        let collector_ip = std::ptr::null();

        let switch = HybridContextSwitch::new(cfp, rfp, collector_ip, 7);
        assert_eq!(switch.get_target_cfp(), cfp);
        assert_eq!(switch.get_target_rfp(), rfp);
        assert_eq!(switch.get_collector_ip(), collector_ip);
        assert_eq!(switch.get_collector_channel_id(), 7);
    }

    #[test]
    fn test_context_switch_creation_with_offsets() {
        // Simulate frame addresses in PSSA arena
        let base = 0x1000 as *mut u8;
        let cfp = base.wrapping_add(512);
        let rfp = base.wrapping_add(256);
        let collector_ip = 0x4000 as *const u8;

        let switch = HybridContextSwitch::new(cfp, rfp, collector_ip, 11);
        assert_eq!(switch.get_target_cfp(), cfp);
        assert_eq!(switch.get_target_rfp(), rfp);
        assert_eq!(switch.get_collector_channel_id(), 11);
    }

    #[test]
    fn test_thread_local_hybrid_context() {
        set_hybrid_context(0x1000, 0x2000);
        let (cfp, rfp) = get_hybrid_context().expect("Context not set");
        assert_eq!(cfp, 0x1000);
        assert_eq!(rfp, 0x2000);

        clear_hybrid_context();
        assert!(get_hybrid_context().is_none());
    }

    #[test]
    fn test_physical_register_layout() {
        // Verify register bindings match DRAFT spec
        #[cfg(target_arch = "x86_64")]
        {
            // x86-64 callee-saved registers:
            // rbp (CFP), r12-r15 (r15=RFP, r14=arena_ptr, r12-r13 spare)
            PhysicalRegisters {
                cfp: std::ptr::null_mut(),
                rfp: std::ptr::null_mut(),
                arena_ptr: std::ptr::null_mut(),
            };
            assert_eq!(std::mem::size_of::<PhysicalRegisters>(), 24); // 3 * 8 bytes
        }

        #[cfg(target_arch = "aarch64")]
        {
            // AArch64 callee-saved registers:
            // x29 (CFP), x28 (RFP), x27 (arena_ptr)
            let regs = PhysicalRegisters {
                cfp: std::ptr::null_mut(),
                rfp: std::ptr::null_mut(),
                arena_ptr: std::ptr::null_mut(),
            };
            assert_eq!(std::mem::size_of::<PhysicalRegisters>(), 24); // 3 * 8 bytes
        }
    }
}
