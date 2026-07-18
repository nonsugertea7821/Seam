//! Phase 4: Seam Compiler - Parsing, Analysis, and Code Generation
//!
//! This module implements the compiler pipeline for Seam fork expressions:
//! 1. Parse source code syntax
//! 2. Extract effects and build AST
//! 3. Perform static analysis
//! 4. Generate executable code

use crate::ast::*;
use crate::effect::{Effect, EffectAnalysis, EffectType};
use crate::contract::{RequiresContract, ResourceRequirement, ContractChecker, RequirementLevel};
use crate::sync::AutoSync;

/// Compilation error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    SyntaxError(String),
    InvalidForkId,
    NoPathsDefined,
    DuplicatePathId,
    InvalidResourceId,
    InvalidAccessType,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::SyntaxError(msg) => write!(f, "Syntax error: {}", msg),
            CompileError::InvalidForkId => write!(f, "Invalid fork ID"),
            CompileError::NoPathsDefined => write!(f, "No paths defined in fork"),
            CompileError::DuplicatePathId => write!(f, "Duplicate path ID"),
            CompileError::InvalidResourceId => write!(f, "Invalid resource ID"),
            CompileError::InvalidAccessType => write!(f, "Invalid access type"),
        }
    }
}

pub type CompileResult<T> = Result<T, CompileError>;

/// Compiler state and operations
pub struct SeamCompiler {
    _auto_resource_id_counter: u32,
}

impl SeamCompiler {
    pub fn new() -> Self {
        SeamCompiler {
            _auto_resource_id_counter: 1,
        }
    }

    /// Parse Seam fork syntax from source code
    /// Format: fork(id) { path(0) { accesses: read(1), write(2); code } ... }
    pub fn parse_fork(&mut self, source: &str) -> CompileResult<ForkExpr> {
        let source = source.trim();

        // Extract fork ID
        let fork_id = self.extract_fork_id(source)?;
        let mut fork = ForkExpr::new(fork_id);

        // Extract paths
        let paths_content = self.extract_paths_content(source)?;
        let path_definitions = self.split_paths(&paths_content)?;

        let mut seen_path_ids = std::collections::HashSet::new();

        for path_def in path_definitions {
            let path = self.parse_path(&path_def)?;

            if seen_path_ids.contains(&path.path_id) {
                return Err(CompileError::DuplicatePathId);
            }
            seen_path_ids.insert(path.path_id);

            fork.add_path(path);
        }

        if fork.path_count() == 0 {
            return Err(CompileError::NoPathsDefined);
        }

        Ok(fork)
    }

    /// Extract fork ID from fork declaration
    fn extract_fork_id(&self, source: &str) -> CompileResult<u32> {
        if let Some(start) = source.find("fork(") {
            if let Some(end) = source[start + 5..].find(')') {
                if let Ok(id) = source[start + 5..start + 5 + end].trim().parse::<u32>() {
                    return Ok(id);
                }
            }
        }
        Err(CompileError::InvalidForkId)
    }

    /// Extract the content between outer braces
    fn extract_paths_content(&self, source: &str) -> CompileResult<String> {
        if let Some(start) = source.find('{') {
            if let Some(end) = source.rfind('}') {
                if end > start {
                    return Ok(source[start + 1..end].to_string());
                }
            }
        }
        Err(CompileError::SyntaxError("Missing braces".to_string()))
    }

    /// Split multiple path definitions
    fn split_paths(&self, content: &str) -> CompileResult<Vec<String>> {
        let mut paths = Vec::new();
        let mut current_path = String::new();
        let mut brace_depth = 0;

        for ch in content.chars() {
            match ch {
                '{' => {
                    brace_depth += 1;
                    current_path.push(ch);
                }
                '}' => {
                    brace_depth -= 1;
                    current_path.push(ch);
                    if brace_depth == 0 && !current_path.trim().is_empty() {
                        paths.push(current_path.trim().to_string());
                        current_path.clear();
                    }
                }
                _ => current_path.push(ch),
            }
        }

        if !current_path.trim().is_empty() {
            paths.push(current_path.trim().to_string());
        }

        if paths.is_empty() {
            return Err(CompileError::NoPathsDefined);
        }

        Ok(paths)
    }

