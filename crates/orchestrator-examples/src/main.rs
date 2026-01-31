//! Run workflow examples from the workflows module (one example per file).

mod workflows;

use crate::workflows::expense_report;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Personal finance: where is the money going? ===\n");
    let statement_path = expense_report::default_statement_path();
    let out_path = std::path::Path::new("./expense_report.xlsx");
    let written = expense_report::run_expense_report_workflow(&statement_path, out_path)?;
    println!(
        "Expense report written to {}.\n  Open it to see spending by category and monthly totals.",
        written.display()
    );
    Ok(())
}
