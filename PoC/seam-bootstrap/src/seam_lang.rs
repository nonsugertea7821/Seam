//! Phase 5: Seam Language AST Definitions
//!
//! Complete Abstract Syntax Tree for the Seam programming language.
//! Based on the DRAFT specification covering: primitives, records, resources,
//! channels, requires contracts, fork/join, and abort/collect semantics.

use std::fmt;

// ===========================================================================
// Primitive Types
// ===========================================================================

/// Seam primitive types — platform-independent fixed-size semantics
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SeamPrimitive {
    Void,
    Bool,
    Byte,
    UByte,
    Short,
    UShort,
    Int,
    UInt,
    Long,
    ULong,
    Float,
    Double,
    Char,
    SeamString,
}

impl SeamPrimitive {
    /// Bit width of the primitive type
    pub fn bit_width(&self) -> usize {
        match self {
            SeamPrimitive::Void => 0,
            SeamPrimitive::Bool => 1,
            SeamPrimitive::Byte | SeamPrimitive::UByte | SeamPrimitive::Char => 8,
            SeamPrimitive::Short | SeamPrimitive::UShort => 16,
            SeamPrimitive::Int | SeamPrimitive::UInt | SeamPrimitive::Float => 32,
            SeamPrimitive::Long | SeamPrimitive::ULong | SeamPrimitive::Double => 64,
            SeamPrimitive::SeamString => 0, // variable length
        }
    }

    /// Whether the type is a signed integer
    pub fn is_signed(&self) -> bool {
        matches!(self,
            SeamPrimitive::Byte | SeamPrimitive::Short |
            SeamPrimitive::Int  | SeamPrimitive::Long
        )
    }

    /// Whether the type is floating-point
    pub fn is_float(&self) -> bool {
        matches!(self, SeamPrimitive::Float | SeamPrimitive::Double)
    }

    /// Whether the type is void
    pub fn is_void(&self) -> bool {
        *self == SeamPrimitive::Void
    }

    /// Parse from a keyword string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "void"   => Some(SeamPrimitive::Void),
            "bool"   => Some(SeamPrimitive::Bool),
            "byte"   => Some(SeamPrimitive::Byte),
            "ubyte"  => Some(SeamPrimitive::UByte),
            "short"  => Some(SeamPrimitive::Short),
            "ushort" => Some(SeamPrimitive::UShort),
            "int"    => Some(SeamPrimitive::Int),
            "uint"   => Some(SeamPrimitive::UInt),
            "long"   => Some(SeamPrimitive::Long),
            "ulong"  => Some(SeamPrimitive::ULong),
            "float"  => Some(SeamPrimitive::Float),
            "double" => Some(SeamPrimitive::Double),
            "char"   => Some(SeamPrimitive::Char),
            "string" => Some(SeamPrimitive::SeamString),
            _ => None,
        }
    }
}

impl fmt::Display for SeamPrimitive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SeamPrimitive::Void   => "void",
            SeamPrimitive::Bool   => "bool",
            SeamPrimitive::Byte   => "byte",
            SeamPrimitive::UByte  => "ubyte",
            SeamPrimitive::Short  => "short",
            SeamPrimitive::UShort => "ushort",
            SeamPrimitive::Int    => "int",
            SeamPrimitive::UInt   => "uint",
            SeamPrimitive::Long   => "long",
            SeamPrimitive::ULong  => "ulong",
            SeamPrimitive::Float  => "float",
            SeamPrimitive::Double => "double",
            SeamPrimitive::Char   => "char",
            SeamPrimitive::SeamString => "string",
        };
        write!(f, "{}", s)
    }
}

// ===========================================================================
// Type Reference
// ===========================================================================

/// Seam type reference
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeamType {
    Primitive(SeamPrimitive),
    /// Named user-defined type (Record, Resource, or Channel)
    Named(String),
    /// Unique (linear ownership) record — moved, not copied
    Unique(String),
}

impl SeamType {
    pub fn void() -> Self { SeamType::Primitive(SeamPrimitive::Void) }

    pub fn is_void(&self) -> bool {
        matches!(self, SeamType::Primitive(SeamPrimitive::Void))
    }

    /// Parse a type from a keyword string
    pub fn from_str(s: &str) -> Self {
        if let Some(p) = SeamPrimitive::from_str(s) {
            SeamType::Primitive(p)
        } else {
            SeamType::Named(s.to_string())
        }
    }
}

