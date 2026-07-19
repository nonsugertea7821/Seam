//! Direct Jump Resolution — Compile-Time :collect Binding
//!
//! Resolves :collect bindings at compile time to enable O(1) abort
//! without dynamic dispatch.
//!
//! DRAFT spec: Channel() :collect RecoveryChannel
//! → Static resolution at compilation
//! → collector_ip is known at generate time
//! → Direct jmp (not call) to collector entry

use std::collections::HashMap;
use std::cell::RefCell;
use crate::cfp_rfp::{get_hybrid_context, set_hybrid_context, HybridContextSwitch};

const NO_PARENT_CHANNEL_ID: u32 = u32::MAX;

/// Direct jump target for :collect resolution
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DirectJumpTarget {
    /// Collector entry point instruction pointer
    pub collector_ip: *const u8,
    /// Channel ID of collector (for identification)
    pub collector_channel_id: u32,
    /// Parent channel ID that introduced this collect boundary
    pub parent_channel_id: u32,
    /// CFP value for control transfer
    pub target_cfp: *mut u8,
    /// Offset from RFP where local resources start
    pub local_resource_offset: i32,
}

impl DirectJumpTarget {
    pub fn new(
        collector_ip: *const u8,
        collector_channel_id: u32,
        parent_channel_id: u32,
        target_cfp: *mut u8,
        local_resource_offset: i32,
    ) -> Self {
        DirectJumpTarget {
            collector_ip,
            collector_channel_id,
            parent_channel_id,
            target_cfp,
            local_resource_offset,
        }
    }

    /// Check whether this collect boundary has a static parent channel.
    pub fn has_parent_channel(&self) -> bool {
        self.parent_channel_id != NO_PARENT_CHANNEL_ID
    }
}

/// Collect binding resolution table
/// Maps channel invocation to its :collect target
#[derive(Clone)]
pub struct CollectBindingTable {
    /// Map: source_channel_id → direct jump target
    bindings: HashMap<u32, DirectJumpTarget>,
    /// Map: collector_channel_id → source_channel_id
    collector_sources: HashMap<u32, u32>,
}

impl CollectBindingTable {
    /// Create new collect binding table
    pub fn new() -> Self {
        CollectBindingTable {
            bindings: HashMap::new(),
            collector_sources: HashMap::new(),
        }
    }

    /// Register :collect binding
    /// Called by compiler codegen for each `Channel() :collect RecoveryChannel` pair
    pub fn register_collect_binding(
        &mut self,
        source_channel_id: u32,
        collector_channel_id: u32,
        parent_channel_id: u32,
        collector_ip: *const u8,
        target_cfp: *mut u8,
        local_resource_offset: i32,
    ) -> Result<(), String> {
        if self.bindings.contains_key(&source_channel_id) {
            return Err(format!("Duplicate collect binding for channel {}", source_channel_id));
        }

        if self.collector_sources.contains_key(&collector_channel_id) {
            return Err(format!("Duplicate collector channel binding for {}", collector_channel_id));
        }

        self.bindings.insert(
            source_channel_id,
            DirectJumpTarget::new(
                collector_ip,
                collector_channel_id,
                parent_channel_id,
                target_cfp,
                local_resource_offset,
            ),
        );
        self.collector_sources.insert(collector_channel_id, source_channel_id);

        Ok(())
    }

    /// Resolve direct jump target for abort (O(1) HashMap lookup)
    /// Returns the pre-computed jump target if found
    pub fn resolve(&self, source_channel_id: u32) -> Option<&DirectJumpTarget> {
        self.bindings.get(&source_channel_id)
    }

    /// Check if a channel has a collect binding
    pub fn has_binding(&self, source_channel_id: u32) -> bool {
        self.bindings.contains_key(&source_channel_id)
    }

    /// Resolve the source channel associated with a collector channel.
    pub fn source_for_collector(&self, collector_channel_id: u32) -> Option<u32> {
        self.collector_sources.get(&collector_channel_id).copied()
    }

    /// Resolve the collect boundary that should handle a secondary abort from a collector.
    pub fn resolve_secondary_abort_binding(&self, collector_channel_id: u32) -> Option<&DirectJumpTarget> {
        let source_channel_id = self.source_for_collector(collector_channel_id)?;
        let current_target = self.resolve(source_channel_id)?;

        if !current_target.has_parent_channel() {
            return None;
        }

        self.resolve(current_target.parent_channel_id)
    }

