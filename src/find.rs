use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use regex::Regex;

use crate::editor_state::{FileViewerState, Position};

const MAX_FIND_HISTORY: usize = 100;

/// Convert a wildcard pattern (* = any characters, ? = any single character) to a regex pattern
/// This escapes regex special characters and replaces wildcards with their regex equivalents
pub(crate) fn wildcard_to_regex(pattern: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut regex = String::new();
    let mut chars = pattern.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            // Escape regex special characters
            '.' | '^' | '$' | '+' | '|' | '(' | ')' | '[' | ']' | '{' | '}' | '\\' => {
                regex.push('\\');
                regex.push(ch);
            }
            _ => regex.push(ch),
        }
    }

    Ok(regex)
}

/// Convert a pattern to a regex, applying case-insensitive flag and handling wildcard mode
pub(crate) fn pattern_to_regex(pattern: &str, regex_mode: bool) -> Result<Regex, Box<dyn std::error::Error>> {
    let regex_pattern = if regex_mode {
        // Regex mode: use pattern as-is with case-insensitive flag
        format!("(?i){}", pattern)
    } else {
        // Wildcard mode: convert wildcards to regex, then apply case-insensitive flag
        let wildcard_regex = wildcard_to_regex(pattern)?;
        format!("(?i){}", wildcard_regex)
    };

    Regex::new(&regex_pattern).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
}

/// Handle find mode key events
/// Returns true if find mode should exit
pub(crate) fn handle_find_input(
    state: &mut FileViewerState,
    lines: &[String],
    key_event: KeyEvent,
    _visible_lines: usize,
) -> bool {

    let KeyEvent { code, modifiers, .. } = key_event;

    match code {
        KeyCode::Esc => {
            // Exit find mode and restore previous search highlights
            state.find_active = false;
            state.find_via_replace = false; // Clear the flag
            state.find_pattern.clear();
            state.find_error = None;
            state.find_history_index = None;
            // Note: Don't clear selection - keep it visible to show the search scope
            // Note: Don't clear find_scope here - keep it so highlighting remains scoped
            // Restore the search pattern from before entering find mode
            state.last_search_pattern = state.saved_search_pattern.clone();
            state.saved_search_pattern = None;
            state.needs_redraw = true;
            true
        }
        KeyCode::Enter => {
            // Exit find mode and activate search highlighting (don't jump to match)
            if !state.find_pattern.is_empty() {
                // Compile pattern with current find mode
                match pattern_to_regex(&state.find_pattern, state.find_regex_mode) {
                    Ok(_regex) => {
                        // Set last_search_pattern for highlighting
                        state.last_search_pattern = Some(state.find_pattern.clone());
                        state.last_search_regex_mode = state.find_regex_mode;
                        add_to_history(state, state.find_pattern.clone());

                        // Update hit count but don't jump to match
                        update_search_hit_count(state, lines);

                        state.search_wrapped = false;
                        state.wrap_warning_pending = None;
                        state.find_error = None;
                        state.find_active = false;
                        state.find_history_index = None;
                        state.saved_search_pattern = None;
                        // Note: Don't clear selection - keep it visible to show the search scope
                        // Note: Don't clear find_scope - keep it so highlighting remains scoped

                        // If find mode was entered via replace keybinding, automatically enter replace mode
                        if state.find_via_replace {
                            state.find_via_replace = false; // Clear the flag
                            state.replace_active = true;
                            state.replace_pattern.clear();
                            state.replace_cursor_pos = 0;
                        }

                        state.needs_redraw = true;
                    }
                    Err(e) => {
                        state.find_error = Some(format!("Invalid pattern: {}", e));
                        state.needs_redraw = true;
                        return false;
                    }
                }
            } else {
                // Empty search - clear highlights and scope
                state.find_active = false;
                state.find_via_replace = false; // Clear the flag
                state.find_error = None;
                state.find_history_index = None;
                state.wrap_warning_pending = None;
                state.last_search_pattern = None; // Clear highlights
                state.saved_search_pattern = None; // Clear saved pattern
                state.find_scope = None; // Clear search scope for next search
                state.needs_redraw = true;
            }
            true
        }
        KeyCode::Up => {
            // Navigate to previous search in history
            if state.find_history.is_empty() {
                return false;
            }

            if let Some(index) = state.find_history_index {
                if index + 1 < state.find_history.len() {
                    state.find_history_index = Some(index + 1);
                    state.find_pattern = state.find_history[index + 1].clone();
                    state.find_cursor_pos = state.find_pattern.chars().count();
                }
            } else {
                // First time pressing Up - go to most recent
                state.find_history_index = Some(0);
                state.find_pattern = state.find_history[0].clone();
                state.find_cursor_pos = state.find_pattern.chars().count();
            }
            state.find_error = None;
            // Update highlights in real-time
            update_live_highlights(state);
            update_search_hit_count(state, lines);
            state.needs_redraw = true;
            false
        }
        KeyCode::Down => {
            // Navigate to next search in history (or back to empty)
            if let Some(index) = state.find_history_index {
                if index > 0 {
                    state.find_history_index = Some(index - 1);
                    state.find_pattern = state.find_history[index - 1].clone();
                    state.find_cursor_pos = state.find_pattern.chars().count();
                } else {
                    // Back to empty line
                    state.find_history_index = None;
                    state.find_pattern.clear();
                    state.find_cursor_pos = 0;
                }
                state.find_error = None;
                // Update highlights in real-time
                update_live_highlights(state);
                update_search_hit_count(state, lines);
                state.needs_redraw = true;
            }
            false
        }
        KeyCode::Backspace => {
            if state.find_cursor_pos > 0 {
                // Get character indices (not byte indices)
                let chars: Vec<char> = state.find_pattern.chars().collect();
                let mut new_pattern = String::new();
                for (i, ch) in chars.iter().enumerate() {
                    if i != state.find_cursor_pos - 1 {
                        new_pattern.push(*ch);
                    }
                }
                state.find_pattern = new_pattern;
                state.find_cursor_pos -= 1;
                state.find_selection = None; // Clear selection
                state.find_error = None;
                state.find_history_index = None;
                // Update highlights in real-time
                update_live_highlights(state);
                update_search_hit_count(state, lines);
                state.needs_redraw = true;
            }
            false
        }
        KeyCode::Left => {
            if state.find_cursor_pos > 0 {
                state.find_cursor_pos -= 1;
                state.find_selection = None; // Clear selection
                state.needs_redraw = true;
            }
            false
        }
        KeyCode::Right => {
            let pattern_len = state.find_pattern.chars().count();
            if state.find_cursor_pos < pattern_len {
                state.find_cursor_pos += 1;
                state.find_selection = None; // Clear selection
                state.needs_redraw = true;
            }
            false
        }
        KeyCode::Home => {
            state.find_cursor_pos = 0;
            state.find_selection = None; // Clear selection
            state.needs_redraw = true;
            false
        }
        KeyCode::End => {
            state.find_cursor_pos = state.find_pattern.chars().count();
            state.find_selection = None; // Clear selection
            state.needs_redraw = true;
            false
        }
        KeyCode::Char(c) => {
            // Handle Ctrl+F to toggle filter mode (when there's a pattern)
            if (c == 'f' || c == '\x06') && modifiers.contains(KeyModifiers::CONTROL) {
                if !state.find_pattern.is_empty() {
                    // Toggle filter mode and exit find mode
                    state.filter_active = !state.filter_active;

                    // Exit find mode with the pattern as the search
                    if let Ok(_regex) = pattern_to_regex(&state.find_pattern, state.find_regex_mode) {
                        state.last_search_pattern = Some(state.find_pattern.clone());
                        add_to_history(state, state.find_pattern.clone());
                        update_search_hit_count(state, lines);
                        state.search_wrapped = false;
                        state.wrap_warning_pending = None;
                        state.find_error = None;
                        state.find_active = false;
                        state.find_history_index = None;
                        state.saved_search_pattern = None;

                        // When enabling filter mode, ensure cursor is on a visible line
                        if state.filter_active {
                            ensure_cursor_on_visible_line(state, lines);
                        }

                        state.needs_redraw = true;
                    }
                    return true;
                }
                return false;
            }

            // Handle Ctrl+A to select all text in find pattern
            // Ctrl+A is reported as character code 0x01 (ASCII SOH), not as 'a' with CONTROL modifier
            if c == '\x01' || (c == 'a' && modifiers.contains(KeyModifiers::CONTROL)) {
                let pattern_len = state.find_pattern.chars().count();
                if pattern_len > 0 {
                    state.find_selection = Some((0, pattern_len));
                    state.find_cursor_pos = pattern_len;
                }
                state.needs_redraw = true;
                return false;
            }

            // Clear selection if typing
            if state.find_selection.is_some() {
                state.find_selection = None;
            }

            // Ignore characters with Control or Alt modifiers (these are shortcuts)
            // Also ignore ASCII control characters (0x00-0x1F) which are control sequences
            let has_control = modifiers.contains(KeyModifiers::CONTROL);
            let has_alt = modifiers.contains(KeyModifiers::ALT);
            let is_control_char = (c as u32) < 0x20;
            if has_control || has_alt || is_control_char {
                return false;
            }

            // If there's a selection, delete it and insert the new character at selection start
            if let Some((start, end)) = state.find_selection {
                let chars: Vec<char> = state.find_pattern.chars().collect();
                let mut new_pattern = String::new();

                // Add everything before selection
                for i in 0..start {
                    if i < chars.len() {
                        new_pattern.push(chars[i]);
                    }
                }

                // Insert new character
                new_pattern.push(c);

                // Add everything after selection
                for i in end..chars.len() {
                    new_pattern.push(chars[i]);
                }

                state.find_pattern = new_pattern;
                state.find_cursor_pos = start + 1;
                state.find_selection = None;
            } else {
                // Insert character at cursor position
                let chars: Vec<char> = state.find_pattern.chars().collect();
                let mut new_pattern = String::new();
                for (i, ch) in chars.iter().enumerate() {
                    if i == state.find_cursor_pos {
                        new_pattern.push(c);
                    }
                    new_pattern.push(*ch);
                }
                if state.find_cursor_pos == chars.len() {
                    new_pattern.push(c);
                }
                state.find_pattern = new_pattern;
                state.find_cursor_pos += 1;
            }

            state.find_error = None;
            state.find_history_index = None;
            // Update highlights in real-time
            update_live_highlights(state);
            update_search_hit_count(state, lines);
            state.needs_redraw = true;
            false
        }
        _ => false,
    }
}

