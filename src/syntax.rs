use syntect::highlighting::{Theme, ThemeSet, Style};
use syntect::parsing::{SyntaxSet, SyntaxReference};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use bincode; // for deserialization of precompiled assets

use crate::settings::Settings;

/// Color and style specification for syntax highlighting.
#[derive(Debug, Clone)]
struct ColorSpec {
    fg: Option<crossterm::style::Color>,
    bold: bool,
    italic: bool,
}

impl ColorSpec {
    fn from_syntect_style(style: Style) -> Self {
        Self {
            fg: Some(crossterm::style::Color::Rgb { r: style.foreground.r, g: style.foreground.g, b: style.foreground.b }),
            bold: style.font_style.contains(syntect::highlighting::FontStyle::BOLD),
            italic: style.font_style.contains(syntect::highlighting::FontStyle::ITALIC),
        }
    }
    fn apply_to_stdout(&self, stdout: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        use crossterm::execute;
        use crossterm::style::{Attribute, SetAttribute, SetForegroundColor};
        if let Some(fg) = self.fg { execute!(stdout, SetForegroundColor(fg))?; }
        if self.bold { execute!(stdout, SetAttribute(Attribute::Bold))?; }
        if self.italic { execute!(stdout, SetAttribute(Attribute::Italic))?; }
        Ok(())
    }
}

/// A span of text with associated color/style information.
#[derive(Debug, Clone)]
pub(crate) struct StyledSpan {
    pub(crate) start: usize,
    pub(crate) end: usize,
    color_spec: ColorSpec,
}
impl StyledSpan {
    pub(crate) fn apply_to_stdout(&self, stdout: &mut impl std::io::Write) -> Result<(), std::io::Error> { self.color_spec.apply_to_stdout(stdout) }
}

/// Trait for pluggable highlighters.
pub(crate) trait Highlighter: Send + Sync {
    fn highlight_line(&self, line: &str, filename: &str, settings: &Settings) -> Vec<StyledSpan>;
}

/// Syntect-based highlighter implementation encapsulating syntax/theme data.
pub(crate) struct SyntectHighlighter {
    syntax_set: SyntaxSet,
    theme: Theme,
    file_size_cache: Mutex<HashMap<String, u64>>,
}

impl SyntectHighlighter {
    pub(crate) fn new() -> Self {
        let (syntax_set, theme) = Self::load_assets();
        Self { syntax_set, theme, file_size_cache: Mutex::new(HashMap::new()) }
    }

    fn load_assets() -> (SyntaxSet, Theme) {
        // Allow precompiled assets via env var
        if let Ok(path) = std::env::var("UE_PRECOMPILED_SYNTECT") {
            if let Ok(data) = std::fs::read(&path) {
                if let Ok((ss, themes_vec)) = bincode::deserialize::<(SyntaxSet, Vec<(String, Theme)>)>(&data) {
                    let theme = themes_vec.iter()
                        .find(|(n, _)| n == "base16-ocean.dark")
                        .or_else(|| themes_vec.iter().find(|(n, _)| n == "Monokai"))
                        .map(|(_, t)| t.clone())
                        .unwrap_or_else(|| themes_vec.first().expect("at least one theme").1.clone());
                    return (ss, theme);
                }
            }
        }
        let mut builder = SyntaxSet::load_defaults_newlines().into_builder();
        if let Ok(custom_dir) = Self::custom_syntax_dir() {
            if custom_dir.exists() {
                if let Err(e) = builder.add_from_folder(&custom_dir, false) {
                    eprintln!("Warning: Failed to load custom syntax files from {:?}: {}", custom_dir, e);
                }
            }
        }
        let ss = builder.build();
        let ts = ThemeSet::load_defaults();
        let theme = ts.themes.get("base16-ocean.dark")
            .or_else(|| ts.themes.get("Monokai"))
            .or_else(|| ts.themes.values().next())
            .expect("No themes available")
            .clone();
        (ss, theme)
    }

