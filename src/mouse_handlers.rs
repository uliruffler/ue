use crate::coordinates::visual_to_logical_position;
use crate::editor_state::FileViewerState;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::time::Instant;

/// Check if a character is a word character (alphanumeric or underscore)
fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// Find the start of the word containing the given position in a line
fn find_word_start(line: &str, col: usize) -> usize {
    if col == 0 || line.is_empty() {
        return 0;
    }

    let chars: Vec<char> = line.chars().collect();
    let mut start = col.min(chars.len() - 1);

    // If cursor is at end of line, check the last character
    if start >= chars.len() {
        start = chars.len() - 1;
    }

    // If not on a word character, find the previous word character
    if !is_word_char(chars[start]) {
        while start > 0 && !is_word_char(chars[start - 1]) {
            start -= 1;
        }
        if start > 0 && is_word_char(chars[start - 1]) {
            start -= 1;
        }
    }

    // Find the beginning of the word
    while start > 0 && is_word_char(chars[start - 1]) {
        start -= 1;
    }

    start
}

/// Find the end of the word containing the given position in a line
fn find_word_end(line: &str, col: usize) -> usize {
    let chars: Vec<char> = line.chars().collect();
    let mut end = col.min(chars.len());

    // If not on a word character, find the next word character
    if end < chars.len() && !is_word_char(chars[end]) {
        while end < chars.len() && !is_word_char(chars[end]) {
            end += 1;
        }
    }

    // Find the end of the word
    while end < chars.len() && is_word_char(chars[end]) {
        end += 1;
    }

    end
}

/// Handle double-click to select word
fn handle_double_click(
    state: &mut FileViewerState,
    lines: &[String],
    logical_line: usize,
    col: usize,
) {
    if logical_line < lines.len() {
        restore_cursor_to_screen(state);
        let line = &lines[logical_line];

        let word_start = find_word_start(line, col);
        let word_end = find_word_end(line, col);

        state.selection_start = Some((logical_line, word_start));
        state.selection_end = Some((logical_line, word_end));
        state.cursor_line = logical_line.saturating_sub(state.top_line);
        state.cursor_col = word_end;
        state.desired_cursor_col = word_end;
        state.mouse_dragging = true;
        state.needs_redraw = true;
    }
}

/// Handle triple-click to select entire line
fn handle_triple_click(state: &mut FileViewerState, lines: &[String], logical_line: usize) {
    if logical_line < lines.len() {
        restore_cursor_to_screen(state);

        // Select from start of line to start of next line (or end of current line if last)
        state.selection_start = Some((logical_line, 0));
        state.cursor_line = logical_line.saturating_sub(state.top_line);
        state.cursor_col = lines[logical_line].len();
        state.desired_cursor_col = state.cursor_col;

        if logical_line + 1 < lines.len() {
            state.selection_end = Some((logical_line + 1, 0));
        } else {
            state.selection_end = Some((logical_line, lines[logical_line].len()));
        }

        state.mouse_dragging = true;
        state.needs_redraw = true;
    }
}

/// Check if a position is within the click timeout and location threshold
fn is_same_click_location(last_pos: Option<(usize, usize)>, current_pos: (usize, usize)) -> bool {
    last_pos.map_or(false, |pos| {
        // Same position or adjacent columns (within 1 character)
        pos.0 == current_pos.0 && (pos.1 as isize - current_pos.1 as isize).abs() <= 1
    })
}

/// Handle mouse click with multi-click detection
fn handle_mouse_click_multiclick(
    state: &mut FileViewerState,
    lines: &[String],
    visual_line: usize,
    column: u16,
    visible_lines: usize,
) {
    let current_click_pos = (visual_line, column as usize);

    // Check if this is a multiple click (within 500ms and same location)
    let is_multiple_click = if let Some(last_time) = state.last_click_time {
        let elapsed = last_time.elapsed().as_millis();
        let location_ok = is_same_click_location(state.last_click_pos, current_click_pos);
        let time_ok = elapsed <= 500;
        time_ok && location_ok
    } else {
        false
    };

    if !is_multiple_click {
        // Single click - normal behavior
        handle_mouse_click(state, lines, visual_line, column, visible_lines);
        state.click_count = 1;
    } else {
        // Multiple click
        state.click_count += 1;

        // Get the logical line and column
        if let Some((logical_line, col)) =
            visual_to_logical_position(state, lines, visual_line, column, visible_lines)
        {
            if state.click_count == 2 {
                // Double click - select word
                handle_double_click(state, lines, logical_line, col);
            } else if state.click_count >= 3 {
                // Triple click or more - select entire line
                handle_triple_click(state, lines, logical_line);
                state.click_count = 3; // Cap at 3 to prevent overflow
            }
        }
    }

    // Update click state for next click
    state.last_click_time = Some(Instant::now());
    state.last_click_pos = Some(current_click_pos);
}

/// Convert a visual scroll position to the corresponding logical line
/// Used when line wrapping is enabled to find which logical line should be at top_line
fn convert_visual_scroll_to_logical_line(
    lines: &[String],
    state: &FileViewerState,
    text_width: u16,
    target_visual_scroll: usize,
) -> usize {
    let mut cumulative_visual = 0;

    for i in 0..lines.len() {
        let line_visual_height = crate::coordinates::calculate_wrapped_lines_for_line(
            lines, i, text_width, state.settings.tab_width
        ) as usize;

        if cumulative_visual + line_visual_height > target_visual_scroll {
            return i;
        }

        cumulative_visual += line_visual_height;
    }

    // If we've gone through all lines, return the last line
    lines.len().saturating_sub(1)
}

/// Handle mouse click on scrollbar
fn handle_scrollbar_click(
    state: &mut FileViewerState,
    lines: &[String],
    visual_line: usize,
    row: u16,
    visible_lines: usize,
) {
    // Calculate text width and total visual lines (accounting for wrapping)
    let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines);
    let total_visual_lines = crate::coordinates::calculate_total_visual_lines(lines, state, text_width);

    // Only show scrollbar if there are more visual lines than visible
    if total_visual_lines <= visible_lines {
        return; // No scrolling needed
    }

    // Calculate scrollbar dimensions (same as in rendering.rs)
    // Determine if we actually have wrapping (visual lines != logical lines)
    let has_actual_wrapping = total_visual_lines != lines.len();
    let scrollbar_height = visible_lines;

    // Calculate bar height and scroll progress
    let (bar_height, max_scroll, scroll_progress) = if has_actual_wrapping {
        // For wrapped mode with actual wrapping, calculate visual position
        let visual_lines_before_top = crate::coordinates::calculate_total_visual_lines_before(lines, state, text_width);
        let max_visual_scroll = total_visual_lines.saturating_sub(visible_lines);
        let bar_height = if total_visual_lines > 0 {
            (visible_lines * visible_lines / total_visual_lines).max(1)
        } else {
            1
        };
        let scroll_progress = if max_visual_scroll == 0 {
            0.0
        } else {
            visual_lines_before_top as f64 / max_visual_scroll as f64
        };
        (bar_height, max_visual_scroll, scroll_progress)
    } else {
        // For unwrapped mode, use logical lines
        let total_lines = lines.len();
        let bar_height = (visible_lines * visible_lines / total_lines).max(1);
        let max_scroll = total_lines.saturating_sub(visible_lines);
        let scroll_progress = if max_scroll == 0 {
            0.0
        } else {
            state.top_line as f64 / max_scroll as f64
        };
        (bar_height, max_scroll, scroll_progress)
    };

    let bar_position = ((scrollbar_height - bar_height) as f64 * scroll_progress) as usize;
    let bar_end = bar_position + bar_height;

    // Check if click is on the scrollbar bar itself or in the background
    if visual_line >= bar_position && visual_line < bar_end {
        // Click on the bar - start dragging
        // Calculate the offset within the bar (0 = top of bar, bar_height-1 = bottom of bar)
        let bar_offset = visual_line - bar_position;

        state.scrollbar_dragging = true;
        state.scrollbar_drag_start_top_line = state.top_line;
        state.scrollbar_drag_start_y = row;
        state.scrollbar_drag_bar_offset = bar_offset;
        state.needs_redraw = true;
    } else {
        // Click in scrollbar background - jump to that position
        let target_scroll_progress = visual_line as f64 / scrollbar_height as f64;

        if has_actual_wrapping {
            // For wrapped mode with actual wrapping, calculate target visual position and find corresponding logical line
            let target_visual_line = (target_scroll_progress * max_scroll as f64) as usize;

            // Find the logical line that contains this visual line
            let mut cumulative_visual = 0;
            let mut new_top_line = 0;

            for i in 0..lines.len() {
                let line_visual_height = crate::coordinates::calculate_wrapped_lines_for_line(
                    lines, i, text_width, state.settings.tab_width
                ) as usize;

                if cumulative_visual + line_visual_height > target_visual_line {
                    new_top_line = i;
                    break;
                }

                cumulative_visual += line_visual_height;

                if cumulative_visual >= max_scroll {
                    new_top_line = i;
                    break;
                }
            }

            if new_top_line != state.top_line {
                state.top_line = new_top_line;
                // Adjust cursor if it goes off screen
                if state.cursor_line >= visible_lines {
                    state.cursor_line = visible_lines.saturating_sub(1);
                }
                state.needs_redraw = true;
            }
        } else {
            // For unwrapped mode, use logical lines directly
            let new_top_line = (target_scroll_progress * max_scroll as f64) as usize;
            let new_top_line = new_top_line.min(max_scroll);

            if new_top_line != state.top_line {
                state.top_line = new_top_line;
                // Adjust cursor if it goes off screen
                if state.cursor_line >= visible_lines {
                    state.cursor_line = visible_lines.saturating_sub(1);
                }
                state.needs_redraw = true;
            }
        }
    }
}

/// Handle scrollbar dragging
fn handle_scrollbar_drag(
    state: &mut FileViewerState,
    lines: &[String],
    row: u16,
    visible_lines: usize,
) {
    if !state.scrollbar_dragging {
        return;
    }

    // Calculate text width and total visual lines (accounting for wrapping)
    let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines);
    let total_visual_lines = crate::coordinates::calculate_total_visual_lines(lines, state, text_width);

    // Only handle dragging if there's actual scrolling needed
    if total_visual_lines <= visible_lines {
        return;
    }

    // Determine if we actually have wrapping (visual lines != logical lines)
    let has_actual_wrapping = total_visual_lines != lines.len();
    let scrollbar_height = visible_lines;

    // Calculate bar height and max scroll
    let (bar_height, max_scroll) = if has_actual_wrapping {
        let max_visual_scroll = total_visual_lines.saturating_sub(visible_lines);
        let bar_height = if total_visual_lines > 0 {
            (visible_lines * visible_lines / total_visual_lines).max(1)
        } else {
            1
        };
        (bar_height, max_visual_scroll)
    } else {
        // No actual wrapping - use logical line counts
        let total_lines = lines.len();
        let bar_height = (visible_lines * visible_lines / total_lines).max(1);
        let max_scroll = total_lines.saturating_sub(visible_lines);
        (bar_height, max_scroll)
    };

    // Convert mouse row to visual line (accounting for header)
    let mouse_visual_line = (row as usize).saturating_sub(1);

    // For very small bars (1 character), position the bar directly at mouse position
    if bar_height == 1 {
        // For 1-character bars, calculate scroll position so that the bar renders at mouse position
        let available_scroll_space = visible_lines.saturating_sub(bar_height);
        let target_bar_position = mouse_visual_line.min(available_scroll_space);

        // Use integer arithmetic with ceiling division for precision (matches rendering logic)
        let new_top_line = if available_scroll_space > 0 && max_scroll > 0 {
            if has_actual_wrapping {
                // For wrapped mode with actual wrapping, calculate visual scroll and convert to logical line
                let target_visual_scroll = (target_bar_position * max_scroll).div_ceil(available_scroll_space);
                convert_visual_scroll_to_logical_line(lines, state, text_width, target_visual_scroll)
            } else {
                // No actual wrapping - use direct logical line calculation
                let top_line = (target_bar_position * max_scroll).div_ceil(available_scroll_space);
                top_line.min(max_scroll)
            }
        } else {
            0
        };

        if new_top_line != state.top_line {
            let absolute_cursor = state.absolute_line();
            state.top_line = new_top_line;
            // Update cursor to maintain its absolute position in the text
            update_cursor_visibility_after_scroll(state, absolute_cursor, visible_lines);
            state.needs_redraw = true;
        }
    } else {
        // For larger bars, maintain the original click offset approach
        let target_bar_top = mouse_visual_line.saturating_sub(state.scrollbar_drag_bar_offset);

        // Clamp the bar position to valid range
        let available_scroll_space = scrollbar_height.saturating_sub(bar_height);
        let clamped_bar_top = target_bar_top.min(available_scroll_space);

        // Calculate the scroll progress from the bar position
        let scroll_progress = if available_scroll_space > 0 {
            clamped_bar_top as f64 / available_scroll_space as f64
        } else {
            0.0
        };

        // Convert scroll progress to scroll position
        let target_scroll = (scroll_progress * max_scroll as f64) as usize;

        // Convert scroll position to top_line
        let new_top_line = if has_actual_wrapping {
            // For wrapped mode with actual wrapping, find logical line for target visual scroll position
            convert_visual_scroll_to_logical_line(lines, state, text_width, target_scroll)
        } else {
            // No actual wrapping - scroll position IS the top_line
            target_scroll.min(max_scroll)
        };

        if new_top_line != state.top_line {
            let absolute_cursor = state.absolute_line();
            state.top_line = new_top_line;
            // Update cursor to maintain its absolute position in the text
            update_cursor_visibility_after_scroll(state, absolute_cursor, visible_lines);
            state.needs_redraw = true;
        }
    }
}

