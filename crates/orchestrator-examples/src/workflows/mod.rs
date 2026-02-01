// pub mod child_workflow_demo;
// pub mod context_payload_demo;
// pub mod copy_files;
// pub mod cyclic_demo;
// pub mod expense_report;
// pub mod invoice_line_processor;
// pub mod news_aggregator;
// pub mod price_drop_checker;
// pub mod print_readme;
// pub mod retry_until_success;
// pub mod single_file_read;
// pub mod stock_report;

pub mod personal_reports;

// #[allow(unused_imports)]
// pub use child_workflow_demo::child_workflow_demo_workflow;
// #[allow(unused_imports)]
// pub use context_payload_demo::run_context_payload_demo_workflow;
// #[allow(unused_imports)]
// pub use copy_files::copy_files_workflow;
// #[allow(unused_imports)]
// pub use cyclic_demo::cyclic_demo_workflow;
// #[allow(unused_imports)]
// pub use expense_report::{default_statement_path, run_expense_report_workflow};
// #[allow(unused_imports)]
// pub use print_readme::print_readme_workflow;
// #[allow(unused_imports)]
// pub use single_file_read::single_file_read_workflow;
// #[allow(unused_imports)]
// pub use stock_report::{default_csv_path, run_stock_report_workflow};
// #[allow(unused_imports)]
// pub use invoice_line_processor::{default_invoice_path, run_invoice_line_processor_workflow};
// #[allow(unused_imports)]
// pub use price_drop_checker::run_price_drop_checker_workflow;
// #[allow(unused_imports)]
// pub use news_aggregator::run_news_aggregator_workflow;
// #[allow(unused_imports)]
// pub use retry_until_success::run_retry_until_success_workflow;

pub use personal_reports::{ensure_dummy_data, run_personal_reports_workflow};