impl fmt::Display for SeamType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeamType::Primitive(p) => write!(f, "{}", p),
            SeamType::Named(n)     => write!(f, "{}", n),
            SeamType::Unique(n)    => write!(f, "unique {}", n),
        }
    }
}

// ===========================================================================
// Field Definition
// ===========================================================================

/// A single field declaration inside a record or resource
#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub ty: SeamType,
    /// Whether the field is mutable (var keyword, resource fields)
    pub is_var: bool,
}

impl FieldDef {
    pub fn new(name: impl Into<String>, ty: SeamType, is_var: bool) -> Self {
        FieldDef { name: name.into(), ty, is_var }
    }

    pub fn immutable(name: impl Into<String>, ty: SeamType) -> Self {
        FieldDef::new(name, ty, false)
    }

    pub fn mutable(name: impl Into<String>, ty: SeamType) -> Self {
        FieldDef::new(name, ty, true)
    }
}

// ===========================================================================
// Record Type — Immutable
// ===========================================================================

/// Record type: fully immutable compound data (copy-on-write semantics)
#[derive(Debug, Clone)]
pub struct RecordDef {
    pub name: String,
    pub fields: Vec<FieldDef>,
}

impl RecordDef {
    pub fn new(name: impl Into<String>) -> Self {
        RecordDef { name: name.into(), fields: Vec::new() }
    }

    pub fn with_field(mut self, field: FieldDef) -> Self {
        self.fields.push(field);
        self
    }
}

// ===========================================================================
// Resource Type — Mutable, Stateful
// ===========================================================================

/// Resource type: mutable stateful data requiring `requires` contracts
#[derive(Debug, Clone)]
pub struct ResourceDef {
    pub name: String,
    pub fields: Vec<FieldDef>,
    /// True when declared inside a channel (path-safe, no requires needed)
    pub is_local: bool,
}

impl ResourceDef {
    pub fn new(name: impl Into<String>) -> Self {
        ResourceDef { name: name.into(), fields: Vec::new(), is_local: false }
    }

    pub fn local(name: impl Into<String>) -> Self {
        ResourceDef { name: name.into(), fields: Vec::new(), is_local: true }
    }

    pub fn with_field(mut self, field: FieldDef) -> Self {
        self.fields.push(field);
        self
    }
}

// ===========================================================================
// Requires Contract
// ===========================================================================

/// A single resource field access reference in a requires block
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceFieldAccess {
    pub resource_type: String,
    pub field_name: String,
}

impl ResourceFieldAccess {
    pub fn new(resource_type: impl Into<String>, field_name: impl Into<String>) -> Self {
        ResourceFieldAccess {
            resource_type: resource_type.into(),
            field_name: field_name.into(),
        }
    }
}

impl fmt::Display for ResourceFieldAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.resource_type, self.field_name)
    }
}

/// `requires { read { ... } write { ... } }` contract block
#[derive(Debug, Clone, Default)]
pub struct RequiresBlock {
    pub reads: Vec<ResourceFieldAccess>,
    pub writes: Vec<ResourceFieldAccess>,
}

impl RequiresBlock {
    pub fn new() -> Self { RequiresBlock::default() }

    pub fn add_read(&mut self, access: ResourceFieldAccess) {
        if !self.reads.contains(&access) {
            self.reads.push(access);
        }
    }

    pub fn add_write(&mut self, access: ResourceFieldAccess) {
        if !self.writes.contains(&access) {
            self.writes.push(access);
        }
    }

    /// Sorted, deduplicated list of all resource type names accessed
    pub fn all_resources(&self) -> Vec<String> {
        let mut res: Vec<String> = self.reads.iter()
            .chain(self.writes.iter())
            .map(|a| a.resource_type.clone())
            .collect();
        res.sort();
        res.dedup();
        res
    }

    pub fn is_empty(&self) -> bool {
        self.reads.is_empty() && self.writes.is_empty()
    }
}

// ===========================================================================
// Expressions
// ===========================================================================

/// Seam expression
#[derive(Debug, Clone)]
pub enum SeamExpr {
    Ident(String),
    IntLit(i64),
    BoolLit(bool),
    StringLit(String),
    FieldAccess { expr: Box<SeamExpr>, field: String },
    Call { callee: String, args: Vec<SeamExpr> },
}

// ===========================================================================
// Statements
// ===========================================================================

