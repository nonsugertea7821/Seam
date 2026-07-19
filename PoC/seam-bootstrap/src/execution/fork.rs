//! Fork and Parallel Path Management
//!
//! Implements the `fork` construct for spawning parallel execution paths
//! with 2PST (Two-Phase Static Transaction) semantics

use crate::transaction::Transaction;
use std::sync::Arc;
use std::sync::Mutex;

/// Fork path result
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathResult {
    /// Path returned normally
    Returned = 0,
    /// Path aborted
    Aborted = 1,
    /// Poisoned by resource corruption
    Poisoned = 2,
}

/// Join point metadata
#[repr(C)]
pub struct JoinPoint {
    /// Number of paths in this fork
    num_paths: u32,
    /// Results for each path
    results: Vec<PathResult>,
}

impl JoinPoint {
    /// Create new join point
    pub fn new(num_paths: u32) -> Self {
        JoinPoint {
            num_paths,
            results: vec![PathResult::Returned; num_paths as usize],
        }
    }

    /// Register result for a path
    pub fn set_result(&mut self, path_id: u32, result: PathResult) {
        if (path_id as usize) < self.results.len() {
            self.results[path_id as usize] = result;
        }
    }

    /// Get result for path
    pub fn get_result(&self, path_id: u32) -> Option<PathResult> {
        if (path_id as usize) < self.results.len() {
            Some(self.results[path_id as usize])
        } else {
            None
        }
    }

    /// Check if all paths returned successfully
    pub fn all_succeeded(&self) -> bool {
        self.results
            .iter()
            .all(|&r| r == PathResult::Returned)
    }

    /// Check if any path aborted
    pub fn any_aborted(&self) -> bool {
        self.results.iter().any(|&r| r == PathResult::Aborted)
    }

    /// Check if any path poisoned
    pub fn any_poisoned(&self) -> bool {
        self.results.iter().any(|&r| r == PathResult::Poisoned)
    }

    /// Get number of paths
    pub fn num_paths(&self) -> u32 {
        self.num_paths
    }
}

/// Fork path descriptor
pub struct ForkPath {
    /// Path ID (0..num_paths-1)
    path_id: u32,
    /// Associated channel(s) to invoke
    channels: Vec<u32>,
    /// Transaction for 2PST
    transaction: Arc<Mutex<Transaction>>,
}

impl ForkPath {
    /// Create new fork path
    pub fn new(path_id: u32, transaction_id: u32) -> Self {
        ForkPath {
            path_id,
            channels: Vec::new(),
            transaction: Arc::new(Mutex::new(Transaction::new(transaction_id))),
        }
    }

    /// Add channel to this path
    pub fn add_channel(&mut self, channel_id: u32) {
        self.channels.push(channel_id);
    }

    /// Get path ID
    #[inline]
    pub fn path_id(&self) -> u32 {
        self.path_id
    }

    /// Get channels in this path
    #[inline]
    pub fn channels(&self) -> &[u32] {
        &self.channels
    }

    /// Get transaction
    pub fn transaction(&self) -> Arc<Mutex<Transaction>> {
        Arc::clone(&self.transaction)
    }

    /// Start speculative execution
    pub fn begin_speculative(&self) {
        if let Ok(mut tx) = self.transaction.lock() {
            tx.begin_speculative();
        }
    }

    /// End speculative execution - commit or abort
    pub fn end_speculative(&self, should_abort: bool) -> Result<(), &'static str> {
        if let Ok(mut tx) = self.transaction.lock() {
            if should_abort {
                tx.abort();
                Ok(())
            } else {
                tx.commit()
            }
        } else {
            Err("Failed to lock transaction")
        }
    }
}

/// Fork context managing multiple parallel paths
pub struct ForkContext {
    /// Fork ID
    fork_id: u32,
    /// All paths in this fork
    paths: Vec<Arc<Mutex<ForkPath>>>,
    /// Join point for synchronization
    join_point: Arc<Mutex<JoinPoint>>,
}

