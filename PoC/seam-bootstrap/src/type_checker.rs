//! Phase 5: Seam Type Checker
//!
//! Static semantic verification of Seam programs:
//! - Type resolution: all named types must be declared
//! - Resource contract validation: requires { read/write } fields must exist
//! - Collect binding validity: `:collect Channel` must name a declared channel
//! - Fork conflict detection: write-write conflicts are errors, read-write need barriers
//! - Duplicate definition detection

use crate::seam_lang::*;
use std::collections::HashMap;

// ===========================================================================
// Type Error
// ===========================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum TypeError {
    /// A named type reference has no declaration
    UndeclaredType { name: String, context: String },
    /// A resource referenced in `requires` is not declared
    UndeclaredResource { resource: String, context: String },
    /// A field referenced in `requires` does not exist on the resource
    UndeclaredField { resource: String, field: String },
    /// `:collect Foo` references an undeclared channel
    UndeclaredCollectTarget { channel: String, context: String },
    /// A record and resource share the same name
    DuplicateDefinition { name: String, kind: String },
    /// Two fork paths write the same resource (deadlock risk)
    ForkWriteConflict { path_a: u32, path_b: u32, resource: String },
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeError::UndeclaredType { name, context } =>
                write!(f, "Undeclared type '{}' used in {}", name, context),
            TypeError::UndeclaredResource { resource, context } =>
                write!(f, "Undeclared resource '{}' in requires block of {}", resource, context),
            TypeError::UndeclaredField { resource, field } =>
                write!(f, "Resource '{}' has no field '{}'", resource, field),
            TypeError::UndeclaredCollectTarget { channel, context } =>
                write!(f, "':collect {}' refers to undeclared channel in {}", channel, context),
            TypeError::DuplicateDefinition { name, kind } =>
                write!(f, "Duplicate {} definition: '{}'", kind, name),
            TypeError::ForkWriteConflict { path_a, path_b, resource } =>
                write!(f, "Fork write-write conflict: paths {} and {} both write '{}' — mutex required",
                       path_a, path_b, resource),
        }
    }
}

// ===========================================================================
// Type Environment
// ===========================================================================

/// Kind of a declared item
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    Record,
    Resource,
    Channel,
}

/// Resolved symbol table built from a SeamProgram
pub struct TypeEnvironment {
    pub records:  HashMap<String, RecordDef>,
    pub resources: HashMap<String, ResourceDef>,
    pub channels: HashMap<String, ChannelDef>,
}

impl TypeEnvironment {
    pub fn new() -> Self {
        TypeEnvironment {
            records:   HashMap::new(),
            resources: HashMap::new(),
            channels:  HashMap::new(),
        }
    }

    pub fn resolve(&self, name: &str) -> Option<ItemKind> {
        if self.records.contains_key(name)   { return Some(ItemKind::Record);   }
        if self.resources.contains_key(name) { return Some(ItemKind::Resource); }
        if self.channels.contains_key(name)  { return Some(ItemKind::Channel);  }
        None
    }

    pub fn get_resource_field(&self, resource: &str, field: &str) -> Option<&FieldDef> {
        self.resources.get(resource)?.fields.iter().find(|f| f.name == field)
    }
}

impl Default for TypeEnvironment {
    fn default() -> Self { TypeEnvironment::new() }
}

// ===========================================================================
// Type Check Result
// ===========================================================================

#[derive(Debug)]
pub struct TypeCheckResult {
    pub errors:   Vec<TypeError>,
    pub warnings: Vec<String>,
    /// Number of channels verified
    pub channel_count: usize,
    /// Number of global resources found
    pub resource_count: usize,
    /// (path_a, path_b, resource) for each detected fork conflict (RAW/WAR)
    pub read_write_conflicts: Vec<(u32, u32, String)>,
}

impl TypeCheckResult {
    pub fn new() -> Self {
        TypeCheckResult {
            errors:   Vec::new(),
            warnings: Vec::new(),
            channel_count: 0,
            resource_count: 0,
            read_write_conflicts: Vec::new(),
        }
    }

    pub fn is_ok(&self) -> bool { self.errors.is_empty() }

    pub fn error_count(&self)   -> usize { self.errors.len() }
    pub fn warning_count(&self) -> usize { self.warnings.len() }
}

impl Default for TypeCheckResult {
    fn default() -> Self { TypeCheckResult::new() }
}

// ===========================================================================
// Type Checker
// ===========================================================================

