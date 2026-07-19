//! Shadow Arena — Thread-Local Shadow Buffers for 2PST Phase 1
//!
//! Each fork path maintains an isolated shadow buffer for speculative writes.
//! Shared resources (OS handles, syscalls) remain accessible but tracked.
//!
//! 2PST Phase 1: Speculative Execution
//! - Path 0,1,2,... execute with independent shadow buffers (lock-free)
//! - Writes go to shadow buffer, not main memory
//! - Shared resources (OS) can be accessed with explicit tracking

use std::sync::atomic::{AtomicUsize, Ordering};
use std::cell::RefCell;
use std::collections::HashMap;

/// Single staged write in shadow buffer
#[derive(Clone)]
pub struct StagedWrite {
    /// Resource ID being written
    resource_id: u32,
    /// Offset within resource
    offset: usize,
    /// Staged data
    data: Vec<u8>
}

impl StagedWrite {
    pub fn new(resource_id: u32, offset: usize, data: Vec<u8>) -> Self {
        StagedWrite {
            resource_id,
            offset,
            data
        }
    }

    pub fn resource_id(&self) -> u32 {
        self.resource_id
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn size(&self) -> usize {
        self.data.len()
    }
}

/// Per-path shadow buffer — isolated write staging area
pub struct ShadowBuffer {
    /// Writes staged by this path: resource_id → vec of writes
    writes: HashMap<u32, Vec<StagedWrite>>,
    /// Total staged bytes for this path
    staged_bytes: usize,
}

impl ShadowBuffer {
    pub fn new() -> Self {
        ShadowBuffer {
            writes: HashMap::new(),
            staged_bytes: 0,
        }
    }

    /// Add write to shadow buffer (Phase 1: speculative)
    pub fn add_write(&mut self, resource_id: u32, offset: usize, data: Vec<u8>) {
        let size = data.len();
        self.staged_bytes += size;

        let write = StagedWrite::new(resource_id, offset, data);
        self.writes.entry(resource_id)
            .or_insert_with(Vec::new)
            .push(write);
    }

    /// Get writes for a resource
    pub fn get_writes(&self, resource_id: u32) -> Option<&Vec<StagedWrite>> {
        self.writes.get(&resource_id)
    }

    /// Get all resource IDs with pending writes
    pub fn resource_ids(&self) -> Vec<u32> {
        self.writes.keys().cloned().collect()
    }

    /// Total staged bytes in this path's buffer
    pub fn staged_bytes(&self) -> usize {
        self.staged_bytes
    }

    /// Clear shadow buffer (Phase 3: abort cleanup)
    pub fn clear(&mut self) {
        self.writes.clear();
        self.staged_bytes = 0;
    }

    /// Is shadow buffer empty?
    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
    }
}

/// Shared resource access tracker for fork paths
/// Tracks which path accesses which OS resource
#[derive(Clone, Copy, Debug)]
pub enum SharedResourceAccess {
    /// Read-only access (no conflict)
    Read,
    /// Write access (potential conflict with other writers)
    Write,
    /// Read-modify-write
    ReadWrite,
}

/// Per-path shared resource accesses
pub struct SharedResourceAccessSet {
    /// Shared resource ID → access type
    accesses: HashMap<u32, SharedResourceAccess>,
}

impl SharedResourceAccessSet {
    pub fn new() -> Self {
        SharedResourceAccessSet {
            accesses: HashMap::new(),
        }
    }

    pub fn record_access(&mut self, resource_id: u32, access: SharedResourceAccess) {
        self.accesses.insert(resource_id, access);
    }

    pub fn get_accesses(&self) -> &HashMap<u32, SharedResourceAccess> {
        &self.accesses
    }

    pub fn conflicts_with(&self, other: &SharedResourceAccessSet) -> Vec<u32> {
        let mut conflicts = Vec::new();
        for (rid, my_access) in &self.accesses {
            if let Some(other_access) = other.accesses.get(rid) {
                // Write-write or write-read conflicts
                match (my_access, other_access) {
                    (SharedResourceAccess::Write, _) | (_, SharedResourceAccess::Write) => {
                        conflicts.push(*rid);
                    }
                    _ => {}
                }
            }
        }
        conflicts
    }
}

