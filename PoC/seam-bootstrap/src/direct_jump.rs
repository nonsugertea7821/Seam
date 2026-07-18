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

/// Direct jump target for :collect resolution
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DirectJumpTarget {
    /// Collector entry point instruction pointer
    pub collector_ip: *const u8,
    /// Channel ID of collector (for identification)
    pub collector_channel_id: u32,
    /// CFP value for control transfer
    pub target_cfp: *mut u8,
    /// Offset from RFP where local resources start
    pub local_resource_offset: i32,
}

impl DirectJumpTarget {
    pub fn new(
        collector_ip: *const u8,
        collector_channel_id: u32,
        target_cfp: *mut u8,
        local_resource_offset: i32,
    ) -> Self {
        DirectJumpTarget {
            collector_ip,
            collector_channel_id,
            target_cfp,
            local_resource_offset,
        }
    }
}

/// Collect binding resolution table
/// Maps channel invocation to its :collect target
pub struct CollectBindingTable {
    /// Map: source_channel_id → direct jump target
    bindings: HashMap<u32, DirectJumpTarget>,
}

impl CollectBindingTable {
    /// Create new collect binding table
    pub fn new() -> Self {
        CollectBindingTable {
            bindings: HashMap::new(),
        }
    }

    /// Register :collect binding
    /// Called by compiler codegen for each `Channel() :collect RecoveryChannel` pair
    pub fn register_collect_binding(
        &mut self,
        source_channel_id: u32,
        collector_channel_id: u32,
        collector_ip: *const u8,
        target_cfp: *mut u8,
        local_resource_offset: i32,
    ) -> Result<(), String> {
        if self.bindings.contains_key(&source_channel_id) {
            return Err(format!("Duplicate collect binding for channel {}", source_channel_id));
        }

        self.bindings.insert(
            source_channel_id,
            DirectJumpTarget::new(collector_ip, collector_channel_id, target_cfp, local_resource_offset),
        );

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
            if pos + 20 > bytes.len() {
                return Err("Buffer truncated".to_string());
            }

            let source_id = u32::from_le_bytes([
                bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]
            ]);
            let collector_id = u32::from_le_bytes([
                bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]
            ]);
            let collector_ip = u64::from_le_bytes([
                bytes[pos + 8], bytes[pos + 9], bytes[pos + 10], bytes[pos + 11],
                bytes[pos + 12], bytes[pos + 13], bytes[pos + 14], bytes[pos + 15]
            ]) as *const u8;
            let target_cfp = u64::from_le_bytes([
                bytes[pos + 16], bytes[pos + 17], bytes[pos + 18], bytes[pos + 19],
                bytes[pos + 20], bytes[pos + 21], bytes[pos + 22], bytes[pos + 23]
            ]) as *mut u8;
            let local_resource_offset = i32::from_le_bytes([
                bytes[pos + 24], bytes[pos + 25], bytes[pos + 26], bytes[pos + 27]
            ]);

            table.register_collect_binding(
                source_id,
                collector_id,
                collector_ip,
                target_cfp,
                local_resource_offset,
            )?;

            pos += 28;
        }

        Ok(table)
    }
}

thread_local! {
    static COLLECT_BINDINGS: CollectBindingTable = CollectBindingTable::new();
}

/// Get or create thread-local collect bindings
pub fn get_collect_bindings() -> &'static CollectBindingTable {
    thread_local! {
        static BINDINGS: CollectBindingTable = CollectBindingTable::new();
    }
    COLLECT_BINDINGS.with(|bindings| unsafe { std::mem::transmute::<&CollectBindingTable, &'static CollectBindingTable>(bindings) })
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

        table.register_collect_binding(42, 99, collector_ip, cfp, 16).unwrap();

        let target = table.resolve(42).expect("resolution failed");
        assert_eq!(target.collector_channel_id, 99);
        assert_eq!(target.collector_ip, collector_ip);
        assert_eq!(target.target_cfp, cfp);
        assert_eq!(target.local_resource_offset, 16);
    }

    #[test]
    fn test_duplicate_binding_rejection() {
        let mut table = CollectBindingTable::new();
        table.register_collect_binding(1, 2, std::ptr::null(), std::ptr::null_mut(), 0).unwrap();

        let result = table.register_collect_binding(1, 3, std::ptr::null(), std::ptr::null_mut(), 0);
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
            table.register_collect_binding(i, i + 100, std::ptr::null(), std::ptr::null_mut(), 0).unwrap();
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
            0x1000 as *const u8,
            0x2000 as *mut u8,
            32,
        ).unwrap();

        table1.register_collect_binding(
            30,
            40,
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

        table.register_collect_binding(1, 10, std::ptr::null(), std::ptr::null_mut(), 0).unwrap();
        table.register_collect_binding(2, 20, std::ptr::null(), std::ptr::null_mut(), 0).unwrap();

        let all = table.all_bindings();
        assert_eq!(all.len(), 2);
    }
}
