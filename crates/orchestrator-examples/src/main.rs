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
    //     /// Personal finance: bank statement CSV -> Excel (spending by category and month).
    //     ExpenseReport {
    //         /// Path to bank statement CSV (default: bundled sample).
    //         #[arg(short, long)]
    //         statement: Option<String>,
    //         /// Output Excel path (default: ./expense_report.xlsx).
    //         #[arg(short, long, default_value = "./expense_report.xlsx")]
    //         output: String,
    //     },
    //     /// Stock report: CSV -> Polars pivot -> Excel.
    //     StockReport {
    //         /// Path to stock CSV (default: bundled sample).
    //         #[arg(short, long)]
    //         csv: Option<String>,
    //         /// Output Excel path (default: ./stock_report.xlsx).
    //         #[arg(short, long, default_value = "./stock_report.xlsx")]
    //         output: String,
    //     },
    //     /// Cyclic workflow demo: entry -> A -> B -> A (cycle) -> sink. Demonstrates cycle handling.
    //     CyclicDemo,
    //     /// Read a file and pass through echo (multi-block chain).
    //     PrintReadme {
    //         /// Path to file to read (default: ../../README.md).
    //         #[arg(default_value = "../../README.md")]
    //         path: String,
    //     },
    //     /// Minimal workflow: single file_read block.
    //     SingleFileRead {
    //         /// Path to file to read.
    //         path: String,
    //     },
    //     /// Copy files in parallel: for each (src, dst) pair, file_read -> file_write.
    //     CopyFiles {
    //         /// Pairs as "src1:dst1" "src2:dst2" (e.g. "a.txt:b.txt").
    //         pairs: Vec<String>,
    //     },
    //     /// Invoice line processor: read file -> Split -> process first line. Optional cron (daily).
    //     InvoiceLineProcessor {
    //         /// Path to invoice lines file (default: bundled sample).
    //         #[arg(short, long)]
    //         input: Option<String>,
    //     },
    //     /// Price drop checker: Trigger -> Delay -> fetch price (HTTP or stub) -> Merge -> notify file.
    //     PriceDropChecker {
    //         /// Output path for notify file (default: ./price_drop_notify.txt).
    //         #[arg(short, long, default_value = "./price_drop_notify.txt")]
    //         output: String,
    //         /// Price API URL (when set, uses HTTP block; otherwise stub).
    //         #[arg(short, long)]
    //         url: Option<String>,
    //         /// Stub price for demo when no URL (default: 85.0).
    //         #[arg(short, long, default_value = "85.0")]
    //         price_stub: f64,
    //     },
    //     /// News aggregator: Trigger -> parallel HTTP -> Merge -> report.
    //     NewsAggregator {
    //         /// Comma-separated URLs to fetch (default: example.com, example.org).
    //         #[arg(short, long)]
    //         urls: Option<String>,
    //     },
    //     /// Retry until success: check -> Conditional (200?) -> sink. Cycle needs runtime branch selection.
    //     RetryUntilSuccess {
    //         /// Stub status for demo: "200" or "retry" (default: 200).
    //         #[arg(short, long, default_value = "200")]
    //         stub_status: String,
    //     },
    //     /// Child workflow demo: Trigger -> child_workflow(echo) -> echo. Shows composition.
    //     ChildWorkflowDemo,
    //     /// Data-flow demo: Merge(Trigger, HTTP) -> Split -> echo. Demonstrates distant data flow.
    //     ContextPayloadDemo {
    //         /// URL to fetch (default: example.com).
    //         #[arg(short, long)]
    //         url: Option<String>,
    //     },
    // }
    /// Personal reports: daily notes + reports dir → combine → transform → split → file writes + email child.
    PersonalReports {
        /// Base directory for data (daily_notes/, reports/, email_template.hbs). If set, dummy data is generated here.
        #[arg(long)]
        data_dir: Option<String>,
        /// Daily notes directory (default: data_dir/daily_notes or data/personal_reports/daily_notes).
        #[arg(long)]
        daily_notes_dir: Option<String>,
        /// Reports directory (default: data_dir/reports or data/personal_reports/reports).
        #[arg(long)]
        reports_dir: Option<String>,
        /// Email template path (default: data_dir/email_template.hbs or data/personal_reports/email_template.hbs).
        #[arg(long)]
        template_path: Option<String>,
        /// Next-day note output path (default: data_dir/next_day_note.md).
        #[arg(long)]
        next_day_note_path: Option<String>,
        /// Stub mailer output file path (default: data_dir/personal_reports_email.html).
        #[arg(long)]
        email_out: Option<String>,
    },
    /// AI news digest: RSS urls file -> split/fetch/parse -> dedupe -> AI markdown -> email + append logs.
    AiNewsDigest {
        /// Base directory for data/templates/state/logs. If set, dummy data is generated here.
        #[arg(long)]
        data_dir: Option<String>,
        /// Feeds file path (default: data_dir/feeds.txt).
        #[arg(long)]
        feeds_file: Option<String>,
        /// Prompt file path (default: data_dir/prompt.md).
        #[arg(long)]
        prompt_file: Option<String>,
        /// Email template path (default: data_dir/email_template.hbs).
        #[arg(long)]
        email_template: Option<String>,
        /// Audit templates directory (default: data_dir/templates).
        #[arg(long)]
        template_dir: Option<String>,
        /// State directory for sent_items.jsonl and runs.jsonl (default: data_dir/state).
        #[arg(long)]
        state_dir: Option<String>,
        /// Logs directory (default: data_dir/logs).
        #[arg(long)]
        logs_dir: Option<String>,
        /// Cron expression (7-field supported by cron block).
        #[arg(long, default_value = "0 */15 * * * * *")]
        cron: String,
        /// Recipient email.
        #[arg(long, default_value = "user@example.com")]
        to: String,
        /// Email subject.
        #[arg(long, default_value = "AI Inshorts Digest")]
        subject: String,
        /// AI model name.
        #[arg(long, default_value = "gpt-5-nano")]
        model: String,
        /// Optional API key env var name for AI block (default OPENAI_API_KEY).
        #[arg(long)]
        api_key_env: Option<String>,
        /// Max new items per run.
        #[arg(long, default_value = "20")]
        max_items: usize,
    },
}

