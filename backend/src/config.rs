use std::{env, path::Path};
/// Initialize the environment variables
pub fn init() {
    let _ = dotenvy::from_path(Path::new(
        format!("{}/.env", env!("CARGO_MANIFEST_DIR")).as_str(),
    ));
    dotenvy::dotenv().ok();
}
/// Get the environment variable
pub fn get_env<T: std::str::FromStr + Default>(key: &str) -> T {
    let result = env::var(key);
    match result {
        Ok(s) => match s.parse() {
            Ok(val) => val,
            Err(_) => {
                tracing::error!("Error parsing {}", key);
                String::from("").parse().unwrap_or_default()
            }
        },
        Err(_) => String::from("").parse().unwrap_or_default(),
    }
}
