use backend::{
    block::{AIProvider, BlockExecutorTrait, FileOperationType, utils::*},
    config, logger,
    workflow::Workflow,
};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    config::init();
    let _ = logger::init();
    // agent triggers via cron
    // trigger everyday at 00:00
    // trigger ai with tool to search the web for particular topic
    // After search data is completed another AI is triggered to compile information into markdown
    // Save output to file

    let api_key = config::get_env::<String>("OPENAI_API_KEY");

    // Cron
    let mut flow = Workflow::new();
    let cron_block_result = create_cron_block("* * * * * * *");
    let cron_block = match cron_block_result {
        Err(e) => return Err(Box::new(e)),
        Ok(block) => block,
    };
    let dt = chrono::Local::now();
    let naive_utc = dt.naive_utc();
    let offset = dt.offset().clone();
    let dt_new = chrono::DateTime::<chrono::Local>::from_naive_utc_and_offset(naive_utc, offset);
    let dt_str = dt_new.to_string();
    flow.register_block(cron_block.clone());

    let ai_search_block = create_ai_block(
        AIProvider::OpenAi,
        &api_key,
        &format!(
            "Find latest news related to Rust. Give me full information based on what you know. Don't ask me questions. Upto 500 words. Make sure to keep it data dense. Current ISO Time: {}",
            dt_str
        ),
    );
    flow.register_block(ai_search_block.clone());

    let ai_report = create_ai_block(
        AIProvider::OpenAi,
        &api_key,
        "Don't ask questions or follow up. Just do the work. Format this into a good full executive summary after validating infomation: ###INPUT",
    );
    flow.register_block(ai_report.clone());

    let file_save_block = create_file_block(FileOperationType::WRITE, "~/", "test.md");
    flow.register_block(file_save_block.clone());

    flow.register_forward_link(&cron_block, &ai_search_block);
    flow.register_forward_link(&ai_search_block, &ai_report);
    flow.register_forward_link(&ai_report, &file_save_block);

    flow.execute(cron_block.get_id().clone());
    return Ok(());
}
