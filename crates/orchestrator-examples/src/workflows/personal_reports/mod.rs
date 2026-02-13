//! Personal reports workflow: trigger → ListDirectory(daily_notes) + ListDirectory(reports)
//! → SelectFirst → FileRead / ReadPaths → Combine → ReportTransform → SplitByKeys
//! → 4× FileWrite + ChildWorkflow(email) → Combine → NextDayNote → FileWrite.

mod blocks;

use std::path::Path;

use std::sync::Arc;

use orchestrator_blocks::{
    Block, PulldownMarkdownRenderer, register_markdown_to_html, registry_with_mailer,
};
use orchestrator_core::{BlockRegistry, RunError, Workflow, WorkflowDefinition};

use blocks::{LettreMailer, NextDayNoteBlock, ReadPathsBlock, ReportTransformBlock, StubMailer};

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
    std::fs::write(
        template,
        "<!DOCTYPE html><html><body>\n{{{body}}}\n</body></html>\n",
    )?;
    Ok(())
}

fn make_registry(
    mailer: Arc<dyn orchestrator_blocks::SendEmail>,
    markdown_renderer: Arc<dyn orchestrator_blocks::MarkdownToHtml>,
) -> BlockRegistry {
    let mut r = registry_with_mailer(mailer);
    register_markdown_to_html(&mut r, markdown_renderer);
    r.register_custom("read_paths", |_, _input_from| Ok(Box::new(ReadPathsBlock)));
    r.register_custom("report_transform", |_, _input_from| {
        Ok(Box::new(ReportTransformBlock))
    });
    r.register_custom("next_day_note", |_, _input_from| {
        Ok(Box::new(NextDayNoteBlock))
    });
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
    let entry = Block::custom_transform(None::<String>);
    let markdown = Block::markdown_to_html();
    let file_read = Block::file_read(Some(template_path.to_string_lossy().as_ref()));
    let combine = Block::combine(vec!["body".to_string(), "template".to_string()]);
    let handlebars = Block::template_handlebars(None::<String>, None);
    let send_email = Block::send_email(to_email, Some(subject));
    w.link(&entry, &markdown);
    w.link(&entry, &file_read);
    w.link(&markdown, &combine);
    w.link(&file_read, &combine);
    w.link(&combine, &handlebars);
    w.link(&handlebars, &send_email);
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
    let mailer: Arc<dyn orchestrator_blocks::SendEmail> = match LettreMailer::from_env() {
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
    let cron = Block::cron("0 */1 * * * * *");
    let list_notes =
        Block::list_directory_force_config(Some(daily_notes_path.to_string_lossy().as_ref()));
    let list_reports =
        Block::list_directory_force_config(Some(reports_path.to_string_lossy().as_ref()));

    w.link(&cron, &list_notes);
    w.link(&cron, &list_reports);

    let select = Block::select_first(None::<String>);
    let read_note = Block::file_read(None::<&str>);
    let read_paths = Block::custom("read_paths", serde_json::json!({}));

    w.link(&list_notes, &select);
    w.link(&select, &read_note);
    w.link(&list_reports, &read_paths);

    let combine1 = Block::combine(vec!["daily_note".to_string(), "reports".to_string()]);
    w.link(&read_note, &combine1);
    w.link(&read_paths, &combine1);

    let report_transform = Block::custom("report_transform", serde_json::json!({}));
    w.link(&combine1, &report_transform);

    let split = Block::split_by_keys(vec![
        "daily".to_string(),
        "weekly".to_string(),
        "monthly".to_string(),
        "yearly".to_string(),
        "consolidated".to_string(),
        "consolidated_md".to_string(),
    ]);
    w.link(&report_transform, &split);

    let daily_path = reports_path.join("daily.md");
    let weekly_path = reports_path.join("weekly.md");
    let monthly_path = reports_path.join("monthly.md");
    let yearly_path = reports_path.join("yearly.md");
    let consolidated_path = reports_path.join("consolidated.md");

    let write_daily = Block::file_write(Some(daily_path.to_string_lossy().as_ref()));
    let write_weekly = Block::file_write(Some(weekly_path.to_string_lossy().as_ref()));
    let write_monthly = Block::file_write(Some(monthly_path.to_string_lossy().as_ref()));
    let write_yearly = Block::file_write(Some(yearly_path.to_string_lossy().as_ref()));
    let child = Block::child_workflow(child_def);
    let write_consolidated = Block::file_write(Some(consolidated_path.to_string_lossy().as_ref()));

    w.link(&split, &write_daily);
    w.link(&split, &write_weekly);
    w.link(&split, &write_monthly);
    w.link(&split, &write_yearly);
    w.link(&split, &child);
    w.link(&split, &write_consolidated);

    let combine2 = Block::combine(vec![
        "daily_out".to_string(),
        "weekly_out".to_string(),
        "monthly_out".to_string(),
        "yearly_out".to_string(),
        "email_out".to_string(),
    ]);
    w.link(&write_daily, &combine2);
    w.link(&write_weekly, &combine2);
    w.link(&write_monthly, &combine2);
    w.link(&write_yearly, &combine2);
    w.link(&child, &combine2);

    let next_day = Block::custom("next_day_note", serde_json::json!({}));
    w.link(&combine2, &next_day);

    let write_next = Block::file_write(Some(next_day_note_path.to_string_lossy().as_ref()));
    w.link(&next_day, &write_next);

    w.run()?;
    Ok(())
}
