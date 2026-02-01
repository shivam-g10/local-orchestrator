//! Personal reports workflow: trigger → ListDirectory(daily_notes) + ListDirectory(reports)
//! → SelectFirst → FileRead / ReadPaths → Combine → ReportTransform → SplitByKeys
//! → 4× FileWrite + ChildWorkflow(email) → Combine → NextDayNote → FileWrite.

mod blocks;

use std::path::Path;

use std::sync::Arc;

use orchestrator_core::{BlockRegistry, RunError, Workflow, WorkflowDefinition};
use orchestrator_blocks::{registry_with_mailer, register_markdown_to_html, Block, PulldownMarkdownRenderer};

use blocks::{
    LettreMailer, ReadPathsBlock, ReportTransformBlock, NextDayNoteBlock, StubMailer,
};

/// Ensure dummy data exists under `base_path`: one daily note (today), reports/*.md with a year's metrics, email_template.hbs.
pub fn ensure_dummy_data(base_path: &Path) -> Result<(), std::io::Error> {
    let daily_notes = base_path.join("daily_notes");
    let reports = base_path.join("reports");
    std::fs::create_dir_all(&daily_notes)?;
    std::fs::create_dir_all(&reports)?;

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let today_note = format!(
        "# {today}\n\n- [x] Done task\n- [ ] Missed\n- [ ] Delayed\n- [ ] Skipped\n",
        today = today
    );
    std::fs::write(daily_notes.join(format!("{}.md", today)), today_note)?;

    // One of each report type: metrics-only content as if accumulated over a year.
    std::fs::write(
        reports.join("daily.md"),
        "Completed: 0 | Open: 0 | Repeated: 0\n",
    )?;
    std::fs::write(
        reports.join("weekly.md"),
        "Completed: 12 | Open: 5 | Repeated: 1\n",
    )?;
    std::fs::write(
        reports.join("monthly.md"),
        "Completed: 48 | Open: 8 | Repeated: 3\n",
    )?;
    std::fs::write(
        reports.join("yearly.md"),
        "Completed: 420 | Open: 15 | Repeated: 10\n",
    )?;
    std::fs::write(
        reports.join("consolidated.md"),
        "# Consolidated\n\n## Daily - 1970-01-01\nCompleted: 0 | Open: 0 | Repeated: 0\n\n## Weekly - 1970-01-01 - 1970-01-07\nCompleted: 0 | Open: 0 | Repeated: 0\n\n## Monthly - 1970-01\nCompleted: 0 | Open: 0 | Repeated: 0\n\n## Yearly - 1970\nCompleted: 0 | Open: 0 | Repeated: 0\n\n## Total till date\nCompleted: 420 | Open: 15 | Repeated: 10\n",
    )?;

    let template = base_path.join("email_template.hbs");
    std::fs::write(template, "<!DOCTYPE html><html><body>\n{{{body}}}\n</body></html>\n")?;
    Ok(())
}

fn make_registry(
    mailer: Arc<dyn orchestrator_blocks::SendEmail>,
    markdown_renderer: Arc<dyn orchestrator_blocks::MarkdownToHtml>,
) -> BlockRegistry {
    let mut r = registry_with_mailer(mailer);
    register_markdown_to_html(&mut r, markdown_renderer);
    r.register_custom("read_paths", |_| Ok(Box::new(ReadPathsBlock)));
    r.register_custom("report_transform", |_| Ok(Box::new(ReportTransformBlock)));
    r.register_custom("next_day_note", |_| Ok(Box::new(NextDayNoteBlock)));
    r
}

/// Build child workflow definition (email): entry (identity) → MarkdownToHtml + FileRead → Combine → TemplateHandlebars → SendEmail.
fn build_email_child_definition(
    template_path: &Path,
    to_email: &str,
    subject: &str,
    mailer: Arc<dyn orchestrator_blocks::SendEmail>,
    markdown_renderer: Arc<dyn orchestrator_blocks::MarkdownToHtml>,
) -> WorkflowDefinition {
    let registry = make_registry(mailer, markdown_renderer);
    let mut w = Workflow::with_registry(registry);
    let entry_id = w.add(Block::custom_transform(None::<String>));
    let markdown_id = w.add(Block::markdown_to_html());
    let file_read_id = w.add(Block::file_read(Some(template_path.to_string_lossy().as_ref())));
    let combine_id = w.add(Block::combine(vec!["body".to_string(), "template".to_string()]));
    let handlebars_id = w.add(Block::template_handlebars(None::<String>, None));
    let send_email_id = w.add(Block::send_email(to_email, Some(subject)));
    w.link(entry_id, markdown_id);
    w.link(entry_id, file_read_id);
    w.link(markdown_id, combine_id);
    w.link(file_read_id, combine_id);
    w.link(combine_id, handlebars_id);
    w.link(handlebars_id, send_email_id);
    w.into_definition()
}

