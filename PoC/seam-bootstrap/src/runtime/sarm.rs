//! SARM (Static Abort Register Map) — Register Restoration Metadata
//!
//! Stored in .rodata, used at abort time to restore callee-saved registers.
//! Enables deterministic register state after direct jump abort.
//!
//! Each abort point has associated metadata:
//! - Which registers were saved at entry
//! - Offset from RFP to save area
//! - Target collector IP

use std::collections::BTreeMap;

/// Single SARM entry mapping abort point to register restoration data
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SARMEntry {
    /// Channel ID where abort occurred
    pub abort_channel_id: u32,
    /// Bitmask of callee-saved registers to restore
    /// x86-64: bit 0=rbx, 1=r12, 2=r13, 3=r14, 4=r15, ...
    /// AArch64: bits 0-10 for x19-x28
    pub callee_saved_mask: u32,
    /// Offset from RFP to register save area
    pub rfp_offset_to_saved: i32,
    /// Collector entry point instruction pointer
    pub collector_target_ip: *const u8,
}

/// Complete SARM table for a compiled program
/// Enables O(log n) lookup of abort metadata
pub struct SARMTable {
    /// Map: abort_channel_id → SARMEntry (sorted for determinism)
    entries: BTreeMap<u32, SARMEntry>,
}

impl SARMTable {
    /// Create new empty SARM table
    pub fn new() -> Self {
        SARMTable {
            entries: BTreeMap::new(),
        }
    }

    /// Register abort point with callee-saved metadata
    /// Called by compiler codegen
    pub fn register_abort_point(
        &mut self,
        channel_id: u32,
        callee_saved_mask: u32,
        rfp_offset: i32,
        collector_ip: *const u8,
    ) -> Result<(), String> {
        if self.entries.contains_key(&channel_id) {
            return Err(format!("Duplicate channel ID in SARM: {}", channel_id));
        }

        self.entries.insert(channel_id, SARMEntry {
            abort_channel_id: channel_id,
            callee_saved_mask,
            rfp_offset_to_saved: rfp_offset,
            collector_target_ip: collector_ip,
        });

        Ok(())
    }

    /// Look up SARM entry for abort (O(log n) BTreeMap lookup)
    pub fn lookup(&self, channel_id: u32) -> Option<&SARMEntry> {
        self.entries.get(&channel_id)
    }

    /// Get all SARM entries (for serialization to .rodata)
    pub fn all_entries(&self) -> Vec<&SARMEntry> {
        self.entries.values().collect()
    }

    /// Restore callee-saved registers based on SARM entry
    /// Called by abort handler after direct jump
    ///
    /// # Safety
    /// Caller must ensure RFP points to valid register save area
    #[cfg(target_arch = "x86_64")]
    pub unsafe fn restore_registers(&self, channel_id: u32, rfp: *mut u8) -> Result<(), String> {
        let entry = self.lookup(channel_id)
            .ok_or(format!("No SARM entry for channel {}", channel_id))?;

        if entry.callee_saved_mask == 0 {
            return Ok(()); // No registers to restore
        }

        let save_ptr = rfp.offset(entry.rfp_offset_to_saved as isize) as *const u64;

        // x86-64 callee-saved registers restoration
        // Bitmask encoding:
        // Bit 0: rbx, Bit 1: r12, Bit 2: r13, Bit 3: r14, Bit 4: r15
        let mask = entry.callee_saved_mask;
        let mut _offset = 0;

        if mask & 0x01 != 0 {
            // Restore rbx
            core::arch::asm!(
                "mov rbx, [{}]",
                in(reg) save_ptr.offset(_offset),
                options(nostack, preserves_flags)
            );
            _offset += 1;
        }

        if mask & 0x02 != 0 {
            // Restore r12
            core::arch::asm!(
                "mov r12, [{}]",
                in(reg) save_ptr.offset(_offset),
                options(nostack, preserves_flags)
            );
            _offset += 1;
        }

        if mask & 0x04 != 0 {
            // Restore r13
            core::arch::asm!(
                "mov r13, [{}]",
                in(reg) save_ptr.offset(_offset),
                options(nostack, preserves_flags)
            );
            _offset += 1;
        }

        // r14 (arena_ptr) and r15 (RFP) are already set by direct jump

        Ok(())
    }

    #[cfg(target_arch = "aarch64")]
    pub unsafe fn restore_registers(&self, channel_id: u32, rfp: *mut u8) -> Result<(), String> {
        let entry = self.lookup(channel_id)
            .ok_or(format!("No SARM entry for channel {}", channel_id))?;

        if entry.callee_saved_mask == 0 {
            return Ok(()); // No registers to restore
        }

        let save_ptr = rfp.offset(entry.rfp_offset_to_saved as isize) as *const u64;

        // AArch64 callee-saved: x19-x28 (10 registers)
        // Restore pairs efficiently with ldp
        if entry.callee_saved_mask & 0x03 != 0 {
            core::arch::asm!(
                "ldp x19, x20, [{}]",
                in(reg) save_ptr,
                options(nostack, preserves_flags)
            );
        }

        // x29 (CFP) and x28 (RFP) are already set by direct jump

        Ok(())
    }

