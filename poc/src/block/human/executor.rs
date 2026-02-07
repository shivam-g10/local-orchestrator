use crate::block::{
    ExecutionRunResult,
    human::{FormFieldConfig, FormSubmit, HumanBlockBody},
};

pub fn execute_human(_: Option<String>, _: HumanBlockBody) -> ExecutionRunResult {
    Ok(None)
}

#[allow(dead_code)]
pub fn submit_form(_: Vec<FormFieldConfig>, _: Vec<FormSubmit>) -> bool {
    false
}
