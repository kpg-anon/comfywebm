//! Font files installed on this system, for captions drawn by ffmpeg's drawtext.

use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use ttf_parser::name_id;

/// One font face, as the caption font list shows it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FontFace {
    pub path: String,
    pub family: String,
    /// "Bold", "Italic", "Regular", ...
    pub style: String,
    /// 100..900 (400 regular, 700 bold), for the player's preview
    pub weight: u16,
    pub italic: bool,
}

/// True for a file drawtext can load by path (.ttf, .otf, .ttc, .otc).
pub fn is_font_file(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc" | "otc"))
        .unwrap_or(false)
}

fn font_dirs() -> Vec<PathBuf> {
    let mut d = Vec::new();
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    if cfg!(windows) {
        d.push(env("WINDIR").unwrap_or_else(|| PathBuf::from("C:\\Windows")).join("Fonts"));
        if let Some(l) = env("LOCALAPPDATA") {
            d.push(l.join("Microsoft\\Windows\\Fonts"));
        }
    } else if cfg!(target_os = "macos") {
        d.push("/System/Library/Fonts".into());
        d.push("/Library/Fonts".into());
        if let Some(h) = env("HOME") {
            d.push(h.join("Library/Fonts"));
        }
    } else {
        d.push("/usr/share/fonts".into());
        d.push("/usr/local/share/fonts".into());
        if let Some(h) = env("HOME") {
            d.push(h.join(".local/share/fonts"));
            d.push(h.join(".fonts"));
        }
    }
    d
}

fn walk(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() && depth < 4 {
            walk(&p, depth + 1, out);
        } else if is_font_file(&p) {
            out.push(p);
        }
    }
}

/// The best name with this id: US English first, then any readable one.
fn name(face: &ttf_parser::Face, id: u16) -> Option<String> {
    let names: Vec<_> = face.names().into_iter().filter(|n| n.name_id == id).collect();
    names
        .iter()
        .filter(|n| n.language() == ttf_parser::Language::English_UnitedStates)
        .chain(names.iter())
        .find_map(|n| n.to_string())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Reads the first face of a font file. drawtext has no face index, so later
/// faces in a collection (.ttc) cannot be used and are not listed.
pub fn read_face(path: &Path) -> Option<FontFace> {
    let data = std::fs::read(path).ok()?;
    let face = ttf_parser::Face::parse(&data, 0).ok()?;
    let family = name(&face, name_id::TYPOGRAPHIC_FAMILY).or_else(|| name(&face, name_id::FAMILY))?;
    let style = name(&face, name_id::TYPOGRAPHIC_SUBFAMILY)
        .or_else(|| name(&face, name_id::SUBFAMILY))
        .unwrap_or_else(|| "Regular".into());
    Some(FontFace {
        path: path.to_string_lossy().into_owned(),
        family,
        style,
        weight: face.weight().to_number(),
        italic: face.is_italic(),
    })
}

/// Every readable font face in the system and user font folders, sorted by
/// family, then weight, upright before italic. Hidden families (names that
/// start with "." on macOS) and repeated family + style pairs are left out.
pub fn list_fonts() -> Vec<FontFace> {
    let mut files = Vec::new();
    for d in font_dirs() {
        walk(&d, 0, &mut files);
    }
    let mut seen = HashSet::new();
    let mut out: Vec<FontFace> = files
        .iter()
        .filter_map(|p| read_face(p))
        .filter(|f| !f.family.starts_with('.'))
        .filter(|f| seen.insert((f.family.to_lowercase(), f.style.to_lowercase())))
        .collect();
    out.sort_by(|a, b| {
        a.family
            .to_lowercase()
            .cmp(&b.family.to_lowercase())
            .then(a.weight.cmp(&b.weight))
            .then(a.italic.cmp(&b.italic))
            .then(a.style.cmp(&b.style))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_file_extensions() {
        assert!(is_font_file(Path::new("C:/Windows/Fonts/arialbd.TTF")));
        assert!(is_font_file(Path::new("/x/y.otf")));
        assert!(!is_font_file(Path::new("/x/y.txt")));
        assert!(!is_font_file(Path::new("/x/ttf")));
    }

    #[test]
    fn lists_installed_fonts() {
        // a CI runner may have no fonts, so only what is listed is checked
        let all = list_fonts();
        if cfg!(windows) {
            assert!(all.iter().any(|f| f.family == "Arial"), "Arial not in {} fonts", all.len());
        }
        for f in all.iter().take(20) {
            assert!(Path::new(&f.path).is_file());
            assert!(!f.family.is_empty());
        }
    }
}
