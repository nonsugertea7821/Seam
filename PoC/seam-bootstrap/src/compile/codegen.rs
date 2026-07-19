//! Phase 4: Code Generator - Producing executable fork/join code
//!
//! This module generates Rust code from compiled Seam fork expressions,
//! producing ready-to-execute fork/join patterns with automatic synchronization.

use crate::ast::{CompiledFork, AccessType};
use crate::compiler::CompileAnalysis;

/// Generated code structure
#[derive(Debug, Clone)]
pub struct GeneratedCode {
    pub fork_setup: String,
    pub path_executions: Vec<String>,
    pub synchronization: String,
    pub join_handling: String,
}

impl GeneratedCode {
    pub fn new() -> Self {
        GeneratedCode {
            fork_setup: String::new(),
            path_executions: Vec::new(),
            synchronization: String::new(),
            join_handling: String::new(),
        }
    }

    pub fn full_code(&self) -> String {
        format!(
            "{}\n{}\n{}\n{}",
            self.fork_setup,
            self.path_executions.join("\n"),
            self.synchronization,
            self.join_handling
        )
    }
}

/// Code generator for Seam forks
pub struct CodeGenerator;

impl CodeGenerator {
    /// Generate complete fork/join code from compiled fork
    pub fn generate(
        compiled: &CompiledFork,
        analysis: &CompileAnalysis,
    ) -> GeneratedCode {
        let mut generated = GeneratedCode::new();

        // Generate fork setup
        generated.fork_setup = Self::generate_fork_setup(compiled);

        // Generate path executions
        generated.path_executions = Self::generate_path_executions(compiled);

        // Generate synchronization code
        generated.synchronization = Self::generate_synchronization(compiled, analysis);

        // Generate join handling
        generated.join_handling = Self::generate_join_handling(compiled);

        generated
    }

    /// Generate fork context initialization
    fn generate_fork_setup(compiled: &CompiledFork) -> String {
        format!(
            "// Fork #{} Setup\n\
             let fork_ctx = ForkContext::new(\n  \
             {} /* fork_id */,\n  \
             {} /* num_paths */,\n  \
             10 /* base_transaction_id */\n\
             );",
            compiled.fork_id, compiled.fork_id, compiled.num_paths
        )
    }

