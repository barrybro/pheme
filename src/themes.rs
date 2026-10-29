//! Article themes, after NetNewsWire's ArticleThemesManager. A theme is a template and a
//! stylesheet. Pheme's own is "Default"; NetNewsWire's bundled themes come built in, and
//! `.nnwtheme` folders added to the themes folder are picked up too.
//! https://github.com/Ranchero-Software/NetNewsWire/blob/main/Technotes/Themes.md

use crate::paths;
use anyhow::{Context, Result, anyhow, bail};
use std::io::Read;
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

/// Installs a theme from a `.nnwtheme` folder or a zip file holding one, replacing an
/// installed theme in a folder of the same name. Returns the theme's name.
pub fn import(path: &Path) -> Result<String> {
    import_into(path, &folder())
}

fn import_into(path: &Path, themes: &Path) -> Result<String> {
    let staging = themes.join(".importing");
    let _ = std::fs::remove_dir_all(&staging);
    let result = stage(path, &staging).and_then(|stem| {
        if !staging.join("template.html").is_file() || !staging.join("stylesheet.css").is_file() {
            bail!("That isn’t a NetNewsWire theme: a theme needs a template.html and a stylesheet.css.");
        }
        let installed = themes.join(format!("{stem}.nnwtheme"));
        if installed.exists() {
            std::fs::remove_dir_all(&installed).context("Couldn’t replace the installed theme")?;
        }
        std::fs::rename(&staging, &installed).context("Couldn’t install the theme")?;
        Ok(theme_name(&installed).unwrap_or(stem))
    });
    let _ = std::fs::remove_dir_all(&staging);
    result
}

/// Copies or unzips the theme into `staging`. Returns the name for its folder, without
/// the `.nnwtheme`.
fn stage(path: &Path, staging: &Path) -> Result<String> {
    if path.is_dir() {
        copy_dir(path, staging).context("Couldn’t copy the theme")?;
        Ok(folder_stem(path))
    } else {
        unzip(path, staging)
    }
}

/// Copies files and folders, leaving out symbolic links.
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// More than any real theme, whose fonts can be embedded in its stylesheet.
const MAX_THEME_BYTES: u64 = 20 * 1024 * 1024;

/// Unzips the theme in a zip file: the `.nnwtheme` folder inside it, or else the folder
/// holding its template.html. Returns the name for its folder, without the `.nnwtheme`.
fn unzip(path: &Path, staging: &Path) -> Result<String> {
    let file = std::fs::File::open(path).with_context(|| format!("Couldn’t open {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|_| anyhow!("That isn’t a theme folder or a zip file."))?;
    // Names that would land outside the destination, like `../x`, come back as None.
    let names: Vec<PathBuf> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok()?.enclosed_name())
        .filter(|p| !p.starts_with("__MACOSX"))
        .collect();
    let root = names
        .iter()
        .find_map(|p| p.ancestors().find(|a| a.extension().is_some_and(|x| x == "nnwtheme")).map(Path::to_path_buf))
        .or_else(|| {
            let template = names.iter().find(|p| p.file_name().is_some_and(|n| n == "template.html"))?;
            Some(template.parent().map(Path::to_path_buf).unwrap_or_default())
        })
        .context("There’s no NetNewsWire theme in that zip file.")?;

    let mut total = 0;
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let Some(name) = entry.enclosed_name() else { continue };
        let Ok(relative) = name.strip_prefix(&root) else { continue };
        if !entry.is_file() || relative.as_os_str().is_empty() || name.starts_with("__MACOSX") {
            continue;
        }
        let target = staging.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut limited = entry.take(MAX_THEME_BYTES - total + 1);
        total += std::io::copy(&mut limited, &mut std::fs::File::create(&target)?)?;
        if total > MAX_THEME_BYTES {
            bail!("That theme is too large.");
        }
    }
    Ok(root.file_name().map(|n| folder_stem(Path::new(n))).unwrap_or_else(|| folder_stem(path)))
}

/// `Name.nnwtheme` or `Name.nnwtheme.zip` → `Name`.
fn folder_stem(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let name = name.strip_suffix(".zip").unwrap_or(&name);
    let name = name.strip_suffix(".nnwtheme").unwrap_or(name).trim();
    if name.is_empty() { "Imported Theme".into() } else { name.into() }
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

    fn write_zip(path: &Path, files: &[(&str, &str)]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, contents) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut zip, contents.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn imports_a_zipped_theme() {
        let dir = tempfile::tempdir().unwrap();
        let themes = dir.path().join("Themes");
        std::fs::create_dir(&themes).unwrap();
        let archive = dir.path().join("Download.zip");
        write_zip(&archive, &[
            ("Pretty.nnwtheme/Info.plist", "<key>Name</key><string>Pretty Theme</string>"),
            ("Pretty.nnwtheme/template.html", "[[body]]"),
            ("Pretty.nnwtheme/stylesheet.css", "body {}"),
            ("__MACOSX/Pretty.nnwtheme/._template.html", "junk"),
            ("../escape.txt", "nope"),
        ]);
        assert_eq!(import_into(&archive, &themes).unwrap(), "Pretty Theme");
        let installed = themes.join("Pretty.nnwtheme");
        assert_eq!(std::fs::read_to_string(installed.join("template.html")).unwrap(), "[[body]]");
        assert!(!dir.path().join("escape.txt").exists());
        assert!(!themes.join(".importing").exists());

        // Importing again replaces it.
        assert_eq!(import_into(&archive, &themes).unwrap(), "Pretty Theme");
    }

    #[test]
    fn imports_a_theme_folder_and_rejects_others() {
        let dir = tempfile::tempdir().unwrap();
        let themes = dir.path().join("Themes");
        std::fs::create_dir(&themes).unwrap();
        let source = dir.path().join("Plain.nnwtheme");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("template.html"), "[[body]]").unwrap();
        assert!(import_into(&source, &themes).is_err(), "no stylesheet");
        std::fs::write(source.join("stylesheet.css"), "body {}").unwrap();
        assert_eq!(import_into(&source, &themes).unwrap(), "Plain");

        let not_zip = dir.path().join("notes.txt");
        std::fs::write(&not_zip, "hello").unwrap();
        assert!(import_into(&not_zip, &themes).is_err());
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
