//! Feedly credentials, kept in a file only the user can read.

use crate::paths;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    /// Obtained through the OAuth sign-in flow; refreshable.
    OAuth,
    /// A developer access token pasted in by the user.
    DeveloperToken,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub kind: CredentialKind,
    pub user_id: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub display_name: String,
}

fn path() -> PathBuf {
    paths::data_dir().join("credentials.json")
}

pub fn load() -> Option<Credentials> {
    let text = std::fs::read_to_string(path()).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save(credentials: &Credentials) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path())?;
    file.write_all(serde_json::to_string_pretty(credentials)?.as_bytes())?;
    Ok(())
}

pub fn delete() {
    let _ = std::fs::remove_file(path());
}
