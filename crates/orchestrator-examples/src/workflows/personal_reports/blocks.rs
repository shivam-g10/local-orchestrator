//! Custom blocks for the personal_reports example: ReadPaths, ReportTransform, NextDayNote,
//! StubMailer, LettreMailer (POC-style SMTP), TriggerOnce.

use std::path::Path;
use std::str::FromStr;

use lettre::{
    Address, Message, SmtpTransport, Transport,
    message::{Mailbox, header::ContentType},
    transport::smtp::PoolConfig,
};

use orchestrator_blocks::SendEmail;
use orchestrator_core::block::{
    BlockError, BlockExecutionResult, BlockExecutor, BlockInput, BlockOutput,
};

/// ReadPathsBlock: input = List of paths, reads all files, output = JSON { "contents": [ { "path", "content" } ] }.
pub struct ReadPathsBlock;

impl BlockExecutor for ReadPathsBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        let paths: Vec<String> = match &input {
            BlockInput::List { items } => items.clone(),
            BlockInput::Json(v) => {
                let arr = v.as_array().ok_or_else(|| {
                    BlockError::Other(
                        "read_paths expects List or JSON array of path strings".into(),
                    )
                })?;
                let items: Result<Vec<String>, _> = arr
                    .iter()
                    .map(|v| {
                        v.as_str().map(String::from).ok_or_else(|| {
                            BlockError::Other("path elements must be strings".into())
                        })
                    })
                    .collect();
                items?
            }
            BlockInput::Error { message } => return Err(BlockError::Other(message.clone())),
            _ => {
                return Err(BlockError::Other(
                    "read_paths expects List or JSON array of paths".into(),
                ));
            }
        };
        let mut contents = Vec::new();
        for p in &paths {
            let s = std::fs::read_to_string(Path::new(p))
                .map_err(|e| BlockError::Other(format!("read_paths {}: {}", p, e)))?;
            contents.push(serde_json::json!({ "path": p, "content": s }));
        }
        let value = serde_json::json!({ "contents": contents });
        Ok(BlockExecutionResult::Once(BlockOutput::Json { value }))
    }
}

use std::collections::HashMap;

use chrono::{Datelike, Local, Weekday};

/// Task counts: completed, open, repeated.
#[derive(Debug, Default, Clone, Copy)]
struct TaskCounts {
    completed: usize,
    open: usize,
    repeated: usize,
}

impl TaskCounts {
    fn add(self, other: TaskCounts) -> TaskCounts {
        TaskCounts {
            completed: self.completed + other.completed,
            open: self.open + other.open,
            repeated: self.repeated + other.repeated,
        }
    }
}

fn parse_task_counts_from_note(text: &str) -> TaskCounts {
    let mut completed = 0usize;
    let mut open = 0usize;
    let mut task_texts: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("- [x]") || line.starts_with("- [X]") {
            completed += 1;
            let t = line
                .strip_prefix("- [x]")
                .or_else(|| line.strip_prefix("- [X]"))
                .unwrap_or(line)
                .trim();
            task_texts.push(t.to_string());
        } else if line.starts_with("- [ ]") {
            open += 1;
            let t = line.strip_prefix("- [ ]").unwrap_or(line).trim();
            task_texts.push(t.to_string());
        }
    }
    let repeated = if task_texts.len() > 1 {
        let mut freq: HashMap<String, usize> = HashMap::new();
        for t in &task_texts {
            *freq.entry(t.clone()).or_insert(0) += 1;
        }
        freq.values().filter(|&&c| c > 1).count()
    } else {
        0
    };
    TaskCounts {
        completed,
        open,
        repeated,
    }
}

