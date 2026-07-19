//! Contract Verification
//!
//! Implements `requires` contracts that specify resource access requirements.
//! Contracts are verified at compile time and runtime.

use crate::effect::EffectSet;
use std::collections::HashMap;

/// Requirement level for contract enforcement
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequirementLevel {
    /// Must be satisfied (compile-time error if violated)
    Required = 1,
    /// Should be satisfied (warning if violated)
    Expected = 2,
    /// Optional, best-effort
    Optional = 3,
}

/// Single resource requirement
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceRequirement {
    /// Resource ID required
    resource_id: u32,
    /// Whether write access is needed
    requires_write: bool,
}

impl ResourceRequirement {
    /// Create read requirement
    pub fn read(resource_id: u32) -> Self {
        ResourceRequirement {
            resource_id,
            requires_write: false,
        }
    }

    /// Create write requirement
    pub fn write(resource_id: u32) -> Self {
        ResourceRequirement {
            resource_id,
            requires_write: true,
        }
    }

    /// Get resource ID
    #[inline]
    pub fn resource_id(&self) -> u32 {
        self.resource_id
    }

    /// Check if write is required
    #[inline]
    pub fn requires_write(&self) -> bool {
        self.requires_write
    }
}

/// Contract specifying resource requirements
#[repr(C)]
#[derive(Clone)]
pub struct RequiresContract {
    /// Name of contract (function or path name)
    name: String,
    /// Requirements (sorted by resource ID)
    requirements: Vec<ResourceRequirement>,
    /// Level of enforcement
    level: RequirementLevel,
}

impl RequiresContract {
    /// Create new contract
    pub fn new(name: String, level: RequirementLevel) -> Self {
        RequiresContract {
            name,
            requirements: Vec::new(),
            level,
        }
    }

    /// Add requirement
    pub fn add_requirement(&mut self, req: ResourceRequirement) {
        self.requirements.push(req);
        self.requirements.sort_by_key(|r| r.resource_id);
    }

    /// Get requirements
    pub fn requirements(&self) -> &[ResourceRequirement] {
        &self.requirements
    }

    /// Get contract name
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get enforcement level
    #[inline]
    pub fn level(&self) -> RequirementLevel {
        self.level
    }

    /// Check if contract is satisfied by effect set
    pub fn is_satisfied_by(&self, effects: &EffectSet) -> bool {
        for req in &self.requirements {
            let satisfied = effects
                .effects()
                .iter()
                .any(|e| {
                    e.resource_id() == req.resource_id
                        && (!req.requires_write || e.is_write())
                });

            if !satisfied {
                return false;
            }
        }
        true
    }

    /// Get unsatisfied requirements
    pub fn unsatisfied_requirements(&self, effects: &EffectSet) -> Vec<ResourceRequirement> {
        self.requirements
            .iter()
            .copied()
            .filter(|req| {
                !effects
                    .effects()
                    .iter()
                    .any(|e| {
                        e.resource_id() == req.resource_id
                            && (!req.requires_write || e.is_write())
                    })
            })
            .collect()
    }
}

/// Contract checker for verifying fork path contracts
#[repr(C)]
#[derive(Clone)]
pub struct ContractChecker {
    /// Named contracts
    contracts: HashMap<String, RequiresContract>,
    /// Violations found
    violations: Vec<ContractViolation>,
}

/// A contract violation
#[repr(C)]
#[derive(Debug, Clone)]
pub struct ContractViolation {
    /// Contract name
    pub contract_name: String,
    /// Path ID that violated
    pub path_id: u32,
    /// Unsatisfied requirements
    pub missing_resources: Vec<u32>,
}

impl ContractChecker {
    /// Create new contract checker
    pub fn new() -> Self {
        ContractChecker {
            contracts: HashMap::new(),
            violations: Vec::new(),
        }
    }

    /// Register contract
    pub fn register_contract(&mut self, contract: RequiresContract) {
        self.contracts.insert(contract.name().to_string(), contract);
    }

    /// Check contract for path
    pub fn check_contract(
        &mut self,
        contract_name: &str,
        path_id: u32,
        effects: &EffectSet,
    ) -> Result<(), Vec<u32>> {
        if let Some(contract) = self.contracts.get(contract_name) {
            let missing = contract.unsatisfied_requirements(effects);
            if !missing.is_empty() {
                let missing_resources: Vec<u32> =
                    missing.iter().map(|r| r.resource_id).collect();

                if contract.level() == RequirementLevel::Required {
                    self.violations.push(ContractViolation {
                        contract_name: contract_name.to_string(),
                        path_id,
                        missing_resources: missing_resources.clone(),
                    });
                }

                return Err(missing_resources);
            }
            Ok(())
        } else {
            Err(vec![])
        }
    }

    /// Get all violations
    pub fn violations(&self) -> &[ContractViolation] {
        &self.violations
    }

    /// Clear violations
    pub fn clear_violations(&mut self) {
        self.violations.clear();
    }

    /// Check if any required contracts violated
    pub fn has_required_violations(&self) -> bool {
        !self.violations.is_empty()
    }
}

impl Default for ContractChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::{Effect, EffectType};

    #[test]
    fn test_requirement_creation() {
        let req_read = ResourceRequirement::read(1);
        assert_eq!(req_read.resource_id(), 1);
        assert!(!req_read.requires_write());

        let req_write = ResourceRequirement::write(2);
        assert_eq!(req_write.resource_id(), 2);
        assert!(req_write.requires_write());
    }

    #[test]
    fn test_contract_creation() {
        let contract = RequiresContract::new("test_path".to_string(), RequirementLevel::Required);
        assert_eq!(contract.name(), "test_path");
        assert_eq!(contract.level(), RequirementLevel::Required);
    }

    #[test]
    fn test_contract_satisfied() {
        let mut contract = RequiresContract::new("test".to_string(), RequirementLevel::Required);
        contract.add_requirement(ResourceRequirement::read(1));

        let mut effects = EffectSet::new();
        effects.add_effect(Effect::new(1, EffectType::Read));

        assert!(contract.is_satisfied_by(&effects));
    }

    #[test]
    fn test_contract_not_satisfied() {
        let mut contract = RequiresContract::new("test".to_string(), RequirementLevel::Required);
        contract.add_requirement(ResourceRequirement::write(1));

        let mut effects = EffectSet::new();
        effects.add_effect(Effect::new(1, EffectType::Read));

        assert!(!contract.is_satisfied_by(&effects));
    }

    #[test]
    fn test_contract_checker() {
        let mut checker = ContractChecker::new();

        let mut contract = RequiresContract::new("path0".to_string(), RequirementLevel::Required);
        contract.add_requirement(ResourceRequirement::read(1));
        checker.register_contract(contract);

        let mut effects = EffectSet::new();
        effects.add_effect(Effect::new(1, EffectType::Read));

        let result = checker.check_contract("path0", 0, &effects);
        assert!(result.is_ok());
    }

    #[test]
    fn test_contract_violation() {
        let mut checker = ContractChecker::new();

        let mut contract = RequiresContract::new("path0".to_string(), RequirementLevel::Required);
        contract.add_requirement(ResourceRequirement::write(1));
        checker.register_contract(contract);

        let mut effects = EffectSet::new();
        effects.add_effect(Effect::new(2, EffectType::Read));

        let result = checker.check_contract("path0", 0, &effects);
        assert!(result.is_err());
        assert!(checker.has_required_violations());
    }
}