    /// Serialize SARM table to bytes (for .rodata section)
    pub fn serialize_to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Header: entry count (u32)
        bytes.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());

        // Entries in sorted order (BTreeMap guarantees ordering)
        for (_, entry) in &self.entries {
            bytes.extend_from_slice(&entry.abort_channel_id.to_le_bytes());
            bytes.extend_from_slice(&entry.callee_saved_mask.to_le_bytes());
            bytes.extend_from_slice(&entry.rfp_offset_to_saved.to_le_bytes());
            bytes.extend_from_slice(&(entry.collector_target_ip as u64).to_le_bytes());
        }

        bytes
    }

    /// Deserialize SARM table from bytes
    pub fn deserialize_from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 4 {
            return Err("SARM buffer too small".to_string());
        }

        let mut table = SARMTable::new();
        let entry_count = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;

        let mut pos = 4;
        for _ in 0..entry_count {
            if pos + 20 > bytes.len() {
                return Err("SARM buffer truncated".to_string());
            }

            let channel_id = u32::from_le_bytes([
                bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]
            ]);
            let callee_saved_mask = u32::from_le_bytes([
                bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]
            ]);
            let rfp_offset = i32::from_le_bytes([
                bytes[pos + 8], bytes[pos + 9], bytes[pos + 10], bytes[pos + 11]
            ]);
            let collector_ip = u64::from_le_bytes([
                bytes[pos + 12], bytes[pos + 13], bytes[pos + 14], bytes[pos + 15],
                bytes[pos + 16], bytes[pos + 17], bytes[pos + 18], bytes[pos + 19]
            ]) as *const u8;

            table.register_abort_point(channel_id, callee_saved_mask, rfp_offset, collector_ip)?;
            pos += 20;
        }

        Ok(table)
    }

    /// Number of SARM entries
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Total serialized size in bytes
    pub fn serialized_size(&self) -> usize {
        4 + (self.entries.len() * 16) // 4-byte header + 16 bytes per entry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sarm_registration() {
        let mut sarm = SARMTable::new();
        sarm.register_abort_point(1, 0x0F, 32, std::ptr::null()).expect("registration failed");
        assert_eq!(sarm.entry_count(), 1);
    }

    #[test]
    fn test_sarm_lookup() {
        let mut sarm = SARMTable::new();
        let collector = 0x4000 as *const u8;
        sarm.register_abort_point(42, 0xFF, 16, collector).unwrap();

        let entry = sarm.lookup(42).expect("lookup failed");
        assert_eq!(entry.abort_channel_id, 42);
        assert_eq!(entry.callee_saved_mask, 0xFF);
        assert_eq!(entry.rfp_offset_to_saved, 16);
        assert_eq!(entry.collector_target_ip, collector);
    }

    #[test]
    fn test_sarm_duplicate_rejection() {
        let mut sarm = SARMTable::new();
        sarm.register_abort_point(1, 0x0F, 32, std::ptr::null()).unwrap();

        let result = sarm.register_abort_point(1, 0x0F, 32, std::ptr::null());
        assert!(result.is_err());
    }

    #[test]
    fn test_sarm_multiple_entries() {
        let mut sarm = SARMTable::new();
        sarm.register_abort_point(1, 0x0F, 32, std::ptr::null()).unwrap();
        sarm.register_abort_point(2, 0x1F, 64, std::ptr::null()).unwrap();
        sarm.register_abort_point(3, 0x2F, 48, std::ptr::null()).unwrap();

        assert_eq!(sarm.entry_count(), 3);
        assert!(sarm.lookup(2).is_some());
    }

    #[test]
    fn test_sarm_serialization() {
        let mut sarm = SARMTable::new();
        sarm.register_abort_point(1, 0x0F, 32, std::ptr::null()).unwrap();
        sarm.register_abort_point(2, 0x1F, 64, std::ptr::null()).unwrap();

        let bytes = sarm.serialize_to_bytes();
        // Header (4) + 2 entries * 20 bytes each = 44 bytes
        assert_eq!(bytes.len(), 4 + 2 * 20);
        assert!(bytes.len() > 4); // At least header
    }

    #[test]
    fn test_sarm_serialization_roundtrip() {
        let mut sarm1 = SARMTable::new();
        let collector1 = 0x1000 as *const u8;
        let collector2 = 0x2000 as *const u8;

        sarm1.register_abort_point(10, 0x0F, 16, collector1).unwrap();
        sarm1.register_abort_point(20, 0x1F, 32, collector2).unwrap();

        let bytes = sarm1.serialize_to_bytes();
        let sarm2 = SARMTable::deserialize_from_bytes(&bytes).expect("deserialization failed");

        assert_eq!(sarm2.entry_count(), 2);
        assert!(sarm2.lookup(10).is_some());
        assert!(sarm2.lookup(20).is_some());
    }

    #[test]
    fn test_sarm_entry_ordering() {
        // BTreeMap ensures deterministic ordering
        let mut sarm = SARMTable::new();
        sarm.register_abort_point(100, 0x00, 0, std::ptr::null()).unwrap();
        sarm.register_abort_point(50, 0x00, 0, std::ptr::null()).unwrap();
        sarm.register_abort_point(75, 0x00, 0, std::ptr::null()).unwrap();

        let all = sarm.all_entries();
        assert_eq!(all[0].abort_channel_id, 50);
        assert_eq!(all[1].abort_channel_id, 75);
        assert_eq!(all[2].abort_channel_id, 100);
    }
}
