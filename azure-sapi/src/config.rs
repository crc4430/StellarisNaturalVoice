//! Resolution of the working directory used for the log file. Azure config
//! (region, key, voice) travels through the registry voice token instead of a
//! local assets directory.

use std::path::PathBuf;

/// Directory for `engine.log` (and any future local state). Order: explicit
/// value from the voice token, then env var, then `%LOCALAPPDATA%\AzureSapi`.
pub fn assets_dir(from_token: Option<String>) -> PathBuf {
    if let Some(dir) = from_token {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    if let Ok(dir) = std::env::var("AZURE_SAPI_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    let local = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(local).join("AzureSapi")
}