    /// Resolve the parent channel that owns the collect boundary for a collector.
    pub fn parent_channel_for_collector(&self, collector_channel_id: u32) -> Option<u32> {
        let source_channel_id = self.source_for_collector(collector_channel_id)?;
        self.resolve(source_channel_id).and_then(|target| {
            if target.has_parent_channel() {
                Some(target.parent_channel_id)
            } else {
                None
            }
        })
    }

    /// Execute direct jump to collector
    /// Called by abort handler
    ///
    /// # Safety
    /// Caller must ensure:
    /// - source_channel_id has a registered binding
    /// - CFP and collector_ip are valid
    pub unsafe fn execute_jump(
        &self,
        source_channel_id: u32,
        rfp: *mut u8,
    ) -> Result<(), String> {
        let target = self.resolve(source_channel_id)
            .ok_or(format!("No collect binding for channel {}", source_channel_id))?;

        self.execute_jump_to_target(target, rfp)
    }

    /// Execute the direct jump for a resolved target.
    unsafe fn execute_jump_to_target(
        &self,
        target: &DirectJumpTarget,
        rfp: *mut u8,
    ) -> Result<(), String> {

        #[cfg(target_arch = "x86_64")]
        {
            // x86-64: Direct jump with simultaneous register setup
            //   mov rbp, target_cfp      — control frame
            //   mov r15, rfp             — resource frame (ghost for cleanup)
            //   jmp collector_ip         — direct jump (not call)
            core::arch::asm!(
                "mov rbp, {cfp}",
                "mov r15, {rfp}",
                "jmp {collector}",
                cfp = in(reg) target.target_cfp,
                rfp = in(reg) rfp,
                collector = in(reg) target.collector_ip,
                options(noreturn)
            );
        }

        #[cfg(target_arch = "aarch64")]
        {
            // AArch64: Direct jump with simultaneous register setup
            //   mov x29, target_cfp      — control frame
            //   mov x28, rfp             — resource frame (ghost for cleanup)
            //   br collector_ip          — direct branch (not bl)
            core::arch::asm!(
                "mov x29, {cfp}",
                "mov x28, {rfp}",
                "br {collector}",
                cfp = in(reg) target.target_cfp,
                rfp = in(reg) rfp,
                collector = in(reg) target.collector_ip,
                options(noreturn)
            );
        }

        Ok(())
    }

    /// Execute a secondary-abort jump using the collector channel identity.
    ///
    /// This resolves the collector's parent collect boundary and jumps to that
    /// parent's collector directly.
    pub unsafe fn execute_secondary_abort_jump(
        &self,
        collector_channel_id: u32,
        rfp: *mut u8,
    ) -> Result<(), String> {
        let target = self
            .resolve_secondary_abort_binding(collector_channel_id)
            .ok_or(format!("No parent collect binding for collector channel {}", collector_channel_id))?;

        self.execute_jump_to_target(target, rfp)
    }

    /// Get number of registered bindings
    pub fn binding_count(&self) -> usize {
        self.bindings.len()
    }

    /// Get all bindings (for debugging/inspection)
    pub fn all_bindings(&self) -> Vec<(u32, &DirectJumpTarget)> {
        self.bindings.iter().map(|(k, v)| (*k, v)).collect()
    }

    /// Serialize collect bindings to bytes (for storage/transmission)
    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Header: binding count
        bytes.extend_from_slice(&(self.bindings.len() as u32).to_le_bytes());

        // Bindings
        for (source_id, target) in &self.bindings {
            bytes.extend_from_slice(&source_id.to_le_bytes());
            bytes.extend_from_slice(&target.collector_channel_id.to_le_bytes());
            bytes.extend_from_slice(&target.parent_channel_id.to_le_bytes());
            bytes.extend_from_slice(&(target.collector_ip as u64).to_le_bytes());
            bytes.extend_from_slice(&(target.target_cfp as u64).to_le_bytes());
            bytes.extend_from_slice(&target.local_resource_offset.to_le_bytes());
        }

