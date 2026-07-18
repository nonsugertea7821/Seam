//! Seam VM PoC Bootstrap
//!
//! Path-bounded Shadow Stack Arena (PSSA) implementation with hybrid context
//! (CFP/RFP) and static abort/collector semantics.
//! Includes 2PST (Two-Phase Static Transaction) support for fork paths.

pub mod pssa;
pub mod context;
pub mod abort;
pub mod channel;
pub mod resource;
pub mod shadow_buffer;
pub mod transaction;
pub mod fork;

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

/// Seam VM initialization
pub fn vm_init(arena_size: usize) -> Result<ExecutionContext, &'static str> {
    ExecutionContext::new(arena_size)
}
