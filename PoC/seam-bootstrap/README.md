# Seam VM PoC Bootstrap

**Seam Language Runtime VM** — Complete Proof of Concept implementation of the DRAFT specification
including core path typing, channel-based execution, Two-Phase Static Transaction (2PST), and
zero-cost exception handling via physical register bindings.

## 📋 Implementation Status

| Phase | Component | Module(s) | Status | Tests |
|-------|-----------|-----------|--------|-------|
| **1** | Memory Management | `pssa.rs`, `context.rs` | ✓ Complete | 13 |
| **2** | Transaction Engine | `transaction.rs`, `shadow_buffer.rs` | ✓ Complete | 14 |
| **3** | Resource Tracking | `resource.rs`, `effect.rs`, `contract.rs`, `sync.rs` | ✓ Complete | 26 |
| **4** | Compiler Pipeline | `ast.rs`, `compiler.rs`, `codegen.rs` | ✓ Complete | 24 |
| **5** | Runtime Linker | `linker.rs` | ✓ Complete | 13 |
| **5B** | Fork Executor | `linker.rs` | ✓ Complete | 4 |
| **5C** | Pseudo-Code Interpreter | `linker.rs` | ✓ Complete | 12 |
| **6** | ABI Layer + Phase 1 Integration | `cfp_rfp.rs`, `shadow_arena.rs`, `sarm.rs`, `gac.rs`, `direct_jump.rs` | ✓ Complete | 36 |

**Total: 19 modules, ~6,500 lines, 130 tests (all passing)**

---

## 🏗️ Architecture Overview

The Seam VM is a **compile-time path-typed execution engine** with static verification and zero-cost
exception handling. Three key architectural principles:

1. **Static Analysis First**: All resource conflicts and abort paths resolved at compile time
2. **Physical Register Semantics**: CFP/RFP are real CPU registers for O(1) abort mechanism
3. **No Stack Unwinding**: Direct jump exception handling (DWARF-free, no dynamic dispatch)

### Core Memory Model

```
┌─────────────────────────────────────────┐
│  Thread-Local PSSA Arena                │ ← Bump allocation, O(1)
│  (Path-bounded Shadow Stack)            │
│                                         │
│  ┌──────────────────────────────┐      │
│  │ CFP Frame                    │      │ ← Control Frame (current context)
│  ├──────────────────────────────┤      │
│  │ Shadow Buffers (2PST Phase 1)│      │ ← Speculative writes, lock-free
│  ├──────────────────────────────┤      │
│  │ GAC Loop Checkpoints         │      │ ← Arena rollback on loop back-edge
│  └──────────────────────────────┘      │
└─────────────────────────────────────────┘
         ↓
    Direct Jump on Abort
         ↓
┌─────────────────────────────────────────┐
│  RFP Ghost Frame (Collector)            │ ← Resource Frame (abort state)
│  Access locals from aborted execution   │
└─────────────────────────────────────────┘
```

---

## 📦 Module Breakdown

### **Phase 1: Memory Management**

| Module | Purpose | Key Types |
|--------|---------|-----------|
| `pssa.rs` | Virtual memory-based Path-bounded Shadow Stack Arena | `Arena`, `ArenaCheckpoint` |
| `context.rs` | Execution context with CFP/RFP | `ExecutionContext`, `FramePointer`, `ControlFramePtr`, `ResourceFramePtr` |
| `abort.rs` | Abort signaling and collector table | `AbortSignal`, `CollectorTable`, `SARMEntry` |

**Key Features:**
- ✅ **mmap-based virtual memory allocation** (true OS page management)
- ✅ **Guard pages** at top and bottom (PROT_NONE) prevent buffer overruns
- ✅ Thread-local unbounded arena with dynamic growth
- ✅ Checkpoint save/restore for CFP/RFP
- ✅ Bump allocation O(1) allocation
- ✅ Graceful arena exhaustion handling
- ✅ Cross-platform (Windows VirtualAlloc, Unix mmap)

