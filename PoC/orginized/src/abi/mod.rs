//! Seam VM ABI public contract.
//!
//! This module defines the ABI boundary between Seam runtime semantics and
//! platform-specific execution primitives.
//!
//! The ABI layer separates:
//!
//! - execution allocation state,
//! - operating-system memory ownership,
//! - execution transfer state,
//! - architecture-specific CPU operations.
//!
//! Contract summary:
//!
//! - `OperatorArena` represents active allocation state only.
//! - `VirtualMapping` owns the underlying OS virtual memory resource.
//! - `ArenaCheckpoint` stores an allocation restoration boundary.
//! - `TransferContext` contains the minimum state required for ABI control
//!   transfer.
//! - `ControlOperatorPtr` and `ResourcePtr` provide semantic pointer wrappers.
//! - `AbiError` is the only error type exposed by the ABI boundary.
//!
//! Allocation contract:
//!
//! - `alloc(capacity)` creates one initialized `OperatorArena` and one owned
//!   `VirtualMapping`.
//! - The returned arena is immediately usable.
//! - The mapping owner is responsible for releasing the native memory.
//! - Failed allocation never exposes partially initialized ABI state.
//!
//! Arena contract:
//!
//! - `OperatorArena` does not own virtual memory.
//! - `OperatorArena` manages only allocation boundaries.
//! - Successful allocations advance the allocation boundary monotonically.
//! - `ArenaCheckpoint` is valid only during the lifetime of the originating
//!   arena.
//!
//! Transfer contract:
//!
//! - `transfer(context)` binds ABI state to architecture-defined registers.
//! - `transfer(context)` performs a direct, non-returning control transfer.
//! - The ABI layer does not resolve execution paths.
//! - The ABI layer does not determine collectors.
//! - The ABI layer does not restore arena state.
//!
//! Ownership contract:
//!
//! - `VirtualMapping` owns OS memory lifetime.
//! - `OperatorArena` owns allocation metadata only.
//! - `ArenaCheckpoint` owns no resources.
//! - `TransferContext` owns no resources.
//! - Pointer wrappers own no resources.
//!
//! Platform contract:
//!
//! - Platform modules allocate and release native virtual memory.
//! - Native failures are converted into `AbiError`.
//! - OS-specific error codes never cross the ABI boundary.
//!
//! Architecture contract:
//!
//! - Architecture modules provide CPU primitives only.
//! - Architecture modules do not define Seam execution semantics.
//! - ABI semantics are defined in core modules.

pub mod error;

#[path = "core/arena.rs"]
mod arena;

#[path = "core/checkpoint.rs"]
mod checkpoint;

#[path = "core/mapping.rs"]
mod mapping;

#[path = "core/pointer.rs"]
mod pointer;

#[path = "core/transfer.rs"]
mod transfer;

mod arch {
    #[cfg(target_arch = "x86_64")]
    #[path = "arch/x86_64.rs"]
    pub(crate) mod x86_64;

    #[cfg(target_arch = "aarch64")]
    #[path = "arch/aarch64.rs"]
    pub(crate) mod aarch64;
}

#[cfg(unix)]
#[path = "platform/unix.rs"]
mod unix;

#[cfg(windows)]
#[path = "platform/windows.rs"]
mod windows;

pub use arena::OperatorArena;
pub use checkpoint::ArenaCheckpoint;
pub use error::AbiError;
pub use mapping::VirtualMapping;
pub use pointer::{ControlOperatorPtr, ResourcePtr};
pub use transfer::{transfer, TransferContext};

#[cfg(unix)]
pub use unix::alloc;

#[cfg(windows)]
pub use windows::alloc;