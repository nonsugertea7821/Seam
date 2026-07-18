# Phase 5C: Pseudo-Code Interpreter

## Overview

**Phase 5C** implements the **Pseudo-Code Interpreter**, responsible for:

1. **Deserialize** pseudo-code strings from `CompiledFork.generated_code`
2. **Parse** into structured `Instruction` enum
3. **Execute** instructions sequentially per path
4. **Track** resource access (read/write operations)
5. **Generate** `PathResult` with execution status

Integration point with **Phase 5B (ForkExecutor)**: Replaces placeholder `phase_dispatch()` with actual code execution.

---

## Architecture

```
CompiledFork
    └─ generated_code: String ─────────┐
                                       │
                              CodeInterpreter
                                 ├─ parse()     ──→ Vec<Instruction>
                                 └─ execute()   ──→ PathResult
                                       │
                                       ↓
LinkedFork ────────────────────→ ForkExecutor
                                  (phase_dispatch)
                                       │
                                       ↓
                              PathResult (collected)
                                       │
                                       ↓
                            ForkExecutionResult
```

---

## Key Types

### `Instruction`

Parsed pseudo-code instruction:

```rust
pub enum Instruction {
    ReadResource(u32),      // Read from resource ID
    WriteResource(u32),     // Write to resource ID
    ReadWriteResource(u32), // Read-write to resource ID
    Barrier,                // Memory barrier (fence)
    Success,                // Mark path as successful
    Abort,                  // Abort execution (stop)
}
```

### `CodeInterpreter`

Stateless interpreter for parsing and executing pseudo-code:

```rust
pub struct CodeInterpreter;

impl CodeInterpreter {
    /// Parse pseudo-code string into instructions
    pub fn parse(code: &str) -> Vec<Instruction>;
    
    /// Execute parsed instructions for a single path
    pub fn execute(
        path_id: u32,
        resource_id: ResourceId,
        instructions: &[Instruction],
    ) -> PathResult;
}
```

**Methods:**

- `parse(code: &str) -> Vec<Instruction>`
  - Input: Pseudo-code string (simple format)
  - Returns: Structured instruction vector
  - Skips empty lines and comments (`//`)
  - Gracefully handles malformed instructions

- `execute(path_id, resource_id, instructions) -> PathResult`
  - Executes instructions sequentially
  - Stops on `Abort` instruction
  - Records `Success` state from explicit `Success` instruction
  - Executes memory fences for `Barrier` instructions
  - Returns status-only `PathResult`

### `ResourceAccessTracker`

Tracks resource accesses during path execution:

```rust
pub struct ResourceAccessTracker {
    pub path_id: u32,
    pub reads: Vec<u32>,
    pub writes: Vec<u32>,
}
```

**Methods:**

- `new(path_id: u32) -> Self` — Create tracker for path
- `record_read(resource_id: u32)` — Record read access
- `record_write(resource_id: u32)` — Record write access
- `total_accesses() -> usize` — Get total read + write count

---

## Pseudo-Code Format

Simple text-based instruction set (one instruction per line):

### Instruction Syntax

| Instruction | Format | Example | Effect |
|-------------|--------|---------|--------|
| Read | `read <id>` | `read 1` | Track read access to resource 1 |
| Write | `write <id>` | `write 2` | Track write access to resource 2 |
| Read-Write | `readwrite <id>` | `readwrite 3` | Track read-write to resource 3 |
| Barrier | `barrier` | `barrier` | Execute memory fence (atomic::fence) |
| Success | `success` | `success` | Mark path as successful |
| Abort | `abort` | `abort` | Stop execution (abort this path) |

### Example Pseudo-Code

```
// Path 0: Read-modify-write to resource 1
read 1
write 1
barrier
success

// Path 1: Read resource 2, abort if conflict
read 2
abort

// Path 2: Multiple resources
read 1
write 2
barrier
success
```

### Parsing Rules

1. Empty lines: Ignored
2. Comment lines (start with `//`): Ignored
3. Whitespace: Trimmed and normalized
4. Unknown instructions: Skipped
5. Invalid resource IDs: Skipped

---

## Execution Model (5 Phases + 5C Integration)

### ForkExecutor Execution Flow

```
Phase 1: Setup
    └─ Allocate frames for each path in PSSA arena

Phase 2: Dispatch (NOW USES PHASE 5C)
    ├─ For each path:
    │  ├─ Deserialize generated_code from LinkedFork
    │  ├─ Parse into instructions (CodeInterpreter::parse)
    │  ├─ Execute sequentially (CodeInterpreter::execute)
    │  └─ Collect PathResult
    │
    └─ All path results stored in ExecutionState

Phase 3: Barriers
    └─ Execute MemoryBarrier instances from Phase 3 (AutoSync)

Phase 4: Collect
    └─ Aggregate results from all paths

Phase 5: Join
    └─ Synchronize at join point, verify completion
```

