use crate::block::executor_error::ExecutorError;

use super::CronBlockBody;

pub fn execute_cron(_: Option<String>, _: CronBlockBody) -> Result<Option<String>, ExecutorError> {
    Ok(None)
}
