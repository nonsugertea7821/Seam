//! Automatic Synchronization
//!
//! Detects when synchronization is needed and manages automatic sync points.
//! Eliminates manual synchronization coding through compile-time analysis.
//! 
//! Memory Barriers:
//! - Acquire barriers: Prevent subsequent loads/stores from moving before
//! - Release barriers: Prevent prior loads/stores from moving after
//! - Full fence: Prevents all reordering (both directions)

use crate::effect::EffectAnalysis;
use std::collections::BTreeSet;
use std::sync::atomic::{Ordering};

/// Type of memory barrier required
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BarrierKind {
    /// No barrier needed
    None = 0,
    /// Acquire barrier: Prevent subsequent ops from reordering before
    Acquire = 1,
    /// Release barrier: Prevent prior ops from reordering after
    Release = 2,
    /// Full fence: Bidirectional barrier
    FullFence = 3,
}

impl BarrierKind {
    /// Get Rust atomic Ordering for this barrier kind
    pub fn to_atomic_ordering(&self) -> Ordering {
        match self {
            BarrierKind::None => Ordering::Relaxed,
            BarrierKind::Acquire => Ordering::Acquire,
            BarrierKind::Release => Ordering::Release,
            BarrierKind::FullFence => Ordering::SeqCst,
        }
    }

    /// Human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            BarrierKind::None => "None",
            BarrierKind::Acquire => "Acquire",
            BarrierKind::Release => "Release",
            BarrierKind::FullFence => "SeqCst",
        }
    }
}

/// Memory barrier implementation with actual sync semantics
#[repr(C)]
#[derive(Debug, Clone)]
pub struct MemoryBarrier {
    /// Kind of barrier
    kind: BarrierKind,
    /// Resource ID being synchronized
    resource_id: u32,
    /// Sync point this barrier implements
    sync_point: u32,
}

impl MemoryBarrier {
    /// Create new memory barrier
    pub fn new(kind: BarrierKind, resource_id: u32, sync_point: u32) -> Self {
        MemoryBarrier {
            kind,
            resource_id,
            sync_point,
        }
    }

    /// Execute the barrier (thread fence)
    pub fn execute(&self) {
        match self.kind {
            BarrierKind::None => {
                // No fence needed (relaxed)
                #[cfg(not(miri))]
                {
                    // Compiler barrier to prevent aggressive optimization
                    std::sync::atomic::compiler_fence(Ordering::Release);
                }
            }
            BarrierKind::Acquire => {
                // Acquire fence: block subsequent memory ops
                std::sync::atomic::fence(Ordering::Acquire);
            }
            BarrierKind::Release => {
                // Release fence: block prior memory ops
                std::sync::atomic::fence(Ordering::Release);
            }
            BarrierKind::FullFence => {
                // Full sequential consistency barrier
                std::sync::atomic::fence(Ordering::SeqCst);
            }
        }
    }

    /// Get barrier kind
    #[inline]
    pub fn kind(&self) -> BarrierKind {
        self.kind
    }

    /// Get resource ID
    #[inline]
    pub fn resource_id(&self) -> u32 {
        self.resource_id
    }

    /// Get sync point ID
    #[inline]
    pub fn sync_point(&self) -> u32 {
        self.sync_point
    }
}

/// Type of synchronization required
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SyncKind {
    /// No synchronization needed
    None = 0,
    /// Read-after-write dependency
    RAW = 1,
    /// Write-after-read dependency
    WAR = 2,
    /// Write-after-write conflict
    WAW = 3,
}

/// A required synchronization point
#[repr(C)]
#[derive(Debug, Clone)]
pub struct SyncPoint {
    /// Resource ID requiring sync
    resource_id: u32,
    /// Type of synchronization
    sync_kind: SyncKind,
    /// Path IDs involved
    paths: Vec<u32>,
}

impl SyncPoint {
    /// Create new sync point
    pub fn new(resource_id: u32, sync_kind: SyncKind) -> Self {
        SyncPoint {
            resource_id,
            sync_kind,
            paths: Vec::new(),
        }
    }

