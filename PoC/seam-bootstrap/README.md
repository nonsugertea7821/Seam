# Seam VM PoC Bootstrap

Path Typing Virtual Machine - Proof of Concept implementation of core Seam VM concepts from DRAFT.md

## Architecture Overview

This PoC implements the fundamental concepts described in the Seam architecture specification:

### Key Components

1. **PSSA (Path-bounded Shadow Stack Arena)**
   - Thread-local contiguous virtual memory region
   - Bump allocation with O(1) cost
   - Generational Arena Checkpoint (GAC) for loop support
   - Static upper bound verification at compile time

2. **Hybrid Execution Context**
   - **CFP** (Control Frame Pointer): Current execution context
   - **RFP** (Resource Frame Pointer): Aborted ghost frame for cleanup
   - Architecture-specific register bindings

3. **Abort & Collector Semantics**
   - Direct jump on abort (no stack unwinding)
   - IC flag (In-Collector) for secondary abort prevention
   - Static Abort Register Map (SARM) for register restoration

4. **Channel System**
   - `entry` path: Main execution flow
   - `collector` path: Error recovery and cleanup
   - Static binding of collector functions

5. **Architecture Support**
   - x86-64: CFP=rbp, RFP=r15, arena_ptr=r14
   - AArch64: CFP=x29, RFP=x28, arena_ptr=x27

## Building

```bash
cargo build --release
```

## Running the PoC

```bash
cargo run --release --bin seam-vm
```

## Testing

```bash
# Run all tests
cargo test --verbose

# Run specific test
cargo test test_arena_allocation -- --nocapture
```

## Project Structure

```
seam-bootstrap/
├── Cargo.toml
├── README.md
└── src/
    ├── lib.rs              # Library root with module exports
    ├── main.rs             # PoC demonstration binary
    ├── pssa.rs             # PSSA arena implementation
    ├── context.rs          # CFP/RFP execution context
    ├── abort.rs            # Abort/collector framework
    ├── channel.rs          # Channel definitions
    └── arch/
        ├── x86_64/
        │   └── mod.rs      # x86-64 assembly intrinsics
        └── aarch64/
            └── mod.rs      # AArch64 assembly intrinsics
```

## Features Implemented

- [x] PSSA memory management with bump allocation
- [x] Checkpoint save/restore (GAC)
- [x] Hybrid CFP/RFP context tracking
- [x] Channel metadata and invocation
- [x] Collector table and abort signaling
- [x] x86-64 intrinsics for frame pointer manipulation
- [x] AArch64 intrinsics for frame pointer manipulation
- [x] Inline assembly for register access
- [x] Basic unit tests

## Features for Full Implementation

- [ ] 2PST (Two-Phase Static Transaction) for parallel fork
- [ ] Resource access tracking and `requires` contract verification
- [ ] DWARF-free static abort register mapping (SARM)
- [ ] Loop frame handling with automatic arena rollback
- [ ] Thread-local context switching
- [ ] Signal/interrupt integration with abort paths
- [ ] Compiler backend code generation
- [ ] Seam language parser and type system

## References

See `DRAFT.md` for the complete Seam VM architecture specification including:
- Path Typing semantics
- Entry/Collector call patterns
- 2PST (Two-Phase Static Transaction) model
- OS boundary crossing strategies
- Memory safety guarantees

## Notes

This PoC focuses on demonstrating core VM mechanics. Production implementation would require:

1. **Performance tuning**: Optimize arena allocation and context switching
2. **Debugging support**: Add stack introspection and breakpoint framework
3. **Error handling**: Full error recovery with deterministic cleanup
4. **Concurrency**: Multi-threaded execution with lock-free algorithms
5. **Compiler integration**: Backend code generation from Seam source

## License

This is a research prototype for the Axiomium Seam project.
