use std::fs;
use std::path::{Path, PathBuf};
use crate::settings::Settings;
use crate::syntax::{Highlighter, StyledSpan, ColorSpec};

// Minimal NanoRC syntax parser: supports lines starting with "syntax" and "color".
// Example subset:
// syntax rust ".*\.rs" "Cargo.toml"
// color brightgreen "fn" "let" "pub" 
// color cyan "^[[:space:]]*#.*$" (regex-like patterns)
// Supports basic regex features: ^, $, .*, [[:space:]], character classes, word boundaries

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
    Regex { 
        anchor_start: bool,
        anchor_end: bool,
        parts: Vec<PatternPart>,
    },
}

#[derive(Debug, Clone)]
enum PatternPart {
    Literal(String),
    AnyChar,
    ZeroOrMore, // represents .*
    Whitespace, // single whitespace char
    CharClass(String), // simple class like A-Za-z_
    OneOrMore(Box<PatternPart>), // new: repetition +
    WordBoundaryStart,
    WordBoundaryEnd,
    ZeroOrMoreWhitespace, // new: matches zero or more consecutive whitespace
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
        let mut file_count = 0;
        const MAX_FILES: usize = 50; // Limit total nanorc files to prevent excessive loading
        
        for d in &dirs {
            if file_count >= MAX_FILES { break; }
            if d.exists() 
                && let Ok(rd) = fs::read_dir(&d) {
                for entry in rd.flatten() {
                    if file_count >= MAX_FILES { break; }
                    let p = entry.path();
                    if p.extension().and_then(|s| s.to_str()) == Some("nanorc") {
                        Self::parse_file(&p, &mut rules, &mut exts);
                        file_count += 1;
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
            dirs.push(PathBuf::from(&home).join(".ue").join("syntax"));
            for s in &settings.syntax.dirs {
                let expanded = if let Some(stripped) = s.strip_prefix("~/") { PathBuf::from(&home).join(stripped) } else { PathBuf::from(s) };
                dirs.push(expanded);
            }
        }
        if std::env::var("UE_DISABLE_SYSTEM_NANORC").ok().as_deref() != Some("1") {
            let system = PathBuf::from("/usr/share/nano");
            if system.exists() { dirs.push(system); }
        }
        let mut seen = std::collections::HashSet::new();
        dirs.retain(|p| {
            let canon = p.to_string_lossy().to_string();
            if seen.contains(&canon) { false } else { seen.insert(canon); true }
        });
        dirs
    }

    fn parse_file(path: &Path, rules: &mut Vec<NanoRule>, exts: &mut Vec<String>) {
        let content = match fs::read_to_string(path) { Ok(c) => c, Err(_) => return };
        let mut line_count = 0;
        const MAX_LINES: usize = 1000; // Safety limit per file
        const MAX_RULES_PER_FILE: usize = 200; // Limit rules from single file
        let rules_before = rules.len();
        
        for line in content.lines() {
            line_count += 1;
            if line_count > MAX_LINES || rules.len() - rules_before > MAX_RULES_PER_FILE {
                break; // Safety guard: prevent excessive processing
            }
            
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
                    if parts.len() >= 3 
                        && let Some((color, bold)) = Self::parse_color(&parts[1]) {
                        let mut pats = Vec::new();
                        for raw in parts.iter().skip(2) {
                            if raw.is_empty() { continue; }
                            pats.extend(Self::compile_pattern(raw));
                        }
                        rules.push(NanoRule { color, bold, patterns: pats });
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
        // retain \b for now – we interpret nano style word boundaries \< \>
        // Convert nano style word boundaries first
        r = r.replace("\\<", "__WB_START__").replace("\\>", "__WB_END__");
        // Remove standard \b (treat similarly)
        r = r.replace("\\b", "__WB_START__");
        let anchor_start = r.starts_with('^');
        let anchor_end = r.ends_with('$');
        if anchor_start { r = r[1..].to_string(); }
        if anchor_end { r = r[..r.len()-1].to_string(); }
        
        // Alternation simple
        if let Some(open) = r.find('(')
            && let Some(close) = r.rfind(')')
            && close > open {
            let inner = &r[open+1..close];
            if inner.contains('|') {
                let alts: Vec<String> = inner.split('|').map(|s| s.to_string()).collect();
                return vec![Pattern::KeywordSet(alts)];
            }
        }
        if r.contains('|') && !r.contains('[') {
            let alts: Vec<String> = r.split('|').map(|s| s.to_string()).collect();
            return vec![Pattern::KeywordSet(alts)];
        }
        
        let mut parts: Vec<PatternPart> = Vec::new();
        let chars: Vec<char> = r.chars().collect();
        let mut i = 0;
        let mut loop_count = 0;
        while i < chars.len() {
            loop_count += 1;
            if loop_count > 10000 {
                break; // Safety guard: prevent infinite loops
            }
            match chars[i] {
                '.' => {
                    if i + 1 < chars.len() && chars[i+1] == '*' {
                        parts.push(PatternPart::ZeroOrMore); i += 2;
                    } else if i + 1 < chars.len() && chars[i+1] == '+' { // treat .+ as any char repeated
                        parts.push(PatternPart::OneOrMore(Box::new(PatternPart::AnyChar))); i += 2;
                    } else { parts.push(PatternPart::AnyChar); i += 1; }
                }
                '[' => {
                    let mut end_pos = None; let mut j = i+1; while j < chars.len() { if chars[j] == '\\' && j+1 < chars.len() { j += 2; } else if chars[j] == ']' { end_pos = Some(j); break; } else { j += 1; } }
                    if let Some(end_idx) = end_pos {
                        let class_str: String = chars[i+1..end_idx].iter().collect();
                        let part = if class_str == ":space:" { PatternPart::Whitespace } else { PatternPart::CharClass(class_str) };
                        // Check quantifier following
                        if end_idx + 1 < chars.len() && chars[end_idx+1] == '+' { parts.push(PatternPart::OneOrMore(Box::new(part))); i = end_idx + 2; } else { parts.push(part); i = end_idx + 1; }
                    } else { parts.push(PatternPart::Literal("[".into())); i += 1; }
                }
                '_' => { // treat underscore as literal
                    parts.push(PatternPart::Literal("_".into())); i += 1;
                }
                '\\' => {
                    // escaped sequence
                    if i + 1 < chars.len() {
                        let nxt = chars[i+1];
                        match nxt {
                            't' => parts.push(PatternPart::Literal("\t".into())),
                            'n' => parts.push(PatternPart::Literal("\n".into())),
                            _ => parts.push(PatternPart::Literal(nxt.to_string())),
                        }
                        i += 2;
                    } else { parts.push(PatternPart::Literal("\\".into())); i += 1; }
                }
                '+' => { // stray + treat literal
                    parts.push(PatternPart::Literal("+".into())); i += 1;
                }
                ' ' => { // literal space
                    parts.push(PatternPart::Literal(" ".into())); i += 1;
                }
                _ => {
                    // accumulate literal run possibly followed by + quantifier (for simple word sequences)
                    let mut lit = String::new();
                    while i < chars.len() {
                        let c = chars[i];
                        if c == '.' || c == '[' || c == '\\' || c == '*' || c == '+' { break; }
                        lit.push(c); i += 1;
                    }
                    if !lit.is_empty() {
                        if i < chars.len() && chars[i] == '+' { parts.push(PatternPart::OneOrMore(Box::new(PatternPart::Literal(lit)))); i += 1; } else { parts.push(PatternPart::Literal(lit)); }
                    }
                }
            }
        }
        // After initial parts construction, merge whitespace + '*' into ZeroOrMoreWhitespace
        let mut merged: Vec<PatternPart> = Vec::new();
        let mut idx_merge = 0;
        while idx_merge < parts.len() {
            if idx_merge + 1 < parts.len() {
                match (&parts[idx_merge], &parts[idx_merge+1]) {
                    (PatternPart::Whitespace, PatternPart::Literal(s)) if s == "*" => {
                        merged.push(PatternPart::ZeroOrMoreWhitespace);
                        idx_merge += 2;
                        continue;
                    }
                    (PatternPart::CharClass(class), PatternPart::Literal(s)) if s == "*" && class == ":space:" => {
                        merged.push(PatternPart::ZeroOrMoreWhitespace);
                        idx_merge += 2;
                        continue;
                    }
                    _ => {}
                }
            }
            merged.push(parts[idx_merge].clone());
            idx_merge += 1;
        }
        parts = merged;
        // Replace placeholder word boundaries
        let mut with_boundaries: Vec<PatternPart> = Vec::new();
        for p in parts { match &p { PatternPart::Literal(s) if s == "__WB_START__" => with_boundaries.push(PatternPart::WordBoundaryStart), PatternPart::Literal(s) if s == "__WB_END__" => with_boundaries.push(PatternPart::WordBoundaryEnd), _ => with_boundaries.push(p) } }
        let final_parts = with_boundaries;
        if !anchor_start && !anchor_end && final_parts.len() == 1 {
            if let Some(PatternPart::Literal(s)) = final_parts.first() { return vec![Pattern::Keyword(s.clone())]; }
        }
        vec![Pattern::Regex { anchor_start, anchor_end, parts: final_parts }]
    }

    fn matches_pattern(p: &Pattern, line: &str, idx: usize) -> Option<usize> {
        match p {
            Pattern::Keyword(w) => Self::match_word(line, idx, w),
            Pattern::KeywordSet(set) => {
                for w in set { 
                    if let Some(e) = Self::match_word(line, idx, w) { 
                        return Some(e); 
                    } 
                }
                None
            }
            Pattern::Regex { anchor_start, anchor_end, parts } => {
                // Check start anchor
                if *anchor_start && idx != 0 {
                    return None;
                }
                
                Self::match_regex_parts(line, idx, parts, *anchor_end)
            }
        }
    }
    
    fn match_regex_parts(line: &str, start_idx: usize, parts: &[PatternPart], anchor_end: bool) -> Option<usize> {
        let mut idx = start_idx;
        let bytes = line.as_bytes();
        for part in parts {
            match part {
                PatternPart::Literal(s) => { if !line[idx..].starts_with(s) { return None; } idx += s.len(); }
                PatternPart::AnyChar => { if idx >= line.len() { return None; } if let Some(c) = line[idx..].chars().next() { idx += c.len_utf8(); } }
                PatternPart::ZeroOrMore => { idx = line.len(); }
                PatternPart::Whitespace => { if idx >= line.len() || !bytes[idx].is_ascii_whitespace() { return None; } idx += 1; }
                PatternPart::CharClass(class) => { if idx >= line.len() { return None; } let ch = bytes[idx] as char; if !Self::char_matches_class(ch, class) { return None; } idx += 1; }
                PatternPart::OneOrMore(inner) => {
                    // require at least one match of inner then greedily continue
                    let mut local = idx; let mut count = 0;
                    loop {
                        let before = local;
                        let ok = match &**inner {
                            PatternPart::Literal(s) => line[local..].starts_with(s) && { local += s.len(); true },
                            PatternPart::AnyChar => if local < line.len() { if let Some(c)=line[local..].chars().next(){ local += c.len_utf8(); true } else { false } } else { false },
                            PatternPart::Whitespace => if local < line.len() && bytes[local].is_ascii_whitespace() { local += 1; true } else { false },
                            PatternPart::CharClass(class) => if local < line.len() { let ch = bytes[local] as char; if Self::char_matches_class(ch, class) { local += 1; true } else { false } } else { false },
                            _ => false,
                        };
                        if ok { count += 1; } else { break; }
                        if before == local { break; }
                    }
                    if count == 0 { return None; }
                    idx = local;
                }
                PatternPart::WordBoundaryStart => {
                    // ensure previous is boundary
                    if start_idx != idx { // not at pattern start implies we need boundary at current position
                        let prev = if idx == 0 { None } else { line[idx-1..].chars().next() };
                        if let Some(pc) = prev { if pc.is_alphanumeric() || pc == '_' { return None; } }
                    }
                }
                PatternPart::WordBoundaryEnd => {
                    let next = if idx >= line.len() { None } else { line[idx..].chars().next() };
                    if let Some(nc) = next { if nc.is_alphanumeric() || nc == '_' { return None; } }
                }
                PatternPart::ZeroOrMoreWhitespace => {
                    // consume all consecutive whitespace
                    while idx < line.len() && line.as_bytes()[idx].is_ascii_whitespace() { idx += 1; }
                }
            }
        }
        if anchor_end && idx != line.len() { return None; }
        if idx > start_idx { Some(idx) } else { None }
    }
    
    fn char_matches_class(ch: char, class: &str) -> bool {
        // Simple character class matching
        match class {
            "A-Za-z_" => ch.is_ascii_alphabetic() || ch == '_',
            "A-Za-z0-9_" => ch.is_ascii_alphanumeric() || ch == '_',
            "0-9" => ch.is_ascii_digit(),
            "A-Z_" => ch.is_ascii_uppercase() || ch == '_',
            "A-Z0-9_" => ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_',
            _ => false,
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
        if let Ok(settings) = Settings::load() 
            && let Some(ea) = ext_actual 
            && let Some(mapped) = settings.syntax.extension_aliases.get(ea) { 
            check_exts.push(mapped.clone()); 
        }
        for e in check_exts { if self.exts.iter().any(|x| x == &e) { return true; } }
        false
    }
}

impl Highlighter for NanorcHighlighter {
    fn highlight_line(&self, line: &str, filename: &str, settings: &Settings) -> Vec<StyledSpan> {
        if !settings.syntax.enable { return Vec::new(); }
        if !self.extension_matches(filename) { return Vec::new(); }
        
        let mut spans = Vec::new();
        let mut covered = vec![false; line.len()];
        for rule in &self.rules {
            let mut i = 0;
            let mut safety = 0usize;
            let safety_limit = line.len().saturating_mul(10).max(500); // generous cap
            while i < line.len() {
                if safety > safety_limit { break; }
                safety += 1;
                if covered[i] { i += 1; continue; }
                let mut matched = false;
                for pat in &rule.patterns {
                    if let Some(end) = Self::matches_pattern(pat, line, i) && end > i {
                        spans.push(StyledSpan { start: i, end, color_spec: ColorSpec { fg: Some(rule.color), bold: rule.bold, italic: false } });
                        covered[i..end.min(line.len())].fill(true);
                        i = end;
                        matched = true;
                        break;
                    }
                }
                if !matched { i += 1; }
            }
        }
        // Merge contiguous spans with same color
        if spans.len() > 1 {
            spans.sort_by_key(|s| s.start);
            let mut merged: Vec<StyledSpan> = Vec::with_capacity(spans.len());
            for s in spans.into_iter() {
                if let Some(last) = merged.last_mut() {
                    if last.end == s.start && last.color_spec.fg == s.color_spec.fg && last.color_spec.bold == s.color_spec.bold && last.color_spec.italic == s.color_spec.italic {
                        last.end = s.end;
                        continue;
                    }
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

    // HIGH PRIORITY TESTS

    #[test]
    fn test_comment_highlighting_entire_line() {
        // This was the original user complaint - comments only highlighting the # character
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("syntax");
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("test.nanorc");
        std::fs::write(&file, "syntax test *.txt\ncolor cyan \"^[[:space:]]*#.*$\"\n").unwrap();
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        settings.syntax.dirs.clear(); // Only use test directory, not system dirs
        let hl = NanorcHighlighter::new(&settings);
        
        // Comment should highlight from # to end of line, not just the # character
        let spans = hl.highlight_line("  # this is a comment", "test.txt", &settings);
        assert_eq!(spans.len(), 1, "Should have exactly one span for the comment");
        assert_eq!(spans[0].start, 2, "Comment should start after whitespace at the #");
        assert_eq!(spans[0].end, 22, "Comment should extend to end of line");
    }

    #[test]
    fn test_no_overlapping_highlights() {
        // Critical: First matching rule should take precedence
        // Keywords inside comments should NOT be highlighted separately
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("syntax");
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("test.nanorc");
        // First rule for comments, second for keywords
        std::fs::write(&file, 
            "syntax test *.txt\n\
             color cyan \"^#.*$\"\n\
             color green \"\\b(keyword)\\b\"\n").unwrap();
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        let hl = NanorcHighlighter::new(&settings);
        
        // "keyword" appears inside the comment, but should NOT be highlighted separately
        let spans = hl.highlight_line("# this has keyword in it", "test.txt", &settings);
        assert_eq!(spans.len(), 1, "Comment should be one span, not split by keyword");
        assert_eq!(spans[0].start, 0, "Comment starts at beginning");
        assert_eq!(spans[0].end, 25, "Comment extends to end");
    }

    #[test]
    fn test_anchored_patterns() {
        // Test that ^ and $ anchors work correctly
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("syntax");
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("test.nanorc");
        std::fs::write(&file, "syntax test *.txt\ncolor brightmagenta \"^\\[.*\\]$\"\n").unwrap();
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        let hl = NanorcHighlighter::new(&settings);
        
        // Should match entire line when pattern is anchored at start and end
        let spans = hl.highlight_line("[section]", "test.txt", &settings);
        assert_eq!(spans.len(), 1, "Should match anchored pattern");
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, 9);
        
        // Should NOT match if not at start
        let spans = hl.highlight_line("text [section]", "test.txt", &settings);
        assert_eq!(spans.len(), 0, "Should not match when not at line start");
        
        // Should NOT match if not at end
        let spans = hl.highlight_line("[section] text", "test.txt", &settings);
        assert_eq!(spans.len(), 0, "Should not match when not at line end");
    }

    #[test]
    fn test_greedy_dot_star_matching() {
        // Verify .* matches to end of line (greedy behavior)
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("syntax");
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("test.nanorc");
        std::fs::write(&file, "syntax test *.txt\ncolor cyan \"#.*$\"\n").unwrap();
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        let hl = NanorcHighlighter::new(&settings);
        
        // .* should match everything from # to end of line
        let spans = hl.highlight_line("text # comment with more stuff", "test.txt", &settings);
        assert_eq!(spans.len(), 1, "Should have one span for comment");
        assert_eq!(spans[0].start, 5, "Should start at #");
        assert_eq!(spans[0].end, 31, "Should match to end of line (greedy)");
    }

    // MEDIUM PRIORITY TESTS

    #[test]
    fn test_character_class_matching() {
        // Test both simple character classes and POSIX classes
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("syntax");
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("test.nanorc");
        std::fs::write(&file, "syntax test *.txt\ncolor brightgreen \"^[[:space:]]*[A-Za-z_][A-Za-z0-9_]*[[:space:]]*=\"\n").unwrap();
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        let hl = NanorcHighlighter::new(&settings);
        
        // Should match key assignment pattern
        let spans = hl.highlight_line("  my_key = value", "test.txt", &settings);
        assert_eq!(spans.len(), 1, "Should match key assignment");
        assert_eq!(spans[0].start, 0, "Should start at beginning");
        assert!(spans[0].end >= 9, "Should include key name and equals sign");
        
        // Test with different identifier
        let spans = hl.highlight_line("tab_width = 4", "test.txt", &settings);
        assert_eq!(spans.len(), 1, "Should match different key");
    }

    #[test]
    fn test_keyword_alternation() {
        // Test pipe-separated alternatives (true|false)
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("syntax");
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("test.nanorc");
        std::fs::write(&file, "syntax test *.txt\ncolor green \"\\b(true|false)\\b\"\n").unwrap();
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        let hl = NanorcHighlighter::new(&settings);
        
        // Should match "true"
        let spans = hl.highlight_line("value is true", "test.txt", &settings);
        assert_eq!(spans.len(), 1, "Should match 'true'");
        assert_eq!(spans[0].start, 9);
        assert_eq!(spans[0].end, 13);
        
        // Should match "false"
        let spans = hl.highlight_line("value is false", "test.txt", &settings);
        assert_eq!(spans.len(), 1, "Should match 'false'");
        assert_eq!(spans[0].start, 9);
        assert_eq!(spans[0].end, 14);
        
        // Should not match partial words
        let spans = hl.highlight_line("falsehood", "test.txt", &settings);
        assert_eq!(spans.len(), 0, "Should not match partial word");
    }

    #[test]
    fn test_multiple_patterns_per_rule() {
        // Test that multiple patterns in one color rule all work
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("syntax");
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("test.nanorc");
        std::fs::write(&file, "syntax test *.txt\ncolor green \"\\bfn\\b\" \"\\blet\\b\" \"\\bpub\\b\"\n").unwrap();
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        let hl = NanorcHighlighter::new(&settings);
        
        let spans = hl.highlight_line("pub fn test let x", "test.txt", &settings);
        assert_eq!(spans.len(), 3, "Should have three separate highlights");
        
        // Verify each keyword is highlighted
        assert_eq!(spans[0].start, 0);  // pub
        assert_eq!(spans[0].end, 3);
        assert_eq!(spans[1].start, 4);  // fn
        assert_eq!(spans[1].end, 6);
        assert_eq!(spans[2].start, 12); // let
        assert_eq!(spans[2].end, 15);
    }

    #[test]
    fn test_whitespace_character_class() {
        // Test [[:space:]] POSIX character class
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("syntax");
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("test.nanorc");
        std::fs::write(&file, "syntax test *.txt\ncolor cyan \"^[[:space:]]+\"\n").unwrap();
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        let hl = NanorcHighlighter::new(&settings);
        
        // Should match leading whitespace
        let spans = hl.highlight_line("   text", "test.txt", &settings);
        assert_eq!(spans.len(), 1, "Should match leading whitespace");
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, 3);
        
        // Should not match whitespace not at start (due to ^ anchor)
        let spans = hl.highlight_line("text   more", "test.txt", &settings);
        assert_eq!(spans.len(), 0, "Should not match when not at line start");
    }

    #[test]
    fn test_string_highlighting() {
        // Test string pattern with escaped quotes in character class
        // This pattern was causing infinite loop before the fix
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("syntax");
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("test.nanorc");
        std::fs::write(&file, "syntax test *.txt\ncolor yellow \"\\\"[^\\\"]*\\\"\"\n").unwrap();
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        let hl = NanorcHighlighter::new(&settings);
        
        let spans = hl.highlight_line("text \"hello world\" more", "test.txt", &settings);
        assert_eq!(spans.len(), 1, "Should have one span for the string");
        assert_eq!(spans[0].start, 5, "String should start at opening quote");
        assert_eq!(spans[0].end, 18, "String should end after closing quote");
    }

    #[test]
    fn test_extension_matching() {
        // Verify file extension filtering works
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("syntax");
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("test.nanorc");
        std::fs::write(&file, "syntax test *.sh *.bash\ncolor green \"keyword\"\n").unwrap();
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        let hl = NanorcHighlighter::new(&settings);
        
        // Should match .sh files
        let spans = hl.highlight_line("keyword here", "test.sh", &settings);
        assert_eq!(spans.len(), 1, "Should match .sh extension");
        
        // Should match .bash files
        let spans = hl.highlight_line("keyword here", "test.bash", &settings);
        assert_eq!(spans.len(), 1, "Should match .bash extension");
        
        // Should NOT match .txt files
        let spans = hl.highlight_line("keyword here", "test.txt", &settings);
        assert_eq!(spans.len(), 0, "Should not match .txt extension");
    }

    #[test]
    fn test_syntax_disabled() {
        // Ensure highlighting respects enable/disable setting
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("syntax");
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("test.nanorc");
        std::fs::write(&file, "syntax test *.txt\ncolor green \"keyword\"\n").unwrap();
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = false; // Disabled
        let hl = NanorcHighlighter::new(&settings);
        
        let spans = hl.highlight_line("keyword here", "test.txt", &settings);
        assert_eq!(spans.len(), 0, "Should not highlight when syntax is disabled");
    }

    #[test]
    fn nanorc_no_hang_on_large_keyword_block() {
        // Test that safety guard prevents infinite loops
        // Create a simple highlighter directly with problematic rules
        let rules = vec![
            NanoRule {
                color: crossterm::style::Color::Cyan,
                bold: false,
                patterns: vec![Pattern::Regex {
                    anchor_start: true,
                    anchor_end: true,
                    parts: vec![
                        PatternPart::ZeroOrMoreWhitespace,
                        PatternPart::Literal("#".to_string()),
                        PatternPart::ZeroOrMore,
                    ],
                }],
            },
        ];
        
        let hl = NanorcHighlighter { rules, exts: vec!["sh".to_string()] };
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        
        // Test with comment line - should complete quickly without hanging
        let line = "  # while if function case done";
        let spans = hl.highlight_line(line, "x.sh", &settings);
        assert_eq!(spans.len(), 1, "Comment should be one merged span");
        assert_eq!(spans[0].start, 0, "Anchored pattern matches from line start");
        assert_eq!(spans[0].end, line.len(), "Should extend to end of line");
    }

    // HANG PREVENTION TESTS - Critical regression tests for the hang issue

    #[test]
    fn test_file_count_limit_prevents_excessive_loading() {
        // Test that MAX_FILES limit prevents loading too many nanorc files
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("syntax");
        std::fs::create_dir_all(&base).unwrap();
        
        // Create 100 nanorc files (more than MAX_FILES = 50)
        for i in 0..100 {
            let file = base.join(format!("test{}.nanorc", i));
            std::fs::write(&file, format!("syntax test{} *.txt\ncolor green \"keyword{}\"\n", i, i)).unwrap();
        }
        
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        settings.syntax.dirs.clear();
        
        // Should complete quickly without hanging - only loads first 50 files
        let hl = NanorcHighlighter::new(&settings);
        
        // Verify it loaded files but was limited
        assert!(hl.rules.len() > 0, "Should have loaded some rules");
        assert!(hl.rules.len() <= 50, "Should not load more than MAX_FILES");
    }

    #[test]
    fn test_line_count_limit_per_file() {
        // Test that MAX_LINES limit prevents processing huge files
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("syntax");
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("huge.nanorc");
        
        // Create a file with 2000 lines (more than MAX_LINES = 1000)
        let mut content = String::from("syntax test *.txt\n");
        for i in 0..2000 {
            content.push_str(&format!("color green \"keyword{}\"\n", i));
        }
        std::fs::write(&file, content).unwrap();
        
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        settings.syntax.dirs.clear();
        
        // Should complete quickly without processing all 2000 lines
        let start = std::time::Instant::now();
        let hl = NanorcHighlighter::new(&settings);
        let duration = start.elapsed();
        
        // Should finish very quickly (< 1 second even with safety limits)
        assert!(duration.as_secs() < 1, "Should complete quickly with line limit");
        
        // Should have some rules but not 2000
        assert!(hl.rules.len() > 0, "Should have loaded some rules");
        assert!(hl.rules.len() < 2000, "Should not have processed all 2000 lines");
    }

    #[test]
    fn test_rule_count_limit_per_file() {
        // Test that MAX_RULES_PER_FILE limit prevents excessive rules from one file
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("syntax");
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("many_rules.nanorc");
        
        // Create a file that would generate 300 rules (more than MAX_RULES_PER_FILE = 200)
        let mut content = String::from("syntax test *.txt\n");
        for i in 0..300 {
            content.push_str(&format!("color green \"keyword{}\"\n", i));
        }
        std::fs::write(&file, content).unwrap();
        
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        settings.syntax.dirs.clear();
        
        let hl = NanorcHighlighter::new(&settings);
        
        // Should have limited rules to MAX_RULES_PER_FILE
        assert!(hl.rules.len() > 0, "Should have loaded some rules");
        assert!(hl.rules.len() <= 201, "Should not exceed MAX_RULES_PER_FILE + 1 (syntax line)");
    }

    #[test]
    fn test_pattern_compilation_loop_guard() {
        // Test that the 10,000 iteration limit prevents infinite loops in pattern compilation
        // This tests the safety guard added to compile_pattern()
        
        // Create a potentially problematic pattern that could cause issues
        let patterns = vec![
            "^[[:space:]]*#.*$",           // Complex pattern with character class
            "[[:space:]]*[A-Za-z_][A-Za-z0-9_]*[[:space:]]*=",  // Multiple char classes
            "\\<(keyword1|keyword2|keyword3|keyword4|keyword5)\\>", // Alternation
        ];
        
        for pattern in patterns {
            let start = std::time::Instant::now();
            let result = NanorcHighlighter::compile_pattern(pattern);
            let duration = start.elapsed();
            
            // Should complete very quickly (< 100ms)
            assert!(duration.as_millis() < 100, 
                "Pattern compilation should be fast with loop guard: {}", pattern);
            assert!(!result.is_empty(), "Should produce valid pattern");
        }
    }

    #[test]
    fn test_highlight_loop_safety_guard() {
        // Test that the safety guard in highlight_line prevents infinite loops
        let rules = vec![
            NanoRule {
                color: crossterm::style::Color::Red,
                bold: false,
                // Create a rule that might cause issues without safety guard
                patterns: vec![Pattern::Regex {
                    anchor_start: false,
                    anchor_end: false,
                    parts: vec![
                        PatternPart::Literal("a".to_string()),
                        PatternPart::ZeroOrMore,
                    ],
                }],
            },
        ];
        
        let hl = NanorcHighlighter { rules, exts: vec!["txt".to_string()] };
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        
        // Test with various line lengths
        for len in [10, 100, 1000, 5000] {
            let line = "a".repeat(len);
            let start = std::time::Instant::now();
            let spans = hl.highlight_line(&line, "test.txt", &settings);
            let duration = start.elapsed();
            
            // Should complete quickly even with long lines
            assert!(duration.as_millis() < 500, 
                "Should complete quickly with safety guard for line length {}", len);
            assert!(!spans.is_empty(), "Should produce some highlights");
        }
    }

    #[test]
    fn test_real_world_sh_nanorc_pattern() {
        // Test with the actual problematic pattern from sh.nanorc that caused the hang
        // Main goal: verify it doesn't hang during initialization or highlighting
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("syntax");
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("sh.nanorc");
        
        // This is the actual problematic content from system sh.nanorc
        std::fs::write(&file, r#"syntax sh "\.sh$" "\.bash$"
color cyan "^[[:space:]]*#.*$"
color green "\<(if|then|else|elif|fi|for|while|do|done|case|esac|function)\>"
color brightblue "\<(echo|exit|return|break|continue)\>"
"#).unwrap();
        
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        settings.syntax.dirs.clear();
        
        // Key test: initialization should complete quickly without hanging
        let start = std::time::Instant::now();
        let hl = NanorcHighlighter::new(&settings);
        let init_duration = start.elapsed();
        
        assert!(init_duration.as_secs() < 1, "Initialization should be quick");
        assert!(hl.rules.len() > 0, "Should have loaded rules");
        
        // Key test: highlighting should complete quickly without hanging
        let line = "if [ -f file ]; then echo 'test'; fi";
        let start = std::time::Instant::now();
        let _spans = hl.highlight_line(line, "test.sh", &settings);
        let highlight_duration = start.elapsed();
        
        assert!(highlight_duration.as_millis() < 100, "Highlighting should be instant");
    }

    #[test]
    fn test_system_nanorc_loading_timeout() {
        // Test that loading from /usr/share/nano doesn't hang
        // This is the most realistic test of the original hang issue
        
        // Skip this test if system nanorc is explicitly disabled
        if std::env::var("UE_DISABLE_SYSTEM_NANORC").is_ok() {
            return; // Test not applicable when system files disabled
        }
        
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        // Don't clear dirs - let it try to load system files
        
        let start = std::time::Instant::now();
        let _hl = NanorcHighlighter::new(&settings);
        let duration = start.elapsed();
        
        // Should complete within reasonable time even with system files
        assert!(duration.as_secs() < 5, 
            "Should load system nanorc files within 5 seconds (took {:?})", duration);
        
        // May or may not have loaded files depending on system
        // Just verify it didn't hang
    }

    #[test]
    fn test_no_hang_with_empty_pattern() {
        // Edge case: empty patterns should not cause issues
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("syntax");
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("empty.nanorc");
        
        std::fs::write(&file, r#"syntax test *.txt
color green ""
color red "" ""
color blue "valid"
"#).unwrap();
        
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        settings.syntax.dirs.clear();
        
        // Should handle empty patterns gracefully without hanging
        let hl = NanorcHighlighter::new(&settings);
        
        // Empty patterns are filtered out, only "valid" pattern creates a rule
        assert!(hl.rules.len() > 0, "Should have non-empty patterns");
        
        let spans = hl.highlight_line("valid test", "test.txt", &settings);
        assert_eq!(spans.len(), 1, "Should highlight 'valid' keyword");
    }

    #[test]
    fn test_no_hang_with_complex_nested_patterns() {
        // Test with complex nested patterns that might cause exponential backtracking
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("syntax");
        std::fs::create_dir_all(&base).unwrap();
        let file = base.join("complex.nanorc");
        
        std::fs::write(&file, r#"syntax test *.txt
color green "^[[:space:]]*[A-Za-z_][A-Za-z0-9_]*[[:space:]]*=.*$"
color cyan "^[[:space:]]*#.*$"
color yellow "\<(true|false|null|undefined|NaN)\>"
"#).unwrap();
        
        let mut settings = Settings::load().unwrap();
        settings.syntax.enable = true;
        settings.syntax.dirs.clear();
        
        let start = std::time::Instant::now();
        let hl = NanorcHighlighter::new(&settings);
        let init_duration = start.elapsed();
        
        assert!(init_duration.as_millis() < 500, "Complex patterns should compile quickly");
        
        // Test with a long line
        let line = "  some_variable = true # comment with more text and keywords like false null";
        let start = std::time::Instant::now();
        let spans = hl.highlight_line(line, "test.txt", &settings);
        let highlight_duration = start.elapsed();
        
        assert!(highlight_duration.as_millis() < 100, "Highlighting should be fast");
        assert!(spans.len() > 0, "Should produce highlights");
    }
}
