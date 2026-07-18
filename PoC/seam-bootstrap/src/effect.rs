//! Static Effect Analysis
//!
//! Compile-time analysis of which resources are accessed by each path.
//! Effects are determined statically without runtime overhead.

use std::collections::BTreeSet;

/// Effect type descriptor
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EffectType {
    /// Read-only access
    Read = 1,
    /// Write access
    Write = 2,
    /// Read-modify-write
    ReadWrite = 3,
}

/// Single resource effect
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Effect {
    /// Resource ID
    resource_id: u32,
    /// Type of access
    effect_type: EffectType,
}

impl Effect {
    /// Create new effect
    pub fn new(resource_id: u32, effect_type: EffectType) -> Self {
        Effect {
            resource_id,
            effect_type,
        }
    }

    /// Get resource ID
    #[inline]
    pub fn resource_id(&self) -> u32 {
        self.resource_id
    }

    /// Get effect type
    #[inline]
    pub fn effect_type(&self) -> EffectType {
        self.effect_type
    }

    /// Is this a read?
    #[inline]
    pub fn is_read(&self) -> bool {
        matches!(self.effect_type, EffectType::Read | EffectType::ReadWrite)
    }

    /// Is this a write?
    #[inline]
    pub fn is_write(&self) -> bool {
        matches!(self.effect_type, EffectType::Write | EffectType::ReadWrite)
    }
}

impl PartialOrd for Effect {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Effect {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.resource_id.cmp(&other.resource_id) {
            std::cmp::Ordering::Equal => self.effect_type.cmp(&other.effect_type),
            other_ord => other_ord,
        }
    }
}

/// Set of effects for a single path
#[repr(C)]
pub struct EffectSet {
    /// Effects sorted by resource ID
    effects: BTreeSet<Effect>,
}

impl EffectSet {
    /// Create empty effect set
    pub fn new() -> Self {
        EffectSet {
            effects: BTreeSet::new(),
        }
    }

    /// Add effect
    pub fn add_effect(&mut self, effect: Effect) -> bool {
        self.effects.insert(effect)
    }

    /// Get all effects
    pub fn effects(&self) -> Vec<Effect> {
        self.effects.iter().copied().collect()
    }

    /// Get read effects
    pub fn read_effects(&self) -> Vec<Effect> {
        self.effects
            .iter()
            .copied()
            .filter(|e| e.is_read())
            .collect()
    }

    /// Get write effects
    pub fn write_effects(&self) -> Vec<Effect> {
        self.effects
            .iter()
            .copied()
            .filter(|e| e.is_write())
            .collect()
    }

    /// Check if resource is accessed
    pub fn accesses_resource(&self, resource_id: u32) -> bool {
        self.effects.iter().any(|e| e.resource_id == resource_id)
    }

    /// Get number of effects
    pub fn effect_count(&self) -> usize {
        self.effects.len()
    }

    /// Check if read-only (no writes)
    pub fn is_read_only(&self) -> bool {
        !self.effects.iter().any(|e| e.is_write())
    }

    /// Check if write-only (no reads)
    pub fn is_write_only(&self) -> bool {
        !self.effects.iter().any(|e| e.is_read())
    }

    /// Get union of two effect sets
    pub fn union(&self, other: &EffectSet) -> EffectSet {
        let mut result = EffectSet::new();
        for effect in &self.effects {
            result.add_effect(*effect);
        }
        for effect in &other.effects {
            result.add_effect(*effect);
        }
        result
    }

    /// Get intersection of two effect sets
    pub fn intersection(&self, other: &EffectSet) -> EffectSet {
        let mut result = EffectSet::new();
        for effect in &self.effects {
            if other.effects.contains(effect) {
                result.add_effect(*effect);
            }
        }
        result
    }

    /// Check if disjoint (no overlapping effects)
    pub fn is_disjoint(&self, other: &EffectSet) -> bool {
        self.intersection(other).effect_count() == 0
    }
}

impl Default for EffectSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Analysis of effects for a fork with multiple paths
#[repr(C)]
pub struct EffectAnalysis {
    /// Path ID to effect set mapping
    path_effects: Vec<EffectSet>,
    /// All resources accessed by any path
    union_effects: EffectSet,
}

impl EffectAnalysis {
    /// Create analysis for fork with N paths
    pub fn new(num_paths: u32) -> Self {
        let mut path_effects = Vec::new();
        for _ in 0..num_paths {
            path_effects.push(EffectSet::new());
        }

        EffectAnalysis {
            path_effects,
            union_effects: EffectSet::new(),
        }
    }

    /// Add effect to path
    pub fn add_effect_to_path(&mut self, path_id: u32, effect: Effect) -> Result<(), &'static str> {
        if (path_id as usize) >= self.path_effects.len() {
            return Err("Path ID out of range");
        }

