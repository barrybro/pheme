//! Article themes, after NetNewsWire's ArticleThemesManager. A theme is a template and a
//! stylesheet. Pheme's own is "Default"; NetNewsWire's bundled themes come built in, and
//! `.nnwtheme` folders added to the themes folder are picked up too.
//! https://github.com/Ranchero-Software/NetNewsWire/blob/main/Technotes/Themes.md

use crate::paths;
use std::path::{Path, PathBuf};

pub const DEFAULT: &str = "Default";

pub struct Theme {
    pub template: String,
    pub stylesheet: String,
}

macro_rules! built_in {
    ($($name:literal),* $(,)?) => {
        &[$((
            $name,
            include_str!(concat!("../resources/themes/", $name, ".nnwtheme/template.html")),
            include_str!(concat!("../resources/themes/", $name, ".nnwtheme/stylesheet.css")),
        )),*]
    };
}

/// NetNewsWire's themes (MIT License), as (name, template, stylesheet).
const BUILT_IN: &[(&str, &str, &str)] = built_in!(
    "Appanoose",
    "Biblioteca",
    "Hyperlegible",
    "NewsFax",
    "Promenade",
    "Sepia",
    "Tiqoe Dark",
    "Verdana Revival",
);

/// Where the user's own themes go: ~/.local/share/pheme/Themes.
pub fn folder() -> PathBuf {
    let dir = paths::data_dir().join("Themes");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// "Default", then the built-in themes, then the user's, each by name.
pub fn names() -> Vec<String> {
    let mut names: Vec<String> = std::iter::once(DEFAULT).chain(BUILT_IN.iter().map(|t| t.0)).map(String::from).collect();
    let mut installed: Vec<String> = installed().into_iter().map(|(name, _)| name).filter(|n| !names.contains(n)).collect();
    installed.sort_by_key(|n| n.to_lowercase());
    installed.dedup();
    names.extend(installed);
    names
}

/// The named theme, or `None` for "Default" or a theme that's gone.
pub fn load(name: &str) -> Option<Theme> {
    if name == DEFAULT {
        return None;
    }
    if let Some((_, template, stylesheet)) = BUILT_IN.iter().find(|t| t.0 == name) {
        return Some(Theme { template: template.to_string(), stylesheet: stylesheet.to_string() });
    }
    let (_, dir) = installed().into_iter().find(|(n, _)| n == name)?;
    let read = |file: &str| std::fs::read_to_string(dir.join(file)).ok();
    Some(Theme { template: read("template.html")?, stylesheet: read("stylesheet.css")? })
}

/// The user's `.nnwtheme` folders, with their names.
fn installed() -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(folder()) else { return vec![] };
    entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "nnwtheme") && p.join("template.html").is_file() && p.join("stylesheet.css").is_file())
        .filter_map(|p| Some((theme_name(&p)?, p)))
        .collect()
}

/// The `Name` from a theme's Info.plist, or else its folder name.
fn theme_name(dir: &Path) -> Option<String> {
    let from_plist = std::fs::read_to_string(dir.join("Info.plist")).ok().and_then(|plist| {
        let after_key = plist.split_once("<key>Name</key>")?.1;
        let value = after_key.trim_start().strip_prefix("<string>")?;
        let name = value.split_once("</string>")?.0.trim();
        (!name.is_empty()).then(|| name.to_string())
    });
    from_plist.or_else(|| dir.file_stem().map(|s| s.to_string_lossy().into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_name_from_info_plist() {
        let dir = tempfile::tempdir().unwrap();
        let theme = dir.path().join("Folder Name.nnwtheme");
        std::fs::create_dir(&theme).unwrap();
        assert_eq!(theme_name(&theme).as_deref(), Some("Folder Name"));
        std::fs::write(theme.join("Info.plist"), "<dict>\n\t<key>Name</key>\n\t<string>Real Name</string>\n</dict>").unwrap();
        assert_eq!(theme_name(&theme).as_deref(), Some("Real Name"));
    }

    #[test]
    fn built_in_themes_load() {
        assert!(load(DEFAULT).is_none());
        for (name, _, _) in BUILT_IN {
            let theme = load(name).unwrap();
            assert!(theme.template.contains("[[body]]"), "{name}");
        }
    }
}