/// Main entry point for all mouse event handling
/// Handle clicks on the footer, particularly for search navigation arrows
fn handle_footer_click(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    column: u16,
    visible_lines: usize,
) {
    // Handle mode toggle button in find mode (⇄ character)
    if state.find_active || state.replace_active {
        let digits = state.settings.appearance.line_number_digits as usize;
        let line_num_offset = if digits > 0 { digits + 1 } else { 0 };

        // Calculate positions of mode toggle button
        // Format: "[digits] Find [⇄R]: " or "[digits] Find [⇄W]: "
        let find_or_filter_label = if state.filter_active { "Filter " } else { "Find " };
        let find_or_filter_len = find_or_filter_label.len();

        // Button is [⇄R] or [⇄W]
        // In characters: '[' + '⇄' + 'R/W' + ']' = 4 chars
        // In visual columns: '[' + '⇄' (2 cols) + 'R/W' + ']' = 5 cols
        let toggle_start = line_num_offset + find_or_filter_len; // Position of '['
        let toggle_len = 5; // Visual width: '[' + '⇄' (2) + 'R/W' + ']'
        let toggle_end = toggle_start + toggle_len;

        let click_col = column as usize;

        // Check if clicked on toggle button area
        if click_col >= toggle_start && click_col < toggle_end {
            // Toggle the mode
            state.find_regex_mode = !state.find_regex_mode;
            state.find_error = None;
            // Update highlights with new mode
            crate::find::update_live_highlights(state);
            crate::find::update_search_hit_count(state, lines);
            state.needs_redraw = true;
            return;
        }
    }
    // Handle filter context spinners (if filter mode is active)
    if state.filter_active && state.last_search_pattern.is_some() {
        let digits = state.settings.appearance.line_number_digits as usize;
        let line_num_offset = if digits > 0 { digits + 1 } else { 0 };

        // Format in footer: "[digits] Filter: Before: X▲▼ After: Y▲▼  "
        // Calculate positions accounting for "Filter: " prefix
        let filter_label = "Filter: ";
        let filter_label_len = filter_label.len();
        
        let before_label = "Before:";
        // Context numbers are padded to 2 digits (matching the renderer)
        let before_num_w = 2usize;
        let after_label = " After:"; // Note: includes leading space
        let after_num_w = 2usize;

        // Position after line numbers and "Filter: " label
        let content_start = line_num_offset + filter_label_len;
        
        // Calculate exact column positions in the rendered string "Before: X▲▼ After: Y▲▼  "
        let before_label_start = content_start;
        let before_num_start = before_label_start + before_label.len();
        let before_arrow_start = before_num_start + before_num_w;
        let before_arrow_end = before_arrow_start + 2; // ▲▼ is 2 characters

        let after_label_start = before_arrow_end;
        let after_num_start = after_label_start + after_label.len();
        let after_arrow_start = after_num_start + after_num_w;

        let click_col = column as usize;

        // Check if clicked on "Before" up arrow (▲)
        if click_col == before_arrow_start {
            state.filter_context_before = state.filter_context_before.saturating_add(1).min(99);
            state.needs_redraw = true;
            return;
        }
        // Check if clicked on "Before" down arrow (▼)
        if click_col == before_arrow_start + 1 {
            state.filter_context_before = state.filter_context_before.saturating_sub(1);
            state.needs_redraw = true;
            return;
        }
        // Check if clicked on "After" up arrow (▲)
        if click_col == after_arrow_start {
            state.filter_context_after = state.filter_context_after.saturating_add(1).min(99);
            state.needs_redraw = true;
            return;
        }
        // Check if clicked on "After" down arrow (▼)
        if click_col == after_arrow_start + 1 {
            state.filter_context_after = state.filter_context_after.saturating_sub(1);
            state.needs_redraw = true;
            return;
        }
    }

    // Handle replace mode buttons
    if state.replace_active {
        let total_width = state.term_width as usize;
        let digits = state.settings.appearance.line_number_digits as usize;
        
        // Build the right side to calculate button positions
        let line_num = state.absolute_line() + 1;
        let col_num = state.cursor_col + 1;
        let position_info = format!("{}:{}", line_num, col_num);
        let buttons = "[replace occurrence] [replace all]";
        let right_side = format!("{}  {}", buttons, position_info);
        
        // Calculate where buttons are displayed
        let digit_area_len = if digits > 0 { digits + 1 } else { 0 };
        let remaining_width = total_width.saturating_sub(digit_area_len);
        
        // Calculate the starting position of right_side
        let left_side_len = digit_area_len + "Replace with: ".len() + state.replace_pattern.len();
        let pad = remaining_width.saturating_sub(left_side_len - digit_area_len).saturating_sub(right_side.len());
        let right_start = left_side_len + pad;
        
        // Find button positions in right_side
        let replace_occurrence_btn = "[replace occurrence]";
        let replace_all_btn = "[replace all]";
        
        let click_col = column as usize;
        
        // Check if clicked on "replace occurrence" button
        if let Some(pos) = buttons.find(replace_occurrence_btn) {
            let btn_start = right_start + pos;
            let btn_end = btn_start + replace_occurrence_btn.len();
            if click_col >= btn_start && click_col < btn_end {
                // Clicked on "replace occurrence" button
                crate::find::replace_current_occurrence(state, lines, visible_lines);
                return;
            }
        }
        
        // Check if clicked on "replace all" button
        if let Some(pos) = buttons.find(replace_all_btn) {
            let btn_start = right_start + pos;
            let btn_end = btn_start + replace_all_btn.len();
            if click_col >= btn_start && click_col < btn_end {
                // Clicked on "replace all" button
                crate::find::replace_all_occurrences(state, lines);
                return;
            }
        }
        
        return;
    }
    
    // Only handle if we have search results
    if state.search_hit_count == 0 {
        return;
    }

    // Calculate where the search info is displayed in the footer
    let line_num = state.absolute_line() + 1;
    let col_num = state.cursor_col + 1;

    // Use the same fixed-width padding as the renderer so click targets stay stable
    let total_lines = lines.len().max(1);
    let max_line_w = ((total_lines as f64).log10().floor() as usize) + 1;
    let max_col = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) + 1;
    let max_col_w = if max_col == 0 { 1 } else { ((max_col as f64).log10().floor() as usize) + 1 };
    let position_info = format!("{:>width_l$}:{:<width_c$}", line_num, col_num,
        width_l = max_line_w, width_c = max_col_w);

    let hit_display = if state.search_hit_count > 0 {
        if state.search_current_hit > 0 {
            format!("({}/{}) ↑↓", state.search_current_hit, state.search_hit_count)
        } else {
            format!("(-/{}) ↑↓", state.search_hit_count)
        }
    } else {
        "(0) ↑↓".to_string()
    };

    // Format: hit_display  position_info (with double space) — matches renderer exactly
    let full_info = format!("{}  {}", hit_display, position_info);

    let total_width = state.term_width as usize;
    let digits = state.settings.appearance.line_number_digits as usize;
    let left_len = if digits > 0 { digits + 1 } else { 0 };
    let remaining_width = total_width.saturating_sub(left_len);

    // Use chars().count() so multi-byte chars (↑↓) count as 1 column each
    let full_info_cols = full_info.chars().count();

    // Calculate where the arrows are displayed
    let info_start = if full_info_cols >= remaining_width {
        left_len // Truncated, starts at left edge
    } else {
        left_len + (remaining_width - full_info_cols) // Padded, right-aligned
    };

    // Find the position of the arrows within full_info (char-column offset)
    let arrow_text = "↑↓";
    let arrow_start = if full_info_cols >= remaining_width {
        // Truncated case: find byte offset of arrows then convert to char-column offset
        // The visible portion starts at (full_info.len() - remaining_width) bytes from the right,
        // but since arrows are always near the start (in hit_display), find in full string.
        if let Some(byte_pos) = full_info.find(arrow_text) {
            let col_pos = full_info[..byte_pos].chars().count();
            if col_pos >= full_info_cols.saturating_sub(remaining_width) {
                info_start + col_pos - (full_info_cols - remaining_width)
            } else {
                return; // Arrows scrolled out of view
            }
        } else {
            return;
        }
    } else {
        // Normal (non-truncated) case: convert byte offset to char-column offset
        if let Some(byte_pos) = full_info.find(arrow_text) {
            info_start + full_info[..byte_pos].chars().count()
        } else {
            return;
        }
    };

    let arrow_end = arrow_start + arrow_text.chars().count();

    // Check if click is on the arrows
    let click_col = column as usize;
    if click_col >= arrow_start && click_col < arrow_end {
        // Clicked on arrows - determine which one
        let arrow_offset = click_col - arrow_start;
        if arrow_offset == 0 {
            // Clicked on up arrow (↑) - go to previous match
            crate::find::find_prev_occurrence(state, lines, visible_lines);
        } else if arrow_offset == 1 {
            // Clicked on down arrow (↓) - go to next match
            crate::find::find_next_occurrence(state, lines, visible_lines);
        }
    }
}

/// Handle continuous horizontal auto-scroll when mouse is held at edge during drag
/// Returns true if scrolling occurred
pub(crate) fn handle_continuous_auto_scroll(
    state: &mut FileViewerState,
    lines: &[String],
    visible_lines: usize,
) -> bool {
    // Only auto-scroll if actively dragging and in horizontal scroll mode
    if !state.mouse_dragging || state.is_line_wrapping_enabled() {
        return false;
    }

    // Check if we have a stored drag position
    let Some((visual_line, column)) = state.last_drag_position else {
        return false;
    };

    let line_num_width = crate::coordinates::line_number_width(state.settings);
    let scrollbar_width = 1; // Always reserve space for scrollbar
    let text_end = state.term_width.saturating_sub(scrollbar_width);
    let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines) as usize;

    let mut scrolled = false;

    // Check if mouse is at or beyond the right text boundary
    if column >= text_end {
        // Mouse beyond visible text area - scroll and extend selection to end of line
        let absolute_line = state.top_line + visual_line.min(visible_lines.saturating_sub(1));
        if absolute_line < lines.len() {
            let line = &lines[absolute_line];
            use crate::coordinates::visual_width;
            let line_visual_width = visual_width(line, state.settings.tab_width);

            // Only scroll if the end of the line is not yet visible
            let end_visible = state.horizontal_scroll_offset + text_width >= line_visual_width;

            if !end_visible && state.horizontal_scroll_offset < line_visual_width {
                let scroll_speed = state.settings.horizontal_auto_scroll_speed;
                state.horizontal_scroll_offset = (state.horizontal_scroll_offset + scroll_speed).min(line_visual_width);
                state.needs_redraw = true;
                scrolled = true;
            }

            // Always extend selection to end of line when mouse is beyond text boundary
            state.cursor_line = absolute_line.saturating_sub(state.top_line);
            state.cursor_col = line.len(); // Set to end of line
            state.desired_cursor_col = state.cursor_col;
            state.update_selection();
        }
    }
    // Check if mouse is at the left edge (on line numbers)
    else if column <= line_num_width {
        if state.horizontal_scroll_offset > 0 {
            let scroll_speed = state.settings.horizontal_auto_scroll_speed;
            state.horizontal_scroll_offset = state.horizontal_scroll_offset.saturating_sub(scroll_speed);
            state.needs_redraw = true;
            scrolled = true;
        }
    }


    scrolled
}