    /// Parse a single path definition
    fn parse_path(&mut self, path_def: &str) -> CompileResult<ForkPath> {
        // Extract path ID
        let path_id = self.extract_path_id(path_def)?;

        // Extract accesses
        let mut requires = RequiresClause::new();
        if let Some(start) = path_def.find("accesses:") {
            if let Some(end) = path_def[start..].find(';') {
                let accesses_str = &path_def[start + 9..start + end].trim();
                let accesses = self.parse_accesses(accesses_str)?;
                for access in accesses {
                    requires.add_access(access);
                }
            }
        }

        // Extract code section
        let code = self.extract_code_section(path_def).unwrap_or_else(|_| {
            format!("path_{}_code", path_id)
        });

        Ok(ForkPath::new(path_id, code).with_requires(requires))
    }

    /// Extract path ID from path declaration
    fn extract_path_id(&self, path_def: &str) -> CompileResult<u32> {
        if let Some(start) = path_def.find("path(") {
            if let Some(end) = path_def[start + 5..].find(')') {
                if let Ok(id) = path_def[start + 5..start + 5 + end].trim().parse::<u32>() {
                    return Ok(id);
                }
            }
        }
        Err(CompileError::SyntaxError("Invalid path declaration".to_string()))
    }

    /// Parse access specifications (read(1), write(2), etc.)
    fn parse_accesses(&mut self, accesses_str: &str) -> CompileResult<Vec<AccessSpec>> {
        let mut accesses = Vec::new();

        for token in accesses_str.split(',') {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }

            if token.starts_with("read(") && token.ends_with(')') {
                let id_str = &token[5..token.len() - 1];
                if let Ok(id) = id_str.parse::<u32>() {
                    accesses.push(AccessSpec::read(ResourceId::new(id)));
                } else {
                    return Err(CompileError::InvalidResourceId);
                }
            } else if token.starts_with("write(") && token.ends_with(')') {
                let id_str = &token[6..token.len() - 1];
                if let Ok(id) = id_str.parse::<u32>() {
                    accesses.push(AccessSpec::write(ResourceId::new(id)));
                } else {
                    return Err(CompileError::InvalidResourceId);
                }
            } else {
                return Err(CompileError::InvalidAccessType);
            }
        }

        Ok(accesses)
    }

    /// Extract code section from path definition
    fn extract_code_section(&self, path_def: &str) -> CompileResult<String> {
        if let Some(start) = path_def.find("code:") {
            if let Some(end) = path_def[start..].find('}') {
                let code = path_def[start + 5..start + end].trim();
                return Ok(code.to_string());
            }
        }
        Ok(String::new())
    }

    /// Compile fork expression with full analysis
    pub fn compile(&mut self, fork_expr: &ForkExpr) -> CompileResult<CompiledFork> {
        let mut compiled = CompiledFork::new(fork_expr.fork_id, fork_expr.path_count());

        // Extract accesses from AST and populate compiled fork
        for path in &fork_expr.paths {
            for access in &path.requires.accesses {
                let _access_type = match access.access_type {
                    AccessType::Read => EffectType::Read,
                    AccessType::Write => EffectType::Write,
                    AccessType::ReadWrite => EffectType::ReadWrite,
                };
                compiled.add_access(
                    path.path_id as usize,
                    access.resource_id,
                    access.access_type,
                );
            }
        }

        Ok(compiled)
    }

    /// Perform full compilation pipeline: parse -> analyze -> generate
    pub fn compile_full(
        &mut self,
        source: &str,
    ) -> CompileResult<(CompiledFork, CompileAnalysis)> {
        // Phase 1: Parse
        let fork_expr = self.parse_fork(source)?;

        // Phase 2: Compile to intermediate representation
        let compiled = self.compile(&fork_expr)?;

        // Phase 3: Analyze
        let analysis = self.analyze(&compiled)?;

        Ok((compiled, analysis))
    }

    /// Perform static analysis on compiled fork
    pub fn analyze(&self, compiled: &CompiledFork) -> CompileResult<CompileAnalysis> {
        // Build effect analysis
        let mut effect_analysis = EffectAnalysis::new(compiled.num_paths as u32);

        for path_id in 0..compiled.num_paths {
            for access_spec in &compiled.path_contracts[path_id] {
                let effect_type = match access_spec.access_type {
                    AccessType::Read => EffectType::Read,
                    AccessType::Write => EffectType::Write,
                    AccessType::ReadWrite => EffectType::ReadWrite,
                };

                let effect = Effect::new(access_spec.resource_id.0, effect_type);
                effect_analysis.add_effect_to_path(path_id as u32, effect).ok();
            }
        }

        // Detect conflicts
        let has_write_conflicts = effect_analysis.has_write_conflicts();
        let has_read_write_conflicts = effect_analysis.has_read_write_conflicts();
        let sync_resources = effect_analysis.required_sync_points();

        // Build contract checker
        let mut contract_checker = ContractChecker::new();
        for (path_id, accesses) in compiled.path_contracts.iter().enumerate() {
            let mut contract =
                RequiresContract::new(format!("path_{}", path_id), RequirementLevel::Required);
            for access in accesses {
                let req = match access.access_type {
                    AccessType::Read => ResourceRequirement::read(access.resource_id.0),
                    AccessType::Write => ResourceRequirement::write(access.resource_id.0),
                    AccessType::ReadWrite => ResourceRequirement::read(access.resource_id.0),
                };
                contract.add_requirement(req);
            }
            contract_checker.register_contract(contract);
        }

        // Auto-sync detection
        let mut auto_sync = AutoSync::new(true);
        auto_sync.set_analysis(effect_analysis.clone());

        Ok(CompileAnalysis {
            effect_analysis,
            contract_checker,
            auto_sync,
            has_write_conflicts,
            has_read_write_conflicts,
            sync_resources,
        })
    }
}

