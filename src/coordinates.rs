use crate::editor_state::FileViewerState;
use crate::settings::Settings;
use unicode_width::UnicodeWidthChar;

/// Unicode character for line wrap indicator (carriage return arrow)
pub const WRAP_INDICATOR: char = '↩';

// ---------------------------------------------------------------------------
// Visual-width helpers
// ---------------------------------------------------------------------------

/// Calculate the visual (terminal column) width of a string.
/// Tabs expand to the next multiple of `tab_width`; wide characters (emoji,
/// CJK) count as 2 columns.
pub fn visual_width(s: &str, tab_width: usize) -> usize {
    let mut width = 0;
    for ch in s.chars() {
        width += char_visual_width(ch, width, tab_width);
    }
    width
}

/// Visual width of a single character given the current column position.
/// Tabs align to the next tab stop; everything else uses its Unicode width.
#[inline]
fn char_visual_width(ch: char, current_col: usize, tab_width: usize) -> usize {
    if ch == '\t' {
        tab_width - (current_col % tab_width)
    } else {
        ch.width().unwrap_or(1)
    }
}

/// Visual width of the first `char_index` characters of `s`.
pub fn visual_width_up_to(s: &str, char_index: usize, tab_width: usize) -> usize {
    let mut width = 0;
    for (i, ch) in s.chars().enumerate() {
        if i >= char_index {
            break;
        }
        width += char_visual_width(ch, width, tab_width);
    }
    width
}

/// Convert a visual column position to the corresponding character index,
/// accounting for tab expansion.
pub fn visual_col_to_char_index(line: &str, visual_col: usize, tab_width: usize) -> usize {
    let mut current_visual = 0;
    for (char_idx, ch) in line.chars().enumerate() {
        if current_visual >= visual_col {
            return char_idx;
        }
        current_visual += char_visual_width(ch, current_visual, tab_width);
    }
    line.chars().count()
}

// ---------------------------------------------------------------------------
// Word-wrap calculation
// ---------------------------------------------------------------------------

/// Calculate break points (character indices) for word-wrapping `line` into
/// segments of at most `text_width` terminal columns (one column is reserved
/// for the wrap indicator `↩`).
///
/// Returns a `Vec` of character indices at which new visual lines begin.
pub(crate) fn calculate_word_wrap_points(line: &str, text_width: usize, tab_width: usize) -> Vec<usize> {
    if text_width == 0 || line.is_empty() {
        return vec![];
    }

    // One column is consumed by the wrap indicator on every wrapped segment.
    let usable_width = text_width.saturating_sub(1);
    // Words longer than half the line width are broken at character boundaries.
    let max_word_length = text_width / 2;

    let chars: Vec<char> = line.chars().collect();
    let mut wrap_points = Vec::new();
    let mut line_start_idx = 0;

    while line_start_idx < chars.len() {
        // How wide is the remaining tail?
        let remaining: String = chars[line_start_idx..].iter().collect();
        if visual_width(&remaining, tab_width) <= usable_width {
            break; // Rest fits on one visual line — done.
        }

        // Scan forward to find where the current segment overflows, tracking
        // the last whitespace position for word-boundary breaking.
        let mut current_visual = 0;
        let mut last_space_idx: Option<usize> = None;
        let mut word_start_idx = line_start_idx;
        let mut in_word = false;
        let mut best_break: Option<usize> = None;

        for i in line_start_idx..chars.len() {
            let ch = chars[i];
            let cw = char_visual_width(ch, current_visual, tab_width);

            if current_visual + cw > usable_width {
                // This character would overflow the line.
                if let Some(space_idx) = last_space_idx {
                    let word_visual = visual_width(
                        &chars[word_start_idx..i].iter().collect::<String>(),
                        tab_width,
                    );
                    if word_visual <= max_word_length && space_idx > line_start_idx {
                        // Break after the space (word-boundary wrap).
                        best_break = Some(space_idx + 1);
                        break;
                    }
                }
                // No suitable word boundary — fall back to character wrap.
                best_break = Some(if i > line_start_idx { i } else { line_start_idx + 1 });
                break;
            }

            current_visual += cw;

            if ch.is_whitespace() {
                last_space_idx = Some(i);
                in_word = false;
            } else if !in_word {
                word_start_idx = i;
                in_word = true;
            }
        }

        match best_break {
            Some(bp) if bp > line_start_idx && bp < chars.len() => {
                wrap_points.push(bp);
                line_start_idx = bp;
            }
            _ => break,
        }
    }

    wrap_points
}