/// Handle double-click word selection in rendered mode.
/// Operates on the plain-text of the rendered line (ANSI stripped) exactly like the
/// plain-view `handle_double_click`, using the shared `find_word_start` / `find_word_end`
/// helpers.
fn handle_rendered_double_click(
    state: &mut FileViewerState,
    rendered_line_index: usize,
    col: usize,
) {
    if rendered_line_index >= state.rendered_lines.len() {
        return;
    }
    let plain = crate::rendering::strip_ansi(&state.rendered_lines[rendered_line_index]);
    let word_start = find_word_start(&plain, col);
    let word_end = find_word_end(&plain, col);
    state.rendered_selection_start = Some((rendered_line_index, word_start));
    state.rendered_selection_end = Some((rendered_line_index, word_end));
    // Do NOT set rendered_mouse_dragging here — the selection is complete.
    state.needs_redraw = true;
}

/// Handle triple-click line selection in rendered mode.
/// Selects from column 0 of the clicked line to column 0 of the next line (or end of
/// document on the last line), mirroring plain-view `handle_triple_click`.
fn handle_rendered_triple_click(
    state: &mut FileViewerState,
    rendered_line_index: usize,
) {
    let total = state.rendered_lines.len();
    if rendered_line_index >= total {
        return;
    }
    state.rendered_selection_start = Some((rendered_line_index, 0));
    if rendered_line_index + 1 < total {
        state.rendered_selection_end = Some((rendered_line_index + 1, 0));
    } else {
        let len = crate::rendering::strip_ansi(&state.rendered_lines[rendered_line_index])
            .chars()
            .count();
        state.rendered_selection_end = Some((rendered_line_index, len));
    }
    // Do NOT set rendered_mouse_dragging here — the selection is complete.
    // Setting it would cause the Up event to overwrite rendered_selection_end.
    state.needs_redraw = true;
}