        bytes
    }

    /// Deserialize collect bindings from bytes
    pub fn deserialize(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 4 {
            return Err("Buffer too small for header".to_string());
        }

        let count = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let mut table = CollectBindingTable::new();
        let mut pos = 4;

        for _ in 0..count {
            if pos + 32 > bytes.len() {
                return Err("Buffer truncated".to_string());
            }

            let source_id = u32::from_le_bytes([
                bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]
            ]);
            let collector_id = u32::from_le_bytes([
                bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]
            ]);
            let parent_channel_id = u32::from_le_bytes([
                bytes[pos + 8], bytes[pos + 9], bytes[pos + 10], bytes[pos + 11]
            ]);
            let collector_ip = u64::from_le_bytes([
                bytes[pos + 12], bytes[pos + 13], bytes[pos + 14], bytes[pos + 15],
                bytes[pos + 16], bytes[pos + 17], bytes[pos + 18], bytes[pos + 19]
            ]) as *const u8;
            let target_cfp = u64::from_le_bytes([
                bytes[pos + 20], bytes[pos + 21], bytes[pos + 22], bytes[pos + 23],
                bytes[pos + 24], bytes[pos + 25], bytes[pos + 26], bytes[pos + 27]
            ]) as *mut u8;
            let local_resource_offset = i32::from_le_bytes([
                bytes[pos + 28], bytes[pos + 29], bytes[pos + 30], bytes[pos + 31]
            ]);

            table.register_collect_binding(
                source_id,
                collector_id,
                parent_channel_id,
                collector_ip,
                target_cfp,
                local_resource_offset,
            )?;

            pos += 32;
        }

        Ok(table)
    }
}

thread_local! {
    static COLLECT_BINDINGS: RefCell<CollectBindingTable> = RefCell::new(CollectBindingTable::new());
}

/// Replace the thread-local collect bindings with the supplied table.
pub fn set_collect_bindings(table: CollectBindingTable) {
    COLLECT_BINDINGS.with(|bindings| {
        *bindings.borrow_mut() = table;
    });
}

/// Borrow the thread-local collect bindings and run a closure against them.
pub fn with_collect_bindings<R>(f: impl FnOnce(&CollectBindingTable) -> R) -> R {
    COLLECT_BINDINGS.with(|bindings| {
        let bindings = bindings.borrow();
        f(&bindings)
    })
}

/// Get a cloned snapshot of the thread-local collect bindings.
pub fn get_collect_bindings() -> CollectBindingTable {
    COLLECT_BINDINGS.with(|bindings| bindings.borrow().clone())
}

/// Configure per-context direct-jump state and synchronize thread-local CFP/RFP.
pub(crate) fn set_context_direct_jump_state(
    direct_jump_context: &mut Option<HybridContextSwitch>,
    target_cfp: *mut u8,
    target_rfp: *mut u8,
    collector_ip: *const u8,
    collector_channel_id: u32,
) {
    *direct_jump_context = Some(HybridContextSwitch::new(
        target_cfp,
        target_rfp,
        collector_ip,
        collector_channel_id,
    ));
    set_hybrid_context(target_cfp as usize, target_rfp as usize);
}

/// Clear per-context direct-jump state.
pub(crate) fn clear_context_direct_jump_state(
    direct_jump_context: &mut Option<HybridContextSwitch>,
) {
    *direct_jump_context = None;
}

/// Read current thread-local hybrid context (CFP, RFP).
pub(crate) fn current_context_hybrid_state() -> Option<(usize, usize)> {
    get_hybrid_context()
}

