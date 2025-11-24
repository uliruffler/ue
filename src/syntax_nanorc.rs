use std::fs;
use std::path::{Path, PathBuf};
use crate::settings::Settings;
use crate::syntax::{Highlighter, StyledSpan, ColorSpec};

// Minimal NanoRC syntax parser: supports lines starting with "syntax" and "color".
// Example subset:
// syntax rust ".*\.rs" "Cargo.toml"
// color brightgreen "fn" "let" "pub" 
// color cyan "[A-Z_][A-Z0-9_]+" (NOT IMPLEMENTED: regex, treat as literal if no meta chars)
// For simplicity we treat patterns with '*' as wildcard matching (multi-char), '?' single char.

#[derive(Debug, Clone)]
struct NanoRule {
    color: crossterm::style::Color,
    bold: bool,
    patterns: Vec<Pattern>,
}

#[derive(Debug, Clone)]
enum Pattern {
    Keyword(String),
    KeywordSet(Vec<String>),
    Wildcard(String),
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
        for d in dirs {
            if d.exists() {
                if let Ok(rd) = fs::read_dir(&d) {
                    for entry in rd.flatten() {
                        let p = entry.path();
                        if p.extension().and_then(|s| s.to_str()) == Some("nanorc") {
                            Self::parse_file(&p, &mut rules, &mut exts);
                        }
                    }
                }
            }
        }
        Self { rules, exts }
    }

    fn resolve_dirs(settings: &Settings) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Ok(home) = std::env::var("UE_TEST_HOME")
            .or_else(|_| std::env::var("HOME"))
            .or_else(|_| std::env::var("USERPROFILE"))
        {
            // default user dir
            dirs.push(PathBuf::from(&home).join(".ue").join("syntax"));
            // configured dirs (tilde expansion)
            for s in &settings.syntax.dirs {
                let expanded = if let Some(stripped) = s.strip_prefix("~/") { PathBuf::from(&home).join(stripped) } else { PathBuf::from(s) };
                dirs.push(expanded);
            }
        }
        // system-wide nano syntax
        let system = PathBuf::from("/usr/share/nano");
        if system.exists() { dirs.push(system); }
        // dedupe
        let mut seen = std::collections::HashSet::new();
        dirs.retain(|p| {
            let canon = p.to_string_lossy().to_string();
            if seen.contains(&canon) { false } else { seen.insert(canon); true }
        });
        dirs
    }

    fn parse_file(path: &Path, rules: &mut Vec<NanoRule>, exts: &mut Vec<String>) {
        let content = match fs::read_to_string(path) { Ok(c) => c, Err(_) => return };
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') { continue; }
            let parts = Self::tokenize(trimmed);
            if parts.is_empty() { continue; }
            match parts[0].to_lowercase().as_str() {
                "syntax" => {
                    for pat in parts.iter().skip(2) {
                        if let Some(ext) = Self::extract_extension(pat) { exts.push(ext); }
                    }
                }
                "color" | "icolor" => {
                    if parts.len() >= 3 {
                        if let Some((color, bold)) = Self::parse_color(&parts[1]) {
                            let mut pats = Vec::new();
                            for raw in parts.iter().skip(2) {
                                if raw.is_empty() { continue; }
                                pats.extend(Self::compile_pattern(raw));
                            }
                            rules.push(NanoRule { color, bold, patterns: pats });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn tokenize(line: &str) -> Vec<String> {
        let mut parts = Vec::new();
        let mut cur = String::new();
        let mut in_q = false;
        for c in line.chars() {
            match c {
                '"' => {
                    in_q = !in_q;
                    if !in_q { parts.push(cur.clone()); cur.clear(); }
                }
                ' ' | '\t' if !in_q => {
                    if !cur.is_empty() { parts.push(cur.clone()); cur.clear(); }
                }
                _ => cur.push(c),
            }
        }
        if !cur.is_empty() { parts.push(cur); }
        parts
    }

    fn parse_color(name: &str) -> Option<(crossterm::style::Color, bool)> {
        use crossterm::style::Color;
        let n = name.to_lowercase();
        let (c, b) = match n.as_str() {
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

    fn extract_extension(pattern: &str) -> Option<String> {
        let p = pattern.replace('"', "");
        if p.contains(".sh") { return Some("sh".into()); }
        if let Some(idx) = p.rfind('.') {
            let ext = &p[idx + 1..].trim_end_matches('$');
            if ext.chars().all(|c| c.is_alphanumeric()) { return Some(ext.to_string()); }
        }
        None
    }

    fn compile_pattern(raw: &str) -> Vec<Pattern> {
        let mut r = raw.trim_matches('"').to_string();
        if r.is_empty() { return Vec::new(); }
        r = r.replace("\\b", "");
        r = r.trim_start_matches('^').trim_end_matches('$').to_string();
        if let Some(open) = r.find('(') {
            if let Some(close) = r.rfind(')') { if close > open { let inner = &r[open+1..close]; if inner.contains('|') { let alts: Vec<String> = inner.split('|').map(|s| s.to_string()).collect(); return vec![Pattern::KeywordSet(alts)]; } } }
        }
        if r.contains('|') { let alts: Vec<String> = r.split('|').map(|s| s.to_string()).collect(); return vec![Pattern::KeywordSet(alts)]; }
        if r.contains('*') || r.contains('?') { return vec![Pattern::Wildcard(r)]; }
        vec![Pattern::Keyword(r)]
    }

    fn matches_pattern(p: &Pattern, line: &str, idx: usize) -> Option<usize> {
        match p {
            Pattern::Keyword(w) => Self::match_word(line, idx, w),
            Pattern::KeywordSet(set) => {
                for w in set { if let Some(e) = Self::match_word(line, idx, w) { return Some(e); } }
                None
            }
            Pattern::Wildcard(raw) => {
                if line[idx..].starts_with(raw.trim_matches('*')) {
                    Some((idx + raw.len()).min(line.len()))
                } else { None }
            }
        }
    }

    fn is_word_boundary(ch: Option<char>) -> bool {
        match ch { None => true, Some(c) => !c.is_alphanumeric() && c != '_' }
    }

    fn match_word(line: &str, start: usize, w: &str) -> Option<usize> {
        if line[start..].starts_with(w) {
            let end = start + w.len();
            let prev = if start == 0 { None } else { line.chars().nth(start - 1) };
            let next = line.chars().nth(end);
            if Self::is_word_boundary(prev) && Self::is_word_boundary(next) { return Some(end); }
        }
        None
    }

    fn extension_matches(&self, filename: &str) -> bool {
        let ext_actual = std::path::Path::new(filename).extension().and_then(|e| e.to_str());
        let mut check_exts = Vec::new();
        if let Some(ea) = ext_actual { check_exts.push(ea.to_string()); }
        if let Ok(settings) = Settings::load() { if let Some(ea) = ext_actual { if let Some(mapped) = settings.syntax.extension_aliases.get(ea) { check_exts.push(mapped.clone()); } } }
        for e in check_exts { if self.exts.iter().any(|x| x == &e) { return true; } }
        false
    }
}

impl Highlighter for NanorcHighlighter {
    fn highlight_line(&self, line: &str, filename: &str, settings: &Settings) -> Vec<StyledSpan> {
        if !settings.syntax.enable { return Vec::new(); }
        if !self.extension_matches(filename) { return Vec::new(); }
        let mut spans = Vec::new();
        for rule in &self.rules {
            let mut i = 0;
            while i < line.len() {
                let mut matched = false;
                for pat in &rule.patterns {
                    if let Some(end) = Self::matches_pattern(pat, line, i) {
                        if end > i {
                            spans.push(StyledSpan { start: i, end, color_spec: ColorSpec { fg: Some(rule.color), bold: rule.bold, italic: false } });
                            i = end;
                            matched = true;
                            break;
                        }
                    }
                }
                if !matched { i += 1; }
            }
        }
        spans
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::set_temp_home;
    use crate::syntax::Highlighter;

    #[test]
    fn nanorc_load_and_highlight() {
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("syntax");
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("test.nanorc");
        std::fs::write(&file, "syntax test *.sh\ncolor brightgreen \"\\b(if|then|fi)\\b\"\n").unwrap();
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        let hl = NanorcHighlighter::new(&settings);
        let spans = hl.highlight_line("if then fi", "x.sh", &settings);
        assert!(!spans.is_empty());
    }
}