### Phase 5C Integration with ForkExecutor

```rust
fn phase_dispatch(&mut self, _context: &mut ExecutionContext) -> Result<(), String> {
    // Phase 5C: Execute pseudo-code for each path
    for path_state in self.linked.path_states() {
        // 1. Deserialize pseudo-code from LinkedFork
        let instructions = if let Some(ref code) = self.linked.generated_code {
            CodeInterpreter::parse(code)  // ← Parse pseudo-code
        } else {
            Vec::new()
        };

        // 2. Execute instructions
        let result = CodeInterpreter::execute(
            path_state.path_id(),
            path_state.resource_id(),
            &instructions,
        );  // ← Execute and get PathResult

        // 3. Store result for Phase 4 (Collect)
        self.execution_state.path_results.push(result);
    }
    
    self.execution_state.dispatch_done = true;
    Ok(())
}
```

---

## Example Usage

### 1. Parse Pseudo-Code

```rust
use seam_bootstrap::{CodeInterpreter, Instruction};

let code = r#"
    read 1
    write 1
    barrier
    success
"#;

let instructions = CodeInterpreter::parse(code);
assert_eq!(instructions.len(), 4);
assert_eq!(instructions[0], Instruction::ReadResource(1));
assert_eq!(instructions[3], Instruction::Success);
```

### 2. Execute Path

```rust
use seam_bootstrap::{CodeInterpreter, ResourceId};

let path_id = 0;
let resource_id = ResourceId::new(1);
let instructions = vec![
    Instruction::ReadResource(1),
    Instruction::WriteResource(1),
    Instruction::Success,
];

let result = CodeInterpreter::execute(path_id, resource_id, &instructions);
assert!(result.success);
assert_eq!(result.path_id, 0);
```

### 3. Track Resource Access

```rust
use seam_bootstrap::ResourceAccessTracker;

let mut tracker = ResourceAccessTracker::new(0);
tracker.record_read(1);
tracker.record_write(2);
tracker.record_read(1);  // Duplicate, not recorded

assert_eq!(tracker.reads.len(), 1);
assert_eq!(tracker.writes.len(), 1);
assert_eq!(tracker.total_accesses(), 2);
```

### 4. Full Fork Execution with Phase 5C

```rust
use seam_bootstrap::{
    CompiledFork, RuntimeLinker, AccessType, ResourceId,
    ExecutionContext, ForkExecutor
};

// Create compiled fork
let mut compiled = CompiledFork::new(1, 1);
compiled.add_access(0, ResourceId::new(1), AccessType::Read);

// Link to runtime representation
let mut linked = RuntimeLinker::link(&compiled);

// Add pseudo-code
let code = "read 1\nsuccess".to_string();
linked.set_generated_code(code);

// Create executor and execute
let mut executor = ForkExecutor::new(linked);
let mut ctx = ExecutionContext::new(1024)?;
let result = executor.execute(&mut ctx)?;

assert!(result.is_success());
```

---

## Memory Barriers

The `Barrier` instruction maps to atomic memory fence:

```rust
Instruction::Barrier => {
    std::sync::atomic::fence(std::sync::atomic::Ordering::Release);
}
```

**Semantics:**
- `Release` fence: Prevent prior loads/stores from reordering after
- Synchronized with Phase 3 (AutoSync) barrier generation
- Enables cross-path synchronization for path conflicts

**Integration:** Barriers execute in Phase 3, not during Phase 5C dispatch.

---

## Test Coverage

### Parsing Tests (7 tests)
- ✅ `test_code_interpreter_parse_read_instruction` — Single read
- ✅ `test_code_interpreter_parse_write_instruction` — Single write
- ✅ `test_code_interpreter_parse_readwrite_instruction` — Read-write
- ✅ `test_code_interpreter_parse_multiple_instructions` — Multiple ops
- ✅ `test_code_interpreter_parse_with_comments` — Comment handling
- ✅ `test_code_interpreter_parse_empty_lines` — Empty line handling
- ✅ `test_code_interpreter_parse_barrier` — Barrier instruction

### Execution Tests (5 tests)
- ✅ `test_code_interpreter_execute_success_path` — Successful execution
- ✅ `test_code_interpreter_execute_abort_path` — Abort stops execution
- ✅ `test_code_interpreter_execute_barrier` — Barrier executes
- ✅ `test_fork_executor_with_pseudo_code` — Integration test

