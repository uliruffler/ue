use std::fs;
use std::path::{Path, PathBuf};
use crate::settings::Settings;
use crate::syntax::{Highlighter, StyledSpan};
use regex::Regex;

#[derive(Debug, Clone)]
struct NanoRule {
    color: crossterm::style::Color,
    bold: bool,
    patterns: Vec<Regex>,
}

#[derive(Debug)]
pub(crate) struct NanorcHighlighter {
    rules: Vec<NanoRule>,
    exts: Vec<String>,
}

impl NanorcHighlighter {
    pub(crate) fn new(settings: &Settings) -> Self {
        let dirs = Self::resolve_dirs(settings);
        let mut rules = Vec::new();
        let mut exts = Vec::new();
        let mut count = 0usize;
        const MAX_FILES: usize = 50;
        
        for d in dirs {
            if count >= MAX_FILES {
                break;
            }
            
            if d.exists() && let Ok(rd) = fs::read_dir(&d) {
                for e in rd.flatten() {
                    if count >= MAX_FILES {
                        break;
                    }
                    let p = e.path();
                    if p.extension().and_then(|s| s.to_str()) == Some("nanorc") {
                        Self::parse_file(&p, &mut rules, &mut exts);
                        count += 1;
                    }
                }
            }
        }
        
        Self { rules, exts }
    }

    fn resolve_dirs(settings: &Settings) -> Vec<PathBuf> {
        let mut v = Vec::new();
        
        if let Ok(home) = std::env::var("UE_TEST_HOME").or_else(|_| std::env::var("HOME")) {
            v.push(PathBuf::from(&home).join(".ue/syntax"));
            for s in &settings.syntax.dirs {
                let exp = if let Some(stripped) = s.strip_prefix("~/") {
                    PathBuf::from(&home).join(stripped)
                } else {
                    PathBuf::from(s)
                };
                v.push(exp);
            }
        }
        
        if std::env::var("UE_DISABLE_SYSTEM_NANORC").ok().as_deref() != Some("1") {
            let sys = PathBuf::from("/usr/share/nano");
            if sys.exists() {
                v.push(sys);
            }
        }
        
        // Deduplicate paths (normalize to handle trailing slashes)
        let mut seen = std::collections::HashSet::new();
        v.retain(|p| {
            let normalized = p.to_string_lossy().trim_end_matches('/').to_string();
            seen.insert(normalized)
        });
        v
    }

    fn parse_file(path: &Path, rules: &mut Vec<NanoRule>, exts: &mut Vec<String>) {
        let Ok(content) = fs::read_to_string(path) else { return };
        let mut lines = 0usize;
        const MAX_LINES: usize = 1000;
        const MAX_RULES_PER_FILE: usize = 200;
        let base = rules.len();
        for line in content.lines() {
            lines += 1;
            if lines > MAX_LINES || rules.len() - base > MAX_RULES_PER_FILE { break; }
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') { continue; }
            let parts = Self::tokenize(t);
            if parts.is_empty() { continue; }
            match parts[0].to_lowercase().as_str() {
                "syntax" => {
                    for p in parts.iter().skip(2) {
                        for ext in Self::extract_extensions(p) {
                            if !exts.contains(&ext) { exts.push(ext); }
                        }
                    }
                }
                "color" | "icolor" => {
                    if parts.len() >= 3 && let Some((c, b)) = Self::parse_color(&parts[1]) {
                        let mut pats = Vec::new();
                        for raw in parts.iter().skip(2) {
                            if !raw.is_empty() { pats.extend(Self::compile_pattern(raw)); }
                        }
                        rules.push(NanoRule { color: c, bold: b, patterns: pats });
                    }
                }
                _ => {}
            }
        }
    }

    fn tokenize(s: &str) -> Vec<String> {
        let mut v = Vec::new();
        let mut cur = String::new();
        let mut in_quotes = false;
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;
        
        while i < chars.len() {
            let c = chars[i];
            
            if c == '\\' && i + 1 < chars.len() {
                // Backslash escape - include both the backslash and next char
                cur.push(c);
                cur.push(chars[i + 1]);
                i += 2;
                continue;
            }
            
            if c == '"' {
                if in_quotes {
                    // End of quoted string
                    in_quotes = false;
                    v.push(cur.clone());
                    cur.clear();
                } else {
                    // Start of quoted string
                    in_quotes = true;
                }
                i += 1;
                continue;
            }
            
            if (c == ' ' || c == '\t') && !in_quotes {
                if !cur.is_empty() {
                    v.push(cur.clone());
                    cur.clear();
                }
                i += 1;
            } else {
                cur.push(c);
                i += 1;
            }
        }
        
        if !cur.is_empty() {
            v.push(cur);
        }
        v
    }

