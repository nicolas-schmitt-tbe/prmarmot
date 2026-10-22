//! Colour themes a person drops into `<config dir>/prmarmot/themes/`: JSON
//! files in gpui-component's theme format (the one its community themes
//! use, so a Catppuccin or Tokyo Night file works as it is). This crate lists
//! them, names the broken ones and checks each one's contrast, so the CLI and
//! the Settings dialog agree on what exists; the desktop app loads the
//! colours themselves.

use std::path::{Path, PathBuf};

use crate::config::config_path;

/// Files read from the folder at most; the rest are named as ignored.
pub const MAX_THEME_FILES: usize = 64;
/// A theme file larger than this is ignored (a community theme is ~15 KiB).
pub const MAX_THEME_BYTES: u64 = 1 << 20;
/// WCAG AA for ordinary text.
pub const MIN_TEXT_CONTRAST: f64 = 4.5;

/// The themes folder beside `config.toml`.
pub fn themes_dir() -> PathBuf {
    config_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("themes")
}

/// One theme out of a file: its name (what `colors = "…"` names), whether it
/// is a dark theme, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserTheme {
    pub name: String,
    pub dark: bool,
    pub path: PathBuf,
    /// The first text pair below AA, as one sentence, or `None` when every
    /// pair the file sets passes. Shown in Settings; never blocks the theme.
    pub contrast_warning: Option<String>,
}

/// Every theme in the folder, and one line for each file that could not be
/// read (said in the ignored-config banner, like any other ignored value).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ThemeListing {
    pub themes: Vec<UserTheme>,
    pub problems: Vec<String>,
}

impl ThemeListing {
    /// The theme with this name, ignoring case.
    pub fn find(&self, name: &str) -> Option<&UserTheme> {
        let name = name.trim();
        self.themes
            .iter()
            .find(|theme| theme.name.eq_ignore_ascii_case(name))
    }

    /// The theme named `name` for the appearance on screen: the named theme
    /// when it is of that mode; else one of the same name in that mode, else
    /// a sibling of the same file in that mode, so a set that ships a light
    /// and a dark theme serves both appearances; else the named theme
    /// itself, whatever its mode (the board then draws in that mode).
    pub fn for_mode(&self, name: &str, dark: bool) -> Option<&UserTheme> {
        let named = self.find(name)?;
        if named.dark == dark {
            return Some(named);
        }
        self.themes
            .iter()
            .find(|theme| theme.name.eq_ignore_ascii_case(&named.name) && theme.dark == dark)
            .or_else(|| {
                self.themes
                    .iter()
                    .find(|theme| theme.path == named.path && theme.dark == dark)
            })
            .or(Some(named))
    }

    /// Whether a theme of this name and mode is already listed.
    fn has(&self, name: &str, dark: bool) -> bool {
        self.themes
            .iter()
            .any(|theme| theme.name.eq_ignore_ascii_case(name) && theme.dark == dark)
    }
}

/// The themes in [`themes_dir`]. A missing folder is an empty listing.
pub fn list_themes() -> ThemeListing {
    list_themes_in(&themes_dir())
}

/// The themes in `dir`: every `.json` file, in name order, at most
/// [`MAX_THEME_FILES`] of them.
pub fn list_themes_in(dir: &Path) -> ThemeListing {
    let mut listing = ThemeListing::default();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return listing;
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        })
        .collect();
    files.sort();
    if files.len() > MAX_THEME_FILES {
        listing.problems.push(format!(
            "ignoring {} theme files past the first {MAX_THEME_FILES} in {}",
            files.len() - MAX_THEME_FILES,
            dir.display()
        ));
        files.truncate(MAX_THEME_FILES);
    }
    for path in files {
        match read_theme_file(&path) {
            Ok(themes) => {
                for theme in themes {
                    if listing.has(&theme.name, theme.dark) {
                        listing.problems.push(format!(
                            "ignoring the {} theme {:?} in {}: one of that name was already loaded",
                            if theme.dark { "dark" } else { "light" },
                            theme.name,
                            file_name(&path)
                        ));
                    } else {
                        listing.themes.push(theme);
                    }
                }
            }
            Err(problem) => listing.problems.push(format!(
                "ignoring theme file {}: {problem}",
                file_name(&path)
            )),
        }
    }
    listing
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn read_theme_file(path: &Path) -> Result<Vec<UserTheme>, String> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_THEME_BYTES {
        return Err(format!("{size} bytes is larger than {MAX_THEME_BYTES}"));
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse_theme_set(&text, path)
}

/// The themes of one gpui-component theme file: `{"name": …, "themes":
/// [{"name": …, "mode": "light"|"dark", "colors": {…}}, …]}`.
fn parse_theme_set(text: &str, path: &Path) -> Result<Vec<UserTheme>, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let themes = value
        .get("themes")
        .and_then(serde_json::Value::as_array)
        .ok_or("no \"themes\" list")?;
    let mut out = Vec::new();
    for theme in themes {
        let Some(name) = theme
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
        else {
            return Err("a theme without a name".into());
        };
        let dark = theme
            .get("mode")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|mode| mode.eq_ignore_ascii_case("dark"));
        let contrast_warning = theme
            .get("colors")
            .and_then(serde_json::Value::as_object)
            .and_then(contrast_warning);
        out.push(UserTheme {
            name: name.to_owned(),
            dark,
            path: path.to_path_buf(),
            contrast_warning,
        });
    }
    if out.is_empty() {
        return Err("no themes in it".into());
    }
    Ok(out)
}

