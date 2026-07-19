//! Seam VM PoC Bootstrap
//!
//! Path-bounded Shadow Stack Arena (PSSA) implementation with hybrid context
//! (CFP/RFP) and static abort/collector semantics.
//! Includes 2PST (Two-Phase Static Transaction) support for fork paths.
//! Phase 3: Resource tracking with requires contracts and automatic synchronization.
//! Phase 4: Compiler integration with AST, parsing, and code generation.
//! Phase 6: Low-Level Runtime — ABI layer with CFP/RFP, shadow arena, SARM, GAC, direct jump.

#[path = "runtime/pssa.rs"]
pub mod pssa;
#[path = "runtime/context.rs"]
pub mod context;
#[path = "runtime/abort.rs"]
pub mod abort;
#[path = "execution/channel.rs"]
pub mod channel;
#[path = "execution/resource.rs"]
pub mod resource;
#[path = "execution/shadow_buffer.rs"]
pub mod shadow_buffer;
#[path = "execution/transaction.rs"]
pub mod transaction;
#[path = "execution/fork.rs"]
pub mod fork;
#[path = "compile/effect.rs"]
pub mod effect;
#[path = "compile/contract.rs"]
pub mod contract;
#[path = "execution/sync.rs"]
pub mod sync;
#[path = "compile/ast.rs"]
pub mod ast;
#[path = "compile/compiler.rs"]
pub mod compiler;
#[path = "compile/codegen.rs"]
pub mod codegen;
#[path = "execution/linker.rs"]
pub mod linker;
#[path = "runtime/cfp_rfp.rs"]
pub mod cfp_rfp;
#[path = "runtime/shadow_arena.rs"]
pub mod shadow_arena;
#[path = "runtime/sarm.rs"]
pub mod sarm;
#[path = "runtime/gac.rs"]
pub mod gac;
#[path = "runtime/direct_jump.rs"]
pub mod direct_jump;
#[path = "runtime/signal_handler.rs"]
pub mod signal_handler;
#[path = "runtime/debugger.rs"]
pub mod debugger;
#[path = "runtime/context_debug.rs"]
mod context_debug;

/// Logical package split for runtime-centric types and services.
pub mod runtime {
    pub use crate::abort::{AbortContext, AbortSignal, CollectorFn, CollectorTable};
    pub use crate::cfp_rfp::{HybridContextSwitch, PhysicalRegisters};
    pub use crate::context::{ControlFramePtr, ExecutionContext, FrameLayout, FramePointer, ResourceFramePtr};
    pub use crate::debugger::{Breakpoint, BreakpointCondition, BreakpointLocation, DebuggerContext, GhostFrameSnapshot};
    pub use crate::direct_jump::{CollectBindingTable, DirectJumpTarget};
    pub use crate::gac::LoopFrame;
    pub use crate::pssa::Arena;
    pub use crate::sarm::{SARMEntry, SARMTable};
    pub use crate::shadow_arena::{ShadowArena, ShadowBuffer as ShadowBufferArena};
    pub use crate::signal_handler::{SignalAbortTarget, SignalHandler};
}

/// Logical package split for static analysis and compilation pipeline.
pub mod compile {
    pub use crate::ast::{AccessSpec, AccessType, CompiledFork, ForkExpr, ForkPath as AstForkPath, RequiresClause, ResourceId};
    pub use crate::codegen::{CodeGenerator, GeneratedCode};
    pub use crate::compiler::{CompileAnalysis, CompileError, CompileResult, SeamCompiler};
    pub use crate::contract::{ContractChecker, RequirementLevel, RequiresContract, ResourceRequirement};
    pub use crate::effect::{Effect, EffectAnalysis, EffectSet, EffectType};
}

/// Logical package split for fork execution, transactions, and synchronization.
pub mod execution {
    pub use crate::channel::Channel;
    pub use crate::fork::{ForkContext, ForkGraph, ForkPath};
    pub use crate::linker::{AbortTarget, CodeInterpreter, ForkExecutionResult, ForkExecutor, Instruction, LinkedFork, PathResult, PathState, ResourceAccessTracker, RuntimeLinker};
    pub use crate::resource::{AccessSet, GlobalResource, ResourceAccess, UniqueRecord};
    pub use crate::shadow_buffer::ShadowBuffer;
    pub use crate::sync::{AutoSync, BarrierKind, MemoryBarrier, SyncKind, SyncPoint};
    pub use crate::transaction::{Transaction, TransactionManager, TransactionState};
}

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
