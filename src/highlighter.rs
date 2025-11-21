use crate::syntax::{SyntaxDefinition, ColorSpec};

/// A span of text with associated color/style information.
#[derive(Debug, Clone)]
pub(crate) struct StyledSpan {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) color_spec: ColorSpec,
}

/// Tokenize a line and return styled spans based on syntax definition.
/// 
/// Applies both regex-based pattern matches and keyword matches, checking
/// word boundaries for keywords. Spans are sorted by start position.
/// 
/// # Arguments
/// * `line` - The line of text to highlight
/// * `syntax` - The syntax definition containing patterns and colors
/// 
/// # Returns
/// A vector of styled spans indicating where to apply colors
pub(crate) fn highlight_line(line: &str, syntax: &SyntaxDefinition) -> Vec<StyledSpan> {
    let mut spans = Vec::new();
    
    // First pass: match patterns (regex-based)
    for syn_match in &syntax.matches {
        for mat in syn_match.pattern.find_iter(line) {
            // Skip zero-length matches - they serve no purpose and block keyword matching
            if mat.start() == mat.end() {
                continue;
            }
            
            if let Some(color_spec) = syntax.colors.get(&syn_match.group) {
                spans.push(StyledSpan {
                    start: mat.start(),
                    end: mat.end(),
                    color_spec: color_spec.clone(),
                });
            }
        }
    }
    
    // Second pass: match keywords (collect candidates first)
    let mut keyword_candidates = Vec::new();
    for (group, keywords) in &syntax.keywords {
        if let Some(color_spec) = syntax.colors.get(group) {
            for keyword in keywords {
                // Find whole word matches only
                let mut search_start = 0;
                while let Some(pos) = line[search_start..].find(keyword) {
                    let absolute_pos = search_start + pos;
                    let end_pos = absolute_pos + keyword.len();
                    
                    // Check word boundaries
                    let before_ok = absolute_pos == 0 || !is_word_char(line.chars().nth(absolute_pos - 1).unwrap_or(' '));
                    let after_ok = end_pos >= line.len() || !is_word_char(line.chars().nth(end_pos).unwrap_or(' '));
                    
                    if before_ok && after_ok {
                        keyword_candidates.push(StyledSpan {
                            start: absolute_pos,
                            end: end_pos,
                            color_spec: color_spec.clone(),
                        });
                    }
                    
                    search_start = absolute_pos + 1;
                }
            }
        }
    }
    
    // Filter out keyword candidates that overlap with regex-based spans
    // Regex spans take precedence over keywords
    for candidate in keyword_candidates {
        if !overlaps_existing(&spans, candidate.start, candidate.end) {
            spans.push(candidate);
        }
    }
    
    // Sort spans by start position
    spans.sort_by_key(|s| s.start);
    
    spans
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn overlaps_existing(spans: &[StyledSpan], start: usize, end: usize) -> bool {
    spans.iter().any(|span| {
        !(end <= span.start || start >= span.end)
    })
}


/// Get the extension from a filename
pub(crate) fn get_file_extension(filename: &str) -> Option<String> {
    let path = std::path::Path::new(filename);
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::{SyntaxDefinition, ColorSpec};
    use std::collections::HashMap;

    fn make_color(fg: u8) -> ColorSpec {
        ColorSpec {
            fg: Some(fg),
            bg: None,
            bold: false,
            italic: false,
        }
    }

    #[test]
    fn highlight_keywords() {
        let mut def = SyntaxDefinition {
            keywords: HashMap::new(),
            matches: Vec::new(),
            colors: HashMap::new(),
            links: HashMap::new(),
        };
        
        def.keywords.insert("Keyword".to_string(), vec!["fn".to_string(), "let".to_string()]);
        def.colors.insert("Keyword".to_string(), make_color(4)); // blue
        
        let line = "fn main() { let x = 5; }";
        let spans = highlight_line(line, &def);
        
        assert!(spans.iter().any(|s| s.start == 0 && s.end == 2)); // "fn"
        assert!(spans.iter().any(|s| s.start == 12 && s.end == 15)); // "let"
    }

    #[test]
    fn highlight_respects_word_boundaries() {
        let mut def = SyntaxDefinition {
            keywords: HashMap::new(),
            matches: Vec::new(),
            colors: HashMap::new(),
            links: HashMap::new(),
        };
        
        def.keywords.insert("Keyword".to_string(), vec!["in".to_string()]);
        def.colors.insert("Keyword".to_string(), make_color(4));
        
        let line = "in main inside";
        let spans = highlight_line(line, &def);
        
        // Should only match standalone "in", not "in" within "main" or "inside"
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start, 0);
        assert_eq!(spans[0].end, 2);
    }

    #[test]
    fn get_extension_from_filename() {
        assert_eq!(get_file_extension("test.rs"), Some("rs".to_string()));
        assert_eq!(get_file_extension("/path/to/file.py"), Some("py".to_string()));
        assert_eq!(get_file_extension("noextension"), None);
        assert_eq!(get_file_extension(".hidden"), None);
    }
}

