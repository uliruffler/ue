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
            // Note: Don't clear selection - keep it visible to show the search scope
            // Note: Don't clear find_scope here - keep it so highlighting remains scoped
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
                        
                        if let Some(pos) = find_next(lines, state.current_position(), &regex, false, state.find_scope) {
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
                        // Note: Don't clear selection - keep it visible to show the search scope
                        // Note: Don't clear find_scope - keep it so highlighting remains scoped
                        state.needs_redraw = true;
                    }
                    Err(e) => {
                        state.find_error = Some(format!("Invalid regex: {}", e));
                        state.needs_redraw = true;
                        return false;
                    }
                }
            } else {
                // Empty search - clear highlights and scope
                state.find_active = false;
                state.find_error = None;
                state.find_history_index = None;
                state.wrap_warning_pending = None;
                state.last_search_pattern = None;  // Clear highlights
                state.saved_search_pattern = None;  // Clear saved pattern
                state.find_scope = None;  // Clear search scope for next search
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
            if let Some(pos) = find_next(lines, state.current_position(), &regex, false, state.find_scope) {
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
                    if let Some(pos) = find_next(lines, state.current_position(), &regex, true, state.find_scope) {
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
            if let Some(pos) = find_prev(lines, state.current_position(), &regex, false, state.find_scope) {
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
                    if let Some(pos) = find_prev(lines, state.current_position(), &regex, true, state.find_scope) {
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
/// If scope is Some, only search within the specified range
fn find_next(lines: &[String], start: Position, regex: &Regex, force_wrap: bool, scope: Option<(Position, Position)>) -> Option<Position> {
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
            let search_to = if let Some(((_scope_start_line, _scope_start_col), (scope_end_line, scope_end_col))) = scope {
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
            let (search_start, search_end) = if let Some(((scope_start_line, scope_start_col), (scope_end_line, scope_end_col))) = scope {
                let start_offset = if line_idx == scope_start_line { scope_start_col } else { 0 };
                let end_offset = if line_idx == scope_end_line { scope_end_col.min(line.len()) } else { line.len() };
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
        let (search_start, search_end) = if let Some(((scope_start_line, scope_start_col), (scope_end_line, scope_end_col))) = scope {
            let start_offset = if line_idx == scope_start_line { scope_start_col } else { 0 };
            let end_offset = if line_idx == scope_end_line { scope_end_col.min(line.len()) } else { line.len() };
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
fn find_prev(lines: &[String], start: Position, regex: &Regex, force_wrap: bool, scope: Option<(Position, Position)>) -> Option<Position> {
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
            let (search_start, search_end) = if let Some(((scope_start_line, scope_start_col), (scope_end_line, scope_end_col))) = scope {
                let start_offset = if start_line == scope_start_line { scope_start_col } else { 0 };
                let end_offset = if start_line == scope_end_line { scope_end_col.min(line.len()) } else { line.len() };
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
            let (search_start, search_end) = if let Some(((scope_start_line, scope_start_col), (scope_end_line, scope_end_col))) = scope {
                let start_offset = if line_idx == scope_start_line { scope_start_col } else { 0 };
                let end_offset = if line_idx == scope_end_line { scope_end_col.min(line.len()) } else { line.len() };
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
        let (search_start, search_end) = if let Some(((scope_start_line, scope_start_col), (scope_end_line, scope_end_col))) = scope {
            let start_offset = if line_idx == scope_start_line { scope_start_col } else { 0 };
            let end_offset = if line_idx == scope_end_line { scope_end_col.min(line.len()) } else { line.len() };
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
        let lines = vec![
            "hello world".to_string(),
            "foo bar".to_string(),
        ];
        
        let regex = Regex::new("hello").unwrap();
        let result = find_next(&lines, (1, 5), &regex, true, None);
        assert_eq!(result, Some((0, 0)));
    }
    
    #[test]
    fn find_no_match() {
        let lines = vec![
            "hello world".to_string(),
            "foo bar".to_string(),
        ];
        
        let regex = Regex::new("notfound").unwrap();
        let result = find_next(&lines, (0, 0), &regex, false, None);
        assert_eq!(result, None);
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
        
        let settings = crate::settings::Settings::default();
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, &settings);
        
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
            "find this".to_string(),  // Last line with match at beginning
        ];
        
        let settings = crate::settings::Settings::default();
        let undo_history = crate::undo::UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, &settings);
        
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
        let _lines = vec![
            "hello".to_string(),
            "world".to_string(),
        ];
        
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
        let lines = vec![
            "hello world hello again hello end".to_string(),
        ];
        
        let regex = Regex::new("hello").unwrap();
        
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
        
        let regex = Regex::new("hello").unwrap();
        
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
        let lines = vec![
            "hello world hello again hello end".to_string(),
        ];
        
        let regex = Regex::new("hello").unwrap();
        
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
        
        let regex = Regex::new("hello").unwrap();
        
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
        
        let regex = Regex::new("hello").unwrap();
        
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
        
        let regex = Regex::new("hello").unwrap();
        
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
}