/// Execute direct-jump abort flow for primary/secondary aborts.
pub(crate) fn execute_context_abort_jump(
    direct_jump_context: &Option<HybridContextSwitch>,
    was_in_collector: bool,
    rfp: usize,
) -> Result<(), &'static str> {
    if was_in_collector && direct_jump_context.is_none() {
        return Err("Secondary abort detected - no direct jump context configured");
    }

    if let Some(direct_jump) = direct_jump_context {
        unsafe {
            if was_in_collector {
                let secondary_jump_result = with_collect_bindings(|bindings| {
                    bindings.execute_secondary_abort_jump(
                        direct_jump.get_collector_channel_id(),
                        rfp as *mut u8,
                    )
                });

                if secondary_jump_result.is_err() {
                    return Err("Secondary abort detected - escalating to parent collector failed");
                }

                unreachable!("secondary abort jump does not return");
            }

            // O(1) abort with direct jump (no stack unwinding).
            direct_jump.execute_direct_jump();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_binding_registration() {
        let mut table = CollectBindingTable::new();
        table.register_collect_binding(
            1,
            2,
            NO_PARENT_CHANNEL_ID,
            std::ptr::null(),
            std::ptr::null_mut(),
            32,
        ).unwrap();

        assert_eq!(table.binding_count(), 1);
    }

    #[test]
    fn test_collect_binding_resolution() {
        let mut table = CollectBindingTable::new();
        let collector_ip = 0x1000 as *const u8;
        let cfp = 0x2000 as *mut u8;

        table.register_collect_binding(42, 99, 7, collector_ip, cfp, 16).unwrap();

        let target = table.resolve(42).expect("resolution failed");
        assert_eq!(target.collector_channel_id, 99);
        assert_eq!(target.parent_channel_id, 7);
        assert_eq!(target.collector_ip, collector_ip);
        assert_eq!(target.target_cfp, cfp);
        assert_eq!(target.local_resource_offset, 16);
    }

    #[test]
    fn test_duplicate_binding_rejection() {
        let mut table = CollectBindingTable::new();
        table.register_collect_binding(1, 2, NO_PARENT_CHANNEL_ID, std::ptr::null(), std::ptr::null_mut(), 0).unwrap();

        let result = table.register_collect_binding(1, 3, NO_PARENT_CHANNEL_ID, std::ptr::null(), std::ptr::null_mut(), 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_binding() {
        let table = CollectBindingTable::new();
        assert!(table.resolve(999).is_none());
        assert!(!table.has_binding(999));
    }

    #[test]
    fn test_multiple_bindings() {
        let mut table = CollectBindingTable::new();

        for i in 0..10 {
            table.register_collect_binding(i, i + 100, NO_PARENT_CHANNEL_ID, std::ptr::null(), std::ptr::null_mut(), 0).unwrap();
        }

        assert_eq!(table.binding_count(), 10);

        for i in 0..10 {
            assert!(table.has_binding(i));
            assert_eq!(table.resolve(i).unwrap().collector_channel_id, i + 100);
        }
    }

    #[test]
    fn test_serialization_roundtrip() {
        let mut table1 = CollectBindingTable::new();

        table1.register_collect_binding(
            10,
            20,
            0,
            0x1000 as *const u8,
            0x2000 as *mut u8,
            32,
        ).unwrap();

        table1.register_collect_binding(
            30,
            40,
            10,
            0x3000 as *const u8,
            0x4000 as *mut u8,
            64,
        ).unwrap();

        let bytes = table1.serialize();
        let table2 = CollectBindingTable::deserialize(&bytes).expect("deserialization failed");

        assert_eq!(table2.binding_count(), 2);
        assert!(table2.has_binding(10));
        assert!(table2.has_binding(30));
    }

    #[test]
    fn test_all_bindings() {
        let mut table = CollectBindingTable::new();

        table.register_collect_binding(1, 10, NO_PARENT_CHANNEL_ID, std::ptr::null(), std::ptr::null_mut(), 0).unwrap();
        table.register_collect_binding(2, 20, 1, std::ptr::null(), std::ptr::null_mut(), 0).unwrap();

        let all = table.all_bindings();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_secondary_abort_resolution() {
        let mut table = CollectBindingTable::new();

        table.register_collect_binding(1, 11, 0, 0x1000 as *const u8, 0x2000 as *mut u8, 16).unwrap();
        table.register_collect_binding(0, 10, NO_PARENT_CHANNEL_ID, 0x3000 as *const u8, 0x4000 as *mut u8, 32).unwrap();

        let target = table.resolve_secondary_abort_binding(11).expect("secondary abort target");
        assert_eq!(target.collector_channel_id, 10);
        assert_eq!(target.parent_channel_id, NO_PARENT_CHANNEL_ID);
        assert_eq!(table.parent_channel_for_collector(11), Some(0));
    }
}
