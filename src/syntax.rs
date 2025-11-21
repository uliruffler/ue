use syntect::highlighting::{ThemeSet, Style, Theme};
use syntect::parsing::SyntaxSet;
use std::sync::{OnceLock, Mutex};
use std::collections::HashMap;
use std::path::PathBuf;
use bincode; // for deserialization of precompiled assets

/// Lazily loaded syntax set
static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();

/// Lazily loaded theme
static THEME: OnceLock<Theme> = OnceLock::new();

/// Lazily loaded file size cache
static FILE_SIZE_CACHE: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();

/// Get the global syntax set (loaded once)
fn get_syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(|| {
        if let Ok(path) = std::env::var("UE_PRECOMPILED_SYNTECT") {
            if let Ok(data) = std::fs::read(&path) {
                if let Ok((ss, _themes)) = bincode::deserialize::<(SyntaxSet, Vec<(String, Theme)>)>(&data) {
                    return ss;
                }
            }
        }
        let mut builder = SyntaxSet::load_defaults_newlines().into_builder();
        
        // Load custom syntax files from ~/.ue/syntax/
        if let Ok(custom_syntax_dir) = get_custom_syntax_dir() {
            if custom_syntax_dir.exists() {
                // Load all .sublime-syntax files from the directory
                if let Err(e) = builder.add_from_folder(&custom_syntax_dir, false) {
                    eprintln!("Warning: Failed to load custom syntax files from {:?}: {}", custom_syntax_dir, e);
                }
            }
        }
        
        builder.build()
    })
}

/// Get the path to the custom syntax directory
fn get_custom_syntax_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = std::env::var("UE_TEST_HOME")
        .or_else(|_| std::env::var("HOME"))
        .or_else(|_| std::env::var("USERPROFILE"))?;
    Ok(PathBuf::from(home).join(".ue").join("syntax"))
}

/// Get the global theme (loaded once)
fn get_theme() -> &'static Theme {
    THEME.get_or_init(|| {
        if let Ok(path) = std::env::var("UE_PRECOMPILED_SYNTECT") {
            if let Ok(data) = std::fs::read(&path) {
                if let Ok((_ss, themes)) = bincode::deserialize::<(SyntaxSet, Vec<(String, Theme)>)>(&data) {
                    if let Some((_, t)) = themes.iter().find(|(n, _)| n == "base16-ocean.dark") { return t.clone(); }
                    if let Some((_, t)) = themes.iter().find(|(n, _)| n == "Monokai") { return t.clone(); }
                    return themes.first().expect("at least one theme").1.clone();
                }
            }
        }
        let ts = ThemeSet::load_defaults();
        ts.themes.get("base16-ocean.dark")
            .or_else(|| ts.themes.get("Monokai"))
            .or_else(|| ts.themes.values().next())
            .expect("No themes available")
            .clone()
    })
}

/// Get syntax definition for a file extension
fn get_syntax_by_extension(ext: &str) -> Option<&'static syntect::parsing::SyntaxReference> {
    get_syntax_set().find_syntax_by_extension(ext)
}

/// Get syntax definition for a filename
fn get_syntax_by_name(filename: &str) -> Option<&'static syntect::parsing::SyntaxReference> {
    get_syntax_set().find_syntax_by_extension(filename)
        .or_else(|| get_syntax_set().find_syntax_by_first_line(filename))
}

/// Color and style specification for syntax highlighting.
#[derive(Debug, Clone)]
struct ColorSpec {
    fg: Option<crossterm::style::Color>,
    bold: bool,
    italic: bool,
}

impl ColorSpec {
    /// Create from syntect Style
    fn from_syntect_style(style: Style) -> Self {
        Self {
            fg: Some(crossterm::style::Color::Rgb {
                r: style.foreground.r,
                g: style.foreground.g,
                b: style.foreground.b,
            }),
            bold: style.font_style.contains(syntect::highlighting::FontStyle::BOLD),
            italic: style.font_style.contains(syntect::highlighting::FontStyle::ITALIC),
        }
    }

