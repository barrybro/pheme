//! Where Pheme keeps its files: ~/.config/pheme and ~/.local/share/pheme.

use directories::ProjectDirs;
use std::path::PathBuf;

fn dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "pheme")
}

pub fn config_dir() -> Option<PathBuf> {
    dirs().map(|d| d.config_dir().to_path_buf())
}

pub fn data_dir() -> PathBuf {
    let dir = dirs()
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("pheme"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}
