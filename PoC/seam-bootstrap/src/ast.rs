//! Phase 4: Abstract Syntax Tree (AST) for Seam fork expressions
//!
//! This module defines the Abstract Syntax Tree representation for Seam fork
//! expressions. The AST enables compiler-based effect extraction and code generation.

use std::collections::BTreeMap;

/// Resource identifier in the AST
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceId(pub u32);

impl ResourceId {
    pub fn new(id: u32) -> Self {
        ResourceId(id)
    }
}

/// Access type specification in a requires clause
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccessType {
    Read,
    Write,
    ReadWrite,
}

impl AccessType {
    pub fn is_read(self) -> bool {
        matches!(self, AccessType::Read | AccessType::ReadWrite)
    }

    pub fn is_write(self) -> bool {
        matches!(self, AccessType::Write | AccessType::ReadWrite)
    }
}

/// Resource access specification in a requires contract
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AccessSpec {
    pub resource_id: ResourceId,
    pub access_type: AccessType,
}

impl AccessSpec {
    pub fn new(resource_id: ResourceId, access_type: AccessType) -> Self {
        AccessSpec {
            resource_id,
            access_type,
        }
    }

    pub fn read(resource_id: ResourceId) -> Self {
        AccessSpec::new(resource_id, AccessType::Read)
    }

    pub fn write(resource_id: ResourceId) -> Self {
        AccessSpec::new(resource_id, AccessType::Write)
    }
}

/// Requires contract specification for a fork path
#[derive(Debug, Clone)]
pub struct RequiresClause {
    pub accesses: Vec<AccessSpec>,
}

impl RequiresClause {
    pub fn new() -> Self {
        RequiresClause {
            accesses: Vec::new(),
        }
    }

    pub fn add_access(&mut self, access: AccessSpec) {
        self.accesses.push(access);
        self.accesses.sort();
        self.accesses.dedup();
    }
}

impl Default for RequiresClause {
    fn default() -> Self {
        Self::new()
    }
}

/// A single fork path expression
#[derive(Debug, Clone)]
pub struct ForkPath {
    pub path_id: u32,
    pub requires: RequiresClause,
    pub code: String,
}

impl ForkPath {
    pub fn new(path_id: u32, code: String) -> Self {
        ForkPath {
            path_id,
            requires: RequiresClause::new(),
            code,
        }
    }

    pub fn with_requires(mut self, requires: RequiresClause) -> Self {
        self.requires = requires;
        self
    }

    pub fn add_access(&mut self, access: AccessSpec) {
        self.requires.add_access(access);
    }
}

/// Top-level fork expression in Seam
#[derive(Debug, Clone)]
pub struct ForkExpr {
    pub fork_id: u32,
    pub paths: Vec<ForkPath>,
    pub sync_hints: Vec<String>,
}

impl ForkExpr {
    pub fn new(fork_id: u32) -> Self {
        ForkExpr {
            fork_id,
            paths: Vec::new(),
            sync_hints: Vec::new(),
        }
    }

    pub fn add_path(&mut self, path: ForkPath) {
        self.paths.push(path);
    }

    pub fn path_count(&self) -> usize {
        self.paths.len()
    }

    pub fn resources_accessed(&self) -> Vec<ResourceId> {
        let mut resources = Vec::new();
        for path in &self.paths {
            for access in &path.requires.accesses {
                resources.push(access.resource_id);
            }
        }
        resources.sort();
        resources.dedup();
        resources
    }
}

/// Compiled fork metadata with extracted information
#[derive(Debug, Clone)]
pub struct CompiledFork {
    pub fork_id: u32,
    pub num_paths: usize,
    pub resource_map: BTreeMap<ResourceId, Vec<AccessType>>,
    pub path_contracts: Vec<Vec<AccessSpec>>,
    pub generated_code: String,
}

impl CompiledFork {
    pub fn new(fork_id: u32, num_paths: usize) -> Self {
        CompiledFork {
            fork_id,
            num_paths,
            resource_map: BTreeMap::new(),
            path_contracts: vec![Vec::new(); num_paths],
            generated_code: String::new(),
        }
    }

    pub fn add_access(&mut self, path_id: usize, resource_id: ResourceId, access: AccessType) {
        if path_id < self.num_paths {
            let accesses = self.resource_map.entry(resource_id).or_insert_with(Vec::new);
            if !accesses.contains(&access) {
                accesses.push(access);
            }
            self.path_contracts[path_id].push(AccessSpec::new(resource_id, access));
        }
    }

    pub fn unique_resources(&self) -> Vec<ResourceId> {
        self.resource_map.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_id_creation() {
        let id = ResourceId::new(42);
        assert_eq!(id.0, 42);
    }

    #[test]
    fn test_access_spec_creation() {
        let spec = AccessSpec::read(ResourceId::new(1));
        assert_eq!(spec.resource_id, ResourceId::new(1));
        assert!(spec.access_type.is_read());
        assert!(!spec.access_type.is_write());
    }

    #[test]
    fn test_fork_path_creation() {
        let mut path = ForkPath::new(0, "path_code".to_string());
        path.add_access(AccessSpec::read(ResourceId::new(1)));
        path.add_access(AccessSpec::write(ResourceId::new(2)));

        assert_eq!(path.path_id, 0);
        assert_eq!(path.requires.accesses.len(), 2);
    }

    #[test]
    fn test_fork_expr_resources() {
        let mut fork = ForkExpr::new(1);

        let mut path0 = ForkPath::new(0, "code0".to_string());
        path0.add_access(AccessSpec::read(ResourceId::new(1)));
        fork.add_path(path0);

        let mut path1 = ForkPath::new(1, "code1".to_string());
        path1.add_access(AccessSpec::write(ResourceId::new(1)));
        path1.add_access(AccessSpec::read(ResourceId::new(2)));
        fork.add_path(path1);

        assert_eq!(fork.path_count(), 2);
        let resources = fork.resources_accessed();
        assert_eq!(resources.len(), 2);
        assert!(resources.contains(&ResourceId::new(1)));
        assert!(resources.contains(&ResourceId::new(2)));
    }

    #[test]
    fn test_compiled_fork() {
        let mut compiled = CompiledFork::new(1, 2);
        compiled.add_access(0, ResourceId::new(1), AccessType::Read);
        compiled.add_access(1, ResourceId::new(1), AccessType::Write);

        let resources = compiled.unique_resources();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0], ResourceId::new(1));
    }

    #[test]
    fn test_access_type_checks() {
        let read = AccessType::Read;
        let write = AccessType::Write;
        let readwrite = AccessType::ReadWrite;

        assert!(read.is_read());
        assert!(!read.is_write());

        assert!(!write.is_read());
        assert!(write.is_write());

        assert!(readwrite.is_read());
        assert!(readwrite.is_write());
    }
}
