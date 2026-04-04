use orchestrator_core::block::{
    BlockError, BlockExecutionContext, BlockInput, InputContract, OutputContract, OutputMode,
    ValidateContext, ValueKind, ValueKindSet, resolve_forced_input,
};

pub fn resolve_effective_input(
    ctx: &BlockExecutionContext,
    input_from: &[uuid::Uuid],
    config_input: Option<BlockInput>,
) -> Result<BlockInput, BlockError> {
    if !input_from.is_empty() {
        return resolve_forced_input(input_from, &ctx.store);
    }
    if let Some(input) = config_input {
        return Ok(input);
    }
    Ok(ctx.prev.clone())
}

#[allow(dead_code)]
pub fn validate_expected_input(
    ctx: &ValidateContext<'_>,
    accepted: ValueKindSet,
) -> Result<(), BlockError> {
    if !ctx.forced_refs.is_empty() {
        for (idx, contract) in ctx.forced_refs.iter().enumerate() {
            if !contract.kinds.intersects(accepted) {
                return Err(BlockError::Other(format!(
                    "forced input source {} output kind incompatible with expected input",
                    idx
                )));
            }
        }
        return Ok(());
    }

    match &ctx.prev {
        InputContract::Empty => {
            if accepted.contains(ValueKind::Empty) {
                Ok(())
            } else {
                Err(BlockError::Other(
                    "previous input is empty but block expects non-empty input".into(),
                ))
            }
        }
        InputContract::One(kinds) => {
            if kinds.intersects(accepted) {
                Ok(())
            } else {
                Err(BlockError::Other(
                    "previous input kind incompatible with expected input".into(),
                ))
            }
        }
        InputContract::Multi(kinds) => {
            if kinds.iter().all(|k| k.intersects(accepted)) {
                Ok(())
            } else {
                Err(BlockError::Other(
                    "previous multi-input kind incompatible with expected input".into(),
                ))
            }
        }
    }
}

#[allow(dead_code)]
pub fn validate_single_input_mode(ctx: &ValidateContext<'_>) -> Result<(), BlockError> {
    if !ctx.forced_refs.is_empty() {
        if ctx.forced_refs.len() > 1 {
            return Err(BlockError::Other(
                "forced input expects a single source output".into(),
            ));
        }
        if matches!(ctx.forced_refs[0].mode, OutputMode::Multiple) {
            return Err(BlockError::Other(
                "forced input source produces multiple outputs; single input expected".into(),
            ));
        }
        return Ok(());
    }

    if matches!(ctx.prev, InputContract::Multi(_)) {
        return Err(BlockError::Other(
            "previous linkage provides multiple inputs; single input expected".into(),
        ));
    }
    Ok(())
}

#[allow(dead_code)]
pub fn first_forced_ref_or_prev<'a>(ctx: &'a ValidateContext<'_>) -> &'a [OutputContract] {
    if !ctx.forced_refs.is_empty() {
        return ctx.forced_refs;
    }
    &[]
}