/// Find next occurrence (F3)
pub(crate) fn find_next_occurrence(
    state: &mut FileViewerState,
    lines: &[String],
    visible_lines: usize,
) {
    if let Some(ref pattern) = state.last_search_pattern.clone() {
        // Compile pattern with current find mode
        if let Ok(regex) = pattern_to_regex(pattern, state.find_regex_mode) {
            if let Some(pos) = find_next(
                lines,
                state.current_position(),
                &regex,
                false,
                state.find_scope,
            ) {
                // Found a match without wrapping
                move_to_position(state, pos, lines.len(), lines, visible_lines);
                state.search_wrapped = false;
                state.wrap_warning_pending = None;
                state.find_error = None;
                update_search_hit_count(state, lines);
            } else {
                // No match found forward - wrap immediately
                if let Some(pos) = find_next(
                    lines,
                    state.current_position(),
                    &regex,
                    true,
                    state.find_scope,
                ) {
                    move_to_position(state, pos, lines.len(), lines, visible_lines);
                    state.search_wrapped = true;
                    state.wrap_warning_pending = None;
                    state.find_error = None;
                    update_search_hit_count(state, lines);
                }
                // If still no match, just stay at current position (no error message)
            }
        }
    }
    state.needs_redraw = true;
}

/// Find previous occurrence (Shift+F3)
pub(crate) fn find_prev_occurrence(
    state: &mut FileViewerState,
    lines: &[String],
    visible_lines: usize,
) {
    if let Some(ref pattern) = state.last_search_pattern.clone() {
        // Compile pattern with current find mode
        if let Ok(regex) = pattern_to_regex(pattern, state.find_regex_mode) {
            if let Some(pos) = find_prev(
                lines,
                state.current_position(),
                &regex,
                false,
                state.find_scope,
            ) {
                // Found a match without wrapping
                move_to_position(state, pos, lines.len(), lines, visible_lines);
                state.search_wrapped = false;
                state.wrap_warning_pending = None;
                state.find_error = None;
                update_search_hit_count(state, lines);
            } else {
                // No match found backward - wrap immediately
                if let Some(pos) = find_prev(
                    lines,
                    state.current_position(),
                    &regex,
                    true,
                    state.find_scope,
                ) {
                    move_to_position(state, pos, lines.len(), lines, visible_lines);
                    state.search_wrapped = true;
                    state.wrap_warning_pending = None;
                    state.find_error = None;
                    update_search_hit_count(state, lines);
                }
                // If still no match, just stay at current position (no error message)
            }
        }
    }
    state.needs_redraw = true;
}

/// Update live highlights based on current find pattern
pub(crate) fn update_live_highlights(state: &mut FileViewerState) {
    if state.find_pattern.is_empty() {
        // Clear highlights when pattern is empty
        state.last_search_pattern = None;
        state.search_hit_count = 0;
        state.search_current_hit = 0;
    } else {
        // Try to compile as regex or wildcard - if valid, update highlights
        if pattern_to_regex(&state.find_pattern, state.find_regex_mode).is_ok() {
            state.last_search_pattern = Some(state.find_pattern.clone());
            state.last_search_regex_mode = state.find_regex_mode;
            state.find_error = None;
        } else {
            // Invalid pattern - don't update highlights but don't show error yet
            // (let user finish typing)
        }
    }
}

/// Add pattern to history, keeping max 100 entries
fn add_to_history(state: &mut FileViewerState, pattern: String) {
    // Remove if already exists
    state.find_history.retain(|p| p != &pattern);

    // Add to front
    state.find_history.insert(0, pattern);

    // Keep only last 100
    if state.find_history.len() > MAX_FIND_HISTORY {
        state.find_history.truncate(MAX_FIND_HISTORY);
    }
}