### Tracking Tests (1 test)
- ✅ `test_code_interpreter_resource_access_tracker` — Access tracking

**Total Tests:** 12 (all passing)

---

## Integration Points

### Phase 1: ExecutionContext
- `frame_push(256)` allocates memory for path execution
- Called in ForkExecutor Phase 1 (Setup)

### Phase 3: MemoryBarrier
- `Barrier` instruction executes fences
- Integrated with AutoSync synchronization points
- Executed in ForkExecutor Phase 3

### Phase 4: CompiledFork
- Source of `generated_code` string
- Linked into LinkedFork during Phase 5 linking
- Deserialized by CodeInterpreter in Phase 5C

### Phase 5B: ForkExecutor
- Phase 2 (Dispatch) now uses CodeInterpreter::execute()
- PathResult collected for Phase 4 (Collect)
- Path success/failure determined by execution

---

## Design Decisions

### 1. Stateless CodeInterpreter

**Decision:** `CodeInterpreter` is a zero-state struct (no fields, all methods are static).

**Rationale:**
- No shared state needed for interpretation
- Thread-safe by default
- Each path execution is independent
- Easier to parallelize in future (thread pool)

### 2. Simple String-Based Pseudo-Code

**Decision:** Parse human-readable text format (one instruction per line).

**Rationale:**
- Easy to debug and inspect
- Humans can read generated code
- Compiler can generate directly (no serialization needed)
- Extensible for new instruction types

### 3. Separate ResourceAccessTracker

**Decision:** Tracking is opt-in, not automatic in CodeInterpreter.

**Rationale:**
- Future enhancement: Track accesses during execution
- Currently: Accesses come from requires contracts (compile time)
- Can be integrated into PathResult later
- Maintains separation between parsing and tracking

### 4. Abort Stops Execution Immediately

**Decision:** `Abort` instruction stops loop, doesn't mark failure.

**Rationale:**
- Abort is special case (exit condition, not error)
- Success marker is explicit (not default)
- Future: Will integrate with Phase 6 direct jump
- Allows collector invocation for cleanup

### 5. Memory Fence for Barrier

**Decision:** Execute `std::sync::atomic::fence(Release)` for `Barrier`.

**Rationale:**
- Release fence prevents write reordering before subsequent reads
- Paired with Phase 3 AutoSync point generation
- O(1) cost (no active wait)
- Synchronized execution across paths

---

## Future Enhancements

### 1. Real Register State Tracking

Current: Pseudo-code execution is abstract (no registers updated)
Future: Track CFP/RFP values during path execution for real abort mechanism

### 2. Conditional Execution

Current: Linear instruction sequence
Future: Add `if/branch` instructions for conditional paths

### 3. Resource State Recording

Current: Accesses not recorded (comes from compile-time metadata)
Future: RuntimeResourceAccess tracking during execution

### 4. Multi-Path Synchronization

Current: Barriers execute in Phase 3 (before dispatch)
Future: In-path barriers for cross-path coordination

### 5. Thread Pool Dispatch

Current: Sequential execution in phase_dispatch()
Future: Parallel execution with work-stealing scheduler

---

## Debugging Tips

### 1. Inspect Parsed Instructions

```rust
let code = "read 1\nwrite 1\nsuccess";
let instructions = CodeInterpreter::parse(code);
for (i, instr) in instructions.iter().enumerate() {
    println!("Instruction {}: {:?}", i, instr);
}
```

### 2. Trace Execution

```rust
let result = CodeInterpreter::execute(0, ResourceId::new(1), &instructions);
println!("Path {} result: success={}", result.path_id, result.success);
```

### 3. Verify Resource Mapping

```rust
// Check that generated_code is linked
assert!(linked.generated_code.is_some());
println!("Generated code:\n{}", linked.generated_code.unwrap());
```

---

## Summary

**Phase 5C (Pseudo-Code Interpreter)** bridges **Phase 4 (Code Generation)** and **Phase 5B (ForkExecutor)**:

- Deserializes human-readable pseudo-code into structured instructions
- Executes sequentially per path with O(n) cost (n = instruction count)
- Integrates with Phase 1 (frames), Phase 3 (barriers), Phase 5B (dispatch)
- Foundation for future execution enhancements (real registers, threading, conditionals)
- Fully tested (12 comprehensive tests, all passing)

**Next Priority:** Phase 6 integration (direct jump abort mechanism with Phase 5C).
