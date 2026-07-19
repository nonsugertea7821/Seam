//! Global Resource Definition and Management
//!
//! Defines shared mutable resources that can be accessed from multiple channels
//! and implements the 2PST resource commit protocol

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::ptr;

/// Status word bits for SeamGlobalResource
/// Bit 0: Lock (1 = locked, 0 = unlocked)
/// Bit 1: Poisoned (1 = corrupted state, 0 = valid)
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct ResourceStatus(u64);

impl ResourceStatus {
    pub fn new() -> Self {
        ResourceStatus(0)
    }

    #[inline]
    pub fn is_locked(self) -> bool {
        self.0 & 1 != 0
    }

    #[inline]
    pub fn is_poisoned(self) -> bool {
        (self.0 >> 1) & 1 != 0
    }

    #[inline]
    pub fn set_locked(self) -> Self {
        ResourceStatus(self.0 | 1)
    }

    #[inline]
    pub fn set_unlocked(self) -> Self {
        ResourceStatus(self.0 & !1)
    }

    #[inline]
    pub fn set_poisoned(self) -> Self {
        ResourceStatus(self.0 | 2)
    }
}

/// Global resource descriptor
/// Represents a shared resource that may be accessed by multiple threads
#[repr(C)]
pub struct GlobalResource {
    /// Status word: Bit 0 = lock, Bit 1 = poisoned
    status: AtomicU64,
    /// Pointer to actual data (allocated externally)
    data_ptr: *mut u8,
    /// Size of data
    size: usize,
    /// Resource ID for ordering during commit
    resource_id: u32,
}

impl GlobalResource {
    /// Create a new global resource
    pub fn new(resource_id: u32, size: usize, data: *mut u8) -> Arc<Self> {
        Arc::new(GlobalResource {
            status: AtomicU64::new(0),
            data_ptr: data,
            size,
            resource_id,
        })
    }

    /// Acquire lock on this resource (2PST)
    pub fn acquire_lock(&self) -> bool {
        let mut status = self.status.load(Ordering::Acquire);
        loop {
            if ResourceStatus(status).is_locked() {
                return false; // Already locked - should not happen in static ordering
            }

            let new_status = ResourceStatus(status).set_locked().0;
            match self.status.compare_exchange_weak(
                status,
                new_status,
                Ordering::Release,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(actual) => status = actual,
            }
        }
    }

    /// Release lock on this resource
    pub fn release_lock(&self) {
        let status = self.status.load(Ordering::Acquire);
        let new_status = ResourceStatus(status).set_unlocked().0;
        self.status.store(new_status, Ordering::Release);
    }

    /// Mark resource as poisoned (in case of corruption)
    pub fn set_poisoned(&self) {
        let status = self.status.load(Ordering::Acquire);
        let new_status = ResourceStatus(status).set_poisoned().0;
        self.status.store(new_status, Ordering::Release);
    }

    /// Check if resource is poisoned
    pub fn is_poisoned(&self) -> bool {
        ResourceStatus(self.status.load(Ordering::Acquire)).is_poisoned()
    }

    /// Get resource ID
    #[inline]
    pub fn resource_id(&self) -> u32 {
        self.resource_id
    }

    /// Get data pointer
    #[inline]
    pub fn data_ptr(&self) -> *mut u8 {
        self.data_ptr
    }

    /// Get size
    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }

    /// Flush data from source to this resource (atomic write)
    pub unsafe fn atomic_flush(&self, src: *const u8, len: usize) {
        if len > self.size {
            panic!("Flush size exceeds resource size");
        }
        // Memory barrier before write (architecture-specific)
        #[cfg(target_arch = "x86_64")]
        std::arch::asm!("sfence");
        #[cfg(target_arch = "aarch64")]
        std::arch::asm!("dmb ish");
        
        // Copy data (volatile to prevent optimization)
        ptr::copy_nonoverlapping(src, self.data_ptr, len);
        
        // Memory barrier after write (architecture-specific)
        #[cfg(target_arch = "x86_64")]
        std::arch::asm!("sfence");
        #[cfg(target_arch = "aarch64")]
        std::arch::asm!("dmb ish");
    }
}

/// Unique record wrapper for zero-copy I/O
/// Represents data owned by a specific execution path
#[repr(C)]
pub struct UniqueRecord {
    /// Pointer to data in PSSA arena
    ptr: *mut u8,
    /// Size of record
    size: usize,
    /// Record type ID
    record_type: u32,
}

impl UniqueRecord {
    /// Create a new unique record
    pub fn new(ptr: *mut u8, size: usize, record_type: u32) -> Self {
        UniqueRecord {
            ptr,
            size,
            record_type,
        }
    }

    /// Get pointer
    #[inline]
    pub fn ptr(&self) -> *mut u8 {
        self.ptr
    }

    /// Get size
    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }

    /// Get record type
    #[inline]
    pub fn record_type(&self) -> u32 {
        self.record_type
    }
}

/// Resource access descriptor for static analysis
#[repr(C)]
pub struct ResourceAccess {
    /// Resource ID being accessed
    pub resource_id: u32,
    /// Offset into resource
    pub offset: usize,
    /// Size of access
    pub size: usize,
    /// Read (0) or Write (1)
    pub is_write: bool,
}

/// Collection of resource accesses for a path (static)
pub struct AccessSet {
    /// Sorted by resource_id for atomic commit
    accesses: Vec<ResourceAccess>,
}

impl AccessSet {
    /// Create new access set
    pub fn new() -> Self {
        AccessSet {
            accesses: Vec::new(),
        }
    }

    /// Add access
    pub fn add_access(&mut self, access: ResourceAccess) {
        self.accesses.push(access);
    }

    /// Get accesses sorted by resource ID
    pub fn sorted_accesses(&self) -> Vec<&ResourceAccess> {
        let mut sorted: Vec<_> = self.accesses.iter().collect();
        sorted.sort_by_key(|a| a.resource_id);
        sorted
    }

    /// Get write accesses only
    pub fn write_accesses(&self) -> Vec<&ResourceAccess> {
        self.accesses.iter().filter(|a| a.is_write).collect()
    }
}

impl Default for AccessSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_creation() {
        let mut data = vec![0u8; 256];
        let resource = GlobalResource::new(1, 256, data.as_mut_ptr());
        assert_eq!(resource.resource_id(), 1);
        assert_eq!(resource.size(), 256);
    }

    #[test]
    fn test_resource_lock() {
        let mut data = vec![0u8; 256];
        let resource = GlobalResource::new(1, 256, data.as_mut_ptr());
        
        assert!(!ResourceStatus(resource.status.load(Ordering::Relaxed)).is_locked());
        assert!(resource.acquire_lock());
        assert!(ResourceStatus(resource.status.load(Ordering::Relaxed)).is_locked());
        
        resource.release_lock();
        assert!(!ResourceStatus(resource.status.load(Ordering::Relaxed)).is_locked());
    }

    #[test]
    fn test_access_set_sorting() {
        let mut set = AccessSet::new();
        set.add_access(ResourceAccess {
            resource_id: 3,
            offset: 0,
            size: 64,
            is_write: true,
        });
        set.add_access(ResourceAccess {
            resource_id: 1,
            offset: 0,
            size: 32,
            is_write: false,
        });
        set.add_access(ResourceAccess {
            resource_id: 2,
            offset: 32,
            size: 64,
            is_write: true,
        });

        let sorted = set.sorted_accesses();
        assert_eq!(sorted[0].resource_id, 1);
        assert_eq!(sorted[1].resource_id, 2);
        assert_eq!(sorted[2].resource_id, 3);
    }
}