/// The text-on-surface pairs a theme file can set, by their JSON keys, with
/// the words Settings uses for them. Only pairs the file sets are checked:
/// a colour it leaves out is gpui-component's default, which this crate
/// does not know.
const TEXT_PAIRS: [(&str, &str, &str); 12] = [
    ("foreground", "background", "text on the background"),
    (
        "muted.foreground",
        "background",
        "muted text on the background",
    ),
    (
        "muted.foreground",
        "muted.background",
        "muted text on muted surfaces",
    ),
    (
        "secondary.foreground",
        "secondary.background",
        "secondary text on secondary surfaces",
    ),
    (
        "primary.foreground",
        "primary.background",
        "text on primary buttons",
    ),
    ("link", "background", "links on the background"),
    (
        "danger.background",
        "background",
        "danger text on the background",
    ),
    (
        "warning.background",
        "background",
        "warning text on the background",
    ),
    (
        "success.background",
        "background",
        "success text on the background",
    ),
    (
        "foreground",
        "table.hover.background",
        "text on a hovered row",
    ),
    (
        "foreground",
        "table.active.background",
        "text on a selected row",
    ),
    (
        "popover.foreground",
        "popover.background",
        "text in tooltips and menus",
    ),
];

/// The worst text pair the file sets that falls below AA, as one sentence.
fn contrast_warning(colors: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    let hex = |key: &str| {
        colors
            .get(key)
            .and_then(serde_json::Value::as_str)
            .and_then(parse_hex)
    };
    let mut worst: Option<(f64, &str)> = None;
    for (text, surface, words) in TEXT_PAIRS {
        let (Some(text), Some(surface)) = (hex(text), hex(surface)) else {
            continue;
        };
        let ratio = contrast_ratio(text, surface);
        if ratio < MIN_TEXT_CONTRAST && worst.is_none_or(|(r, _)| ratio < r) {
            worst = Some((ratio, words));
        }
    }
    worst.map(|(ratio, words)| {
        format!("{words} is {ratio:.1}:1, below the {MIN_TEXT_CONTRAST}:1 that keeps small text readable")
    })
}

/// `#RRGGBB` or `#RRGGBBAA` (the alpha is dropped) as `0xRRGGBB`; `#RGB` too.
/// Anything else — a named colour, an `hsl(…)` — is `None` and unchecked.
pub fn parse_hex(text: &str) -> Option<u32> {
    let digits = text.trim().strip_prefix('#')?;
    match digits.len() {
        3 => {
            let v = u32::from_str_radix(digits, 16).ok()?;
            let (r, g, b) = ((v >> 8) & 0xF, (v >> 4) & 0xF, v & 0xF);
            Some((r * 0x11) << 16 | (g * 0x11) << 8 | (b * 0x11))
        }
        6 => u32::from_str_radix(digits, 16).ok(),
        // `#RRGGBBAA`: the alpha is dropped. `get` rather than a byte slice,
        // so a non-ASCII value in a theme file is refused, not a panic.
        8 => digits
            .get(..6)
            .and_then(|rgb| u32::from_str_radix(rgb, 16).ok()),
        _ => None,
    }
}

