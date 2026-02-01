//! Custom blocks for the expense report workflow: CSV reader and expense summary to Excel.
//! Answers "where is the money going" from a bank statement over a financial year.

use std::io::Cursor;
use std::path::Path;

use orchestrator_core::block::{BlockError, BlockExecutor, BlockInput, BlockOutput};
use polars::prelude::*;
use polars_excel_writer::PolarsExcelWriter;
use serde::Serialize;

/// Config for the CSV reader block. Path can be set in config or passed via input.
#[derive(Serialize)]
pub struct CsvReaderConfig {
    pub path: Option<String>,
}

/// Reads a CSV file from path (config or input) and outputs its content as string.
pub struct CsvReaderBlock {
    path: Option<String>,
}

impl CsvReaderBlock {
    pub fn new(config: CsvReaderConfig) -> Self {
        Self { path: config.path }
    }
}

impl BlockExecutor for CsvReaderBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockOutput, BlockError> {
        let path_str = match &input {
            BlockInput::String(s) if !s.trim().is_empty() => s.trim().to_string(),
            BlockInput::Text(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => self
                .path
                .clone()
                .filter(|p| !p.trim().is_empty())
                .ok_or_else(|| BlockError::Other("path required (config or input)".into()))?,
        };
        let path = Path::new(&path_str);
        if !path.exists() {
            return Err(BlockError::FileNotFound(path_str));
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| BlockError::Io(format!("{}: {}", path_str, e)))?;
        if content.lines().next().map(|l| l.contains(',')).unwrap_or(false) {
            Ok(BlockOutput::String { value: content })
        } else {
            Err(BlockError::Other("CSV must have a header row (first line with comma)".into()))
        }
    }
}

/// Config for the expense summary block. Expects bank statement CSV: date, description, amount, category.
#[derive(Serialize)]
pub struct ExpenseSummaryExcelConfig {
    pub date_col: String,
    pub amount_col: String,
    pub category_col: String,
    #[serde(serialize_with = "path_to_string")]
    pub output_path: std::path::PathBuf,
}

fn path_to_string<S>(path: &Path, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    s.serialize_str(&path.to_string_lossy())
}

/// Parses bank statement CSV, aggregates expenses by category and by month,
/// writes an Excel report answering "where is the money going".
pub struct ExpenseSummaryExcelBlock {
    date_col: String,
    amount_col: String,
    category_col: String,
    output_path: std::path::PathBuf,
}

impl ExpenseSummaryExcelBlock {
    pub fn new(config: ExpenseSummaryExcelConfig) -> Self {
        Self {
            date_col: config.date_col,
            amount_col: config.amount_col,
            category_col: config.category_col,
            output_path: config.output_path,
        }
    }
}

impl BlockExecutor for ExpenseSummaryExcelBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockOutput, BlockError> {
        let csv_content = match &input {
            BlockInput::String(s) => s.clone(),
            BlockInput::Text(s) => s.clone(),
            BlockInput::Json(v) => v.to_string(),
            BlockInput::List { items } => items.join("\n"),
            BlockInput::Empty => return Err(BlockError::Other("Bank statement CSV required from upstream".into())),
            BlockInput::Multi { .. } => return Err(BlockError::Other("Bank statement CSV required (single input)".into())),
        };
        let cursor = Cursor::new(csv_content.as_bytes());
        let df = CsvReader::new(cursor)
            .finish()
            .map_err(|e| BlockError::Other(format!("parse csv: {}", e)))?;
        if df.height() == 0 {
            return Err(BlockError::Other("CSV has no data rows".into()));
        }
        let has = |name: &str| {
            df.get_column_names()
                .iter()
                .any(|c| <_ as AsRef<str>>::as_ref(c) == name)
        };
        if !has(&self.date_col) {
            return Err(BlockError::Other(format!("date column '{}' not found", self.date_col)));
        }
        if !has(&self.amount_col) {
            return Err(BlockError::Other(format!("amount column '{}' not found", self.amount_col)));
        }
        if !has(&self.category_col) {
            return Err(BlockError::Other(format!("category column '{}' not found", self.category_col)));
        }

