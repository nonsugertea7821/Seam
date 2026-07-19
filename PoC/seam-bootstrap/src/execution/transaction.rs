//! Two-Phase Static Transaction (2PST) Implementation
//!
//! Implements the 2PST protocol for fork paths:
//! - Phase 1: Speculative execution (writes to shadow buffer)
//! - Phase 2: Static commit (atomic flush to main memory)
//! - Phase 3: Abort cleanup (discard shadow buffer)

use crate::resource::{GlobalResource, AccessSet, ResourceAccess};
use crate::shadow_buffer::{ShadowBuffer, ShadowBufferPool};
use std::sync::Arc;
use std::collections::HashMap;

/// Transaction state machine
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionState {
    /// Idle, not in transaction
    Idle = 0,
    /// Phase 1: Speculative execution
    Speculative = 1,
    /// Phase 2: Acquiring locks and committing
    Committing = 2,
    /// Phase 3: Committed successfully
    Committed = 3,
    /// Aborted and rolled back
    Aborted = 4,
}

/// Transaction context for a fork path
pub struct Transaction {
    /// Unique transaction ID
    transaction_id: u32,
    /// Current state
    state: TransactionState,
    /// Shadow buffer for speculative writes
    shadow_buffer: ShadowBuffer,
    /// Static resource access set (from compiler analysis)
    access_set: AccessSet,
    /// Global resources being accessed
    resources: Vec<Arc<GlobalResource>>,
}

impl Transaction {
    /// Create new transaction
    pub fn new(transaction_id: u32) -> Self {
        Transaction {
            transaction_id,
            state: TransactionState::Idle,
            shadow_buffer: ShadowBuffer::new(),
            access_set: AccessSet::new(),
            resources: Vec::new(),
        }
    }

    /// Start Phase 1: Speculative execution
    pub fn begin_speculative(&mut self) {
        self.state = TransactionState::Speculative;
        self.shadow_buffer.clear();
    }

    /// Record write during speculative execution
    pub fn record_write(&mut self, resource_id: u32, offset: usize, size: usize, data: *mut u8) {
        if self.state != TransactionState::Speculative {
            panic!("Cannot record write outside speculative phase");
        }
        self.shadow_buffer.add_write(resource_id, offset, size, data);
    }

    /// Register a global resource
    pub fn register_resource(&mut self, resource: Arc<GlobalResource>) {
        self.resources.push(resource);
    }

    /// Add static access information
    pub fn add_access(&mut self, access: ResourceAccess) {
        self.access_set.add_access(access);
    }

    /// Phase 2: Commit all writes
    /// Returns Ok(()) on success, Err if any resource is poisoned
    pub fn commit(&mut self) -> Result<(), &'static str> {
        if self.state != TransactionState::Speculative {
            return Err("Transaction not in speculative state");
        }

        self.state = TransactionState::Committing;

        // Get write accesses sorted by resource ID (static ordering)
        let write_accesses = self.access_set.write_accesses();

        // Phase 2a: Acquire locks in static order
        let mut acquired_locks: Vec<Arc<crate::resource::GlobalResource>> = Vec::new();
        for access in &write_accesses {
            // Find the resource
            if let Some(resource) = self
                .resources
                .iter()
                .find(|r| r.resource_id() == access.resource_id)
            {
                if resource.is_poisoned() {
                    // Rollback acquired locks
                    for locked_resource in acquired_locks {
                        locked_resource.release_lock();
                    }
                    self.state = TransactionState::Aborted;
                    return Err("Resource poisoned during commit");
                }

                if !resource.acquire_lock() {
                    // Failed to acquire lock - should not happen with static ordering
                    for locked_resource in acquired_locks {
                        locked_resource.release_lock();
                    }
                    self.state = TransactionState::Aborted;
                    return Err("Failed to acquire resource lock");
                }

                acquired_locks.push(Arc::clone(resource));
            }
        }