/// Per-thread shadow arena managing all fork paths
pub struct ShadowArena {
    /// Active shadow buffers: path_id → buffer
    buffers: RefCell<HashMap<u32, ShadowBuffer>>,
    /// Shared resource accesses per path
    shared_accesses: RefCell<HashMap<u32, SharedResourceAccessSet>>,
    /// Total staged bytes across all paths
    total_staged: AtomicUsize,
    /// Generation counter for snapshot isolation
    generation: AtomicUsize,
}

impl ShadowArena {
    /// Create new shadow arena for current thread
    pub fn new() -> Self {
        ShadowArena {
            buffers: RefCell::new(HashMap::new()),
            shared_accesses: RefCell::new(HashMap::new()),
            total_staged: AtomicUsize::new(0),
            generation: AtomicUsize::new(0),
        }
    }

    /// Create shadow buffer for fork path
    /// Returns path_id for later writes
    pub fn create_path_buffer(&self, path_id: u32) -> Result<u32, String> {
        let mut bufs = self.buffers.borrow_mut();
        if bufs.contains_key(&path_id) {
            return Err(format!("Path {} already has buffer", path_id));
        }

        bufs.insert(path_id, ShadowBuffer::new());
        let mut shared = self.shared_accesses.borrow_mut();
        shared.insert(path_id, SharedResourceAccessSet::new());

        Ok(path_id)
    }

    /// Record write to path's shadow buffer (Phase 1: speculative)
    pub fn shadow_write(&self, path_id: u32, resource_id: u32, offset: usize, data: Vec<u8>) -> Result<(), String> {
        let mut bufs = self.buffers.borrow_mut();
        let buf = bufs.get_mut(&path_id)
            .ok_or(format!("Path {} buffer not found", path_id))?;

        let size = data.len();
        buf.add_write(resource_id, offset, data);
        self.total_staged.fetch_add(size, Ordering::Release);

        Ok(())
    }

    /// Record shared resource access (for conflict detection)
    pub fn record_shared_access(&self, path_id: u32, resource_id: u32, access: SharedResourceAccess) -> Result<(), String> {
        let mut shared = self.shared_accesses.borrow_mut();
        if let Some(set) = shared.get_mut(&path_id) {
            set.record_access(resource_id, access);
            Ok(())
        } else {
            Err(format!("Path {} accesses not initialized", path_id))
        }
    }

    /// Detect conflicts between paths' shared resource accesses
    /// Returns: Vec<(path_a, path_b, shared_resource_id)>
    pub fn detect_shared_conflicts(&self) -> Vec<(u32, u32, u32)> {
        let shared = self.shared_accesses.borrow();
        let mut path_ids: Vec<u32> = shared.keys().cloned().collect();
        path_ids.sort();  // Deterministic ordering for reproducible conflict detection
        let mut conflicts = Vec::new();

        for i in 0..path_ids.len() {
            for j in (i + 1)..path_ids.len() {
                let path_a = path_ids[i];
                let path_b = path_ids[j];

                if let (Some(set_a), Some(set_b)) = (shared.get(&path_a), shared.get(&path_b)) {
                    let resource_conflicts = set_a.conflicts_with(set_b);
                    for rid in resource_conflicts {
                        conflicts.push((path_a, path_b, rid));
                    }
                }
            }
        }

        conflicts
    }

    /// Phase 2: Atomic flush from shadow → main memory
    /// Caller must ensure static lock ordering
    pub fn atomic_flush_to_main(&self, path_id: u32, targets: &[(u32, *mut u8)]) -> Result<(), String> {
        let bufs = self.buffers.borrow();
        let buf = bufs.get(&path_id)
            .ok_or(format!("Path {} buffer not found", path_id))?;

        for (resource_id, main_ptr) in targets {
            if let Some(writes) = buf.get_writes(*resource_id) {
                for write in writes {
                    unsafe {
                        // Write atomically with release semantics
                        std::ptr::copy_nonoverlapping(
                            write.data().as_ptr(),
                            main_ptr.offset(write.offset() as isize),
                            write.size(),
                        );
                        // Ensure visibility before lock release
                        std::sync::atomic::compiler_fence(Ordering::Release);
                    }
                }
            }
        }

        Ok(())
    }

