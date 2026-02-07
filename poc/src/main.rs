use poc::{config, logger, sample_wf};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    config::init();
    let _ = logger::init();
    sample_wf::rust_summary_workflow()
}