/// Character range `[start, end)` for one wrap segment of `line`.
pub(crate) fn get_wrap_segment_range(
    line: &str,
    wrap_index: usize,
    text_width: usize,
    tab_width: usize,
) -> (usize, usize) {
    let wrap_points = calculate_word_wrap_points(line, text_width, tab_width);
    let line_len = line.chars().count();

    match wrap_index {
        0 => (0, wrap_points.first().copied().unwrap_or(line_len)),
        n if n <= wrap_points.len() => {
            let start = wrap_points[n - 1];
            let end = wrap_points.get(n).copied().unwrap_or(line_len);
            (start, end)
        }
        _ => (line_len, line_len),
    }
}

// ---------------------------------------------------------------------------
// Line-count helpers
// ---------------------------------------------------------------------------

/// How many visual lines does logical line `line_index` occupy?
/// Always ≥ 1 (returns 1 when wrapping is disabled or the line is empty).
pub fn calculate_wrapped_lines_for_line(
    lines: &[String],
    line_index: usize,
    text_width: u16,
    tab_width: usize,
) -> u16 {
    calculate_wrapped_lines_for_line_with_wrapping(lines, line_index, text_width, tab_width, true)
}

pub(crate) fn calculate_wrapped_lines_for_line_with_wrapping(
    lines: &[String],
    line_index: usize,
    text_width: u16,
    tab_width: usize,
    wrapping_enabled: bool,
) -> u16 {
    if line_index >= lines.len() || !wrapping_enabled || text_width == 0 {
        return 1;
    }
    let wrap_points = calculate_word_wrap_points(&lines[line_index], text_width as usize, tab_width);
    (wrap_points.len() + 1) as u16
}

/// Visual line count for one logical line, respecting the wrapping flag.
/// Shared by all the counting functions below.
#[inline]
fn visual_lines_for(
    lines: &[String],
    logical_idx: usize,
    text_width: u16,
    tab_width: usize,
    wrapping_enabled: bool,
) -> usize {
    if wrapping_enabled {
        calculate_wrapped_lines_for_line(lines, logical_idx, text_width, tab_width) as usize
    } else {
        1
    }
}

/// Total visual lines for the whole document (used to decide scrollbar visibility).
pub fn calculate_total_visual_lines(
    lines: &[String],
    state: &FileViewerState,
    text_width: u16,
) -> usize {
    let wrapping = state.is_line_wrapping_enabled();
    let tab_w = state.settings.tab_width;
    (0..lines.len())
        .map(|i| visual_lines_for(lines, i, text_width, tab_w, wrapping))
        .sum()
}

/// Total visual lines before `state.top_line` (used for scrollbar positioning).
pub fn calculate_total_visual_lines_before(
    lines: &[String],
    state: &FileViewerState,
    text_width: u16,
) -> usize {
    let wrapping = state.is_line_wrapping_enabled();
    let tab_w = state.settings.tab_width;
    (0..state.top_line.min(lines.len()))
        .map(|i| visual_lines_for(lines, i, text_width, tab_w, wrapping))
        .sum()
}

/// Total visual lines from `top_line` through the cursor line (inclusive).
pub(crate) fn calculate_visual_lines_to_cursor(
    lines: &[String],
    state: &FileViewerState,
    text_width: u16,
) -> usize {
    let wrapping = state.is_line_wrapping_enabled();
    let tab_w = state.settings.tab_width;
    let end_line = (state.top_line + state.cursor_line + 1).min(lines.len());
    (state.top_line..end_line)
        .map(|i| visual_lines_for(lines, i, text_width, tab_w, wrapping))
        .sum()
}

