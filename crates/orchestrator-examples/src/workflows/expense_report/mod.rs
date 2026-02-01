//! Personal finance expense report: bank statement CSV → "where is the money going" Excel.
//! Self-contained under workflows/expense_report/ with sample data in data/sample_bank_statement.csv.
//!
//! ```text
//!   [CsvReader] --> [ExpenseSummaryExcel] --> output.xlsx
//! ```

mod blocks;

use std::path::Path;

use orchestrator_core::{BlockRegistry, RunError, Workflow};

use blocks::{
    CsvReaderBlock, CsvReaderConfig, ExpenseSummaryExcelBlock, ExpenseSummaryExcelConfig,
};

/// Path to the sample bank statement CSV (financial year of monthly expenses).
pub fn default_statement_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("workflows")
        .join("expense_report")
        .join("data")
        .join("sample_bank_statement.csv")
}

fn make_registry() -> BlockRegistry {
    let mut registry = BlockRegistry::default_with_builtins();
    registry.register_custom("csv_reader", |payload| {
        let path = payload
            .get("path")
            .and_then(|v| v.as_str())
            .map(String::from);
        Ok(Box::new(CsvReaderBlock::new(CsvReaderConfig { path })))
    });
    registry.register_custom("expense_summary_excel", |payload| {
        let date_col = payload
            .get("date_col")
            .and_then(|v| v.as_str())
            .unwrap_or("date")
            .to_string();
        let amount_col = payload
            .get("amount_col")
            .and_then(|v| v.as_str())
            .unwrap_or("amount")
            .to_string();
        let category_col = payload
            .get("category_col")
            .and_then(|v| v.as_str())
            .unwrap_or("category")
            .to_string();
        let output_path = payload
            .get("output_path")
            .and_then(|v| v.as_str())
            .map(std::path::PathBuf::from)
            .ok_or_else(|| orchestrator_core::block::BlockError::Other("output_path required".into()))?;
        Ok(Box::new(ExpenseSummaryExcelBlock::new(ExpenseSummaryExcelConfig {
            date_col,
            amount_col,
            category_col,
            output_path,
        })))
    });
    registry
}

/// Run the expense report workflow: read bank statement at `statement_path`,
/// aggregate expenses by category and by month, write Excel to `output_excel_path`.
/// Returns the path to the created Excel file.
pub fn run_expense_report_workflow(
    statement_path: impl AsRef<Path>,
    output_excel_path: impl AsRef<Path>,
) -> Result<std::path::PathBuf, RunError> {
    let statement_path = statement_path.as_ref();
    let output_path = output_excel_path.as_ref().to_path_buf();
    let registry = make_registry();
    let mut w = Workflow::with_registry(registry);

    let reader_id = w
        .add_custom(
            "csv_reader",
            CsvReaderConfig {
                path: Some(statement_path.to_string_lossy().into_owned()),
            },
        )
        .map_err(RunError::Block)?;
    let summary_id = w
        .add_custom(
            "expense_summary_excel",
            ExpenseSummaryExcelConfig {
                date_col: "date".into(),
                amount_col: "amount".into(),
                category_col: "category".into(),
                output_path: output_path.clone(),
            },
        )
        .map_err(RunError::Block)?;

    w.link(reader_id, summary_id);
    w.run()?;
    Ok(output_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn expense_report_workflow_runs() {
        let csv = include_str!("data/sample_bank_statement.csv");
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(csv.as_bytes()).unwrap();
        f.flush().unwrap();
        let csv_path = f.path();
        let out = tempfile::tempdir().unwrap();
        let out_xlsx = out.path().join("expense_report.xlsx");

        let result = run_expense_report_workflow(csv_path, &out_xlsx);
        assert!(result.is_ok());
        assert!(out_xlsx.exists());
    }
}
