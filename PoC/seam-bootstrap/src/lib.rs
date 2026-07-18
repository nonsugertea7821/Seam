//! Seam VM PoC Bootstrap
//!
//! Path-bounded Shadow Stack Arena (PSSA) implementation with hybrid context
//! (CFP/RFP) and static abort/collector semantics.
//! Includes 2PST (Two-Phase Static Transaction) support for fork paths.
//! Phase 3: Resource tracking with requires contracts and automatic synchronization.
//! Phase 4: Compiler integration with AST, parsing, and code generation.
//! Phase 6: Low-Level Runtime — ABI layer with CFP/RFP, shadow arena, SARM, GAC, direct jump.

pub mod pssa;
pub mod context;
pub mod abort;
pub mod channel;
pub mod resource;
pub mod shadow_buffer;
pub mod transaction;
pub mod fork;
pub mod effect;
pub mod contract;
pub mod sync;
pub mod ast;
pub mod compiler;
pub mod codegen;
pub mod linker;
pub mod cfp_rfp;
pub mod shadow_arena;
pub mod sarm;
pub mod gac;
pub mod direct_jump;
pub mod signal_handler;
pub mod debugger;

#[cfg(target_arch = "x86_64")]
pub mod arch {
    pub mod x86_64;
    pub use self::x86_64 as native;
}

#[cfg(target_arch = "aarch64")]
pub mod arch {
    pub mod aarch64;
    pub use self::aarch64 as native;
}

pub use pssa::Arena;
pub use context::{ExecutionContext, FramePointer};
pub use abort::{AbortSignal, CollectorTable};
pub use channel::Channel;
pub use resource::{GlobalResource, UniqueRecord, AccessSet, ResourceAccess};
pub use shadow_buffer::ShadowBuffer;
pub use transaction::{Transaction, TransactionManager, TransactionState};
pub use fork::{ForkContext, ForkGraph, ForkPath};
pub use effect::{Effect, EffectType, EffectSet, EffectAnalysis};
pub use contract::{RequiresContract, ContractChecker, ResourceRequirement, RequirementLevel};
pub use sync::{AutoSync, SyncPoint, SyncKind, MemoryBarrier, BarrierKind};
pub use ast::{ResourceId, AccessType, AccessSpec, RequiresClause, ForkPath as AstForkPath, ForkExpr, CompiledFork};
pub use compiler::{SeamCompiler, CompileError, CompileResult, CompileAnalysis};
pub use codegen::{CodeGenerator, GeneratedCode};
pub use linker::{RuntimeLinker, LinkedFork, PathState, PathResult, ForkExecutor, ForkExecutionResult, CodeInterpreter, Instruction, ResourceAccessTracker, AbortTarget};
pub use cfp_rfp::{PhysicalRegisters, HybridContextSwitch};
pub use shadow_arena::{ShadowBuffer as ShadowBufferArena, ShadowArena};
pub use signal_handler::{SignalHandler, SignalAbortTarget};
pub use debugger::{DebuggerContext, Breakpoint, BreakpointLocation, BreakpointCondition, GhostFrameSnapshot};
pub use sarm::{SARMEntry, SARMTable};
pub use gac::LoopFrame;
pub use direct_jump::{DirectJumpTarget, CollectBindingTable};

/// Seam VM initialization
pub fn vm_init(arena_size: usize) -> Result<ExecutionContext, &'static str> {
    ExecutionContext::new(arena_size)
}