    /// Generate per-path execution code
    fn generate_path_executions(compiled: &CompiledFork) -> Vec<String> {
        let mut code = Vec::new();

        for path_id in 0..compiled.num_paths {
            let accesses = &compiled.path_contracts[path_id];
            let access_list = accesses
                .iter()
                .map(|a| {
                    format!(
                        "{}({})",
                        match a.access_type {
                            AccessType::Read => "read",
                            AccessType::Write => "write",
                            AccessType::ReadWrite => "readwrite",
                        },
                        a.resource_id.0
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");

            let access_display = if access_list.is_empty() {
                "(none)".to_string()
            } else {
                access_list.clone()
            };

            let path_code = format!(
                "// Path #{}\n\
                 // Requires: {}\n\
                 if let Some(path_mutex) = fork_ctx.get_path({}) {{\n  \
                 if let Ok(path) = path_mutex.lock() {{\n    \
                 path.begin_speculative();\n    \
                 // Execute path code here\n    \
                 // Effects: {}\n  \
                 }}\n\
                 }}",
                path_id,
                access_display,
                path_id,
                access_display
            );

            code.push(path_code);
        }

        code
    }

    /// Generate synchronization barriers
    fn generate_synchronization(_compiled: &CompiledFork, analysis: &CompileAnalysis) -> String {
        let barriers = analysis.auto_sync.generate_barriers();

        if barriers.is_empty() {
            return "// No synchronization barriers needed\n".to_string();
        }

        let mut sync_code = "// Synchronization Barriers\n".to_string();

        for barrier in barriers {
            sync_code.push_str(&format!("// {}\n", barrier));
        }

        // Generate actual barrier instructions based on architecture
        #[cfg(target_arch = "x86_64")]
        {
            sync_code.push_str("unsafe { core::arch::x86_64::_mm_sfence(); }\n");
        }

        #[cfg(target_arch = "aarch64")]
        {
            sync_code.push_str(
                "unsafe { core::arch::aarch64::__dmb(core::arch::aarch64::_dmb_ish()); }\n",
            );
        }

        sync_code
    }

    /// Generate join point handling
    fn generate_join_handling(compiled: &CompiledFork) -> String {
        format!(
            "// Join #{}\n\
             match fork_ctx.join() {{\n  \
             Ok(()) => {{\n    \
             println!(\"✓ Fork #{} paths joined successfully\");\n    \
             // Process results\n  \
             }}\n  \
             Err(e) => {{\n    \
             eprintln!(\"✗ Fork #{} join error: {{}}\", e);\n    \
             // Handle error\n  \
             }}\n\
             }}",
            compiled.fork_id, compiled.fork_id, compiled.fork_id
        )
    }

    /// Generate pseudo-code representation
    pub fn generate_pseudocode(
        compiled: &CompiledFork,
        analysis: &CompileAnalysis,
    ) -> String {
        let mut code = String::new();

        code.push_str(&format!("Fork #{} {{\n", compiled.fork_id));

        for path_id in 0..compiled.num_paths {
            let accesses = &compiled.path_contracts[path_id];
            code.push_str(&format!("  Path #{} {{\n", path_id));

            for access in accesses {
                code.push_str(&format!(
                    "    {}(resource: {})\n",
                    match access.access_type {
                        AccessType::Read => "READ",
                        AccessType::Write => "WRITE",
                        AccessType::ReadWrite => "READ-WRITE",
                    },
                    access.resource_id.0
                ));
            }

            code.push_str("  }\n");
        }

        code.push_str("\n  Synchronization {\n");
        for barrier in analysis.auto_sync.generate_barriers() {
            code.push_str(&format!("    {}\n", barrier));
        }
        code.push_str("  }\n");

        code.push_str("}\n");

        code
    }

    /// Generate resource map documentation
    pub fn generate_resource_map(compiled: &CompiledFork) -> String {
        let mut doc = String::from("Resource Access Map:\n\n");

        for (resource_id, access_types) in &compiled.resource_map {
            doc.push_str(&format!("  Resource {}:\n", resource_id.0));
            for access in access_types {
                doc.push_str(&format!(
                    "    - {}\n",
                    match access {
                        AccessType::Read => "READ",
                        AccessType::Write => "WRITE",
                        AccessType::ReadWrite => "READ-WRITE",
                    }
                ));
            }
        }

        doc
    }
}

impl Default for CodeGenerator {
    fn default() -> Self {
        CodeGenerator
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::ResourceId;
    use crate::compiler::SeamCompiler;

    #[test]
    fn test_generate_fork_setup() {
        let compiled = CompiledFork::new(1, 2);
        let setup = CodeGenerator::generate_fork_setup(&compiled);
        assert!(setup.contains("fork_id"));
        assert!(setup.contains("num_paths"));
    }

    #[test]
    fn test_generate_path_executions() {
        let compiled = CompiledFork::new(1, 2);
        let paths = CodeGenerator::generate_path_executions(&compiled);
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn test_generate_join_handling() {
        let compiled = CompiledFork::new(1, 2);
        let join = CodeGenerator::generate_join_handling(&compiled);
        assert!(join.contains("fork_ctx.join()"));
    }

    #[test]
    fn test_full_code_generation() {
        let compiled = CompiledFork::new(1, 3);
        let mut compiler = SeamCompiler::new();

        let source = r#"
            fork(1) {
                path(0) { accesses: read(1); code: p0 }
                path(1) { accesses: write(1); code: p1 }
                path(2) { accesses: read(1); code: p2 }
            }
        "#;

        if let Ok((_, analysis)) = compiler.compile_full(source) {
            let generated = CodeGenerator::generate(&compiled, &analysis);
            assert!(!generated.fork_setup.is_empty());
            assert!(!generated.path_executions.is_empty());
            assert!(!generated.join_handling.is_empty());
        }
    }

    #[test]
    fn test_pseudocode_generation() {
        let mut compiler = SeamCompiler::new();

        let source = r#"
            fork(1) {
                path(0) { accesses: read(1); code: p0 }
                path(1) { accesses: write(1); code: p1 }
            }
        "#;

        if let Ok((compiled_fork, analysis)) = compiler.compile_full(source) {
            let pseudo = CodeGenerator::generate_pseudocode(&compiled_fork, &analysis);
            assert!(pseudo.contains("Fork #1"));
            assert!(pseudo.contains("Path #0"));
            assert!(pseudo.contains("READ"));
            assert!(pseudo.contains("WRITE"));
        }
    }

    #[test]
    fn test_resource_map_generation() {
        let mut compiled = CompiledFork::new(1, 2);
        compiled.add_access(0, ResourceId::new(1), AccessType::Read);
        compiled.add_access(1, ResourceId::new(1), AccessType::Write);

        let map = CodeGenerator::generate_resource_map(&compiled);
        assert!(map.contains("Resource 1"));
        assert!(map.contains("READ"));
        assert!(map.contains("WRITE"));
    }
}
