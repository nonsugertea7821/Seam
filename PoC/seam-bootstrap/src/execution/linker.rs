//! Phase 5: Runtime Linker - Linking Compiled Forks (NOT Execution)
//!
//! This module implements the runtime linker that converts CompiledFork metadata
//! into executable LinkedFork representations. Linking is SEPARATE from execution.
//!
//! Responsibilities (THIS PHASE):
//! 1. Link CompiledFork → LinkedFork (metadata preparation)
//! 2. Extract resource access metadata
//! 3. Prepare MemoryBarrier information
//!
//! Responsibilities (DEFERRED to ForkExecutor):
//! 1. Execute paths (placeholder: execute_path())
//! 2. Collect results
//! 3. Coordinate fork/join lifecycle

use crate::ast::{CompiledFork, ResourceId, AccessType};
use crate::context::ExecutionContext;
use crate::sync::MemoryBarrier;
use std::collections::BTreeMap;

#[path = "linker/phases.rs"]
mod phases;
#[path = "linker/interpreter_bridge.rs"]
mod interpreter_bridge;
#[path = "linker/result_aggregation.rs"]
mod result_aggregation;

pub use interpreter_bridge::{CodeInterpreter, Instruction, ResourceAccessTracker};

/// Result of a path execution (Status only, NOT data carrier)
#[derive(Debug, Clone)]
pub struct PathResult {
    pub path_id: u32,
    pub resource_id: ResourceId,
    pub success: bool,
    pub aborted: bool,  // Phase 7: Track abort status for direct jump
    // NOTE: data field removed - state belongs in ResourceFrame, not here
}

impl PathResult {
    pub fn new(path_id: u32, resource_id: ResourceId) -> Self {
        PathResult {
            path_id,
            resource_id,
            success: false,
            aborted: false,
        }
    }

    pub fn success(mut self) -> Self {
        self.success = true;
        self
    }

    pub fn abort(mut self) -> Self {
        self.aborted = true;
        self.success = false;  // Abort is not success
        self
    }
}

/// Phase 7: Abort target for direct jump to collector
#[derive(Debug, Clone)]
pub struct AbortTarget {
    pub target_cfp: *mut u8,      // Control Frame Pointer
    pub target_rfp: *mut u8,      // Resource Frame Pointer (ghost frame)
    pub collector_ip: *const u8,  // Collector entry point
    pub collector_channel_id: u32, // Collector channel identity for parent resolution
}

impl AbortTarget {
    pub fn new(
        target_cfp: *mut u8,
        target_rfp: *mut u8,
        collector_ip: *const u8,
        collector_channel_id: u32,
    ) -> Self {
        AbortTarget {
            target_cfp,
            target_rfp,
            collector_ip,
            collector_channel_id,
        }
    }
}

/// Per-path metadata (future: merge into ExecutionContext.path_states[])
///
/// NOTE: This is a transitional structure. Eventually ExecutionContext will manage
/// all path state directly via ExecutionContext.path_states[path_id].
///
/// Current design maintains it as separate for backward compatibility, but
/// future refactoring will eliminate this type.
#[derive(Debug, Clone)]
pub struct PathState {
    pub path_id: u32,
    pub resource_id: ResourceId,
    pub access_type: AccessType,
    // No execution responsibility - that's ForkExecutor's job
}

impl PathState {
    pub fn new(path_id: u32, resource_id: ResourceId, access_type: AccessType) -> Self {
        PathState {
            path_id,
            resource_id,
            access_type,
        }
    }

    pub fn path_id(&self) -> u32 {
        self.path_id
    }

    pub fn resource_id(&self) -> ResourceId {
        self.resource_id
    }

    pub fn access_type(&self) -> AccessType {
        self.access_type
    }
}

/// Runtime representation of a linked fork expression
///
/// LinkedFork is the output of Phase 5 linking. It contains all metadata
/// needed for Phase 5B (ForkExecutor) to execute the fork.
#[derive(Debug, Clone)]
pub struct LinkedFork {
    fork_id: u32,
    num_paths: u32,
    path_states: Vec<PathState>,
    barriers: Vec<MemoryBarrier>,
    resource_accesses: BTreeMap<ResourceId, Vec<AccessType>>,
    pub generated_code: Option<String>, // Phase 5C: Pseudo-code to execute
    pub abort_target: Option<AbortTarget>, // Phase 7: Direct jump target for abort
}