/// Visual line offset of the cursor within the viewport (0-based).
pub(crate) fn calculate_cursor_visual_line(
    lines: &[String],
    state: &FileViewerState,
    text_width: u16,
) -> usize {
    let wrapping = state.is_line_wrapping_enabled();
    let tab_w = state.settings.tab_width;

    // Visual lines occupied by logical lines above the cursor.
    let mut visual_line: usize = (state.top_line..state.absolute_line())
        .map(|i| visual_lines_for(lines, i, text_width, tab_w, wrapping))
        .sum();

    // Add the intra-line wrap offset (which visual segment is the cursor on?).
    if wrapping && text_width as usize > 0 && state.absolute_line() < lines.len() {
        let line = &lines[state.absolute_line()];
        let wrap_points = calculate_word_wrap_points(line, text_width as usize, tab_w);
        // The cursor is on the first segment whose wrap-point exceeds cursor_col.
        let segment = wrap_points
            .iter()
            .position(|&wp| state.cursor_col < wp)
            .unwrap_or(wrap_points.len());
        visual_line += segment;
    }

    visual_line
}

// ---------------------------------------------------------------------------
// Column / layout helpers
// ---------------------------------------------------------------------------

/// Width of the line-number gutter column (digits + 1 separator space).
/// Returns 0 when line numbers are disabled.
pub(crate) fn line_number_width(settings: &Settings) -> u16 {
    if settings.appearance.line_number_digits == 0 {
        0
    } else {
        settings.appearance.line_number_digits as u16 + 1
    }
}

/// Actual gutter width needed for the current document length.
/// Uses the configured minimum but widens automatically for tall files.
pub(crate) fn line_number_display_width(settings: &Settings, total_lines: usize) -> u16 {
    if settings.appearance.line_number_digits == 0 {
        return 0;
    }
    let digits_needed = if total_lines == 0 {
        1
    } else {
        (total_lines as f64).log10().floor() as usize + 1
    };
    let display_width = digits_needed.max(settings.appearance.line_number_digits as usize);
    (display_width + 1) as u16
}

/// Usable text width: terminal width minus the gutter and the always-visible
/// scrollbar column (reserving the scrollbar prevents text from jumping).
pub fn calculate_text_width(
    state: &FileViewerState,
    _lines: &[String],
    _visible_lines: usize,
) -> u16 {
    state
        .term_width
        .saturating_sub(line_number_width(state.settings))
        .saturating_sub(1) // scrollbar
}

// ---------------------------------------------------------------------------
// Coordinate conversion
// ---------------------------------------------------------------------------

/// Map a `(visual_line, column)` screen position back to a
/// `(logical_line, char_index)` pair.
/// Returns `None` if the click landed on the gutter or the scrollbar.
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

    // Reject clicks on the gutter or the scrollbar column.
    if column < line_num_width || column >= state.term_width.saturating_sub(1) {
        return None;
    }
    let text_col = (column - line_num_width) as usize;

    // In filter mode only the matching (and context) lines are shown.
    if state.filter_active && state.last_search_pattern.is_some() {
        let pattern = state.last_search_pattern.as_ref().unwrap();
        let filtered = crate::find::get_lines_with_matches_and_context(
            lines,
            pattern,
            state.find_regex_mode,
            state.find_scope,
            state.filter_context_before,
            state.filter_context_after,
        );
        // Skip filtered lines that are above the current scroll position.
        let start = filtered.partition_point(|&l| l < state.top_line);
        let mut current_vl = 0;
        for &logical in &filtered[start..] {
            let vl = visual_lines_for(
                lines, logical, text_width, tab_width, state.is_line_wrapping_enabled(),
            );
            if current_vl + vl > visual_line {
                let col = resolve_visual_col(
                    lines, logical, visual_line - current_vl,
                    text_col, text_width as usize, tab_width, state,
                );
                return Some((logical, col));
            }
            current_vl += vl;
        }
        return None;
    }

    // Normal mode: scan logical lines from top_line.
    let wrapping = state.is_line_wrapping_enabled();
    let mut current_vl = 0;
    for logical in state.top_line..lines.len() {
        let vl = visual_lines_for(lines, logical, text_width, tab_width, wrapping);
        if current_vl + vl > visual_line {
            let mut col = resolve_visual_col(
                lines, logical, visual_line - current_vl,
                text_col, text_width as usize, tab_width, state,
            );
            // Clamp to the end of this wrap segment so a click past the last
            // character of a segment doesn't place the cursor on the next line.
            if wrapping {
                let wrap_points =
                    calculate_word_wrap_points(&lines[logical], text_width as usize, tab_width);
                let line_offset = visual_line - current_vl;
                if line_offset < wrap_points.len() && col > wrap_points[line_offset] {
                    col = wrap_points[line_offset];
                }
            }
            return Some((logical, col));
        }
        current_vl += vl;
    }
    None
}