fn default_personal_reports_data_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("personal_reports")
}

fn default_ai_news_digest_data_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("ai_news_digest")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        //         None => {
        //             println!("=== Personal finance: where is the money going? ===\n");
        //             let statement_path = workflows::expense_report::default_statement_path();
        //             let out_path = Path::new("./expense_report.xlsx");
        //             let written = workflows::expense_report::run_expense_report_workflow(&statement_path, out_path)?;
        //             println!(
        //                 "Expense report written to {}.\n  Open it to see spending by category and monthly totals.",
        //                 written.display()
        //             );
        //         }
        //         Some(Commands::ExpenseReport { statement, output }) => {
        //             println!("=== Personal finance: where is the money going? ===\n");
        //             let statement_path = statement
        //                 .map(std::path::PathBuf::from)
        //                 .unwrap_or_else(workflows::expense_report::default_statement_path);
        //             let out_path = Path::new(&output);
        //             let written = workflows::expense_report::run_expense_report_workflow(&statement_path, out_path)?;
        //             println!(
        //                 "Expense report written to {}.\n  Open it to see spending by category and monthly totals.",
        //                 written.display()
        //             );
        //         }
        //         Some(Commands::StockReport { csv, output }) => {
        //             let csv_path = csv
        //                 .map(std::path::PathBuf::from)
        //                 .unwrap_or_else(workflows::stock_report::default_csv_path);
        //             let out_path = Path::new(&output);
        //             workflows::stock_report::run_stock_report_workflow(&csv_path, out_path)?;
        //             println!("Stock report written to {}.", out_path.display());
        //         }
        //         Some(Commands::CyclicDemo) => {
        //             let result = workflows::cyclic_demo_workflow()?;
        //             println!("Cyclic demo completed. Sink output: {:?}", result);
        //         }
        //         Some(Commands::PrintReadme { path }) => {
        //             let output = workflows::print_readme_workflow(&path)?;
        //             println!("{}", output);
        //         }
        //         Some(Commands::SingleFileRead { path }) => {
        //             let output = workflows::single_file_read_workflow(&path)?;
        //             println!("{}", output);
        //         }
        //         Some(Commands::CopyFiles { pairs }) => {
        //             if pairs.is_empty() {
        //                 eprintln!("copy-files requires at least one pair (e.g. \"src.txt:dst.txt\")");
        //                 std::process::exit(1);
        //             }
        //             let parsed: Vec<(&str, &str)> = pairs
        //                 .iter()
        //                 .map(|s| {
        //                     let mut split = s.splitn(2, ':');
        //                     let src = split.next().unwrap_or("");
        //                     let dst = split.next().unwrap_or("");
        //                     (src, dst)
        //                 })
        //                 .collect();
        //             workflows::copy_files_workflow(&parsed)?;
        //             println!("Copy completed.");
        //         }
        //         Some(Commands::InvoiceLineProcessor { input }) => {
        //             let path = input
        //                 .map(std::path::PathBuf::from)
        //                 .unwrap_or_else(workflows::invoice_line_processor::default_invoice_path);
        //             let out = workflows::run_invoice_line_processor_workflow(&path)?;
        //             println!("Invoice line processor output: {}", out);
        //         }
        //         Some(Commands::PriceDropChecker {
        //             output,
        //             url,
        //             price_stub,
        //         }) => {
        //             let written =
        //                 workflows::run_price_drop_checker_workflow(&output, url.as_deref(), price_stub)?;
        //             println!("Price drop notify written to {}", written);
        //         }
        //         Some(Commands::NewsAggregator { urls }) => {
        //             let url_list = urls
        //                 .as_ref()
        //                 .map(|s| s.split(',').map(str::trim).map(String::from).collect());
        //             let out = workflows::run_news_aggregator_workflow(url_list)?;
        //             println!("News aggregator report:\n{}", out);
        //         }
        //         Some(Commands::RetryUntilSuccess { stub_status }) => {
        //             let out = workflows::run_retry_until_success_workflow(&stub_status)?;
        //             println!("Retry until success output: {}", out);
        //         }
        //         Some(Commands::ChildWorkflowDemo) => {
        //             let out = workflows::child_workflow_demo_workflow()?;
        //             println!("Child workflow demo output: {}", out);
        //         }
        //         Some(Commands::ContextPayloadDemo { url }) => {
        //             let out = workflows::run_context_payload_demo_workflow(url.as_deref())?;
        //             println!("Context/payload demo output: {}", out);
        //         }
        Some(Commands::PersonalReports {
            data_dir,
            daily_notes_dir,
            reports_dir,
            template_path,
            next_day_note_path,
            email_out,
        }) => {
            let base = data_dir
                .map(std::path::PathBuf::from)
                .unwrap_or_else(default_personal_reports_data_dir);
            workflows::ensure_dummy_data(&base)?;
            let daily_notes = daily_notes_dir
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| base.join("daily_notes"));
            let reports = reports_dir
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| base.join("reports"));
            let template = template_path
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| base.join("email_template.hbs"));
            let next_day = next_day_note_path
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| base.join("next_day_note.md"));
            let email_out = email_out
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| base.join("personal_reports_email.html"));
            workflows::run_personal_reports_workflow(
                &daily_notes,
                &reports,
                &template,
                &next_day,
                &email_out,
            )?;
            println!(
                "Personal reports workflow completed. Email stub written to {}",
                email_out.display()
            );
        }
        Some(Commands::AiNewsDigest {
            data_dir,
            feeds_file,
            prompt_file,
            email_template,
            template_dir,
            state_dir,
            logs_dir,
            cron,
            to,
            subject,
            model,
            api_key_env,
            max_items,
        }) => {
            let base = data_dir
                .map(std::path::PathBuf::from)
                .unwrap_or_else(default_ai_news_digest_data_dir);
            workflows::ensure_ai_news_digest_dummy_data(&base)?;
            let feeds = feeds_file
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| base.join("feeds.txt"));
            let prompt = prompt_file
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| base.join("prompt.md"));
            let email_tpl = email_template
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| base.join("email_template.hbs"));
            let templates = template_dir
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| base.join("templates"));
            let state = state_dir
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| base.join("state"));
            let logs = logs_dir
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| base.join("logs"));
            workflows::run_ai_news_digest_workflow(workflows::AiNewsDigestWorkflowConfig {
                feeds_file: &feeds,
                prompt_file: &prompt,
                email_template_path: &email_tpl,
                template_dir: &templates,
                state_dir: &state,
                logs_dir: &logs,
                cron_expr: &cron,
                to_email: &to,
                subject: &subject,
                model: &model,
                api_key_env: api_key_env.as_deref(),
                max_items,
            })?;
            println!(
                "AI news digest workflow completed. State/logs are under {}",
                base.display()
            );
        }
        None => {
            eprintln!(
                "No subcommand given. Use --help or 'personal-reports' to run the personal reports example."
            );
        }
    }

    Ok(())
}