    /// Apply this color specification to stdout
    fn apply_to_stdout(
        &self,
        stdout: &mut impl std::io::Write,
    ) -> Result<(), std::io::Error> {
        use crossterm::execute;
        use crossterm::style::{SetForegroundColor, SetAttribute, Attribute};
        
        if let Some(fg) = self.fg {
            execute!(stdout, SetForegroundColor(fg))?;
        }
        if self.bold {
            execute!(stdout, SetAttribute(Attribute::Bold))?;
        }
        if self.italic {
            execute!(stdout, SetAttribute(Attribute::Italic))?;
        }
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
    /// Apply color to stdout
    pub(crate) fn apply_to_stdout(&self, stdout: &mut impl std::io::Write) -> Result<(), std::io::Error> {
        self.color_spec.apply_to_stdout(stdout)
    }
}

/// Get file size in bytes, using a cache
fn file_size(filename: &str) -> Option<u64> {
    let cache = FILE_SIZE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    {
        let guard = cache.lock().ok()?;
        if let Some(sz) = guard.get(filename) { return Some(*sz); }
    }
    let sz = std::fs::metadata(filename).ok()?.len();
    if let Ok(mut guard) = cache.lock() { guard.insert(filename.to_string(), sz); }
    Some(sz)
}

/// Highlight a line of text using syntect
pub(crate) fn highlight_line(line: &str, filename: &str, settings: &crate::settings::Settings) -> Vec<StyledSpan> {
    // Skip highlighting if disabled or file too large
    if !settings.enable_syntax_highlighting { return Vec::new(); }
    if let Some(sz) = file_size(filename) { if sz > settings.syntax_max_bytes { return Vec::new(); } }
    
    // Get file extension
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str());
    
    // Find syntax definition
    let syntax = if let Some(ext) = ext {
        get_syntax_by_extension(ext)
    } else {
        get_syntax_by_name(filename)
    };
    
    let syntax = match syntax {
        Some(s) => s,
        None => return Vec::new(), // No syntax highlighting for unknown files
    };
    
    // Highlight the line
    let mut highlighter = syntect::easy::HighlightLines::new(syntax, get_theme());
    let ranges = match highlighter.highlight_line(line, get_syntax_set()) {
        Ok(ranges) => ranges,
        Err(_) => return Vec::new(),
    };
    
    // Convert to our StyledSpan format
    let mut spans = Vec::new();
    let mut offset = 0;
    
    for (style, text) in ranges {
        let start = offset;
        let end = offset + text.len();
        
        // Only create span if it has meaningful styling
        if style.foreground != get_theme().settings.foreground.unwrap_or(syntect::highlighting::Color::WHITE) {
            spans.push(StyledSpan {
                start,
                end,
                color_spec: ColorSpec::from_syntect_style(style),
            });
        }
        
        offset = end;
    }
    
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_settings() -> crate::settings::Settings { crate::settings::Settings { keybindings: crate::settings::KeyBindings { quit:"Esc".into(), copy:"Ctrl+c".into(), paste:"Ctrl+v".into(), cut:"Ctrl+x".into(), close:"Ctrl+w".into(), save:"Ctrl+s".into(), undo:"Ctrl+z".into(), redo:"Ctrl+y".into(), file_selector:"Esc".into() }, line_number_digits:3, enable_syntax_highlighting:true, tab_width:4, double_tap_speed_ms:300, header_bg:"#001848".into(), footer_bg:"#001848".into(), line_numbers_bg:"#001848".into(), syntax_max_bytes:500_000 } }
    #[test]
    fn test_rust_syntax_highlighting() { let line = "fn main() {"; let spans = highlight_line(line, "test.rs", &test_settings()); assert!(!spans.is_empty(), "Rust syntax should produce highlights"); }
    #[test]
    fn test_python_syntax_highlighting() { let line = "def hello():"; let spans = highlight_line(line, "test.py", &test_settings()); assert!(!spans.is_empty(), "Python syntax should produce highlights"); }
    #[test]
    fn test_unknown_extension() { let line = "some text"; let spans = highlight_line(line, "test.unknown", &test_settings()); assert!(spans.is_empty(), "Unknown extension should not highlight"); }
    #[test]
    fn test_no_extension() { let line = "#!/bin/bash"; let spans = highlight_line(line, "script", &test_settings()); let _ = spans; }
    #[test]
    fn test_large_file_skips() { 
        let s = test_settings(); 
        let tmp_path = tempfile::Builder::new().suffix(".rs").tempfile().unwrap();
        std::fs::write(tmp_path.path(), "fn a() {}\n").unwrap(); // small file highlights
        let fname = tmp_path.path().to_str().unwrap();
        let spans_small = highlight_line("fn a() {}", fname, &s); assert!(!spans_small.is_empty(), "Expected highlighting for small Rust file");
        // simulate large by setting threshold tiny
        let mut s2 = s.clone(); s2.syntax_max_bytes = 1; let spans_large = highlight_line("fn a() {}", fname, &s2); assert!(spans_large.is_empty(), "Expected skip for large threshold"); }
}