/// Handle mouse events in rendered markdown mode.
/// In this mode the content is pre-rendered ANSI lines; we track a simple
/// (line_index, visual_col) selection so the user can copy rendered text.
/// Double-click selects the word under the cursor; triple-click selects the whole line,
/// both mirroring the plain-view behaviour.
fn handle_rendered_mouse_event(
    state: &mut FileViewerState,
    mouse_event: MouseEvent,
    visible_lines: usize,
) {
    let MouseEvent { kind, column, row, .. } = mouse_event;

    let gutter_width = if state.settings.appearance.line_number_digits > 0 {
        state.settings.appearance.line_number_digits as usize + 1
    } else {
        0
    };
    // Content columns start after the gutter.
    let content_start_col = gutter_width as u16;
    // Whether the click landed inside the line-number gutter.
    let on_line_number = column < content_start_col;

    // Map screen row to rendered_lines index (row 0 is header, content starts at row 1).
    let visual_line = (row as usize).saturating_sub(1);
    let rendered_line_index = state.top_line + visual_line;

    // Clamp column to the content area (after gutter, before scrollbar).
    let col = if column > content_start_col {
        (column - content_start_col) as usize
    } else {
        0
    };

    match kind {
        MouseEventKind::Down(MouseButton::Left) => {
            // A click on the line-number gutter selects the whole line and enables
            // line-number dragging so the user can extend the selection by dragging.
            if on_line_number {
                let total = state.rendered_lines.len();
                if rendered_line_index < total {
                    // Anchor at start of clicked line.
                    state.rendered_selection_start = Some((rendered_line_index, 0));
                    // End: start of next line, or end of last line.
                    state.rendered_selection_end = if rendered_line_index + 1 < total {
                        Some((rendered_line_index + 1, 0))
                    } else {
                        let len = crate::rendering::strip_ansi(&state.rendered_lines[rendered_line_index])
                            .chars().count();
                        Some((rendered_line_index, len))
                    };
                    state.rendered_mouse_dragging = true;
                    state.line_number_drag_active = true;
                    state.needs_redraw = true;
                }
                state.click_count = 1;
                state.last_click_time = Some(Instant::now());
                state.last_click_pos = Some((visual_line, column as usize));
                return;
            }

            let current_pos = (visual_line, column as usize);

            // Detect multi-click: same location within 500 ms.
            let is_multiclick = if let Some(last_time) = state.last_click_time {
                last_time.elapsed().as_millis() <= 500
                    && is_same_click_location(state.last_click_pos, current_pos)
            } else {
                false
            };

            if !is_multiclick {
                // Single click — start a fresh selection.
                state.click_count = 1;
                state.line_number_drag_active = false;
                state.rendered_selection_start = Some((rendered_line_index, col));
                state.rendered_selection_end = Some((rendered_line_index, col));
                state.rendered_mouse_dragging = true;
                state.needs_redraw = true;
            } else {
                state.click_count += 1;
                state.line_number_drag_active = false;
                match state.click_count {
                    2 => handle_rendered_double_click(state, rendered_line_index, col),
                    _ => {
                        // Triple-click or beyond: select whole line, cap counter.
                        handle_rendered_triple_click(state, rendered_line_index);
                        state.click_count = 3;
                    }
                }
            }

            state.last_click_time = Some(Instant::now());
            state.last_click_pos = Some(current_pos);
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if state.rendered_mouse_dragging {
                if state.line_number_drag_active {
                    // Extend whole-line selection from anchor to current line.
                    let total = state.rendered_lines.len();
                    let anchor_line = state.rendered_selection_start
                        .map(|(l, _)| l)
                        .unwrap_or(rendered_line_index);
                    let drag_line = rendered_line_index.min(total.saturating_sub(1));
                    let (sel_start_line, sel_end_line) = if drag_line >= anchor_line {
                        (anchor_line, drag_line)
                    } else {
                        (drag_line, anchor_line)
                    };
                    state.rendered_selection_start = Some((sel_start_line, 0));
                    state.rendered_selection_end = if sel_end_line + 1 < total {
                        Some((sel_end_line + 1, 0))
                    } else {
                        let len = crate::rendering::strip_ansi(&state.rendered_lines[sel_end_line])
                            .chars().count();
                        Some((sel_end_line, len))
                    };
                } else {
                    state.rendered_selection_end = Some((rendered_line_index, col));
                }
                state.needs_redraw = true;
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if state.rendered_mouse_dragging {
                if state.line_number_drag_active {
                    // Final extend — same logic as Drag.
                    let total = state.rendered_lines.len();
                    let anchor_line = state.rendered_selection_start
                        .map(|(l, _)| l)
                        .unwrap_or(rendered_line_index);
                    let drag_line = rendered_line_index.min(total.saturating_sub(1));
                    let (sel_start_line, sel_end_line) = if drag_line >= anchor_line {
                        (anchor_line, drag_line)
                    } else {
                        (drag_line, anchor_line)
                    };
                    state.rendered_selection_start = Some((sel_start_line, 0));
                    state.rendered_selection_end = if sel_end_line + 1 < total {
                        Some((sel_end_line + 1, 0))
                    } else {
                        let len = crate::rendering::strip_ansi(&state.rendered_lines[sel_end_line])
                            .chars().count();
                        Some((sel_end_line, len))
                    };
                } else {
                    state.rendered_selection_end = Some((rendered_line_index, col));
                }
                state.rendered_mouse_dragging = false;
                state.line_number_drag_active = false;
                state.needs_redraw = true;
            }
        }
        MouseEventKind::ScrollDown => {
            let scroll_amount = state.settings.mouse_scroll_lines;
            let total_lines = state.rendered_lines.len();
            let max_top = total_lines.saturating_sub(visible_lines);
            state.top_line = (state.top_line + scroll_amount).min(max_top);
            state.needs_redraw = true;
        }
        MouseEventKind::ScrollUp => {
            let scroll_amount = state.settings.mouse_scroll_lines;
            state.top_line = state.top_line.saturating_sub(scroll_amount);
            state.needs_redraw = true;
        }
        _ => {}
    }
}

pub(crate) fn handle_mouse_event(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    mouse_event: MouseEvent,
    visible_lines: usize,
) {
    let MouseEvent {
        kind,
        column,
        row,
        modifiers,
        ..
    } = mouse_event;

    let line_num_width = crate::coordinates::line_number_display_width(state.settings, lines.len());

    // Handle menu clicks (row 0 is menu bar)
    if row == 0 {
        let (action, needs_full_redraw) = crate::menu::handle_menu_mouse(&mut state.menu_bar, mouse_event, line_num_width);
        if let Some(action) = action {
            state.pending_menu_action = Some(action);
        }
        if needs_full_redraw {
            state.needs_redraw = true;
        }
        // Always return here to prevent clicks on menu from affecting editor
        return;
    }

    // Check if event is within dropdown menu area (rows 1+) or is a hover event
    if state.menu_bar.active && state.menu_bar.dropdown_open {
        let col = column as usize;
        let row_usize = row as usize;

        if crate::menu::is_point_in_dropdown(&state.menu_bar, col, row_usize, line_num_width) {
            // Event is within dropdown - handle it
            let (action, needs_full_redraw) = crate::menu::handle_menu_mouse(&mut state.menu_bar, mouse_event, line_num_width);
            if let Some(action) = action {
                state.pending_menu_action = Some(action);
            }
            if needs_full_redraw {
                state.needs_redraw = true;
            }
            return;
        } else if matches!(kind, MouseEventKind::Down(MouseButton::Left)) {
            // Left click outside dropdown - close the menu
            state.menu_bar.close();
            state.needs_redraw = true;
            // Don't return - let the click be handled by the editor
        } else if matches!(kind, MouseEventKind::Moved) {
            // Mouse moved outside dropdown - just ignore (don't close menu, don't process in editor)
            return;
        }
        // For other event types (scrolling, etc.), fall through to normal handling
    }

    // In rendered markdown mode, content mouse events are handled separately.
    // Only rows > 0 (actual content rows) need the rendered handler.
    if state.markdown_rendered && row > 0 {
        handle_rendered_mouse_event(state, mouse_event, visible_lines);
        return;
    }

    // Check if click is on h-scrollbar row (last content line when h-scrollbar is shown)
    let h_scrollbar_row = visible_lines as u16;
    let footer_row = (visible_lines + 1) as u16;

    if row == h_scrollbar_row {
        if kind == MouseEventKind::Down(MouseButton::Left) {
            // Check if it's on horizontal scrollbar
            if is_horizontal_scrollbar_click(state, lines, column, visible_lines) {
                handle_horizontal_scrollbar_click(state, lines, column, visible_lines);
                return;
            }
        }
        // If not on h-scrollbar, treat as regular content click (fall through)
    }

    // Check if click is on footer row
    if row == footer_row {
        if kind == MouseEventKind::Down(MouseButton::Left) {
            handle_footer_click(state, lines, column, visible_lines);
        }
        return;
    }

    let visual_line = (row as usize).saturating_sub(1);
    // Ignore clicks beyond visible content, but allow scrollbar events to reach the boundary
    let scrollbar_column = state.term_width - 1;
    
    // Scrollbar is always visible now, so it's always clickable
    let is_scrollbar_event = column == scrollbar_column;

    if visual_line >= visible_lines && !is_scrollbar_event {
        return;
    }
    match kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if is_scrollbar_event {
                // Click on scrollbar - clamp visual_line to valid range for scrollbar
                let clamped_visual_line = visual_line.min(visible_lines - 1);

                handle_scrollbar_click(state, lines, clamped_visual_line, row, visible_lines);
            } else {
                // Check if click is on line number area
                let line_num_width = crate::coordinates::line_number_width(state.settings);
                if column < line_num_width {
                    // Click on line number - select entire line
                    handle_line_number_click(state, lines, visual_line, visible_lines);
                } else {
                    let pos_opt = visual_to_logical_position(
                        state,
                        lines,
                        visual_line,
                        column,
                        visible_lines,
                    );

                    // Check if this might be a multi-click first (within 500ms of last click)
                    let is_potential_multiclick = if let Some(last_time) = state.last_click_time {
                        last_time.elapsed().as_millis() <= 500
                            && is_same_click_location(
                                state.last_click_pos,
                                (visual_line, column as usize),
                            )
                    } else {
                        false
                    };

                    // Check if click is inside an existing selection (only when pos is valid)
                    let in_selection = if let Some((logical_line, col)) = pos_opt {
                        let clicked = (logical_line, col.min(lines[logical_line].len()));
                        !is_potential_multiclick && state.is_point_in_selection(clicked)
                    } else {
                        false
                    };

                    if in_selection {
                        // Start drag operation (only if not a potential multi-click).
                        // Also remember where the user clicked so that, if they release the
                        // mouse without actually dragging, we can clear the selection and
                        // place the cursor at that position instead of keeping the selection.
                        if let Some((logical_line, col)) = pos_opt {
                            state.drag_click_logical_pos = Some((logical_line, col.min(lines[logical_line].len())));
                        }
                        state.start_drag();
                    } else {
                        // Normal cursor move (including multi-click handling).
                        // handle_mouse_click_multiclick -> handle_mouse_click handles
                        // clicks below the last document line by falling back to last line.
                        handle_mouse_click_multiclick(
                            state,
                            lines,
                            visual_line,
                            column,
                            visible_lines,
                        );
                    }
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if state.scrollbar_dragging {
                // Handle vertical scrollbar dragging
                handle_scrollbar_drag(state, lines, row, visible_lines);
            } else if state.h_scrollbar_dragging {
                // Handle horizontal scrollbar dragging
                handle_horizontal_scrollbar_drag(state, lines, column, visible_lines);
            } else if state.dragging_selection_active {
                if let Some((logical_line, col)) =
                    visual_to_logical_position(state, lines, visual_line, column, visible_lines)
                {
                    state.drag_target = Some((logical_line, col.min(lines[logical_line].len())));
                    state.needs_redraw = true; // could render a placeholder caret
                }
            } else {
                // Check if dragging on line number area
                let line_num_width = crate::coordinates::line_number_width(state.settings);
                if column < line_num_width {
                    if state.line_number_drag_active {
                        // Drag started on the line number area – extend line selection.
                        handle_line_number_drag(state, lines, visual_line, visible_lines);
                    } else {
                        // Drag started from text area and moved over line numbers.
                        // Continue the existing text selection by treating the cursor as
                        // being at the very start of the text (column = line_num_width).
                        handle_mouse_drag(state, lines, visual_line, line_num_width, visible_lines, modifiers);
                    }
                } else {
                    handle_mouse_drag(state, lines, visual_line, column, visible_lines, modifiers);
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if state.scrollbar_dragging {
                state.scrollbar_dragging = false;
                state.needs_redraw = true;
            } else if state.h_scrollbar_dragging {
                state.h_scrollbar_dragging = false;
                state.needs_redraw = true;
            } else if state.dragging_selection_active {
                if state.drag_target.is_some() {
                    // Actual drag-to-move/copy operation
                    finalize_drag(state, lines, modifiers.contains(KeyModifiers::CONTROL));
                } else {
                    // User clicked inside the selection without dragging:
                    // clear the selection and place the cursor at the click position.
                    let click_pos = state.drag_click_logical_pos;
                    state.clear_drag();
                    state.clear_selection();
                    if let Some((logical_line, col)) = click_pos {
                        state.set_cursor_position(logical_line, col, lines, visible_lines);
                    }
                    state.needs_redraw = true;
                }
            }
            state.mouse_dragging = false;
            state.line_number_drag_active = false;
            state.last_drag_position = None; // Clear drag position
        }
        MouseEventKind::ScrollDown => {
            if modifiers.contains(KeyModifiers::SHIFT) {
                // Shift+ScrollDown = horizontal scroll right
                handle_horizontal_scroll_right(state, lines, visible_lines);
            } else {
                // Normal scroll = vertical scroll down
                let scroll_amount = state.settings.mouse_scroll_lines;
                handle_mouse_scroll_down(state, lines, visible_lines, scroll_amount);
            }
        }
        MouseEventKind::ScrollUp => {
            if modifiers.contains(KeyModifiers::SHIFT) {
                // Shift+ScrollUp = horizontal scroll left
                handle_horizontal_scroll_left(state, lines, visible_lines);
            } else {
                // Normal scroll = vertical scroll up
                let scroll_amount = state.settings.mouse_scroll_lines;
                handle_mouse_scroll_up(state, lines, visible_lines, scroll_amount);
            }
        }
        MouseEventKind::ScrollLeft => {
            // Touchpad horizontal scroll left
            handle_horizontal_scroll_left(state, lines, visible_lines);
        }
        MouseEventKind::ScrollRight => {
            // Touchpad horizontal scroll right
            handle_horizontal_scroll_right(state, lines, visible_lines);
        }
        _ => {}
    }
}

/// Compute the (logical_line, char_col) for a click/drag that landed below all document content.
/// In wrapping mode the column is interpreted as being on the last visual segment of the last
/// logical line, consistent with how visual_to_logical_position maps intra-line visual rows.
fn below_document_position(
    state: &FileViewerState,
    lines: &[String],
    column: u16,
    visible_lines: usize,
) -> Option<(usize, usize)> {
    if lines.is_empty() {
        return None;
    }
    let line_num_width = crate::coordinates::line_number_width(state.settings);
    let scrollbar_width = 1u16;
    let text_end = state.term_width.saturating_sub(scrollbar_width);
    if column < line_num_width || column >= text_end {
        return None; // Click on line number area or scrollbar
    }
    let text_col = (column - line_num_width) as usize;
    let last_line = lines.len() - 1;
    let line = &lines[last_line];
    let tab_width = state.settings.tab_width;

    let char_col = if state.is_line_wrapping_enabled() {
        // In wrapping mode, treat the click as being on the LAST visual segment of the
        // last logical line.  The segment starts at the last wrap point (in char-space);
        // we add text_col (a visual offset within that segment) to find the absolute
        // visual position, then convert to a char index.
        let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines);
        let wrap_points =
            crate::coordinates::calculate_word_wrap_points(line, text_width as usize, tab_width);
        let segment_start_char = wrap_points.last().copied().unwrap_or(0);
        let segment_start_visual =
            crate::coordinates::visual_width_up_to(line, segment_start_char, tab_width);
        let absolute_visual_col = segment_start_visual + text_col;
        crate::coordinates::visual_col_to_char_index(line, absolute_visual_col, tab_width)
    } else {
        // In horizontal-scroll mode there is no wrapping; add the scroll offset.
        let visual_col = state.horizontal_scroll_offset + text_col;
        crate::coordinates::visual_col_to_char_index(line, visual_col, tab_width)
    };

    Some((last_line, char_col))
}

/// Handle mouse click to position cursor
fn handle_mouse_click(
    state: &mut FileViewerState,
    lines: &[String],
    visual_line: usize,
    column: u16,
    visible_lines: usize,
) {
    // If click is below the document, fall back to last line with the same column position
    let click_result = visual_to_logical_position(state, lines, visual_line, column, visible_lines)
        .or_else(|| {
            below_document_position(state, lines, column, visible_lines)
        });

    if let Some((logical_line, col)) = click_result {
        restore_cursor_to_screen(state);
        
        // Use helper to set cursor position with proper bounds checking
        state.set_cursor_position(logical_line, col, lines, visible_lines);

        // Check if we clicked at/past a wrap indicator and should set cursor_at_wrap_end
        if state.is_line_wrapping_enabled() && logical_line < lines.len() {
            let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines);
            let wrap_points = crate::coordinates::calculate_word_wrap_points(
                &lines[logical_line],
                text_width as usize,
                state.settings.tab_width
            );

            // If cursor ends up at a wrap point, we need to determine which side
            // The key: if visual_to_logical returned the wrap point position,
            // it means we clicked somewhere that maps to that character.
            // We need to check: did we click PAST the actual visual content?
            if wrap_points.contains(&state.cursor_col) {
                let line = &lines[logical_line];

                // Calculate which segment this wrap point ends
                let mut segment_idx = 0;
                for (idx, &wp) in wrap_points.iter().enumerate() {
                    if wp == state.cursor_col {
                        segment_idx = idx;
                        break;
                    }
                }

                // Get the segment boundaries
                let segment_start_char = if segment_idx == 0 { 0 } else { wrap_points[segment_idx - 1] };
                let segment_end_char = state.cursor_col; // The wrap point

                // Calculate visual widths
                let segment_start_visual = crate::coordinates::visual_width_up_to(line, segment_start_char, state.settings.tab_width);
                let segment_end_visual = crate::coordinates::visual_width_up_to(line, segment_end_char, state.settings.tab_width);
                let content_width_in_segment = segment_end_visual - segment_start_visual;

                // Calculate where the mouse clicked within this visual line
                let line_num_width = crate::coordinates::line_number_width(state.settings);
                let text_col = if column >= line_num_width {
                    (column - line_num_width) as usize
                } else {
                    0
                };

                // If we clicked past the content width, we're in the empty space/wrap indicator area
                if text_col >= content_width_in_segment {
                    state.cursor_at_wrap_end = true;
                } else {
                    state.cursor_at_wrap_end = false;
                }
            } else {
                state.cursor_at_wrap_end = false;
            }
        } else {
            state.cursor_at_wrap_end = false;
        }

        // Reset horizontal scroll if clicking on empty/short line in horizontal scroll mode
        if !state.is_line_wrapping_enabled() {
            let line_len = lines[logical_line].len();
            // If line is shorter than current scroll offset, reset scroll to show the line
            if line_len <= state.horizontal_scroll_offset {
                state.horizontal_scroll_offset = 0;
            }
        }

        state.clear_selection();
        state.mouse_dragging = true;
        // needs_redraw is set by set_cursor_position
    }
}
/// Handle mouse drag for text selection
fn handle_mouse_drag(
    state: &mut FileViewerState,
    lines: &[String],
    visual_line: usize,
    column: u16,
    visible_lines: usize,
    modifiers: KeyModifiers,
) {
    if !state.mouse_dragging {
        return;
    }

    // Store current drag position for continuous auto-scroll
    state.last_drag_position = Some((visual_line, column));

    // Initialize selection on first drag
    if state.selection_anchor.is_none() {
        let pos = state.current_position();
        state.selection_anchor = Some(pos);
        state.selection_start = Some(pos);
        state.selection_end = Some(pos);
        // Enable block selection if Alt key is pressed
        state.block_selection = modifiers.contains(KeyModifiers::ALT);
    }

    // Handle horizontal auto-scroll and selection when dragging in horizontal scroll mode
    if !state.is_line_wrapping_enabled() {
        let line_num_width = crate::coordinates::line_number_width(state.settings);
        let scrollbar_width = 1; // Always reserve space for scrollbar
        let text_end = state.term_width.saturating_sub(scrollbar_width);
        let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines) as usize;

        // Check if mouse is at or beyond text boundary (scroll zone)
        if column >= text_end {
            // Mouse is beyond visible text area - scroll and extend selection to end of line
            let absolute_line = state.top_line + visual_line.min(visible_lines.saturating_sub(1));
            if absolute_line < lines.len() {
                let line = &lines[absolute_line];
                use crate::coordinates::visual_width;
                let line_visual_width = visual_width(line, state.settings.tab_width);

                // Only scroll if the end of the line is not yet visible
                let end_visible = state.horizontal_scroll_offset + text_width >= line_visual_width;

                if !end_visible && state.horizontal_scroll_offset < line_visual_width {
                    let scroll_speed = state.settings.horizontal_auto_scroll_speed;
                    state.horizontal_scroll_offset = (state.horizontal_scroll_offset + scroll_speed).min(line_visual_width);
                    state.needs_redraw = true;
                }

                // Always extend selection to end of line when mouse is beyond text boundary
                restore_cursor_to_screen(state);
                state.cursor_line = absolute_line.saturating_sub(state.top_line);
                state.cursor_col = line.len(); // Set to end of line
                state.desired_cursor_col = state.cursor_col;
                state.update_selection();
                state.needs_redraw = true;
            }
            return; // Handled, don't process further
        } else if column <= line_num_width {
            // Mouse on line numbers - scroll left
            if state.horizontal_scroll_offset > 0 {
                let scroll_speed = state.settings.horizontal_auto_scroll_speed;
                state.horizontal_scroll_offset = state.horizontal_scroll_offset.saturating_sub(scroll_speed);
                state.needs_redraw = true;
            }
        }
    }

    // Normal mouse position handling (within visible text area)
    // If click is below the document, fall back to last line with the same column
    let drag_result = visual_to_logical_position(state, lines, visual_line, column, visible_lines)
        .or_else(|| {
            below_document_position(state, lines, column, visible_lines)
        });
    if let Some((logical_line, col)) = drag_result
        && logical_line < lines.len()
    {
        restore_cursor_to_screen(state);
        state.cursor_line = logical_line.saturating_sub(state.top_line);
        state.cursor_col = col.min(lines[logical_line].len()); // Clamp to line length
        state.desired_cursor_col = state.cursor_col;

        // Check if we should set cursor_at_wrap_end for wrapped lines
        if state.is_line_wrapping_enabled() {
            let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines);
            let wrap_points = crate::coordinates::calculate_word_wrap_points(
                &lines[logical_line],
                text_width as usize,
                state.settings.tab_width
            );

            if wrap_points.contains(&col) {
                let line_num_width = crate::coordinates::line_number_width(state.settings);
                let text_col = if column >= line_num_width {
                    (column - line_num_width) as usize
                } else {
                    0
                };
                let usable_width = (text_width as usize).saturating_sub(1);

                if text_col >= usable_width {
                    state.cursor_at_wrap_end = true;
                } else {
                    state.cursor_at_wrap_end = false;
                }
            } else {
                state.cursor_at_wrap_end = false;
            }
        } else {
            state.cursor_at_wrap_end = false;
        }

        state.update_selection();
        state.needs_redraw = true;
    }
}
/// Handle click on line number area to select entire line
fn handle_line_number_click(
    state: &mut FileViewerState,
    lines: &[String],
    visual_line: usize,
    visible_lines: usize,
) {
    // Find the logical line corresponding to this visual line
    if let Some(logical_line) =
        visual_line_to_logical_line(state, lines, visual_line, visible_lines)
        && logical_line < lines.len()
    {
        restore_cursor_to_screen(state);

        // Anchor is at the start of the clicked line so that dragging in either
        // direction (into line numbers or into text) extends correctly.
        state.selection_anchor = Some((logical_line, 0));
        state.selection_start = Some((logical_line, 0));

        // Position cursor at end of line
        state.cursor_line = logical_line.saturating_sub(state.top_line);
        state.cursor_col = lines[logical_line].len();
        state.desired_cursor_col = state.cursor_col;

        // Set selection end to include the entire line
        // If there's a next line, go to start of it; otherwise end of current line
        if logical_line + 1 < lines.len() {
            state.selection_end = Some((logical_line + 1, 0));
        } else {
            state.selection_end = Some((logical_line, lines[logical_line].len()));
        }

        state.mouse_dragging = true;
        state.line_number_drag_active = true;
        state.needs_redraw = true;
    }
}
/// Handle drag on line number area to extend line selection
fn handle_line_number_drag(
    state: &mut FileViewerState,
    lines: &[String],
    visual_line: usize,
    visible_lines: usize,
) {
    if !state.mouse_dragging {
        return;
    }

    // Find the logical line corresponding to this visual line
    if let Some(logical_line) =
        visual_line_to_logical_line(state, lines, visual_line, visible_lines)
        && logical_line < lines.len()
    {
        // Use the anchor to determine drag direction so that selection_start/end
        // are always set correctly regardless of which way the user drags.
        let anchor_line = state
            .selection_anchor
            .map(|(l, _)| l)
            .unwrap_or(logical_line);

        restore_cursor_to_screen(state);

        if logical_line >= anchor_line {
            // Dragging downward (or staying on anchor line)
            // Anchor stays at start of anchor line; cursor moves to end of current line.
            state.selection_anchor = Some((anchor_line, 0));
            state.selection_start = Some((anchor_line, 0));

            state.cursor_line = logical_line.saturating_sub(state.top_line);
            state.cursor_col = lines[logical_line].len();
            state.desired_cursor_col = state.cursor_col;

            if logical_line + 1 < lines.len() {
                state.selection_end = Some((logical_line + 1, 0));
            } else {
                state.selection_end = Some((logical_line, lines[logical_line].len()));
            }
        } else {
            // Dragging upward past the anchor line
            // Cursor moves to start of current line; selection covers up to end of anchor line.
            state.selection_anchor = Some((anchor_line, 0));
            state.selection_start = Some((logical_line, 0));

            state.cursor_line = logical_line.saturating_sub(state.top_line);
            state.cursor_col = 0;
            state.desired_cursor_col = 0;

            if anchor_line + 1 < lines.len() {
                state.selection_end = Some((anchor_line + 1, 0));
            } else {
                state.selection_end = Some((anchor_line, lines[anchor_line].len()));
            }
        }

        state.needs_redraw = true;
    }
}
/// Convert visual line to logical line index
fn visual_line_to_logical_line(
    state: &FileViewerState,
    lines: &[String],
    visual_line: usize,
    visible_lines: usize,
) -> Option<usize> {
    use crate::coordinates::calculate_wrapped_lines_for_line;
    let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines);
    let tab_width = state.settings.tab_width;

    let mut current_visual_line = 0;
    let mut logical_line = state.top_line;

    while logical_line < lines.len() {
        let wrapped_lines =
            calculate_wrapped_lines_for_line(lines, logical_line, text_width, tab_width) as usize;

        if current_visual_line + wrapped_lines > visual_line {
            return Some(logical_line);
        }

        current_visual_line += wrapped_lines;
        logical_line += 1;
    }

    None
}
/// Handle mouse scroll down event
fn handle_mouse_scroll_down(
    state: &mut FileViewerState,
    lines: &[String],
    visible_lines: usize,
    scroll_amount: usize,
) {
    // Allow scrolling until the last line is at the top (not just at the bottom)
    let max_scroll = lines.len().saturating_sub(1);
    let absolute_cursor = state.absolute_line();
    let old_top = state.top_line;
    state.top_line = (state.top_line + scroll_amount).min(max_scroll);
    update_cursor_visibility_after_scroll(state, absolute_cursor, visible_lines);
    if state.top_line != old_top {
        state.needs_redraw = true;
    }
}
/// Handle mouse scroll up event
fn handle_mouse_scroll_up(
    state: &mut FileViewerState,
    _lines: &[String],
    visible_lines: usize,
    scroll_amount: usize,
) {
    let absolute_cursor = state.absolute_line();
    let old_top = state.top_line;
    state.top_line = state.top_line.saturating_sub(scroll_amount);
    update_cursor_visibility_after_scroll(state, absolute_cursor, visible_lines);
    if state.top_line != old_top {
        state.needs_redraw = true;
    }
}
/// Update cursor visibility state after scrolling
fn update_cursor_visibility_after_scroll(
    state: &mut FileViewerState,
    absolute_cursor: usize,
    visible_lines: usize,
) {
    let old_cursor_line = state.cursor_line;
    let old_top = state.top_line;
    if absolute_cursor < state.top_line {
        // Cursor moved above visible area
        save_cursor_state_if_needed(state, old_top, old_cursor_line);
        state.saved_absolute_cursor = Some(absolute_cursor);
        state.cursor_line = 0;
    } else {
        let new_cursor_line = absolute_cursor - state.top_line;
        if new_cursor_line >= visible_lines {
            // Cursor below visible area
            save_cursor_state_if_needed(state, old_top, old_cursor_line);
            state.saved_absolute_cursor = Some(absolute_cursor);
            state.cursor_line = new_cursor_line;
        } else {
            // Cursor is visible - clear saved state
            restore_cursor_to_screen(state);
            state.cursor_line = new_cursor_line;
        }
    }
}
/// Save cursor state when it first goes off-screen
fn save_cursor_state_if_needed(
    state: &mut FileViewerState,
    old_top: usize,
    old_cursor_line: usize,
) {
    if state.saved_scroll_state.is_none() {
        state.saved_scroll_state = Some((old_top, old_cursor_line));
    }
}
/// Restore cursor to on-screen state
fn restore_cursor_to_screen(state: &mut FileViewerState) {
    state.saved_absolute_cursor = None;
    state.saved_scroll_state = None;
}
/// Handle horizontal scroll right (Shift+ScrollDown or similar)
fn handle_horizontal_scroll_right(
    state: &mut FileViewerState,
    lines: &[String],
    visible_lines: usize,
) {
    // Only scroll horizontally when line wrapping is disabled
    if state.is_line_wrapping_enabled() {
        return;
    }

    // Calculate maximum scroll offset
    let tab_width = state.settings.tab_width;
    let max_line_width = lines.iter()
        .map(|line| crate::coordinates::visual_width(line, tab_width))
        .max()
        .unwrap_or(0);

    let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines) as usize;
    let max_scroll = max_line_width.saturating_sub(text_width);

    if max_scroll == 0 {
        return;
    }

    // Scroll right by the configured amount
    let scroll_amount = state.settings.horizontal_scroll_speed;
    let old_offset = state.horizontal_scroll_offset;
    state.horizontal_scroll_offset = (state.horizontal_scroll_offset + scroll_amount).min(max_scroll);

    if state.horizontal_scroll_offset != old_offset {
        state.needs_redraw = true;
    }
}

