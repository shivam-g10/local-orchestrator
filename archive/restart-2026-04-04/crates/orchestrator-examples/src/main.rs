//! Sample runner CLI: choose and run a workflow from the examples.

mod workflows;

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "orchestrator-examples")]
#[command(about = "Run sample workflows using orchestrator-core")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Personal reports: daily notes + reports dir -> combine -> transform -> split -> file writes + email child.
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
    /// Trial activation nudge: trigger -> fetch inactive trials -> render nudge -> send email with on_error child logging.
    TrialActivationNudge {
        /// Base directory for workflow data and outputs.
        #[arg(long)]
        data_dir: Option<String>,
        /// Endpoint URL for trial data.
        #[arg(long, default_value = "https://internal/trials/not-activated")]
        endpoint: String,
        /// Recipient email for the nudge.
        #[arg(long, default_value = "growth@company.com")]
        to: String,
        /// Subject line for the nudge.
        #[arg(long, default_value = "Trial Activation Nudge")]
        subject: String,
        /// Cron expression (used only with --use-cron).
        #[arg(long, default_value = "0 */15 * * * * *")]
        cron: String,
        /// Run as a recurring cron workflow instead of a single one-shot run.
        #[arg(long, default_value_t = false)]
        use_cron: bool,
        /// Override trials payload file path used by the built-in mock requester.
        #[arg(long)]
        trials_payload_file: Option<String>,
        /// Override nudge template path.
        #[arg(long)]
        nudge_template: Option<String>,
        /// Override error template path.
        #[arg(long)]
        error_template: Option<String>,
        /// Override email preview output path.
        #[arg(long)]
        email_out: Option<String>,
        /// Override error log output path.
        #[arg(long)]
        error_log: Option<String>,
    },
}

fn default_personal_reports_data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("personal_reports")
}

fn default_ai_news_digest_data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("ai_news_digest")
}

fn default_trial_activation_nudge_data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("trial_activation_nudge")
}

fn configure_observability_defaults() {
    if std::env::var("ORCHESTRATOR_LOG_LEVEL").is_err() {
        // SAFETY: This CLI sets env vars before runtime startup in a single-threaded setup path.
        unsafe {
            std::env::set_var("ORCHESTRATOR_LOG_LEVEL", "info");
        }
    }
    if std::env::var("ORCHESTRATOR_OBSERVABILITY_ENABLED").is_err()
        && std::env::var("ORCHESTRATOR_OBSERVABILITY").is_err()
    {
        // SAFETY: This CLI sets env vars before runtime startup in a single-threaded setup path.
        unsafe {
            std::env::set_var("ORCHESTRATOR_OBSERVABILITY_ENABLED", "1");
        }
    }
}

fn parse_bool_env(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "enabled" => Some(true),
        "0" | "false" | "no" | "off" | "disabled" => Some(false),
        _ => None,
    }
}

fn observability_enabled_from_env() -> bool {
    for key in [
        "ORCHESTRATOR_OBSERVABILITY_ENABLED",
        "ORCHESTRATOR_OBSERVABILITY",
    ] {
        if let Ok(value) = std::env::var(key) {
            return parse_bool_env(&value).unwrap_or(true);
        }
    }
    true
}

fn observability_log_path_from_env() -> Option<PathBuf> {
    std::env::var("ORCHESTRATOR_JSON_LOG_PATH")
        .ok()
        .map(PathBuf::from)
}

fn observability_level_from_env() -> String {
    std::env::var("ORCHESTRATOR_LOG_LEVEL").unwrap_or_else(|_| "info".to_string())
}