impl LinkedFork {
    pub fn new(fork_id: u32, num_paths: u32) -> Self {
        LinkedFork {
            fork_id,
            num_paths,
            path_states: Vec::new(),
            barriers: Vec::new(),
            resource_accesses: BTreeMap::new(),
            generated_code: None,
            abort_target: None,
        }
    }

    pub fn fork_id(&self) -> u32 {
        self.fork_id
    }

    pub fn num_paths(&self) -> u32 {
        self.num_paths
    }

    pub fn add_path_state(&mut self, state: PathState) {
        self.path_states.push(state);
    }

    pub fn add_barrier(&mut self, barrier: MemoryBarrier) {
        self.barriers.push(barrier);
    }

    pub fn add_resource_access(&mut self, resource_id: ResourceId, access_type: AccessType) {
        self.resource_accesses
            .entry(resource_id)
            .or_insert_with(Vec::new)
            .push(access_type);
    }

    pub fn set_generated_code(&mut self, code: String) {
        self.generated_code = Some(code);
    }

    pub fn set_abort_target(&mut self, target: AbortTarget) {
        self.abort_target = Some(target);
    }

    pub fn path_states(&self) -> &[PathState] {
        &self.path_states
    }

    pub fn barriers(&self) -> &[MemoryBarrier] {
        &self.barriers
    }

    pub fn resource_accesses(&self) -> &BTreeMap<ResourceId, Vec<AccessType>> {
        &self.resource_accesses
    }
}

/// Fork execution result
#[derive(Debug, Clone)]
pub struct ForkExecutionResult {
    fork_id: u32,
    paths_completed: u32,
    total_paths: u32,
    collected_results: Vec<PathResult>,
    success: bool,
}

impl ForkExecutionResult {
    pub fn new(fork_id: u32, total_paths: u32) -> Self {
        ForkExecutionResult {
            fork_id,
            paths_completed: 0,
            total_paths,
            collected_results: Vec::new(),
            success: true,
        }
    }

    pub fn fork_id(&self) -> u32 {
        self.fork_id
    }

    pub fn is_complete(&self) -> bool {
        self.paths_completed == self.total_paths
    }

    pub fn is_success(&self) -> bool {
        self.success
    }

    pub fn add_result(&mut self, result: PathResult) {
        self.collected_results.push(result);
        self.paths_completed += 1;
    }

    pub fn get_collected(&self) -> &[PathResult] {
        &self.collected_results
    }
}

///
/// Responsibilities:
/// 1. Execute LinkedFork from Phase 5 linker
/// 2. Manage 5-phase execution (setup → dispatch → barriers → collect → join)
/// 3. Coordinate path execution with ExecutionContext
/// 4. Handle abort paths via direct jump integration (Phase 6)
///
/// Note: Actual path execution code (pseudo-code interpretation) deferred to Phase 5C
pub struct ForkExecutor {
    linked: LinkedFork,
    execution_state: ExecutionState,
}

/// Execution state tracking for fork coordination
#[derive(Debug, Clone)]
struct ExecutionState {
    setup_done: bool,
    dispatch_done: bool,
    barriers_done: bool,
    collect_done: bool,
    join_done: bool,
    path_results: Vec<PathResult>,
}

impl ExecutionState {
    fn new() -> Self {
        ExecutionState {
            setup_done: false,
            dispatch_done: false,
            barriers_done: false,
            collect_done: false,
            join_done: false,
            path_results: Vec::new(),
        }
    }
}

impl ForkExecutor {
    pub fn new(linked: LinkedFork) -> Self {
        ForkExecutor {
            linked,
            execution_state: ExecutionState::new(),
        }
    }

    pub fn linked(&self) -> &LinkedFork {
        &self.linked
    }