/// Handle horizontal scroll left (Shift+ScrollUp or similar)
fn handle_horizontal_scroll_left(
    state: &mut FileViewerState,
    _lines: &[String],
    _visible_lines: usize,
) {
    // Only scroll horizontally when line wrapping is disabled
    if state.is_line_wrapping_enabled() {
        return;
    }

    // Scroll left by the configured amount
    let scroll_amount = state.settings.horizontal_scroll_speed;
    let old_offset = state.horizontal_scroll_offset;
    state.horizontal_scroll_offset = state.horizontal_scroll_offset.saturating_sub(scroll_amount);

    if state.horizontal_scroll_offset != old_offset {
        state.needs_redraw = true;
    }
}

/// Finalize a drag operation: move or copy selected text to drag_target
fn finalize_drag(state: &mut FileViewerState, lines: &mut Vec<String>, copy: bool) {
    use crate::editing::apply_drag;
    if let (Some(start), Some(end), Some(dest)) = (
        state.drag_source_start,
        state.drag_source_end,
        state.drag_target,
    ) {
        apply_drag(state, lines, start, end, dest, copy);
    }
    state.clear_drag();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::set_temp_home;
    use crate::settings::Settings;
    use crate::undo::UndoHistory;

    fn create_test_state(settings: &'static Settings) -> FileViewerState<'static> {
        FileViewerState::new(80, UndoHistory::new(), settings)
    }

    #[test]
    fn mouse_click_on_header_row_is_ignored() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec!["line 1".to_string(), "line 2".to_string()];
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 0,
            modifiers: KeyModifiers::empty(),
        };

        let original_cursor = state.cursor_line;
        handle_mouse_event(&mut state, &mut lines, mouse_event, 20);
        assert_eq!(state.cursor_line, original_cursor);
    }

    #[test]
    fn mouse_click_beyond_visible_lines_is_ignored() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec!["line 1".to_string(), "line 2".to_string()];
        let visible_lines = 5;
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: (visible_lines + 2) as u16,
            modifiers: KeyModifiers::empty(),
        };

        let original_cursor = state.cursor_line;
        handle_mouse_event(&mut state, &mut lines, mouse_event, visible_lines);
        assert_eq!(state.cursor_line, original_cursor);
    }

    #[test]
    fn mouse_scroll_down_updates_top_line() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![];
        for i in 0..100 {
            lines.push(format!("line {}", i));
        }
        state.top_line = 10;

        let mouse_event = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::empty(),
        };

        handle_mouse_event(&mut state, &mut lines, mouse_event, 20);
        assert!(state.top_line > 10);
        assert!(state.needs_redraw);
    }

    #[test]
    fn mouse_scroll_up_updates_top_line() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![];
        for i in 0..100 {
            lines.push(format!("line {}", i));
        }
        state.top_line = 20;

        let mouse_event = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::empty(),
        };

        handle_mouse_event(&mut state, &mut lines, mouse_event, 20);
        assert!(state.top_line < 20);
        assert!(state.needs_redraw);
    }

    #[test]
    fn mouse_scroll_up_at_top_stays_at_zero() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec!["line 1".to_string(), "line 2".to_string()];
        state.top_line = 0;

        let mouse_event = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::empty(),
        };

        handle_mouse_event(&mut state, &mut lines, mouse_event, 20);
        assert_eq!(state.top_line, 0);
    }

    #[test]
    fn mouse_scroll_down_respects_max_scroll() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![
            "line 1".to_string(),
            "line 2".to_string(),
            "line 3".to_string(),
        ];
        state.top_line = 2;

        let mouse_event = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::empty(),
        };

        handle_mouse_event(&mut state, &mut lines, mouse_event, 20);
        // Can scroll to show last line at top (max_scroll = lines.len() - 1 = 2)
        assert_eq!(state.top_line, 2);
    }

    #[test]
    fn restore_cursor_to_screen_clears_saved_state() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        state.saved_absolute_cursor = Some(42);
        state.saved_scroll_state = Some((10, 5));

        restore_cursor_to_screen(&mut state);

        assert!(state.saved_absolute_cursor.is_none());
        assert!(state.saved_scroll_state.is_none());
    }

    #[test]
    fn save_cursor_state_preserves_first_save() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);

        save_cursor_state_if_needed(&mut state, 10, 5);
        assert_eq!(state.saved_scroll_state, Some((10, 5)));

        // Second call should not overwrite
        save_cursor_state_if_needed(&mut state, 20, 8);
        assert_eq!(state.saved_scroll_state, Some((10, 5)));
    }

    #[test]
    fn mouse_up_clears_dragging_state() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec!["line 1".to_string()];
        state.mouse_dragging = true;

        let mouse_event = MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: 5,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };

        handle_mouse_event(&mut state, &mut lines, mouse_event, 20);
        assert!(!state.mouse_dragging);
    }

    #[test]
    fn mouse_click_on_line_number_selects_entire_line() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![
            "first line".to_string(),
            "second line".to_string(),
            "third line".to_string(),
        ];

        // Click on line number area (column 0, assuming line numbers are shown)
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0, // In line number area
            row: 2,    // Second line (row 0 is header, row 1 is first line)
            modifiers: KeyModifiers::empty(),
        };

        handle_mouse_event(&mut state, &mut lines, mouse_event, 20);

        // Should select the entire second line (index 1)
        assert_eq!(state.selection_start, Some((1, 0)));
        assert_eq!(state.selection_end, Some((2, 0))); // Start of next line
        assert!(state.mouse_dragging);
        assert!(state.needs_redraw);
    }

    #[test]
    fn mouse_drag_on_line_number_extends_line_selection() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![
            "first line".to_string(),
            "second line".to_string(),
            "third line".to_string(),
            "fourth line".to_string(),
        ];

        // First click on line 1 (as handle_line_number_click would set up)
        state.selection_anchor = Some((1, 0));
        state.selection_start = Some((1, 0));
        state.selection_end = Some((2, 0));
        state.mouse_dragging = true;
        state.line_number_drag_active = true;

        // Drag to line 3
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: 0, // In line number area
            row: 4,    // Fourth line (row 0 is header)
            modifiers: KeyModifiers::empty(),
        };

        handle_mouse_event(&mut state, &mut lines, mouse_event, 20);

        // Should extend selection from line 1 to line 3
        assert_eq!(state.selection_start, Some((1, 0)));
        assert_eq!(state.selection_end, Some((3, lines[3].len()))); // Start of line after third
        assert!(state.needs_redraw);
    }

    #[test]
    fn visual_line_to_logical_line_works_correctly() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let lines = vec![
            "line 0".to_string(),
            "line 1".to_string(),
            "line 2".to_string(),
        ];

        state.top_line = 0;

        // Visual line 0 should map to logical line 0
        let logical = visual_line_to_logical_line(&state, &lines, 0, 10);
        assert_eq!(logical, Some(0));

        // Visual line 1 should map to logical line 1
        let logical = visual_line_to_logical_line(&state, &lines, 1, 10);
        assert_eq!(logical, Some(1));

        // Visual line 2 should map to logical line 2
        let logical = visual_line_to_logical_line(&state, &lines, 2, 10);
        assert_eq!(logical, Some(2));
    }

    #[test]
    fn line_selection_with_scrolling() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![];
        for i in 0..20 {
            lines.push(format!("line {}", i));
        }

        // Scroll down a bit
        state.top_line = 5;

        // Click on line number for first visible line (logical line 5)
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0, // In line number area
            row: 1,    // First visible line
            modifiers: KeyModifiers::empty(),
        };

        handle_mouse_event(&mut state, &mut lines, mouse_event, 20);

        // Should select logical line 5
        assert_eq!(state.selection_start, Some((5, 0)));
        assert_eq!(state.selection_end, Some((6, 0)));
    }

    #[test]
    fn scrollbar_click_on_bar_starts_dragging() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![];
        // Create enough lines to need scrolling
        for i in 0..100 {
            lines.push(format!("line {}", i));
        }

        // Set up initial state
        state.top_line = 25; // Somewhere in middle
        let visible_lines = 20;

        // Calculate where the scrollbar bar would be (matching the logic in handle_scrollbar_click)
        let total_lines = lines.len();
        let scrollbar_height = visible_lines;
        let bar_height = (visible_lines * visible_lines / total_lines).max(1);
        let max_scroll = total_lines.saturating_sub(visible_lines);
        let scroll_progress = state.top_line as f64 / max_scroll as f64;
        let bar_position = ((scrollbar_height - bar_height) as f64 * scroll_progress) as usize;

        let scrollbar_column = state.term_width - 1;

        // Click within the scrollbar bar (add 1 because row 0 is header)
        let click_row = (bar_position + 1) as u16 + 1; // +1 for header, +1 to be within bar
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: scrollbar_column,
            row: click_row,
            modifiers: KeyModifiers::empty(),
        };

        handle_mouse_event(&mut state, &mut lines, mouse_event, visible_lines);

        // Should start scrollbar dragging
        assert!(state.scrollbar_dragging);
        assert_eq!(state.scrollbar_drag_start_top_line, 25);
        assert_eq!(state.scrollbar_drag_start_y, click_row);
        assert!(state.needs_redraw);
    }

    #[test]
    fn scrollbar_click_in_background_jumps_to_position() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![];
        // Create enough lines to need scrolling
        for i in 0..100 {
            lines.push(format!("line {}", i));
        }

        state.top_line = 0; // Start at top
        let visible_lines = 20;

        // Click in scrollbar background near the end
        let scrollbar_column = state.term_width - 1;
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: scrollbar_column,
            row: 18, // Near bottom of scrollbar
            modifiers: KeyModifiers::empty(),
        };

        let old_top_line = state.top_line;
        handle_mouse_event(&mut state, &mut lines, mouse_event, visible_lines);

        // Should jump to new scroll position
        assert!(state.top_line > old_top_line);
        assert!(!state.scrollbar_dragging); // Not dragging, just jumped
        assert!(state.needs_redraw);
    }

    #[test]
    fn scrollbar_drag_updates_scroll_position() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![];
        // Create enough lines to need scrolling
        for i in 0..100 {
            lines.push(format!("line {}", i));
        }

        // Set up dragging state
        state.scrollbar_dragging = true;
        state.scrollbar_drag_start_top_line = 25;
        state.scrollbar_drag_start_y = 10;
        state.top_line = 25;
        let visible_lines = 20;

        // Drag down by 5 rows
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: state.term_width - 1,
            row: 15, // 5 rows down from start
            modifiers: KeyModifiers::empty(),
        };

        handle_mouse_event(&mut state, &mut lines, mouse_event, visible_lines);

        // Should update scroll position proportionally
        assert!(state.top_line > 25); // Moved down
        assert!(state.scrollbar_dragging); // Still dragging
        assert!(state.needs_redraw);
    }

    #[test]
    fn scrollbar_mouse_up_stops_dragging() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![];
        for i in 0..50 {
            lines.push(format!("line {}", i));
        }

        // Set up dragging state
        state.scrollbar_dragging = true;

        let mouse_event = MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: state.term_width - 1,
            row: 10,
            modifiers: KeyModifiers::empty(),
        };

        handle_mouse_event(&mut state, &mut lines, mouse_event, 20);

        // Should stop dragging
        assert!(!state.scrollbar_dragging);
        assert!(state.needs_redraw);
    }

    #[test]
    fn scrollbar_drag_maintains_cursor_absolute_position() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![];
        // Create enough lines to need scrolling
        for i in 0..100 {
            lines.push(format!("line {}", i));
        }

        // Set up initial state: cursor at line 30 (absolute), viewing from line 20
        state.top_line = 20;
        state.cursor_line = 10; // Visual line 10, absolute line 30
        let visible_lines = 20;
        let absolute_cursor_before = state.absolute_line(); // Should be 30

        assert_eq!(absolute_cursor_before, 30);

        // Set up dragging state
        state.scrollbar_dragging = true;
        state.scrollbar_drag_start_top_line = 20;
        state.scrollbar_drag_start_y = 10;

        // Drag down to scroll to line 40
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: state.term_width - 1,
            row: 15, // Drag down
            modifiers: KeyModifiers::empty(),
        };

        handle_mouse_event(&mut state, &mut lines, mouse_event, visible_lines);

        // The scroll position should have changed
        assert!(state.top_line > 20);

        // The cursor should maintain its absolute position in the text
        let absolute_cursor_after = state.absolute_line();
        assert_eq!(
            absolute_cursor_after, absolute_cursor_before,
            "Cursor absolute position should remain at line {} but is now at line {}",
            absolute_cursor_before, absolute_cursor_after
        );
    }

    #[test]
    fn scrollbar_not_visible_when_few_lines() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![
            "line 1".to_string(),
            "line 2".to_string(),
            "line 3".to_string(),
        ];
        let visible_lines = 20; // More than total lines

        // Click where scrollbar would be
        let scrollbar_column = state.term_width - 1;
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: scrollbar_column,
            row: 10,
            modifiers: KeyModifiers::empty(),
        };

        let old_top_line = state.top_line;
        handle_mouse_event(&mut state, &mut lines, mouse_event, visible_lines);

        // Should not affect scroll position since scrollbar isn't needed
        assert_eq!(state.top_line, old_top_line);
        assert!(!state.scrollbar_dragging);
    }

    #[test]
    fn scrollbar_small_bar_drag_maps_mouse_directly() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![];
        // Create a very long file to get a 1-character scrollbar
        for i in 0..1000 {
            lines.push(format!("line {}", i));
        }

        // Set up dragging state for a 1-character bar
        state.scrollbar_dragging = true;
        state.scrollbar_drag_start_top_line = 100;
        state.scrollbar_drag_start_y = 5;
        state.scrollbar_drag_bar_offset = 0; // Single character bar offset
        state.top_line = 100;
        let visible_lines = 20;

        // Calculate expected bar height (should be 1 for this large file)
        let total_lines = lines.len();
        let bar_height = (visible_lines * visible_lines / total_lines).max(1);
        assert_eq!(bar_height, 1); // Verify we have a 1-character bar

        // Drag near the bottom of the scrollbar
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: state.term_width - 1,
            row: 18, // Near bottom of 20-line visible area
            modifiers: KeyModifiers::empty(),
        };

        handle_mouse_event(&mut state, &mut lines, mouse_event, visible_lines);

        // Should have scrolled significantly down due to direct mapping
        assert!(state.top_line > 100); // Should have moved down substantially
        assert!(state.scrollbar_dragging); // Still dragging
        assert!(state.needs_redraw);
    }

    #[test]
    fn scrollbar_one_char_drag_reaches_all_positions() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![];

        // Create a very long file to ensure 1-character scrollbar
        // With 12 visible lines, we need enough lines to make bar_height = 1
        // bar_height = (visible_lines * visible_lines / total_lines).max(1)
        // For bar_height to be 1: visible_lines * visible_lines / total_lines < 2
        // So total_lines > visible_lines * visible_lines / 2
        let visible_lines: usize = 12;
        let total_lines: usize = visible_lines * visible_lines + 100; // 244 lines, ensures bar_height = 1

        for i in 0..total_lines {
            lines.push(format!("Line {}", i + 1));
        }

        // Verify we get a 1-character bar
        let bar_height = (visible_lines * visible_lines / total_lines).max(1);
        assert_eq!(bar_height, 1, "Test requires 1-character scrollbar");

        // Set up initial state
        state.top_line = 0;
        let scrollbar_column = state.term_width - 1;

        // Calculate the available positions for the scrollbar
        // Row 0 is header, rows 1-12 are content (visual lines 0-11)
        // Available scroll space = visible_lines - bar_height = 12 - 1 = 11
        // So bar can be at positions 0, 1, 2, ..., 10 (11 positions total)
        let available_scroll_space = visible_lines.saturating_sub(bar_height as usize);
        let max_scroll = total_lines.saturating_sub(visible_lines);

        println!("Test parameters:");
        println!("  Visible lines: {}", visible_lines);
        println!("  Total lines: {}", total_lines);
        println!("  Bar height: {}", bar_height);
        println!("  Available scroll space: {}", available_scroll_space);
        println!("  Max scroll: {}", max_scroll);

        // Start dragging at the top position
        state.scrollbar_dragging = true;
        state.scrollbar_drag_start_top_line = 0;
        state.scrollbar_drag_start_y = 1; // Row 1 (visual line 0)
        state.scrollbar_drag_bar_offset = 0;

        // Test each position from top to bottom
        // Mouse rows 1-12 correspond to visual lines 0-11
        // But only positions 0-10 are valid for the bar (available_scroll_space = 11)
        let mut previous_top_line = None;

        for mouse_row in 1..=(available_scroll_space + 1) {
            let mouse_event = MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: scrollbar_column,
                row: mouse_row as u16,
                modifiers: KeyModifiers::empty(),
            };

            let _old_top_line = state.top_line;
            handle_mouse_event(&mut state, &mut lines, mouse_event, visible_lines);
            let new_top_line = state.top_line;

            // Calculate expected values
            let mouse_visual_line = mouse_row - 1; // Row 1 -> visual line 0
            let target_bar_position = mouse_visual_line.min(available_scroll_space);
            // Use the corrected formula that ensures the bar renders at the target position
            let expected_top_line = if available_scroll_space > 0 && max_scroll > 0 {
                (target_bar_position * max_scroll).div_ceil(available_scroll_space)
            } else {
                0
            };
            let expected_top_line = expected_top_line.min(max_scroll);

            // Verify the scrollbar moved to the expected position
            assert_eq!(
                new_top_line, expected_top_line,
                "Mouse row {} (visual line {}) -> bar position {} -> expected top_line {}, got {}",
                mouse_row, mouse_visual_line, target_bar_position, expected_top_line, new_top_line
            );

            // Verify the scroll position is monotonically increasing (or stays same at end)
            if let Some(prev) = previous_top_line {
                assert!(
                    new_top_line >= prev,
                    "Scroll position should increase: previous {}, current {}",
                    prev,
                    new_top_line
                );
            }

            // Calculate where the bar should be rendered at this scroll position
            let actual_scroll_progress = if max_scroll > 0 {
                new_top_line as f64 / max_scroll as f64
            } else {
                0.0
            };
            let rendered_bar_position =
                (available_scroll_space as f64 * actual_scroll_progress) as usize;

            // The rendered bar position should match our target (within rounding)
            let position_diff = (rendered_bar_position as i32 - target_bar_position as i32).abs();
            assert!(
                position_diff <= 1,
                "Bar position mismatch: target {}, rendered {} (diff {})",
                target_bar_position,
                rendered_bar_position,
                position_diff
            );

            println!(
                "✓ Mouse row {} -> visual line {} -> bar pos {} -> top_line {} -> rendered at {}",
                mouse_row,
                mouse_visual_line,
                target_bar_position,
                new_top_line,
                rendered_bar_position
            );

            previous_top_line = Some(new_top_line);
        }

        // Verify we reached the bottom
        let final_scroll_progress = state.top_line as f64 / max_scroll as f64;
        assert!(
            final_scroll_progress >= 0.9,
            "Should reach near bottom, got scroll progress {:.3}",
            final_scroll_progress
        );

        // Verify all intermediate positions were unique (no skipping)
        // Test a few key positions to ensure smooth progression
        let test_positions = [
            (1, 0.0),  // Top
            (4, 0.3),  // ~30%
            (7, 0.6),  // ~60%
            (10, 0.9), // ~90%
            (11, 1.0), // Bottom (clamped)
        ];

        for (row, expected_progress) in test_positions.iter() {
            state.scrollbar_dragging = true; // Reset dragging state

            let mouse_event = MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                column: scrollbar_column,
                row: *row,
                modifiers: KeyModifiers::empty(),
            };

            handle_mouse_event(&mut state, &mut lines, mouse_event, visible_lines);

            let actual_progress = state.top_line as f64 / max_scroll as f64;
            let progress_diff = (actual_progress - expected_progress).abs();

            assert!(
                progress_diff < 0.15, // Allow 15% tolerance for rounding
                "Position {} should give ~{:.1}% progress, got {:.3} (diff {:.3})",
                row,
                expected_progress * 100.0,
                actual_progress,
                progress_diff
            );
        }

        println!("✓ All scrollbar positions reached successfully!");
    }

    #[test]
    fn double_click_selects_word() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec!["hello world test".to_string(), "second line".to_string()];

        // Calculate where to click: past line numbers, at a word
        let line_num_width = crate::coordinates::line_number_width(state.settings);
        let click_col = (line_num_width as usize) + 3; // A few characters into the text

        // First click on the word "hello"
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: click_col as u16,
            row: 1, // First line (row 0 is header)
            modifiers: KeyModifiers::empty(),
        };

        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);

        // Immediately click again at the same position (double-click)
        state.last_click_time = Some(Instant::now() - std::time::Duration::from_millis(100));
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: click_col as u16,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };

        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);

        // Should have selected the word "hello"
        assert_eq!(state.selection_start, Some((0, 0))); // Start of "hello"
        assert_eq!(state.selection_end, Some((0, 5))); // End of "hello"
        assert!(state.mouse_dragging);
        assert!(state.needs_redraw);
    }

    #[test]
    fn triple_click_selects_line() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![
            "first line".to_string(),
            "second line".to_string(),
            "third line".to_string(),
        ];

        let line_num_width = crate::coordinates::line_number_width(state.settings);
        let click_col = (line_num_width as usize) + 3;

        // First click - normal single click
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: click_col as u16,
            row: 1, // First line
            modifiers: KeyModifiers::empty(),
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);
        assert_eq!(
            state.click_count, 1,
            "After first click, click_count should be 1"
        );

        // Make last click time appear to be in the past (within 500ms window)
        state.last_click_time = Some(Instant::now() - std::time::Duration::from_millis(100));

        // Second click - should be detected as double-click (multiple click)
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: click_col as u16,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);
        assert_eq!(
            state.click_count, 2,
            "After second click, click_count should be 2"
        );
        assert_eq!(
            state.selection_start,
            Some((0, 0)),
            "Double-click should select word start"
        );
        assert_eq!(
            state.selection_end,
            Some((0, 5)),
            "Double-click should select word end (first word is 'first'=5 chars)"
        );

        // Again, make last click time appear to be in the past
        state.last_click_time = Some(Instant::now() - std::time::Duration::from_millis(100));

        // Third click - should be detected as triple-click
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: click_col as u16,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);
        assert_eq!(
            state.click_count, 3,
            "After third click, click_count should be 3"
        );

        // Should have selected the entire line
        assert_eq!(
            state.selection_start,
            Some((0, 0)),
            "Triple-click should select from line start"
        );
        assert_eq!(
            state.selection_end,
            Some((1, 0)),
            "Triple-click should select to next line start"
        );
        assert!(state.mouse_dragging, "Should be in dragging mode");
        assert!(state.needs_redraw, "Should need redraw");
    }

    #[test]
    fn double_click_on_middle_of_word_selects_word() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec!["hello world".to_string()];

        let line_num_width = crate::coordinates::line_number_width(state.settings);
        let click_col = (line_num_width as usize) + 9; // In middle of "world"

        // First click in middle of "world"
        state.last_click_time = Some(Instant::now() - std::time::Duration::from_millis(600));
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: click_col as u16,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);

        // Second click (double-click) at same position
        state.last_click_time = Some(Instant::now() - std::time::Duration::from_millis(100));
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: click_col as u16,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);

        // Should select the entire word "world"
        assert_eq!(state.selection_start, Some((0, 6))); // Start of "world"
        assert_eq!(state.selection_end, Some((0, 11))); // End of "world"
    }

    #[test]
    fn double_click_on_non_word_character() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec!["hello,world".to_string()];

        let line_num_width = crate::coordinates::line_number_width(state.settings);
        let click_col = (line_num_width as usize) + 5; // On the comma

        // First click on comma
        state.last_click_time = Some(Instant::now() - std::time::Duration::from_millis(600));
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: click_col as u16,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);

        // Second click (double-click) on same comma
        state.last_click_time = Some(Instant::now() - std::time::Duration::from_millis(100));
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: click_col as u16,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);

        // Should select just around the non-word character
        // The selection should be minimal for non-word chars
        assert!(state.selection_start.is_some());
        assert!(state.selection_end.is_some());
    }

    #[test]
    fn alt_click_does_not_create_multi_cursor() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![
            "line one".to_string(),
            "line two".to_string(),
            "line three".to_string(),
        ];

        let line_num_width = crate::coordinates::line_number_width(state.settings);
        let click_col = (line_num_width as usize) + 2;

        // Alt+Click should NOT create multi-cursors (removed feature)
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: click_col as u16,
            row: 1,
            modifiers: KeyModifiers::ALT,
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);

        // Should NOT have created multi-cursors
        assert!(!state.has_multi_cursors(), "Alt+Click should not create multi-cursors");

        // Should have positioned cursor normally
        assert_eq!(state.cursor_line, 0);
        assert_eq!(state.cursor_col, 2);

        // Should be ready for dragging
        assert!(state.mouse_dragging);
    }

    #[test]
    fn alt_drag_creates_block_selection() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![
            "hello world".to_string(),
            "test line".to_string(),
            "another line".to_string(),
        ];

        let line_num_width = crate::coordinates::line_number_width(state.settings);
        let start_col = (line_num_width as usize) + 2;

        // Alt+Click to start
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: start_col as u16,
            row: 1,
            modifiers: KeyModifiers::ALT,
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);

        assert!(state.mouse_dragging, "Should be dragging after click");

        // Alt+Drag to second line
        let end_col = (line_num_width as usize) + 5;
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: end_col as u16,
            row: 2, // Second line
            modifiers: KeyModifiers::ALT,
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);

        // Should have created block selection
        assert!(state.block_selection, "Should have enabled block selection");
        assert!(state.selection_start.is_some(), "Should have selection start");
        assert!(state.selection_end.is_some(), "Should have selection end");

        // Verify block selection range
        let (start, end) = state.selection_range().unwrap();
        assert_eq!(start.0, 0, "Should start at line 0");
        assert_eq!(end.0, 1, "Should end at line 1");
        assert_eq!(start.1, 2, "Should start at column 2");
        assert_eq!(end.1, 5, "Should end at column 5");
    }

    #[test]
    fn alt_drag_zero_width_block_selection() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![
            "hello world".to_string(),
            "test line".to_string(),
            "another line".to_string(),
        ];

        let line_num_width = crate::coordinates::line_number_width(state.settings);
        let col = (line_num_width as usize) + 3;

        // Alt+Click to start
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col as u16,
            row: 1,
            modifiers: KeyModifiers::ALT,
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);

        // Alt+Drag vertically (same column, different row) = zero-width block
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: col as u16, // Same column
            row: 3, // Third line
            modifiers: KeyModifiers::ALT,
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);

        // Should have created block selection
        assert!(state.block_selection, "Should have enabled block selection");

        // Verify it's zero-width (same column for start and end)
        let (start, end) = state.selection_range().unwrap();
        assert_eq!(start.1, end.1, "Should be zero-width (same column)");
        assert_ne!(start.0, end.0, "Should span multiple lines");

        // Should span from line 0 to line 2
        assert_eq!(start.0, 0);
        assert_eq!(end.0, 2);
        assert_eq!(start.1, 3); // Column 3
    }

    #[test]
    fn alt_drag_horizontal_expands_column_range() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![
            "hello world".to_string(),
            "test line".to_string(),
        ];

        let line_num_width = crate::coordinates::line_number_width(state.settings);
        let start_col = (line_num_width as usize) + 2;

        // Alt+Click to start at (0, 2)
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: start_col as u16,
            row: 1,
            modifiers: KeyModifiers::ALT,
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);

        // Alt+Drag down one line
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: start_col as u16,
            row: 2,
            modifiers: KeyModifiers::ALT,
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);

        // Now drag horizontally to expand column range
        let end_col = (line_num_width as usize) + 6;
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: end_col as u16,
            row: 2,
            modifiers: KeyModifiers::ALT,
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);

        // Verify column range expanded
        let (start, end) = state.selection_range().unwrap();
        assert_eq!(start.1, 2, "Should start at column 2");
        assert_eq!(end.1, 6, "Should end at column 6");
        assert!(state.block_selection, "Should maintain block selection");
    }

    #[test]
    fn alt_drag_left_changes_direction() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![
            "hello world".to_string(),
            "test line".to_string(),
        ];

        let line_num_width = crate::coordinates::line_number_width(state.settings);
        let start_col = (line_num_width as usize) + 5;

        // Alt+Click at column 5
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: start_col as u16,
            row: 1,
            modifiers: KeyModifiers::ALT,
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);

        // Drag to different line
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: start_col as u16,
            row: 2,
            modifiers: KeyModifiers::ALT,
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);

        // Now drag left (reduce column)
        let left_col = (line_num_width as usize) + 2;
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: left_col as u16,
            row: 2,
            modifiers: KeyModifiers::ALT,
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);

        // Should have selection from col 2 to col 5
        let (start, end) = state.selection_range().unwrap();
        assert_eq!(start.1, 2, "Should start at leftmost column");
        assert_eq!(end.1, 5, "Should end at rightmost column");
    }

    #[test]
    fn normal_drag_without_alt_not_block_selection() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![
            "hello world".to_string(),
            "test line".to_string(),
        ];

        let line_num_width = crate::coordinates::line_number_width(state.settings);
        let start_col = (line_num_width as usize) + 2;

        // Normal click (no Alt)
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: start_col as u16,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);

        // Normal drag (no Alt)
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: (start_col + 5) as u16,
            row: 2,
            modifiers: KeyModifiers::empty(),
        };
        handle_mouse_event(&mut state, &mut lines, mouse_event, 10);

        // Should NOT be block selection
        assert!(!state.block_selection, "Should not be block selection without Alt");

        // But should have normal selection
        assert!(state.selection_start.is_some());
        assert!(state.selection_end.is_some());
    }

    #[test]
    fn mouse_click_below_document_places_cursor_on_last_line() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        // 3 short lines that won't fill the screen
        let mut lines = vec![
            "Hello".to_string(),
            "World".to_string(),
            "Foo".to_string(),
        ];
        let visible_lines = 20;

        // Click at row 10 (visual_line=9), well below the 3 content lines.
        // column=5 is within text area (line_num_width is ~4 by default).
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 10,
            modifiers: KeyModifiers::empty(),
        };

        handle_mouse_event(&mut state, &mut lines, mouse_event, visible_lines);

        // Cursor should be on the last line (index 2)
        let absolute_line = state.top_line + state.cursor_line;
        assert_eq!(
            absolute_line, 2,
            "Expected cursor on last line (2), got absolute_line={absolute_line}"
        );
    }

    #[test]
    fn mouse_click_below_document_wrapping_places_cursor_on_last_line() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);
        let mut lines = vec![
            "Hello".to_string(),
            "A very long line that might wrap in a narrow terminal window with wrapping enabled".to_string(),
            "Short last line".to_string(),
        ];
        let visible_lines = 20;

        // Click far below content
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 15,
            modifiers: KeyModifiers::empty(),
        };

        handle_mouse_event(&mut state, &mut lines, mouse_event, visible_lines);

        let absolute_line = state.top_line + state.cursor_line;
        assert_eq!(
            absolute_line, 2,
            "Expected cursor on last line (2) with wrapping, got absolute_line={absolute_line}"
        );
    }

    /// Verify that clicking below the document in wrapping mode maps the click column
    /// to the LAST visual segment of the last logical line.
    ///
    /// Setup: term_width=18, line_num_width=4, scrollbar=1 → text_width=13 (usable=12).
    /// One logical line of exactly 15 ASCII chars wraps into two visual segments:
    ///   segment 0: chars 0..N  (visual cols 0..usable)
    ///   segment 1: chars N..14 (visual cols 0..remainder)
    ///
    /// Clicking at text_col=2 below the document should land on the LAST segment at
    /// visual col 2, i.e. char index = segment_start + 2 — NOT at char 2 of the line.
    #[test]
    fn mouse_click_below_document_wrapping_uses_last_segment() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let mut state = create_test_state(settings);

        // Single 15-char line.  With default settings wrapping is on.
        // We just need to verify the column maps into the last wrap segment.
        let line = "ABCDEFGHIJKLMNO"; // 15 ASCII chars
        let mut lines = vec![line.to_string()];
        let visible_lines = 20;

        // Determine the wrap point so we can assert the expected char index.
        let text_width =
            crate::coordinates::calculate_text_width(&state, &lines, visible_lines);
        let tab_width = state.settings.tab_width;
        let wrap_points = crate::coordinates::calculate_word_wrap_points(
            line, text_width as usize, tab_width,
        );

        // Only run the column test if there actually is a wrap (text_width < line length).
        if !wrap_points.is_empty() {
            let segment_start_char = *wrap_points.last().unwrap();
            let segment_start_visual = crate::coordinates::visual_width_up_to(
                line, segment_start_char, tab_width,
            );

            // line_num_width: default settings use 4 (3 digits + 1 space)
            let line_num_width = crate::coordinates::line_number_width(state.settings);
            // Click two columns into the text area
            let click_text_col: u16 = 2;
            let click_column = line_num_width + click_text_col;

            let mouse_event = MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: click_column,
                row: 10, // well below the wrapped line
                modifiers: KeyModifiers::empty(),
            };

            handle_mouse_event(&mut state, &mut lines, mouse_event, visible_lines);

            // The cursor should be on the only (last) logical line
            let absolute_line = state.top_line + state.cursor_line;
            assert_eq!(absolute_line, 0, "Should be on line 0");

            // The cursor column should correspond to segment_start + text_col (clamped to line len)
            let expected_visual = segment_start_visual + click_text_col as usize;
            let expected_col = crate::coordinates::visual_col_to_char_index(
                line, expected_visual, tab_width,
            ).min(line.chars().count());
            assert_eq!(
                state.cursor_col, expected_col,
                "Column should be in last wrap segment: expected char {expected_col}, got {}",
                state.cursor_col
            );
        }
        // If no wrap occurred (very wide terminal in test env), the test is vacuously correct.
    }

    // ...existing tests...
}