        let by_category = build_by_category_summary(&df, &self.amount_col, &self.category_col)
            .map_err(|e| BlockError::Other(format!("by category: {}", e)))?;
        let monthly = build_monthly_summary(&df, &self.date_col, &self.amount_col)
            .map_err(|e| BlockError::Other(format!("monthly: {}", e)))?;

        if let Some(parent) = self.output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BlockError::Io(format!("create_dir_all: {}", e)))?;
        }

        let mut writer = PolarsExcelWriter::new();
        writer
            .write_dataframe(&by_category)
            .map_err(|e| BlockError::Other(format!("excel sheet: {}", e)))?;
        writer.add_worksheet();
        writer
            .write_dataframe(&monthly)
            .map_err(|e| BlockError::Other(format!("excel sheet: {}", e)))?;
        writer
            .save(&self.output_path)
            .map_err(|e| BlockError::Io(format!("excel save: {}", e)))?;

        Ok(BlockOutput::String {
            value: self.output_path.to_string_lossy().into_owned(),
        })
    }
}

/// Build summary: Category, Total_Spent, Pct_Of_Total. Only negative amounts (expenses) are summed.
/// Uses eager API only (no lazy) to avoid polars-expr feature bugs.
fn build_by_category_summary(
    df: &DataFrame,
    amount_col: &str,
    category_col: &str,
) -> PolarsResult<DataFrame> {
    let amount = df.column(amount_col)?.cast(&DataType::Float64)?;
    let mask = amount.f64()?.lt(0);
    let df_exp = df.filter(&mask)?;
    let categories = df_exp.column(category_col)?.clone();
    let amounts_series = df_exp.column(amount_col)?.cast(&DataType::Float64)?;
    let amounts_ca = amounts_series.f64()?;
    let mut by_cat: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for (cat, amt) in categories.str()?.into_iter().zip(amounts_ca.into_iter()) {
        if let (Some(c), Some(a)) = (cat, amt) {
            let key = c.to_string();
            *by_cat.entry(key).or_insert(0.0) += a.abs();
        }
    }
    let mut cats: Vec<String> = by_cat.keys().cloned().collect();
    cats.sort_by(|a, b| by_cat[b].partial_cmp(&by_cat[a]).unwrap_or(std::cmp::Ordering::Equal));
    let total: f64 = by_cat.values().sum();
    let total_spent: Vec<f64> = cats.iter().map(|c| by_cat[c]).collect();
    let pct: Vec<f64> = total_spent.iter().map(|&s| if total > 0.0 { s / total * 100.0 } else { 0.0 }).collect();
    let out = DataFrame::new(vec![
        Series::new(category_col.into(), cats).into(),
        Series::new("total_spent".into(), total_spent).into(),
        Series::new("pct_of_total".into(), pct).into(),
    ])?;
    Ok(out)
}

/// Build monthly total spent (expenses only). Month = YYYY-MM from date column (string slice).
/// Uses eager API only (no lazy) to avoid polars-expr feature bugs.
fn build_monthly_summary(
    df: &DataFrame,
    date_col: &str,
    amount_col: &str,
) -> PolarsResult<DataFrame> {
    let amount = df.column(amount_col)?.cast(&DataType::Float64)?;
    let mask = amount.f64()?.lt(0);
    let df_exp = df.filter(&mask)?;
    let dates = df_exp.column(date_col)?.str()?;
    let amounts_series = df_exp.column(amount_col)?.cast(&DataType::Float64)?;
    let amounts_ca = amounts_series.f64()?;
    let mut by_month: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for (d, amt) in dates.into_iter().zip(amounts_ca.into_iter()) {
        if let (Some(date_str), Some(a)) = (d, amt) {
            let s: &str = date_str;
            let month = if s.len() >= 7 { s[..7].to_string() } else { s.to_string() };
            *by_month.entry(month).or_insert(0.0) += a.abs();
        }
    }
    let mut months: Vec<String> = by_month.keys().cloned().collect();
    months.sort();
    let total_spent: Vec<f64> = months.iter().map(|m| by_month[m]).collect();
    let out = DataFrame::new(vec![
        Series::new("month".into(), months).into(),
        Series::new("total_spent".into(), total_spent).into(),
    ])?;
    Ok(out)
}