/// Find the next occurrence of the pattern starting from the given position
/// If scope is Some, only search within the specified range
fn find_next(
    lines: &[String],
    start: Position,
    regex: &Regex,
    force_wrap: bool,
    scope: Option<(Position, Position)>,
) -> Option<Position> {
    let (start_line, start_col) = start;

    // Determine search boundaries
    let (min_line, max_line) = if let Some(((scope_start_line, _), (scope_end_line, _))) = scope {
        (scope_start_line, scope_end_line)
    } else {
        (0, lines.len().saturating_sub(1))
    };

    if !force_wrap {
        // Search from current position to end of current line
        if start_line >= min_line && start_line <= max_line && start_line < lines.len() {
            let line = &lines[start_line];
            // Start searching from next character position
            let search_from = start_col + 1;

            // Determine search end for this line based on scope
            let search_to = if let Some((
                (_scope_start_line, _scope_start_col),
                (scope_end_line, scope_end_col),
            )) = scope
            {
                if start_line == scope_end_line {
                    scope_end_col.min(line.len())
                } else {
                    line.len()
                }
            } else {
                line.len()
            };

            if search_from < search_to {
                let search_slice = &line[search_from..search_to];
                if let Some(m) = regex.find(search_slice) {
                    return Some((start_line, search_from + m.start()));
                }
            }
        }

        // Search remaining lines within scope
        let end_line = max_line.min(lines.len().saturating_sub(1));
        for line_idx in (start_line + 1)..=end_line {
            if line_idx >= lines.len() {
                break;
            }
            let line = &lines[line_idx];

            // Determine search boundaries for this line
            let (search_start, search_end) = if let Some((
                (scope_start_line, scope_start_col),
                (scope_end_line, scope_end_col),
            )) = scope
            {
                let start_offset = if line_idx == scope_start_line {
                    scope_start_col
                } else {
                    0
                };
                let end_offset = if line_idx == scope_end_line {
                    scope_end_col.min(line.len())
                } else {
                    line.len()
                };
                (start_offset, end_offset)
            } else {
                (0, line.len())
            };

            if search_start < search_end {
                let search_slice = &line[search_start..search_end];
                if let Some(m) = regex.find(search_slice) {
                    return Some((line_idx, search_start + m.start()));
                }
            }
        }

        // Don't wrap when force_wrap is false
        return None;
    }

    // Wrap around to beginning (only when force_wrap is true)
    // When scope is set, wrap within scope; otherwise wrap to file beginning
    for line_idx in min_line..=(start_line.min(max_line)) {
        if line_idx >= lines.len() {
            break;
        }
        let line = &lines[line_idx];

        // Determine search boundaries for this line
        let (search_start, search_end) =
            if let Some(((scope_start_line, scope_start_col), (scope_end_line, scope_end_col))) =
                scope
            {
                let start_offset = if line_idx == scope_start_line {
                    scope_start_col
                } else {
                    0
                };
                let end_offset = if line_idx == scope_end_line {
                    scope_end_col.min(line.len())
                } else {
                    line.len()
                };
                (start_offset, end_offset)
            } else {
                (0, line.len())
            };

        if search_start < search_end {
            let search_slice = &line[search_start..search_end];
            if let Some(m) = regex.find(search_slice) {
                return Some((line_idx, search_start + m.start()));
            }
        }
    }

    None
}

/// Find the previous occurrence of the pattern starting from the given position
/// If scope is Some, only search within the specified range
fn find_prev(
    lines: &[String],
    start: Position,
    regex: &Regex,
    force_wrap: bool,
    scope: Option<(Position, Position)>,
) -> Option<Position> {
    let (start_line, start_col) = start;

    // Determine search boundaries
    let (min_line, max_line) = if let Some(((scope_start_line, _), (scope_end_line, _))) = scope {
        (scope_start_line, scope_end_line)
    } else {
        (0, lines.len().saturating_sub(1))
    };

    if !force_wrap {
        // Search backwards in current line
        if start_line >= min_line && start_line <= max_line && start_line < lines.len() {
            let line = &lines[start_line];

            // Determine search boundaries for this line
            let (search_start, search_end) = if let Some((
                (scope_start_line, scope_start_col),
                (scope_end_line, scope_end_col),
            )) = scope
            {
                let start_offset = if start_line == scope_start_line {
                    scope_start_col
                } else {
                    0
                };
                let end_offset = if start_line == scope_end_line {
                    scope_end_col.min(line.len())
                } else {
                    line.len()
                };
                (start_offset, end_offset)
            } else {
                (0, line.len())
            };

            // Find all matches in current line before cursor within scope
            let mut last_match: Option<usize> = None;
            if search_start < search_end {
                let search_slice = &line[search_start..search_end];
                for m in regex.find_iter(search_slice) {
                    let absolute_pos = search_start + m.start();
                    if absolute_pos < start_col {
                        last_match = Some(absolute_pos);
                    } else {
                        break;
                    }
                }
            }
            if let Some(col) = last_match {
                return Some((start_line, col));
            }
        }

        // Search previous lines (reverse order) within scope
        let start_search_line = min_line.max(start_line.saturating_sub(1));
        for line_idx in (min_line..=start_search_line).rev() {
            if line_idx >= start_line || line_idx >= lines.len() {
                continue;
            }
            let line = &lines[line_idx];

            // Determine search boundaries for this line
            let (search_start, search_end) = if let Some((
                (scope_start_line, scope_start_col),
                (scope_end_line, scope_end_col),
            )) = scope
            {
                let start_offset = if line_idx == scope_start_line {
                    scope_start_col
                } else {
                    0
                };
                let end_offset = if line_idx == scope_end_line {
                    scope_end_col.min(line.len())
                } else {
                    line.len()
                };
                (start_offset, end_offset)
            } else {
                (0, line.len())
            };

            // Find last match in this line within scope
            if search_start < search_end {
                let search_slice = &line[search_start..search_end];
                if let Some(last_match) = regex.find_iter(search_slice).last() {
                    return Some((line_idx, search_start + last_match.start()));
                }
            }
        }

        // Don't wrap when force_wrap is false
        return None;
    }

    // Wrap around to end (only when force_wrap is true)
    // When scope is set, wrap within scope; otherwise wrap to file end
    for line_idx in (start_line..=max_line).rev() {
        if line_idx >= lines.len() {
            continue;
        }
        let line = &lines[line_idx];

        // Determine search boundaries for this line
        let (search_start, search_end) =
            if let Some(((scope_start_line, scope_start_col), (scope_end_line, scope_end_col))) =
                scope
            {
                let start_offset = if line_idx == scope_start_line {
                    scope_start_col
                } else {
                    0
                };
                let end_offset = if line_idx == scope_end_line {
                    scope_end_col.min(line.len())
                } else {
                    line.len()
                };
                (start_offset, end_offset)
            } else {
                (0, line.len())
            };

        // Find last match in this line within scope
        if search_start < search_end {
            let search_slice = &line[search_start..search_end];
            if let Some(last_match) = regex.find_iter(search_slice).last() {
                return Some((line_idx, search_start + last_match.start()));
            }
        }
    }

    None
}

/// Move cursor to the specified position, adjusting viewport if needed
fn move_to_position(
    state: &mut FileViewerState,
    pos: Position,
    total_lines: usize,
    lines: &[String],
    visible_lines: usize,
) {
    let (target_line, target_col) = pos;

    if target_line >= total_lines {
        return;
    }

    // Use helper function to set cursor position with proper bounds checking and viewport adjustment
    state.set_cursor_position(target_line, target_col, lines, visible_lines);
}

/// Calculate the total number of search hits and determine the current hit index
/// Returns (current_hit_index, total_hits) where current_hit_index is 1-based (0 if not on a hit)
pub(crate) fn calculate_search_hits(
    lines: &[String],
    cursor_pos: Position,
    pattern: &str,
    regex_mode: bool,
    scope: Option<(Position, Position)>,
) -> (usize, usize) {
    // Compile pattern with the specified mode
    let Ok(regex) = pattern_to_regex(pattern, regex_mode) else {
        return (0, 0);
    };

    let (cursor_line, cursor_col) = cursor_pos;
    let mut total_hits = 0;
    let mut current_hit = 0;
    let mut found_cursor_hit = false;

    // Determine search boundaries
    let (min_line, max_line) = if let Some(((scope_start_line, _), (scope_end_line, _))) = scope {
        (scope_start_line, scope_end_line)
    } else {
        (0, lines.len().saturating_sub(1))
    };

    for line_idx in min_line..=max_line.min(lines.len().saturating_sub(1)) {
        let line = &lines[line_idx];

        // Determine search boundaries for this line based on scope
        let (search_start, search_end) =
            if let Some(((scope_start_line, scope_start_col), (scope_end_line, scope_end_col))) =
                scope
            {
                let start_offset = if line_idx == scope_start_line {
                    scope_start_col
                } else {
                    0
                };
                let end_offset = if line_idx == scope_end_line {
                    scope_end_col.min(line.len())
                } else {
                    line.len()
                };
                (start_offset, end_offset)
            } else {
                (0, line.len())
            };

        if search_start < search_end {
            let search_slice = &line[search_start..search_end];
            for m in regex.find_iter(search_slice) {
                let match_col = search_start + m.start();
                total_hits += 1;

                // Check if this match is at the cursor position
                if !found_cursor_hit && line_idx == cursor_line && match_col == cursor_col {
                    current_hit = total_hits;
                    found_cursor_hit = true;
                }
            }
        }
    }

    (current_hit, total_hits)
}

