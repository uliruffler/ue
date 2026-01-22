use crossterm::style::Color;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
enum SwitchAction {
    /// Switch to a specific syntax extension
    SwitchTo(String),
    /// Switch back to previous syntax (pop stack)
    SwitchBack,
}

#[derive(Debug, Clone)]
struct Pattern {
    regex: Regex,
    color: Color,
    priority: i32,
    /// Optional syntax switching action when this pattern matches
    switch_action: Option<SwitchAction>,
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
        switch_action: Option<SwitchAction>,
    ) -> Result<(), regex::Error> {
        let regex = Regex::new(pattern)?;
        self.patterns.push(Pattern {
            regex,
            color,
            priority,
            switch_action,
        });
        Ok(())
    }

    /// Apply syntax highlighting to a line, returning styled segments and optional switch action
    /// Returns (Vec of (start_byte, end_byte, color), Option<(SwitchAction, captured_extension)>)
    fn highlight_line(&self, line: &str) -> (Vec<(usize, usize, Color)>, Option<(SwitchAction, String)>) {
        let mut segments = Vec::new();
        let mut switch_result: Option<(SwitchAction, String)> = None;

        // Find all matches for all patterns
        for pattern in &self.patterns {
            for mat in pattern.regex.find_iter(line) {
                segments.push((mat.start(), mat.end(), pattern.color, pattern.priority));

                // Check for switch action - use highest priority switch action found
                if let Some(ref action) = pattern.switch_action {
                    if switch_result.is_none() || switch_result.as_ref().map_or(false, |(_, _)| true) {
                        // Extract captured group if present (for switch_to=$1 style)
                        let extension = if let SwitchAction::SwitchTo(template) = action {
                            // Check if template contains $1 capture group reference
                            if template.contains("$1") {
                                // Re-match to get captures
                                if let Some(caps) = pattern.regex.captures(line) {
                                    if let Some(capture) = caps.get(1) {
                                        template.replace("$1", capture.as_str())
                                    } else {
                                        template.clone()
                                    }
                                } else {
                                    template.clone()
                                }
                            } else {
                                template.clone()
                            }
                        } else {
                            String::new()
                        };
                        switch_result = Some((action.clone(), extension));
                    }
                }
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
        (result, switch_result)
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

    /// Map common language aliases to their canonical syntax file extensions
    fn resolve_alias(extension: &str) -> &str {
        match extension {
            "bash" | "shell" | "zsh" => "sh",
            "rust" => "rs",
            "python" => "py",
            "csharp" => "cs",
            "javascript" => "js",
            "typescript" => "ts",
            "cpp" | "c++" | "cxx" => "cpp",
            "markdown" => "md",
            "yml" => "yaml",
            "text" => "txt",
            _ => extension,
        }
    }

    fn get_or_load(&mut self, extension: &str) -> Option<&SyntaxDefinition> {
        // Resolve aliases before looking up
        let resolved = Self::resolve_alias(extension);

        if !self.definitions.contains_key(resolved) {
            let def = Self::load_syntax_file(resolved);
            self.definitions.insert(resolved.to_string(), def);
        }

        self.definitions.get(resolved).and_then(|opt| opt.as_ref())
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

            // Parse pattern lines: priority|color|regex[|switch_to=ext or |switch_back]
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() < 3 {
                continue;
            }

            let priority = parts[0].trim().parse::<i32>().ok()?;
            let color = Self::parse_color(parts[1].trim())?;
            let pattern = parts[2].trim();

            // Check for switch directive in 4th field
            let switch_action = if parts.len() > 3 {
                let directive = parts[3].trim();
                if directive == "switch_back" {
                    Some(SwitchAction::SwitchBack)
                } else if let Some(ext) = directive.strip_prefix("switch_to=") {
                    Some(SwitchAction::SwitchTo(ext.to_string()))
                } else {
                    None
                }
            } else {
                None
            };

            // Skip invalid patterns
            if def.add_pattern(pattern, color, priority, switch_action).is_err() {
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
    base_extension: Option<String>,
    /// Stack of syntax contexts - last is current
    syntax_stack: Vec<String>,
}

impl SyntaxHighlighter {
    fn new() -> Self {
        Self {
            cache: SyntaxCache::new(),
            base_extension: None,
            syntax_stack: Vec::new(),
        }
    }

    fn set_file(&mut self, filepath: &str) {
        self.base_extension = Path::new(filepath)
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_string());
        // Reset stack when changing files
        self.syntax_stack.clear();
    }

    fn current_extension(&self) -> Option<&str> {
        // Use top of stack if present, otherwise base extension
        self.syntax_stack.last()
            .map(|s| s.as_str())
            .or(self.base_extension.as_deref())
    }

    fn push_syntax(&mut self, extension: String) {
        self.syntax_stack.push(extension);
    }

    fn pop_syntax(&mut self) {
        self.syntax_stack.pop();
    }

    fn clear_syntax_stack(&mut self) {
        self.syntax_stack.clear();
    }

    fn highlight_line(&mut self, line: &str) -> (Vec<(usize, usize, Color)>, Option<(SwitchAction, String)>) {
        let ext = self.current_extension().map(|s| s.to_string());
        let base_ext = self.base_extension.clone();
        let is_embedded = !self.syntax_stack.is_empty();

        if let Some(ext_str) = ext
            && let Some(def) = self.cache.get_or_load(&ext_str)
        {
            let (highlights, switch) = def.highlight_line(line);

            // If we're in an embedded language and didn't find a switch action,
            // also check the base syntax for switch_back patterns
            if is_embedded && switch.is_none() {
                if let Some(ref base) = base_ext {
                    if let Some(base_def) = self.cache.get_or_load(base) {
                        let (_base_highlights, base_switch) = base_def.highlight_line(line);
                        // Only use base_switch if it's a switch_back action
                        if let Some((ref action, ref ext)) = base_switch {
                            if matches!(action, SwitchAction::SwitchBack) {
                                return (highlights, Some((action.clone(), ext.clone())));
                            }
                        }
                    }
                }
            }

            return (highlights, switch);
        }
        (Vec::new(), None)
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

/// Push a syntax override onto the stack (for embedded languages)
pub(crate) fn push_syntax(extension: &str) {
    HIGHLIGHTER.with(|h| h.borrow_mut().push_syntax(extension.to_string()));
}

/// Pop the last syntax override from the stack
pub(crate) fn pop_syntax() {
    HIGHLIGHTER.with(|h| h.borrow_mut().pop_syntax());
}

/// Clear all syntax overrides and return to base file syntax
pub(crate) fn clear_syntax_stack() {
    HIGHLIGHTER.with(|h| h.borrow_mut().clear_syntax_stack());
}

/// Get syntax highlighting for a line, with optional switch action
/// Returns (Vec of (start_byte, end_byte, color), Option<(is_switch_back, extension)>)
/// where is_switch_back is true for switch_back, false for switch_to with the extension name
pub(crate) fn highlight_line(line: &str) -> (Vec<(usize, usize, Color)>, Option<(bool, String)>) {
    let (highlights, switch) = HIGHLIGHTER.with(|h| h.borrow_mut().highlight_line(line));

    // Convert SwitchAction to simpler bool + string tuple
    let switch_result = switch.map(|(action, ext)| {
        match action {
            SwitchAction::SwitchBack => (true, ext),
            SwitchAction::SwitchTo(_) => (false, ext),
        }
    });

    (highlights, switch_result)
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
        def.add_pattern(r"\b(fn|let|mut)\b", Color::Blue, 1, None)
            .unwrap();
        def.add_pattern(r#""[^"]*""#, Color::Green, 2, None).unwrap();

        let line = r#"let x = "hello";"#;
        let (highlights, _) = def.highlight_line(line);

        assert!(!highlights.is_empty());
    }

    #[test]
    fn test_syntax_switch_to() {
        let mut def = SyntaxDefinition::new();
        // Pattern that triggers switch to rust syntax
        def.add_pattern(
            r"^```(rs)$",
            Color::Green,
            10,
            Some(SwitchAction::SwitchTo("$1".to_string())),
        )
        .unwrap();

        let line = "```rs";
        let (_highlights, switch) = def.highlight_line(line);

        assert!(switch.is_some());
        let (action, ext) = switch.unwrap();
        assert!(matches!(action, SwitchAction::SwitchTo(_)));
        assert_eq!(ext, "rs");
    }

    #[test]
    fn test_syntax_switch_back() {
        let mut def = SyntaxDefinition::new();
        // Pattern that triggers switch back
        def.add_pattern(r"^```$", Color::Green, 10, Some(SwitchAction::SwitchBack))
            .unwrap();

        let line = "```";
        let (_highlights, switch) = def.highlight_line(line);

        assert!(switch.is_some());
        let (action, _) = switch.unwrap();
        assert!(matches!(action, SwitchAction::SwitchBack));
    }

    #[test]
    fn test_resolve_alias() {
        assert_eq!(SyntaxCache::resolve_alias("bash"), "sh");
        assert_eq!(SyntaxCache::resolve_alias("rust"), "rs");
        assert_eq!(SyntaxCache::resolve_alias("python"), "py");
        assert_eq!(SyntaxCache::resolve_alias("rs"), "rs"); // Already canonical
        assert_eq!(SyntaxCache::resolve_alias("unknown"), "unknown"); // No alias
    }

    #[test]
    fn test_syntax_highlighter_stack() {
        let mut highlighter = SyntaxHighlighter::new();

        // Simulate setting a markdown file
        highlighter.base_extension = Some("md".to_string());
        assert_eq!(highlighter.current_extension(), Some("md"));

        // Push rust syntax
        highlighter.push_syntax("rs".to_string());
        assert_eq!(highlighter.current_extension(), Some("rs"));

        // Push python syntax
        highlighter.push_syntax("py".to_string());
        assert_eq!(highlighter.current_extension(), Some("py"));

        // Pop back to rust
        highlighter.pop_syntax();
        assert_eq!(highlighter.current_extension(), Some("rs"));

        // Pop back to markdown
        highlighter.pop_syntax();
        assert_eq!(highlighter.current_extension(), Some("md"));

        // Clear stack
        highlighter.push_syntax("cs".to_string());
        highlighter.clear_syntax_stack();
        assert_eq!(highlighter.current_extension(), Some("md"));
    }
}