    /// Add path involved in sync
    pub fn add_path(&mut self, path_id: u32) {
        if !self.paths.contains(&path_id) {
            self.paths.push(path_id);
        }
    }

    /// Get resource ID
    #[inline]
    pub fn resource_id(&self) -> u32 {
        self.resource_id
    }

    /// Get sync kind
    #[inline]
    pub fn sync_kind(&self) -> SyncKind {
        self.sync_kind
    }

    /// Get involved paths
    pub fn paths(&self) -> &[u32] {
        &self.paths
    }
}

impl PartialEq for SyncPoint {
    fn eq(&self, other: &Self) -> bool {
        self.resource_id == other.resource_id && self.sync_kind == other.sync_kind
    }
}

impl Eq for SyncPoint {}

impl PartialOrd for SyncPoint {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.resource_id.partial_cmp(&other.resource_id)
    }
}

impl Ord for SyncPoint {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.resource_id.cmp(&other.resource_id)
    }
}

/// Automatic synchronization analyzer and manager
#[repr(C)]
#[derive(Clone)]
pub struct AutoSync {
    /// Effect analysis from fork
    analysis: Option<EffectAnalysis>,
    /// Required sync points
    sync_points: BTreeSet<SyncPoint>,
    /// Memory barriers for sync points
    barriers: Vec<MemoryBarrier>,
    /// Whether sync is automatic (true) or manual (false)
    auto_sync_enabled: bool,
}

impl AutoSync {
    /// Create new auto sync manager
    pub fn new(auto_enabled: bool) -> Self {
        AutoSync {
            analysis: None,
            sync_points: BTreeSet::new(),
            barriers: Vec::new(),
            auto_sync_enabled: auto_enabled,
        }
    }

    /// Map sync kind to appropriate memory barrier kind
    fn sync_kind_to_barrier(&self, sync_kind: SyncKind) -> BarrierKind {
        match sync_kind {
            SyncKind::None => BarrierKind::None,
            SyncKind::RAW => {
                // Read-after-write: Acquire barrier prevents next load from seeing stale data
                BarrierKind::Acquire
            }
            SyncKind::WAR => {
                // Write-after-read: Release barrier prevents prior load from moving after write
                BarrierKind::Release
            }
            SyncKind::WAW => {
                // Write-after-write: Full fence ensures ordering between writers
                BarrierKind::FullFence
            }
        }
    }

    /// Set effect analysis from fork
    pub fn set_analysis(&mut self, analysis: EffectAnalysis) {
        self.analysis = Some(analysis);
        if self.auto_sync_enabled {
            self.detect_sync_points();
            self.generate_memory_barriers();
        }
    }

    /// Generate actual memory barriers for detected sync points
    fn generate_memory_barriers(&mut self) {
        self.barriers.clear();

        for (sync_id, sync_point) in self.sync_points.iter().enumerate() {
            let barrier_kind = self.sync_kind_to_barrier(sync_point.sync_kind);
            let barrier = MemoryBarrier::new(barrier_kind, sync_point.resource_id, sync_id as u32);
            self.barriers.push(barrier);
        }
    }