/// A single statement inside entry or collector
#[derive(Debug, Clone)]
pub enum SeamStmt {
    /// `return;` or `return expr;`
    Return(Option<SeamExpr>),
    /// `abort;` — signal execution failure, invoke collector
    Abort,
    /// `Callee(args);` or `Callee(args) :collect OtherChannel;`
    Call { callee: String, args: Vec<SeamExpr>, collect: Option<String> },
    /// `Type name = expr;`
    Let { name: String, ty: SeamType, value: Option<SeamExpr> },
    /// `if (cond) { ... } else { ... }`
    If { condition: SeamExpr, then_body: Vec<SeamStmt>, else_body: Option<Vec<SeamStmt>> },
    /// `fork { path(id) { requires { ... } ... } ... }`
    Fork { paths: Vec<ForkPathStmt> },
}

/// A single path inside a fork statement
#[derive(Debug, Clone)]
pub struct ForkPathStmt {
    pub path_id: u32,
    pub requires: Option<RequiresBlock>,
    pub body: Vec<SeamStmt>,
}

// ===========================================================================
// Channel Definition
// ===========================================================================

/// Parameter of a channel entry signature
#[derive(Debug, Clone)]
pub struct ParamDef {
    pub name: String,
    pub ty: SeamType,
}

impl ParamDef {
    pub fn new(name: impl Into<String>, ty: SeamType) -> Self {
        ParamDef { name: name.into(), ty }
    }
}

/// `<type> entry(<params>) { body }` — the main execution path
#[derive(Debug, Clone)]
pub struct EntryDef {
    pub return_type: SeamType,
    pub params: Vec<ParamDef>,
    pub body: Vec<SeamStmt>,
}

/// `<type> collector { body }` — the recovery/cleanup path
#[derive(Debug, Clone)]
pub struct CollectorDef {
    pub return_type: SeamType,
    pub body: Vec<SeamStmt>,
}

/// Full channel definition with entry, collector, requires contract, and local resources
#[derive(Debug, Clone)]
pub struct ChannelDef {
    pub name: String,
    /// Local resources — path-safe, no requires contract needed
    pub local_resources: Vec<ResourceDef>,
    /// Global resource access contract
    pub requires: Option<RequiresBlock>,
    pub entry: EntryDef,
    pub collector: CollectorDef,
}

// ===========================================================================
// Top-Level Program
// ===========================================================================

/// A top-level item in a Seam source file
#[derive(Debug, Clone)]
pub enum SeamItem {
    Record(RecordDef),
    Resource(ResourceDef),
    Channel(ChannelDef),
}

impl SeamItem {
    pub fn name(&self) -> &str {
        match self {
            SeamItem::Record(r)   => &r.name,
            SeamItem::Resource(r) => &r.name,
            SeamItem::Channel(c)  => &c.name,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            SeamItem::Record(_)   => "record",
            SeamItem::Resource(_) => "resource",
            SeamItem::Channel(_)  => "channel",
        }
    }

    pub fn as_record(&self)   -> Option<&RecordDef>   { if let SeamItem::Record(r)   = self { Some(r) } else { None } }
    pub fn as_resource(&self) -> Option<&ResourceDef> { if let SeamItem::Resource(r) = self { Some(r) } else { None } }
    pub fn as_channel(&self)  -> Option<&ChannelDef>  { if let SeamItem::Channel(c)  = self { Some(c) } else { None } }
}

/// A complete Seam source program
#[derive(Debug, Clone, Default)]
pub struct SeamProgram {
    pub items: Vec<SeamItem>,
}

impl SeamProgram {
    pub fn new() -> Self { SeamProgram::default() }

    pub fn add_item(&mut self, item: SeamItem) { self.items.push(item); }

    pub fn channels(&self)  -> impl Iterator<Item = &ChannelDef>  { self.items.iter().filter_map(|i| i.as_channel()) }
    pub fn resources(&self) -> impl Iterator<Item = &ResourceDef> { self.items.iter().filter_map(|i| i.as_resource()) }
    pub fn records(&self)   -> impl Iterator<Item = &RecordDef>   { self.items.iter().filter_map(|i| i.as_record()) }

    pub fn find_item(&self, name: &str) -> Option<&SeamItem> {
        self.items.iter().find(|i| i.name() == name)
    }

    pub fn item_count(&self) -> usize { self.items.len() }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive_types() {
        assert_eq!(SeamPrimitive::Int.bit_width(), 32);
        assert_eq!(SeamPrimitive::Long.bit_width(), 64);
        assert_eq!(SeamPrimitive::Bool.bit_width(), 1);
        assert_eq!(SeamPrimitive::Void.bit_width(), 0);
        assert!(SeamPrimitive::Int.is_signed());
        assert!(!SeamPrimitive::UInt.is_signed());
        assert!(SeamPrimitive::Float.is_float());
        assert!(SeamPrimitive::Void.is_void());
    }

