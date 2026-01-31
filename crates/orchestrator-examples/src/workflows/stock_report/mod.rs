//! Stock report workflow: custom CSV reader -> Polars pivot -> Excel file.
//! Self-contained under workflows/stock_report/ with dummy data in data/sample_stocks.csv.
//! Kept as alternative example; main runs expense_report.
#![allow(dead_code)]

mod blocks;

use std::path::Path;

use orchestrator_core::{BlockRegistry, RunError, Workflow};

use blocks::{CsvReaderBlock, CsvReaderConfig, PolarsPivotExcelBlock, PolarsPivotExcelConfig};

/// Path to the dummy stock CSV relative to this source file's directory.
/// Resolved at runtime via CARGO_MANIFEST_DIR (orchestrator-examples crate root).
pub fn default_csv_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("workflows")
        .join("stock_report")
        .join("data")
        .join("sample_stocks.csv")
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
    registry.register_custom("polars_pivot_excel", |payload| {
        let index_col = payload
            .get("index_col")
            .and_then(|v| v.as_str())
            .unwrap_or("date")
            .to_string();
        let columns_col = payload
            .get("columns_col")
            .and_then(|v| v.as_str())
            .unwrap_or("symbol")
            .to_string();
        let values_col = payload
            .get("values_col")
            .and_then(|v| v.as_str())
            .unwrap_or("close")
            .to_string();
        let output_path = payload
            .get("output_path")
            .and_then(|v| v.as_str())
            .map(std::path::PathBuf::from)
            .ok_or_else(|| orchestrator_core::block::BlockError::Other("output_path required".into()))?;
        Ok(Box::new(PolarsPivotExcelBlock::new(PolarsPivotExcelConfig {
            index_col,
            columns_col,
            values_col,
            output_path,
        })))
    });
    registry
}

/// Run the stock report workflow: read CSV at `csv_path`, pivot (date x symbol, close), write to `output_excel_path`.
/// Returns the path to the created Excel file or error.
pub fn run_stock_report_workflow(
    csv_path: impl AsRef<Path>,
    output_excel_path: impl AsRef<Path>,
) -> Result<std::path::PathBuf, RunError> {
    let csv_path = csv_path.as_ref();
    let output_path = output_excel_path.as_ref().to_path_buf();
    let registry = make_registry();
    let mut w = Workflow::with_registry(registry);

    let reader_id = w
        .add_custom("csv_reader", CsvReaderConfig { path: Some(csv_path.to_string_lossy().into_owned()) })
        .map_err(RunError::Block)?;
    let pivot_id = w
        .add_custom(
            "polars_pivot_excel",
            PolarsPivotExcelConfig {
                index_col: "date".into(),
                columns_col: "symbol".into(),
                values_col: "close".into(),
                output_path: output_path.clone(),
            },
        )
        .map_err(RunError::Block)?;

    w.link(reader_id, pivot_id);
    w.run()?;
    Ok(output_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn stock_report_workflow_runs() {
        let csv = include_str!("data/sample_stocks.csv");
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(csv.as_bytes()).unwrap();
        f.flush().unwrap();
        let csv_path = f.path();
        let out = tempfile::tempdir().unwrap();
        let out_xlsx = out.path().join("stock_report.xlsx");

        let result = run_stock_report_workflow(csv_path, &out_xlsx);
        assert!(result.is_ok());
        assert!(out_xlsx.exists());
    }
}
