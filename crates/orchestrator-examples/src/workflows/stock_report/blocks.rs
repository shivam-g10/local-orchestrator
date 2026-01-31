//! Custom blocks for the stock report workflow: CSV reader and Polars pivot to Excel.
#![allow(dead_code)]

use std::io::Cursor;
use std::path::Path;

use orchestrator_core::block::{BlockError, BlockExecutor, BlockInput, BlockOutput};
use polars::prelude::{Column, *};
use polars_excel_writer::PolarsExcelWriter;
use serde::Serialize;

/// Config for the CSV reader block. Path can be set in config or passed via input.
#[derive(Serialize)]
pub struct CsvReaderConfig {
    pub path: Option<String>,
}

/// Custom block that reads a CSV file from path (config or input) and outputs its content as string.
/// Validates that the file has a header row (first line contains a comma).
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

/// Config for the Polars pivot-to-Excel block.
#[derive(Serialize)]
pub struct PolarsPivotExcelConfig {
    pub index_col: String,
    pub columns_col: String,
    pub values_col: String,
    #[serde(serialize_with = "path_to_string")]
    pub output_path: std::path::PathBuf,
}

fn path_to_string<S>(path: &std::path::Path, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    s.serialize_str(&path.to_string_lossy())
}

/// Custom block that parses CSV from input, pivots (index x columns = values), and writes to Excel.
pub struct PolarsPivotExcelBlock {
    index_col: String,
    columns_col: String,
    values_col: String,
    output_path: std::path::PathBuf,
}

impl PolarsPivotExcelBlock {
    pub fn new(config: PolarsPivotExcelConfig) -> Self {
        Self {
            index_col: config.index_col,
            columns_col: config.columns_col,
            values_col: config.values_col,
            output_path: config.output_path,
        }
    }
}

impl BlockExecutor for PolarsPivotExcelBlock {
    fn execute(&self, input: BlockInput) -> Result<BlockOutput, BlockError> {
        let csv_content = match &input {
            BlockInput::String(s) => s.as_str(),
            BlockInput::Empty => return Err(BlockError::Other("CSV content required from upstream".into())),
        };
        let cursor = Cursor::new(csv_content.as_bytes());
        let df = CsvReader::new(cursor)
            .finish()
            .map_err(|e| BlockError::Other(format!("polars csv: {}", e)))?;
        if df.height() == 0 {
            return Err(BlockError::Other("CSV has no data rows".into()));
        }
        let has = |name: &str| {
            df.get_column_names()
                .iter()
                .any(|c| <_ as AsRef<str>>::as_ref(c) == name)
        };
        if !has(&self.index_col) {
            return Err(BlockError::Other(format!("index column '{}' not found", self.index_col)));
        }
        if !has(&self.columns_col) {
            return Err(BlockError::Other(format!("columns column '{}' not found", self.columns_col)));
        }
        if !has(&self.values_col) {
            return Err(BlockError::Other(format!("values column '{}' not found", self.values_col)));
        }
        let pivoted = pivot_stock_report(&df, &self.index_col, &self.columns_col, &self.values_col)
            .map_err(|e| BlockError::Other(format!("pivot: {}", e)))?;
        if let Some(parent) = self.output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BlockError::Io(format!("create_dir_all: {}", e)))?;
        }
        let mut writer = PolarsExcelWriter::new();
        writer
            .write_dataframe(&pivoted)
            .map_err(|e| BlockError::Other(format!("excel write: {}", e)))?;
        writer
            .save(&self.output_path)
            .map_err(|e| BlockError::Io(format!("excel save: {}", e)))?;
        Ok(BlockOutput::String {
            value: self.output_path.to_string_lossy().into_owned(),
        })
    }
}

/// Pivot long-format DataFrame to wide: index_col = rows, columns_col = columns, values_col = values.
fn pivot_stock_report(
    df: &DataFrame,
    index_col: &str,
    columns_col: &str,
    values_col: &str,
) -> PolarsResult<DataFrame> {
    let index_series = df.column(index_col)?.clone();
    let columns_series = df.column(columns_col)?.clone();
    let values_series = df.column(values_col)?.clone();
    let unique_cols: Vec<String> = {
        let mut v: Vec<String> = columns_series
            .str()?
            .into_iter()
            .filter_map(|o| o.map(String::from))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        v.sort();
        v
    };
    let unique_index_col = index_series.unique_stable()?;
    let unique_index_series = unique_index_col.as_materialized_series().clone();
    let mut out_columns: Vec<Series> = vec![unique_index_series.clone()];
    for col_val in &unique_cols {
        let mask = columns_series.str()?.equal(col_val.as_str());
        let taken = values_series.filter(&mask)?;
        let idx = index_series.filter(&mask)?;
        let taken_s = taken.as_materialized_series();
        let idx_s = idx.as_materialized_series();
        let aligned = align_to_index(&unique_index_series, idx_s, taken_s)?;
        out_columns.push(aligned.with_name(col_val.as_str().into()));
    }
    let columns: Vec<Column> = out_columns.into_iter().map(Column::from).collect();
    DataFrame::new(columns)
}

fn align_to_index(index_full: &Series, index_subset: &Series, values_subset: &Series) -> PolarsResult<Series> {
    let mut builder = PrimitiveChunkedBuilder::<Float64Type>::new("".into(), index_full.len());
    let subset_map: std::collections::HashMap<String, Option<f64>> = index_subset
        .str()?
        .into_iter()
        .zip(values_subset.cast(&DataType::Float64)?.f64()?)
        .filter_map(|(k, v)| k.map(|key| (key.to_string(), v)))
        .collect();
    for key in index_full.str()?.into_iter() {
        let val = key.and_then(|k| subset_map.get(k).copied().flatten());
        builder.append_option(val);
    }
    Ok(builder.finish().into_series())
}