    fn parse_color(n: &str) -> Option<(crossterm::style::Color, bool)> {
        use crossterm::style::Color;
        let l = n.to_lowercase();
        let (c, b) = match l.as_str() {
            "red" => (Color::Red, false),
            "green" => (Color::Green, false),
            "blue" => (Color::Blue, false),
            "yellow" => (Color::Yellow, false),
            "cyan" => (Color::Cyan, false),
            "magenta" => (Color::Magenta, false),
            "white" => (Color::White, false),
            "black" => (Color::Black, false),
            "brightred" => (Color::Red, true),
            "brightgreen" => (Color::Green, true),
            "brightblue" => (Color::Blue, true),
            "brightyellow" => (Color::Yellow, true),
            "brightcyan" => (Color::Cyan, true),
            "brightmagenta" => (Color::Magenta, true),
            _ => return None,
        };
        Some((c, b))
    }

    fn extract_extensions(p: &str) -> Vec<String> {
        let mut result = Vec::new();
        let s = p.replace('"', "");
        
        // Handle glob-style patterns like *.txt or *.sh
        if let Some(stripped) = s.strip_prefix("*.") {
            // Extract extension from *.ext pattern
            let ext: String = stripped.chars()
                .take_while(|c| c.is_alphanumeric())
                .collect();
            if !ext.is_empty() {
                result.push(ext);
                return result;
            }
        }
        
        // Strategy: scan for patterns like \.ext or \.(ext1|ext2|ext3)
        // This handles both simple cases and complex patterns like nano's sh.nanorc
        let mut i = 0;
        let chars: Vec<char> = s.chars().collect();
        
        while i < chars.len() {
            // Look for \. pattern (escaped dot)
            if i + 1 < chars.len() && chars[i] == '\\' && chars[i + 1] == '.' {
                i += 2; // Skip past \.
                
                if i < chars.len() && chars[i] == '(' {
                    // Pattern like \.(sh|bash)
                    i += 1;
                    let start = i;
                    let mut depth = 1;
                    
                    // Find matching closing paren
                    while i < chars.len() && depth > 0 {
                        if chars[i] == '(' { depth += 1; }
                        else if chars[i] == ')' { depth -= 1; }
                        i += 1;
                    }
                    
                    if depth == 0 {
                        let group: String = chars[start..i-1].iter().collect();
                        // Extract alternatives separated by |
                        for part in group.split('|') {
                            let ext: String = part.chars().filter(|c| c.is_alphanumeric()).collect();
                            if !ext.is_empty() && !result.contains(&ext) {
                                result.push(ext);
                            }
                        }
                    }
                } else {
                    // Pattern like \.sh
                    let ext: String = chars[i..].iter()
                        .take_while(|c| c.is_alphanumeric())
                        .collect();
                    let len = ext.len();
                    if !ext.is_empty() && !result.contains(&ext) {
                        result.push(ext);
                    }
                    i += len;
                }
            } else {
                i += 1;
            }
        }
        
        // Also look for literal words that might be filenames without dots
        // like "profile", "Makefile", etc.
        // Look for words that appear after )\. or )word) patterns
        // This handles patterns like: |(/etc/|(^|/)\.)profile)$
        let mut i = 0;
        while i < chars.len() {
            // Look for )word or .)word patterns near the end
            if i > 0 && (chars[i-1] == ')' || chars[i-1] == '.') && chars[i].is_alphabetic() {
                let start = i;
                while i < chars.len() && chars[i].is_alphabetic() {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                
                // Check if this looks like the end of the pattern
                // (followed by ) or $ or end of string)
                let at_end = i >= chars.len() 
                    || (i < chars.len() && (chars[i] == ')' || chars[i] == '$'))
                    || (i + 1 < chars.len() && chars[i] == ')' && chars[i+1] == '$');
                
                // Only add if it looks like a reasonable filename
                if at_end && word.len() >= 3 && word.len() <= 20 
                    && word.chars().all(|c| c.is_lowercase() || c == '_') 
                    && !result.contains(&word) {
                    result.push(word);
                }
            } else {
                i += 1;
            }
        }
        
        result
    }

    fn compile_pattern(raw: &str) -> Vec<Regex> {
        if raw.is_empty() {
            return Vec::new();
        }
        
        let mut r = raw.to_string();
        
        // Convert nanorc regex syntax to Rust regex syntax
        // Replace \< and \> with \b (word boundaries)
        r = r.replace("\\<", "\\b").replace("\\>", "\\b");
        
        // Convert POSIX character classes to Rust regex equivalents
        // [[:space:]] -> [\s]
        r = r.replace("[[:space:]]", r"[\s]");
        // [[:alnum:]] -> [a-zA-Z0-9]
        r = r.replace("[[:alnum:]]", "[a-zA-Z0-9]");
        // [[:alpha:]] -> [a-zA-Z]
        r = r.replace("[[:alpha:]]", "[a-zA-Z]");
        // [[:digit:]] -> [0-9]
        r = r.replace("[[:digit:]]", "[0-9]");
        // [[:xdigit:]] -> [0-9a-fA-F]
        r = r.replace("[[:xdigit:]]", "[0-9a-fA-F]");
        // [[:punct:]] -> punctuation characters
        r = r.replace("[[:punct:]]", r"[!-/:-@\[-`{-~]");
        // [[:blank:]] -> space and tab
        r = r.replace("[[:blank:]]", "[ \\t]");
        // [[:upper:]] -> [A-Z]
        r = r.replace("[[:upper:]]", "[A-Z]");
        // [[:lower:]] -> [a-z]
        r = r.replace("[[:lower:]]", "[a-z]");
        
        // Try to compile the pattern as-is
        match Regex::new(&r) {
            Ok(regex) => vec![regex],
            Err(_) => {
                // If compilation fails, silently ignore and return empty
                Vec::new()
            }
        }
    }


    fn extension_matches(&self, filename: &str) -> bool {
        let ext = Path::new(filename).extension().and_then(|e| e.to_str());
        if let Some(e) = ext && self.exts.iter().any(|x| x == e) {
            return true;
        }
        false
    }
}

impl Highlighter for NanorcHighlighter {
    fn highlight_line(&self, line: &str, filename: &str, settings: &Settings) -> Vec<StyledSpan> {
        if !settings.syntax.enable {
            return Vec::new();
        }
        
        if !self.extension_matches(filename) {
            return Vec::new();
        }
        
        let mut spans = Vec::new();
        let mut covered = vec![false; line.len()];
        
        // Apply rules in order (first match wins for each position)
        for rule in &self.rules {
            for pattern in &rule.patterns {
                // Find all matches in the line
                for m in pattern.find_iter(line) {
                    let start = m.start();
                    let end = m.end();
                    
                    // Skip if this position is already covered
                    if start < covered.len() && covered[start] {
                        continue;
                    }
                    
                    // Check for special comment adjustment:
                    // If pattern matches entire line and contains '#' and line starts with '#',
                    // adjust to start at the '#' (skip leading whitespace)
                    let adj_start = if start == 0
                        && end == line.len()
                        && line.contains('#')
                        && line.trim_start().starts_with('#')
                    {
                        line.find('#').unwrap_or(start)
                    } else {
                        start
                    };
                    
                    spans.push(StyledSpan {
                        start: adj_start,
                        end,
                        color_spec: crate::syntax::ColorSpec {
                            fg: Some(rule.color),
                            bold: rule.bold,
                            italic: false,
                        },
                    });
                    
                    // Mark positions as covered
                    for k in adj_start..end {
                        if k < covered.len() {
                            covered[k] = true;
                        }
                    }
                }
            }
        }
        
        // Sort spans by start position
        if spans.len() > 1 {
            spans.sort_by_key(|s| s.start);
            
            // Merge adjacent spans with same color
            let mut merged: Vec<StyledSpan> = Vec::new();
            for s in spans.into_iter() {
                if let Some(last) = merged.last_mut()
                    && last.end == s.start
                    && last.color_spec.fg == s.color_spec.fg
                    && last.color_spec.bold == s.color_spec.bold
                {
                    last.end = s.end;
                    continue;
                }
                merged.push(s);
            }
            return merged;
        }
        
        spans
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::set_temp_home;
    use crate::syntax::Highlighter;
    
    fn enabled() -> Settings {
        let mut s = Settings::load().unwrap();
        s.syntax.enable = true;
        s
    }
    
    #[test]
    fn debug_pattern_compilation() {
        let pattern = "^[[:space:]]*#.*$";
        let patterns = NanorcHighlighter::compile_pattern(pattern);
        assert_eq!(patterns.len(), 1, "Should compile to one regex pattern");
        
        // Test that it matches a comment line
        let line = "  # this is a comment";
        assert!(patterns[0].is_match(line), "Pattern should match comment line");
    }
    
    #[test]
    fn debug_simple_literal() {
        let patterns = NanorcHighlighter::compile_pattern(";");
        assert_eq!(patterns.len(), 1);
        assert!(patterns[0].is_match("test;"), "Should match semicolon");
    }
    
    #[test]
    fn debug_whitespace_pattern_matching() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let f = b.join("test.nanorc");
        fs::write(&f, "syntax test *.txt\ncolor cyan \"^[[:space:]]+\"\n").unwrap();
        let mut s = enabled();
        s.syntax.dirs.clear();
        let h = NanorcHighlighter::new(&s);
        
        // Check what we loaded
        assert!(!h.rules.is_empty(), "Should have loaded rules");
        assert!(!h.exts.is_empty(), "Should have loaded extensions");
        assert!(h.exts.contains(&"txt".to_string()), "Should have txt extension");
        
        let line = "   text";
        let spans = h.highlight_line(line, "test.txt", &s);
        
        if spans.is_empty() {
            panic!("No spans returned! Rules: {}, Exts: {:?}, Extension matches: {}", 
                h.rules.len(), h.exts, h.extension_matches("test.txt"));
        }
        
        assert_eq!(spans.len(), 1, "Should have 1 span");
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, 3);
    }
    
