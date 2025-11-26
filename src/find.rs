use crossterm::event::{KeyCode, KeyEvent};
use regex::Regex;

use crate::editor_state::{FileViewerState, Position};

const MAX_FIND_HISTORY: usize = 100;

/// Handle find mode key events
/// Returns true if find mode should exit
pub(crate) fn handle_find_input(
    state: &mut FileViewerState,
    lines: &[String],
    key_event: KeyEvent,
    visible_lines: usize,
) -> bool {
    let KeyEvent { code, .. } = key_event;
    
    match code {
        KeyCode::Esc => {
            // Exit find mode and restore previous search highlights
            state.find_active = false;
            state.find_pattern.clear();
            state.find_error = None;
            state.find_history_index = None;
            // Restore the search pattern from before entering find mode
            state.last_search_pattern = state.saved_search_pattern.clone();
            state.saved_search_pattern = None;
            state.needs_redraw = true;
            true
        }
        KeyCode::Enter => {
            // Perform search and exit find mode
            if !state.find_pattern.is_empty() {
                match Regex::new(&state.find_pattern) {
                    Ok(regex) => {
                        // Always set last_search_pattern for highlighting
                        state.last_search_pattern = Some(state.find_pattern.clone());
                        add_to_history(state, state.find_pattern.clone());
                        
                        if let Some(pos) = find_next(lines, state.current_position(), &regex, false) {
                            move_to_position(state, pos, lines.len(), visible_lines);
                            state.search_wrapped = false;
                            state.wrap_warning_pending = None;
                            state.find_error = None;
                        } else {
                            // No match found forward - show message
                            state.find_error = Some("No matches found forward".to_string());
                            state.wrap_warning_pending = None;
                        }
                        state.find_active = false;
                        state.find_history_index = None;
                        state.saved_search_pattern = None;  // Clear saved pattern
                        state.needs_redraw = true;
                    }
                    Err(e) => {
                        state.find_error = Some(format!("Invalid regex: {}", e));
                        state.needs_redraw = true;
                        return false;
                    }
                }
            } else {
                // Empty search - clear highlights
                state.find_active = false;
                state.find_error = None;
                state.find_history_index = None;
                state.wrap_warning_pending = None;
                state.last_search_pattern = None;  // Clear highlights
                state.saved_search_pattern = None;  // Clear saved pattern
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
                state.find_error = None;
                state.find_history_index = None;
                // Update highlights in real-time
                update_live_highlights(state);
                state.needs_redraw = true;
            }
            false
        }
        KeyCode::Left => {
            if state.find_cursor_pos > 0 {
                state.find_cursor_pos -= 1;
                state.needs_redraw = true;
            }
            false
        }
        KeyCode::Right => {
            let pattern_len = state.find_pattern.chars().count();
            if state.find_cursor_pos < pattern_len {
                state.find_cursor_pos += 1;
                state.needs_redraw = true;
            }
            false
        }
        KeyCode::Home => {
            state.find_cursor_pos = 0;
            state.needs_redraw = true;
            false
        }
        KeyCode::End => {
            state.find_cursor_pos = state.find_pattern.chars().count();
            state.needs_redraw = true;
            false
        }
        KeyCode::Char(c) => {
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
            state.find_error = None;
            state.find_history_index = None;
            // Update highlights in real-time
            update_live_highlights(state);
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
        if let Ok(regex) = Regex::new(pattern) {
            if let Some(pos) = find_next(lines, state.current_position(), &regex, false) {
                // Found a match without wrapping
                move_to_position(state, pos, lines.len(), visible_lines);
                state.search_wrapped = false;
                state.wrap_warning_pending = None;
                state.find_error = None;
            } else {
                // No match found in forward direction
                // Check if we have a pending wrap warning for next
                if state.wrap_warning_pending.as_deref() == Some("next") {
                    // Second press - actually wrap
                    if let Some(pos) = find_next(lines, state.current_position(), &regex, true) {
                        move_to_position(state, pos, lines.len(), visible_lines);
                        state.search_wrapped = true;
                        state.wrap_warning_pending = None;
                        state.find_error = Some("Search wrapped to beginning".to_string());
                    } else {
                        state.find_error = Some("No matches found".to_string());
                        state.wrap_warning_pending = None;
                    }
                } else {
                    // First press - show warning
                    state.wrap_warning_pending = Some("next".to_string());
                    state.find_error = Some("Press again to wrap to beginning".to_string());
                }
            }
        }
    } else {
        state.find_error = Some("No previous search".to_string());
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
        if let Ok(regex) = Regex::new(pattern) {
            if let Some(pos) = find_prev(lines, state.current_position(), &regex, false) {
                // Found a match without wrapping
                move_to_position(state, pos, lines.len(), visible_lines);
                state.search_wrapped = false;
                state.wrap_warning_pending = None;
                state.find_error = None;
            } else {
                // No match found in backward direction
                // Check if we have a pending wrap warning for prev
                if state.wrap_warning_pending.as_deref() == Some("prev") {
                    // Second press - actually wrap
                    if let Some(pos) = find_prev(lines, state.current_position(), &regex, true) {
                        move_to_position(state, pos, lines.len(), visible_lines);
                        state.search_wrapped = true;
                        state.wrap_warning_pending = None;
                        state.find_error = Some("Search wrapped to end".to_string());
                    } else {
                        state.find_error = Some("No matches found".to_string());
                        state.wrap_warning_pending = None;
                    }
                } else {
                    // First press - show warning
                    state.wrap_warning_pending = Some("prev".to_string());
                    state.find_error = Some("Press again to wrap to end".to_string());
                }
            }
        }
    } else {
        state.find_error = Some("No previous search".to_string());
    }
    state.needs_redraw = true;
}

/// Update live highlights based on current find pattern
fn update_live_highlights(state: &mut FileViewerState) {
    if state.find_pattern.is_empty() {
        // Clear highlights when pattern is empty
        state.last_search_pattern = None;
    } else {
        // Try to compile as regex - if valid, update highlights
        if Regex::new(&state.find_pattern).is_ok() {
            state.last_search_pattern = Some(state.find_pattern.clone());
            state.find_error = None;
        } else {
            // Invalid regex - don't update highlights but don't show error yet
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
fn find_next(lines: &[String], start: Position, regex: &Regex, force_wrap: bool) -> Option<Position> {
    let (start_line, start_col) = start;
    
    if !force_wrap {
        // Search from current position to end of current line
        if start_line < lines.len() {
            let line = &lines[start_line];
            // Start searching from next character position
            let search_from = start_col + 1;
            if search_from < line.len()
                && let Some(m) = regex.find(&line[search_from..]) {
                    return Some((start_line, search_from + m.start()));
                }
        }
        
        // Search remaining lines
        for (line_idx, line) in lines.iter().enumerate().skip(start_line + 1) {
            if let Some(m) = regex.find(line) {
                return Some((line_idx, m.start()));
            }
        }
        
        // Don't wrap when force_wrap is false
        return None;
    }
    
    // Wrap around to beginning (only when force_wrap is true)
    for (line_idx, line) in lines.iter().enumerate().take(start_line + 1) {
        if let Some(m) = regex.find(line) {
            return Some((line_idx, m.start()));
        }
    }
    
    None
}

/// Find the previous occurrence of the pattern starting from the given position
fn find_prev(lines: &[String], start: Position, regex: &Regex, force_wrap: bool) -> Option<Position> {
    let (start_line, start_col) = start;
    
    if !force_wrap {
        // Search backwards in current line
        if start_line < lines.len() {
            let line = &lines[start_line];
            // Find all matches in current line before cursor
            let mut last_match: Option<usize> = None;
            for m in regex.find_iter(line) {
                if m.start() < start_col {
                    last_match = Some(m.start());
                } else {
                    break;
                }
            }
            if let Some(col) = last_match {
                return Some((start_line, col));
            }
        }
        
        // Search previous lines (reverse order)
        for line_idx in (0..start_line).rev() {
            let line = &lines[line_idx];
            // Find last match in this line
            if let Some(last_match) = regex.find_iter(line).last() {
                return Some((line_idx, last_match.start()));
            }
        }
        
        // Don't wrap when force_wrap is false
        return None;
    }
    
    // Wrap around to end (only when force_wrap is true)
    for line_idx in (start_line..lines.len()).rev() {
        let line = &lines[line_idx];
        // Find last match in this line
        if let Some(last_match) = regex.find_iter(line).last() {
            return Some((line_idx, last_match.start()));
        }
    }
    
    None
}

/// Move cursor to the specified position, adjusting viewport if needed
fn move_to_position(state: &mut FileViewerState, pos: Position, total_lines: usize, visible_lines: usize) {
    let (target_line, target_col) = pos;
    
    if target_line >= total_lines {
        return;
    }
    
    // Clear any off-screen saved cursor
    state.saved_absolute_cursor = None;
    state.saved_scroll_state = None;
    
    // Update cursor position
    state.cursor_col = target_col;
    
    // Check if we need to scroll the viewport
    if target_line < state.top_line {
        // Target is above viewport - scroll up
        state.top_line = target_line;
        state.cursor_line = 0;
    } else if target_line >= state.top_line + visible_lines {
        // Target is below viewport - scroll down
        state.top_line = target_line.saturating_sub(visible_lines / 2);
        state.cursor_line = target_line - state.top_line;
    } else {
        // Target is within viewport
        state.cursor_line = target_line - state.top_line;
    }
    
    state.needs_redraw = true;
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
        
        let regex = Regex::new("hello").unwrap();
        
        // Find next from position (0, 0) should skip current position and find next occurrence
        let result = find_next(&lines, (0, 0), &regex, false);
        assert_eq!(result, Some((2, 0)));
        
        // Find next from end of first "hello" should wrap around
        let result = find_next(&lines, (0, 4), &regex, false);
        assert_eq!(result, Some((2, 0)));
        
        // Find from line 1 should find line 2
        let result = find_next(&lines, (1, 0), &regex, false);
        assert_eq!(result, Some((2, 0)));
    }
    
    #[test]
    fn find_wraps_around() {
        let lines = vec![
            "hello world".to_string(),
            "foo bar".to_string(),
        ];
        
        let regex = Regex::new("hello").unwrap();
        let result = find_next(&lines, (1, 5), &regex, true);
        assert_eq!(result, Some((0, 0)));
    }
    
    #[test]
    fn find_no_match() {
        let lines = vec![
            "hello world".to_string(),
            "foo bar".to_string(),
        ];
        
        let regex = Regex::new("notfound").unwrap();
        let result = find_next(&lines, (0, 0), &regex, false);
        assert_eq!(result, None);
    }
    
    #[test]
    fn find_next_two_press_wrap() {
        let lines = vec![
            "hello world".to_string(),
            "foo bar".to_string(),
            "baz qux".to_string(),
        ];
        
        let settings = Box::leak(Box::new(crate::settings::Settings::load().expect("Failed to load test settings")));
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);
        
        // Set up: cursor at end, pattern "hello" (which is at the beginning)
        state.cursor_line = 2;
        state.top_line = 0;
        state.cursor_col = 0;
        state.last_search_pattern = Some("hello".to_string());
        
        // First press: should show warning, not wrap
        find_next_occurrence(&mut state, &lines, 10);
        assert_eq!(state.wrap_warning_pending, Some("next".to_string()));
        assert_eq!(state.find_error, Some("Press again to wrap to beginning".to_string()));
        assert_eq!(state.cursor_line, 2); // cursor should not move
        
        // Second press: should actually wrap
        find_next_occurrence(&mut state, &lines, 10);
        assert_eq!(state.wrap_warning_pending, None);
        assert_eq!(state.absolute_line(), 0); // cursor should move to line 0
        assert_eq!(state.cursor_col, 0);
        assert_eq!(state.find_error, Some("Search wrapped to beginning".to_string()));
    }
    
    #[test]
    fn find_prev_two_press_wrap() {
        let lines = vec![
            "foo bar".to_string(),
            "baz qux".to_string(),
            "hello world".to_string(),
        ];
        
        let settings = Box::leak(Box::new(crate::settings::Settings::load().expect("Failed to load test settings")));
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);
        
        // Set up: cursor at beginning, pattern "hello" (which is at the end)
        state.cursor_line = 0;
        state.top_line = 0;
        state.cursor_col = 0;
        state.last_search_pattern = Some("hello".to_string());
        
        // First press: should show warning, not wrap
        find_prev_occurrence(&mut state, &lines, 10);
        assert_eq!(state.wrap_warning_pending, Some("prev".to_string()));
        assert_eq!(state.find_error, Some("Press again to wrap to end".to_string()));
        assert_eq!(state.cursor_line, 0); // cursor should not move
        
        // Second press: should actually wrap
        find_prev_occurrence(&mut state, &lines, 10);
        assert_eq!(state.wrap_warning_pending, None);
        assert_eq!(state.absolute_line(), 2); // cursor should move to line 2
        assert_eq!(state.cursor_col, 0);
        assert_eq!(state.find_error, Some("Search wrapped to end".to_string()));
    }
    
    #[test]
    fn find_next_clears_warning_on_match() {
        let lines = vec![
            "hello world".to_string(),
            "hello again".to_string(),
            "foo bar".to_string(),
        ];
        
        let settings = Box::leak(Box::new(crate::settings::Settings::load().expect("Failed to load test settings")));
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);
        
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
            "find this".to_string(),  // Last line with match at beginning
        ];
        
        let settings = Box::leak(Box::new(crate::settings::Settings::load().expect("Failed to load test settings")));
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);
        
        // Set up: cursor at beginning of last line where "find" is located
        state.cursor_line = 2;
        state.top_line = 0;
        state.cursor_col = 0;  // Cursor at position where match starts
        state.last_search_pattern = Some("find".to_string());
        
        // First find_next should not find anything forward (starts search from col 1)
        find_next_occurrence(&mut state, &lines, 10);
        // Should show wrap warning
        assert_eq!(state.wrap_warning_pending, Some("next".to_string()));
        assert_eq!(state.cursor_line, 2); // cursor should not move
        
        // Second press should wrap and find "find" at line 2, col 0
        find_next_occurrence(&mut state, &lines, 10);
        assert_eq!(state.wrap_warning_pending, None);
        assert_eq!(state.absolute_line(), 2);
        assert_eq!(state.cursor_col, 0);
        assert_eq!(state.find_error, Some("Search wrapped to beginning".to_string()));
    }
}