fn print_observability_status(suggested_log_path: &Path) {
    let enabled = observability_enabled_from_env();
    let level = observability_level_from_env();
    let log_path = observability_log_path_from_env();
    match (enabled, log_path) {
        (true, Some(path)) => {
            println!(
                "Observability: enabled (level={}, file={})",
                level,
                path.display()
            );
        }
        (true, None) => {
            println!(
                "Observability: enabled (level={}, console text on stdout). Set ORCHESTRATOR_JSON_LOG_PATH={} to write JSONL to file.",
                level,
                suggested_log_path.display()
            );
        }
        (false, Some(path)) => {
            println!(
                "Observability: disabled via env (level={}, file={})",
                level,
                path.display()
            );
        }
        (false, None) => {
            println!("Observability: disabled via env (level={}, stdout)", level);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::PersonalReports {
            data_dir,
            daily_notes_dir,
            reports_dir,
            template_path,
            next_day_note_path,
            email_out,
        }) => {
            let base = data_dir
                .map(PathBuf::from)
                .unwrap_or_else(default_personal_reports_data_dir);
            let observability_log_path = base.join("logs").join("orchestrator.jsonl");
            configure_observability_defaults();
            print_observability_status(&observability_log_path);
            workflows::ensure_dummy_data(&base)?;
            let daily_notes = daily_notes_dir
                .map(PathBuf::from)
                .unwrap_or_else(|| base.join("daily_notes"));
            let reports = reports_dir
                .map(PathBuf::from)
                .unwrap_or_else(|| base.join("reports"));
            let template = template_path
                .map(PathBuf::from)
                .unwrap_or_else(|| base.join("email_template.hbs"));
            let next_day = next_day_note_path
                .map(PathBuf::from)
                .unwrap_or_else(|| base.join("next_day_note.md"));
            let email_out = email_out
                .map(PathBuf::from)
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
                .map(PathBuf::from)
                .unwrap_or_else(default_ai_news_digest_data_dir);
            workflows::ensure_ai_news_digest_dummy_data(&base)?;
            let feeds = feeds_file
                .map(PathBuf::from)
                .unwrap_or_else(|| base.join("feeds.txt"));
            let prompt = prompt_file
                .map(PathBuf::from)
                .unwrap_or_else(|| base.join("prompt.md"));
            let email_tpl = email_template
                .map(PathBuf::from)
                .unwrap_or_else(|| base.join("email_template.hbs"));
            let templates = template_dir
                .map(PathBuf::from)
                .unwrap_or_else(|| base.join("templates"));
            let state = state_dir
                .map(PathBuf::from)
                .unwrap_or_else(|| base.join("state"));
            let logs = logs_dir
                .map(PathBuf::from)
                .unwrap_or_else(|| base.join("logs"));
            let observability_log_path = logs.join("orchestrator.jsonl");
            configure_observability_defaults();
            print_observability_status(&observability_log_path);
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
        Some(Commands::TrialActivationNudge {
            data_dir,
            endpoint,
            to,
            subject,
            cron,
            use_cron,
            trials_payload_file,
            nudge_template,
            error_template,
            email_out,
            error_log,
        }) => {
            let base = data_dir
                .map(PathBuf::from)
                .unwrap_or_else(default_trial_activation_nudge_data_dir);
            let observability_log_path = base.join("logs").join("orchestrator.jsonl");
            configure_observability_defaults();
            print_observability_status(&observability_log_path);
            workflows::ensure_trial_activation_nudge_dummy_data(&base)?;

            let trials_payload = trials_payload_file
                .map(PathBuf::from)
                .unwrap_or_else(|| base.join("trials_not_activated.json"));
            let nudge_template = nudge_template
                .map(PathBuf::from)
                .unwrap_or_else(|| base.join("nudge_template.hbs"));
            let error_template = error_template
                .map(PathBuf::from)
                .unwrap_or_else(|| base.join("error_template.hbs"));
            let email_out = email_out
                .map(PathBuf::from)
                .unwrap_or_else(|| base.join("email_preview.html"));
            let error_log = error_log
                .map(PathBuf::from)
                .unwrap_or_else(|| base.join("error_log.jsonl"));

            workflows::run_trial_activation_nudge_workflow(
                workflows::TrialActivationNudgeWorkflowConfig {
                    endpoint_url: &endpoint,
                    to_email: &to,
                    subject: &subject,
                    cron_expr: &cron,
                    use_cron,
                    nudge_template_path: &nudge_template,
                    error_template_path: &error_template,
                    trials_payload_path: &trials_payload,
                    email_out_path: &email_out,
                    error_log_path: &error_log,
                },
            )?;

            println!(
                "Trial activation nudge workflow completed. Email preview: {} | Error log: {}",
                email_out.display(),
                error_log.display()
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