    /// Detect required synchronization points
    pub fn detect_sync_points(&mut self) {
        self.sync_points.clear();

        if let Some(analysis) = &self.analysis {
            // Detect write-write conflicts (WAW)
            let write_resources: Vec<_> = analysis
                .union_effects()
                .write_effects()
                .iter()
                .map(|e| e.resource_id())
                .collect();

            for write_resource in write_resources {
                let mut writers = Vec::new();
                for path_id in 0..analysis.num_paths() {
                    if let Some(effects) = analysis.path_effects(path_id) {
                        if effects
                            .effects()
                            .iter()
                            .any(|e| e.resource_id() == write_resource && e.is_write())
                        {
                            writers.push(path_id);
                        }
                    }
                }

                if writers.len() > 1 {
                    let mut sync = SyncPoint::new(write_resource, SyncKind::WAW);
                    for writer in writers {
                        sync.add_path(writer);
                    }
                    self.sync_points.insert(sync);
                }
            }

            // Detect read-write conflicts (RAW and WAR)
            for write_resource in analysis.union_effects().write_effects() {
                let resource_id = write_resource.resource_id();

                for path_id in 0..analysis.num_paths() {
                    if let Some(effects) = analysis.path_effects(path_id) {
                        // Check reads in other paths after writes
                        for other_path_id in (path_id + 1)..analysis.num_paths() {
                            if let Some(other_effects) = analysis.path_effects(other_path_id) {
                                // RAW: this path writes, other path reads
                                if other_effects
                                    .effects()
                                    .iter()
                                    .any(|e| e.resource_id() == resource_id && e.is_read())
                                {
                                    let mut sync = SyncPoint::new(resource_id, SyncKind::RAW);
                                    sync.add_path(path_id);
                                    sync.add_path(other_path_id);
                                    self.sync_points.insert(sync);
                                }

                                // WAR: this path reads, other path writes
                                if effects
                                    .effects()
                                    .iter()
                                    .any(|e| e.resource_id() == resource_id && e.is_read())
                                    && other_effects
                                        .effects()
                                        .iter()
                                        .any(|e| e.resource_id() == resource_id && e.is_write())
                                {
                                    let mut sync = SyncPoint::new(resource_id, SyncKind::WAR);
                                    sync.add_path(path_id);
                                    sync.add_path(other_path_id);
                                    self.sync_points.insert(sync);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Get all required sync points
    pub fn sync_points(&self) -> Vec<SyncPoint> {
        self.sync_points.iter().cloned().collect()
    }

    /// Get WAW sync points (write-write conflicts)
    pub fn waw_sync_points(&self) -> Vec<SyncPoint> {
        self.sync_points
            .iter()
            .filter(|s| s.sync_kind == SyncKind::WAW)
            .cloned()
            .collect()
    }

    /// Get RAW sync points (read-after-write dependencies)
    pub fn raw_sync_points(&self) -> Vec<SyncPoint> {
        self.sync_points
            .iter()
            .filter(|s| s.sync_kind == SyncKind::RAW)
            .cloned()
            .collect()
    }

    /// Get WAR sync points (write-after-read dependencies)
    pub fn war_sync_points(&self) -> Vec<SyncPoint> {
        self.sync_points
            .iter()
            .filter(|s| s.sync_kind == SyncKind::WAR)
            .cloned()
            .collect()
    }

    /// Get sync points for resource
    pub fn sync_points_for_resource(&self, resource_id: u32) -> Vec<SyncPoint> {
        self.sync_points
            .iter()
            .filter(|s| s.resource_id == resource_id)
            .cloned()
            .collect()
    }

    /// Check if sync is needed for resource
    pub fn needs_sync(&self, resource_id: u32) -> bool {
        self.sync_points.iter().any(|s| s.resource_id == resource_id)
    }

    /// Get number of sync points
    pub fn sync_count(&self) -> usize {
        self.sync_points.len()
    }

    /// Get all memory barriers
    pub fn barriers(&self) -> &[MemoryBarrier] {
        &self.barriers
    }

    /// Get memory barriers for resource
    pub fn barriers_for_resource(&self, resource_id: u32) -> Vec<&MemoryBarrier> {
        self.barriers
            .iter()
            .filter(|b| b.resource_id == resource_id)
            .collect()
    }

    /// Execute all memory barriers (thread fences)
    pub fn execute_barriers(&self) {
        for barrier in &self.barriers {
            barrier.execute();
        }
    }

    /// Execute barriers for specific resource
    pub fn execute_barriers_for_resource(&self, resource_id: u32) {
        for barrier in self.barriers_for_resource(resource_id) {
            barrier.execute();
        }
    }

    /// Check if auto sync is enabled
    pub fn is_auto_sync_enabled(&self) -> bool {
        self.auto_sync_enabled
    }

    /// Generate dependency barrier code (pseudo-code and actual barriers)
    pub fn generate_barriers(&self) -> Vec<String> {
        let mut barriers = Vec::new();

        for (idx, sync) in self.sync_points.iter().enumerate() {
            let kind_str = match sync.sync_kind {
                SyncKind::None => "NONE",
                SyncKind::RAW => "RAW",
                SyncKind::WAR => "WAR",
                SyncKind::WAW => "WAW",
            };

            let paths_str = sync
                .paths
                .iter()
                .map(|p| format!("P{}", p))
                .collect::<Vec<_>>()
                .join(",");

            // Include actual barrier type
            let barrier_kind = if idx < self.barriers.len() {
                self.barriers[idx].kind().name()
            } else {
                "Unknown"
            };

            barriers.push(format!(
                "BARRIER(resource={}, kind={}, barrier_type={}, paths=[{}])",
                sync.resource_id, kind_str, barrier_kind, paths_str
            ));
        }

        barriers
    }
}

impl Default for AutoSync {
    fn default() -> Self {
        Self::new(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::{Effect, EffectAnalysis, EffectType};

    fn create_test_analysis() -> EffectAnalysis {
        let mut analysis = EffectAnalysis::new(2);
        analysis
            .add_effect_to_path(0, Effect::new(1, EffectType::Write))
            .unwrap();
        analysis
            .add_effect_to_path(1, Effect::new(1, EffectType::Read))
            .unwrap();
        analysis
    }

    #[test]
    fn test_sync_point_creation() {
        let sync = SyncPoint::new(1, SyncKind::WAW);
        assert_eq!(sync.resource_id(), 1);
        assert_eq!(sync.sync_kind(), SyncKind::WAW);
    }

    #[test]
    fn test_auto_sync_creation() {
        let auto_sync = AutoSync::new(true);
        assert!(auto_sync.is_auto_sync_enabled());
        assert_eq!(auto_sync.sync_count(), 0);
    }

    #[test]
    fn test_auto_sync_detection() {
        let mut auto_sync = AutoSync::new(true);
        let analysis = create_test_analysis();
        auto_sync.set_analysis(analysis);

        assert!(auto_sync.sync_count() > 0);
        assert!(!auto_sync.raw_sync_points().is_empty());
    }

    #[test]
    fn test_sync_barrier_generation() {
        let mut auto_sync = AutoSync::new(true);
        let analysis = create_test_analysis();
        auto_sync.set_analysis(analysis);

        let barriers = auto_sync.generate_barriers();
        assert!(!barriers.is_empty());

        let barrier_str = barriers[0].clone();
        assert!(barrier_str.contains("resource="));
        assert!(barrier_str.contains("kind="));
        assert!(barrier_str.contains("paths="));
    }

    #[test]
    fn test_no_sync_needed_disjoint() {
        let mut auto_sync = AutoSync::new(true);
        let mut analysis = EffectAnalysis::new(2);

        // Path 0 reads resource 1
        analysis
            .add_effect_to_path(0, Effect::new(1, EffectType::Read))
            .unwrap();

        // Path 1 reads resource 2 (different resource, no conflict)
        analysis
            .add_effect_to_path(1, Effect::new(2, EffectType::Read))
            .unwrap();

        auto_sync.set_analysis(analysis);

        assert_eq!(auto_sync.sync_count(), 0);
    }

    #[test]
    fn test_manual_sync_disabled() {
        let mut auto_sync = AutoSync::new(false);
        let analysis = create_test_analysis();
        auto_sync.set_analysis(analysis);

        // With auto sync disabled, detect_sync_points not called during set_analysis
        // Must be called manually
        assert_eq!(auto_sync.sync_count(), 0);
    }

    #[test]
    fn test_barrier_kind_atomic_ordering() {
        // Verify barrier kinds map to correct atomic orderings
        assert_eq!(BarrierKind::None.to_atomic_ordering(), Ordering::Relaxed);
        assert_eq!(BarrierKind::Acquire.to_atomic_ordering(), Ordering::Acquire);
        assert_eq!(BarrierKind::Release.to_atomic_ordering(), Ordering::Release);
        assert_eq!(
            BarrierKind::FullFence.to_atomic_ordering(),
            Ordering::SeqCst
        );
    }

    #[test]
    fn test_barrier_kind_names() {
        assert_eq!(BarrierKind::None.name(), "None");
        assert_eq!(BarrierKind::Acquire.name(), "Acquire");
        assert_eq!(BarrierKind::Release.name(), "Release");
        assert_eq!(BarrierKind::FullFence.name(), "SeqCst");
    }

    #[test]
    fn test_memory_barrier_creation() {
        let barrier = MemoryBarrier::new(BarrierKind::Acquire, 42, 0);
        assert_eq!(barrier.kind(), BarrierKind::Acquire);
        assert_eq!(barrier.resource_id(), 42);
        assert_eq!(barrier.sync_point(), 0);
    }

    #[test]
    fn test_memory_barrier_execution() {
        // Test that barriers execute without panicking
        let barrier_none = MemoryBarrier::new(BarrierKind::None, 1, 0);
        barrier_none.execute();

        let barrier_acquire = MemoryBarrier::new(BarrierKind::Acquire, 1, 0);
        barrier_acquire.execute();

        let barrier_release = MemoryBarrier::new(BarrierKind::Release, 1, 0);
        barrier_release.execute();

        let barrier_fence = MemoryBarrier::new(BarrierKind::FullFence, 1, 0);
        barrier_fence.execute();
    }

    #[test]
    fn test_sync_kind_to_barrier_mapping() {
        let mut auto_sync = AutoSync::new(true);
        let analysis = create_test_analysis();
        auto_sync.set_analysis(analysis);

        // Should have generated barriers
        assert!(!auto_sync.barriers().is_empty());

        // RAW should map to Acquire barrier
        let raw_barriers: Vec<_> = auto_sync
            .barriers()
            .iter()
            .filter(|b| {
                auto_sync
                    .sync_points()
                    .iter()
                    .find(|s| {
                        s.resource_id == b.resource_id && s.sync_kind == SyncKind::RAW
                    })
                    .is_some()
            })
            .collect();

        for barrier in raw_barriers {
            assert_eq!(barrier.kind(), BarrierKind::Acquire);
        }
    }

    #[test]
    fn test_barriers_for_resource() {
        let mut auto_sync = AutoSync::new(true);
        let analysis = create_test_analysis();
        auto_sync.set_analysis(analysis);

        // Get barriers for resource 1
        let resource_1_barriers = auto_sync.barriers_for_resource(1);
        assert!(!resource_1_barriers.is_empty());

        // Get barriers for non-existent resource
        let resource_99_barriers = auto_sync.barriers_for_resource(99);
        assert!(resource_99_barriers.is_empty());
    }

    #[test]
    fn test_barrier_generation_includes_barrier_type() {
        let mut auto_sync = AutoSync::new(true);
        let analysis = create_test_analysis();
        auto_sync.set_analysis(analysis);

        let barriers = auto_sync.generate_barriers();
        assert!(!barriers.is_empty());

        let barrier_str = barriers[0].clone();
        assert!(barrier_str.contains("barrier_type="));
        // Should contain one of the barrier types
        assert!(
            barrier_str.contains("Acquire")
                || barrier_str.contains("Release")
                || barrier_str.contains("SeqCst")
                || barrier_str.contains("None")
        );
    }

    #[test]
    fn test_execute_barriers_all() {
        let mut auto_sync = AutoSync::new(true);
        let analysis = create_test_analysis();
        auto_sync.set_analysis(analysis);

        // Execute all barriers - should not panic
        auto_sync.execute_barriers();
    }

    #[test]
    fn test_execute_barriers_for_resource() {
        let mut auto_sync = AutoSync::new(true);
        let analysis = create_test_analysis();
        auto_sync.set_analysis(analysis);

        // Execute barriers for resource 1 - should not panic
        auto_sync.execute_barriers_for_resource(1);

        // Execute barriers for non-existent resource - should not panic
        auto_sync.execute_barriers_for_resource(99);
    }
}

