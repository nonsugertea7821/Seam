# Design Policy Correction: Phase 5 Runtime Linker

## Executive Summary

**Architectural Review Result**: Phase 5 design repositioned from "Runtime Executor" to "Linking Layer Only"

**Status**: ✅ COMPLETED - Documentation updated, tests passing, design principles clarified

**Date**: 2026-07-18

---

## Feedback Analysis

### Architecture Review Findings

#### Positive Aspects ✅
1. **Three-layer architecture is natural**
   - CompiledFork (Static IR)
   - LinkedFork (Executable representation)
   - ExecutionContext (CPU state)
   - Pattern matches LLVM: IR → MachineFunction → Machine Code

2. **Resource access determinism**
   - Correctly placed at link time (not runtime)
   - BTreeMap ensures reproducible ordering
   - Metadata preparation before execution

3. **Barrier integration**
   - Natural to include in linking phase
   - MemoryBarrier specification at compile time
   - Execution deferred to runtime

#### Areas for Improvement 🔄

1. **PathExecutor responsibility too broad**
   - Currently: resource + result + execute
   - Competes with ExecutionContext
   - Solution: Reduce to PathState metadata only

2. **PathResult carries data**
   - Causes Resource duplication
   - data: Vec<u8> conflicts with ResourceFrame
   - Solution: Status-only (success/failure/abort/dispatch)

3. **RuntimeLinker does execution**
   - Linker ≠ Executor
   - Violates single responsibility
   - Solution: link() only, defer execute() to ForkExecutor

4. **Signal Integration timing**
   - Premature before execution is stable
   - execute_path() is still placeholder
   - Solution: Pseudo-code Interpreter first

---

## Design Corrections

### 1. Phase 5 Scope: Linking Only

**Before**:
```rust
impl RuntimeLinker {
    pub fn link() → LinkedFork
    pub fn execute() → ForkExecutionResult  // ❌ Violates responsibility
    pub fn link_and_execute() → ForkExecutionResult  // ❌ Convenience trap
}
```

**After**:
```rust
impl RuntimeLinker {
    pub fn link() → LinkedFork  // ✅ Single responsibility
}

// Future: ForkExecutor handles execution
impl ForkExecutor {
    pub fn execute(&mut self) → Result<ForkExecutionResult>
}
```

**Rationale**: Linker prepares, Executor acts. Clear separation enables independent testing.

---

### 2. PathResult: Status Only

**Before**:
```rust
pub struct PathResult {
    path_id: u32,
    resource_id: ResourceId,
    success: bool,
    data: Option<Vec<u8>>,  // ❌ Causes Resource duplication
}
```

**After**:
```rust
pub struct PathResult {
    path_id: u32,
    resource_id: ResourceId,
    success: bool,
    // Status only: success, failure, abort, dispatch
    // Data lives in ResourceFrame, not here
}
```

**Rationale**: Avoid two-place state management. Resource is the authority.

---

### 3. PathExecutor → PathState

**Before**:
```rust
pub struct PathExecutor {
    path_id: u32,
    resource_id: ResourceId,
    access_type: AccessType,
    results: Arc<Mutex<Vec<PathResult>>>,  // ❌ Execution responsibility
}
```

**After** (Future refactoring):
```rust
pub struct PathState {
    path_id: u32,
    resource_id: ResourceId,
    access_type: AccessType,
    // No results - metadata only
}

// Integration into ExecutionContext
pub struct ExecutionContext {
    path_states: Vec<PathState>,  // Multiple paths tracked
    // ... existing fields
}
```

**Rationale**: ExecutionContext already manages paths. No need for separate concept.

---

### 4. Roadmap Reordering

**Before**:
```
5. Linking ✓
6. Signal Integration
7. Optimization
```

**After**:
```
5. Linking ✓
5B. ForkExecutor (NEW) - Execution coordination
5C. Pseudo-code Interpreter (NEW) - Path execution
6. Direct Jump Integration
6+ Signal Integration (DEFERRED)
7. Optimization
```

**Rationale**: Build execution foundation before signal integration. Signals integrate cleanly after runtime is stable.

---

## Implementation Path Forward

