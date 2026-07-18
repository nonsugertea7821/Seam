//! Seam VM PoC Bootstrap
//!
//! Path-bounded Shadow Stack Arena (PSSA) implementation with hybrid context
//! (CFP/RFP) and static abort/collector semantics.

pub mod pssa;
pub mod context;
pub mod abort;
pub mod channel;

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

/// Seam VM initialization
pub fn vm_init(arena_size: usize) -> Result<ExecutionContext, &'static str> {
    ExecutionContext::new(arena_size)
}
