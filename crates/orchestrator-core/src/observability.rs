use once_cell::sync::OnceCell;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

static INIT: OnceCell<()> = OnceCell::new();

fn parse_bool_env(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "enabled" => Some(true),
        "0" | "false" | "no" | "off" | "disabled" => Some(false),
        _ => None,
    }
}

fn observability_enabled() -> bool {
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

fn resolve_env_filter() -> tracing_subscriber::EnvFilter {
    if let Ok(level) = std::env::var("ORCHESTRATOR_LOG_LEVEL")
        && let Ok(filter) = tracing_subscriber::EnvFilter::try_new(level)
    {
        return filter;
    }
    tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
}

/// Initialize observability logging once per process.
///
/// Environment variables:
/// - `ORCHESTRATOR_OBSERVABILITY_ENABLED` / `ORCHESTRATOR_OBSERVABILITY`: optional enable/disable flag (default enabled).
/// - `ORCHESTRATOR_LOG_LEVEL`: optional level/filter override (`info`, `debug`, etc.).
/// - `ORCHESTRATOR_JSON_LOG_PATH`: optional log file path. If set, logs are JSONL in that file.
///   If unset, logs are emitted to stdout in a human-readable console format.
/// - `RUST_LOG`: optional filter override.
pub fn init_observability() {
    INIT.get_or_init(|| {
        if !observability_enabled() {
            return;
        }

        let env_filter = resolve_env_filter();
        if let Ok(path_raw) = std::env::var("ORCHESTRATOR_JSON_LOG_PATH") {
            let path = std::path::PathBuf::from(path_raw);
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                let _ = std::fs::create_dir_all(parent);
            }
            let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("orchestrator.logs.jsonl");
            let writer = tracing_appender::rolling::never(dir, file_name);
            let json_layer = tracing_subscriber::fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(true)
                .with_target(false)
                .with_writer(writer);
            let _ = tracing_subscriber::registry()
                .with(env_filter)
                .with(json_layer)
                .try_init();
        } else {
            let console_layer = tracing_subscriber::fmt::layer()
                .compact()
                .with_target(false)
                .with_writer(std::io::stdout);
            let _ = tracing_subscriber::registry()
                .with(env_filter)
                .with(console_layer)
                .try_init();
        }
    });
}