    /// Phase 3: Clear shadow buffer on abort
    pub fn clear_path_buffer(&self, path_id: u32) -> Result<(), String> {
        let mut bufs = self.buffers.borrow_mut();
        if let Some(buf) = bufs.get_mut(&path_id) {
            let freed = buf.staged_bytes();
            buf.clear();
            self.total_staged.fetch_sub(freed, Ordering::Release);
            Ok(())
        } else {
            Err(format!("Path {} buffer not found", path_id))
        }
    }

    /// Clear all buffers (for fork completion)
    pub fn clear_all(&self) {
        let mut bufs = self.buffers.borrow_mut();
        let _total = bufs.values().map(|b| b.staged_bytes()).sum::<usize>();
        bufs.clear();
        self.total_staged.store(0, Ordering::Release);

        let mut shared = self.shared_accesses.borrow_mut();
        shared.clear();
    }

    /// Total staged bytes across all paths
    pub fn total_staged(&self) -> usize {
        self.total_staged.load(Ordering::Acquire)
    }

    /// Current generation for snapshot isolation
    pub fn generation(&self) -> usize {
        self.generation.load(Ordering::Acquire)
    }

    /// Increment generation (for new fork epoch)
    pub fn next_generation(&self) {
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// Get staged bytes for a specific path
    pub fn path_staged_bytes(&self, path_id: u32) -> usize {
        self.buffers.borrow()
            .get(&path_id)
            .map(|b| b.staged_bytes())
            .unwrap_or(0)
    }
}

thread_local! {
    static SHADOW_ARENA: ShadowArena = ShadowArena::new();
}

/// Get or create thread-local shadow arena
pub fn get_shadow_arena() -> &'static ShadowArena {
    thread_local! {
        static SHADOW: ShadowArena = ShadowArena::new();
    }
    SHADOW_ARENA.with(|arena| unsafe { std::mem::transmute::<&ShadowArena, &'static ShadowArena>(arena) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shadow_buffer_writes() {
        let arena = ShadowArena::new();
        arena.create_path_buffer(0).unwrap();

        let data = b"test_data".to_vec();
        arena.shadow_write(0, 1, 0, data).unwrap();

        assert_eq!(arena.path_staged_bytes(0), 9);
    }

    #[test]
    fn test_multiple_path_buffers() {
        let arena = ShadowArena::new();
        arena.create_path_buffer(0).unwrap();
        arena.create_path_buffer(1).unwrap();

        arena.shadow_write(0, 1, 0, b"data0".to_vec()).unwrap();
        arena.shadow_write(1, 2, 0, b"data1".to_vec()).unwrap();

        assert_eq!(arena.path_staged_bytes(0), 5);
        assert_eq!(arena.path_staged_bytes(1), 5);
        assert_eq!(arena.total_staged(), 10);
    }

    #[test]
    fn test_shared_resource_conflict_detection() {
        let arena = ShadowArena::new();
        arena.create_path_buffer(0).unwrap();
        arena.create_path_buffer(1).unwrap();

        // Path 0: read from OS resource 10
        arena.record_shared_access(0, 10, SharedResourceAccess::Read).unwrap();

        // Path 1: write to OS resource 10
        arena.record_shared_access(1, 10, SharedResourceAccess::Write).unwrap();

        let conflicts = arena.detect_shared_conflicts();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0], (0, 1, 10));
    }

    #[test]
    fn test_abort_clear() {
        let arena = ShadowArena::new();
        arena.create_path_buffer(0).unwrap();

        arena.shadow_write(0, 1, 0, b"abort_test".to_vec()).unwrap();
        assert_eq!(arena.total_staged(), 10);

        arena.clear_path_buffer(0).unwrap();
        assert_eq!(arena.total_staged(), 0);
    }

    #[test]
    fn test_shadow_buffer_isolation() {
        let buf1 = ShadowBuffer::new();
        let buf2 = ShadowBuffer::new();

        let data = b"isolated".to_vec();
        let mut buf1_mut = buf1;
        buf1_mut.add_write(1, 0, data.clone());

        assert_eq!(buf1_mut.staged_bytes(), 8);
        assert_eq!(buf2.staged_bytes(), 0); // buf2 untouched
    }
}
