use crate::editor_state::FileViewerState;
use crate::settings::Settings;

/// Calculate the visual width of a string, considering tabs
/// Tabs are expanded to the next multiple of tab_width
pub(crate) fn visual_width(s: &str, tab_width: usize) -> usize {
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

/// Calculate visual width up to a given character index in a string
pub(crate) fn visual_width_up_to(s: &str, char_index: usize, tab_width: usize) -> usize {
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

pub(crate) fn calculate_wrapped_lines_for_line(
    lines: &[String],
    line_index: usize,
    text_width: u16,
    tab_width: usize,
) -> u16 {
    if line_index >= lines.len() {
        return 1;
    }

    let line = &lines[line_index];
    let visual_len = visual_width(line, tab_width);
    let width = text_width as usize;

    if width == 0 {
        return 1;
    }

    // Calculate how many visual lines this logical line needs
    let wrapped_lines = visual_len.div_ceil(width); // Ceiling division
    wrapped_lines.max(1) as u16
}

pub(crate) fn calculate_cursor_visual_line(
    lines: &[String],
    state: &FileViewerState,
    text_width: u16,
) -> usize {
    let mut visual_line = 0;
    let text_width_usize = text_width as usize;
    let tab_width = state.settings.tab_width;

    // Count visual lines from top_line to cursor's logical line
    for i in state.top_line..state.absolute_line() {
        visual_line += calculate_wrapped_lines_for_line(lines, i, text_width, tab_width) as usize;
    }

    // Add the visual line offset within the cursor's logical line itself
    if text_width_usize > 0 && state.absolute_line() < lines.len() {
        let line = &lines[state.absolute_line()];
        let visual_col = visual_width_up_to(line, state.cursor_col, tab_width);
        visual_line += visual_col / text_width_usize;
    }

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

    // Count visual lines from top_line up to and including cursor_line
    let end_line = (state.top_line + state.cursor_line).min(lines.len());
    for i in state.top_line..end_line {
        visual_lines += calculate_wrapped_lines_for_line(lines, i, text_width, tab_width) as usize;
    }

    // Add the wrapped lines for the cursor's current line
    if end_line < lines.len() {
        visual_lines +=
            calculate_wrapped_lines_for_line(lines, end_line, text_width, tab_width) as usize;
    }

    visual_lines
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

    // Convert column to text column (excluding line numbers and scrollbar)
    let scrollbar_width = if lines.len() > visible_lines { 1 } else { 0 };
    let text_start = line_num_width;
    let text_end = state.term_width.saturating_sub(scrollbar_width);

    if column < text_start || column >= text_end {
        return None; // Click was on line number area or scrollbar
    }

    let text_col = (column - line_num_width) as usize;

    // Find which logical line this visual line corresponds to
    let mut current_visual_line = 0;
    let mut logical_line = state.top_line;

    while logical_line < lines.len() {
        let wrapped_lines =
            calculate_wrapped_lines_for_line(lines, logical_line, text_width, tab_width) as usize;

        if current_visual_line + wrapped_lines > visual_line {
            // This is the logical line we're looking for
            let line_offset = visual_line - current_visual_line;
            let visual_col_in_line = line_offset * (text_width as usize) + text_col;

            // Convert visual column to character index considering tabs
            let line = &lines[logical_line];
            let col_in_line = visual_col_to_char_index(line, visual_col_in_line, tab_width);
            return Some((logical_line, col_in_line));
        }

        current_visual_line += wrapped_lines;
        logical_line += 1;
    }

    None
}

/// Convert a visual column position to a character index, considering tabs
pub(crate) fn visual_col_to_char_index(line: &str, visual_col: usize, tab_width: usize) -> usize {
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

/// Calculate the text width available for content, accounting for line numbers and scrollbar
/// Scrollbar takes 1 column when there are more lines than fit on screen
pub(crate) fn calculate_text_width(
    state: &FileViewerState,
    lines: &[String],
    visible_lines: usize,
) -> u16 {
    let line_num_width = line_number_width(state.settings);
    let scrollbar_width = if lines.len() > visible_lines { 1 } else { 0 };
    state
        .term_width
        .saturating_sub(line_num_width)
        .saturating_sub(scrollbar_width)
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
}
