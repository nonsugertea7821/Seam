//! Abort and Collector Management
//!
//! Implements abort path and collector invocation with:
//! - Direct context jump (no stack unwinding)
//! - IC flag (In-Collector) for secondary abort prevention
//! - Static Abort Register Map (SARM)

use crate::context::ResourceFramePtr;
use crate::sarm::SARMEntry;
use std::collections::HashMap;

/// Abort signal definition
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbortSignal {
    /// Normal return (no abort)
    Ok = 0,
    /// User-triggered abort
    Abort = 1,
    /// System abort (OS error)
    SystemError = 2,
    /// Resource exhaustion
    OutOfMemory = 3,
    /// Invalid operation
    InvalidState = 4,
}

/// Collector entry point function signature
pub type CollectorFn = unsafe extern "C" fn(rfp: ResourceFramePtr) -> i32;

/// Collector table mapping channel IDs to collector functions
pub struct CollectorTable {
    /// Map: channel_id -> collector_function
    collectors: HashMap<u32, (CollectorFn, SARMEntry)>,
    /// SARM entries for register restoration
    sarm_entries: Vec<SARMEntry>,
}

impl CollectorTable {
    /// Create a new empty collector table
    pub fn new() -> Self {
        CollectorTable {
            collectors: HashMap::new(),
            sarm_entries: Vec::new(),
        }
    }

    /// Register a collector for a channel
    pub fn register(
        &mut self,
        channel_id: u32,
        collector: CollectorFn,
        sarm: SARMEntry,
    ) {
        self.collectors.insert(channel_id, (collector, sarm));
        self.sarm_entries.push(sarm);
    }

    /// Unregister a collector
    pub fn unregister(&mut self, channel_id: u32) {
        self.collectors.remove(&channel_id);
    }

    /// Invoke collector for a channel
    pub fn invoke(
        &self,
        channel_id: u32,
        rfp: ResourceFramePtr,
    ) -> Result<i32, &'static str> {
        if let Some((collector, _sarm)) = self.collectors.get(&channel_id) {
            Ok(unsafe { collector(rfp) })
        } else {
            Err("Collector not registered for channel")
        }
    }

    /// Get SARM entry for a channel
    pub fn get_sarm(&self, channel_id: u32) -> Option<&SARMEntry> {
        self.collectors.get(&channel_id).map(|(_, sarm)| sarm)
    }

    /// Restore callee-saved registers from SARM
    /// 
    /// # Safety
    /// Only call when abort is triggered and registers need restoration
    pub unsafe fn restore_registers(&self, rfp: ResourceFramePtr, sarm: &SARMEntry) {
        if sarm.callee_saved_mask == 0 {
            return; // No registers to restore
        }

        // This is architecture-specific and handled in arch modules
        // Here we just define the interface
        let saved_area = (rfp.0 as *const u8).add(sarm.rfp_offset_to_saved as usize);

        // On x86-64, this would restore rax, rcx, rdx, rsi, rdi, r8-r11
        // On AArch64, this would restore x0-x7, x16-x17
        
        // Implementation delegated to arch-specific code
        #[cfg(target_arch = "x86_64")]
        crate::arch::native::restore_registers_x86_64(saved_area, sarm.callee_saved_mask);

        #[cfg(target_arch = "aarch64")]
        crate::arch::native::restore_registers_aarch64(saved_area, sarm.callee_saved_mask);
    }
}

impl Default for CollectorTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Abort context for propagating abort information through the system
#[repr(C)]
pub struct AbortContext {
    /// Type of abort signal
    pub signal: AbortSignal,
    /// Error code if applicable
    pub error_code: i32,
    /// Message (if any)
    pub message_ptr: *const u8,
}

impl AbortContext {
    /// Create a new abort context
    pub fn new(signal: AbortSignal) -> Self {
        AbortContext {
            signal,
            error_code: 0,
            message_ptr: std::ptr::null(),
        }
    }

    /// Create abort context with error code
    pub fn with_error(signal: AbortSignal, code: i32) -> Self {
        AbortContext {
            signal,
            error_code: code,
            message_ptr: std::ptr::null(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collector_table() {
        let table = CollectorTable::new();
        assert!(table.get_sarm(1).is_none());
    }

    #[test]
    fn test_abort_context() {
        let ctx = AbortContext::new(AbortSignal::Abort);
        assert_eq!(ctx.signal, AbortSignal::Abort);
        assert_eq!(ctx.error_code, 0);
    }
}