    /// Execute linked fork with 5-phase coordination
    ///
    /// Phases:
    /// 1. Setup: Allocate execution frames for each path
    /// 2. Dispatch: Schedule paths to execution engine
    /// 3. Barriers: Execute synchronization barriers (Phase 3)
    /// 4. Collect: Gather results from all paths
    /// 5. Join: Synchronize paths at join point
    pub fn execute(&mut self, context: &mut ExecutionContext) -> Result<ForkExecutionResult, String> {
        // Phase 1: Setup - Allocate execution frames
        self.phase_setup(context)?;

        // Phase 2: Dispatch - Schedule paths to execution
        self.phase_dispatch(context)?;

        // Phase 3: Barriers - Execute memory barriers
        self.phase_barriers(context)?;

        // Phase 4: Collect - Gather path execution results
        self.phase_collect(context)?;

        // Phase 5: Join - Synchronize paths at join point
        self.phase_join(context)?;

        Ok(result_aggregation::finalize(&self.linked, &self.execution_state))
    }

    /// Phase 1: Setup - Allocate frames for each path
    fn phase_setup(&mut self, context: &mut ExecutionContext) -> Result<(), String> {
        phases::phase_setup(&self.linked, &mut self.execution_state, context)
    }

    /// Phase 2: Dispatch - Schedule paths to execution engine
    ///
    /// Executes pseudo-code for each path using CodeInterpreter (Phase 5C).
    /// Phase 7: If abort detected, executes direct jump to collector.
    fn phase_dispatch(&mut self, context: &mut ExecutionContext) -> Result<(), String> {
        phases::phase_dispatch(&self.linked, &mut self.execution_state, context)
    }

    /// Phase 3: Barriers - Execute synchronization barriers
    ///
    /// Executes all memory barriers generated by Phase 3 (AutoSync).
    /// Barriers coordinate access patterns across paths.
    fn phase_barriers(&mut self, _context: &mut ExecutionContext) -> Result<(), String> {
        phases::phase_barriers(&self.linked, &mut self.execution_state)
    }

    /// Phase 4: Collect - Gather results from path execution
    ///
    /// Aggregates status and metadata from all executed paths.
    /// In a real implementation, would coordinate with scheduler.
    fn phase_collect(&mut self, _context: &mut ExecutionContext) -> Result<(), String> {
        phases::phase_collect(&mut self.execution_state)
    }

    /// Phase 5: Join - Synchronize paths at join point
    ///
    /// Ensures all paths have completed before returning to caller.
    /// Verifies synchronization invariants.
    fn phase_join(&mut self, _context: &mut ExecutionContext) -> Result<(), String> {
        phases::phase_join(&self.linked, &mut self.execution_state)
    }
}

/// Runtime linker for fork expressions (LINKING ONLY, NOT EXECUTION)
pub struct RuntimeLinker;

impl RuntimeLinker {
    /// Link a CompiledFork to a runtime representation
    ///
    /// This is the ONLY operation in Phase 5. Execution is deferred to ForkExecutor (Phase 5B).
    pub fn link(compiled: &CompiledFork) -> LinkedFork {
        let mut linked = LinkedFork::new(compiled.fork_id as u32, compiled.num_paths as u32);

        // Link path states from compiled contracts
        for (path_id, accesses) in compiled.path_contracts.iter().enumerate() {
            for access in accesses {
                let state = PathState::new(path_id as u32, access.resource_id, access.access_type);
                linked.add_path_state(state);
            }
        }

        // Link resource access metadata
        for (resource_id, access_types) in &compiled.resource_map {
            for access_type in access_types {
                linked.add_resource_access(*resource_id, *access_type);
            }
        }

        linked
    }

