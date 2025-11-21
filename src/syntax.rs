use syntect::highlighting::{ThemeSet, Style, Theme};
use syntect::parsing::SyntaxSet;
use std::sync::OnceLock;

/// Lazily loaded syntax set
static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();

/// Lazily loaded theme
static THEME: OnceLock<Theme> = OnceLock::new();

/// Get the global syntax set (loaded once)
fn get_syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(|| SyntaxSet::load_defaults_newlines())
}

/// Get the global theme (loaded once)
fn get_theme() -> &'static Theme {
    THEME.get_or_init(|| {
        let ts = ThemeSet::load_defaults();
        // Use a dark theme that works well in terminals
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

/// Highlight a line of text using syntect
pub(crate) fn highlight_line(line: &str, filename: &str) -> Vec<StyledSpan> {
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

    #[test]
    fn test_rust_syntax_highlighting() {
        let line = "fn main() {";
        let spans = highlight_line(line, "test.rs");
        
        // Should have some highlighted spans for Rust keywords
        assert!(!spans.is_empty(), "Rust syntax should produce highlights");
    }

    #[test]
    fn test_python_syntax_highlighting() {
        let line = "def hello():";
        let spans = highlight_line(line, "test.py");
        
        // Should have some highlighted spans for Python keywords
        assert!(!spans.is_empty(), "Python syntax should produce highlights");
    }

    #[test]
    fn test_unknown_extension() {
        let line = "some text";
        let spans = highlight_line(line, "test.unknown");
        
        // Unknown files should return empty spans
        assert!(spans.is_empty(), "Unknown extension should not highlight");
    }

    #[test]
    fn test_no_extension() {
        let line = "#!/bin/bash";
        let spans = highlight_line(line, "script");
        
        // Should try to detect by content
        // Result may vary, just ensure it doesn't crash
        let _ = spans;
    }
}