    fn custom_syntax_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let home = std::env::var("UE_TEST_HOME")
            .or_else(|_| std::env::var("HOME"))
            .or_else(|_| std::env::var("USERPROFILE"))?;
        Ok(PathBuf::from(home).join(".ue").join("syntax"))
    }

    fn file_size(&self, filename: &str) -> Option<u64> {
        if let Ok(mut guard) = self.file_size_cache.lock() {
            if let Some(sz) = guard.get(filename) { return Some(*sz); }
            let sz = std::fs::metadata(filename).ok()?.len();
            guard.insert(filename.to_string(), sz);
            Some(sz)
        } else { None }
    }

    fn find_syntax(&self, filename: &str) -> Option<&SyntaxReference> {
        let ext = std::path::Path::new(filename).extension().and_then(|e| e.to_str());
        if let Some(ext) = ext { self.syntax_set.find_syntax_by_extension(ext) } else { None }
            .or_else(|| self.syntax_set.find_syntax_by_first_line(filename))
    }
}

impl Highlighter for SyntectHighlighter {
    fn highlight_line(&self, line: &str, filename: &str, settings: &Settings) -> Vec<StyledSpan> {
        if !settings.enable_syntax_highlighting { return Vec::new(); }
        if let Some(sz) = self.file_size(filename) { if sz > settings.syntax_max_bytes { return Vec::new(); } }
        let syntax = match self.find_syntax(filename) { Some(s) => s, None => return Vec::new() };
        let mut highlighter = syntect::easy::HighlightLines::new(syntax, &self.theme);
        let ranges = match highlighter.highlight_line(line, &self.syntax_set) { Ok(r) => r, Err(_) => return Vec::new() };
        let mut spans = Vec::new();
        let mut offset = 0;
        for (style, text) in ranges {
            let start = offset; let end = offset + text.len();
            if style.foreground != self.theme.settings.foreground.unwrap_or(syntect::highlighting::Color::WHITE) {
                spans.push(StyledSpan { start, end, color_spec: ColorSpec::from_syntect_style(style) });
            }
            offset = end;
        }
        spans
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::set_temp_home;

    fn test_settings() -> Settings { Settings::load().expect("Failed to load test settings") }
    fn get_hl() -> SyntectHighlighter { SyntectHighlighter::new() }

    #[test]
    fn rust_syntax_highlighting_produces_spans() {
        let (_tmp, _guard) = set_temp_home();
        let hl = get_hl();
        let spans = hl.highlight_line("fn main() {", "test.rs", &test_settings());
        assert!(!spans.is_empty());
    }
    #[test]
    fn python_syntax_highlighting_produces_spans() {
        let (_tmp, _guard) = set_temp_home();
        let hl = get_hl();
        let spans = hl.highlight_line("def hello():", "test.py", &test_settings());
        assert!(!spans.is_empty());
    }
    #[test]
    fn unknown_extension_returns_empty() {
        let (_tmp, _guard) = set_temp_home();
        let hl = get_hl();
        let spans = hl.highlight_line("some text", "test.unknown", &test_settings());
        assert!(spans.is_empty());
    }
    #[test]
    fn large_file_skips() {
        let (_tmp, _guard) = set_temp_home();
        let hl = get_hl();
        let s = test_settings();
        let tmp_path = tempfile::Builder::new().suffix(".rs").tempfile().unwrap();
        std::fs::write(tmp_path.path(), "fn a() {}\n").unwrap();
        let fname = tmp_path.path().to_str().unwrap();
        let spans_small = hl.highlight_line("fn a() {}", fname, &s);
        assert!(!spans_small.is_empty());
        let mut s2 = s.clone(); s2.syntax_max_bytes = 1;
        let spans_large = hl.highlight_line("fn a() {}", fname, &s2);
        assert!(spans_large.is_empty());
    }
}
