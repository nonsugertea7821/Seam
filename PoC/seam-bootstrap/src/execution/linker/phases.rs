use crate::context::ExecutionContext;

use super::{CodeInterpreter, ExecutionState, LinkedFork};
use super::result_aggregation;

pub(crate) fn phase_setup(
    linked: &LinkedFork,
    execution_state: &mut ExecutionState,
    context: &mut ExecutionContext,
) -> Result<(), String> {
    for _ in linked.path_states() {
        context.frame_push(256).map_err(|e| format!("Setup failed: {}", e))?;
    }

    execution_state.setup_done = true;
    Ok(())
}

pub(crate) fn phase_dispatch(
    linked: &LinkedFork,
    execution_state: &mut ExecutionState,
    context: &mut ExecutionContext,
) -> Result<(), String> {
    for path_state in linked.path_states() {
        let instructions = if let Some(ref code) = linked.generated_code {
            CodeInterpreter::parse(code)
        } else {
            Vec::new()
        };

        let result = CodeInterpreter::execute(
            path_state.path_id(),
            path_state.resource_id(),
            &instructions,
        );

        if result.aborted {
            if let Some(ref abort_target) = linked.abort_target {
                let current_cfp = context.cfp().0 as *mut u8;

                context.set_direct_jump_context(
                    abort_target.target_cfp,
                    current_cfp,
                    abort_target.collector_ip,
                    abort_target.collector_channel_id,
                );

                let _ = context.abort(None).map_err(|e| format!("Abort failed: {}", e));
            }
        }

        result_aggregation::append_path_result(execution_state, result);
    }

    execution_state.dispatch_done = true;
    Ok(())
}

pub(crate) fn phase_barriers(
    linked: &LinkedFork,
    execution_state: &mut ExecutionState,
) -> Result<(), String> {
    for barrier in linked.barriers() {
        barrier.execute();
    }

    execution_state.barriers_done = true;
    Ok(())
}

pub(crate) fn phase_collect(execution_state: &mut ExecutionState) -> Result<(), String> {
    execution_state.collect_done = true;
    Ok(())
}

pub(crate) fn phase_join(
    linked: &LinkedFork,
    execution_state: &mut ExecutionState,
) -> Result<(), String> {
    if execution_state.path_results.len() as u32 != linked.num_paths() {
        return Err(format!(
            "Join failed: {} paths out of {} completed",
            execution_state.path_results.len(),
            linked.num_paths()
        ));
    }

    execution_state.join_done = true;
    Ok(())
}