/// WCAG 2.x relative luminance of an `0xRRGGBB` colour.
pub fn luminance(hex: u32) -> f64 {
    let channel = |shift: u32| {
        let c = f64::from((hex >> shift) & 0xFF) / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}

/// WCAG contrast ratio of two `0xRRGGBB` colours, 1 to 21.
pub fn contrast_ratio(a: u32, b: u32) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// `ink` at `alpha` over the opaque `surface`, rounded to 8 bits per channel:
/// the colour a translucent tint is in fact drawn as, and what contrast is
/// measured against.
pub fn blend(ink: u32, surface: u32, alpha: f64) -> u32 {
    let mut out = 0u32;
    for shift in [16, 8, 0] {
        let i = f64::from((ink >> shift) & 0xFF);
        let s = f64::from((surface >> shift) & 0xFF);
        let c = (s + (i - s) * alpha).round().clamp(0.0, 255.0) as u32;
        out |= c << shift;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_refuses_non_ascii_without_panicking() {
        assert_eq!(parse_hex("#ab€€"), None); // 8 bytes, not 8 digits
        assert_eq!(parse_hex("#aabbccdd"), Some(0xAABBCC));
        assert_eq!(parse_hex("#abc"), Some(0xAABBCC));
        assert_eq!(parse_hex("#aabbcc"), Some(0xAABBCC));
        assert_eq!(parse_hex("#aabbc"), None);
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("prmarmot-themes-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const CATPPUCCIN: &str = r##"{
      "name": "Catppuccin",
      "themes": [
        {"name": "Catppuccin Latte", "mode": "light",
         "colors": {"background": "#eff1f5", "foreground": "#4c4f69", "muted.foreground": "#9ca0b0"}},
        {"name": "Catppuccin Mocha", "mode": "dark",
         "colors": {"background": "#1e1e2e", "foreground": "#cdd6f4", "muted.foreground": "#a6adc8"}}
      ]
    }"##;

    #[test]
    fn the_folder_lists_every_theme_and_names_the_broken_file() {
        let dir = temp_dir("list");
        std::fs::write(dir.join("catppuccin.json"), CATPPUCCIN).unwrap();
        std::fs::write(dir.join("broken.json"), "{ not json").unwrap();
        std::fs::write(dir.join("notes.txt"), "not a theme").unwrap();
        std::fs::write(dir.join("empty.json"), r#"{"name": "x", "themes": []}"#).unwrap();
        // One name for both sides, and a second light "Sample" that is a duplicate.
        std::fs::write(
            dir.join("sample.json"),
            r##"{"themes": [
              {"name": "Sample", "mode": "light", "colors": {"background": "#ffffff", "foreground": "#222222"}},
              {"name": "Sample", "mode": "dark", "colors": {"background": "#111111", "foreground": "#eeeeee"}},
              {"name": "sample", "mode": "light", "colors": {"background": "#ffffff", "foreground": "#222222"}}
            ]}"##,
        )
        .unwrap();
        let listing = list_themes_in(&dir);
        assert_eq!(
            listing
                .themes
                .iter()
                .map(|t| (t.name.as_str(), t.dark))
                .collect::<Vec<_>>(),
            vec![
                ("Catppuccin Latte", false),
                ("Catppuccin Mocha", true),
                ("Sample", false),
                ("Sample", true)
            ]
        );
        assert_eq!(listing.problems.len(), 3, "{:?}", listing.problems);
        assert!(listing.problems[0].starts_with("ignoring theme file broken.json:"));
        assert!(listing.problems[1].contains("empty.json"));
        assert!(
            listing.problems[2].contains("light theme \"sample\" in sample.json"),
            "{}",
            listing.problems[2]
        );
        // One name serves both appearances.
        assert!(listing.for_mode("Sample", true).unwrap().dark);
        assert!(!listing.for_mode("sample", false).unwrap().dark);
        // A set's sibling serves the other appearance.
        assert_eq!(
            listing.for_mode("catppuccin latte", true).unwrap().name,
            "Catppuccin Mocha"
        );
        assert_eq!(
            listing.for_mode("Catppuccin Mocha", false).unwrap().name,
            "Catppuccin Latte"
        );
        assert!(listing.find("nope").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_folder_is_an_empty_listing() {
        let listing = list_themes_in(Path::new("/nonexistent/prmarmot-themes"));
        assert_eq!(listing, ThemeListing::default());
    }

    #[test]
    fn a_theme_with_poor_contrast_warns_but_still_loads() {
        let dir = temp_dir("contrast");
        std::fs::write(
            dir.join("pale.json"),
            r##"{"themes": [{"name": "Pale", "mode": "light",
                "colors": {"background": "#ffffff", "foreground": "#222222", "muted.foreground": "#bbbbbb"}}]}"##,
        )
        .unwrap();
        let listing = list_themes_in(&dir);
        let pale = listing.find("Pale").unwrap();
        let warning = pale.contrast_warning.as_deref().unwrap();
        assert!(
            warning.starts_with("muted text on the background is 1.9:1"),
            "{warning}"
        );
        // Catppuccin Latte's muted text is below AA too; its body text passes.
        std::fs::write(dir.join("catppuccin.json"), CATPPUCCIN).unwrap();
        let listing = list_themes_in(&dir);
        assert!(listing
            .find("Catppuccin Mocha")
            .unwrap()
            .contrast_warning
            .is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn hex_colours_parse_with_and_without_alpha() {
        assert_eq!(parse_hex("#1A2B3C"), Some(0x1A2B3C));
        assert_eq!(parse_hex(" #1a2b3cff "), Some(0x1A2B3C));
        assert_eq!(parse_hex("#abc"), Some(0xAABBCC));
        assert_eq!(parse_hex("red"), None);
        assert_eq!(parse_hex("#12345"), None);
    }

    #[test]
    fn the_contrast_formula_is_wcag_s_and_blending_rounds_per_channel() {
        assert!((contrast_ratio(0xFFFFFF, 0x000000) - 21.0).abs() < 1e-9);
        assert!((contrast_ratio(0x6C757D, 0xFFFFFF) - 4.69).abs() < 0.01);
        assert_eq!(blend(0x000000, 0xFFFFFF, 0.5), 0x808080);
        assert_eq!(blend(0xC92A2A, 0xFFFFFF, 0.06), 0xFCF2F2);
        assert_eq!(blend(0x123456, 0x123456, 0.3), 0x123456);
    }
}