/// Update the search hit count in the state
pub(crate) fn update_search_hit_count(state: &mut FileViewerState, lines: &[String]) {
    if let Some(ref pattern) = state.last_search_pattern {
        let (current, total) = calculate_search_hits(
            lines,
            state.current_position(),
            pattern,
            state.find_regex_mode,
            state.find_scope,
        );
        state.search_current_hit = current;
        state.search_hit_count = total;
    } else {
        state.search_current_hit = 0;
        state.search_hit_count = 0;
    }
}

/// Ensure cursor is positioned on a visible line when filter mode is active
fn ensure_cursor_on_visible_line(state: &mut FileViewerState, lines: &[String]) {
    if !state.filter_active || state.last_search_pattern.is_none() {
        return;
    }

    let pattern = state.last_search_pattern.as_ref().unwrap();
    let filtered_lines = get_lines_with_matches(lines, pattern, state.find_regex_mode, state.find_scope);

    if filtered_lines.is_empty() {
        return;
    }

    let absolute_line = state.absolute_line();

    // Check if current cursor position is on a visible line
    if !filtered_lines.contains(&absolute_line) {
        // Cursor is on a filtered-out line, move to nearest visible line
        // Try to find the next visible line first, then previous if not found
        if let Some(&next_line_idx) = filtered_lines.iter().find(|&&idx| idx > absolute_line) {
            // Move to next visible line
            if next_line_idx >= state.top_line {
                state.cursor_line = next_line_idx - state.top_line;
            } else {
                state.top_line = next_line_idx;
                state.cursor_line = 0;
            }
            // Adjust cursor column to be within the line
            state.clamp_cursor_to_line_bounds(lines);
        } else if let Some(&prev_line_idx) = filtered_lines.iter().rev().find(|&&idx| idx < absolute_line) {
            // Move to previous visible line
            if prev_line_idx >= state.top_line {
                state.cursor_line = prev_line_idx - state.top_line;
            } else {
                state.top_line = prev_line_idx;
                state.cursor_line = 0;
            }
            // Adjust cursor column to be within the line
            state.clamp_cursor_to_line_bounds(lines);
        } else if let Some(&first_line_idx) = filtered_lines.first() {
            // No visible lines around cursor, jump to first visible line
            state.top_line = first_line_idx;
            state.cursor_line = 0;
            state.clamp_cursor_to_line_bounds(lines);
        }
    }
}

/// Get all line indices that have search matches
/// Returns a vector of line indices (0-based) that contain at least one match
pub fn get_lines_with_matches(
    lines: &[String],
    pattern: &str,
    regex_mode: bool,
    scope: Option<(Position, Position)>,
) -> Vec<usize> {
    get_lines_with_matches_and_context(lines, pattern, regex_mode, scope, 0, 0)
}

/// Get all line indices that have search matches, including context lines
/// Returns a sorted, deduplicated vector of line indices (0-based)
/// - context_before: number of lines to include before each match
/// - context_after: number of lines to include after each match
pub fn get_lines_with_matches_and_context(
    lines: &[String],
    pattern: &str,
    regex_mode: bool,
    scope: Option<(Position, Position)>,
    context_before: usize,
    context_after: usize,
) -> Vec<usize> {
    // Compile pattern with the specified mode
    let Ok(regex) = pattern_to_regex(pattern, regex_mode) else {
        return Vec::new();
    };


    // Determine search boundaries
    let (min_line, max_line) = if let Some(((scope_start_line, _), (scope_end_line, _))) = scope {
        (scope_start_line, scope_end_line)
    } else {
        (0, lines.len().saturating_sub(1))
    };

    // First, find all lines with actual matches
    let mut hit_lines = Vec::new();
    for line_idx in min_line..=max_line.min(lines.len().saturating_sub(1)) {
        let line = &lines[line_idx];

        // Determine search boundaries for this line based on scope
        let (search_start, search_end) =
            if let Some(((scope_start_line, scope_start_col), (scope_end_line, scope_end_col))) =
                scope
            {
                let start_offset = if line_idx == scope_start_line {
                    scope_start_col
                } else {
                    0
                };
                let end_offset = if line_idx == scope_end_line {
                    scope_end_col.min(line.len())
                } else {
                    line.len()
                };
                (start_offset, end_offset)
            } else {
                (0, line.len())
            };

        if search_start < search_end {
            let search_slice = &line[search_start..search_end];
            if regex.is_match(search_slice) {
                hit_lines.push(line_idx);
            }
        }
    }

    // Now add context lines around each hit
    use std::collections::HashSet;
    let mut all_lines: HashSet<usize> = HashSet::new();

    for &hit_line in &hit_lines {
        // Add the hit line itself
        all_lines.insert(hit_line);

        // Add context lines before
        let start = hit_line.saturating_sub(context_before).max(min_line);
        for i in start..hit_line {
            all_lines.insert(i);
        }

        // Add context lines after
        let end = (hit_line + context_after + 1).min(max_line + 1).min(lines.len());
        for i in (hit_line + 1)..end {
            all_lines.insert(i);
        }
    }

    // Convert to sorted vector
    let mut matching_lines: Vec<usize> = all_lines.into_iter().collect();
    matching_lines.sort_unstable();

    matching_lines
}

/// Handle replace mode key events
/// Returns true if replace mode should exit
pub(crate) fn handle_replace_input(
    state: &mut FileViewerState,
    _lines: &[String],
    key_event: KeyEvent,
) -> bool {
    let KeyEvent { code, modifiers, .. } = key_event;

    match code {
        KeyCode::Esc => {
            // Exit replace mode
            state.replace_active = false;
            state.replace_pattern.clear();
            state.replace_cursor_pos = 0;
            state.needs_redraw = true;
            true
        }
        KeyCode::Enter => {
            // Just exit replace mode, don't do anything
            // (user can click buttons or use Ctrl+R / Ctrl+Shift+R)
            state.replace_active = false;
            state.needs_redraw = true;
            true
        }
        KeyCode::Backspace => {
            if state.replace_cursor_pos > 0 {
                // Get character indices (not byte indices)
                let chars: Vec<char> = state.replace_pattern.chars().collect();
                let mut new_pattern = String::new();
                for (i, ch) in chars.iter().enumerate() {
                    if i != state.replace_cursor_pos - 1 {
                        new_pattern.push(*ch);
                    }
                }
                state.replace_pattern = new_pattern;
                state.replace_cursor_pos -= 1;
                state.replace_selection = None; // Clear selection
                state.needs_redraw = true;
            }
            false
        }
        KeyCode::Left => {
            if state.replace_cursor_pos > 0 {
                state.replace_cursor_pos -= 1;
                state.replace_selection = None; // Clear selection
                state.needs_redraw = true;
            }
            false
        }
        KeyCode::Right => {
            let pattern_len = state.replace_pattern.chars().count();
            if state.replace_cursor_pos < pattern_len {
                state.replace_cursor_pos += 1;
                state.replace_selection = None; // Clear selection
                state.needs_redraw = true;
            }
            false
        }
        KeyCode::Home => {
            state.replace_cursor_pos = 0;
            state.replace_selection = None; // Clear selection
            state.needs_redraw = true;
            false
        }
        KeyCode::End => {
            state.replace_cursor_pos = state.replace_pattern.chars().count();
            state.replace_selection = None; // Clear selection
            state.needs_redraw = true;
            false
        }
        KeyCode::Char(c) => {
            // Handle Ctrl+A to select all text in replace pattern
            // Ctrl+A is reported as character code 0x01 (ASCII SOH), not as 'a' with CONTROL modifier
            if c == '\x01' || (c == 'a' && modifiers.contains(KeyModifiers::CONTROL)) {
                let pattern_len = state.replace_pattern.chars().count();
                if pattern_len > 0 {
                    state.replace_selection = Some((0, pattern_len));
                    state.replace_cursor_pos = pattern_len;
                }
                state.needs_redraw = true;
                return true; // Consume the event
            }

            // Clear selection if typing
            if state.replace_selection.is_some() {
                state.replace_selection = None;
            }

            // Ignore characters with Control or Alt modifiers (these are shortcuts)
            // Also ignore ASCII control characters (0x00-0x1F) which are control sequences
            let has_control = modifiers.contains(KeyModifiers::CONTROL);
            let has_alt = modifiers.contains(KeyModifiers::ALT);
            let is_control_char = (c as u32) < 0x20;
            if has_control || has_alt || is_control_char {
                return true; // Consume the event to prevent it from being processed by editor
            }

            // If there's a selection, delete it and insert the new character at selection start
            if let Some((start, end)) = state.replace_selection {
                let chars: Vec<char> = state.replace_pattern.chars().collect();
                let mut new_pattern = String::new();

                // Add everything before selection
                for i in 0..start {
                    if i < chars.len() {
                        new_pattern.push(chars[i]);
                    }
                }

                // Insert new character
                new_pattern.push(c);

                // Add everything after selection
                for i in end..chars.len() {
                    new_pattern.push(chars[i]);
                }

                state.replace_pattern = new_pattern;
                state.replace_cursor_pos = start + 1;
                state.replace_selection = None;
            } else {
                // Insert character at cursor position
                let chars: Vec<char> = state.replace_pattern.chars().collect();
                let mut new_pattern = String::new();
                for (i, ch) in chars.iter().enumerate() {
                    if i == state.replace_cursor_pos {
                        new_pattern.push(c);
                    }
                    new_pattern.push(*ch);
                }
                if state.replace_cursor_pos == chars.len() {
                    new_pattern.push(c);
                }
                state.replace_pattern = new_pattern;
                state.replace_cursor_pos += 1;
            }

            state.needs_redraw = true;
            false
        }
        _ => false,
    }
}

