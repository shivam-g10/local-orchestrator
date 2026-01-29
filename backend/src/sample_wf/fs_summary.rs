use std::{env, error::Error};

use crate::{
    block::{Block, BlockExecutorTrait, utils::*},
    config,
    workflow::Workflow,
};
pub fn rust_summary_workflow() -> Result<(), Box<dyn Error>> {
    let api_key = config::get_env::<String>("OPENAI_API_KEY");

    // Cron
    let mut flow = Workflow::new();

    let dt = chrono::Local::now();
    let naive_utc = dt.naive_utc();
    let offset = dt.offset();
    let dt_new = chrono::DateTime::<chrono::Local>::from_naive_utc_and_offset(naive_utc, *offset);
    let dt_str = dt_new.to_string();

    let mut ai_fs_search_block: Block = ai_utils::create_ai_block(
        ai_utils::AIProvider::OpenAi,
        &api_key,
        &format!(
            "Extract all useful information from this current directory '.'. Make sure to keep it data dense. Current ISO Time: {}",
            dt_str
        ),
    );
    let fs_tools = ai_utils::create_fs_tools(".").expect("Error in setting fs tools directory");
    ai_fs_search_block = ai_utils::add_fs_tools_to_ai_block(ai_fs_search_block, fs_tools).expect("Error in adding fs tools to block");
    flow.register_block(ai_fs_search_block.clone());

    let ai_report = ai_utils::create_ai_block(
        ai_utils::AIProvider::OpenAi,
        &api_key,
        "Don't ask questions or follow up. Just do the work. Format this into a good full executive summary: ###INPUT",
    );
    flow.register_block(ai_report.clone());

    let current_dir = match env::current_dir() {
        Ok(dir) => dir.to_str().unwrap().to_owned(),
        Err(e) => {
            panic!("Error getting cwd {e}");
        }
    };

    let file_save_block = file_utils::create_file_block(
        file_utils::FileOperationType::WRITE,
        &current_dir,
        "test.md",
    );
    flow.register_block(file_save_block.clone());

    flow.register_forward_link(&ai_fs_search_block, &ai_report);
    flow.register_forward_link(&ai_report, &file_save_block);

    flow.execute(*ai_fs_search_block.get_id(), None);
    Ok(())
}