use std::error::Error;

use crate::{
    block::{Block, BlockExecutorTrait, utils::*},
    config,
    workflow::Workflow,
};

pub fn rust_social_workflow() -> Result<(), Box<dyn Error>> {
    let api_key = config::get_env::<String>("OPENAI_API_KEY");

    // Cron
    let mut flow = Workflow::new();
    let cron_block_result = cron_utils::create_cron_block("0 */5 * * * * *");
    let cron_block = match cron_block_result {
        Err(e) => return Err(Box::new(e)),
        Ok(block) => block,
    };
    flow.register_block(cron_block.clone());

    let ai_search_block: Block = ai_utils::create_ai_block(
        ai_utils::AIProvider::OpenAi,
        &api_key,
        "Don't ask questions or follow up. Just do the work. Give me a concise update on what people in India have been talking about on social media in the last 5 minutes since: ###INPUT",
    );
    flow.register_block(ai_search_block.clone());

    let ai_email = ai_utils::create_ai_block(
        ai_utils::AIProvider::OpenAi,
        &api_key,
        "Don't ask questions or follow up. Just do the work. Format this into a beautiful email. Make sure the core is highlighted. Make sure the html is send ready. Give me only the HTML: ###INPUT",
    );
    flow.register_block(ai_email.clone());

    let email_block_result =
        email_utils::create_email_block("test@test.com", "Test email", "Social Pulse");
    let email_block = match email_block_result {
        Err(e) => return Err(Box::new(e)),
        Ok(block) => block,
    };
    flow.register_block(email_block.clone());

    flow.register_forward_link(&cron_block, &ai_search_block);
    flow.register_forward_link(&ai_search_block, &ai_email);
    flow.register_forward_link(&ai_email, &email_block);

    flow.execute(*cron_block.get_id(), None);
    Ok(())
}