/// Parse "Completed: N | Open: M | Repeated: R" from report text (supports **bold** or plain).
/// Uses the last line containing "Completed:" so multi-section content (e.g. headers) does not confuse parsing.
fn parse_metrics_from_report(text: &str) -> TaskCounts {
    let metrics_line = text.lines().rev().find(|l| l.contains("Completed:"));
    let line = match metrics_line {
        Some(l) => l,
        None => return TaskCounts::default(),
    };
    let mut completed = 0usize;
    let mut open = 0usize;
    let mut repeated = 0usize;
    for part in line.split('|') {
        let part = part.trim().trim_matches('*');
        if let Some(s) = part.strip_prefix("Completed:") {
            completed = s.trim().parse().unwrap_or(0);
        } else if let Some(s) = part.strip_prefix("Open:") {
            open = s.trim().parse().unwrap_or(0);
        } else if let Some(s) = part.strip_prefix("Repeated:") {
            repeated = s.trim().parse().unwrap_or(0);
        }
    }
    TaskCounts {
        completed,
        open,
        repeated,
    }
}

fn metrics_line(completed: usize, open: usize, repeated: usize) -> String {
    format!(
        "Completed: {} | Open: {} | Repeated: {}",
        completed, open, repeated
    )
}

/// Find content by path ending (e.g. "daily.md").
fn find_content(contents: &[(String, String)], path_ends: &str) -> Option<String> {
    contents
        .iter()
        .find(|(path, _)| path.ends_with(path_ends))
        .map(|(_, c)| c.clone())
}

/// ReportTransformBlock: input = JSON { "daily_note", "reports" }; output = JSON { "daily", "weekly", "monthly", "yearly", "consolidated" }.
/// Today's note → task counts. Daily = today only. Weekly/Monthly/Yearly = roll or replace by period. Consolidated = all sections + Total till date.
pub struct ReportTransformBlock;

impl BlockExecutor for ReportTransformBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        let (daily_note, reports_json) = match &input {
            BlockInput::Json(v) => {
                let daily_note = v
                    .get("daily_note")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                let reports = v.get("reports").cloned().unwrap_or(serde_json::Value::Null);
                (daily_note, reports)
            }
            BlockInput::Error { message } => return Err(BlockError::Other(message.clone())),
            _ => {
                return Err(BlockError::Other(
                    "report_transform expects Json { daily_note, reports }".into(),
                ));
            }
        };

        let today = Local::now().date_naive();
        let today_counts = parse_task_counts_from_note(&daily_note);

        let contents: Vec<(String, String)> = reports_json
            .get("contents")
            .and_then(|c| c.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| {
                        let path = x
                            .get("path")
                            .and_then(|p| p.as_str())
                            .unwrap_or("")
                            .to_string();
                        let content = x
                            .get("content")
                            .and_then(|c| c.as_str())
                            .map(String::from)?;
                        Some((path, content))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let _prev_daily_content = find_content(&contents, "daily.md");
        let prev_weekly_content = find_content(&contents, "weekly.md");
        let prev_monthly_content = find_content(&contents, "monthly.md");
        let prev_yearly_content = find_content(&contents, "yearly.md");
        let prev_consolidated_content = find_content(&contents, "consolidated.md");

        let prev_weekly = prev_weekly_content
            .as_deref()
            .map(parse_metrics_from_report)
            .unwrap_or_default();
        let prev_monthly = prev_monthly_content
            .as_deref()
            .map(parse_metrics_from_report)
            .unwrap_or_default();
        let prev_yearly = prev_yearly_content
            .as_deref()
            .map(parse_metrics_from_report)
            .unwrap_or_default();
        let prev_total = prev_consolidated_content
            .as_deref()
            .and_then(|c| c.split("## Total till date").nth(1))
            .and_then(|s| s.lines().find(|l| l.contains("Completed:")))
            .map(parse_metrics_from_report)
            .unwrap_or_default();

        let week_finished = today.weekday() == Weekday::Mon;
        let month_finished = today.day() == 1;
        let year_finished = today.month() == 1 && today.day() == 1;

        let weekly_counts = if week_finished {
            today_counts
        } else {
            prev_weekly.add(today_counts)
        };
        let monthly_counts = if month_finished {
            today_counts
        } else {
            prev_monthly.add(today_counts)
        };
        let yearly_counts = if year_finished {
            today_counts
        } else {
            prev_yearly.add(today_counts)
        };
        let total_counts = prev_total.add(today_counts);

        let days_back = today.weekday().num_days_from_monday();
        let week_start = (0..days_back).fold(today, |d, _| d.pred_opt().unwrap_or(d));
        let week_end = (0..6).fold(week_start, |d, _| d.succ_opt().unwrap_or(d));
        let week_range = format!("{} - {}", week_start, week_end);

        let date_str = today.format("%Y-%m-%d").to_string();
        let month_str = today.format("%Y-%m").to_string();
        let year_str = today.format("%Y").to_string();

        let daily = format!(
            "## Daily - {}\n{}",
            date_str,
            metrics_line(
                today_counts.completed,
                today_counts.open,
                today_counts.repeated
            )
        );
        let weekly = format!(
            "## Weekly - {}\n{}",
            week_range,
            metrics_line(
                weekly_counts.completed,
                weekly_counts.open,
                weekly_counts.repeated
            )
        );
        let monthly = format!(
            "## Monthly - {}\n{}",
            month_str,
            metrics_line(
                monthly_counts.completed,
                monthly_counts.open,
                monthly_counts.repeated
            )
        );
        let yearly = format!(
            "## Yearly - {}\n{}",
            year_str,
            metrics_line(
                yearly_counts.completed,
                yearly_counts.open,
                yearly_counts.repeated
            )
        );
        let total_section = format!(
            "## Total till date\n{}",
            metrics_line(
                total_counts.completed,
                total_counts.open,
                total_counts.repeated
            )
        );

        let consolidated = format!(
            "# Consolidated\n\n{}\n\n{}\n\n{}\n\n{}\n\n{}",
            daily, weekly, monthly, yearly, total_section
        );

        let value = serde_json::json!({
            "daily": daily,
            "weekly": weekly,
            "monthly": monthly,
            "yearly": yearly,
            "consolidated": consolidated,
            "consolidated_md": consolidated.clone(),
        });
        Ok(BlockExecutionResult::Once(BlockOutput::Json { value }))
    }
}

