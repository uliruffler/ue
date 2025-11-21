use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use regex::Regex;
use once_cell::sync::Lazy;
use std::sync::Mutex;

/// Cached syntax definitions by file extension
static SYNTAX_CACHE: Lazy<Mutex<HashMap<String, SyntaxDefinition>>> = 
    Lazy::new(|| Mutex::new(HashMap::new()));

/// A syntax highlighting definition loaded from a Vim syntax file.
/// 
/// Contains keyword groups, regex-based pattern matches, and color specifications
/// for different syntax groups.
#[derive(Debug, Clone)]
pub(crate) struct SyntaxDefinition {
    pub(crate) keywords: HashMap<String, Vec<String>>,
    pub(crate) matches: Vec<SyntaxMatch>,
    pub(crate) colors: HashMap<String, ColorSpec>,
    pub(crate) links: HashMap<String, String>, // group -> linked_group mappings
}

/// A regex-based syntax match pattern.
#[derive(Debug, Clone)]
pub(crate) struct SyntaxMatch {
    pub(crate) group: String,
    pub(crate) pattern: Regex,
}

/// Color and style specification for syntax highlighting.
/// 
/// Supports foreground/background colors using ANSI codes (0-255),
/// plus bold and italic text attributes.
#[derive(Debug, Clone)]
pub(crate) struct ColorSpec {
    pub(crate) fg: Option<u8>,
    pub(crate) bg: Option<u8>,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
}

impl ColorSpec {
    /// Convert ANSI color code to crossterm Color
    pub(crate) fn ansi_to_color(code: u8) -> crossterm::style::Color {
        use crossterm::style::Color;
        match code {
            0 => Color::Black,
            1 => Color::DarkRed,
            2 => Color::DarkGreen,
            3 => Color::DarkYellow,
            4 => Color::DarkBlue,
            5 => Color::DarkMagenta,
            6 => Color::DarkCyan,
            7 => Color::Grey,
            8 => Color::DarkGrey,
            9 => Color::Red,
            10 => Color::Green,
            11 => Color::Yellow,
            12 => Color::Blue,
            13 => Color::Magenta,
            14 => Color::Cyan,
            15 => Color::White,
            _ => Color::AnsiValue(code),
        }
    }
    
