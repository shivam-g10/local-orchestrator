use backend::{
    block::{BlockExecutorTrait, utils::*},
    config, logger,
    workflow::Workflow,
};
use std::{env, error::Error};

fn main() -> Result<(), Box<dyn Error>> {
    config::init();
    let _ = logger::init();
    rust_newsletter_workflow()
}

#[allow(dead_code)]
fn rust_newsletter_workflow() -> Result<(), Box<dyn Error>> {
    let api_key = config::get_env::<String>("OPENAI_API_KEY");

    // Cron
    let mut flow = Workflow::new();
    let cron_block_result = cron_utils::create_cron_block("* * * * * * *");
    let cron_block = match cron_block_result {
        Err(e) => return Err(Box::new(e)),
        Ok(block) => block,
    };
    let dt = chrono::Local::now();
    let naive_utc = dt.naive_utc();
    let offset = dt.offset();
    let dt_new = chrono::DateTime::<chrono::Local>::from_naive_utc_and_offset(naive_utc, *offset);
    let dt_str = dt_new.to_string();
    flow.register_block(cron_block.clone());

    let ai_search_block: backend::block::Block = ai_utils::create_ai_block(
        ai_utils::AIProvider::OpenAi,
        &api_key,
        &format!(
            "Find latest news related to Rust in the past week. Give me full information based on what you know. Don't ask me questions. Upto 500 words. Make sure to keep it data dense. Current ISO Time: {}",
            dt_str
        ),
    );
    flow.register_block(ai_search_block.clone());

    let ai_email = ai_utils::create_ai_block(
        ai_utils::AIProvider::OpenAi,
        &api_key,
        "Don't ask questions or follow up. Just do the work. Format this into a beautiful email html for a newsletter. Make sure the html is send ready. Give me only the HTML: ###INPUT",
    );
    flow.register_block(ai_email.clone());

    let email_block_result = email_utils::create_email_block(
    "test@test.com",
        "Test email",
        "Rust NewsLetter",
    );
    let email_block = match email_block_result {
        Err(e) => return Err(Box::new(e)),
        Ok(block) => block,
    };
    flow.register_block(email_block.clone());

    flow.register_forward_link(&cron_block, &ai_search_block);
    flow.register_forward_link(&ai_search_block, &ai_email);
    flow.register_forward_link(&ai_email, &email_block);

    flow.execute(*cron_block.get_id());
    Ok(())
}

#[allow(dead_code)]
fn rust_note_workflow() -> Result<(), Box<dyn Error>> {
    let api_key = config::get_env::<String>("OPENAI_API_KEY");

    // Cron
    let mut flow = Workflow::new();
    let cron_block_result = cron_utils::create_cron_block("* * * * * * *");
    let cron_block = match cron_block_result {
        Err(e) => return Err(Box::new(e)),
        Ok(block) => block,
    };
    let dt = chrono::Local::now();
    let naive_utc = dt.naive_utc();
    let offset = dt.offset();
    let dt_new = chrono::DateTime::<chrono::Local>::from_naive_utc_and_offset(naive_utc, *offset);
    let dt_str = dt_new.to_string();
    flow.register_block(cron_block.clone());

    let ai_search_block: backend::block::Block = ai_utils::create_ai_block(
        ai_utils::AIProvider::OpenAi,
        &api_key,
        &format!(
            "Find latest news related to Rust. Give me full information based on what you know. Don't ask me questions. Upto 500 words. Make sure to keep it data dense. Current ISO Time: {}",
            dt_str
        ),
    );
    flow.register_block(ai_search_block.clone());

    let ai_report = ai_utils::create_ai_block(
        ai_utils::AIProvider::OpenAi,
        &api_key,
        "Don't ask questions or follow up. Just do the work. Format this into a good full executive summary after validating infomation: ###INPUT",
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

    flow.register_forward_link(&cron_block, &ai_search_block);
    flow.register_forward_link(&ai_search_block, &ai_report);
    flow.register_forward_link(&ai_report, &file_save_block);

    flow.execute(*cron_block.get_id());
    Ok(())
}