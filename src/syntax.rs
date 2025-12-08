use crossterm::style::Color;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
struct Pattern {
    regex: Regex,
    color: Color,
    priority: i32,
}

#[derive(Debug)]
struct SyntaxDefinition {
    patterns: Vec<Pattern>,
}

impl SyntaxDefinition {
    fn new() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    fn add_pattern(
        &mut self,
        pattern: &str,
        color: Color,
        priority: i32,
    ) -> Result<(), regex::Error> {
        let regex = Regex::new(pattern)?;
        self.patterns.push(Pattern {
            regex,
            color,
            priority,
        });
        Ok(())
    }

    /// Apply syntax highlighting to a line, returning styled segments
    /// Returns Vec of (start_byte, end_byte, color)
    fn highlight_line(&self, line: &str) -> Vec<(usize, usize, Color)> {
        let mut segments = Vec::new();

        // Find all matches for all patterns
        for pattern in &self.patterns {
            for mat in pattern.regex.find_iter(line) {
                segments.push((mat.start(), mat.end(), pattern.color, pattern.priority));
            }
        }

        // Sort by priority (higher first), then by start position
        segments.sort_by(|a, b| b.3.cmp(&a.3).then_with(|| a.0.cmp(&b.0)));

        // Remove overlaps - higher priority wins
        let mut result = Vec::new();
        let mut covered: Vec<bool> = vec![false; line.len()];

        for (start, end, color, _priority) in segments {
            let has_uncovered = covered
                .iter()
                .take(end.min(line.len()))
                .skip(start)
                .any(|&c| !c);

            if has_uncovered {
                // Mark as covered
                for item in covered.iter_mut().take(end.min(line.len())).skip(start) {
                    *item = true;
                }
                result.push((start, end, color));
            }
        }

        // Sort by start position for rendering
        result.sort_by_key(|s| s.0);
        result
    }
}

struct SyntaxCache {
    definitions: HashMap<String, Option<SyntaxDefinition>>,
}

impl SyntaxCache {
    fn new() -> Self {
        Self {
            definitions: HashMap::new(),
        }
    }

    fn get_or_load(&mut self, extension: &str) -> Option<&SyntaxDefinition> {
        if !self.definitions.contains_key(extension) {
            let def = Self::load_syntax_file(extension);
            self.definitions.insert(extension.to_string(), def);
        }

        self.definitions.get(extension).and_then(|opt| opt.as_ref())
    }

    fn load_syntax_file(extension: &str) -> Option<SyntaxDefinition> {
        // Use the default_syntax module which handles both user files and embedded defaults
        let content = crate::default_syntax::get_syntax_content(extension)?;
        Self::parse_syntax_file(&content)
    }

    fn parse_syntax_file(content: &str) -> Option<SyntaxDefinition> {
        let mut def = SyntaxDefinition::new();

        for line in content.lines() {
            let line = line.trim();

            // Skip comments and empty lines
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse pattern lines: priority|color|regex
            let parts: Vec<&str> = line.splitn(3, '|').collect();
            if parts.len() != 3 {
                continue;
            }

            let priority = parts[0].trim().parse::<i32>().ok()?;
            let color = Self::parse_color(parts[1].trim())?;
            let pattern = parts[2].trim();

            // Skip invalid patterns
            if def.add_pattern(pattern, color, priority).is_err() {
                continue;
            }
        }

        Some(def)
    }

    fn parse_color(s: &str) -> Option<Color> {
        match s.to_lowercase().as_str() {
            "black" => Some(Color::Black),
            "dark_grey" | "darkgrey" | "dark_gray" | "darkgray" => Some(Color::DarkGrey),
            "red" => Some(Color::Red),
            "dark_red" | "darkred" => Some(Color::DarkRed),
            "green" => Some(Color::Green),
            "dark_green" | "darkgreen" => Some(Color::DarkGreen),
            "yellow" => Some(Color::Yellow),
            "dark_yellow" | "darkyellow" => Some(Color::DarkYellow),
            "blue" => Some(Color::Blue),
            "dark_blue" | "darkblue" => Some(Color::DarkBlue),
            "magenta" => Some(Color::Magenta),
            "dark_magenta" | "darkmagenta" => Some(Color::DarkMagenta),
            "cyan" => Some(Color::Cyan),
            "dark_cyan" | "darkcyan" => Some(Color::DarkCyan),
            "white" => Some(Color::White),
            "grey" | "gray" => Some(Color::Grey),
            _ if s.starts_with('#') && s.len() == 7 => {
                // Parse hex color #RRGGBB
                let r = u8::from_str_radix(&s[1..3], 16).ok()?;
                let g = u8::from_str_radix(&s[3..5], 16).ok()?;
                let b = u8::from_str_radix(&s[5..7], 16).ok()?;
                Some(Color::Rgb { r, g, b })
            }
            _ => None,
        }
    }
}

/// Global syntax highlighter
struct SyntaxHighlighter {
    cache: SyntaxCache,
    current_extension: Option<String>,
}

impl SyntaxHighlighter {
    fn new() -> Self {
        Self {
            cache: SyntaxCache::new(),
            current_extension: None,
        }
    }

    fn set_file(&mut self, filepath: &str) {
        self.current_extension = Path::new(filepath)
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_string());
    }

    fn highlight_line(&mut self, line: &str) -> Vec<(usize, usize, Color)> {
        if let Some(ext) = &self.current_extension
            && let Some(def) = self.cache.get_or_load(ext)
        {
            return def.highlight_line(line);
        }
        Vec::new()
    }
}

// Thread-local singleton for syntax highlighter
use std::cell::RefCell;
thread_local! {
    static HIGHLIGHTER: RefCell<SyntaxHighlighter> = RefCell::new(SyntaxHighlighter::new());
}

/// Set the current file for syntax highlighting
pub(crate) fn set_current_file(filepath: &str) {
    HIGHLIGHTER.with(|h| h.borrow_mut().set_file(filepath));
}

/// Get syntax highlighting for a line
/// Returns Vec of (start_byte, end_byte, color)
pub(crate) fn highlight_line(line: &str) -> Vec<(usize, usize, Color)> {
    HIGHLIGHTER.with(|h| h.borrow_mut().highlight_line(line))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_color() {
        assert!(matches!(SyntaxCache::parse_color("red"), Some(Color::Red)));
        assert!(matches!(
            SyntaxCache::parse_color("dark_blue"),
            Some(Color::DarkBlue)
        ));
        assert!(matches!(
            SyntaxCache::parse_color("darkgreen"),
            Some(Color::DarkGreen)
        ));

        if let Some(Color::Rgb { r, g, b }) = SyntaxCache::parse_color("#FF0000") {
            assert_eq!(r, 255);
            assert_eq!(g, 0);
            assert_eq!(b, 0);
        } else {
            panic!("Failed to parse hex color");
        }
    }

    #[test]
    fn test_highlight_simple() {
        let mut def = SyntaxDefinition::new();
        def.add_pattern(r"\b(fn|let|mut)\b", Color::Blue, 1)
            .unwrap();
        def.add_pattern(r#""[^"]*""#, Color::Green, 2).unwrap();

        let line = r#"let x = "hello";"#;
        let highlights = def.highlight_line(line);

        assert!(!highlights.is_empty());
    }
}
