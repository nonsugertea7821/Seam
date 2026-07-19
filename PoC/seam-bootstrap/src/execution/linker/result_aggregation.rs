use super::{ExecutionState, ForkExecutionResult, LinkedFork, PathResult};

pub(super) fn append_path_result(execution_state: &mut ExecutionState, result: PathResult) {
    execution_state.path_results.push(result);
}

pub(super) fn finalize(linked: &LinkedFork, execution_state: &ExecutionState) -> ForkExecutionResult {
    let mut result = ForkExecutionResult::new(linked.fork_id(), linked.num_paths());
    for path_result in &execution_state.path_results {
        result.add_result(path_result.clone());
    }
    result
}