impl ForkContext {
    /// Create new fork context
    pub fn new(fork_id: u32, num_paths: u32, base_tx_id: u32) -> Self {
        let mut paths = Vec::new();
        for i in 0..num_paths {
            let path = ForkPath::new(i, base_tx_id + i);
            paths.push(Arc::new(Mutex::new(path)));
        }

        ForkContext {
            fork_id,
            paths,
            join_point: Arc::new(Mutex::new(JoinPoint::new(num_paths))),
        }
    }

    /// Get fork ID
    #[inline]
    pub fn fork_id(&self) -> u32 {
        self.fork_id
    }

    /// Get path
    pub fn get_path(&self, path_id: u32) -> Option<Arc<Mutex<ForkPath>>> {
        if (path_id as usize) < self.paths.len() {
            Some(Arc::clone(&self.paths[path_id as usize]))
        } else {
            None
        }
    }

    /// Get number of paths
    pub fn num_paths(&self) -> u32 {
        self.paths.len() as u32
    }

    /// Record path result
    pub fn record_result(&self, path_id: u32, result: PathResult) {
        if let Ok(mut jp) = self.join_point.lock() {
            jp.set_result(path_id, result);
        }
    }

    /// Wait for all paths to complete and check results
    pub fn join(&self) -> Result<(), &'static str> {
        if let Ok(jp) = self.join_point.lock() {
            if jp.any_poisoned() {
                return Err("One or more paths corrupted resource state");
            }
            if jp.any_aborted() {
                return Err("One or more paths aborted");
            }
            Ok(())
        } else {
            Err("Failed to lock join point")
        }
    }

    /// Check join point status
    pub fn join_point(&self) -> Arc<Mutex<JoinPoint>> {
        Arc::clone(&self.join_point)
    }
}

/// Fork graph builder for compile-time fork specifications
pub struct ForkGraph {
    /// Paths in this fork
    paths: Vec<Vec<u32>>,
}

impl ForkGraph {
    /// Create new fork graph
    pub fn new() -> Self {
        ForkGraph {
            paths: Vec::new(),
        }
    }

    /// Add a path (sequence of channel IDs)
    pub fn add_path(&mut self, channels: Vec<u32>) {
        self.paths.push(channels);
    }

    /// Get number of paths
    pub fn num_paths(&self) -> u32 {
        self.paths.len() as u32
    }

    /// Get path
    pub fn get_path(&self, path_id: u32) -> Option<&Vec<u32>> {
        if (path_id as usize) < self.paths.len() {
            Some(&self.paths[path_id as usize])
        } else {
            None
        }
    }
}

impl Default for ForkGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_point_creation() {
        let jp = JoinPoint::new(2);
        assert_eq!(jp.num_paths(), 2);
        assert!(jp.all_succeeded());
    }

    #[test]
    fn test_join_point_results() {
        let mut jp = JoinPoint::new(2);
        jp.set_result(0, PathResult::Returned);
        jp.set_result(1, PathResult::Returned);
        assert!(jp.all_succeeded());

        jp.set_result(1, PathResult::Aborted);
        assert!(jp.any_aborted());
    }

    #[test]
    fn test_fork_path_creation() {
        let path = ForkPath::new(0, 1);
        assert_eq!(path.path_id(), 0);
    }

    #[test]
    fn test_fork_context_creation() {
        let ctx = ForkContext::new(1, 2, 10);
        assert_eq!(ctx.fork_id(), 1);
        assert_eq!(ctx.num_paths(), 2);
    }

    #[test]
    fn test_fork_graph() {
        let mut graph = ForkGraph::new();
        graph.add_path(vec![1, 2]);
        graph.add_path(vec![3, 4]);

        assert_eq!(graph.num_paths(), 2);
        assert_eq!(graph.get_path(0), Some(&vec![1, 2]));
    }
}