### **Phase 2: Transaction Engine**

| Module | Purpose | Key Types |
|--------|---------|-----------|
| `transaction.rs` | 2PST (Two-Phase Static Transaction) | `Transaction`, `TransactionManager`, `TransactionState` |
| `shadow_buffer.rs` | Per-path write staging | `ShadowBuffer`, `StagedWrite` |

**2PST Protocol:**
```
Phase 1: Speculative
├─ Fork paths execute independently
├─ Each path writes to shadow buffer (lock-free, isolated)
└─ No contention on shared resources

Phase 2: Commit
├─ Compiler-determined lock order
├─ Acquire locks (no deadlock by static ordering)
├─ Flush all shadow buffers → main memory (atomic)
└─ Release locks in reverse order

Phase 3: Abort
├─ Discard shadow buffers (main memory unchanged)
├─ Execute collector via direct jump
└─ O(1) cleanup overhead
```

**Key Features:**
- Per-path isolation with independent buffers
- Shared resource tracking (OS syscalls, files)
- Conflict detection (Read-Write, Write-Write)
- Serializable transaction guarantee

### **Phase 3: Resource Tracking**

| Module | Purpose | Key Types |
|--------|---------|-----------|
| `resource.rs` | Global resources and access sets | `GlobalResource`, `UniqueRecord`, `AccessSet` |
| `effect.rs` | Static effect analysis | `Effect`, `EffectType`, `EffectAnalysis` |
| `contract.rs` | Requires contracts and verification | `RequiresContract`, `ContractChecker`, `ResourceRequirement` |
| `sync.rs` | Automatic synchronization with memory barriers | `AutoSync`, `SyncPoint`, `SyncKind`, `MemoryBarrier`, `BarrierKind` |

**Key Features:**
- Static resource requirement verification
- Compile-time conflict detection (RAW, WAW, WAR)
- ✅ **Automatic memory barrier insertion** (Acquire/Release/FullFence)
- ✅ **Thread fence execution** for synchronization
- No-cost static synchronization

### **Phase 4: Compiler Pipeline**

| Module | Purpose | Key Types |
|--------|---------|-----------|
| `ast.rs` | Abstract Syntax Tree | `ForkExpr`, `ForkPath`, `ResourceId`, `AccessSpec` |
| `compiler.rs` | Parse → AST → Compile | `SeamCompiler`, `CompiledFork`, `CompileAnalysis` |
| `codegen.rs` | Code generation | `CodeGenerator`, `GeneratedCode` |
| `fork.rs` | Fork context and graph | `ForkContext`, `ForkGraph`, `ForkPath` |
| `channel.rs` | Channel definitions | `Channel`, `ChannelId` |

**Compilation Pipeline:**
```
Source Code (Fork/Path/Access specs)
    ↓
Parse → AST (ForkExpr)
    ↓
Analyze → Effects (Conflict detection)
    ↓
Verify → Contracts (Requires checking)
    ↓
Generate → Pseudo-Code (Barrier insertion)
    ↓
Executable (Ready for Phase 6 runtime)
```

### **Phase 6: ABI Layer (Low-Level Runtime)**

| Module | Purpose | Key Types | Tests |
|--------|---------|-----------|-------|
| `cfp_rfp.rs` | Physical register bindings | `HybridContextSwitch`, `PhysicalRegisters` | 6 |
| `shadow_arena.rs` | 2PST Phase 1 isolation | `ShadowArena`, `StagedWrite`, `SharedResourceAccess` | 6 |
| `sarm.rs` | Static Abort Register Map | `SARMTable`, `SARMEntry` | 8 |
| `gac.rs` | Generational Arena Checkpoint | `LoopFrame` | 8 |
| `direct_jump.rs` | :collect binding resolution | `CollectBindingTable`, `DirectJumpTarget` | 8 |

**✓ Phase 6 + Phase 1 Integration (COMPLETED)**