    // HIGH PRIORITY TESTS
    
    #[test]
    fn test_comment_highlighting_entire_line() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let f = b.join("test.nanorc");
        fs::write(&f, "syntax test *.txt\ncolor cyan \"^[[:space:]]*#.*$\"\n").unwrap();
        let mut s = enabled();
        s.syntax.dirs.clear();
        let h = NanorcHighlighter::new(&s);
        
        let line = "  # this is a comment";
        let spans = h.highlight_line(line, "test.txt", &s);
        assert_eq!(spans.len(), 1, "Should have exactly one span for the comment");
        assert_eq!(spans[0].start, 2, "Comment should start after whitespace at the #");
        assert_eq!(spans[0].end, line.len(), "Comment should extend to end of line");
    }
    
    #[test]
    fn test_no_overlapping_highlights() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let f = b.join("test.nanorc");
        fs::write(&f, "syntax test *.txt\ncolor cyan \"^#.*$\"\ncolor green \"\\b(keyword)\\b\"\n").unwrap();
        let mut s = enabled();
        s.syntax.dirs.clear();
        let h = NanorcHighlighter::new(&s);
        
        let line = "# this has keyword in it";
        let spans = h.highlight_line(line, "test.txt", &s);
        assert_eq!(spans.len(), 1, "Comment should be one span, not split by keyword");
        assert_eq!(spans[0].start, 0, "Comment starts at beginning");
        assert_eq!(spans[0].end, line.len(), "Comment extends to end");
    }
    
    #[test]
    fn test_anchored_patterns() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let f = b.join("test.nanorc");
        fs::write(&f, "syntax test *.txt\ncolor brightmagenta \"^\\[.*\\]$\"\n").unwrap();
        let mut s = enabled();
        s.syntax.dirs.clear();
        let h = NanorcHighlighter::new(&s);
        
        let spans = h.highlight_line("[section]", "test.txt", &s);
        assert_eq!(spans.len(), 1, "Should match anchored pattern");
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, 9);
        
        let spans = h.highlight_line("text [section]", "test.txt", &s);
        assert_eq!(spans.len(), 0, "Should not match when not at line start");
        
        let spans = h.highlight_line("[section] text", "test.txt", &s);
        assert_eq!(spans.len(), 0, "Should not match when not at line end");
    }
    
    #[test]
    fn test_greedy_dot_star_matching() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let f = b.join("test.nanorc");
        fs::write(&f, "syntax test *.txt\ncolor cyan \"#.*$\"\n").unwrap();
        let mut s = enabled();
        s.syntax.dirs.clear();
        let h = NanorcHighlighter::new(&s);
        
        let line = "text # comment with more stuff";
        let spans = h.highlight_line(line, "test.txt", &s);
        assert_eq!(spans.len(), 1, "Should have one span for comment");
        assert_eq!(spans[0].start, 5, "Should start at #");
        assert_eq!(spans[0].end, line.len(), "Should match to end of line (greedy)");
    }
    
    // MEDIUM PRIORITY TESTS
    
    #[test]
    fn test_character_class_matching() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let f = b.join("test.nanorc");
        fs::write(&f, "syntax test *.txt\ncolor brightgreen \"^[[:space:]]*[A-Za-z_][A-Za-z0-9_]*[[:space:]]*=\"\n").unwrap();
        let mut s = enabled();
        s.syntax.dirs.clear();
        let h = NanorcHighlighter::new(&s);
        
        let spans = h.highlight_line("  my_key = value", "test.txt", &s);
        assert_eq!(spans.len(), 1, "Should match key assignment");
        assert_eq!(spans[0].start, 0, "Should start at beginning");
        assert!(spans[0].end >= 9, "Should include key name and equals sign");
        
        let spans = h.highlight_line("tab_width = 4", "test.txt", &s);
        assert_eq!(spans.len(), 1, "Should match different key");
    }
    
    #[test]
    fn test_keyword_alternation() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let f = b.join("test.nanorc");
        fs::write(&f, "syntax test *.txt\ncolor green \"\\b(true|false)\\b\"\n").unwrap();
        let mut s = enabled();
        s.syntax.dirs.clear();
        let h = NanorcHighlighter::new(&s);
        
        let spans = h.highlight_line("value is true", "test.txt", &s);
        assert_eq!(spans.len(), 1, "Should match 'true'");
        assert_eq!(spans[0].start, 9);
        assert_eq!(spans[0].end, 13);
        
        let spans = h.highlight_line("value is false", "test.txt", &s);
        assert_eq!(spans.len(), 1, "Should match 'false'");
        assert_eq!(spans[0].start, 9);
        assert_eq!(spans[0].end, 14);
        
        let spans = h.highlight_line("falsehood", "test.txt", &s);
        assert_eq!(spans.len(), 0, "Should not match partial word");
    }
    
    #[test]
    fn test_multiple_patterns_per_rule() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let f = b.join("test.nanorc");
        fs::write(&f, "syntax test *.txt\ncolor green \"\\bfn\\b\" \"\\blet\\b\" \"\\bpub\\b\"\n").unwrap();
        let mut s = enabled();
        s.syntax.dirs.clear();
        let h = NanorcHighlighter::new(&s);
        
        let spans = h.highlight_line("pub fn test let x", "test.txt", &s);
        assert_eq!(spans.len(), 3, "Should have three separate highlights");
        
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, 3);
        assert_eq!(spans[1].start, 4);
        assert_eq!(spans[1].end, 6);
        assert_eq!(spans[2].start, 12);
        assert_eq!(spans[2].end, 15);
    }
    
    #[test]
    fn test_whitespace_character_class() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let f = b.join("test.nanorc");
        fs::write(&f, "syntax test *.txt\ncolor cyan \"^[[:space:]]+\"\n").unwrap();
        let mut s = enabled();
        s.syntax.dirs.clear();
        let h = NanorcHighlighter::new(&s);
        
        let spans = h.highlight_line("   text", "test.txt", &s);
        assert_eq!(spans.len(), 1, "Should match leading whitespace");
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, 3);
        
        let spans = h.highlight_line("text   more", "test.txt", &s);
        assert_eq!(spans.len(), 0, "Should not match when not at line start");
    }
    
    #[test]
    fn test_extension_matching() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let f = b.join("test.nanorc");
        fs::write(&f, "syntax test *.sh *.bash\ncolor green \"keyword\"\n").unwrap();
        let mut s = enabled();
        s.syntax.dirs.clear();
        let h = NanorcHighlighter::new(&s);
        
        let spans = h.highlight_line("keyword here", "test.sh", &s);
        assert_eq!(spans.len(), 1, "Should match .sh extension");
        
        let spans = h.highlight_line("keyword here", "test.bash", &s);
        assert_eq!(spans.len(), 1, "Should match .bash extension");
        
        let spans = h.highlight_line("keyword here", "test.txt", &s);
        assert_eq!(spans.len(), 0, "Should not match .txt extension");
    }
    
    #[test]
    fn test_syntax_disabled() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let f = b.join("test.nanorc");
        fs::write(&f, "syntax test *.txt\ncolor green \"keyword\"\n").unwrap();
        let mut s = Settings::load().unwrap();
        s.syntax.enable = false;
        s.syntax.dirs.clear();
        let h = NanorcHighlighter::new(&s);
        
        let spans = h.highlight_line("keyword here", "test.txt", &s);
        assert_eq!(spans.len(), 0, "Should not highlight when syntax is disabled");
    }
    
    // HANG PREVENTION TESTS
    
    #[test]
    fn test_file_count_limit_prevents_excessive_loading() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        
        for i in 0..100 {
            let file = b.join(format!("test{}.nanorc", i));
            fs::write(&file, format!("syntax test{} *.txt\ncolor green \"keyword{}\"\n", i, i)).unwrap();
        }
        
        let mut s = enabled();
        s.syntax.dirs.clear();
        let h = NanorcHighlighter::new(&s);
        
        assert!(!h.rules.is_empty(), "Should have loaded some rules");
        assert!(h.rules.len() <= 50, "Should not load more than MAX_FILES");
    }
    
    #[test]
    fn test_line_count_limit_per_file() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let file = b.join("huge.nanorc");
        
        let mut content = String::from("syntax test *.txt\n");
        for i in 0..2000 {
            content.push_str(&format!("color green \"keyword{}\"\n", i));
        }
        fs::write(&file, content).unwrap();
        
        let mut s = enabled();
        s.syntax.dirs.clear();
        
        let start = std::time::Instant::now();
        let h = NanorcHighlighter::new(&s);
        let duration = start.elapsed();
        
        assert!(duration.as_secs() < 1, "Should complete quickly with line limit");
        assert!(!h.rules.is_empty(), "Should have loaded some rules");
        assert!(h.rules.len() < 2000, "Should not have processed all 2000 lines");
    }
    
    #[test]
    fn test_rule_count_limit_per_file() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let file = b.join("many_rules.nanorc");
        
        let mut content = String::from("syntax test *.txt\n");
        for i in 0..300 {
            content.push_str(&format!("color green \"keyword{}\"\n", i));
        }
        fs::write(&file, content).unwrap();
        
        let mut s = enabled();
        s.syntax.dirs.clear();
        let h = NanorcHighlighter::new(&s);
        
        assert!(!h.rules.is_empty(), "Should have loaded some rules");
        assert!(h.rules.len() <= 201, "Should not exceed MAX_RULES_PER_FILE + 1");
    }
    
    #[test]
    fn test_real_world_sh_nanorc_pattern() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let file = b.join("sh.nanorc");
        
        fs::write(
            &file,
            r#"syntax sh "\.sh$" "\.bash$"
color cyan "^[[:space:]]*#.*$"
color green "\<(if|then|else|elif|fi|for|while|do|done|case|esac|function)\>"
color brightblue "\<(echo|exit|return|break|continue)\>"
"#,
        )
        .unwrap();
        
        let mut s = enabled();
        s.syntax.dirs.clear();
        
        let start = std::time::Instant::now();
        let hl = NanorcHighlighter::new(&s);
        let init_duration = start.elapsed();
        
        assert!(init_duration.as_secs() < 1, "Initialization should be quick");
        assert!(!hl.rules.is_empty(), "Should have loaded rules");
        
        let line = "if [ -f file ]; then echo 'test'; fi";
        let start = std::time::Instant::now();
        let _spans = hl.highlight_line(line, "test.sh", &s);
        let highlight_duration = start.elapsed();
        
        assert!(highlight_duration.as_millis() < 100, "Highlighting should be instant");
    }
    
    // SH.NANORC COMPREHENSIVE TESTS
    
    #[test]
    fn test_sh_keywords() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let f = b.join("sh.nanorc");
        fs::write(&f, r#"syntax sh "\.sh$"
color green "\<(if|then|else|fi)\>"
"#).unwrap();
        let mut s = enabled();
        s.syntax.dirs.clear();
        let h = NanorcHighlighter::new(&s);
        
        let line = "if test then else fi endif";
        let spans = h.highlight_line(line, "test.sh", &s);
        
        assert!(spans.iter().any(|s| &line[s.start..s.end] == "if"
            && matches!(s.color_spec.fg, Some(crossterm::style::Color::Green))));
        assert!(spans.iter().any(|s| &line[s.start..s.end] == "then"
            && matches!(s.color_spec.fg, Some(crossterm::style::Color::Green))));
        assert!(spans.iter().any(|s| &line[s.start..s.end] == "else"
            && matches!(s.color_spec.fg, Some(crossterm::style::Color::Green))));
        assert!(spans.iter().any(|s| &line[s.start..s.end] == "fi"
            && matches!(s.color_spec.fg, Some(crossterm::style::Color::Green))));
        
        let green_words: Vec<_> = spans
            .iter()
            .filter(|s| matches!(s.color_spec.fg, Some(crossterm::style::Color::Green)))
            .map(|s| &line[s.start..s.end])
            .collect();
        
        assert!(!green_words.contains(&"test"), "'test' should not be green keyword");
        assert!(!green_words.contains(&"endif"), "'endif' should not match 'if' keyword");
    }
    
    #[test]
    fn test_sh_builtins() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let f = b.join("sh.nanorc");
        fs::write(&f, r#"syntax sh "\.sh$"
color green "\<(export|local)\>"
"#).unwrap();
        let mut s = enabled();
        s.syntax.dirs.clear();
        let h = NanorcHighlighter::new(&s);
        
        let line = "export VAR=1 local x=2 exporter";
        let spans = h.highlight_line(line, "test.sh", &s);
        
        assert!(spans.iter().any(|s| &line[s.start..s.end] == "export"));
        assert!(spans.iter().any(|s| &line[s.start..s.end] == "local"));
        
        let green_words: Vec<_> = spans
            .iter()
            .filter(|s| matches!(s.color_spec.fg, Some(crossterm::style::Color::Green)))
            .map(|s| &line[s.start..s.end])
            .collect();
        assert!(!green_words.contains(&"exporter"), "'exporter' should not match 'export' keyword");
    }
    
    #[test]
    fn test_sh_symbols() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let f = b.join("sh.nanorc");
        // Use regular string with escapes, not raw string
        fs::write(&f, "syntax sh \"\\.sh$\"\ncolor green \";\"\ncolor green \"\\(\"\ncolor green \"\\)\"\n").unwrap();
        let mut s = enabled();
        s.syntax.dirs.clear();
        
        // Test tokenization first
        let test_line = "color green \"\\(\"";
        let tokens = NanorcHighlighter::tokenize(test_line);
        assert_eq!(tokens.len(), 3, "Should have 3 tokens");
        assert_eq!(tokens[2], r"\(", "Third token should be backslash-paren");
        
        // Test pattern compilation
        let patterns = NanorcHighlighter::compile_pattern(r"\(");
        assert_eq!(patterns.len(), 1, "Pattern \\( should compile");
        
        let h = NanorcHighlighter::new(&s);
        
        assert!(!h.rules.is_empty(), "Should have loaded rules");
        assert!(!h.exts.is_empty(), "Should have loaded extensions");
        
        let line = "func(); other()";
        let spans = h.highlight_line(line, "test.sh", &s);
        
        assert!(!spans.is_empty(), "Should have some spans, got: {}", spans.len());
        
        // Check that each symbol is covered by a green span
        let semicolon_pos = line.find(';').unwrap();
        let open_paren_pos = line.find('(').unwrap();
        let close_paren_pos = line.find(')').unwrap();
        
        assert!(spans.iter().any(|s| s.start <= semicolon_pos && s.end > semicolon_pos
            && matches!(s.color_spec.fg, Some(crossterm::style::Color::Green))), 
            "Semicolon should be highlighted");
        assert!(spans.iter().any(|s| s.start <= open_paren_pos && s.end > open_paren_pos
            && matches!(s.color_spec.fg, Some(crossterm::style::Color::Green))), 
            "Open paren should be highlighted");
        assert!(spans.iter().any(|s| s.start <= close_paren_pos && s.end > close_paren_pos
            && matches!(s.color_spec.fg, Some(crossterm::style::Color::Green))), 
            "Close paren should be highlighted");
    }
    
    #[test]
    fn test_sh_comments() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let f = b.join("sh.nanorc");
        fs::write(&f, r#"syntax sh "\.sh$"
color cyan "^#.*$"
color cyan " #.*$"
"#).unwrap();
        let mut s = enabled();
        s.syntax.dirs.clear();
        let h = NanorcHighlighter::new(&s);
        
        let line1 = "# this is a comment";
        let spans1 = h.highlight_line(line1, "test.sh", &s);
        assert_eq!(spans1.len(), 1);
        assert_eq!(spans1[0].start, 0);
        assert_eq!(spans1[0].end, line1.len());
        assert!(matches!(spans1[0].color_spec.fg, Some(crossterm::style::Color::Cyan)));
        
        let line2 = "echo test # comment";
        let spans2 = h.highlight_line(line2, "test.sh", &s);
        
        let cyan_spans: Vec<_> = spans2
            .iter()
            .filter(|s| matches!(s.color_spec.fg, Some(crossterm::style::Color::Cyan)))
            .collect();
        
        assert!(!cyan_spans.is_empty());
        assert!(cyan_spans.iter().any(|s| s.end == line2.len()));
    }
    
    #[test]
    fn test_parse_sh_nanorc_file() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let f = b.join("sh.nanorc");
        
        // Create a minimal sh.nanorc
        fs::write(&f, "syntax sh \"\\.sh$\" \"\\.bash$\"\ncolor green \"echo\"\n").unwrap();
        
        let mut s = enabled();
        s.syntax.dirs.clear();
        
        // This should only load the one file we created
        let h = NanorcHighlighter::new(&s);
        
        // Should have loaded exactly one file with sh and bash extensions
        assert!(h.exts.contains(&"sh".to_string()), "Should have 'sh' extension");
        assert!(h.exts.contains(&"bash".to_string()), "Should have 'bash' extension");
        assert!(h.rules.len() > 0, "Should have loaded at least one rule");
        
        // Should match .sh files
        assert!(h.extension_matches("test.sh"));
        assert!(h.extension_matches("script.bash"));
    }
    
    #[test]
    fn test_syntax_line_tokenization() {
        let line1 = r#"syntax sh "\.sh$" "\.bash$""#;
        let tokens1 = NanorcHighlighter::tokenize(line1);
        assert_eq!(tokens1[0], "syntax");
        assert_eq!(tokens1[1], "sh");
        assert_eq!(tokens1[2], "\\.sh$");
        assert_eq!(tokens1[3], "\\.bash$");
        
        // Test extension extraction
        let exts1 = NanorcHighlighter::extract_extensions(&tokens1[2]);
        let exts2 = NanorcHighlighter::extract_extensions(&tokens1[3]);
        assert!(exts1.contains(&"sh".to_string()));
        assert!(exts2.contains(&"bash".to_string()));
    }
    
    #[test]
    fn test_extract_extensions_simple_and_alternation() {
        // Simple patterns
        let single = NanorcHighlighter::extract_extensions("\\.sh$");
        assert_eq!(single, vec!["sh"]);
        
        // Alternation
        let alt = NanorcHighlighter::extract_extensions("\\.(sh|bash|zsh)$");
        assert_eq!(alt, vec!["sh", "bash", "zsh"]);
        
        let mixed = NanorcHighlighter::extract_extensions("name \\.(py|rs)$ other");
        assert_eq!(mixed, vec!["py", "rs"]);
        
        // Complex nano sh.nanorc pattern
        let complex = NanorcHighlighter::extract_extensions(
            r#"(\.sh|(^|/|\.)(a|ba|c|da|k|mk|pdk|tc|z)sh(rc|_profile)?|(/etc/|(^|/)\.)profile)$"#
        );
        assert!(complex.contains(&"sh".to_string()), "Should extract 'sh' from complex pattern, got: {:?}", complex);
        assert!(complex.contains(&"profile".to_string()), "Should extract 'profile' from complex pattern");
    }
    
    #[test]
    fn test_sh_extension_loaded_from_alternation_syntax_line() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let f = b.join("sh.nanorc");
        fs::write(&f, "syntax sh \"\\.(sh|bash)$\"\ncolor green \"echo\"\n").unwrap();
        let mut s = enabled();
        s.syntax.dirs.clear();
        let h = NanorcHighlighter::new(&s);
        assert!(h.extension_matches("test.sh"));
        assert!(h.extension_matches("test.bash"));
        assert!(!h.extension_matches("test.py"));
        let spans = h.highlight_line("echo test", "file.sh", &s);
        assert!(!spans.is_empty());
    }
    
    #[test]
    fn test_negated_character_class() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let f = b.join("test.nanorc");
        // Pattern: double-quoted string with [^"] meaning "any char except quote"
        fs::write(&f, "syntax test *.txt\ncolor brightyellow \"\\\"[^\\\"]*\\\"\"\n").unwrap();
        let mut s = enabled();
        s.syntax.dirs.clear();
        let h = NanorcHighlighter::new(&s);
        
        let line = r#"echo "hello world""#;
        let spans = h.highlight_line(line, "test.txt", &s);
        
        // Should match the quoted string "hello world"
        assert_eq!(spans.len(), 1, "Should have one span for the string");
        assert_eq!(&line[spans[0].start..spans[0].end], r#""hello world""#, "Should match the entire quoted string");
        assert!(spans[0].color_spec.bold, "brightyellow should be bold");
        assert!(matches!(spans[0].color_spec.fg, Some(crossterm::style::Color::Yellow)), "Should be yellow");
    }
    
    #[test]
    fn test_sh_string_highlighting() {
        unsafe { std::env::set_var("UE_DISABLE_SYSTEM_NANORC", "1"); }
        let (tmp, _g) = set_temp_home();
        let b = tmp.path().join(".ue/syntax");
        fs::create_dir_all(&b).unwrap();
        let f = b.join("sh.nanorc");
        fs::write(&f, r#"syntax sh "\.sh$"
color brightyellow "\"[^\"]*\""
color brightyellow "'[^']*'"
"#).unwrap();
        let mut s = enabled();
        s.syntax.dirs.clear();
        let h = NanorcHighlighter::new(&s);
        
        let line1 = r#"echo "test string""#;
        let spans1 = h.highlight_line(line1, "test.sh", &s);
        
        assert!(!spans1.is_empty(), "Should highlight double-quoted string");
        assert!(spans1.iter().any(|s| {
            &line1[s.start..s.end] == r#""test string""# 
            && s.color_spec.bold 
            && matches!(s.color_spec.fg, Some(crossterm::style::Color::Yellow))
        }), "Should have yellow bold string span");
        
        let line2 = "echo 'single quoted'";
        let spans2 = h.highlight_line(line2, "test.sh", &s);
        
        assert!(!spans2.is_empty(), "Should highlight single-quoted string");
        assert!(spans2.iter().any(|s| {
            &line2[s.start..s.end] == "'single quoted'" 
            && s.color_spec.bold 
            && matches!(s.color_spec.fg, Some(crossterm::style::Color::Yellow))
        }), "Should have yellow bold string span for single quotes");
    }
}