/// NextDayNoteBlock: input = Multi or Json from Combine (five outputs); output = single string (next-day note template).
pub struct NextDayNoteBlock;

fn output_to_string(o: &BlockOutput) -> String {
    match o {
        BlockOutput::Empty => String::new(),
        BlockOutput::String { value } => value.clone(),
        BlockOutput::Text { value } => value.clone(),
        BlockOutput::Json { value } => value.to_string(),
        BlockOutput::List { items } => items.join("\n"),
    }
}

impl BlockExecutor for NextDayNoteBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        let summary = match &input {
            BlockInput::Multi { outputs } => outputs
                .iter()
                .map(output_to_string)
                .collect::<Vec<_>>()
                .join("\n---\n"),
            BlockInput::Json(v) => v.to_string(),
            BlockInput::Error { message } => return Err(BlockError::Other(message.clone())),
            _ => {
                return Err(BlockError::Other(
                    "next_day_note expects Multi or Json".into(),
                ));
            }
        };
        let next_day = "Next day note placeholder\n---\n(empty tasks)\n";
        let body = format!("{}\n\n{}", next_day, summary);
        Ok(BlockExecutionResult::Once(BlockOutput::String {
            value: body,
        }))
    }
}

/// Stub mailer: writes HTML body to a file instead of sending email.
pub struct StubMailer {
    pub output_path: std::path::PathBuf,
}

impl SendEmail for StubMailer {
    fn send_email(
        &self,
        subject: &str,
        _to_name: &str,
        to_email: &str,
        body: String,
    ) -> Result<(), orchestrator_blocks::SendEmailError> {
        let header = format!("To: {}\nSubject: {}\n\n", to_email, subject);
        let full = header + &body;
        std::fs::write(&self.output_path, full)
            .map_err(|e| orchestrator_blocks::SendEmailError(format!("stub mailer: {}", e)))?;
        Ok(())
    }
}