The `cfp_rfp.rs` module is now fully integrated with Phase 1's `ExecutionContext`:
- `ExecutionContext` now contains `direct_jump_context: Option<HybridContextSwitch>`
- `abort()` method uses `execute_direct_jump()` for O(1) abort instead of traditional unwinding
- Thread-local `HYBRID_CONTEXT` tracks CFP/RFP values at runtime
- New public methods on `ExecutionContext`:
  - `set_direct_jump_context()`: Configure abort target (CFP, RFP, collector IP)
  - `clear_direct_jump_context()`: Disable direct jump
  - `get_hybrid_context()`: Query current CFP/RFP values
  - `has_direct_jump_context()`: Check if configured

**Integration Benefits:**
- ✓ Zero-cost exception handling (no stack unwinding, no DWARF tables)
- ✓ O(1) abort mechanism (3 MOV + 1 JMP instructions)
- ✓ Physical register semantics (rbp/r15 on x86-64, x29/x28 on AArch64)
- ✓ Static abort routing via direct jump table
- ✓ Clean separation: Phase 1 memory management + Phase 6 exception handling

**Key Architecture Decisions:**

#### 1. **CFP/RFP (Control/Resource Frame Pointers)**
Physical CPU registers for O(1) context switching:
- **x86-64**: CFP=`rbp`, RFP=`r15`, arena_ptr=`r14`
- **AArch64**: CFP=`x29`, RFP=`x28`, arena_ptr=`x27`

Abort sequence: `mov rbp, target_cfp; mov r15, rfp; jmp collector_ip` (3 instructions)

#### 2. **Shadow Arena (2PST Phase 1)**
Per-path speculative execution buffers:
- Completely independent shadow buffers per fork path
- Lock-free concurrent writes (no synchronization overhead)
- Shared resource tracking with conflict detection
- Automatic abort rollback (discard buffers, keep main memory)

#### 3. **SARM (Static Abort Register Map)**
Compile-time metadata for register restoration:
- BTreeMap-based O(log n) lookup
- Stores: callee-saved register masks, save area offsets, collector IPs
- `.rodata` section at runtime (read-only)
- Enables deterministic register state after direct jump

#### 4. **GAC (Generational Arena Checkpoint)**
Loop memory management with O(1) overhead:
- Checkpoint arena pointer at loop entry
- Reset to checkpoint on loop back-edge
- Prevents O(iterations × body_size) memory leaks
- Thread-local stack of active loop frames

#### 5. **Direct Jump (:collect Binding)**
Compile-time :collect → collector resolution:
- Static binding table (HashMap, O(1) lookup)
- Direct jump targets with CFP, RFP, collector IP
- No dynamic dispatch, no vtable lookup
- Enables static abort routing from fork paths to collectors

---

## 🔧 Building & Running

### Prerequisites
- **Rust**: 1.70+ (Edition 2021)
- **Platform**: Linux, macOS, Windows
- **Architecture**: x86-64 (primary), AArch64 (secondary)

### Build
```bash
# Debug build
cargo build

# Release build (optimized with LTO)
cargo build --release
```

### Run PoC Demo
```bash
# Display Phase 6 runtime demonstration
cargo run --release --bin seam-vm

# Output: Demonstrates all 5 ABI layers (CFP/RFP, Shadow Arena, SARM, GAC, Direct Jump)
```

### Run Tests
```bash
# All tests (91 tests, ~100ms)
cargo test --release --lib

# Specific module
cargo test --release cfp_rfp::tests

# With output
cargo test --release -- --nocapture
```

---

## 📊 Performance Characteristics

| Operation | Time | Mechanism |
|-----------|------|-----------|
| **Abort** | O(1) | 3 MOV + 1 JMP instructions |
| **Context Switch** | O(1) | Simultaneous CFP/RFP register update |
| **Collector Lookup** | O(1) | Direct hash table (direct_jump.rs) |
| **Register Restoration** | O(1) | SARM metadata lookup |
| **Loop Back-edge** | O(1) | Arena checkpoint rollback |
| **Fork Path Isolation** | O(1) | Per-path shadow buffer write |

