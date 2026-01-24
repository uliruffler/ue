use crate::editor_state::FileViewerState;
use crate::settings::Settings;

/// Unicode character for line wrap indicator (carriage return arrow)
pub const WRAP_INDICATOR: char = '↩';

/// Calculate the visual width of a string, considering tabs
/// Tabs are expanded to the next multiple of tab_width
pub fn visual_width(s: &str, tab_width: usize) -> usize {
    let mut width = 0;
    for ch in s.chars() {
        if ch == '\t' {
            // Calculate spaces to next tab stop
            let spaces_to_next_tab = tab_width - (width % tab_width);
            width += spaces_to_next_tab;
        } else {
            width += 1;
        }
    }
    width
}


/// Calculate wrap points for word wrapping
/// Returns vector of character indices where line should be broken
/// Reserve 1 character width for the wrap indicator
pub(crate) fn calculate_word_wrap_points(line: &str, text_width: usize, tab_width: usize) -> Vec<usize> {
    if text_width == 0 || line.is_empty() {
        return vec![];
    }

    let usable_width = text_width.saturating_sub(1); // Reserve 1 for wrap indicator
    let max_word_length = text_width / 2;
    let mut wrap_points = vec![];
    let chars: Vec<char> = line.chars().collect();

    let mut line_start_idx = 0;

    while line_start_idx < chars.len() {
        let remaining_line: String = chars[line_start_idx..].iter().collect();
        let remaining_visual_width = visual_width(&remaining_line, tab_width);

        if remaining_visual_width <= usable_width {
            // Rest of line fits, we're done
            break;
        }

        // Find where to break
        // First, try to find a word boundary (whitespace)
        let mut best_break = None;
        let mut current_visual = 0;
        let mut last_space_idx = None;
        let mut word_start_idx = line_start_idx;
        let mut in_word = false;

        for i in line_start_idx..chars.len() {
            let ch = chars[i];
            let char_visual_width = if ch == '\t' {
                tab_width - (current_visual % tab_width)
            } else {
                1
            };

            if current_visual + char_visual_width > usable_width {
                // Would exceed line width
                if let Some(space_idx) = last_space_idx {
                    // Check if the word is reasonable length
                    let word_text: String = chars[word_start_idx..i].iter().collect();
                    let word_visual = visual_width(&word_text, tab_width);

                    if word_visual <= max_word_length && space_idx > line_start_idx {
                        // Break at the last space
                        best_break = Some(space_idx + 1); // Break after the space
                        break;
                    }
                }

                // No good word boundary found, use character wrap
                // Go back to find where we can fit
                if i > line_start_idx {
                    best_break = Some(i);
                } else {
                    best_break = Some(line_start_idx + 1);
                }
                break;
            }

            current_visual += char_visual_width;

            if ch.is_whitespace() {
                last_space_idx = Some(i);
                in_word = false;
            } else if !in_word {
                word_start_idx = i;
                in_word = true;
            }
        }

        if let Some(break_idx) = best_break {
            if break_idx > line_start_idx && break_idx < chars.len() {
                wrap_points.push(break_idx);
                line_start_idx = break_idx;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    wrap_points
}

/// Calculate visual width up to a given character index in a string
pub fn visual_width_up_to(s: &str, char_index: usize, tab_width: usize) -> usize {
    let mut width = 0;
    for (i, ch) in s.chars().enumerate() {
        if i >= char_index {
            break;
        }
        if ch == '\t' {
            let spaces_to_next_tab = tab_width - (width % tab_width);
            width += spaces_to_next_tab;
        } else {
            width += 1;
        }
    }
    width
}

/// Calculate the width of the line number column based on settings
pub(crate) fn line_number_width(settings: &Settings) -> u16 {
    if settings.appearance.line_number_digits == 0 {
        0
    } else {
        // digits + 1 space separator
        settings.appearance.line_number_digits as u16 + 1
    }
}

/// Calculate the actual display width for line numbers based on document length
/// This accounts for documents that need more digits than the setting specifies
pub(crate) fn line_number_display_width(settings: &Settings, total_lines: usize) -> u16 {
    if settings.appearance.line_number_digits == 0 {
        0
    } else {
        // Calculate the width needed for the document
        let actual_width = if total_lines == 0 {
            1
        } else {
            ((total_lines as f64).log10().floor() as usize) + 1
        };

        // Use the larger of line_number_digits or actual_width, plus 1 for space separator
        let display_width = actual_width.max(settings.appearance.line_number_digits as usize);
        (display_width + 1) as u16
    }
}

pub fn calculate_wrapped_lines_for_line(
    lines: &[String],
    line_index: usize,
    text_width: u16,
    tab_width: usize,
) -> u16 {
    calculate_wrapped_lines_for_line_with_wrapping(lines, line_index, text_width, tab_width, true)
}

/// Get the character range for a specific wrap segment
/// Returns (start_char_index, end_char_index) for the given wrap_index
pub(crate) fn get_wrap_segment_range(
    line: &str,
    wrap_index: usize,
    text_width: usize,
    tab_width: usize,
) -> (usize, usize) {
    let wrap_points = calculate_word_wrap_points(line, text_width, tab_width);
    let line_len = line.chars().count();

    if wrap_index == 0 {
        if wrap_points.is_empty() {
            (0, line_len)
        } else {
            (0, wrap_points[0])
        }
    } else if wrap_index <= wrap_points.len() {
        let start = wrap_points[wrap_index - 1];
        let end = if wrap_index < wrap_points.len() {
            wrap_points[wrap_index]
        } else {
            line_len
        };
        (start, end)
    } else {
        (line_len, line_len)
    }
}

pub(crate) fn calculate_wrapped_lines_for_line_with_wrapping(
    lines: &[String],
    line_index: usize,
    text_width: u16,
    tab_width: usize,
    wrapping_enabled: bool,
) -> u16 {
    if line_index >= lines.len() {
        return 1;
    }

    // If wrapping is disabled, each logical line is exactly 1 visual line
    if !wrapping_enabled {
        return 1;
    }

    let line = &lines[line_index];
    let width = text_width as usize;

    if width == 0 {
        return 1;
    }

    // Use word wrapping
    let wrap_points = calculate_word_wrap_points(line, width, tab_width);
    (wrap_points.len() + 1).max(1) as u16
}

pub(crate) fn calculate_cursor_visual_line(
    lines: &[String],
    state: &FileViewerState,
    text_width: u16,
) -> usize {
    let mut visual_line = 0;
    let text_width_usize = text_width as usize;
    let tab_width = state.settings.tab_width;
    let wrapping_enabled = state.is_line_wrapping_enabled();

    // Count visual lines from top_line to cursor's logical line
    for i in state.top_line..state.absolute_line() {
        if wrapping_enabled {
            visual_line += calculate_wrapped_lines_for_line(lines, i, text_width, tab_width) as usize;
        } else {
            visual_line += 1; // When wrapping is disabled, each line is exactly 1 visual line
        }
    }

    // Add the visual line offset within the cursor's logical line itself
    if wrapping_enabled && text_width_usize > 0 && state.absolute_line() < lines.len() {
        let line = &lines[state.absolute_line()];
        let cursor_col = state.cursor_col;

        // Find which wrap segment the cursor is in
        let wrap_points = calculate_word_wrap_points(line, text_width_usize, tab_width);

        for (idx, &wrap_point) in wrap_points.iter().enumerate() {
            if cursor_col < wrap_point {
                visual_line += idx;
                break;
            }
        }
        // If cursor is after all wrap points, it's on the last segment
        if wrap_points.is_empty() || cursor_col >= *wrap_points.last().unwrap() {
            visual_line += wrap_points.len();
        }
    }
    // When wrapping is disabled, cursor is always on the same visual line as the logical line (no offset)

    visual_line
}

/// Calculate total visual lines consumed from top_line through cursor_line (inclusive)
/// This accounts for line wrapping - a logical line may consume multiple visual lines
pub(crate) fn calculate_visual_lines_to_cursor(
    lines: &[String],
    state: &FileViewerState,
    text_width: u16,
) -> usize {
    let tab_width = state.settings.tab_width;
    let mut visual_lines = 0;
    let wrapping_enabled = state.is_line_wrapping_enabled();

    // Count visual lines from top_line up to and including cursor_line
    let end_line = (state.top_line + state.cursor_line).min(lines.len());
    for i in state.top_line..end_line {
        if wrapping_enabled {
            visual_lines += calculate_wrapped_lines_for_line(lines, i, text_width, tab_width) as usize;
        } else {
            visual_lines += 1; // When wrapping is disabled, each line is exactly 1 visual line
        }
    }

    // Add the wrapped lines for the cursor's current line
    if end_line < lines.len() {
        if wrapping_enabled {
            visual_lines += calculate_wrapped_lines_for_line(lines, end_line, text_width, tab_width) as usize;
        } else {
            visual_lines += 1; // When wrapping is disabled, each line is exactly 1 visual line
        }
    }

    visual_lines
}

/// Calculate total visual lines for the entire document, accounting for line wrapping
/// This is used to determine if scrollbar should be shown
pub fn calculate_total_visual_lines(
    lines: &[String],
    state: &FileViewerState,
    text_width: u16,
) -> usize {
    let tab_width = state.settings.tab_width;
    let wrapping_enabled = state.is_line_wrapping_enabled();
    let mut total_visual_lines = 0;

    for i in 0..lines.len() {
        if wrapping_enabled {
            total_visual_lines += calculate_wrapped_lines_for_line(lines, i, text_width, tab_width) as usize;
        } else {
            total_visual_lines += 1;
        }
    }

    total_visual_lines
}

/// Calculate total visual lines before top_line (for scrollbar positioning)
pub fn calculate_total_visual_lines_before(
    lines: &[String],
    state: &FileViewerState,
    text_width: u16,
) -> usize {
    let tab_width = state.settings.tab_width;
    let wrapping_enabled = state.is_line_wrapping_enabled();
    let mut visual_lines_before = 0;

    for i in 0..state.top_line {
        if i >= lines.len() {
            break;
        }
        if wrapping_enabled {
            visual_lines_before += calculate_wrapped_lines_for_line(lines, i, text_width, tab_width) as usize;
        } else {
            visual_lines_before += 1;
        }
    }

    visual_lines_before
}

pub(crate) fn visual_to_logical_position(
    state: &FileViewerState,
    lines: &[String],
    visual_line: usize,
    column: u16,
    visible_lines: usize,
) -> Option<(usize, usize)> {
    let line_num_width = line_number_width(state.settings);
    let text_width = calculate_text_width(state, lines, visible_lines);
    let tab_width = state.settings.tab_width;

    // Always reserve space for scrollbar to prevent text jumping
    let scrollbar_width = 1;
    let text_start = line_num_width;
    let text_end = state.term_width.saturating_sub(scrollbar_width);

    if column < text_start || column >= text_end {
        return None; // Click was on line number area or scrollbar
    }

    let text_col = (column - line_num_width) as usize;

    // Get filtered lines if filter mode is active
    let filtered_lines = if state.filter_active && state.last_search_pattern.is_some() {
        let pattern = state.last_search_pattern.as_ref().unwrap();
        crate::find::get_lines_with_matches_and_context(
            lines,
            pattern,
            state.find_regex_mode,
            state.find_scope,
            state.filter_context_before,
            state.filter_context_after,
        )
    } else {
        Vec::new()
    };

    // Find which logical line this visual line corresponds to
    let mut current_visual_line = 0;
    
    if state.filter_active && !filtered_lines.is_empty() {
        // Filter mode: iterate through filtered lines only
        let mut filtered_index = 0;
        
        // Find starting position in filtered lines based on top_line
        while filtered_index < filtered_lines.len() && filtered_lines[filtered_index] < state.top_line {
            filtered_index += 1;
        }
        
        while filtered_index < filtered_lines.len() {
            let logical_line = filtered_lines[filtered_index];
            let wrapping_enabled = state.is_line_wrapping_enabled();
            let wrapped_lines = if wrapping_enabled {
                calculate_wrapped_lines_for_line(lines, logical_line, text_width, tab_width) as usize
            } else {
                1  // No wrapping - each line is exactly 1 visual line
            };

            if current_visual_line + wrapped_lines > visual_line {
                // This is the logical line we're looking for
                let line_offset = visual_line - current_visual_line;

                // Calculate visual column in line, accounting for horizontal scroll
                let visual_col_in_line = if wrapping_enabled {
                    // Wrapped mode with word wrapping: use wrap points
                    let line = &lines[logical_line];
                    let wrap_points = calculate_word_wrap_points(line, text_width as usize, tab_width);

                    if wrap_points.is_empty() || line_offset == 0 {
                        // First segment or no wrapping
                        text_col
                    } else if line_offset - 1 < wrap_points.len() {
                        // Subsequent segment - add offset from wrap point
                        let segment_start_char = wrap_points[line_offset - 1];
                        let segment_start_visual = visual_width_up_to(line, segment_start_char, tab_width);
                        segment_start_visual + text_col
                    } else {
                        // Beyond all wrap points - shouldn't happen but handle gracefully
                        let last_wrap = wrap_points.last().copied().unwrap_or(0);
                        let last_visual = visual_width_up_to(line, last_wrap, tab_width);
                        last_visual + text_col
                    }
                } else {
                    // Horizontal scroll mode: add scroll offset to screen column
                    state.horizontal_scroll_offset + text_col
                };

                // Convert visual column to character index considering tabs
                let line = &lines[logical_line];
                let col_in_line = visual_col_to_char_index(line, visual_col_in_line, tab_width);
                return Some((logical_line, col_in_line));
            }

            current_visual_line += wrapped_lines;
            filtered_index += 1;
        }
    } else {
        // Normal mode: iterate through all lines
        let mut logical_line = state.top_line;

        while logical_line < lines.len() {
            let wrapping_enabled = state.is_line_wrapping_enabled();
            let wrapped_lines = if wrapping_enabled {
                calculate_wrapped_lines_for_line(lines, logical_line, text_width, tab_width) as usize
            } else {
                1  // No wrapping - each line is exactly 1 visual line
            };

            if current_visual_line + wrapped_lines > visual_line {
                // This is the logical line we're looking for
                let line_offset = visual_line - current_visual_line;

                // Calculate visual column in line, accounting for horizontal scroll
                let visual_col_in_line = if wrapping_enabled {
                    // Wrapped mode with word wrapping: use wrap points
                    let line = &lines[logical_line];
                    let wrap_points = calculate_word_wrap_points(line, text_width as usize, tab_width);
                    
                    if wrap_points.is_empty() || line_offset == 0 {
                        // First segment or no wrapping
                        text_col
                    } else if line_offset - 1 < wrap_points.len() {
                        // Subsequent segment - add offset from wrap point
                        let segment_start_char = wrap_points[line_offset - 1];
                        let segment_start_visual = visual_width_up_to(line, segment_start_char, tab_width);
                        segment_start_visual + text_col
                    } else {
                        // Beyond all wrap points - shouldn't happen but handle gracefully
                        let last_wrap = wrap_points.last().copied().unwrap_or(0);
                        let last_visual = visual_width_up_to(line, last_wrap, tab_width);
                        last_visual + text_col
                    }
                } else {
                    // Horizontal scroll mode: add scroll offset to screen column
                    state.horizontal_scroll_offset + text_col
                };

                // Convert visual column to character index considering tabs
                let line = &lines[logical_line];
                let mut col_in_line = visual_col_to_char_index(line, visual_col_in_line, tab_width);
                
                // Check if we're past the wrap point for this segment - if so, clamp to wrap point
                if wrapping_enabled {
                    let wrap_points = calculate_word_wrap_points(line, text_width as usize, tab_width);
                    
                    // Find which wrap point applies to this segment
                    if line_offset < wrap_points.len() {
                        let segment_wrap_point = wrap_points[line_offset];
                        
                        // If calculated position is beyond this segment's wrap point, clamp it
                        if col_in_line > segment_wrap_point {
                            col_in_line = segment_wrap_point;
                        }
                    }
                }
                
                return Some((logical_line, col_in_line));
            }

            current_visual_line += wrapped_lines;
            logical_line += 1;
        }
    }

    None
}

/// Convert a visual column position to a character index, considering tabs
pub fn visual_col_to_char_index(line: &str, visual_col: usize, tab_width: usize) -> usize {
    let mut current_visual = 0;
    for (char_idx, ch) in line.chars().enumerate() {
        if current_visual >= visual_col {
            return char_idx;
        }
        if ch == '\t' {
            let spaces_to_next_tab = tab_width - (current_visual % tab_width);
            current_visual += spaces_to_next_tab;
        } else {
            current_visual += 1;
        }
    }
    line.chars().count()
}

/// Helper to adjust top_line on resize without losing scroll position
pub(crate) fn adjust_view_for_resize(
    prev_top_line: usize,
    absolute_cursor_line: usize,
    visible_lines: usize,
    total_lines: usize,
) -> (usize, usize) {
    if total_lines == 0 {
        return (0, 0);
    }
    // Clamp visible_lines to at least 1
    let vl = visible_lines.max(1);
    let mut new_top = prev_top_line.min(total_lines.saturating_sub(1));
    // Ensure cursor is visible: if above, move top up to cursor
    if absolute_cursor_line < new_top {
        new_top = absolute_cursor_line;
    }
    // If below bottom, scroll so cursor is last visible line
    if absolute_cursor_line >= new_top + vl {
        new_top = absolute_cursor_line.saturating_sub(vl - 1);
    }
    // Final clamp
    let max_top = total_lines.saturating_sub(1);
    if new_top > max_top {
        new_top = max_top;
    }
    let rel_cursor = absolute_cursor_line.saturating_sub(new_top);
    (new_top, rel_cursor)
}
/// Calculate text width available for displaying content (terminal width minus line numbers and scrollbar)
/// The scrollbar is always visible to prevent text jumping, so we always reserve 1 column for it.
pub fn calculate_text_width(
    state: &FileViewerState,
    _lines: &[String],
    _visible_lines: usize,
) -> u16 {
    let line_num_width = line_number_width(state.settings);

    // Always reserve space for scrollbar (1 column) to prevent text jumping
    state.term_width
        .saturating_sub(line_num_width)
        .saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::adjust_view_for_resize;

    #[test]
    fn resize_keeps_scroll_when_cursor_visible_and_space_expands() {
        // Previously scrolled; expanding view should not jump to top
        let (top, rel) = adjust_view_for_resize(50, 60, 100, 200);
        assert_eq!(top, 50);
        assert_eq!(rel, 10);
    }

    #[test]
    fn resize_scrolls_up_if_cursor_above_top() {
        let (top, rel) = adjust_view_for_resize(30, 10, 25, 100);
        assert_eq!(top, 10);
        assert_eq!(rel, 0);
    }

    #[test]
    fn resize_scrolls_down_if_cursor_below_bottom() {
        let (top, rel) = adjust_view_for_resize(0, 30, 20, 200);
        assert_eq!(top, 11); // 30 - (20 - 1)
        assert_eq!(rel, 19); // cursor becomes last visible line
    }

    #[test]
    fn resize_shrink_preserves_cursor_visibility() {
        let (top, rel) = adjust_view_for_resize(50, 65, 10, 120);
        assert_eq!(top, 56); // 65 - (10 - 1)
        assert_eq!(rel, 9); // last visible line
    }

    #[test]
    fn empty_file_returns_zeroes() {
        let (top, rel) = adjust_view_for_resize(5, 5, 10, 0);
        assert_eq!(top, 0);
        assert_eq!(rel, 0);
    }

    // Tests for visual_width function
    use super::{line_number_width, visual_width, visual_width_up_to};

    #[test]
    fn test_visual_width_empty_string() {
        assert_eq!(visual_width("", 4), 0);
        assert_eq!(visual_width("", 8), 0);
    }

    #[test]
    fn test_visual_width_no_tabs() {
        assert_eq!(visual_width("hello", 4), 5);
        assert_eq!(visual_width("test", 8), 4);
    }

    #[test]
    fn test_visual_width_single_tab_at_start() {
        // Tab at position 0 advances to next tab stop
        assert_eq!(visual_width("\t", 4), 4);
        assert_eq!(visual_width("\t", 8), 8);
    }

    #[test]
    fn test_visual_width_tab_in_middle() {
        // "a\tb" with tab_width=4: 'a' (1), then tab advances to 4, then 'b' (5)
        assert_eq!(visual_width("a\tb", 4), 5);
        // "ab\tc" with tab_width=4: 'ab' (2), tab advances to 4, 'c' (5)
        assert_eq!(visual_width("ab\tc", 4), 5);
        // "abc\td" with tab_width=4: 'abc' (3), tab advances to 4, 'd' (5)
        assert_eq!(visual_width("abc\td", 4), 5);
    }

    #[test]
    fn test_visual_width_multiple_tabs() {
        // "\t\t" with tab_width=4: first tab to 4, second tab to 8
        assert_eq!(visual_width("\t\t", 4), 8);
        // "a\t\t" with tab_width=4: 'a' (1), first tab to 4, second tab to 8
        assert_eq!(visual_width("a\t\t", 4), 8);
    }

    #[test]
    fn test_visual_width_tab_width_8() {
        assert_eq!(visual_width("\t", 8), 8);
        assert_eq!(visual_width("a\tb", 8), 9);
        assert_eq!(visual_width("abcdefg\tx", 8), 9);
    }

    #[test]
    fn test_visual_width_mixed_content() {
        // Complex case: "hello\tworld\t!" with tab_width=4
        // "hello" (5), tab to 8, "world" (13), tab to 16, "!" (17)
        assert_eq!(visual_width("hello\tworld\t!", 4), 17);
    }

    #[test]
    fn test_visual_width_only_tabs() {
        assert_eq!(visual_width("\t\t\t", 4), 12);
        assert_eq!(visual_width("\t\t\t", 8), 24);
    }

    // Tests for visual_width_up_to function
    #[test]
    fn test_visual_width_up_to_empty() {
        assert_eq!(visual_width_up_to("", 0, 4), 0);
        assert_eq!(visual_width_up_to("hello", 0, 4), 0);
    }

    #[test]
    fn test_visual_width_up_to_no_tabs() {
        assert_eq!(visual_width_up_to("hello", 3, 4), 3);
        assert_eq!(visual_width_up_to("world", 5, 4), 5);
    }

    #[test]
    fn test_visual_width_up_to_with_tab() {
        // "a\tbc" with tab_width=4: up to index 1 (before tab) = 1
        assert_eq!(visual_width_up_to("a\tbc", 1, 4), 1);
        // up to index 2 (after tab) = 4 (tab expands to position 4)
        assert_eq!(visual_width_up_to("a\tbc", 2, 4), 4);
        // up to index 3 (after 'b') = 5
        assert_eq!(visual_width_up_to("a\tbc", 3, 4), 5);
    }

    #[test]
    fn test_visual_width_up_to_multiple_tabs() {
        // "\t\tx" with tab_width=4
        assert_eq!(visual_width_up_to("\t\tx", 0, 4), 0);
        assert_eq!(visual_width_up_to("\t\tx", 1, 4), 4);
        assert_eq!(visual_width_up_to("\t\tx", 2, 4), 8);
        assert_eq!(visual_width_up_to("\t\tx", 3, 4), 9);
    }

    #[test]
    fn test_visual_width_up_to_beyond_length() {
        // Should stop at string length
        assert_eq!(visual_width_up_to("hi", 10, 4), 2);
        assert_eq!(visual_width_up_to("\t", 10, 4), 4);
    }

    // Tests for line_number_width function
    #[test]
    fn test_line_number_width_disabled() {
        use crate::settings::Settings;
        let mut settings = Settings::default();
        settings.appearance.line_number_digits = 0;
        assert_eq!(line_number_width(&settings), 0);
    }

    #[test]
    fn test_line_number_width_enabled() {
        use crate::settings::Settings;
        let mut settings = Settings::default();
        settings.appearance.line_number_digits = 3;
        assert_eq!(line_number_width(&settings), 4); // 3 digits + 1 separator

        settings.appearance.line_number_digits = 5;
        assert_eq!(line_number_width(&settings), 6); // 5 digits + 1 separator
    }

    // Tests for calculate_wrapped_lines_for_line function
    #[test]
    fn test_wrapped_lines_empty_line() {
        let lines = vec![String::new()];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 80, 4), 1);
    }

    #[test]
    fn test_wrapped_lines_short_line_no_wrap() {
        let lines = vec!["hello".to_string()];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 80, 4), 1);
    }

    #[test]
    fn test_wrapped_lines_exact_width() {
        // Line is exactly the text width, but no spaces means it's one long word
        // Word wrapping falls back to character wrapping for long words (> width/2)
        // With 1 char reserved for wrap indicator, 80 chars wraps to 2 lines
        let lines = vec!["x".repeat(80)];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 80, 4), 2);
    }

    #[test]
    fn test_wrapped_lines_one_over_wraps_to_two() {
        // Line is one character over the text width
        let lines = vec!["x".repeat(81)];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 80, 4), 2);
    }

    #[test]
    fn test_wrapped_lines_double_width() {
        // Line is exactly double the text width
        // 160 chars with usable width 79 = 3 segments
        let lines = vec!["x".repeat(160)];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 80, 4), 3);
    }

    #[test]
    fn test_wrapped_lines_triple_width() {
        // Line needs multiple visual lines with character wrapping fallback
        // 200 chars with usable width 79 per line = 3 segments
        let lines = vec!["x".repeat(200)];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 80, 4), 3);
    }

    #[test]
    fn test_wrapped_lines_with_tabs() {
        // Tab expands to 4 spaces, so "\t" has visual width 4
        let lines = vec!["\t\t\t\t\t\t\t\t\t\t".to_string()]; // 10 tabs = 40 visual width
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 80, 4), 1);

        // 21 tabs = 84 visual width, wraps to 2 lines
        let lines = vec!["\t".repeat(21)];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 80, 4), 2);
    }

    #[test]
    fn test_wrapped_lines_mixed_tabs_and_chars() {
        // "hello\tworld" where tab expands from position 5
        // "hello" = 5, tab goes to 8 (3 spaces), "world" = 5, total = 13
        let lines = vec!["hello\tworld".to_string()];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 80, 4), 1);
    }

    #[test]
    fn test_wrapped_lines_zero_width_returns_one() {
        let lines = vec!["hello".to_string()];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 0, 4), 1);
    }

    #[test]
    fn test_word_wrapping_with_spaces() {
        // Test actual word wrapping with whitespace
        // "hello world test" should fit on one line at width 80
        let lines = vec!["hello world test".to_string()];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 80, 4), 1);

        // Short width forces word wrap
        let lines = vec!["hello world".to_string()];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 10, 4), 2);
    }

    #[test]
    fn test_word_wrapping_preserves_words() {
        // Words should not be broken if they fit within max word length
        // Width 20, max word = 10
        // "short words here" (5+6+4 = 15 chars + 2 spaces) should wrap at word boundaries
        let lines = vec!["short words here".to_string()];
        let wrapped = calculate_wrapped_lines_for_line(&lines, 0, 20, 4);
        assert!(wrapped >= 1);
    }


    #[test]
    fn test_wrapped_lines_beyond_line_count() {
        let lines = vec!["line1".to_string()];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 5, 80, 4), 1);
    }

    // Tests for calculate_cursor_visual_line function
    use crate::editor_state::FileViewerState;
    use crate::settings::Settings;
    use crate::undo::UndoHistory;
    use super::{calculate_cursor_visual_line, calculate_visual_lines_to_cursor,
                visual_col_to_char_index, calculate_text_width, visual_to_logical_position,
                calculate_wrapped_lines_for_line};

    fn create_test_state_for_coords(settings: &Settings) -> FileViewerState<'_> {
        FileViewerState {
            top_line: 0,
            cursor_line: 0,
            cursor_col: 0,
            desired_cursor_col: 0,
            selection_start: None,
            selection_end: None,
            selection_anchor: None,
            block_selection: false,
            multi_cursors: Vec::new(),
            cursor_blink_state: false,
            last_blink_time: None,
            needs_redraw: false,
            needs_footer_redraw: false,
            modified: false,
            term_width: 80,
            undo_history: UndoHistory::new(),
            settings,
            mouse_dragging: false,
            saved_absolute_cursor: None,
            saved_scroll_state: None,
            drag_source_start: None,
            drag_source_end: None,
            drag_text: None,
            dragging_selection_active: false,
            drag_target: None,
            last_save_time: None,
            find_active: false,
            find_regex_mode: true,
            find_via_replace: false,
            filter_active: false,
            filter_context_before: 0,
            filter_context_after: 0,
            find_pattern: String::new(),
            find_cursor_pos: 0,
            find_selection: None,
            find_error: None,
            find_history: Vec::new(),
            find_history_index: None,
            last_search_pattern: None,
            last_search_regex_mode: true,
            saved_search_pattern: None,
            search_wrapped: false,
            wrap_warning_pending: None,
            find_scope: None,
            search_hit_count: 0,
            search_current_hit: 0,
            replace_active: false,
            replace_pattern: String::new(),
            replace_cursor_pos: 0,
            replace_selection: None,
            goto_line_active: false,
            goto_line_input: String::new(),
            goto_line_cursor_pos: 0,
            goto_line_typing_started: false,
            scrollbar_dragging: false,
            scrollbar_drag_start_top_line: 0,
            scrollbar_drag_start_y: 0,
            scrollbar_drag_bar_offset: 0,
            h_scrollbar_dragging: false,
            h_scrollbar_drag_start_offset: 0,
            h_scrollbar_drag_start_x: 0,
            h_scrollbar_drag_bar_offset: 0,
            help_active: false,
            help_context: crate::help::HelpContext::Editor,
            help_scroll_offset: 0,
            horizontal_scroll_offset: 0,
            line_wrapping_override: None,
            last_click_time: None,
            last_click_pos: None,
            click_count: 0,
            last_drag_position: None,
            menu_bar: crate::menu::MenuBar::new(),
            pending_menu_action: None,
            close_all_confirmation_active: false,
            close_all_confirmed: false,
            is_untitled: false,
            cursor_at_wrap_end: false,
            status_message: None,
        }
    }

    #[test]
    fn test_cursor_visual_line_at_top_no_wrap() {
        let settings = Settings::default();
        let mut state = create_test_state_for_coords(&settings);
        let lines = vec!["line1".to_string(), "line2".to_string()];

        state.top_line = 0;
        state.cursor_line = 0;
        state.cursor_col = 0;

        let visual_line = calculate_cursor_visual_line(&lines, &state, 80);
        assert_eq!(visual_line, 0);
    }

    #[test]
    fn test_cursor_visual_line_second_line_no_wrap() {
        let settings = Settings::default();
        let mut state = create_test_state_for_coords(&settings);
        let lines = vec!["line1".to_string(), "line2".to_string()];

        state.top_line = 0;
        state.cursor_line = 1;
        state.cursor_col = 0;

        let visual_line = calculate_cursor_visual_line(&lines, &state, 80);
        assert_eq!(visual_line, 1);
    }

    #[test]
    fn test_cursor_visual_line_with_wrapped_previous_line() {
        let settings = Settings::default();
        let mut state = create_test_state_for_coords(&settings);
        // First line wraps to 2 visual lines, cursor on second logical line
        let lines = vec!["x".repeat(100), "line2".to_string()];

        state.top_line = 0;
        state.cursor_line = 1;
        state.cursor_col = 0;

        let visual_line = calculate_cursor_visual_line(&lines, &state, 80);
        assert_eq!(visual_line, 2); // First line takes 2 visual lines
    }

    #[test]
    fn test_cursor_visual_line_within_wrapped_line_first_wrap() {
        let settings = Settings::default();
        let mut state = create_test_state_for_coords(&settings);
        let lines = vec!["x".repeat(100)];

        state.top_line = 0;
        state.cursor_line = 0;
        state.cursor_col = 50; // Within first 80 characters

        let visual_line = calculate_cursor_visual_line(&lines, &state, 80);
        assert_eq!(visual_line, 0); // Still on first visual line
    }

    #[test]
    fn test_cursor_visual_line_within_wrapped_line_second_wrap() {
        let settings = Settings::default();
        let mut state = create_test_state_for_coords(&settings);
        let lines = vec!["x".repeat(100)];

        state.top_line = 0;
        state.cursor_line = 0;
        state.cursor_col = 85; // Beyond first 80 characters

        let visual_line = calculate_cursor_visual_line(&lines, &state, 80);
        assert_eq!(visual_line, 1); // On second visual line
    }

    #[test]
    fn test_cursor_visual_line_after_scrolling() {
        let settings = Settings::default();
        let mut state = create_test_state_for_coords(&settings);
        let lines = vec![
            "line1".to_string(),
            "line2".to_string(),
            "line3".to_string(),
        ];

        state.top_line = 1; // Scrolled down by 1
        state.cursor_line = 1; // Relative cursor at line 1
        // Absolute cursor is at line 2 (top_line + cursor_line)

        let visual_line = calculate_cursor_visual_line(&lines, &state, 80);
        assert_eq!(visual_line, 1); // One line visible above cursor
    }

    #[test]
    fn test_cursor_visual_line_after_scrolling_with_wrapped_lines() {
        let settings = Settings::default();
        let mut state = create_test_state_for_coords(&settings);
        let lines = vec![
            "x".repeat(150), // Wraps to 2 visual lines
            "y".repeat(150), // Wraps to 2 visual lines
            "line3".to_string(),
        ];

        state.top_line = 1;
        state.cursor_line = 1; // Absolute line 2
        state.cursor_col = 0;

        let visual_line = calculate_cursor_visual_line(&lines, &state, 80);
        assert_eq!(visual_line, 2); // Line 1 takes 2 visual lines
    }

    // Tests for calculate_visual_lines_to_cursor function
    #[test]
    fn test_visual_lines_to_cursor_at_top() {
        let settings = Settings::default();
        let mut state = create_test_state_for_coords(&settings);
        let lines = vec!["line1".to_string(), "line2".to_string()];

        state.top_line = 0;
        state.cursor_line = 0;

        let visual_lines = calculate_visual_lines_to_cursor(&lines, &state, 80);
        assert_eq!(visual_lines, 1); // Includes the cursor line itself
    }

    #[test]
    fn test_visual_lines_to_cursor_second_line() {
        let settings = Settings::default();
        let mut state = create_test_state_for_coords(&settings);
        let lines = vec!["line1".to_string(), "line2".to_string()];

        state.top_line = 0;
        state.cursor_line = 1;

        let visual_lines = calculate_visual_lines_to_cursor(&lines, &state, 80);
        assert_eq!(visual_lines, 2); // Line 0 + Line 1
    }

    #[test]
    fn test_visual_lines_to_cursor_with_wrapped_lines() {
        let settings = Settings::default();
        let mut state = create_test_state_for_coords(&settings);
        let lines = vec![
            "x".repeat(100), // Takes 2 visual lines
            "line2".to_string(),
        ];

        state.top_line = 0;
        state.cursor_line = 1;

        let visual_lines = calculate_visual_lines_to_cursor(&lines, &state, 80);
        assert_eq!(visual_lines, 3); // 2 for first line + 1 for second
    }

    // Tests for visual_col_to_char_index function
    #[test]
    fn test_visual_col_to_char_no_tabs() {
        let line = "hello world";
        assert_eq!(visual_col_to_char_index(line, 0, 4), 0);
        assert_eq!(visual_col_to_char_index(line, 5, 4), 5);
        assert_eq!(visual_col_to_char_index(line, 11, 4), 11);
    }

    #[test]
    fn test_visual_col_to_char_with_tab() {
        let line = "a\tb"; // 'a' at 0, tab to 4, 'b' at 4
        assert_eq!(visual_col_to_char_index(line, 0, 4), 0); // 'a'
        assert_eq!(visual_col_to_char_index(line, 1, 4), 1); // tab
        assert_eq!(visual_col_to_char_index(line, 4, 4), 2); // 'b'
    }

    #[test]
    fn test_visual_col_to_char_beyond_end() {
        let line = "hello";
        assert_eq!(visual_col_to_char_index(line, 100, 4), 5);
    }

    // Tests for calculate_text_width function
    #[test]
    fn test_calculate_text_width_no_scrollbar() {
        let settings = Settings::default();
        let mut state = create_test_state_for_coords(&settings);
        state.term_width = 80;
        let lines = vec!["line1".to_string()]; // Only 1 line, fits in 20 visible

        let width = calculate_text_width(&state, &lines, 20);
        // 80 - line_number_width (default 4 for 3 digits + separator) - scrollbar (1) = 75
        assert_eq!(width, 75);
    }

    #[test]
    fn test_calculate_text_width_with_scrollbar() {
        let settings = Settings::default();
        let mut state = create_test_state_for_coords(&settings);
        state.term_width = 80;
        let lines = (0..30).map(|i| format!("line{}", i)).collect::<Vec<_>>();

        let width = calculate_text_width(&state, &lines, 20);
        // 80 - line_number_width (4) - scrollbar (1) = 75
        assert_eq!(width, 75);
    }

    #[test]
    fn test_calculate_text_width_no_line_numbers() {
        let mut settings = Settings::default();
        settings.appearance.line_number_digits = 0;
        let mut state = create_test_state_for_coords(&settings);
        state.term_width = 80;
        let lines = vec!["line1".to_string()];

        let width = calculate_text_width(&state, &lines, 20);
        // 80 - 0 (no line numbers) - scrollbar (1) = 79
        assert_eq!(width, 79);
    }

    // Tests for visual_to_logical_position function
    #[test]
    fn test_visual_to_logical_simple() {
        let settings = Settings::default();
        let mut state = create_test_state_for_coords(&settings);
        state.term_width = 80;
        state.top_line = 0;
        let lines = vec!["hello".to_string(), "world".to_string()];

        // Click on first line, column 10 (5 for line number gutter + 5 for text)
        let result = visual_to_logical_position(&state, &lines, 0, 10, 20);
        assert_eq!(result, Some((0, 5)));
    }

    #[test]
    fn test_visual_to_logical_second_line() {
        let settings = Settings::default();
        let mut state = create_test_state_for_coords(&settings);
        state.term_width = 80;
        state.top_line = 0;
        let lines = vec!["hello".to_string(), "world".to_string()];

        // Click on second visual line
        let result = visual_to_logical_position(&state, &lines, 1, 10, 20);
        assert_eq!(result, Some((1, 5)));
    }

    #[test]
    fn test_visual_to_logical_wrapped_line_second_wrap() {
        let settings = Settings::default();
        let mut state = create_test_state_for_coords(&settings);
        state.term_width = 80;
        state.top_line = 0;
        let lines = vec!["x".repeat(100)];

        // Click on second visual line (which is still first logical line)
        // With line_number_width=3, text_width=77
        // Second visual line starts at character 77, click at column 10
        let result = visual_to_logical_position(&state, &lines, 1, 10, 20);
        // Character index should be 77 + (10 - 3) = 84
        assert!(result.is_some());
        let (line, col) = result.unwrap();
        assert_eq!(line, 0); // Still first logical line
        assert!(col >= 77); // Should be in the second wrap segment
    }

    #[test]
    fn test_visual_to_logical_click_on_line_number_returns_none() {
        let settings = Settings::default();
        let mut state = create_test_state_for_coords(&settings);
        state.term_width = 80;
        let lines = vec!["hello".to_string()];

        // Click on line number area (column 0-3)
        let result = visual_to_logical_position(&state, &lines, 0, 2, 20);
        assert_eq!(result, None);
    }

    #[test]
    fn test_visual_to_logical_click_on_scrollbar_returns_none() {
        let settings = Settings::default();
        let mut state = create_test_state_for_coords(&settings);
        state.term_width = 80;
        let lines = (0..30).map(|i| format!("line{}", i)).collect::<Vec<_>>();

        // Click on scrollbar (last column)
        let result = visual_to_logical_position(&state, &lines, 0, 79, 20);
        assert_eq!(result, None);
    }
}