        self.path_effects[path_id as usize].add_effect(effect);
        self.union_effects.add_effect(effect);
        Ok(())
    }

    /// Get effects for path
    pub fn path_effects(&self, path_id: u32) -> Option<&EffectSet> {
        if (path_id as usize) < self.path_effects.len() {
            Some(&self.path_effects[path_id as usize])
        } else {
            None
        }
    }

    /// Get union of all path effects
    pub fn union_effects(&self) -> &EffectSet {
        &self.union_effects
    }

    /// Check for write-write conflicts between paths
    pub fn has_write_conflicts(&self) -> bool {
        let writes: Vec<_> = self
            .union_effects
            .write_effects()
            .iter()
            .map(|e| e.resource_id)
            .collect();

        // Check if multiple paths write to same resource
        for write_resource in writes {
            let mut writer_count = 0;
            for path_effect in &self.path_effects {
                if path_effect
                    .effects()
                    .iter()
                    .any(|e| e.resource_id == write_resource && e.is_write())
                {
                    writer_count += 1;
                    if writer_count > 1 {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check for read-write conflicts (potential data races)
    pub fn has_read_write_conflicts(&self) -> bool {
        // For each resource, check if one path reads and another writes
        for write_effect in self.union_effects.write_effects() {
            let resource_id = write_effect.resource_id;

            // Find all paths that write this resource
            let mut writers = Vec::new();
            for (idx, path_effect) in self.path_effects.iter().enumerate() {
                if path_effect
                    .effects()
                    .iter()
                    .any(|e| e.resource_id == resource_id && e.is_write())
                {
                    writers.push(idx);
                }
            }

            // Find all paths that read this resource
            let mut readers = Vec::new();
            for (idx, path_effect) in self.path_effects.iter().enumerate() {
                if path_effect
                    .effects()
                    .iter()
                    .any(|e| e.resource_id == resource_id && e.is_read())
                {
                    readers.push(idx);
                }
            }

            // If any writer and any reader exist, there's a conflict
            if !writers.is_empty() && !readers.is_empty() {
                return true;
            }
        }
        false
    }

    /// Get synchronization points needed (resources with conflicts)
    pub fn required_sync_points(&self) -> Vec<u32> {
        let mut sync_resources = BTreeSet::new();

        // Add resources with write-write conflicts
        let writes: Vec<_> = self
            .union_effects
            .write_effects()
            .iter()
            .map(|e| e.resource_id)
            .collect();

        for write_resource in writes {
            let mut writer_count = 0;
            for path_effect in &self.path_effects {
                if path_effect
                    .effects()
                    .iter()
                    .any(|e| e.resource_id == write_resource && e.is_write())
                {
                    writer_count += 1;
                }
            }
            if writer_count > 1 {
                sync_resources.insert(write_resource);
            }
        }

        // Add resources with read-write conflicts
        for effect in self.union_effects.write_effects() {
            let resource_id = effect.resource_id;
            for path_effect in &self.path_effects {
                if path_effect.accesses_resource(resource_id)
                    && path_effect
                        .effects()
                        .iter()
                        .any(|e| e.resource_id == resource_id && e.is_read())
                {
                    sync_resources.insert(resource_id);
                }
            }
        }

        sync_resources.iter().copied().collect()
    }

    /// Number of paths in analysis
    pub fn num_paths(&self) -> u32 {
        self.path_effects.len() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effect_creation() {
        let effect = Effect::new(1, EffectType::Read);
        assert_eq!(effect.resource_id(), 1);
        assert_eq!(effect.effect_type(), EffectType::Read);
        assert!(effect.is_read());
        assert!(!effect.is_write());
    }

    #[test]
    fn test_effect_set_operations() {
        let mut es = EffectSet::new();
        es.add_effect(Effect::new(1, EffectType::Read));
        es.add_effect(Effect::new(2, EffectType::Write));

        assert_eq!(es.effect_count(), 2);
        assert_eq!(es.read_effects().len(), 1);
        assert_eq!(es.write_effects().len(), 1);
    }

    #[test]
    fn test_effect_set_disjoint() {
        let mut es1 = EffectSet::new();
        es1.add_effect(Effect::new(1, EffectType::Read));

        let mut es2 = EffectSet::new();
        es2.add_effect(Effect::new(2, EffectType::Write));

        assert!(es1.is_disjoint(&es2));
    }

    #[test]
    fn test_effect_analysis_conflicts() {
        let mut analysis = EffectAnalysis::new(2);

        // Path 0 reads resource 1
        analysis
            .add_effect_to_path(0, Effect::new(1, EffectType::Read))
            .unwrap();

        // Path 1 writes resource 1 (conflict!)
        analysis
            .add_effect_to_path(1, Effect::new(1, EffectType::Write))
            .unwrap();

        assert!(analysis.has_read_write_conflicts());
        assert!(analysis.required_sync_points().contains(&1));
    }

    #[test]
    fn test_effect_analysis_write_write() {
        let mut analysis = EffectAnalysis::new(2);

        // Both paths write to same resource
        analysis
            .add_effect_to_path(0, Effect::new(1, EffectType::Write))
            .unwrap();
        analysis
            .add_effect_to_path(1, Effect::new(1, EffectType::Write))
            .unwrap();

        assert!(analysis.has_write_conflicts());
    }

    #[test]
    fn test_effect_analysis_safe() {
        let mut analysis = EffectAnalysis::new(2);

        // Path 0 reads resource 1
        analysis
            .add_effect_to_path(0, Effect::new(1, EffectType::Read))
            .unwrap();

        // Path 1 reads resource 2 (no conflict)
        analysis
            .add_effect_to_path(1, Effect::new(2, EffectType::Read))
            .unwrap();

        assert!(!analysis.has_write_conflicts());
        assert!(!analysis.has_read_write_conflicts());
    }
}
