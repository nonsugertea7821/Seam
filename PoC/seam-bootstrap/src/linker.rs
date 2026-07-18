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

/// Result of a path execution (Status only, NOT data carrier)
#[derive(Debug, Clone)]
pub struct PathResult {
    pub path_id: u32,
    pub resource_id: ResourceId,
    pub success: bool,
    // NOTE: data field removed - state belongs in ResourceFrame, not here
}

impl PathResult {
    pub fn new(path_id: u32, resource_id: ResourceId) -> Self {
        PathResult {
            path_id,
            resource_id,
            success: false,
        }
    }

    pub fn success(mut self) -> Self {
        self.success = true;
        self
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
}

impl LinkedFork {
    pub fn new(fork_id: u32, num_paths: u32) -> Self {
        LinkedFork {
            fork_id,
            num_paths,
            path_states: Vec::new(),
            barriers: Vec::new(),
            resource_accesses: BTreeMap::new(),
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

/// Phase 5B: ForkExecutor — Execute LinkedFork with 5-phase coordination
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

    fn all_done(&self) -> bool {
        self.setup_done
            && self.dispatch_done
            && self.barriers_done
            && self.collect_done
            && self.join_done
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
        let mut result =
            ForkExecutionResult::new(self.linked.fork_id(), self.linked.num_paths());

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

        // Build final result from execution state
        for path_result in &self.execution_state.path_results {
            result.add_result(path_result.clone());
        }

        Ok(result)
    }

    /// Phase 1: Setup - Allocate frames for each path
    fn phase_setup(&mut self, context: &mut ExecutionContext) -> Result<(), String> {
        // Allocate frame in arena for each path
        for path_state in self.linked.path_states() {
            // Each path needs a frame for local variables and state
            // Frame size: 256 bytes (typical, can be tuned)
            context.frame_push(256).map_err(|e| format!("Setup failed: {}", e))?;
        }

        self.execution_state.setup_done = true;
        Ok(())
    }

    /// Phase 2: Dispatch - Schedule paths to execution engine
    ///
    /// In a real implementation, this would dispatch to a scheduler/thread pool.
    /// For now: paths execute sequentially in placeholder execution.
    fn phase_dispatch(&mut self, _context: &mut ExecutionContext) -> Result<(), String> {
        // Future: Thread pool dispatch or async scheduling
        // Current: Mark for collection phase
        for path_state in self.linked.path_states() {
            // Placeholder: Create result for each path
            let result = PathResult::new(path_state.path_id(), path_state.resource_id()).success();
            self.execution_state.path_results.push(result);
        }

        self.execution_state.dispatch_done = true;
        Ok(())
    }

    /// Phase 3: Barriers - Execute synchronization barriers
    ///
    /// Executes all memory barriers generated by Phase 3 (AutoSync).
    /// Barriers coordinate access patterns across paths.
    fn phase_barriers(&mut self, _context: &mut ExecutionContext) -> Result<(), String> {
        // Execute each barrier
        for barrier in self.linked.barriers() {
            barrier.execute();
        }

        self.execution_state.barriers_done = true;
        Ok(())
    }

    /// Phase 4: Collect - Gather results from path execution
    ///
    /// Aggregates status and metadata from all executed paths.
    /// In a real implementation, would coordinate with scheduler.
    fn phase_collect(&mut self, _context: &mut ExecutionContext) -> Result<(), String> {
        // Results already collected in phase_dispatch for prototype
        // Real implementation would:
        // 1. Wait for all paths to complete
        // 2. Gather status from each path
        // 3. Check for abort conditions (via RFP/direct_jump_context)
        // 4. Accumulate results

        self.execution_state.collect_done = true;
        Ok(())
    }

    /// Phase 5: Join - Synchronize paths at join point
    ///
    /// Ensures all paths have completed before returning to caller.
    /// Verifies synchronization invariants.
    fn phase_join(&mut self, _context: &mut ExecutionContext) -> Result<(), String> {
        // Verify all paths completed
        if self.execution_state.path_results.len() as u32 != self.linked.num_paths() {
            return Err(format!(
                "Join failed: {} paths out of {} completed",
                self.execution_state.path_results.len(),
                self.linked.num_paths()
            ));
        }

        self.execution_state.join_done = true;
        Ok(())
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
        assert!(executor.execution_state.all_done());
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
}