    /// Create a ForkExecutor from a linked fork (future: more involved setup)
    pub fn create_executor(linked: LinkedFork) -> ForkExecutor {
        ForkExecutor::new(linked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;

    #[test]
    fn test_path_result_creation() {
        let result = PathResult::new(0, ResourceId::new(1));
        assert_eq!(result.path_id, 0);
        assert_eq!(result.resource_id, ResourceId::new(1));
        assert!(!result.success);
    }

    #[test]
    fn test_path_result_success() {
        let result = PathResult::new(0, ResourceId::new(1)).success();
        assert!(result.success);
    }

    #[test]
    fn test_path_state_creation() {
        let state = PathState::new(0, ResourceId::new(1), AccessType::Read);
        assert_eq!(state.path_id(), 0);
        assert_eq!(state.resource_id(), ResourceId::new(1));
        assert_eq!(state.access_type(), AccessType::Read);
    }

    #[test]
    fn test_linked_fork_creation() {
        let linked = LinkedFork::new(1, 2);
        assert_eq!(linked.fork_id(), 1);
        assert_eq!(linked.num_paths(), 2);
    }

    #[test]
    fn test_linked_fork_add_path_state() {
        let mut linked = LinkedFork::new(1, 2);
        let state = PathState::new(0, ResourceId::new(1), AccessType::Read);
        linked.add_path_state(state);

        assert_eq!(linked.path_states().len(), 1);
        assert_eq!(linked.path_states()[0].path_id(), 0);
    }

    #[test]
    fn test_linked_fork_resource_accesses() {
        let mut linked = LinkedFork::new(1, 2);
        linked.add_resource_access(ResourceId::new(1), AccessType::Read);
        linked.add_resource_access(ResourceId::new(1), AccessType::Write);
        linked.add_resource_access(ResourceId::new(2), AccessType::Read);

        assert_eq!(linked.resource_accesses().len(), 2);
        let resource_1_accesses = linked.resource_accesses().get(&ResourceId::new(1)).unwrap();
        assert_eq!(resource_1_accesses.len(), 2);
    }

    #[test]
    fn test_fork_execution_result_creation() {
        let result = ForkExecutionResult::new(1, 2);
        assert_eq!(result.fork_id(), 1);
        assert_eq!(result.total_paths, 2);
        assert!(!result.is_complete());
        assert!(result.is_success());
    }

    #[test]
    fn test_fork_execution_result_completion() {
        let mut result = ForkExecutionResult::new(1, 2);
        result.add_result(PathResult::new(0, ResourceId::new(1)));
        result.add_result(PathResult::new(1, ResourceId::new(1)));

        assert!(result.is_complete());
        assert_eq!(result.get_collected().len(), 2);
    }

    #[test]
    fn test_runtime_linker_link() {
        let mut compiled = CompiledFork::new(1, 2);
        compiled.add_access(0, ResourceId::new(1), AccessType::Read);
        compiled.add_access(1, ResourceId::new(1), AccessType::Write);

        let linked = RuntimeLinker::link(&compiled);
        assert_eq!(linked.fork_id(), 1);
        assert_eq!(linked.num_paths(), 2);
        assert_eq!(linked.path_states().len(), 2);
    }

    #[test]
    fn test_runtime_linker_link_resource_accesses() {
        let mut compiled = CompiledFork::new(1, 2);
        compiled.add_access(0, ResourceId::new(1), AccessType::Read);
        compiled.add_access(1, ResourceId::new(1), AccessType::Write);
        compiled.add_access(1, ResourceId::new(2), AccessType::Read);

        let linked = RuntimeLinker::link(&compiled);
        assert_eq!(linked.resource_accesses().len(), 2);

        let resource_1_accesses = linked.resource_accesses().get(&ResourceId::new(1));
        assert!(resource_1_accesses.is_some());
        assert_eq!(resource_1_accesses.unwrap().len(), 2);
    }

    #[test]
    fn test_fork_executor_creation() {
        let linked = LinkedFork::new(1, 2);
        let executor = ForkExecutor::new(linked);
        assert_eq!(executor.linked().fork_id(), 1);
    }

    #[test]
    fn test_fork_executor_placeholder_execute() {
        let mut compiled = CompiledFork::new(1, 2);
        compiled.add_access(0, ResourceId::new(1), AccessType::Read);
        compiled.add_access(1, ResourceId::new(1), AccessType::Write);

        let linked = RuntimeLinker::link(&compiled);
        let mut executor = ForkExecutor::new(linked);

        // Phase 5B implementation: 5-phase execution
        let mut ctx = ExecutionContext::new(1024).expect("Failed to create context");
        let result = executor.execute(&mut ctx);
        assert!(result.is_ok());

        let exec_result = result.unwrap();
        assert_eq!(exec_result.paths_completed, 2);
        assert!(exec_result.is_complete());
        assert!(exec_result.is_success());
    }

    #[test]
    fn test_fork_executor_phase_setup() {
        let mut compiled = CompiledFork::new(1, 2);
        compiled.add_access(0, ResourceId::new(1), AccessType::Read);
        compiled.add_access(1, ResourceId::new(2), AccessType::Write);

        let linked = RuntimeLinker::link(&compiled);
        let mut executor = ForkExecutor::new(linked);

        let mut ctx = ExecutionContext::new(1024).expect("Failed to create context");

        // Verify phases execute in order
        assert!(!executor.execution_state.setup_done);
        let result = executor.execute(&mut ctx);
        assert!(result.is_ok());

        // All phases should be complete
        assert!(executor.execution_state.setup_done);
        assert!(executor.execution_state.dispatch_done);
        assert!(executor.execution_state.barriers_done);
        assert!(executor.execution_state.collect_done);
        assert!(executor.execution_state.join_done);
    }

    #[test]
    fn test_fork_executor_path_results() {
        let mut compiled = CompiledFork::new(2, 3);
        compiled.add_access(0, ResourceId::new(1), AccessType::Read);
        compiled.add_access(1, ResourceId::new(2), AccessType::Write);
        compiled.add_access(2, ResourceId::new(1), AccessType::Read);

        let linked = RuntimeLinker::link(&compiled);
        let mut executor = ForkExecutor::new(linked);

        let mut ctx = ExecutionContext::new(2048).expect("Failed to create context");
        let result = executor.execute(&mut ctx);
        assert!(result.is_ok());

        let exec_result = result.unwrap();
        assert_eq!(exec_result.fork_id(), 2);
        assert_eq!(exec_result.get_collected().len(), 3);

        // All paths should have succeeded
        for path_result in exec_result.get_collected() {
            assert!(path_result.success);
        }
    }

    #[test]
    fn test_fork_executor_with_barriers() {
        let mut compiled = CompiledFork::new(3, 2);
        compiled.add_access(0, ResourceId::new(1), AccessType::Read);
        compiled.add_access(1, ResourceId::new(1), AccessType::Write);

        let mut linked = RuntimeLinker::link(&compiled);
        
        // Add a barrier (Phase 3 integration)
        let barrier = MemoryBarrier::new(crate::sync::BarrierKind::FullFence, 1, 0);
        linked.add_barrier(barrier);

        let mut executor = ForkExecutor::new(linked);

        let mut ctx = ExecutionContext::new(1024).expect("Failed to create context");
        let result = executor.execute(&mut ctx);
        assert!(result.is_ok());

        let exec_result = result.unwrap();
        assert!(exec_result.is_complete());
        assert!(exec_result.is_success());
    }


    #[test]
    fn test_runtime_linker_link_and_create_executor() {
        let mut compiled = CompiledFork::new(1, 2);
        compiled.add_access(0, ResourceId::new(1), AccessType::Read);
        compiled.add_access(1, ResourceId::new(1), AccessType::Write);

        let linked = RuntimeLinker::link(&compiled);
        let executor = RuntimeLinker::create_executor(linked);
        assert_eq!(executor.linked().fork_id(), 1);
    }

    // Phase 5C: CodeInterpreter Tests

    #[test]
    fn test_code_interpreter_parse_read_instruction() {
        let code = "read 42";
        let instructions = CodeInterpreter::parse(code);
        assert_eq!(instructions.len(), 1);
        assert_eq!(instructions[0], Instruction::ReadResource(42));
    }

    #[test]
    fn test_code_interpreter_parse_write_instruction() {
        let code = "write 10";
        let instructions = CodeInterpreter::parse(code);
        assert_eq!(instructions.len(), 1);
        assert_eq!(instructions[0], Instruction::WriteResource(10));
    }

    #[test]
    fn test_code_interpreter_parse_readwrite_instruction() {
        let code = "readwrite 5";
        let instructions = CodeInterpreter::parse(code);
        assert_eq!(instructions.len(), 1);
        assert_eq!(instructions[0], Instruction::ReadWriteResource(5));
    }

    #[test]
    fn test_code_interpreter_parse_multiple_instructions() {
        let code = r#"
            read 1
            write 2
            barrier
            success
        "#;
        let instructions = CodeInterpreter::parse(code);
        assert_eq!(instructions.len(), 4);
        assert_eq!(instructions[0], Instruction::ReadResource(1));
        assert_eq!(instructions[1], Instruction::WriteResource(2));
        assert_eq!(instructions[2], Instruction::Barrier);
        assert_eq!(instructions[3], Instruction::Success);
    }

    #[test]
    fn test_code_interpreter_parse_with_comments() {
        let code = r#"
            // Path execution starts here
            read 1
            // Read from resource 1
            write 2
        "#;
        let instructions = CodeInterpreter::parse(code);
        assert_eq!(instructions.len(), 2);
        assert_eq!(instructions[0], Instruction::ReadResource(1));
        assert_eq!(instructions[1], Instruction::WriteResource(2));
    }

    #[test]
    fn test_code_interpreter_parse_empty_lines() {
        let code = r#"
            
            read 1
            
            write 2
            
        "#;
        let instructions = CodeInterpreter::parse(code);
        assert_eq!(instructions.len(), 2);
        assert_eq!(instructions[0], Instruction::ReadResource(1));
        assert_eq!(instructions[1], Instruction::WriteResource(2));
    }

    #[test]
    fn test_code_interpreter_execute_success_path() {
        let path_id = 0;
        let resource_id = ResourceId::new(1);
        let instructions = vec![
            Instruction::ReadResource(1),
            Instruction::WriteResource(1),
            Instruction::Success,
        ];

        let result = CodeInterpreter::execute(path_id, resource_id, &instructions);
        assert_eq!(result.path_id, path_id);
        assert_eq!(result.resource_id, resource_id);
        assert!(result.success);
    }

    #[test]
    fn test_code_interpreter_execute_abort_path() {
        let path_id = 1;
        let resource_id = ResourceId::new(2);
        let instructions = vec![
            Instruction::ReadResource(2),
            Instruction::Abort,
            Instruction::Success, // Should not be reached
        ];

        let result = CodeInterpreter::execute(path_id, resource_id, &instructions);
        assert_eq!(result.path_id, path_id);
        assert_eq!(result.resource_id, resource_id);
        // Abort stops execution before success marker
    }

    #[test]
    fn test_code_interpreter_execute_barrier() {
        let path_id = 2;
        let resource_id = ResourceId::new(3);
        let instructions = vec![
            Instruction::ReadResource(3),
            Instruction::Barrier,
            Instruction::Success,
        ];

        let result = CodeInterpreter::execute(path_id, resource_id, &instructions);
        assert!(result.success);
    }

    #[test]
    fn test_code_interpreter_resource_access_tracker() {
        let mut tracker = ResourceAccessTracker::new(0);
        assert_eq!(tracker.path_id, 0);
        assert_eq!(tracker.total_accesses(), 0);

        tracker.record_read(1);
        tracker.record_read(2);
        assert_eq!(tracker.reads.len(), 2);
        assert_eq!(tracker.total_accesses(), 2);

        tracker.record_write(1);
        tracker.record_write(3);
        assert_eq!(tracker.writes.len(), 2);
        assert_eq!(tracker.total_accesses(), 4);

        // Duplicate reads should not be recorded
        tracker.record_read(1);
        assert_eq!(tracker.total_accesses(), 4);
    }

    #[test]
    fn test_linked_fork_with_generated_code() {
        let mut linked = LinkedFork::new(1, 2);
        let code = "read 1\nwrite 1\nsuccess".to_string();
        linked.set_generated_code(code.clone());

        assert!(linked.generated_code.is_some());
        assert_eq!(linked.generated_code.unwrap(), code);
    }

    #[test]
    fn test_fork_executor_with_pseudo_code() {
        let mut compiled = CompiledFork::new(4, 1);
        compiled.add_access(0, ResourceId::new(1), AccessType::Read);

        let mut linked = RuntimeLinker::link(&compiled);
        let code = "read 1\nsuccess".to_string();
        linked.set_generated_code(code);

        let mut executor = ForkExecutor::new(linked);
        let mut ctx = ExecutionContext::new(1024).expect("Failed to create context");

        let result = executor.execute(&mut ctx);
        assert!(result.is_ok());

        let exec_result = result.unwrap();
        assert_eq!(exec_result.paths_completed, 1);
        assert!(exec_result.is_success());
    }

    // Phase 7: Direct Jump Integration Tests

    #[test]
    fn test_path_result_abort_flag() {
        let path_id = 0;
        let resource_id = ResourceId::new(1);
        
        let result = PathResult::new(path_id, resource_id);
        assert!(!result.aborted);
        assert!(!result.success);
        
        let aborted_result = result.abort();
        assert!(aborted_result.aborted);
        assert!(!aborted_result.success);  // Abort is not success
    }

    #[test]
    fn test_abort_target_creation() {
        let target_cfp = std::ptr::null_mut::<u8>();
        let target_rfp = ptr::without_provenance_mut::<u8>(0x1000);
        let collector_ip = ptr::without_provenance::<u8>(0x2000);
        
        let abort_target = AbortTarget::new(target_cfp, target_rfp, collector_ip, 42);
        assert_eq!(abort_target.target_cfp, target_cfp);
        assert_eq!(abort_target.target_rfp, target_rfp);
        assert_eq!(abort_target.collector_ip, collector_ip);
        assert_eq!(abort_target.collector_channel_id, 42);
    }

    #[test]
    fn test_code_interpreter_abort_instruction_sets_flag() {
        let path_id = 0;
        let resource_id = ResourceId::new(1);
        let instructions = vec![
            Instruction::ReadResource(1),
            Instruction::Abort,
            Instruction::Success,  // Should not be reached
        ];

        let result = CodeInterpreter::execute(path_id, resource_id, &instructions);
        assert!(result.aborted);
        assert!(!result.success);  // Abort stops execution before success
    }

    #[test]
    fn test_code_interpreter_abort_stops_execution() {
        let path_id = 1;
        let resource_id = ResourceId::new(2);
        let instructions = vec![
            Instruction::ReadResource(2),
            Instruction::Abort,
            Instruction::WriteResource(2),  // Should not execute
            Instruction::Success,            // Should not execute
        ];

        let result = CodeInterpreter::execute(path_id, resource_id, &instructions);
        assert!(result.aborted);
        // Verify only read executed (no side effects from write/success)
    }

    #[test]
    fn test_linked_fork_abort_target_storage() {
        let mut linked = LinkedFork::new(1, 2);
        
        let target_cfp = ptr::without_provenance_mut::<u8>(0x3000);
        let target_rfp = ptr::without_provenance_mut::<u8>(0x4000);
        let collector_ip = ptr::without_provenance::<u8>(0x5000);
        
        let abort_target = AbortTarget::new(target_cfp, target_rfp, collector_ip, 99);
        linked.set_abort_target(abort_target);
        
        assert!(linked.abort_target.is_some());
        let stored = linked.abort_target.as_ref().unwrap();
        assert_eq!(stored.target_cfp, target_cfp);
    }

    #[test]
    fn test_phase7_abort_detection_in_result() {
        // Verify that CodeInterpreter properly marks abort in PathResult
        let instructions = vec![
            Instruction::ReadResource(1),
            Instruction::Abort,
        ];
        
        let result = CodeInterpreter::execute(0, ResourceId::new(1), &instructions);
        assert!(result.aborted);
        assert_eq!(result.path_id, 0);
        assert_eq!(result.resource_id, ResourceId::new(1));
    }

    #[test]
    fn test_phase7_abort_vs_success() {
        // Ensure abort and success are mutually exclusive
        let abort_result = PathResult::new(0, ResourceId::new(1)).abort();
        let success_result = PathResult::new(0, ResourceId::new(1)).success();
        
        assert!(abort_result.aborted && !abort_result.success);
        assert!(!success_result.aborted && success_result.success);
    }
}