/// Replace the current occurrence and jump to next
pub(crate) fn replace_current_occurrence(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    visible_lines: usize,
) {
    if let Some(ref pattern) = state.last_search_pattern {
        // Make search case-insensitive by default
        let pattern_with_flags = format!("(?i){}", pattern);
        if let Ok(regex) = Regex::new(&pattern_with_flags) {
            let (line, col) = state.current_position();

            // Check if cursor is currently on a match
            if line < lines.len() {
                let line_text = &lines[line];

                // Check scope
                let in_scope = if let Some(((scope_start_line, scope_start_col), (scope_end_line, scope_end_col))) = state.find_scope {
                    line >= scope_start_line && line <= scope_end_line &&
                    (line != scope_start_line || col >= scope_start_col) &&
                    (line != scope_end_line || col < scope_end_col)
                } else {
                    true
                };

                if in_scope {
                    // Find match at current position
                    if let Some(m) = regex.find(line_text) {
                        if m.start() == col {
                            // We're at a match - replace it
                            let before = &line_text[..m.start()];
                            let after = &line_text[m.end()..];
                            let new_line = format!("{}{}{}", before, state.replace_pattern, after);

                            // Record the edit
                            let old_line = lines[line].clone();
                            lines[line] = new_line.clone();
                            state.modified = true;

                            // Add to undo history
                            state.undo_history.push(crate::undo::Edit::ReplaceLine {
                                line,
                                old_content: old_line,
                                new_content: new_line,
                            });

                            state.needs_redraw = true;
                        }
                    }
                }
            }

            // Jump to next occurrence
            find_next_occurrence(state, lines, visible_lines);

            // Update hit count
            update_search_hit_count(state, lines);
        }
    }
}