impl Default for SeamCompiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Analysis results from compilation
#[derive(Clone)]
pub struct CompileAnalysis {
    pub effect_analysis: EffectAnalysis,
    pub contract_checker: ContractChecker,
    pub auto_sync: AutoSync,
    pub has_write_conflicts: bool,
    pub has_read_write_conflicts: bool,
    pub sync_resources: Vec<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compiler_creation() {
        let compiler = SeamCompiler::new();
        assert_eq!(compiler._auto_resource_id_counter, 1);
    }

    #[test]
    fn test_extract_fork_id() {
        let compiler = SeamCompiler::new();
        let source = "fork(42) { }";
        assert_eq!(compiler.extract_fork_id(source).unwrap(), 42);
    }

    #[test]
    fn test_parse_accesses() {
        let mut compiler = SeamCompiler::new();
        let accesses_str = "read(1), write(2), read(3)";
        let accesses = compiler.parse_accesses(accesses_str).unwrap();
        assert_eq!(accesses.len(), 3);
    }

    #[test]
    fn test_parse_simple_fork() {
        let mut compiler = SeamCompiler::new();
        let source = r#"
            fork(1) {
                path(0) { accesses: read(1), write(2); code: execute_path_0() }
                path(1) { accesses: write(1); code: execute_path_1() }
            }
        "#;

        let fork = compiler.parse_fork(source).unwrap();
        assert_eq!(fork.fork_id, 1);
        assert_eq!(fork.path_count(), 2);
    }

    #[test]
    fn test_compile_fork() {
        let mut compiler = SeamCompiler::new();
        let source = r#"
            fork(1) {
                path(0) { accesses: read(1); code: path_0_code }
                path(1) { accesses: write(1); code: path_1_code }
            }
        "#;

        let fork = compiler.parse_fork(source).unwrap();
        let compiled = compiler.compile(&fork).unwrap();
        assert_eq!(compiled.fork_id, 1);
        assert_eq!(compiled.num_paths, 2);
    }

    #[test]
    fn test_analyze_compiled_fork() {
        let mut compiler = SeamCompiler::new();
        let source = r#"
            fork(1) {
                path(0) { accesses: read(1); code: path_0 }
                path(1) { accesses: write(1); code: path_1 }
            }
        "#;

        let fork = compiler.parse_fork(source).unwrap();
        let compiled = compiler.compile(&fork).unwrap();
        let analysis = compiler.analyze(&compiled).unwrap();

        assert!(analysis.has_read_write_conflicts);
        assert!(!analysis.has_write_conflicts);
        assert!(!analysis.sync_resources.is_empty());
    }

    #[test]
    fn test_full_compilation_pipeline() {
        let mut compiler = SeamCompiler::new();
        let source = r#"
            fork(1) {
                path(0) { accesses: read(1), read(2); code: p0 }
                path(1) { accesses: write(1); code: p1 }
                path(2) { accesses: read(1), write(3); code: p2 }
            }
        "#;

        let (compiled, analysis) = compiler.compile_full(source).unwrap();
        assert_eq!(compiled.fork_id, 1);
        assert_eq!(compiled.num_paths, 3);
        assert!(analysis.has_read_write_conflicts);
    }
}
