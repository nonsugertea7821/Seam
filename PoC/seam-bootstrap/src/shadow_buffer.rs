//! Shadow Buffer for Speculative Execution (2PST Phase 1)
//!
//! Each thread maintains a shadow buffer within its PSSA arena for writes
//! during speculative execution. On commit, these are flushed to main memory.

use std::collections::HashMap;

/// Shadow buffer entry representing a pending write
#[repr(C)]
pub struct ShadowWrite {
    /// Resource ID being written to
    resource_id: u32,
    /// Offset within resource
    offset: usize,
    /// Data size
    size: usize,
    /// Data pointer (in shadow buffer)
    data: *mut u8,
}

impl ShadowWrite {
    /// Create new shadow write
    pub fn new(resource_id: u32, offset: usize, size: usize, data: *mut u8) -> Self {
        ShadowWrite {
            resource_id,
            offset,
            size,
            data,
        }
    }

    pub fn resource_id(&self) -> u32 {
        self.resource_id
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn data(&self) -> *mut u8 {
        self.data
    }
}

/// Thread-local shadow buffer for speculative execution
/// Maps resource_id -> list of pending writes
pub struct ShadowBuffer {
    /// Writes grouped by resource ID
    writes: HashMap<u32, Vec<ShadowWrite>>,
}

impl ShadowBuffer {
    /// Create new shadow buffer
    pub fn new() -> Self {
        ShadowBuffer {
            writes: HashMap::new(),
        }
    }

    /// Record a speculative write
    pub fn add_write(&mut self, resource_id: u32, offset: usize, size: usize, data: *mut u8) {
        let write = ShadowWrite::new(resource_id, offset, size, data);
        self.writes
            .entry(resource_id)
            .or_insert_with(Vec::new)
            .push(write);
    }

    /// Get writes for a specific resource
    pub fn get_writes(&self, resource_id: u32) -> Option<&Vec<ShadowWrite>> {
        self.writes.get(&resource_id)
    }

    /// Get all resource IDs with pending writes (sorted)
    pub fn resource_ids(&self) -> Vec<u32> {
        let mut ids: Vec<_> = self.writes.keys().copied().collect();
        ids.sort();
        ids
    }

    /// Clear shadow buffer
    pub fn clear(&mut self) {
        self.writes.clear();
    }

    /// Get total number of pending writes
    pub fn write_count(&self) -> usize {
        self.writes.values().map(|v| v.len()).sum()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
    }
}

impl Default for ShadowBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Shadow buffer pool for managing multiple speculative paths
pub struct ShadowBufferPool {
    /// Map: path_id -> shadow buffer
    buffers: HashMap<u32, ShadowBuffer>,
}

impl ShadowBufferPool {
    /// Create new pool
    pub fn new() -> Self {
        ShadowBufferPool {
            buffers: HashMap::new(),
        }
    }

    /// Get or create buffer for path
    pub fn get_buffer_mut(&mut self, path_id: u32) -> &mut ShadowBuffer {
        self.buffers
            .entry(path_id)
            .or_insert_with(ShadowBuffer::new)
    }

    /// Get buffer (read-only)
    pub fn get_buffer(&self, path_id: u32) -> Option<&ShadowBuffer> {
        self.buffers.get(&path_id)
    }

    /// Remove buffer (typically on path completion)
    pub fn remove_buffer(&mut self, path_id: u32) -> Option<ShadowBuffer> {
        self.buffers.remove(&path_id)
    }

    /// Clear all buffers
    pub fn clear(&mut self) {
        self.buffers.clear();
    }

    /// Get number of active paths
    pub fn active_paths(&self) -> usize {
        self.buffers.len()
    }
}

impl Default for ShadowBufferPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shadow_buffer_creation() {
        let buffer = ShadowBuffer::new();
        assert!(buffer.is_empty());
        assert_eq!(buffer.write_count(), 0);
    }

    #[test]
    fn test_shadow_write_recording() {
        let mut buffer = ShadowBuffer::new();
        let data = vec![1u8, 2, 3, 4];
        let data_ptr = data.as_ptr() as *mut u8;

        buffer.add_write(1, 0, 4, data_ptr);
        assert_eq!(buffer.write_count(), 1);
        assert!(buffer.get_writes(1).is_some());
    }

    #[test]
    fn test_resource_id_sorting() {
        let mut buffer = ShadowBuffer::new();
        let data = vec![0u8; 16];
        let ptr = data.as_ptr() as *mut u8;

        buffer.add_write(3, 0, 4, ptr);
        buffer.add_write(1, 0, 4, ptr);
        buffer.add_write(2, 0, 4, ptr);

        let ids = buffer.resource_ids();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn test_shadow_buffer_pool() {
        let mut pool = ShadowBufferPool::new();
        let buf1 = pool.get_buffer_mut(1);
        assert!(buf1.is_empty());

        let data = vec![0u8; 8];
        let ptr = data.as_ptr() as *mut u8;
        buf1.add_write(1, 0, 8, ptr);

        assert_eq!(pool.active_paths(), 1);
        assert_eq!(pool.get_buffer(1).unwrap().write_count(), 1);
    }
}