/// Translate a screen `text_col` on wrap-segment `line_offset` of
/// `lines[logical]` into a character index within that line.
fn resolve_visual_col(
    lines: &[String],
    logical: usize,
    line_offset: usize,
    text_col: usize,
    text_width: usize,
    tab_width: usize,
    state: &FileViewerState,
) -> usize {
    let line = &lines[logical];
    let visual_col = if state.is_line_wrapping_enabled() {
        let wrap_points = calculate_word_wrap_points(line, text_width, tab_width);
        if line_offset == 0 || wrap_points.is_empty() {
            text_col
        } else {
            // Offset from the start of this segment's first character.
            let seg_start_char = wrap_points[line_offset - 1];
            visual_width_up_to(line, seg_start_char, tab_width) + text_col
        }
    } else {
        // Horizontal-scroll mode: the screen column is offset by the scroll amount.
        state.horizontal_scroll_offset + text_col
    };
    visual_col_to_char_index(line, visual_col, tab_width)
}

// ---------------------------------------------------------------------------
// Resize helper
// ---------------------------------------------------------------------------

/// Recalculate `(top_line, relative_cursor_line)` after a terminal resize so
/// that the cursor stays visible without unnecessarily jumping to the top.
pub(crate) fn adjust_view_for_resize(
    prev_top_line: usize,
    absolute_cursor_line: usize,
    visible_lines: usize,
    total_lines: usize,
) -> (usize, usize) {
    if total_lines == 0 {
        return (0, 0);
    }
    let vl = visible_lines.max(1);
    let mut new_top = prev_top_line.min(total_lines.saturating_sub(1));

    if absolute_cursor_line < new_top {
        new_top = absolute_cursor_line;
    }
    if absolute_cursor_line >= new_top + vl {
        new_top = absolute_cursor_line.saturating_sub(vl - 1);
    }
    new_top = new_top.min(total_lines.saturating_sub(1));

    let rel_cursor = absolute_cursor_line.saturating_sub(new_top);
    (new_top, rel_cursor)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use crate::undo::UndoHistory;

    /// Create a minimal test state with term_width=80 and default settings.
    fn make_state(settings: &Settings) -> crate::editor_state::FileViewerState<'_> {
        let mut s = crate::editor_state::FileViewerState::new(80, UndoHistory::new(), settings);
        s.term_width = 80;
        s
    }

    // --- adjust_view_for_resize ---

    #[test]
    fn resize_keeps_scroll_when_cursor_visible_and_space_expands() {
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
        assert_eq!(top, 11);
        assert_eq!(rel, 19);
    }

    #[test]
    fn resize_shrink_preserves_cursor_visibility() {
        let (top, rel) = adjust_view_for_resize(50, 65, 10, 120);
        assert_eq!(top, 56);
        assert_eq!(rel, 9);
    }

    #[test]
    fn empty_file_returns_zeroes() {
        let (top, rel) = adjust_view_for_resize(5, 5, 10, 0);
        assert_eq!(top, 0);
        assert_eq!(rel, 0);
    }

    // --- visual_width ---

    #[test]
    fn test_visual_width_empty_string() {
        assert_eq!(visual_width("", 4), 0);
    }

    #[test]
    fn test_visual_width_no_tabs() {
        assert_eq!(visual_width("hello", 4), 5);
    }

    #[test]
    fn test_visual_width_single_tab_at_start() {
        assert_eq!(visual_width("\t", 4), 4);
        assert_eq!(visual_width("\t", 8), 8);
    }

    #[test]
    fn test_visual_width_tab_in_middle() {
        assert_eq!(visual_width("a\tb",   4), 5); // 'a'(1) → tab→4 → 'b'(5)
        assert_eq!(visual_width("ab\tc",  4), 5); // 'ab'(2) → tab→4 → 'c'(5)
        assert_eq!(visual_width("abc\td", 4), 5); // 'abc'(3) → tab→4 → 'd'(5)
    }

    #[test]
    fn test_visual_width_multiple_tabs() {
        assert_eq!(visual_width("\t\t",  4), 8);
        assert_eq!(visual_width("a\t\t", 4), 8);
    }

    #[test]
    fn test_visual_width_tab_width_8() {
        assert_eq!(visual_width("\t",        8), 8);
        assert_eq!(visual_width("a\tb",      8), 9);
        assert_eq!(visual_width("abcdefg\tx", 8), 9);
    }

    #[test]
    fn test_visual_width_mixed_content() {
        // "hello\tworld\t!" with tab_width=4:
        // "hello"(5) → tab→8, "world"(13) → tab→16, "!"(17)
        assert_eq!(visual_width("hello\tworld\t!", 4), 17);
    }

    #[test]
    fn test_visual_width_only_tabs() {
        assert_eq!(visual_width("\t\t\t", 4), 12);
        assert_eq!(visual_width("\t\t\t", 8), 24);
    }

    // --- visual_width_up_to ---

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
        assert_eq!(visual_width_up_to("a\tbc", 1, 4), 1); // before tab
        assert_eq!(visual_width_up_to("a\tbc", 2, 4), 4); // after tab
        assert_eq!(visual_width_up_to("a\tbc", 3, 4), 5); // after 'b'
    }

    #[test]
    fn test_visual_width_up_to_multiple_tabs() {
        assert_eq!(visual_width_up_to("\t\tx", 0, 4), 0);
        assert_eq!(visual_width_up_to("\t\tx", 1, 4), 4);
        assert_eq!(visual_width_up_to("\t\tx", 2, 4), 8);
        assert_eq!(visual_width_up_to("\t\tx", 3, 4), 9);
    }

    #[test]
    fn test_visual_width_up_to_beyond_length() {
        assert_eq!(visual_width_up_to("hi", 10, 4), 2);
        assert_eq!(visual_width_up_to("\t",  10, 4), 4);
    }

    // --- line_number_width ---

    #[test]
    fn test_line_number_width_disabled() {
        let mut settings = Settings::default();
        settings.appearance.line_number_digits = 0;
        assert_eq!(line_number_width(&settings), 0);
    }

    #[test]
    fn test_line_number_width_enabled() {
        let mut settings = Settings::default();
        settings.appearance.line_number_digits = 3;
        assert_eq!(line_number_width(&settings), 4); // 3 + 1 separator

        settings.appearance.line_number_digits = 5;
        assert_eq!(line_number_width(&settings), 6);
    }

    // --- calculate_wrapped_lines_for_line ---

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
        // 80 chars, usable width 79 → wraps into 2 segments
        let lines = vec!["x".repeat(80)];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 80, 4), 2);
    }

    #[test]
    fn test_wrapped_lines_one_over_wraps_to_two() {
        let lines = vec!["x".repeat(81)];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 80, 4), 2);
    }

    #[test]
    fn test_wrapped_lines_double_width() {
        // 160 chars, usable 79 → 3 segments
        let lines = vec!["x".repeat(160)];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 80, 4), 3);
    }

    #[test]
    fn test_wrapped_lines_triple_width() {
        let lines = vec!["x".repeat(200)];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 80, 4), 3);
    }

    #[test]
    fn test_wrapped_lines_with_tabs() {
        // 10 tabs × 4 = 40 visual cols → fits in 80
        let lines = vec!["\t".repeat(10)];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 80, 4), 1);

        // 21 tabs × 4 = 84 visual cols → wraps
        let lines = vec!["\t".repeat(21)];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 80, 4), 2);
    }

    #[test]
    fn test_wrapped_lines_mixed_tabs_and_chars() {
        let lines = vec!["hello\tworld".to_string()]; // 13 visual cols
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 80, 4), 1);
    }

    #[test]
    fn test_wrapped_lines_zero_width_returns_one() {
        let lines = vec!["hello".to_string()];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 0, 4), 1);
    }

    #[test]
    fn test_word_wrapping_with_spaces() {
        let lines = vec!["hello world test".to_string()];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 80, 4), 1);

        let lines = vec!["hello world".to_string()];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 0, 10, 4), 2);
    }

    #[test]
    fn test_word_wrapping_preserves_words() {
        let lines = vec!["short words here".to_string()];
        assert!(calculate_wrapped_lines_for_line(&lines, 0, 20, 4) >= 1);
    }

    #[test]
    fn test_wrapped_lines_beyond_line_count() {
        let lines = vec!["line1".to_string()];
        assert_eq!(calculate_wrapped_lines_for_line(&lines, 5, 80, 4), 1);
    }

    // --- calculate_cursor_visual_line ---

    #[test]
    fn test_cursor_visual_line_at_top_no_wrap() {
        let settings = Settings::default();
        let state = make_state(&settings);
        let lines = vec!["line1".to_string(), "line2".to_string()];
        assert_eq!(calculate_cursor_visual_line(&lines, &state, 80), 0);
    }

    #[test]
    fn test_cursor_visual_line_second_line_no_wrap() {
        let settings = Settings::default();
        let mut state = make_state(&settings);
        let lines = vec!["line1".to_string(), "line2".to_string()];
        state.cursor_line = 1;
        assert_eq!(calculate_cursor_visual_line(&lines, &state, 80), 1);
    }

    #[test]
    fn test_cursor_visual_line_with_wrapped_previous_line() {
        let settings = Settings::default();
        let mut state = make_state(&settings);
        let lines = vec!["x".repeat(100), "line2".to_string()];
        state.cursor_line = 1;
        assert_eq!(calculate_cursor_visual_line(&lines, &state, 80), 2);
    }

    #[test]
    fn test_cursor_visual_line_within_wrapped_line_first_wrap() {
        let settings = Settings::default();
        let mut state = make_state(&settings);
        let lines = vec!["x".repeat(100)];
        state.cursor_col = 50;
        assert_eq!(calculate_cursor_visual_line(&lines, &state, 80), 0);
    }

    #[test]
    fn test_cursor_visual_line_within_wrapped_line_second_wrap() {
        let settings = Settings::default();
        let mut state = make_state(&settings);
        let lines = vec!["x".repeat(100)];
        state.cursor_col = 85;
        assert_eq!(calculate_cursor_visual_line(&lines, &state, 80), 1);
    }

    #[test]
    fn test_cursor_visual_line_after_scrolling() {
        let settings = Settings::default();
        let mut state = make_state(&settings);
        let lines = vec!["line1".to_string(), "line2".to_string(), "line3".to_string()];
        state.top_line = 1;
        state.cursor_line = 1;
        assert_eq!(calculate_cursor_visual_line(&lines, &state, 80), 1);
    }

    #[test]
    fn test_cursor_visual_line_after_scrolling_with_wrapped_lines() {
        let settings = Settings::default();
        let mut state = make_state(&settings);
        let lines = vec!["x".repeat(150), "y".repeat(150), "line3".to_string()];
        state.top_line = 1;
        state.cursor_line = 1;
        assert_eq!(calculate_cursor_visual_line(&lines, &state, 80), 2);
    }

    // --- calculate_visual_lines_to_cursor ---

    #[test]
    fn test_visual_lines_to_cursor_at_top() {
        let settings = Settings::default();
        let state = make_state(&settings);
        let lines = vec!["line1".to_string(), "line2".to_string()];
        assert_eq!(calculate_visual_lines_to_cursor(&lines, &state, 80), 1);
    }

    #[test]
    fn test_visual_lines_to_cursor_second_line() {
        let settings = Settings::default();
        let mut state = make_state(&settings);
        let lines = vec!["line1".to_string(), "line2".to_string()];
        state.cursor_line = 1;
        assert_eq!(calculate_visual_lines_to_cursor(&lines, &state, 80), 2);
    }

    #[test]
    fn test_visual_lines_to_cursor_with_wrapped_lines() {
        let settings = Settings::default();
        let mut state = make_state(&settings);
        let lines = vec!["x".repeat(100), "line2".to_string()];
        state.cursor_line = 1;
        assert_eq!(calculate_visual_lines_to_cursor(&lines, &state, 80), 3);
    }

    // --- visual_col_to_char_index ---

    #[test]
    fn test_visual_col_to_char_no_tabs() {
        let line = "hello world";
        assert_eq!(visual_col_to_char_index(line,  0, 4), 0);
        assert_eq!(visual_col_to_char_index(line,  5, 4), 5);
        assert_eq!(visual_col_to_char_index(line, 11, 4), 11);
    }

    #[test]
    fn test_visual_col_to_char_with_tab() {
        let line = "a\tb"; // 'a'→col0, tab→cols1-3, 'b'→col4
        assert_eq!(visual_col_to_char_index(line, 0, 4), 0);
        assert_eq!(visual_col_to_char_index(line, 1, 4), 1);
        assert_eq!(visual_col_to_char_index(line, 4, 4), 2);
    }

    #[test]
    fn test_visual_col_to_char_beyond_end() {
        assert_eq!(visual_col_to_char_index("hello", 100, 4), 5);
    }

    // --- calculate_text_width ---

    #[test]
    fn test_calculate_text_width_no_scrollbar() {
        let settings = Settings::default();
        let mut state = make_state(&settings);
        state.term_width = 80;
        let lines = vec!["line1".to_string()];
        // 80 - gutter(4: 3 digits + separator) - scrollbar(1) = 75
        assert_eq!(calculate_text_width(&state, &lines, 20), 75);
    }

    #[test]
    fn test_calculate_text_width_with_scrollbar() {
        let settings = Settings::default();
        let mut state = make_state(&settings);
        state.term_width = 80;
        let lines = (0..30).map(|i| format!("line{}", i)).collect::<Vec<_>>();
        assert_eq!(calculate_text_width(&state, &lines, 20), 75);
    }

    #[test]
    fn test_calculate_text_width_no_line_numbers() {
        let mut settings = Settings::default();
        settings.appearance.line_number_digits = 0;
        let mut state = make_state(&settings);
        state.term_width = 80;
        let lines = vec!["line1".to_string()];
        // 80 - 0 - scrollbar(1) = 79
        assert_eq!(calculate_text_width(&state, &lines, 20), 79);
    }

    // --- visual_to_logical_position ---

    #[test]
    fn test_visual_to_logical_simple() {
        let settings = Settings::default();
        let mut state = make_state(&settings);
        state.top_line = 0;
        let lines = vec!["hello".to_string(), "world".to_string()];
        // col 10 - gutter(4) = offset 6, clamped to line length 5
        let result = visual_to_logical_position(&state, &lines, 0, 10, 20);
        assert_eq!(result, Some((0, 5)));
    }

    #[test]
    fn test_visual_to_logical_second_line() {
        let settings = Settings::default();
        let mut state = make_state(&settings);
        state.top_line = 0;
        let lines = vec!["hello".to_string(), "world".to_string()];
        let result = visual_to_logical_position(&state, &lines, 1, 10, 20);
        assert_eq!(result, Some((1, 5)));
    }

    #[test]
    fn test_visual_to_logical_wrapped_line_second_wrap() {
        let settings = Settings::default();
        let mut state = make_state(&settings);
        state.top_line = 0;
        let lines = vec!["x".repeat(100)];
        // Click on the second visual line — still logical line 0.
        let result = visual_to_logical_position(&state, &lines, 1, 10, 20);
        assert!(result.is_some());
        let (line, col) = result.unwrap();
        assert_eq!(line, 0);
        assert!(col >= 77); // into the second wrap segment
    }

    #[test]
    fn test_visual_to_logical_click_on_line_number_returns_none() {
        let settings = Settings::default();
        let state = make_state(&settings);
        let lines = vec!["hello".to_string()];
        assert_eq!(visual_to_logical_position(&state, &lines, 0, 2, 20), None);
    }

    #[test]
    fn test_visual_to_logical_click_on_scrollbar_returns_none() {
        let settings = Settings::default();
        let state = make_state(&settings);
        let lines = (0..30).map(|i| format!("line{}", i)).collect::<Vec<_>>();
        assert_eq!(visual_to_logical_position(&state, &lines, 0, 79, 20), None);
    }
}
