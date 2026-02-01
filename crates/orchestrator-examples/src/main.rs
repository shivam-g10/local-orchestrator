//! Sample runner CLI: choose and run a workflow from the examples.

mod workflows;

use clap::{Parser, Subcommand};
use std::path::Path;

#[derive(Parser)]
#[command(name = "orchestrator-examples")]
#[command(about = "Run sample workflows using orchestrator-core")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Personal finance: bank statement CSV -> Excel (spending by category and month).
    ExpenseReport {
        /// Path to bank statement CSV (default: bundled sample).
        #[arg(short, long)]
        statement: Option<String>,
        /// Output Excel path (default: ./expense_report.xlsx).
        #[arg(short, long, default_value = "./expense_report.xlsx")]
        output: String,
    },
    /// Stock report: CSV -> Polars pivot -> Excel.
    StockReport {
        /// Path to stock CSV (default: bundled sample).
        #[arg(short, long)]
        csv: Option<String>,
        /// Output Excel path (default: ./stock_report.xlsx).
        #[arg(short, long, default_value = "./stock_report.xlsx")]
        output: String,
    },
    /// Cyclic workflow demo: entry -> A -> B -> A (cycle) -> sink. Demonstrates cycle handling.
    CyclicDemo,
    /// Read a file and pass through echo (multi-block chain).
    PrintReadme {
        /// Path to file to read (default: ../../README.md).
        #[arg(default_value = "../../README.md")]
        path: String,
    },
    /// Minimal workflow: single file_read block.
    SingleFileRead {
        /// Path to file to read.
        path: String,
    },
    /// Copy files in parallel: for each (src, dst) pair, file_read -> file_write.
    CopyFiles {
        /// Pairs as "src1:dst1" "src2:dst2" (e.g. "a.txt:b.txt").
        pairs: Vec<String>,
    },
    /// Invoice line processor: read file -> Split -> process first line. Optional cron (daily).
    InvoiceLineProcessor {
        /// Path to invoice lines file (default: bundled sample).
        #[arg(short, long)]
        input: Option<String>,
    },
    /// Price drop checker: Trigger -> Delay -> fetch price (HTTP or stub) -> Merge -> notify file.
    PriceDropChecker {
        /// Output path for notify file (default: ./price_drop_notify.txt).
        #[arg(short, long, default_value = "./price_drop_notify.txt")]
        output: String,
        /// Price API URL (when set, uses HTTP block; otherwise stub).
        #[arg(short, long)]
        url: Option<String>,
        /// Stub price for demo when no URL (default: 85.0).
        #[arg(short, long, default_value = "85.0")]
        price_stub: f64,
    },
    /// News aggregator: Trigger -> parallel HTTP -> Merge -> report.
    NewsAggregator {
        /// Comma-separated URLs to fetch (default: example.com, example.org).
        #[arg(short, long)]
        urls: Option<String>,
    },
    /// Retry until success: check -> Conditional (200?) -> sink. Cycle needs runtime branch selection.
    RetryUntilSuccess {
        /// Stub status for demo: "200" or "retry" (default: 200).
        #[arg(short, long, default_value = "200")]
        stub_status: String,
    },
    /// Child workflow demo: Trigger -> child_workflow(echo) -> echo. Shows composition.
    ChildWorkflowDemo,
    /// Data-flow demo: Merge(Trigger, HTTP) -> Split -> echo. Demonstrates distant data flow.
    ContextPayloadDemo {
        /// URL to fetch (default: example.com).
        #[arg(short, long)]
        url: Option<String>,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            println!("=== Personal finance: where is the money going? ===\n");
            let statement_path = workflows::expense_report::default_statement_path();
            let out_path = Path::new("./expense_report.xlsx");
            let written = workflows::expense_report::run_expense_report_workflow(&statement_path, out_path)?;
            println!(
                "Expense report written to {}.\n  Open it to see spending by category and monthly totals.",
                written.display()
            );
        }
        Some(Commands::ExpenseReport { statement, output }) => {
            println!("=== Personal finance: where is the money going? ===\n");
            let statement_path = statement
                .map(std::path::PathBuf::from)
                .unwrap_or_else(workflows::expense_report::default_statement_path);
            let out_path = Path::new(&output);
            let written = workflows::expense_report::run_expense_report_workflow(&statement_path, out_path)?;
            println!(
                "Expense report written to {}.\n  Open it to see spending by category and monthly totals.",
                written.display()
            );
        }
        Some(Commands::StockReport { csv, output }) => {
            let csv_path = csv
                .map(std::path::PathBuf::from)
                .unwrap_or_else(workflows::stock_report::default_csv_path);
            let out_path = Path::new(&output);
            workflows::stock_report::run_stock_report_workflow(&csv_path, out_path)?;
            println!("Stock report written to {}.", out_path.display());
        }
        Some(Commands::CyclicDemo) => {
            let result = workflows::cyclic_demo_workflow()?;
            println!("Cyclic demo completed. Sink output: {:?}", result);
        }
        Some(Commands::PrintReadme { path }) => {
            let output = workflows::print_readme_workflow(&path)?;
            println!("{}", output);
        }
        Some(Commands::SingleFileRead { path }) => {
            let output = workflows::single_file_read_workflow(&path)?;
            println!("{}", output);
        }
        Some(Commands::CopyFiles { pairs }) => {
            if pairs.is_empty() {
                eprintln!("copy-files requires at least one pair (e.g. \"src.txt:dst.txt\")");
                std::process::exit(1);
            }
            let parsed: Vec<(&str, &str)> = pairs
                .iter()
                .map(|s| {
                    let mut split = s.splitn(2, ':');
                    let src = split.next().unwrap_or("");
                    let dst = split.next().unwrap_or("");
                    (src, dst)
                })
                .collect();
            workflows::copy_files_workflow(&parsed)?;
            println!("Copy completed.");
        }
        Some(Commands::InvoiceLineProcessor { input }) => {
            let path = input
                .map(std::path::PathBuf::from)
                .unwrap_or_else(workflows::invoice_line_processor::default_invoice_path);
            let out = workflows::run_invoice_line_processor_workflow(&path)?;
            println!("Invoice line processor output: {}", out);
        }
        Some(Commands::PriceDropChecker {
            output,
            url,
            price_stub,
        }) => {
            let written =
                workflows::run_price_drop_checker_workflow(&output, url.as_deref(), price_stub)?;
            println!("Price drop notify written to {}", written);
        }
        Some(Commands::NewsAggregator { urls }) => {
            let url_list = urls
                .as_ref()
                .map(|s| s.split(',').map(str::trim).map(String::from).collect());
            let out = workflows::run_news_aggregator_workflow(url_list)?;
            println!("News aggregator report:\n{}", out);
        }
        Some(Commands::RetryUntilSuccess { stub_status }) => {
            let out = workflows::run_retry_until_success_workflow(&stub_status)?;
            println!("Retry until success output: {}", out);
        }
        Some(Commands::ChildWorkflowDemo) => {
            let out = workflows::child_workflow_demo_workflow()?;
            println!("Child workflow demo output: {}", out);
        }
        Some(Commands::ContextPayloadDemo { url }) => {
            let out = workflows::run_context_payload_demo_workflow(url.as_deref())?;
            println!("Context/payload demo output: {}", out);
        }
    }

    Ok(())
}