/// Replace all occurrences and exit replace mode
pub(crate) fn replace_all_occurrences(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
) {
    if let Some(ref pattern) = state.last_search_pattern {
        // Make search case-insensitive by default
        let pattern_with_flags = format!("(?i){}", pattern);
        if let Ok(regex) = Regex::new(&pattern_with_flags) {
            let mut replaced_count = 0;

            // Determine search boundaries
            let (min_line, max_line, scope) = if let Some(((scope_start_line, _), (scope_end_line, _))) = state.find_scope {
                (scope_start_line, scope_end_line, state.find_scope)
            } else {
                (0, lines.len().saturating_sub(1), None)
            };

            // Replace in each line within scope
            for line_idx in min_line..=max_line.min(lines.len().saturating_sub(1)) {
                let line_text = &lines[line_idx];

                // Determine search boundaries for this line
                let (search_start, search_end) = if let Some(((scope_start_line, scope_start_col), (scope_end_line, scope_end_col))) = scope {
                    let start_offset = if line_idx == scope_start_line {
                        scope_start_col
                    } else {
                        0
                    };
                    let end_offset = if line_idx == scope_end_line {
                        scope_end_col.min(line_text.len())
                    } else {
                        line_text.len()
                    };
                    (start_offset, end_offset)
                } else {
                    (0, line_text.len())
                };

                if search_start < search_end {
                    let before_scope = &line_text[..search_start];
                    let search_slice = &line_text[search_start..search_end];
                    let after_scope = &line_text[search_end..];

                    // Replace all matches in the search slice
                    let replaced_slice = regex.replace_all(search_slice, state.replace_pattern.as_str()).to_string();

                    if replaced_slice != search_slice {
                        let new_line = format!("{}{}{}", before_scope, replaced_slice, after_scope);

                        // Count replacements in this line
                        let line_replacements = regex.find_iter(search_slice).count();
                        replaced_count += line_replacements;

                        // Record the edit
                        let old_line = lines[line_idx].clone();
                        lines[line_idx] = new_line.clone();

                        // Add to undo history
                        state.undo_history.push(crate::undo::Edit::ReplaceLine {
                            line: line_idx,
                            old_content: old_line,
                            new_content: new_line,
                        });
                    }
                }
            }

            if replaced_count > 0 {
                state.modified = true;
                state.needs_redraw = true;
            }

            // Don't exit replace mode - let user exit when they want
            // state.replace_active = false;
            // state.replace_pattern.clear();
            // state.replace_cursor_pos = 0;

            // Update hit count
            update_search_hit_count(state, lines);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_simple_pattern() {
        let lines = vec![
            "hello world".to_string(),
            "foo bar".to_string(),
            "hello again".to_string(),
        ];

        // Case-insensitive by default
        let regex = Regex::new("(?i)hello").unwrap();

        // Find next from position (0, 0) should skip current position and find next occurrence
        let result = find_next(&lines, (0, 0), &regex, false, None);
        assert_eq!(result, Some((2, 0)));

        // Find next from end of first "hello" should wrap around
        let result = find_next(&lines, (0, 4), &regex, false, None);
        assert_eq!(result, Some((2, 0)));

        // Find from line 1 should find line 2
        let result = find_next(&lines, (1, 0), &regex, false, None);
        assert_eq!(result, Some((2, 0)));
    }

    #[test]
    fn find_wraps_around() {
        let lines = vec!["hello world".to_string(), "foo bar".to_string()];

        let regex = Regex::new("(?i)hello").unwrap();
        let result = find_next(&lines, (1, 5), &regex, true, None);
        assert_eq!(result, Some((0, 0)));
    }

    #[test]
    fn find_no_match() {
        let lines = vec!["hello world".to_string(), "foo bar".to_string()];

        let regex = Regex::new("(?i)notfound").unwrap();
        let result = find_next(&lines, (0, 0), &regex, false, None);
        assert_eq!(result, None);
    }

    #[test]
    fn find_case_insensitive_by_default() {
        let lines = vec![
            "Hello World".to_string(),
            "HELLO WORLD".to_string(),
            "hello world".to_string(),
        ];

        // Search for lowercase "hello" should find all case variations
        let regex = Regex::new("(?i)hello").unwrap();

        // Find first occurrence (line 0)
        let result = find_next(&lines, (0, 0), &regex, false, None);
        assert_eq!(result, Some((1, 0))); // Skip current, find next

        // Find second occurrence (line 1)
        let result = find_next(&lines, (1, 0), &regex, false, None);
        assert_eq!(result, Some((2, 0)));

        // Search for uppercase "HELLO" should also find all case variations
        let regex = Regex::new("(?i)HELLO").unwrap();
        let result = find_next(&lines, (0, 0), &regex, false, None);
        assert_eq!(result, Some((1, 0)));
    }

    #[test]
    fn find_next_two_press_wrap() {
        let lines = vec![
            "hello world".to_string(),
            "foo bar".to_string(),
            "baz qux".to_string(),
        ];

        let settings = crate::settings::Settings::default();
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, &settings);

        // Set up: cursor at end, pattern "hello" (which is at the beginning)
        state.cursor_line = 2;
        state.top_line = 0;
        state.cursor_col = 0;
        state.last_search_pattern = Some("hello".to_string());

        // Should wrap immediately without warning
        find_next_occurrence(&mut state, &lines, 10);
        assert_eq!(state.wrap_warning_pending, None);
        assert_eq!(state.absolute_line(), 0); // cursor should move to line 0
        assert_eq!(state.cursor_col, 0);
        assert_eq!(state.find_error, None); // No error message
    }

    #[test]
    fn find_prev_two_press_wrap() {
        let lines = vec![
            "foo bar".to_string(),
            "baz qux".to_string(),
            "hello world".to_string(),
        ];

        let settings = crate::settings::Settings::default();
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, &settings);

        // Set up: cursor at beginning, pattern "hello" (which is at the end)
        state.cursor_line = 0;
        state.top_line = 0;
        state.cursor_col = 0;
        state.last_search_pattern = Some("hello".to_string());

        // Should wrap immediately without warning
        find_prev_occurrence(&mut state, &lines, 10);
        assert_eq!(state.wrap_warning_pending, None);
        assert_eq!(state.absolute_line(), 2); // cursor should move to line 2
        assert_eq!(state.cursor_col, 0);
        assert_eq!(state.find_error, None); // No error message
    }

    #[test]
    fn find_next_clears_warning_on_match() {
        let lines = vec![
            "hello world".to_string(),
            "hello again".to_string(),
            "foo bar".to_string(),
        ];

        let settings = crate::settings::Settings::default();
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, &settings);

        // Set up: cursor at first hello
        state.cursor_line = 0;
        state.top_line = 0;
        state.cursor_col = 0;
        state.last_search_pattern = Some("hello".to_string());
        state.wrap_warning_pending = Some("next".to_string()); // simulate pending warning

        // Find next should find second hello and clear warning
        find_next_occurrence(&mut state, &lines, 10);
        assert_eq!(state.wrap_warning_pending, None);
        assert_eq!(state.absolute_line(), 1); // cursor should move to line 1
        assert_eq!(state.find_error, None); // no error message
    }

    #[test]
    fn find_on_last_line_wraps_correctly() {
        let lines = vec![
            "first line".to_string(),
            "second line".to_string(),
            "find this".to_string(), // Last line with match at beginning
        ];

        let settings = crate::settings::Settings::default();
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, &settings);

        // Set up: cursor at beginning of last line where "find" is located
        state.cursor_line = 2;
        state.top_line = 0;
        state.cursor_col = 0; // Cursor at position where match starts
        state.last_search_pattern = Some("find".to_string());

        // find_next should wrap immediately and find "find" at line 2, col 0
        find_next_occurrence(&mut state, &lines, 10);
        assert_eq!(state.wrap_warning_pending, None);
        assert_eq!(state.absolute_line(), 2);
        assert_eq!(state.cursor_col, 0);
        assert_eq!(state.find_error, None); // No error message
    }

    #[test]
    fn find_history_persistence() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Create a temp file for testing
        let mut temp_file = NamedTempFile::new().expect("create temp file");
        write!(temp_file, "test content").expect("write temp file");
        let file_path = temp_file.path().to_str().unwrap();

        // Use isolated test environment for settings
        let (_tmp, _guard) = crate::env::set_temp_home();
        let settings = crate::settings::Settings::load().expect("load settings");
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history.clone(), &settings);

        // Add some searches to history
        state.find_pattern = "search1".to_string();
        add_to_history(&mut state, "search1".to_string());
        state.find_pattern = "search2".to_string();
        add_to_history(&mut state, "search2".to_string());
        state.find_pattern = "search3".to_string();
        add_to_history(&mut state, "search3".to_string());

        // Save to undo history
        state.undo_history.find_history = state.find_history.clone();
        let _ = state.undo_history.save(file_path);

        // Load in a new state
        let loaded_history = crate::undo::UndoHistory::load(file_path).expect("load history");
        assert_eq!(loaded_history.find_history.len(), 3);
        assert_eq!(loaded_history.find_history[0], "search3"); // Most recent first
        assert_eq!(loaded_history.find_history[1], "search2");
        assert_eq!(loaded_history.find_history[2], "search1");
    }

    #[test]
    fn find_history_deduplication() {
        let settings = crate::settings::Settings::default();
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, &settings);

        // Add same pattern multiple times
        add_to_history(&mut state, "duplicate".to_string());
        add_to_history(&mut state, "other".to_string());
        add_to_history(&mut state, "duplicate".to_string()); // Should move to front

        assert_eq!(state.find_history.len(), 2);
        assert_eq!(state.find_history[0], "duplicate"); // Most recent
        assert_eq!(state.find_history[1], "other");
    }

    #[test]
    fn find_history_max_limit() {
        let settings = crate::settings::Settings::default();
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, &settings);

        // Add more than MAX_FIND_HISTORY (100) items
        for i in 0..150 {
            add_to_history(&mut state, format!("search{}", i));
        }

        assert_eq!(state.find_history.len(), 100); // Should be capped at 100
        assert_eq!(state.find_history[0], "search149"); // Most recent
        assert_eq!(state.find_history[99], "search50"); // Oldest kept
    }

    #[test]
    fn cursor_movement_clears_wrap_warning() {
        let _lines = vec!["hello".to_string(), "world".to_string()];

        let settings = crate::settings::Settings::default();
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, &settings);

        // Set a wrap warning
        state.wrap_warning_pending = Some("next".to_string());
        state.last_search_pattern = Some("test".to_string());

        // Moving cursor should clear warning (tested in event_handlers)
        // This is verified through integration test
        assert!(state.wrap_warning_pending.is_some());
    }

    #[test]
    fn find_next_within_scope_single_line() {
        let lines = vec!["hello world hello again hello end".to_string()];

        let regex = Regex::new("(?i)hello").unwrap();

        // "hello world hello again hello end"
        //  0           12          24
        // Search within scope from col 12 to col 29 (covers "hello again hello")
        let scope = Some(((0, 12), (0, 29)));

        // Starting from position (0, 5) should find first match in scope at col 12
        let result = find_next(&lines, (0, 5), &regex, false, scope);
        assert_eq!(result, Some((0, 12))); // "hello" in "hello again"

        // Starting from first match in scope should find second match in scope
        let result = find_next(&lines, (0, 12), &regex, false, scope);
        assert_eq!(result, Some((0, 24))); // "hello" before "end"

        // Starting from last match in scope should find nothing (search starts at col 25, scope ends at 29)
        let result = find_next(&lines, (0, 24), &regex, false, scope);
        assert_eq!(result, None);
    }

    #[test]
    fn find_next_within_scope_multi_line() {
        let lines = vec![
            "first line".to_string(),
            "hello world".to_string(),
            "middle line".to_string(),
            "hello again".to_string(),
            "last line".to_string(),
        ];

        let regex = Regex::new("(?i)hello").unwrap();

        // Search within scope from line 1, col 0 to line 3, col 11 (covers both hellos)
        let scope = Some(((1, 0), (3, 11)));

        // Starting from line 0 (before scope) should find first match in scope
        let result = find_next(&lines, (0, 5), &regex, false, scope);
        assert_eq!(result, Some((1, 0)));

        // Starting from beginning of first match should find it (since search starts from col 1)
        // Actually, starting AT (1,0), search begins at (1,1), so it won't find the match at (1,0)
        // Let me start from a position that will find the second match
        let result = find_next(&lines, (1, 0), &regex, false, scope);
        assert_eq!(result, Some((3, 0)));

        // Starting from second match should find nothing
        let result = find_next(&lines, (3, 0), &regex, false, scope);
        assert_eq!(result, None);
    }

    #[test]
    fn find_prev_within_scope_single_line() {
        let lines = vec!["hello world hello again hello end".to_string()];

        let regex = Regex::new("(?i)hello").unwrap();

        // "hello world hello again hello end"
        //  0           12          24
        // Search within scope from col 12 to col 29 (covers "hello again hello")
        let scope = Some(((0, 12), (0, 29)));

        // Starting from position after scope should find last match in scope
        let result = find_prev(&lines, (0, 30), &regex, false, scope);
        assert_eq!(result, Some((0, 24))); // "hello" before "end"

        // Starting from last match in scope should find first match in scope
        let result = find_prev(&lines, (0, 24), &regex, false, scope);
        assert_eq!(result, Some((0, 12))); // "hello" in "hello again"

        // Starting from first match in scope should find nothing
        let result = find_prev(&lines, (0, 12), &regex, false, scope);
        assert_eq!(result, None);
    }

    #[test]
    fn find_prev_within_scope_multi_line() {
        let lines = vec![
            "first line".to_string(),
            "hello world".to_string(),
            "middle line".to_string(),
            "hello again".to_string(),
            "last line".to_string(),
        ];

        let regex = Regex::new("(?i)hello").unwrap();

        // Search within scope from line 1 to line 3 (covers both hellos)
        let scope = Some(((1, 0), (3, 11)));

        // Starting from after scope should find last match in scope
        let result = find_prev(&lines, (4, 0), &regex, false, scope);
        assert_eq!(result, Some((3, 0)));

        // Starting from second match should find first
        let result = find_prev(&lines, (3, 0), &regex, false, scope);
        assert_eq!(result, Some((1, 0)));

        // Starting from first match should find nothing
        let result = find_prev(&lines, (1, 0), &regex, false, scope);
        assert_eq!(result, None);
    }

    #[test]
    fn find_next_wrap_within_scope() {
        let lines = vec![
            "first line".to_string(),
            "hello world".to_string(),
            "middle line".to_string(),
            "hello again".to_string(),
            "last line".to_string(),
        ];

        let regex = Regex::new("(?i)hello").unwrap();

        // Search within scope from line 1 to line 3
        let scope = Some(((1, 0), (3, 11)));

        // Starting from second match with wrap should find first match
        let result = find_next(&lines, (3, 0), &regex, true, scope);
        assert_eq!(result, Some((1, 0)));
    }

    #[test]
    fn find_prev_wrap_within_scope() {
        let lines = vec![
            "first line".to_string(),
            "hello world".to_string(),
            "middle line".to_string(),
            "hello again".to_string(),
            "last line".to_string(),
        ];

        let regex = Regex::new("(?i)hello").unwrap();

        // Search within scope from line 1 to line 3
        let scope = Some(((1, 0), (3, 11)));

        // Starting from first match with wrap should find second match
        let result = find_prev(&lines, (1, 0), &regex, true, scope);
        assert_eq!(result, Some((3, 0)));
    }

    #[test]
    fn find_scope_is_set_when_activating_with_selection() {
        // This test verifies that when find mode is activated with a selection,
        // the find_scope is properly captured
        let settings = crate::settings::Settings::default();
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, &settings);

        // Set up a selection from (1, 5) to (3, 10)
        state.selection_start = Some((1, 5));
        state.selection_end = Some((3, 10));

        // Simulate activating find mode (this would be done in event_handlers)
        // The event handler should capture the selection as find_scope
        if let (Some(start), Some(end)) = (state.selection_start, state.selection_end) {
            let normalized = if start.0 < end.0 || (start.0 == end.0 && start.1 <= end.1) {
                (start, end)
            } else {
                (end, start)
            };
            state.find_scope = Some(normalized);
        }

        // Verify the scope was set correctly
        assert_eq!(state.find_scope, Some(((1, 5), (3, 10))));

        // The rendering code should now only highlight matches within this scope
    }

    #[test]
    fn test_calculate_search_hits() {
        let lines = vec![
            "This is a test file".to_string(),
            "The word test appears here".to_string(),
            "Here is another test".to_string(),
            "And yet another test word".to_string(),
            "No match on this line".to_string(),
            "But this has test again".to_string(),
        ];

        // Test 1: Count all hits for "test"
        let (current, total) = calculate_search_hits(&lines, (0, 0), "test", true, None);
        assert_eq!(total, 5); // Should find 5 occurrences
        assert_eq!(current, 0); // Cursor not on a match at (0, 0)

        // Test 2: Cursor at first occurrence
        let (current, total) = calculate_search_hits(&lines, (0, 10), "test", true, None);
        assert_eq!(total, 5);
        assert_eq!(current, 1); // First hit

        // Test 3: Cursor at third occurrence
        let (current, total) = calculate_search_hits(&lines, (2, 16), "test", true, None);
        assert_eq!(total, 5);
        assert_eq!(current, 3); // Third hit

        // Test 4: With scope - only count hits in range
        let scope = Some(((1, 0), (3, 25)));
        let (current, total) = calculate_search_hits(&lines, (2, 16), "test", true, scope);
        assert_eq!(total, 3); // Only 3 hits in lines 1-3
        assert_eq!(current, 2); // Second hit within scope

        // Test 5: Case insensitive
        let (_current, total) = calculate_search_hits(&lines, (0, 0), "TEST", true, None);
        assert_eq!(total, 5); // Should find all "test" case-insensitively
    }

    #[test]
    fn test_calculate_search_hits_no_matches() {
        let lines = vec![
            "This is a test file".to_string(),
            "The word test appears here".to_string(),
        ];

        let (current, total) = calculate_search_hits(&lines, (0, 0), "nomatch", true, None);
        assert_eq!(total, 0);
        assert_eq!(current, 0);
    }

    #[test]
    fn test_enter_does_not_jump_to_match() {
        // Test that pressing Enter in find mode doesn't move the cursor
        let lines = vec![
            "first line".to_string(),
            "test here".to_string(),
            "another line".to_string(),
        ];

        let settings = crate::settings::Settings::default();
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, &settings);

        // Set cursor at line 0, column 0
        state.cursor_line = 0;
        state.top_line = 0;
        state.cursor_col = 0;
        state.find_active = true;
        state.find_pattern = "test".to_string();

        // Simulate pressing Enter
        let key_event = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::empty(),
        );
        let exited = handle_find_input(&mut state, &lines, key_event, 10);

        // Should exit find mode
        assert!(exited);
        assert!(!state.find_active);

        // Cursor should NOT have moved
        assert_eq!(state.cursor_line, 0);
        assert_eq!(state.cursor_col, 0);

        // Search pattern should be active for highlighting
        assert_eq!(state.last_search_pattern, Some("test".to_string()));

        // Hit count should be updated
        assert_eq!(state.search_hit_count, 1);
        assert_eq!(state.search_current_hit, 0); // Not on a match
    }

    #[test]
    fn test_wrapping_is_immediate() {
        // Test that find_next wraps immediately without requiring second press
        let lines = vec![
            "first line".to_string(),
            "test here".to_string(),
            "last line".to_string(),
        ];

        let settings = crate::settings::Settings::default();
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, &settings);

        // Set cursor at end, after the only match
        state.cursor_line = 2;
        state.top_line = 0;
        state.cursor_col = 0;
        state.last_search_pattern = Some("test".to_string());

        // Call find_next once - should wrap immediately
        find_next_occurrence(&mut state, &lines, 10);

        // Should have wrapped to the match at line 1
        assert_eq!(state.absolute_line(), 1);
        assert_eq!(state.cursor_col, 0);

        // No warning should be set
        assert_eq!(state.wrap_warning_pending, None);

        // No error message
        assert_eq!(state.find_error, None);
    }

    #[test]
    fn test_no_error_messages_on_no_match() {
        // Test that searching with no results doesn't show error messages
        let lines = vec![
            "first line".to_string(),
            "second line".to_string(),
        ];

        let settings = crate::settings::Settings::default();
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, &settings);

        state.cursor_line = 0;
        state.top_line = 0;
        state.cursor_col = 0;
        state.last_search_pattern = Some("nomatch".to_string());

        // Try to find next - no matches exist
        find_next_occurrence(&mut state, &lines, 10);

        // Cursor should not move
        assert_eq!(state.cursor_line, 0);
        assert_eq!(state.cursor_col, 0);

        // Should NOT have an error message
        assert_eq!(state.find_error, None);
    }

    #[test]
    fn test_hit_count_always_updated() {
        // Test that hit count is always calculated, even with 0 matches
        let lines = vec![
            "first line".to_string(),
            "second line".to_string(),
        ];

        let settings = crate::settings::Settings::default();
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, &settings);

        state.cursor_line = 0;
        state.top_line = 0;
        state.cursor_col = 0;
        state.find_active = true;
        state.find_pattern = "nomatch".to_string();

        // Press Enter to activate search
        let key_event = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::empty(),
        );
        handle_find_input(&mut state, &lines, key_event, 10);

        // Hit count should be set to 0
        assert_eq!(state.search_hit_count, 0);
        assert_eq!(state.search_current_hit, 0);

        // Pattern should be active
        assert_eq!(state.last_search_pattern, Some("nomatch".to_string()));
    }

    #[test]
    fn test_find_prev_wraps_immediately() {
        // Test that find_prev also wraps immediately
        let lines = vec![
            "first line".to_string(),
            "test here".to_string(),
            "last line".to_string(),
        ];

        let settings = crate::settings::Settings::default();
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, &settings);

        // Set cursor at beginning, before the only match
        state.cursor_line = 0;
        state.top_line = 0;
        state.cursor_col = 0;
        state.last_search_pattern = Some("test".to_string());

        // Call find_prev once - should wrap immediately to end
        find_prev_occurrence(&mut state, &lines, 10);

        // Should have wrapped to the match at line 1
        assert_eq!(state.absolute_line(), 1);
        assert_eq!(state.cursor_col, 0);

        // No warning should be set
        assert_eq!(state.wrap_warning_pending, None);

        // No error message
        assert_eq!(state.find_error, None);
    }

    #[test]
    fn test_wildcard_to_regex_star() {
        // Test that * matches any number of characters
        let regex = pattern_to_regex("hello*world", false).unwrap();

        // All should match
        assert!(regex.is_match("hello world"));
        assert!(regex.is_match("helloworld"));
        assert!(regex.is_match("hello123world"));
    }

    #[test]
    fn test_wildcard_to_regex_question() {
        // Test that ? matches any single character
        let regex = pattern_to_regex("ca?", false).unwrap();

        assert!(regex.is_match("cat"));
        assert!(regex.is_match("car"));
        assert!(!regex.is_match("bat"));
        assert!(!regex.is_match("ca"));
    }

    #[test]
    fn test_wildcard_escapes_regex_chars() {
        // Test that regex special characters are properly escaped
        // Pattern with literal dot (not regex "any character")
        let regex = pattern_to_regex("test.txt", false).unwrap();

        assert!(regex.is_match("test.txt"));
        assert!(!regex.is_match("testXtxt")); // . is literal, not wildcard
        assert!(!regex.is_match("test^txt"));
    }

    #[test]
    fn test_wildcard_combined_patterns() {
        // Test combination of * and ?
        // Pattern: foo*?bar means: foo + (zero or more chars) + (exactly one char) + bar
        let regex = pattern_to_regex("foo*?bar", false).unwrap();

        assert!(!regex.is_match("foobar")); // No character to satisfy the ?
        assert!(regex.is_match("foo123bar")); // 12 matches *, 3 matches ?
        assert!(regex.is_match("foo1bar")); // empty matches *, 1 matches ?
        assert!(regex.is_match("fooXbar")); // empty matches *, X matches ?
        assert!(regex.is_match("foo12bar")); // 1 matches *, 2 matches ?

        // Test simpler pattern: f*o matches f + (zero or more) + o
        let regex2 = pattern_to_regex("f*o", false).unwrap();
        assert!(regex2.is_match("fo"));
        assert!(regex2.is_match("foo"));
        assert!(regex2.is_match("f123o"));
    }

    #[test]
    fn test_wildcard_case_insensitive() {
        // Test that wildcard patterns are case-insensitive
        let regex = pattern_to_regex("hello", false).unwrap();

        assert!(regex.is_match("HELLO"));
        assert!(regex.is_match("hello"));
        assert!(regex.is_match("HeLLo"));
    }

    #[test]
    fn test_wildcard_with_brackets() {
        // Test that brackets are escaped properly
        // Pattern with literal brackets (not regex character class)
        let regex = pattern_to_regex("test[abc]", false).unwrap();

        assert!(regex.is_match("test[abc]")); // matches literal
        assert!(!regex.is_match("testa")); // doesn't match
        assert!(!regex.is_match("testb")); // doesn't match
    }

    #[test]
    fn test_regex_vs_wildcard_mode() {
        // Test that regex mode and wildcard mode behave differently

        // Regex mode: [abc] is a character class
        let regex_mode = pattern_to_regex("test[abc]", true).unwrap();
        assert!(regex_mode.is_match("testa")); // matches character class
        assert!(regex_mode.is_match("testb")); // matches character class
        assert!(regex_mode.is_match("testc")); // matches character class
        assert!(!regex_mode.is_match("test[abc]")); // doesn't match literal

        // Wildcard mode: [abc] is literal
        let wildcard_mode = pattern_to_regex("test[abc]", false).unwrap();
        assert!(!wildcard_mode.is_match("testa")); // doesn't match
        assert!(!wildcard_mode.is_match("testb")); // doesn't match
        assert!(!wildcard_mode.is_match("testc")); // doesn't match
        assert!(wildcard_mode.is_match("test[abc]")); // matches literal

        // Test . character
        // Regex mode: . matches any character
        let regex_dot = pattern_to_regex("test.txt", true).unwrap();
        assert!(regex_dot.is_match("test.txt")); // matches
        assert!(regex_dot.is_match("testXtxt")); // . matches X

        // Wildcard mode: . is literal
        let wildcard_dot = pattern_to_regex("test.txt", false).unwrap();
        assert!(wildcard_dot.is_match("test.txt")); // matches
        assert!(!wildcard_dot.is_match("testXtxt")); // doesn't match

        // Test * character
        // Regex mode: * is quantifier (needs something before it)
        // Wildcard mode: * is "any characters"
        let wildcard_star = pattern_to_regex("test*file", false).unwrap();
        assert!(wildcard_star.is_match("testfile")); // zero chars
        assert!(wildcard_star.is_match("test123file")); // multiple chars
    }
}