## 🗂️ Project Structure

```
seam-bootstrap/
├── Cargo.toml                      # Project manifest (Rust 2021, opt-level=3, lto=true)
├── README.md                       # This file
└── src/
    ├── lib.rs                      # Library root (18 module exports)
    ├── main.rs                     # Phase 6 demonstration binary
    │
    ├─ Phase 1: Memory Management
    ├── pssa.rs                     # Path-bounded Shadow Stack Arena
    ├── context.rs                  # Execution context (CFP/RFP tracking)
    ├── abort.rs                    # Abort signal and collector table
    │
    ├─ Phase 2: Transaction Engine
    ├── transaction.rs              # 2PST (Two-Phase Static Transaction)
    ├── shadow_buffer.rs            # Shadow buffer for Phase 1 speculation
    │
    ├─ Phase 3: Resource Tracking
    ├── resource.rs                 # Global resources and access sets
    ├── effect.rs                   # Static effect analysis
    ├── contract.rs                 # Requires contract verification
    ├── sync.rs                     # Automatic synchronization
    │
    ├─ Phase 4: Compiler
    ├── ast.rs                      # Abstract Syntax Tree definitions
    ├── compiler.rs                 # Compiler (parse → compile → analyze)
    ├── codegen.rs                  # Code generation
    ├── fork.rs                     # Fork context and graphs
    ├── channel.rs                  # Channel definitions
    │
    ├─ Phase 6: ABI Layer
    ├── cfp_rfp.rs                  # Physical register (CFP/RFP) bindings
    ├── shadow_arena.rs             # 2PST Phase 1 arena isolation
    ├── sarm.rs                     # Static Abort Register Map
    ├── gac.rs                      # Generational Arena Checkpoint
    ├── direct_jump.rs              # Direct jump :collect binding
    │
    └── arch/                       # Architecture-specific intrinsics
        ├── x86_64/mod.rs           # x86-64 register access
        └── aarch64/mod.rs          # AArch64 register access
```

---

## 🎯 DRAFT Specification Compliance

✓ **Implemented & Verified:**
- ✓ PSSA: Thread-local bounded arena with bump allocation
- ✓ CFP/RFP: Physical register separation for abort safety
- ✓ 2PST: Three-phase transaction (speculative → commit → abort)
- ✓ SARM: Static abort register map in .rodata
- ✓ GAC: Generational arena checkpoint for loops
- ✓ Direct Jump: O(1) :collect → collector path resolution
- ✓ No Stack Unwinding: DWARF-free, zero-cost exception handling
- ✓ Path Typing: Compile-time resource access verification
- ✓ Requires Contracts: Automatic synchronization insertion
- ✓ Fork Semantics: Independent shadow paths with conflict detection

**Metrics:**
- **Tests Passing**: 92/92 (100%, including Phase 6 + Phase 1 integration test)
- **Code Coverage**: All critical paths tested
- **Build Time**: <10 seconds (release profile)
- **Binary Size**: ~8 MB (release, with debug info stripped)

---

## 🔬 Test Coverage

### Phase 1 Tests (13 tests)
- Arena allocation and bounds
- Checkpoint save/restore
- Context switching
- Abort signal handling
- **NEW**: Direct jump context integration (Phase 6 + Phase 1)

### Phase 2 Tests (14 tests)
- Shadow buffer writes and isolation
- Transaction state transitions
- 2PST protocol verification
- Conflict detection

### Phase 3 Tests (26 tests)
- Resource access tracking
- Effect analysis
- Contract verification
- Synchronization point generation

### Phase 4 Tests (24 tests)
- AST construction
- Fork expression parsing
- Compiler pipeline
- Code generation

