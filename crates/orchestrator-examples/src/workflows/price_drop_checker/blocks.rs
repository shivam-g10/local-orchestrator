//! Custom block: fetch price (stub or HTTP). For demo we return a fixed price string.

use orchestrator_core::block::{BlockError, BlockExecutor, BlockInput, BlockOutput};

/// Config for fetch_price: optional URL; demo uses stub.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FetchPriceConfig {
    pub price_stub: Option<f64>,
}

/// Block that "fetches" a price (stub returns configured value as string).
pub struct FetchPriceBlock {
    price_stub: f64,
}

impl FetchPriceBlock {
    pub fn new(price_stub: f64) -> Self {
        Self { price_stub }
    }
}

impl BlockExecutor for FetchPriceBlock {
    fn execute(&self, _input: BlockInput) -> Result<BlockOutput, BlockError> {
        Ok(BlockOutput::Text {
            value: format!("{:.2}", self.price_stub),
        })
    }
}