pub struct TypeChecker {
    env: TypeEnvironment,
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker { env: TypeEnvironment::new() }
    }

    // --- Program-level check ---

    pub fn check_program(&mut self, program: &SeamProgram) -> TypeCheckResult {
        let mut result = TypeCheckResult::new();

        // Phase 1: Register all top-level declarations (forward declarations)
        for item in &program.items {
            match item {
                SeamItem::Record(r) => {
                    if self.env.records.contains_key(&r.name) ||
                       self.env.resources.contains_key(&r.name)
                    {
                        result.errors.push(TypeError::DuplicateDefinition {
                            name: r.name.clone(), kind: "record".to_string()
                        });
                    } else {
                        self.env.records.insert(r.name.clone(), r.clone());
                    }
                }
                SeamItem::Resource(r) => {
                    if self.env.resources.contains_key(&r.name) ||
                       self.env.records.contains_key(&r.name)
                    {
                        result.errors.push(TypeError::DuplicateDefinition {
                            name: r.name.clone(), kind: "resource".to_string()
                        });
                    } else {
                        self.env.resources.insert(r.name.clone(), r.clone());
                        result.resource_count += 1;
                    }
                }
                SeamItem::Channel(c) => {
                    if self.env.channels.contains_key(&c.name) {
                        result.errors.push(TypeError::DuplicateDefinition {
                            name: c.name.clone(), kind: "channel".to_string()
                        });
                    } else {
                        self.env.channels.insert(c.name.clone(), c.clone());
                        result.channel_count += 1;
                    }
                }
            }
        }

        // Phase 2: Deep validation of each item
        for item in &program.items {
            match item {
                SeamItem::Record(r)   => self.check_record(r, &mut result),
                SeamItem::Resource(r) => self.check_resource_top(r, &mut result),
                SeamItem::Channel(c)  => self.check_channel(c, &mut result),
            }
        }

        result
    }

    // --- Type reference validation ---

    fn check_type(&self, ty: &SeamType, context: &str, result: &mut TypeCheckResult) {
        match ty {
            SeamType::Named(name) | SeamType::Unique(name) => {
                // Accept primitive aliases and declared user types
                if SeamPrimitive::from_str(name).is_none() && self.env.resolve(name).is_none() {
                    result.errors.push(TypeError::UndeclaredType {
                        name: name.clone(),
                        context: context.to_string(),
                    });
                }
            }
            SeamType::Primitive(_) => {} // always valid
        }
    }

    // --- Record validation ---

    fn check_record(&self, rec: &RecordDef, result: &mut TypeCheckResult) {
        for field in &rec.fields {
            self.check_type(&field.ty, &format!("record '{}'", rec.name), result);
        }
    }

    // --- Resource validation ---

    fn check_resource_top(&self, res: &ResourceDef, result: &mut TypeCheckResult) {
        for field in &res.fields {
            self.check_type(&field.ty, &format!("resource '{}'", res.name), result);
        }
    }

    fn check_local_resource(&self, res: &ResourceDef, channel: &str, result: &mut TypeCheckResult) {
        for field in &res.fields {
            self.check_type(
                &field.ty,
                &format!("local resource '{}' in channel '{}'", res.name, channel),
                result,
            );
        }
    }

    // --- Requires contract validation ---

    fn check_requires(&self, req: &RequiresBlock, channel: &str, result: &mut TypeCheckResult) {
        for access in req.reads.iter().chain(req.writes.iter()) {
            if !self.env.resources.contains_key(&access.resource_type) {
                result.errors.push(TypeError::UndeclaredResource {
                    resource: access.resource_type.clone(),
                    context: format!("requires block of channel '{}'", channel),
                });
            } else if self.env.get_resource_field(&access.resource_type, &access.field_name).is_none() {
                result.errors.push(TypeError::UndeclaredField {
                    resource: access.resource_type.clone(),
                    field: access.field_name.clone(),
                });
            }
        }
    }

    // --- Channel validation ---

    fn check_channel(&self, ch: &ChannelDef, result: &mut TypeCheckResult) {
        // Local resources
        for local in &ch.local_resources {
            self.check_local_resource(local, &ch.name, result);
        }

        // Requires contract
        if let Some(req) = &ch.requires {
            self.check_requires(req, &ch.name, result);
        }

        // Entry signature
        self.check_type(
            &ch.entry.return_type,
            &format!("entry return type of '{}'", ch.name),
            result,
        );
        for param in &ch.entry.params {
            self.check_type(
                &param.ty,
                &format!("entry param '{}' of '{}'", param.name, ch.name),
                result,
            );
        }
        self.check_stmts(&ch.entry.body, &ch.name, result);

        // Collector signature
        self.check_type(
            &ch.collector.return_type,
            &format!("collector return type of '{}'", ch.name),
            result,
        );
        self.check_stmts(&ch.collector.body, &ch.name, result);
    }

    // --- Statement validation ---

    fn check_stmts(&self, stmts: &[SeamStmt], channel: &str, result: &mut TypeCheckResult) {
        for stmt in stmts {
            self.check_stmt(stmt, channel, result);
        }
    }

    fn check_stmt(&self, stmt: &SeamStmt, channel: &str, result: &mut TypeCheckResult) {
        match stmt {
            SeamStmt::Return(_) | SeamStmt::Abort => {}

            SeamStmt::Call { callee, collect, .. } => {
                // Called target: warn if unknown (may be external/FFI)
                if !self.env.channels.contains_key(callee.as_str())
                    && SeamPrimitive::from_str(callee).is_none()
                {
                    result.warnings.push(format!(
                        "Channel '{}' calls '{}' which is not a declared channel (external?)",
                        channel, callee
                    ));
                }
                // :collect target must be a declared channel
                if let Some(collect_name) = collect {
                    if !self.env.channels.contains_key(collect_name.as_str()) {
                        result.errors.push(TypeError::UndeclaredCollectTarget {
                            channel: collect_name.clone(),
                            context: format!("channel '{}'", channel),
                        });
                    }
                }
            }

            SeamStmt::Let { ty, .. } => {
                self.check_type(ty, &format!("let binding in '{}'", channel), result);
            }

            SeamStmt::If { then_body, else_body, .. } => {
                self.check_stmts(then_body, channel, result);
                if let Some(body) = else_body {
                    self.check_stmts(body, channel, result);
                }
            }

            SeamStmt::Fork { paths } => {
                self.check_fork(paths, channel, result);
            }
        }
    }

    // --- Fork conflict analysis ---

    fn check_fork(&self, paths: &[ForkPathStmt], channel: &str, result: &mut TypeCheckResult) {
        // Validate requires blocks and path bodies
        for path in paths {
            if let Some(req) = &path.requires {
                self.check_requires(req, channel, result);
            }
            self.check_stmts(&path.body, channel, result);
        }

        // Collect write sets per path
        let write_sets: Vec<(u32, Vec<String>)> = paths.iter().map(|p| {
            let writes = p.requires.as_ref()
                .map(|r| r.writes.iter().map(|a| a.resource_type.clone()).collect())
                .unwrap_or_default();
            (p.path_id, writes)
        }).collect();

        // Collect read sets per path
        let read_sets: Vec<(u32, Vec<String>)> = paths.iter().map(|p| {
            let reads = p.requires.as_ref()
                .map(|r| r.reads.iter().map(|a| a.resource_type.clone()).collect())
                .unwrap_or_default();
            (p.path_id, reads)
        }).collect();

        // Write-write conflict → error (requires mutex)
        for i in 0..write_sets.len() {
            for j in (i + 1)..write_sets.len() {
                let (id_a, ref writes_a) = write_sets[i];
                let (id_b, ref writes_b) = write_sets[j];
                for res in writes_a {
                    if writes_b.contains(res) {
                        result.errors.push(TypeError::ForkWriteConflict {
                            path_a: id_a,
                            path_b: id_b,
                            resource: res.clone(),
                        });
                    }
                }
            }
        }

        // Read-write conflict → warning (barrier required, handled by 2PST)
        for (write_path, ref writes) in &write_sets {
            for (read_path, ref reads) in &read_sets {
                if write_path == read_path { continue; }
                for res in writes {
                    if reads.contains(res) {
                        result.warnings.push(format!(
                            "Fork in '{}': path {} writes '{}' while path {} reads it — barrier inserted by 2PST",
                            channel, write_path, res, read_path
                        ));
                        result.read_write_conflicts.push((*write_path, *read_path, res.clone()));
                    }
                }
            }
        }
    }

    /// Expose the built type environment for downstream use
    pub fn environment(&self) -> &TypeEnvironment {
        &self.env
    }
}