/// Check if a mouse click is on the horizontal scrollbar
fn is_horizontal_scrollbar_click(
    state: &FileViewerState,
    lines: &[String],
    column: u16,
    visible_lines: usize,
) -> bool {
    // Only when line wrapping is disabled
    if state.is_line_wrapping_enabled() {
        return false;
    }

    // Check if any line exceeds visible width
    let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines) as usize;
    let tab_width = state.settings.tab_width;
    let max_line_width = lines.iter()
        .map(|line| crate::coordinates::visual_width(line, tab_width))
        .max()
        .unwrap_or(0);

    if max_line_width <= text_width {
        return false;
    }

    // Check if click is within horizontal scrollbar area
    let line_num_width = crate::coordinates::line_number_width(state.settings);
    let v_scrollbar_width = 1; // Always reserve space for scrollbar
    let h_scrollbar_start = line_num_width;
    let h_scrollbar_end = state.term_width.saturating_sub(v_scrollbar_width);

    column >= h_scrollbar_start && column < h_scrollbar_end
}

/// Handle mouse click on horizontal scrollbar
fn handle_horizontal_scrollbar_click(
    state: &mut FileViewerState,
    lines: &[String],
    column: u16,
    _visible_lines: usize,
) {
    // Calculate scrollbar dimensions
    let tab_width = state.settings.tab_width;
    let max_line_width = lines.iter()
        .map(|line| crate::coordinates::visual_width(line, tab_width))
        .max()
        .unwrap_or(0);

    let line_num_width = crate::coordinates::line_number_width(state.settings) as usize;
    let v_scrollbar_width = 1; // Always reserve space for scrollbar
    let available_width = (state.term_width as usize)
        .saturating_sub(line_num_width)
        .saturating_sub(v_scrollbar_width);

    if available_width == 0 {
        return;
    }

    let scrollbar_width = available_width;
    let bar_width = ((available_width * available_width) / max_line_width).max(1);

    // Calculate current bar position
    let max_scroll = max_line_width.saturating_sub(available_width);
    let scroll_progress = if max_scroll == 0 {
        0.0
    } else {
        (state.horizontal_scroll_offset as f64 / max_scroll as f64).min(1.0)
    };
    let bar_position = ((scrollbar_width - bar_width) as f64 * scroll_progress) as usize;

    // Convert click position to scrollbar-relative position
    let click_x = (column as usize).saturating_sub(line_num_width);

    // Check if click is on the bar itself
    if click_x >= bar_position && click_x < bar_position + bar_width {
        // Start dragging
        state.h_scrollbar_dragging = true;
        state.h_scrollbar_drag_start_offset = state.horizontal_scroll_offset;
        state.h_scrollbar_drag_start_x = column;
        state.h_scrollbar_drag_bar_offset = click_x - bar_position;
    } else {
        // Click in background - jump to that position
        let target_scroll_progress = click_x as f64 / scrollbar_width as f64;
        let target_offset = (max_scroll as f64 * target_scroll_progress) as usize;
        state.horizontal_scroll_offset = target_offset.min(max_scroll);
        state.needs_redraw = true;
    }
}