    #[test]
    fn test_primitive_from_str() {
        assert_eq!(SeamPrimitive::from_str("int"),    Some(SeamPrimitive::Int));
        assert_eq!(SeamPrimitive::from_str("string"), Some(SeamPrimitive::SeamString));
        assert_eq!(SeamPrimitive::from_str("void"),   Some(SeamPrimitive::Void));
        assert_eq!(SeamPrimitive::from_str("double"), Some(SeamPrimitive::Double));
        assert_eq!(SeamPrimitive::from_str("unknown"), None);
    }

    #[test]
    fn test_seam_type() {
        let t = SeamType::from_str("int");
        assert_eq!(t, SeamType::Primitive(SeamPrimitive::Int));
        let t2 = SeamType::from_str("MyRecord");
        assert_eq!(t2, SeamType::Named("MyRecord".to_string()));
        assert!(SeamType::void().is_void());
        assert!(!SeamType::from_str("int").is_void());
    }

    #[test]
    fn test_record_def() {
        let rec = RecordDef::new("Data")
            .with_field(FieldDef::immutable("num", SeamType::from_str("int")))
            .with_field(FieldDef::immutable("str", SeamType::from_str("string")));
        assert_eq!(rec.name, "Data");
        assert_eq!(rec.fields.len(), 2);
        assert!(!rec.fields[0].is_var);
        assert!(!rec.fields[1].is_var);
    }

    #[test]
    fn test_resource_def() {
        let res = ResourceDef::new("MyResource")
            .with_field(FieldDef::mutable("counter", SeamType::from_str("int")))
            .with_field(FieldDef::mutable("label",   SeamType::from_str("string")));
        assert_eq!(res.name, "MyResource");
        assert_eq!(res.fields.len(), 2);
        assert!(res.fields[0].is_var);
        assert!(!res.is_local);
    }

    #[test]
    fn test_requires_block() {
        let mut req = RequiresBlock::new();
        req.add_read(ResourceFieldAccess::new("Counter", "count"));
        req.add_write(ResourceFieldAccess::new("Counter", "label"));
        assert_eq!(req.reads.len(), 1);
        assert_eq!(req.writes.len(), 1);
        assert_eq!(req.all_resources(), vec!["Counter"]);
        // Dedup
        req.add_read(ResourceFieldAccess::new("Counter", "count"));
        assert_eq!(req.reads.len(), 1);
    }

    #[test]
    fn test_resource_field_access_display() {
        let access = ResourceFieldAccess::new("SharedState", "value");
        assert_eq!(format!("{}", access), "SharedState.value");
    }

    #[test]
    fn test_channel_def_structure() {
        let entry = EntryDef {
            return_type: SeamType::void(),
            params: vec![ParamDef::new("arg", SeamType::from_str("int"))],
            body: vec![SeamStmt::Abort],
        };
        let collector = CollectorDef {
            return_type: SeamType::void(),
            body: vec![SeamStmt::Return(None)],
        };
        let ch = ChannelDef {
            name: "TestChannel".to_string(),
            local_resources: vec![],
            requires: None,
            entry,
            collector,
        };
        assert_eq!(ch.name, "TestChannel");
        assert_eq!(ch.entry.params.len(), 1);
        assert!(matches!(ch.entry.body[0], SeamStmt::Abort));
        assert!(matches!(ch.collector.body[0], SeamStmt::Return(None)));
    }

    #[test]
    fn test_seam_program() {
        let mut prog = SeamProgram::new();
        prog.add_item(SeamItem::Record(RecordDef::new("Point")));
        prog.add_item(SeamItem::Resource(ResourceDef::new("State")));
        assert_eq!(prog.item_count(), 2);
        assert!(prog.find_item("Point").is_some());
        assert!(prog.find_item("Missing").is_none());
        assert_eq!(prog.records().count(), 1);
        assert_eq!(prog.resources().count(), 1);
        assert_eq!(prog.channels().count(), 0);
    }

    #[test]
    fn test_local_resource() {
        let local = ResourceDef::local("Cache")
            .with_field(FieldDef::mutable("data", SeamType::from_str("int")));
        assert!(local.is_local);
        assert_eq!(local.name, "Cache");
    }
}