### Immediate (Phase 5B: ForkExecutor)
```rust
pub struct ForkExecutor {
    linked: LinkedFork,
    context: ExecutionContext,
}

impl ForkExecutor {
    pub fn new(linked: LinkedFork) → Self

    pub fn execute(&mut self) → Result<ForkExecutionResult>
        // Phase 1: Setup
        // Phase 2: Dispatch (via Scheduler)
        // Phase 3: Execute barriers
        // Phase 4: Collect results
        // Phase 5: Join
}
```

**Deliverables**:
- Execution coordination logic
- 5-phase execution model
- Integration with ExecutionContext.path_states[]
- Result aggregation

### Next (Phase 5C: Pseudo-code Interpreter)
```rust
pub struct CodeInterpreter {
    pseudo_code: &str,  // From CompiledFork.generated_code
}

impl CodeInterpreter {
    pub fn interpret(&self, executor: &PathExecutor) → Result<PathResult>
        // Replace execute_path() placeholder
        // Deserialize and execute pseudo-code
        // Capture resource accesses
}
```

**Deliverables**:
- Pseudo-code deserialization
- Actual path execution (not placeholder)
- Resource state tracking

### Later (Phase 6: Direct Jump Integration)
- Connect ForkExecutor abort paths to direct_jump_context
- Use existing SARM infrastructure
- Collector invocation via direct jump

### Final (Phase 6+: Signal Integration)
- After execution is stable
- Connect abort handler to OS signals
- Use SARM for register restoration

---

## Current Implementation Status

### What Stays ✅
- `linker.rs` module (link phase complete)
- LinkedFork structure and linking logic
- MemoryBarrier integration
- All 115 tests passing

### What Needs Future Work 🔄
- Remove `execute()` from RuntimeLinker (move to ForkExecutor)
- Remove `link_and_execute()` convenience API
- Replace PathExecutor with PathState (merge into ExecutionContext)
- Remove `data` field from PathResult
- Implement actual pseudo-code interpretation

### Backward Compatibility ✅
- Current tests remain valid
- API additions only (no removals yet)
- Future refactoring fully scoped

---

## LLVM Pattern Application

Seam VM architecture now clearly mirrors LLVM compilation pipeline:

```
┌──────────────────────────────┐
│ Source (Seam Language)       │
└──────────────────────────────┘
           ↓
        Parse
           ↓
┌──────────────────────────────┐
│ IR (Abstract Fork Syntax)    │
└──────────────────────────────┘
           ↓ Phase 4 Compiler
┌──────────────────────────────┐
│ CompiledFork (IR with effects)
└──────────────────────────────┘
           ↓ Phase 5 Linker
┌──────────────────────────────┐
│ LinkedFork (Machine function) │
└──────────────────────────────┘
           ↓ Phase 5B Executor
┌──────────────────────────────┐
│ ExecutionContext (Machine)    │
└──────────────────────────────┘
           ↓
      CPU Execution
```

Each layer has **single responsibility**:
- **Compiler**: IR generation, effect analysis
- **Linker**: Metadata preparation, barrier setup
- **Executor**: Path coordination, result collection
- **CPU**: Actual instruction execution

---

## Key Takeaways

1. **Separation of concerns is critical**
   - Link time ≠ runtime
   - Metadata ≠ state
   - Preparation ≠ execution

2. **LLVM pattern generalizes**
   - Source → IR → MachineFunction → Machine Code
   - Maps to Seam: Fork → CompiledFork → LinkedFork → ExecutionContext

3. **Deferred decisions are better**
   - Signal integration after execution foundation
   - Pseudo-code interpretation before signals
   - Each phase independent

4. **Status-only results avoid bugs**
   - Single source of truth for state
   - PathResult: status carrier, not data holder
   - ResourceFrame: state authority

---

## Conclusion

Phase 5 design revision repositions RuntimeLinker from "execution coordinator" to "linking layer". This maintains clean architecture and enables future phases to integrate smoothly.

**Immediate benefit**: Clearer responsibility, easier future maintenance

**Long-term benefit**: Foundation for stable, extensible runtime system

**Next step**: ForkExecutor (Phase 5B) - Execution coordination