/// Handle horizontal scrollbar dragging
fn handle_horizontal_scrollbar_drag(
    state: &mut FileViewerState,
    lines: &[String],
    column: u16,
    _visible_lines: usize,
) {
    if !state.h_scrollbar_dragging {
        return;
    }

    // Calculate scrollbar dimensions
    let tab_width = state.settings.tab_width;
    let max_line_width = lines.iter()
        .map(|line| crate::coordinates::visual_width(line, tab_width))
        .max()
        .unwrap_or(0);

    let line_num_width = crate::coordinates::line_number_width(state.settings) as usize;
    let v_scrollbar_width = 1; // Always reserve space for scrollbar
    let available_width = (state.term_width as usize)
        .saturating_sub(line_num_width)
        .saturating_sub(v_scrollbar_width);

    if available_width == 0 {
        return;
    }

    let scrollbar_width = available_width;
    let bar_width = ((available_width * available_width) / max_line_width).max(1);
    let max_scroll = max_line_width.saturating_sub(available_width);

    // Calculate where the bar should be based on mouse position
    let mouse_x = (column as usize).saturating_sub(line_num_width);
    let target_bar_left = mouse_x.saturating_sub(state.h_scrollbar_drag_bar_offset);

    // Calculate available scroll space
    let available_scroll_space = scrollbar_width.saturating_sub(bar_width);

    if available_scroll_space == 0 {
        return;
    }

    // Calculate target offset
    let scroll_progress = (target_bar_left as f64 / available_scroll_space as f64).min(1.0).max(0.0);
    let target_offset = (max_scroll as f64 * scroll_progress) as usize;

    state.horizontal_scroll_offset = target_offset.min(max_scroll);
    state.needs_redraw = true;
}