/// Load .env from examples crate dir or current dir (for personal_reports).
fn load_env() {
    if let Ok(canon) = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".env")
        .canonicalize()
    {
        let _ = dotenvy::from_path(canon);
    }
    let _ = dotenvy::dotenv();
}

fn env_var(key: &str) -> String {
    std::env::var(key).unwrap_or_default()
}

/// POC-style mailer: SMTP via lettre, config from env (SMTP, SMTP_PORT, SMTP_UNAME, SMTP_PASS, DEFAULT_SENDER, DEFAULT_SENDER_NAME, DEPLOY_ENV).
/// Use when SMTP is set; otherwise use [`StubMailer`].
pub struct LettreMailer {
    transport: SmtpTransport,
    default_sender: Mailbox,
}

impl LettreMailer {
    /// Create mailer from env. Call after loading .env (e.g. [`load_env`]). Returns error if required env is missing.
    pub fn from_env() -> Result<Self, orchestrator_blocks::SendEmailError> {
        load_env();
        let smtp_server = env_var("SMTP");
        let smtp_port: u16 = env_var("SMTP_PORT").parse().unwrap_or(25);
        let smtp_user_name = env_var("SMTP_UNAME");
        let smtp_user_pass = env_var("SMTP_PASS");
        let default_sender = env_var("DEFAULT_SENDER");
        let default_sender_name = env_var("DEFAULT_SENDER_NAME");

        if smtp_server.is_empty() || default_sender.is_empty() {
            return Err(orchestrator_blocks::SendEmailError(
                "LettreMailer requires SMTP and DEFAULT_SENDER in env".into(),
            ));
        }

        let sender = Address::from_str(&default_sender)
            .map_err(|e| orchestrator_blocks::SendEmailError(e.to_string()))?;
        let default_sender_mailbox = Mailbox::new(
            if default_sender_name.is_empty() {
                None
            } else {
                Some(default_sender_name)
            },
            sender,
        );

        let pool_config = PoolConfig::new().min_idle(1);
        let transport = if env_var("DEPLOY_ENV") == "production" && !smtp_user_name.is_empty() {
            let smtps_url = format!(
                "smtps://{}:{}@{}:{}",
                smtp_user_name, smtp_user_pass, smtp_server, smtp_port
            );
            SmtpTransport::from_url(&smtps_url)
                .map_err(|e| orchestrator_blocks::SendEmailError(e.to_string()))?
                .pool_config(pool_config)
                .build()
        } else {
            SmtpTransport::builder_dangerous(&smtp_server)
                .pool_config(pool_config)
                .port(smtp_port)
                .build()
        };

        Ok(Self {
            transport,
            default_sender: default_sender_mailbox,
        })
    }
}

impl SendEmail for LettreMailer {
    fn send_email(
        &self,
        subject: &str,
        to_name: &str,
        to_email: &str,
        body: String,
    ) -> Result<(), orchestrator_blocks::SendEmailError> {
        let to = Address::from_str(to_email)
            .map_err(|e| orchestrator_blocks::SendEmailError(e.to_string()))?;
        let to_mailbox = Mailbox::new(Some(to_name.to_string()), to);
        let email = Message::builder()
            .to(to_mailbox)
            .reply_to(self.default_sender.clone())
            .from(self.default_sender.clone())
            .subject(subject)
            .header(ContentType::TEXT_HTML)
            .body(body)
            .map_err(|e| orchestrator_blocks::SendEmailError(e.to_string()))?;
        self.transport
            .send(&email)
            .map_err(|e| orchestrator_blocks::SendEmailError(e.to_string()))?;
        Ok(())
    }
}

/// TriggerOnceBlock: returns Once(Empty) so the workflow runs once (optional demo mode).
#[allow(dead_code)]
pub struct TriggerOnceBlock;

impl BlockExecutor for TriggerOnceBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockExecutionResult, BlockError> {
        let _ = input;
        Ok(BlockExecutionResult::Once(BlockOutput::Empty))
    }
}