impl Default for TypeChecker {
    fn default() -> Self { TypeChecker::new() }
}

/// Convenience: check a Seam program and return the result
pub fn check_seam(program: &SeamProgram) -> TypeCheckResult {
    TypeChecker::new().check_program(program)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seam_parser::parse_seam;

    #[test]
    fn test_valid_record_and_resource() {
        let src = r#"
            record Point { int x; int y; }
            resource State { var int count; }
        "#;
        let prog = parse_seam(src).expect("Parse failed");
        let result = check_seam(&prog);
        assert!(result.is_ok(), "Errors: {:?}", result.errors);
        assert_eq!(result.resource_count, 1);
    }

    #[test]
    fn test_undeclared_type_in_record() {
        let src = r#"
            record BadRecord { UnknownType field; }
        "#;
        let prog = parse_seam(src).expect("Parse failed");
        let result = check_seam(&prog);
        assert!(!result.is_ok());
        assert!(result.errors.iter().any(|e| matches!(e, TypeError::UndeclaredType { .. })));
    }

    #[test]
    fn test_named_type_resolves_to_declared_record() {
        let src = r#"
            record Inner { int value; }
            record Outer { Inner child; }
        "#;
        let prog = parse_seam(src).expect("Parse failed");
        let result = check_seam(&prog);
        assert!(result.is_ok(), "Errors: {:?}", result.errors);
    }

    #[test]
    fn test_valid_requires_contract() {
        let src = r#"
            resource Counter { var int count; var string label; }
            channel Reader {
                requires {
                    read { Counter.count; }
                    write { Counter.label; }
                }
                void entry() { return; }
                void collector { return; }
            }
        "#;
        let prog = parse_seam(src).expect("Parse failed");
        let result = check_seam(&prog);
        assert!(result.is_ok(), "Errors: {:?}", result.errors);
        assert_eq!(result.channel_count, 1);
    }

    #[test]
    fn test_undeclared_resource_in_requires() {
        let src = r#"
            channel Ghost {
                requires { read { PhantomResource.field; } }
                void entry() { return; }
                void collector { return; }
            }
        "#;
        let prog = parse_seam(src).expect("Parse failed");
        let result = check_seam(&prog);
        assert!(!result.is_ok());
        assert!(result.errors.iter().any(|e| matches!(e, TypeError::UndeclaredResource { .. })));
    }

    #[test]
    fn test_undeclared_field_in_requires() {
        let src = r#"
            resource State { var int value; }
            channel Checker {
                requires { read { State.nonexistent; } }
                void entry() { return; }
                void collector { return; }
            }
        "#;
        let prog = parse_seam(src).expect("Parse failed");
        let result = check_seam(&prog);
        assert!(!result.is_ok());
        assert!(result.errors.iter().any(|e| matches!(e, TypeError::UndeclaredField { .. })));
    }

    #[test]
    fn test_valid_collect_binding() {
        let src = r#"
            channel Child {
                void entry() { return; }
                void collector { return; }
            }
            channel GrandChild {
                void entry() { return; }
                void collector { return; }
            }
            channel Parent {
                void entry() {
                    Child() :collect GrandChild;
                    return;
                }
                void collector { return; }
            }
        "#;
        let prog = parse_seam(src).expect("Parse failed");
        let result = check_seam(&prog);
        assert!(result.is_ok(), "Errors: {:?}", result.errors);
    }

    #[test]
    fn test_invalid_collect_target() {
        let src = r#"
            channel Parent {
                void entry() {
                    Child() :collect NoSuchChannel;
                    return;
                }
                void collector { return; }
            }
        "#;
        let prog = parse_seam(src).expect("Parse failed");
        let result = check_seam(&prog);
        assert!(!result.is_ok());
        assert!(result.errors.iter().any(|e| matches!(e, TypeError::UndeclaredCollectTarget { .. })));
    }

    #[test]
    fn test_fork_write_write_conflict() {
        let src = r#"
            resource Shared { var int value; }
            channel Concurrent {
                void entry() {
                    fork {
                        path(0) {
                            requires { write { Shared.value; } }
                            return;
                        }
                        path(1) {
                            requires { write { Shared.value; } }
                            return;
                        }
                    }
                }
                void collector { return; }
            }
        "#;
        let prog = parse_seam(src).expect("Parse failed");
        let result = check_seam(&prog);
        assert!(!result.is_ok());
        assert!(result.errors.iter().any(|e| matches!(e, TypeError::ForkWriteConflict { .. })));
    }

    #[test]
    fn test_fork_read_write_warning() {
        let src = r#"
            resource Shared { var int value; }
            channel Parallel {
                void entry() {
                    fork {
                        path(0) {
                            requires { read { Shared.value; } }
                            return;
                        }
                        path(1) {
                            requires { write { Shared.value; } }
                            return;
                        }
                    }
                }
                void collector { return; }
            }
        "#;
        let prog = parse_seam(src).expect("Parse failed");
        let result = check_seam(&prog);
        // Read-write is a warning + conflict record, not an error
        assert!(result.is_ok(), "Unexpected errors: {:?}", result.errors);
        assert!(!result.read_write_conflicts.is_empty(), "Expected RAW/WAR conflict recorded");
        assert!(!result.warnings.is_empty(), "Expected barrier warning");
    }
}