/// Run the personal reports workflow. Paths can be dirs/files under a base (e.g. from ensure_dummy_data).
/// `email_out_path` is where the stub mailer writes the email HTML when SMTP is not configured.
/// When SMTP and DEFAULT_SENDER are set in env (or .env), uses POC-style lettre mailer instead.
pub fn run_personal_reports_workflow(
    daily_notes_path: &Path,
    reports_path: &Path,
    template_path: &Path,
    next_day_note_path: &Path,
    email_out_path: &Path,
) -> Result<(), RunError> {
    let mailer: Arc<dyn orchestrator_blocks::SendEmail> =
        match LettreMailer::from_env() {
            Ok(m) => Arc::new(m),
            Err(_) => Arc::new(StubMailer {
                output_path: email_out_path.to_path_buf(),
            }),
        };
    let markdown_renderer: Arc<dyn orchestrator_blocks::MarkdownToHtml> =
        Arc::new(PulldownMarkdownRenderer);
    let child_def = build_email_child_definition(
        template_path,
        "user@example.com",
        "Personal Reports Consolidated",
        Arc::clone(&mailer),
        Arc::clone(&markdown_renderer),
    );
    let registry = make_registry(mailer, markdown_renderer);

    let mut w = Workflow::with_registry(registry);

    // Cron 0.15 uses 7 fields: sec min hour day month day_of_week year. E.g. every 5 min:
    let cron_id = w.add(Block::cron("0 */1 * * * * *"));
    let list_notes_id = w.add(Block::list_directory_force_config(Some(daily_notes_path.to_string_lossy().as_ref())));
    let list_reports_id = w.add(Block::list_directory_force_config(Some(reports_path.to_string_lossy().as_ref())));

    w.link(cron_id, list_notes_id);
    w.link(cron_id, list_reports_id);

    let select_id = w.add(Block::select_first(None::<String>));
    let read_note_id = w.add(Block::file_read(None::<&str>));
    let read_paths_id = w.add_custom("read_paths", serde_json::json!({}))?;

    w.link(list_notes_id, select_id);
    w.link(select_id, read_note_id);
    w.link(list_reports_id, read_paths_id);

    let combine1_id = w.add(Block::combine(vec!["daily_note".to_string(), "reports".to_string()]));
    w.link(read_note_id, combine1_id);
    w.link(read_paths_id, combine1_id);

    let report_transform_id = w.add_custom("report_transform", serde_json::json!({}))?;
    w.link(combine1_id, report_transform_id);

    let split_id = w.add(Block::split_by_keys(vec![
        "daily".to_string(),
        "weekly".to_string(),
        "monthly".to_string(),
        "yearly".to_string(),
        "consolidated".to_string(),
        "consolidated_md".to_string(),
    ]));
    w.link(report_transform_id, split_id);

    let daily_path = reports_path.join("daily.md");
    let weekly_path = reports_path.join("weekly.md");
    let monthly_path = reports_path.join("monthly.md");
    let yearly_path = reports_path.join("yearly.md");
    let consolidated_path = reports_path.join("consolidated.md");

    let write_daily_id = w.add(Block::file_write(Some(daily_path.to_string_lossy().as_ref())));
    let write_weekly_id = w.add(Block::file_write(Some(weekly_path.to_string_lossy().as_ref())));
    let write_monthly_id = w.add(Block::file_write(Some(monthly_path.to_string_lossy().as_ref())));
    let write_yearly_id = w.add(Block::file_write(Some(yearly_path.to_string_lossy().as_ref())));
    let child_id = w.add(Block::child_workflow(child_def));
    let write_consolidated_id = w.add(Block::file_write(Some(consolidated_path.to_string_lossy().as_ref())));

    w.link(split_id, write_daily_id);
    w.link(split_id, write_weekly_id);
    w.link(split_id, write_monthly_id);
    w.link(split_id, write_yearly_id);
    w.link(split_id, child_id);
    w.link(split_id, write_consolidated_id);

    let combine2_id = w.add(Block::combine(vec![
        "daily_out".to_string(),
        "weekly_out".to_string(),
        "monthly_out".to_string(),
        "yearly_out".to_string(),
        "email_out".to_string(),
    ]));
    w.link(write_daily_id, combine2_id);
    w.link(write_weekly_id, combine2_id);
    w.link(write_monthly_id, combine2_id);
    w.link(write_yearly_id, combine2_id);
    w.link(child_id, combine2_id);

    let next_day_id = w.add_custom("next_day_note", serde_json::json!({}))?;
    w.link(combine2_id, next_day_id);

    let write_next_id = w.add(Block::file_write(Some(next_day_note_path.to_string_lossy().as_ref())));
    w.link(next_day_id, write_next_id);

    w.run()?;
    Ok(())
}