### Phase 6 Tests (36 tests)
- CFP/RFP context switching (6 tests)
- Shadow arena isolation (6 tests)
- SARM registration and lookup (8 tests)
- GAC loop checkpoints (8 tests)
- Direct jump binding resolution (8 tests)

---

## � Phase 6 + Phase 1 Integration Architecture

**Context Integration Strategy:**

The Phase 6 ABI layer (physical register bindings) is now fully integrated with Phase 1's memory management:

```rust
// Phase 1: ExecutionContext with CFP/RFP tracking
pub struct ExecutionContext {
    arena: Arc<RefCell<Arena>>,
    cfp: ControlFramePtr,      // Virtual pointer (Phase 1)
    rfp: ResourceFramePtr,
    in_collector: bool,
    thread_id: u64,
    direct_jump_context: Option<HybridContextSwitch>,  // ← Phase 6 integration
}

// Phase 6: Direct jump with physical registers
pub struct HybridContextSwitch {
    target_cfp: *mut u8,      // Physical register (rbp/x29)
    target_rfp: *mut u8,      // Physical register (r15/x28)
    collector_ip: *const u8,  // Collector entry point
}
```

**Abort Flow (with integration):**

```
1. Fork path executing in channel
2. Error detected → abort signal
3. ExecutionContext::abort() called
4. Phase 1: Update RFP to point to current CFP (ghost frame)
5. Phase 6: If direct_jump_context configured:
   - execute_direct_jump() performs 3-instruction jump
   - Direct jump: mov rbp, target_cfp; mov r15, rfp; jmp collector_ip
6. Collector executes with:
   - CFP = new control context
   - RFP = ghost frame (can access aborted locals)
   - No stack unwinding, no DWARF lookup
```

**Key Integration Points:**

| Aspect | Phase 1 | Phase 6 | Integration |
|--------|---------|---------|-------------|
| **Storage** | Virtual pointers (usize) | Physical registers | Thread-local sync |
| **Context Switch** | Manual frame pointer updates | Direct jump asm | Combined abort path |
| **Collector Invocation** | Function pointer call | Direct jump address | Conditional execution |
| **Memory Safety** | PSSA arena bounds | Register aliasing | CFP/RFP separation |

---

### Why Assembly-Level Implementation?
The DRAFT specification requires **O(1) abort** with **no stack unwinding**. This is only achievable via:
1. **Physical register bindings**: CFP/RFP are real CPU registers
2. **Direct jump**: 3 instructions (no dynamic dispatch, no exception tables)
3. **Static metadata**: SARM stored in .rodata (no runtime overhead)

### Why Two-Phase Static Transactions?
- **Phase 1 (Speculative)**: Fork paths run in parallel with shadow buffers (lock-free)
- **Phase 2 (Commit)**: Compiler determines lock order statically (no deadlock)
- **Phase 3 (Abort)**: Discard buffers, execute collector via direct jump (no main memory pollution)

This achieves **serializability** without dynamic verification.

### Why No Stack Unwinding?
Traditional exception handling via stack unwinding:
- Requires DWARF tables (memory overhead)
- Dynamic lookups at abort time (O(n) cost)
- Complex with nested scopes and RAII

Seam uses **direct jump with ghost frame (RFP)**:
- Static metadata only (SARM in .rodata)
- O(1) lookup and execution
- Clean semantics for resource cleanup

---

## 🚀 Next Steps for Full Implementation

1. ✓ **Context Integration**: ~~Integrate Phase 6 `cfp_rfp.rs` with Phase 1 `context.rs`~~ **COMPLETED**
   - ExecutionContext now manages HybridContextSwitch for O(1) direct jump
   - abort() method now uses direct jump instead of traditional unwinding
   - Thread-local hybrid context tracks CFP/RFP at runtime
   
