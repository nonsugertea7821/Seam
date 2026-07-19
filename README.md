# Seam

Seam is an experimental systems programming language and runtime model that treats **execution paths** as first-class, statically verifiable entities.

Most languages type values. Seam also types control-flow paths.
By attaching contracts to path behavior, Seam aims to make execution safety, recovery behavior, and shared-resource effects predictable at compile time.

## Status

This repository currently contains:

- A language and VM draft specification: [DRAFT.md](DRAFT.md)
- A substantial runtime/bootstrap PoC: [PoC/seam-bootstrap](PoC/seam-bootstrap)
- Design notes and phase reports: [PoC/seam-bootstrap/design](PoC/seam-bootstrap/design)

Current maturity:

- Language and ABI design are still evolving (draft stage)
- Runtime PoC is implemented and testable
- Not production-ready

## Core Model (High-Level)

The following is the stable conceptual surface used across this repository.

1. **Primitive and data types**
   - Primitive types with fixed size semantics
   - Immutable `record` for structural data
   - Mutable `resource` for stateful entities

2. **Channel-based execution**
   - `channel` is the control-flow unit (similar to a function, but path-oriented)
   - `entry` is the normal execution path
   - `collector` is the recovery path

3. **Abort and collect semantics**
   - `abort` does not use traditional stack unwinding
   - Parent callsites can bind recovery with `:collect`
   - VM performs direct control transfer to the collector boundary

4. **Static effect/contracts direction**
   - `requires { read/write ... }` contracts model resource access
   - Intended for compile-time conflict analysis and safe concurrency planning

## Architecture Direction

Seam design and PoC implementation focus on the following VM-level ideas:

- **PSSA** (Path-bounded Shadow Stack Arena): thread-local arena for path-bounded execution memory
- **CFP/RFP split context**: separate control/resource frame pointers for deterministic abort recovery
- **SARM** (Static Abort Register Map): static metadata for register restoration and jump targets
- **Direct Jump abort path**: O(1) abort-to-collector transfer
- **GAC** (Generational Arena Checkpoint): loop memory checkpoint/rollback behavior
- **2PST** (Two-Phase Static Transaction): speculative-to-commit model for concurrent resource writes

For formal details and rationale, see [DRAFT.md](DRAFT.md).

## PoC Implementation Summary

The PoC lives in [PoC/seam-bootstrap](PoC/seam-bootstrap) and currently provides:

- Core runtime modules for arena/context/abort handling
- Resource/effect/contract model scaffolding
- Compiler-side structures (AST, analysis, pseudo-code generation)
- Runtime linker + fork executor path
- Signal and debugger integration modules
- Architecture bindings for x86-64 and AArch64

Observed from current test run:

- `cargo test --release --lib` passes with **162/162 tests**

Main module surface is exported from [PoC/seam-bootstrap/src/lib.rs](PoC/seam-bootstrap/src/lib.rs).

## Getting Started

### Prerequisites

- Rust 1.70+ (Edition 2021)
- Windows, Linux, or macOS
- x86-64 (primary), AArch64 (secondary path in source)

### Build the PoC

```bash
cd PoC/seam-bootstrap
cargo build --release
```

### Run tests

```bash
cd PoC/seam-bootstrap
cargo test --release --lib
```

### Run demo binary

```bash
cd PoC/seam-bootstrap
cargo run --release --bin seam-vm
```

## Repository Map

- [DRAFT.md](DRAFT.md)
  - Full language/VM RFC draft
  - Includes conceptual model, ABI-level direction, and exploratory sections
- [PoC/seam-bootstrap/README.md](PoC/seam-bootstrap/README.md)
  - Detailed phase-by-phase PoC description
- [PoC/seam-bootstrap/REPORT.md](PoC/seam-bootstrap/REPORT.md)
  - Test coverage and requirement mapping report
- [PoC/seam-bootstrap/design](PoC/seam-bootstrap/design)
  - Focused design writeups (PSSA modernization, direct jump integration, signal integration, debugger integration, etc.)

## Design Notes Index

- [PoC/seam-bootstrap/design/PSSA_MODERNIZATION.md](PoC/seam-bootstrap/design/PSSA_MODERNIZATION.md)
- [PoC/seam-bootstrap/design/BARRIER_INSERTION.md](PoC/seam-bootstrap/design/BARRIER_INSERTION.md)
- [PoC/seam-bootstrap/design/RUNTIME_LINKING.md](PoC/seam-bootstrap/design/RUNTIME_LINKING.md)
- [PoC/seam-bootstrap/design/FORK_EXECUTOR.md](PoC/seam-bootstrap/design/FORK_EXECUTOR.md)
- [PoC/seam-bootstrap/design/PSEUDO_CODE_INTERPRETER.md](PoC/seam-bootstrap/design/PSEUDO_CODE_INTERPRETER.md)
- [PoC/seam-bootstrap/design/DIRECT_JUMP_INTEGRATION.md](PoC/seam-bootstrap/design/DIRECT_JUMP_INTEGRATION.md)
- [PoC/seam-bootstrap/design/SIGNAL_INTEGRATION.md](PoC/seam-bootstrap/design/SIGNAL_INTEGRATION.md)
- [PoC/seam-bootstrap/design/DEBUGGER_INTEGRATION.md](PoC/seam-bootstrap/design/DEBUGGER_INTEGRATION.md)

## Scope and Non-Goals (Current)

Current repository does **not** yet present a complete, production language toolchain.
At this stage, this project should be read as:

- A strong VM/runtime architecture prototype
- A language semantics draft under active design
- A research-grade implementation path toward Seam 1.0-alpha

## Roadmap (Pragmatic)

1. Stabilize language grammar/type rules around path typing and contracts
2. Tighten compiler-to-runtime ABI contract generation
3. Expand concurrency validation and failure mode proofs
4. Integrate end-to-end source pipeline (language input to executable runtime form)
5. Harden platform behavior and diagnostics for production-like workloads

## License

No license file is currently defined at repository root.
Please add a license before external distribution.