        // Phase 2b: Atomic flush of all writes
        let write_resources = self.shadow_buffer.resource_ids();
        for resource_id in write_resources {
            if let Some(writes) = self.shadow_buffer.get_writes(resource_id) {
                if let Some(resource) = self
                    .resources
                    .iter()
                    .find(|r| r.resource_id() == resource_id)
                {
                    for write in writes {
                        unsafe {
                            resource.atomic_flush(write.data(), write.size());
                        }
                    }
                }
            }
        }

        // Phase 2c: Release locks
        for resource in acquired_locks {
            resource.release_lock();
        }

        self.state = TransactionState::Committed;
        Ok(())
    }

    /// Phase 3: Abort - discard all changes
    pub fn abort(&mut self) {
        if self.state == TransactionState::Committed {
            return; // Already committed
        }

        self.state = TransactionState::Aborted;
        self.shadow_buffer.clear();
    }

    /// Get transaction state
    #[inline]
    pub fn state(&self) -> TransactionState {
        self.state
    }

    /// Get transaction ID
    #[inline]
    pub fn transaction_id(&self) -> u32 {
        self.transaction_id
    }

    /// Get shadow buffer
    #[inline]
    pub fn shadow_buffer(&self) -> &ShadowBuffer {
        &self.shadow_buffer
    }
}

/// Transaction manager for fork context
pub struct TransactionManager {
    /// Map: thread_id -> transaction pool
    transactions: HashMap<u64, ShadowBufferPool>,
    /// Next transaction ID
    next_tx_id: u32,
}

impl TransactionManager {
    /// Create new manager
    pub fn new() -> Self {
        TransactionManager {
            transactions: HashMap::new(),
            next_tx_id: 1,
        }
    }

    /// Allocate new transaction ID
    pub fn allocate_tx_id(&mut self) -> u32 {
        let id = self.next_tx_id;
        self.next_tx_id = self.next_tx_id.wrapping_add(1);
        id
    }

    /// Get or create pool for thread
    pub fn get_pool_mut(&mut self, thread_id: u64) -> &mut ShadowBufferPool {
        self.transactions
            .entry(thread_id)
            .or_insert_with(ShadowBufferPool::new)
    }

    /// Get pool (read-only)
    pub fn get_pool(&self, thread_id: u64) -> Option<&ShadowBufferPool> {
        self.transactions.get(&thread_id)
    }

    /// Clean up thread pool
    pub fn cleanup_thread(&mut self, thread_id: u64) {
        self.transactions.remove(&thread_id);
    }

    /// Get total active transactions
    pub fn active_transactions(&self) -> usize {
        self.transactions.values().map(|p| p.active_paths()).sum()
    }
}

impl Default for TransactionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_creation() {
        let tx = Transaction::new(1);
        assert_eq!(tx.transaction_id(), 1);
        assert_eq!(tx.state(), TransactionState::Idle);
    }

    #[test]
    fn test_transaction_speculative_phase() {
        let mut tx = Transaction::new(1);
        tx.begin_speculative();
        assert_eq!(tx.state(), TransactionState::Speculative);

        let data = vec![0u8; 64];
        let ptr = data.as_ptr() as *mut u8;
        tx.record_write(1, 0, 64, ptr);

        assert_eq!(tx.shadow_buffer().write_count(), 1);
    }

    #[test]
    fn test_transaction_abort() {
        let mut tx = Transaction::new(1);
        tx.begin_speculative();

        let data = vec![0u8; 32];
        let ptr = data.as_ptr() as *mut u8;
        tx.record_write(1, 0, 32, ptr);

        assert_eq!(tx.shadow_buffer().write_count(), 1);

        tx.abort();
        assert_eq!(tx.state(), TransactionState::Aborted);
        assert_eq!(tx.shadow_buffer().write_count(), 0); // Shadow buffer cleared on abort
    }

    #[test]
    fn test_transaction_manager() {
        let mut manager = TransactionManager::new();
        let tx_id1 = manager.allocate_tx_id();
        let tx_id2 = manager.allocate_tx_id();

        assert_ne!(tx_id1, tx_id2);
        assert!(tx_id1 < tx_id2);
    }
}