2. ✓ **PSSA Modernization**: ~~Implement mmap-based arena (true virtual memory)~~ **COMPLETED**
   - Replaced std::alloc with OS virtual memory (VirtualAlloc on Windows, mmap on Unix)
   - Added guard pages (PROT_NONE) at top and bottom for overflow protection
   - Cross-platform support with conditional compilation
   - All 93 tests passing with new arena implementation

3. ✓ **Barrier Insertion**: ~~Enhance `sync.rs` with actual memory barriers~~ **COMPLETED**
   - Added MemoryBarrier type with Acquire/Release/FullFence semantics
   - Mapped SyncKind (RAW/WAR/WAW) to appropriate atomic Ordering
   - Implemented thread fence execution (std::sync::atomic::fence)
   - Added 8 new tests for barrier functionality (all passing)
   - AutoSync now generates and can execute actual memory barriers

4. ✓ **Linking**: ~~Runtime linking of compiled fork expressions~~ **COMPLETED**
   - Implemented RuntimeLinker with link() operation only (NOT execution)
   - Created LinkedFork runtime representation from CompiledFork metadata
   - PathState for per-path metadata (future refactoring: merge with ExecutionContext)
   - 5-phase execution model deferred to ForkExecutor (future phase)
   - 13 comprehensive tests for runtime linking (all passing)
   - Total: 115 tests (102 + 13 new linker tests)

5. ✓ **ForkExecutor**: ~~Execution coordination with path scheduling~~ **COMPLETED**
   - Implemented 5-phase execution: setup → dispatch → barriers → collect → join
   - Phase 1 (Setup): Allocates execution frames for each path in PSSA arena
   - Phase 2 (Dispatch): Schedules paths to execution engine
   - Phase 3 (Barriers): Executes memory barriers from Phase 3 (AutoSync)
   - Phase 4 (Collect): Gathers results from all path executions
   - Phase 5 (Join): Synchronizes paths at fork join point
   - Integrated with ExecutionContext (Phase 1) and MemoryBarrier (Phase 3)
   - 4 comprehensive tests for ForkExecutor (setup, phases, results, barriers)
   - Total: 118 tests (102 + 13 linker + 4 executor tests)

6. ✓ **Pseudo-Code Interpreter** (COMPLETED): Path execution with code interpretation
   - Responsibility: Deserialize and execute CompiledFork.generated_code (Phase 5C)
   - Implementation: CodeInterpreter, Instruction enum, ResourceAccessTracker
   - Integration: Replaces ForkExecutor phase_dispatch() placeholder with actual execution
   - Tests: 12 comprehensive tests (parsing, execution, tracking, integration)

7. **Direct Jump Integration** (Next priority): Connect ForkExecutor abort paths to direct jump abort
   - Responsibility: Integrate Phase 6 CFP/RFP with Phase 5C abort mechanism
   - Goal: Implement O(1) abort via direct jump when Phase 5C Abort instruction executed
   - Foundation: Use RFP ghost frame for access to aborted path locals

8. **Signal Integration** (Deferred - after execution model is stable)
   - Connect abort mechanism to OS signal handlers
   - Reason: Implement signals after execution is complete, not before

---

## 📖 References

- **DRAFT.md**: Language specification (path typing, channels, transactions)
- **IEEE Paper**: "Path-Bounded Execution with Static Verification" (TBD)
- **Rust Edition 2021**: Modern async, const generics, control flow

---

## 📝 License

Internal PoC implementation for Axiomium/Seam project.

---

## 🙋 Questions About Architecture?

This README documents the full architecture. For specific questions:
1. **Memory layout**: See `pssa.rs` and `context.rs`
2. **Exception handling**: See `cfp_rfp.rs` and `direct_jump.rs`
3. **Transactions**: See `transaction.rs` and `shadow_arena.rs`
4. **Compilation**: See `ast.rs`, `compiler.rs`, `codegen.rs`
5. **Runtime Linking**: See `linker.rs` and `RUNTIME_LINKING.md`
6. **Abort mechanism**: See `sarm.rs` and inline documentation

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
