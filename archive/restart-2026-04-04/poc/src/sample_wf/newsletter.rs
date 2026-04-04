use std::error::Error;

use crate::{
    block::{Block, BlockExecutorTrait, utils::*},
    config,
    workflow::Workflow,
};

pub fn rust_newsletter_workflow() -> Result<(), Box<dyn Error>> {
    let api_key = config::get_env::<String>("OPENAI_API_KEY");

    // Cron
    let mut flow = Workflow::new();
    let dt = chrono::Local::now();
    let naive_utc = dt.naive_utc();
    let offset = dt.offset();
    let dt_new = chrono::DateTime::<chrono::Local>::from_naive_utc_and_offset(naive_utc, *offset);
    let dt_str = dt_new.to_string();

    let ai_search_block: Block = ai_utils::create_ai_block(
        ai_utils::AIProvider::OpenAi,
        &api_key,
        &format!(
            "Find latest news related to Rust in the past week. Don't ask me questions. Upto 500 words. Make sure to keep it data dense. Current ISO Time: {}",
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

    let email_block_result =
        email_utils::create_email_block("test@test.com", "Test email", "Rust NewsLetter");

    let email_block = match email_block_result {
        Err(e) => return Err(Box::new(e)),
        Ok(block) => block,
    };
    flow.register_block(email_block.clone());

    flow.register_forward_link(&ai_search_block, &ai_email);
    flow.register_forward_link(&ai_email, &email_block);

    flow.execute(*ai_search_block.get_id(), None);
    Ok(())
}