    /// Apply this color specification to stdout
    pub(crate) fn apply_to_stdout(
        &self,
        stdout: &mut impl std::io::Write,
    ) -> Result<(), std::io::Error> {
        use crossterm::execute;
        use crossterm::style::{SetForegroundColor, SetBackgroundColor, SetAttribute, Attribute};
        
        if let Some(fg) = self.fg {
            execute!(stdout, SetForegroundColor(Self::ansi_to_color(fg)))?;
        }
        if let Some(bg) = self.bg {
            execute!(stdout, SetBackgroundColor(Self::ansi_to_color(bg)))?;
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


impl SyntaxDefinition {
    fn new() -> Self {
        Self {
            keywords: HashMap::new(),
            matches: Vec::new(),
            colors: HashMap::new(),
            links: HashMap::new(),
        }
    }
}

/// Get syntax definition for a file extension (e.g., "rs", "py").
/// 
/// Loads from `~/.ue/syntax/{ext}.vim` if available. Results are cached
/// for performance.
/// 
/// # Arguments
/// * `ext` - File extension without the dot (e.g., "rs", "py", "js")
/// 
/// # Returns
/// * `Some(SyntaxDefinition)` if a syntax file exists and can be parsed
/// * `None` if no syntax file exists or parsing fails
pub(crate) fn get_syntax_for_extension(ext: &str) -> Option<SyntaxDefinition> {
    // Check cache first
    {
        let cache = SYNTAX_CACHE.lock().ok()?;
        if let Some(def) = cache.get(ext) {
            return Some(def.clone());
        }
    }
    
    // Load from file
    let syntax_path = get_syntax_path(ext).ok()?;
    if !syntax_path.exists() {
        return None;
    }
    
    let content = fs::read_to_string(&syntax_path).ok()?;
    let def = parse_vim_syntax(&content)?;
    
    // Cache it
    if let Ok(mut cache) = SYNTAX_CACHE.lock() {
        cache.insert(ext.to_string(), def.clone());
    }
    
    Some(def)
}

/// Get path to syntax file for extension
fn get_syntax_path(ext: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = std::env::var("UE_TEST_HOME")
        .or_else(|_| std::env::var("HOME"))
        .or_else(|_| std::env::var("USERPROFILE"))?;
    
    Ok(PathBuf::from(home)
        .join(".ue")
        .join("syntax")
        .join(format!("{}.vim", ext)))
}

/// Parse a simplified subset of Vim syntax file
fn parse_vim_syntax(content: &str) -> Option<SyntaxDefinition> {
    let mut def = SyntaxDefinition::new();
    
    for line in content.lines() {
        let trimmed = line.trim();
        
        // Skip comments and empty lines
        if trimmed.starts_with('"') || trimmed.is_empty() {
            continue;
        }
        
        // Parse "syn keyword {group} {word1} {word2} ..."
        // Note: Vim syntax files may use tabs or spaces as separators
        if trimmed.starts_with("syn keyword") || trimmed.starts_with("syntax keyword") {
            parse_keyword_line(trimmed, &mut def);
        }
        // Parse "syn match {group} /{pattern}/"
        else if trimmed.starts_with("syn match") || trimmed.starts_with("syntax match") {
            parse_match_line(trimmed, &mut def);
        }
        // Parse "hi def link {group} {target}" or "hi link {group} {target}"
        else if trimmed.starts_with("hi def link") || trimmed.starts_with("highlight def link")
                || trimmed.starts_with("hi link") || trimmed.starts_with("highlight link") {
            parse_link_line(trimmed, &mut def);
        }
        // Parse "hi {group} ctermfg={color}"
        else if trimmed.starts_with("hi ") || trimmed.starts_with("highlight ") {
            parse_highlight_line(trimmed, &mut def);
        }
    }
    
    // Resolve links to actual colors
    resolve_links(&mut def);
    
    Some(def)
}

fn parse_keyword_line(line: &str, def: &mut SyntaxDefinition) {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 3 {
        return;
    }
    
    let group = parts[2].to_string();
    
    // Vim syntax options that should not be treated as keywords
    let vim_options = [
        "contained", "nextgroup=", "skipwhite", "skipempty", "skipnl",
        "display", "conceal", "concealends", "transparent", "oneline",
        "fold", "extend", "excludenl", "keepend"
    ];
    
    let keywords: Vec<String> = parts[3..]
        .iter()
        .take_while(|s| {
            // Stop at inline comments
            if s.starts_with('"') {
                return false;
            }
            // Stop at Vim syntax options
            for opt in &vim_options {
                if s.starts_with(opt) {
                    return false;
                }
            }
            true
        })
        .map(|s| s.to_string())
        .collect();
    
    if !keywords.is_empty() {
        def.keywords.entry(group)
            .or_insert_with(Vec::new)
            .extend(keywords);
    }
}

fn parse_match_line(line: &str, def: &mut SyntaxDefinition) {
    // Skip patterns marked as 'contained' - they require Vim syntax region context
    if line.contains("contained") {
        return;
    }
    
    // Extract group and pattern from: syn match {group} /{pattern}/
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 {
        return;
    }
    
    let group = parts[2].to_string();
    
    // Find the pattern part - everything after the group name
    // Find where the group ends and extract the rest
    if let Some(group_pos) = line.find(&group) {
        let after_group = &line[group_pos + group.len()..].trim_start();
        if let Some(pattern_str) = extract_pattern(after_group) {
            // Skip empty patterns (they match everywhere)
            if pattern_str.is_empty() {
                return;
            }
            
            // Skip complex Vim patterns that we can't easily convert
            // These include: lookbehind/lookahead, Vim-specific atoms, etc.
            if is_complex_vim_pattern(&pattern_str) {
                return;
            }
            
            // Convert Vim regex to Rust regex
            let rust_pattern = convert_vim_regex_to_rust(&pattern_str);
            if let Ok(pattern) = Regex::new(&rust_pattern) {
                def.matches.push(SyntaxMatch {
                    group,
                    pattern,
                });
            }
        }
    }
}

/// Check if a Vim pattern is too complex to convert reliably
fn is_complex_vim_pattern(pattern: &str) -> bool {
    // Skip patterns with Vim-specific features we can't easily convert
    let complex_features = [
        r"\@",      // Vim lookahead/lookbehind operators
        r"\%",      // Vim specific atoms
        r"\_",      // Vim newline-matching variants
        r"\ze",     // Vim zero-width match end
        r"\zs",     // Vim zero-width match start
        r"\h",      // Vim head of word character
        r"\a",      // Vim alphabetic character
        r"\l",      // Vim lowercase character
        r"\u",      // Vim uppercase character
    ];
    
    for feature in &complex_features {
        if pattern.contains(feature) {
            return true;
        }
    }
    false
}

/// Convert Vim regex patterns to Rust regex
fn convert_vim_regex_to_rust(vim_pattern: &str) -> String {
    vim_pattern
        .replace(r"\+", "+")      // \+ -> + (one or more)
        .replace(r"\?", "?")      // \? -> ? (zero or one)
        .replace(r"\{", "{")      // \{ -> { (literal brace)
        .replace(r"\}", "}")      // \} -> } (literal brace)
        .replace(r"\|", "|")      // \| -> | (alternation)
        .replace(r"\(", "(")      // \( -> ( (grouping)
        .replace(r"\)", ")")      // \) -> ) (grouping)
}

fn extract_pattern(text: &str) -> Option<String> {
    // Handle patterns like /pattern/ or 'pattern' or "pattern"
    // Need to handle escaped delimiters like \/
    let delimiters = ['/', '\'', '"'];
    
    for delim in delimiters {
        if text.starts_with(delim) {
            // Find the closing delimiter, accounting for escaped ones
            let mut i = 1;
            let chars: Vec<char> = text.chars().collect();
            while i < chars.len() {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    // Skip escaped character
                    i += 2;
                } else if chars[i] == delim {
                    // Found closing delimiter
                    return Some(chars[1..i].iter().collect());
                } else {
                    i += 1;
                }
            }
        }
    }
    
    None
}

fn parse_highlight_line(line: &str, def: &mut SyntaxDefinition) {
    // Parse: hi {group} ctermfg={color} ctermbg={color} cterm=bold,italic
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }
    
    let group = parts[1].to_string();
    let mut color_spec = ColorSpec {
        fg: None,
        bg: None,
        bold: false,
        italic: false,
    };
    
    for part in &parts[2..] {
        if let Some(value) = part.strip_prefix("ctermfg=") {
            color_spec.fg = parse_color_value(value);
        } else if let Some(value) = part.strip_prefix("ctermbg=") {
            color_spec.bg = parse_color_value(value);
        } else if let Some(value) = part.strip_prefix("cterm=") {
            if value.contains("bold") {
                color_spec.bold = true;
            }
            if value.contains("italic") {
                color_spec.italic = true;
            }
        }
    }
    
    def.colors.insert(group, color_spec);
}

fn parse_color_value(value: &str) -> Option<u8> {
    // Parse numeric color codes or color names
    if let Ok(num) = value.parse::<u8>() {
        return Some(num);
    }
    
    // Map common color names to ANSI codes
    match value.to_lowercase().as_str() {
        "black" => Some(0),
        "red" => Some(1),
        "green" => Some(2),
        "yellow" => Some(3),
        "blue" => Some(4),
        "magenta" => Some(5),
        "cyan" => Some(6),
        "white" => Some(7),
        "darkgray" | "darkgrey" => Some(8),
        "lightred" => Some(9),
        "lightgreen" => Some(10),
        "lightyellow" => Some(11),
        "lightblue" => Some(12),
        "lightmagenta" => Some(13),
        "lightcyan" => Some(14),
        "lightgray" | "lightgrey" => Some(15),
        _ => None,
    }
}

fn parse_link_line(line: &str, def: &mut SyntaxDefinition) {
    // Parse: hi def link {group} {target} or hi link {group} {target}
    let parts: Vec<&str> = line.split_whitespace().collect();
    
    // "hi def link Group Target" -> parts[0]="hi", parts[1]="def", parts[2]="link", parts[3]=group, parts[4]=target
    // "hi link Group Target" -> parts[0]="hi", parts[1]="link", parts[2]=group, parts[3]=target
    let (group_idx, target_idx) = if parts.get(1) == Some(&"def") {
        (3, 4)
    } else {
        (2, 3)
    };
    
    if parts.len() > target_idx {
        let group = parts[group_idx].to_string();
        let target = parts[target_idx].to_string();
        def.links.insert(group, target);
    }
}

fn resolve_links(def: &mut SyntaxDefinition) {
    // Resolve all links to actual colors
    let mut resolved = HashMap::new();
    
    for (group, target) in &def.links {
        if let Some(color) = resolve_link_chain(target, def) {
            resolved.insert(group.clone(), color);
        }
    }
    
    // Add resolved colors to the colors map
    for (group, color) in resolved {
        def.colors.insert(group, color);
    }
}

fn resolve_link_chain(group: &str, def: &SyntaxDefinition) -> Option<ColorSpec> {
    // First check if this group has a direct color definition
    if let Some(color) = def.colors.get(group) {
        return Some(color.clone());
    }
    
    // Check if this is a default Vim highlight group
    if let Some(color) = get_default_color(group) {
        return Some(color);
    }
    
    // Follow the link chain (with cycle detection)
    let mut visited = std::collections::HashSet::new();
    let mut current = group;
    
    while let Some(next) = def.links.get(current) {
        if !visited.insert(current) {
            // Cycle detected
            return None;
        }
        
        // Check if the target has a direct color
        if let Some(color) = def.colors.get(next) {
            return Some(color.clone());
        }
        
        // Check if the target is a default group
        if let Some(color) = get_default_color(next) {
            return Some(color);
        }
        
        current = next;
    }
    
    None
}

fn get_default_color(group: &str) -> Option<ColorSpec> {
    // Map default Vim highlight groups to colors
    // Based on common terminal color schemes
    match group {
        "Comment" => Some(ColorSpec {
            fg: Some(8), // DarkGrey
            bg: None,
            bold: false,
            italic: true,
        }),
        "Constant" | "String" | "Character" | "Number" | "Boolean" | "Float" => Some(ColorSpec {
            fg: Some(9), // Red
            bg: None,
            bold: false,
            italic: false,
        }),
        "Identifier" | "Function" => Some(ColorSpec {
            fg: Some(14), // Cyan
            bg: None,
            bold: false,
            italic: false,
        }),
        "Statement" | "Conditional" | "Repeat" | "Label" | "Operator" | "Keyword" | "Exception" => Some(ColorSpec {
            fg: Some(11), // Yellow
            bg: None,
            bold: true,
            italic: false,
        }),
        "PreProc" | "Include" | "Define" | "Macro" | "PreCondit" => Some(ColorSpec {
            fg: Some(13), // Magenta
            bg: None,
            bold: false,
            italic: false,
        }),
        "Type" | "StorageClass" | "Structure" | "Typedef" => Some(ColorSpec {
            fg: Some(10), // Green
            bg: None,
            bold: false,
            italic: false,
        }),
        "Special" | "SpecialChar" | "Tag" | "Delimiter" | "SpecialComment" | "Debug" => Some(ColorSpec {
            fg: Some(12), // Blue
            bg: None,
            bold: false,
            italic: false,
        }),
        "Underlined" => Some(ColorSpec {
            fg: Some(13), // Magenta
            bg: None,
            bold: false,
            italic: false,
        }),
        "Ignore" => Some(ColorSpec {
            fg: Some(8), // DarkGrey
            bg: None,
            bold: false,
            italic: false,
        }),
        "Error" => Some(ColorSpec {
            fg: Some(15), // White
            bg: Some(1),  // DarkRed
            bold: true,
            italic: false,
        }),
        "Todo" => Some(ColorSpec {
            fg: Some(0),  // Black
            bg: Some(11), // Yellow
            bold: true,
            italic: false,
        }),
        _ => None,
    }
}

/// Clear syntax cache (useful for testing)
#[cfg(test)]
pub(crate) fn clear_syntax_cache() {
    if let Ok(mut cache) = SYNTAX_CACHE.lock() {
        cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::set_temp_home;
    use std::fs;

    #[test]
    fn parse_keyword_syntax() {
        let syntax = r#"
" Comment line
syn keyword rustKeyword fn let mut const
syn keyword rustType u8 u16 u32 String
"#;
        let def = parse_vim_syntax(syntax).unwrap();
        assert!(def.keywords.contains_key("rustKeyword"));
        assert!(def.keywords.get("rustKeyword").unwrap().contains(&"fn".to_string()));
        assert!(def.keywords.get("rustType").unwrap().contains(&"String".to_string()));
    }

    #[test]
    fn parse_match_syntax() {
        let syntax = r#"
syn match rustNumber /\d\+/
syn match rustComment /\/\/.*/
"#;
        let def = parse_vim_syntax(syntax).unwrap();
        assert_eq!(def.matches.len(), 2);
        assert_eq!(def.matches[0].group, "rustNumber");
    }

    #[test]
    fn parse_highlight_colors() {
        let syntax = r#"
hi rustKeyword ctermfg=blue cterm=bold
hi rustComment ctermfg=green
hi rustString ctermfg=red ctermbg=black
"#;
        let def = parse_vim_syntax(syntax).unwrap();
        let keyword_color = def.colors.get("rustKeyword").unwrap();
        assert_eq!(keyword_color.fg, Some(4)); // blue
        assert!(keyword_color.bold);
        
        let comment_color = def.colors.get("rustComment").unwrap();
        assert_eq!(comment_color.fg, Some(2)); // green
    }

    #[test]
    fn load_syntax_from_file() {
        let (_tmp, _guard) = set_temp_home();
        clear_syntax_cache();
        
        let syntax_dir = _tmp.path().join(".ue").join("syntax");
        fs::create_dir_all(&syntax_dir).unwrap();
        
        let rust_syntax = r#"
syn keyword rustKeyword fn let
hi rustKeyword ctermfg=blue
"#;
        fs::write(syntax_dir.join("rs.vim"), rust_syntax).unwrap();
        
        let def = get_syntax_for_extension("rs").unwrap();
        assert!(def.keywords.contains_key("rustKeyword"));
    }

    #[test]
    fn cache_syntax_definitions() {
        let (_tmp, _guard) = set_temp_home();
        clear_syntax_cache();
        
        let syntax_dir = _tmp.path().join(".ue").join("syntax");
        fs::create_dir_all(&syntax_dir).unwrap();
        
        let syntax_file = syntax_dir.join("test.vim");
        fs::write(&syntax_file, "syn keyword testKeyword foo").unwrap();
        
        // Load once
        let def1 = get_syntax_for_extension("test");
        assert!(def1.is_some());
        
        // Delete file
        fs::remove_file(&syntax_file).unwrap();
        
        // Should still be cached
        let def2 = get_syntax_for_extension("test");
        assert!(def2.is_some());
    }

    #[test]
    fn parse_hi_def_link() {
        let syntax = r#"
syn keyword csType int string bool
hi def link csType Type
"#;
        let def = parse_vim_syntax(syntax).unwrap();
        
        // Should have the keyword defined
        assert!(def.keywords.contains_key("csType"));
        assert!(def.keywords.get("csType").unwrap().contains(&"int".to_string()));
        
        // Should resolve the link to the default Type color (green)
        assert!(def.colors.contains_key("csType"));
        let color = def.colors.get("csType").unwrap();
        assert_eq!(color.fg, Some(10)); // Green for Type
    }

    #[test]
    fn parse_hi_link_chain() {
        let syntax = r#"
syn keyword myKeyword fn let
hi def link myKeyword Statement
"#;
        let def = parse_vim_syntax(syntax).unwrap();
        
        // Should resolve Statement to yellow + bold
        assert!(def.colors.contains_key("myKeyword"));
        let color = def.colors.get("myKeyword").unwrap();
        assert_eq!(color.fg, Some(11)); // Yellow for Statement
        assert!(color.bold);
    }
}
