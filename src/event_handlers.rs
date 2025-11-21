use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind, MouseButton};
use crossterm::execute;
use std::io::Write;

use crate::coordinates::{visual_to_logical_position, line_number_width, visual_col_to_char_index};
use crate::editor_state::FileViewerState;
use crate::editing::{
    apply_redo, apply_undo, handle_copy, handle_cut, handle_editing_keys, handle_paste, save_file, delete_file_history,
};
use crate::settings::Settings;

/// Result of handle_key_event: (should_quit, should_close_file)
pub(crate) fn handle_key_event(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    key_event: KeyEvent,
    settings: &Settings,
    visible_lines: usize,
    filename: &str,
) -> Result<(bool, bool), std::io::Error> {
    let KeyEvent { code, modifiers, .. } = key_event;
    
    // Handle close file (Ctrl+W)
    if settings.keybindings.close_matches(&code, &modifiers) {
        if state.modified {
            // Show confirmation prompt
            if show_close_confirmation(state)? {
                // User confirmed - delete file history
                let _ = delete_file_history(filename);
                return Ok((false, true)); // Don't quit editor, but close this file
            } else {
                // User cancelled
                state.needs_redraw = true;
                return Ok((false, false));
            }
        } else {
            // No unsaved changes - just delete
            let _ = delete_file_history(filename);
            return Ok((false, true));
        }
    }
    
    // Check for exit commands
    if is_exit_command(&code, &modifiers, settings) {
        // Before exiting, persist final scroll and cursor position
        let abs = state.absolute_line();
        state.undo_history.update_cursor(state.top_line, abs, state.cursor_col);
        let _ = state.undo_history.save(filename);
        return Ok((true, false));
    }

    // Handle save
    if settings.keybindings.save_matches(&code, &modifiers) {
        save_file(filename, lines)?;
        state.modified = false;
        state.needs_redraw = true;
        // Clear the unsaved file content since we just saved
        state.undo_history.clear_unsaved_state();
        // Save undo history when saving the file
        let _ = state.undo_history.save(filename);
        return Ok((false, false));
    }
    
    // Handle undo
    if settings.keybindings.undo_matches(&code, &modifiers) {
        if apply_undo(state, lines, filename, visible_lines) {
            state.needs_redraw = true;
        }
        return Ok((false, false));
    }
    
    // Handle redo
    if settings.keybindings.redo_matches(&code, &modifiers) {
        if apply_redo(state, lines, filename, visible_lines) {
            state.needs_redraw = true;
        }
        return Ok((false, false));
    }
    
    // Handle copy
    if settings.keybindings.copy_matches(&code, &modifiers) {
        handle_copy(state, lines)?;
        return Ok((false, false));
    }
    
    // Handle paste
    if settings.keybindings.paste_matches(&code, &modifiers) {
        if handle_paste(state, lines, filename) {
            state.needs_redraw = true;
        }
        return Ok((false, false));
    }

    // Handle cut
    if settings.keybindings.cut_matches(&code, &modifiers) {
        if handle_cut(state, lines, filename) { /* already set redraw */ }
        return Ok((false, false));
    }

    let is_shift = modifiers.contains(KeyModifiers::SHIFT);
    let is_navigation = is_navigation_key(&code);
    
    if is_navigation && is_shift {
        state.start_selection();
    }

    let did_edit = handle_editing_keys(state, lines, &code, &modifiers, visible_lines, filename);
    let moved = handle_navigation(state, lines, code, visible_lines);
    update_selection_state(state, moved, is_shift, is_navigation);
    update_redraw_flags(state, did_edit, moved);

    Ok((false, false))
}

pub(crate) fn handle_mouse_event(
    state: &mut FileViewerState,
    lines: &[String],
    mouse_event: MouseEvent,
    visible_lines: usize,
) {
    let MouseEvent { kind, column, row, .. } = mouse_event;
    
    // Calculate the position in the file based on mouse coordinates
    // Row 0 is the header, row 1+ is content
    if row == 0 {
        return; // Ignore clicks on header
    }
    
    let visual_line = (row as usize).saturating_sub(1); // Subtract header line
    
    // Don't process clicks beyond visible content area
    if visual_line >= visible_lines {
        return;
    }
    
    match kind {
        MouseEventKind::Down(MouseButton::Left) => {
            // Calculate which logical line and column was clicked
            if let Some((logical_line, col)) = visual_to_logical_position(
                state,
                lines,
                visual_line,
                column,
            ) {
                // Move cursor to clicked position
                if logical_line < lines.len() {
                    state.saved_absolute_cursor = None; // Cursor is back on screen
                    state.saved_scroll_state = None; // Clear saved scroll state
                    state.cursor_line = logical_line.saturating_sub(state.top_line);
                    state.cursor_col = col.min(lines[logical_line].len());
                    state.clear_selection();
                    state.mouse_dragging = true;
                    state.needs_redraw = true;
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if state.mouse_dragging {
                // Start or update selection
                if state.selection_start.is_none() {
                    state.selection_start = Some(state.current_position());
                }
                
                if let Some((logical_line, col)) = visual_to_logical_position(
                    state,
                    lines,
                    visual_line,
                    column,
                ) {
                    if logical_line < lines.len() {
                        state.saved_absolute_cursor = None; // Cursor is back on screen
                        state.saved_scroll_state = None; // Clear saved scroll state
                        state.cursor_line = logical_line.saturating_sub(state.top_line);
                        state.cursor_col = col.min(lines[logical_line].len());
                        state.selection_end = Some(state.current_position());
                        state.needs_redraw = true;
                    }
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            state.mouse_dragging = false;
        }
        MouseEventKind::ScrollDown => {
            // Scroll down by 3 lines, keeping cursor at its absolute position
            let scroll_amount = 3;
            let max_scroll = lines.len().saturating_sub(1);
            let absolute_cursor = state.absolute_line();
            let old_top = state.top_line;
            let old_cursor_line = state.cursor_line;
            state.top_line = (state.top_line + scroll_amount).min(max_scroll);
            
            // Check if cursor is now above the visible area
            if absolute_cursor < state.top_line {
                // Cursor is above visible area
                // Save scroll state if this is the first time cursor goes off-screen
                if state.saved_scroll_state.is_none() {
                    state.saved_scroll_state = Some((old_top, old_cursor_line));
                }
                state.saved_absolute_cursor = Some(absolute_cursor);
                state.cursor_line = 0; // Doesn't matter, cursor is hidden
            } else {
                // Cursor is on or below the screen - clear saved state
                state.saved_absolute_cursor = None;
                state.saved_scroll_state = None;
                state.cursor_line = absolute_cursor - state.top_line;
            }
            
            // Only redraw if we actually scrolled
            if state.top_line != old_top {
                state.needs_redraw = true;
            }
        }
        MouseEventKind::ScrollUp => {
            // Scroll up by 3 lines, keeping cursor at its absolute position
            let scroll_amount = 3;
            let absolute_cursor = state.absolute_line();
            let old_top = state.top_line;
            let old_cursor_line = state.cursor_line;
            state.top_line = state.top_line.saturating_sub(scroll_amount);
            
            // Check if cursor should come back into view or stay/go off-screen
            if absolute_cursor >= state.top_line {
                // Calculate new cursor_line
                let new_cursor_line = absolute_cursor - state.top_line;
                
                // Check if cursor is now below the visible area
                if new_cursor_line >= visible_lines {
                    // Cursor is below visible area
                    // Save scroll state if this is the first time cursor goes off-screen
                    if state.saved_scroll_state.is_none() {
                        state.saved_scroll_state = Some((old_top, old_cursor_line));
                    }
                    state.saved_absolute_cursor = Some(absolute_cursor);
                    state.cursor_line = new_cursor_line; // Keep tracking even though invisible
                } else {
                    // Cursor is visible - clear saved state
                    state.saved_absolute_cursor = None;
                    state.saved_scroll_state = None;
                    state.cursor_line = new_cursor_line;
                }
            } else {
                // Cursor is still above the visible area (shouldn't normally happen when scrolling up,
                // but keep it for consistency)
                state.saved_absolute_cursor = Some(absolute_cursor);
                state.cursor_line = 0;
            }
            
            // Only redraw if we actually scrolled
            if state.top_line != old_top {
                state.needs_redraw = true;
            }
        }
        _ => {}
    }
}

fn is_exit_command(code: &KeyCode, modifiers: &KeyModifiers, settings: &Settings) -> bool {
    settings.keybindings.quit_matches(code, modifiers)
        || settings.keybindings.close_matches(code, modifiers)
}

fn is_navigation_key(code: &KeyCode) -> bool {
    matches!(
        code,
        KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right 
        | KeyCode::Home | KeyCode::End | KeyCode::PageUp | KeyCode::PageDown
    )
}

fn update_selection_state(state: &mut FileViewerState, moved: bool, is_shift: bool, is_navigation: bool) {
    if moved {
        if is_shift { state.update_selection(); } else { state.clear_selection(); }
    } else if !is_shift && is_navigation { state.clear_selection(); }
}

fn update_redraw_flags(state: &mut FileViewerState, did_edit: bool, moved: bool) {
    if did_edit || moved { state.needs_redraw = true; }
    if did_edit { state.modified = true; }
}

/// Handle moving up through wrapped lines
fn handle_up_navigation(state: &mut FileViewerState, lines: &[String], _visible_lines: usize) {
    use crate::coordinates::{visual_width_up_to, calculate_wrapped_lines_for_line};
    
    let absolute_line = state.absolute_line();
    if absolute_line >= lines.len() {
        return;
    }
    
    let line_num_width = line_number_width(state.settings);
    let text_width = state.term_width.saturating_sub(line_num_width) as usize;
    let tab_width = state.settings.tab_width;
    
    if text_width == 0 {
        return;
    }
    
    let line = &lines[absolute_line];
    let visual_col = visual_width_up_to(line, state.cursor_col, tab_width);
    let current_wrap_line = visual_col / text_width;
    
    // If we're not on the first wrapped line of this logical line, move up within the same line
    if current_wrap_line > 0 {
        // Move up one visual line within the same logical line
        let target_visual_col = visual_col.saturating_sub(text_width);
        state.cursor_col = visual_col_to_char_index(line, target_visual_col, tab_width);
    } else {
        // We're on the first wrapped line, move to previous logical line
        if state.cursor_line > 0 {
            state.cursor_line -= 1;
            
            // Move to the last wrapped line of the previous logical line
            let prev_absolute = state.absolute_line();
            if prev_absolute < lines.len() {
                let prev_line = &lines[prev_absolute];
                let num_wrapped = calculate_wrapped_lines_for_line(lines, prev_absolute, text_width as u16, tab_width) as usize;
                
                // Calculate target column on the last wrap line
                let target_wrap_line = num_wrapped.saturating_sub(1);
                let target_visual_col = (target_wrap_line * text_width) + (visual_col % text_width);
                state.cursor_col = visual_col_to_char_index(prev_line, target_visual_col, tab_width);
            }
        } else if state.top_line > 0 {
            // Scroll up
            state.top_line -= 1;
            
            // Move to the last wrapped line of the new top line
            let new_top_absolute = state.top_line;
            if new_top_absolute < lines.len() {
                let new_top_line = &lines[new_top_absolute];
                let num_wrapped = calculate_wrapped_lines_for_line(lines, new_top_absolute, text_width as u16, tab_width) as usize;
                
                let target_wrap_line = num_wrapped.saturating_sub(1);
                let target_visual_col = (target_wrap_line * text_width) + (visual_col % text_width);
                state.cursor_col = visual_col_to_char_index(new_top_line, target_visual_col, tab_width);
            }
        }
    }
}

/// Handle moving down through wrapped lines
fn handle_down_navigation(state: &mut FileViewerState, lines: &[String], visible_lines: usize) {
    use crate::coordinates::{visual_width_up_to, calculate_wrapped_lines_for_line, calculate_visual_lines_to_cursor};
    
    let absolute_line = state.absolute_line();
    if absolute_line >= lines.len() {
        return;
    }
    
    let line_num_width = line_number_width(state.settings);
    let text_width = state.term_width.saturating_sub(line_num_width) as usize;
    let tab_width = state.settings.tab_width;
    
    if text_width == 0 {
        return;
    }
    
    let line = &lines[absolute_line];
    let visual_col = visual_width_up_to(line, state.cursor_col, tab_width);
    let current_wrap_line = visual_col / text_width;
    let num_wrapped = calculate_wrapped_lines_for_line(lines, absolute_line, text_width as u16, tab_width) as usize;
    
    // If we're not on the last wrapped line of this logical line, move down within the same line
    if current_wrap_line + 1 < num_wrapped {
        // Move down one visual line within the same logical line
        let target_visual_col = visual_col + text_width;
        state.cursor_col = visual_col_to_char_index(line, target_visual_col, tab_width);
    } else {
        // We're on the last wrapped line, move to next logical line
        if absolute_line + 1 < lines.len() {
            // Try moving to next logical line
            state.cursor_line += 1;
            
            // Move to the first wrapped line of the next logical line
            let next_absolute = state.absolute_line();
            if next_absolute < lines.len() {
                let next_line = &lines[next_absolute];
                let target_visual_col = visual_col % text_width;
                state.cursor_col = visual_col_to_char_index(next_line, target_visual_col, tab_width);
            }
            
            // Check if cursor is now beyond visible area (accounting for wrapped lines)
            let visual_lines_consumed = calculate_visual_lines_to_cursor(lines, state, text_width as u16);
            
            if visual_lines_consumed > visible_lines {
                // Cursor would be off-screen, scroll instead
                state.cursor_line -= 1;
                state.top_line += 1;
                
                // Recalculate position after scroll
                let current_absolute = state.absolute_line();
                if current_absolute < lines.len() {
                    let current_line = &lines[current_absolute];
                    let target_visual_col = visual_col % text_width;
                    state.cursor_col = visual_col_to_char_index(current_line, target_visual_col, tab_width);
                }
            }
        }
    }
}

/// Convert visual column to character index, accounting for tabs
fn handle_navigation(
    state: &mut FileViewerState,
    lines: &[String],
    code: KeyCode,
    visible_lines: usize,
) -> bool {
    // If cursor is saved (off-screen), restore it to the original viewport position
    if let Some(saved_absolute) = state.saved_absolute_cursor {
        // Clear the saved position
        state.saved_absolute_cursor = None;
        
        // Restore to the saved scroll state (where we were before cursor disappeared)
        if let Some((saved_top, saved_cursor_line)) = state.saved_scroll_state {
            state.top_line = saved_top;
            state.cursor_line = saved_cursor_line;
        } else {
            // Fallback: center cursor in viewport if no saved state
            let desired_cursor_line = visible_lines / 2;
            state.top_line = saved_absolute.saturating_sub(desired_cursor_line);
            state.cursor_line = saved_absolute.saturating_sub(state.top_line);
        }
        
        // Clear saved scroll state
        state.saved_scroll_state = None;
        
        // Now proceed with the navigation from this restored position
    }
    
    let lines_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    match code {
        KeyCode::Up => { 
            handle_up_navigation(state, lines, visible_lines);
            state.adjust_cursor_col(&lines_refs); 
            true 
        }
        KeyCode::Down => {
            handle_down_navigation(state, lines, visible_lines);
            state.adjust_cursor_col(&lines_refs); 
            true 
        }
        KeyCode::Left => { if state.cursor_col > 0 { state.cursor_col -= 1; true } else { false } }
        KeyCode::Right => { if let Some(line) = lines.get(state.top_line + state.cursor_line) { if state.cursor_col < line.len() { state.cursor_col += 1; true } else { false } } else { false } }
        KeyCode::Home => { state.cursor_col = 0; true }
        KeyCode::End => { if let Some(line) = lines.get(state.top_line + state.cursor_line) { state.cursor_col = line.len(); } true }
        KeyCode::PageDown => { let new_top = (state.top_line + visible_lines).min(lines.len().saturating_sub(visible_lines)); state.top_line = new_top; if state.top_line + state.cursor_line >= lines.len() { state.cursor_line = lines.len().saturating_sub(state.top_line + 1); } true }
        KeyCode::PageUp => { state.top_line = state.top_line.saturating_sub(visible_lines); true }
        _ => false,
    }
}

/// Show confirmation prompt for closing file with unsaved changes
/// Returns true if user confirms (Enter), false if cancelled (Esc)
fn show_close_confirmation(_state: &mut FileViewerState) -> Result<bool, std::io::Error> {
    use crossterm::terminal;
    use crossterm::event;
    
    let mut stdout = std::io::stdout();
    let (_, term_height) = terminal::size()?;
    let footer_row = term_height - 1;
    
    // Display warning message in footer
    execute!(
        stdout,
        crossterm::cursor::MoveTo(0, footer_row),
        crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine),
        crossterm::style::SetForegroundColor(crossterm::style::Color::Yellow)
    )?;
    write!(stdout, "Discard changes? [Enter=Yes, Esc=Cancel]")?;
    execute!(stdout, crossterm::style::ResetColor)?;
    stdout.flush()?;
    
    // Wait for user response
    loop {
        if let event::Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Enter => {
                    return Ok(true); // User confirmed
                }
                KeyCode::Esc => {
                    return Ok(false); // User cancelled
                }
                _ => {
                    // Ignore other keys, wait for Enter or Esc
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use crate::undo::UndoHistory;
    use crate::editor_state::FileViewerState;
    use crate::env::set_temp_home;
    use crossterm::event::{MouseEvent, MouseEventKind, MouseButton, KeyModifiers};

    fn create_test_state() -> FileViewerState<'static> {
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
        let undo_history = UndoHistory::new();
        FileViewerState::new(80, undo_history, settings)
    }

    fn create_test_lines(count: usize) -> Vec<String> {
        (0..count).map(|i| format!("Line {}", i)).collect()
    }

    #[test]
    fn scroll_down_keeps_cursor_visible_initially() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(50);
        state.top_line = 5;
        state.cursor_line = 5; // Cursor at absolute line 10
        
        let mouse_event = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        
        handle_mouse_event(&mut state, &lines, mouse_event, 10);
        
        // After scrolling down 3 lines: top_line = 8, cursor should be at line 2 (absolute 10)
        assert_eq!(state.top_line, 8);
        assert_eq!(state.cursor_line, 2);
        assert_eq!(state.absolute_line(), 10);
        assert!(state.is_cursor_visible(&lines, 10, 80));
        assert!(state.saved_absolute_cursor.is_none());
    }

    #[test]
    fn scroll_down_makes_cursor_disappear_when_above_viewport() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(50);
        state.top_line = 8;
        state.cursor_line = 2; // Cursor at absolute line 10
        
        let mouse_event = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        
        handle_mouse_event(&mut state, &lines, mouse_event, 10);
        
        // After scrolling down 3 lines: top_line = 11, cursor at 10 is now above viewport
        assert_eq!(state.top_line, 11);
        assert_eq!(state.absolute_line(), 10);
        assert!(!state.is_cursor_visible(&lines, 10, 80));
        assert_eq!(state.saved_absolute_cursor, Some(10));
    }

    #[test]
    fn scroll_down_continues_with_cursor_hidden() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(50);
        state.top_line = 11;
        state.cursor_line = 0;
        state.saved_absolute_cursor = Some(10); // Cursor already hidden at line 10
        
        let mouse_event = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        
        handle_mouse_event(&mut state, &lines, mouse_event, 10);
        
        // After scrolling down 3 more lines: top_line = 14, cursor still at 10
        assert_eq!(state.top_line, 14);
        assert_eq!(state.absolute_line(), 10);
        assert!(!state.is_cursor_visible(&lines, 10, 80));
        assert_eq!(state.saved_absolute_cursor, Some(10));
    }

    #[test]
    fn scroll_up_brings_cursor_back_into_view() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(50);
        state.top_line = 14;
        state.cursor_line = 0;
        state.saved_absolute_cursor = Some(10); // Cursor hidden at line 10
        
        let mouse_event = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        
        handle_mouse_event(&mut state, &lines, mouse_event, 10);
        
        // After scrolling up 3 lines: top_line = 11, cursor still hidden
        assert_eq!(state.top_line, 11);
        assert_eq!(state.absolute_line(), 10);
        assert!(!state.is_cursor_visible(&lines, 10, 80));
        assert_eq!(state.saved_absolute_cursor, Some(10));
        
        // Scroll up again
        handle_mouse_event(&mut state, &lines, mouse_event, 10);
        
        // After scrolling up 3 more lines: top_line = 8, cursor now visible at line 2
        assert_eq!(state.top_line, 8);
        assert_eq!(state.cursor_line, 2);
        assert_eq!(state.absolute_line(), 10);
        assert!(state.is_cursor_visible(&lines, 10, 80));
        assert!(state.saved_absolute_cursor.is_none());
    }

    #[test]
    fn scroll_up_keeps_cursor_visible() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(50);
        state.top_line = 10;
        state.cursor_line = 15; // Cursor at absolute line 25 (below viewport of 10 lines)
        
        let mouse_event = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        
        handle_mouse_event(&mut state, &lines, mouse_event, 10);
        
        // After scrolling up 3 lines: top_line = 7, cursor at line 18 (absolute 25)
        assert_eq!(state.top_line, 7);
        assert_eq!(state.cursor_line, 18);
        assert_eq!(state.absolute_line(), 25);
        // Cursor still beyond visible area
        assert!(!state.is_cursor_visible(&lines, 10, 80));
    }

    #[test]
    fn scroll_down_respects_max_scroll() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(20);
        state.top_line = 18;
        state.cursor_line = 1; // Cursor at absolute line 19
        
        let mouse_event = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        
        handle_mouse_event(&mut state, &lines, mouse_event, 10);
        
        // Should only scroll to max (19), not beyond
        assert_eq!(state.top_line, 19);
        assert_eq!(state.absolute_line(), 19);
    }

    #[test]
    fn scroll_up_stops_at_zero() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(50);
        state.top_line = 2;
        state.cursor_line = 5; // Cursor at absolute line 7
        
        let mouse_event = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        
        handle_mouse_event(&mut state, &lines, mouse_event, 10);
        
        // Should stop at top_line = 0
        assert_eq!(state.top_line, 0);
        assert_eq!(state.cursor_line, 7);
        assert_eq!(state.absolute_line(), 7);
        assert!(state.is_cursor_visible(&lines, 10, 80));
    }

    #[test]
    fn mouse_click_clears_saved_cursor() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(50);
        state.top_line = 20;
        state.cursor_line = 0;
        state.saved_absolute_cursor = Some(10); // Cursor hidden
        
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 5,
            modifiers: KeyModifiers::empty(),
        };
        
        handle_mouse_event(&mut state, &lines, mouse_event, 10);
        
        // Click should bring cursor back and clear saved position
        assert!(state.saved_absolute_cursor.is_none());
    }

    #[test]
    fn complete_scroll_scenario() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(50);
        let visible_lines = 10;
        
        // Setup: cursor at line 10, viewport shows lines 5-15
        state.top_line = 5;
        state.cursor_line = 5;
        assert_eq!(state.absolute_line(), 10);
        assert!(state.is_cursor_visible(&lines, visible_lines, 80));
        
        let scroll_down = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        let scroll_up = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        
        // Scroll down once: cursor still visible
        handle_mouse_event(&mut state, &lines, scroll_down, visible_lines);
        assert_eq!(state.top_line, 8);
        assert_eq!(state.cursor_line, 2);
        assert!(state.is_cursor_visible(&lines, visible_lines, 80));
        
        // Scroll down again: cursor disappears
        handle_mouse_event(&mut state, &lines, scroll_down, visible_lines);
        assert_eq!(state.top_line, 11);
        assert_eq!(state.absolute_line(), 10);
        assert!(!state.is_cursor_visible(&lines, visible_lines, 80));
        assert_eq!(state.saved_absolute_cursor, Some(10));
        
        // Scroll down more: cursor stays hidden at same absolute position
        handle_mouse_event(&mut state, &lines, scroll_down, visible_lines);
        assert_eq!(state.top_line, 14);
        assert_eq!(state.absolute_line(), 10);
        assert!(!state.is_cursor_visible(&lines, visible_lines, 80));
        
        // Scroll up: cursor still hidden
        handle_mouse_event(&mut state, &lines, scroll_up, visible_lines);
        assert_eq!(state.top_line, 11);
        assert_eq!(state.absolute_line(), 10);
        assert!(!state.is_cursor_visible(&lines, visible_lines, 80));
        
        // Scroll up again: cursor comes back into view
        handle_mouse_event(&mut state, &lines, scroll_up, visible_lines);
        assert_eq!(state.top_line, 8);
        assert_eq!(state.cursor_line, 2);
        assert_eq!(state.absolute_line(), 10);
        assert!(state.is_cursor_visible(&lines, visible_lines, 80));
        assert!(state.saved_absolute_cursor.is_none());
        assert!(state.saved_scroll_state.is_none());
    }
    
    #[test]
    fn keyboard_navigation_restores_cursor_from_saved_position() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(50);
        let visible_lines = 10;
        
        // Setup: cursor at line 10, viewport at lines 5-15
        state.top_line = 5;
        state.cursor_line = 5;
        assert_eq!(state.absolute_line(), 10);
        
        // Scroll down until cursor disappears
        let scroll_down = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        
        // First scroll: cursor still visible at line 2
        handle_mouse_event(&mut state, &lines, scroll_down, visible_lines);
        assert_eq!(state.top_line, 8);
        assert_eq!(state.cursor_line, 2);
        
        // Second scroll: cursor disappears (saved at line 10, scroll state saved as top=8, cursor=2)
        handle_mouse_event(&mut state, &lines, scroll_down, visible_lines);
        assert_eq!(state.top_line, 11);
        assert!(!state.is_cursor_visible(&lines, visible_lines, 80));
        assert_eq!(state.saved_absolute_cursor, Some(10));
        assert_eq!(state.saved_scroll_state, Some((8, 2))); // Saved the position just before disappearing
        
        // Press Up arrow key
        let moved = handle_navigation(&mut state, &lines, KeyCode::Up, visible_lines);
        
        assert!(moved);
        // Cursor should be restored to the saved scroll state (top=8, cursor=2), then moved up
        // After Up: cursor_line = 1, absolute = 9
        assert_eq!(state.top_line, 8);
        assert_eq!(state.cursor_line, 1);
        assert_eq!(state.absolute_line(), 9);
        assert!(state.is_cursor_visible(&lines, visible_lines, 80));
        assert!(state.saved_absolute_cursor.is_none());
        assert!(state.saved_scroll_state.is_none());
    }
    
    #[test]
    fn keyboard_navigation_down_from_saved_position() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(50);
        let visible_lines = 10;
        
        // Setup: cursor at line 10, scroll until it disappears
        state.top_line = 5;
        state.cursor_line = 5;
        
        let scroll_down = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        
        // Scroll twice to make cursor disappear
        handle_mouse_event(&mut state, &lines, scroll_down, visible_lines);
        handle_mouse_event(&mut state, &lines, scroll_down, visible_lines);
        
        // Cursor should be hidden with saved state
        assert!(!state.is_cursor_visible(&lines, visible_lines, 80));
        assert_eq!(state.saved_scroll_state, Some((8, 2)));
        
        // Press Down arrow key
        let moved = handle_navigation(&mut state, &lines, KeyCode::Down, visible_lines);
        
        assert!(moved);
        // Cursor should be restored to saved scroll state, then moved down
        // Restore to top=8, cursor=2, then Down: cursor=3, absolute=11
        assert_eq!(state.top_line, 8);
        assert_eq!(state.cursor_line, 3);
        assert_eq!(state.absolute_line(), 11);
        assert!(state.is_cursor_visible(&lines, visible_lines, 80));
        assert!(state.saved_absolute_cursor.is_none());
    }
    
    #[test]
    fn keyboard_navigation_left_right_from_saved_position() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(50);
        let visible_lines = 10;
        
        // Setup: cursor at line 10, col 5, scroll until it disappears
        state.top_line = 5;
        state.cursor_line = 5;
        state.cursor_col = 5;
        
        let scroll_down = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        
        // Scroll twice to make cursor disappear
        handle_mouse_event(&mut state, &lines, scroll_down, visible_lines);
        handle_mouse_event(&mut state, &lines, scroll_down, visible_lines);
        
        assert!(!state.is_cursor_visible(&lines, visible_lines, 80));
        
        // Press Left arrow key
        let moved = handle_navigation(&mut state, &lines, KeyCode::Left, visible_lines);
        
        assert!(moved);
        // Cursor should be restored to saved scroll state and column moved left
        assert_eq!(state.top_line, 8);
        assert_eq!(state.cursor_line, 2);
        assert_eq!(state.cursor_col, 4);
        assert_eq!(state.absolute_line(), 10);
        assert!(state.is_cursor_visible(&lines, visible_lines, 80));
        assert!(state.saved_absolute_cursor.is_none());
    }
    
    #[test]
    fn scroll_up_makes_cursor_disappear_below_viewport() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(50);
        let visible_lines = 10;
        
        // Setup: cursor at line 20, viewport at 15-25, cursor at visual line 5
        state.top_line = 15;
        state.cursor_line = 5;
        assert_eq!(state.absolute_line(), 20);
        
        let scroll_up = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        
        // First scroll up: cursor moves down visually but still visible
        handle_mouse_event(&mut state, &lines, scroll_up, visible_lines);
        assert_eq!(state.top_line, 12);
        assert_eq!(state.cursor_line, 8);
        assert_eq!(state.absolute_line(), 20);
        assert!(state.is_cursor_visible(&lines, visible_lines, 80));
        
        // Second scroll up: cursor disappears below viewport
        handle_mouse_event(&mut state, &lines, scroll_up, visible_lines);
        assert_eq!(state.top_line, 9);
        assert_eq!(state.cursor_line, 11); // Below visible_lines=10
        assert_eq!(state.absolute_line(), 20);
        assert!(!state.is_cursor_visible(&lines, visible_lines, 80));
        assert_eq!(state.saved_absolute_cursor, Some(20));
        assert_eq!(state.saved_scroll_state, Some((12, 8))); // Saved position just before disappearing
    }
    
    #[test]
    fn scroll_down_brings_cursor_back_from_below() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(50);
        let visible_lines = 10;
        
        // Setup: cursor at line 20, scroll up until it disappears below
        state.top_line = 15;
        state.cursor_line = 5;
        
        let scroll_up = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        let scroll_down = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        
        // Scroll up twice to make cursor disappear
        handle_mouse_event(&mut state, &lines, scroll_up, visible_lines);
        handle_mouse_event(&mut state, &lines, scroll_up, visible_lines);
        
        assert!(!state.is_cursor_visible(&lines, visible_lines, 80));
        assert_eq!(state.saved_absolute_cursor, Some(20));
        
        // Scroll down: cursor still hidden
        handle_mouse_event(&mut state, &lines, scroll_down, visible_lines);
        assert_eq!(state.top_line, 12);
        assert_eq!(state.cursor_line, 8);
        assert_eq!(state.absolute_line(), 20);
        assert!(state.is_cursor_visible(&lines, visible_lines, 80));
        assert!(state.saved_absolute_cursor.is_none());
        assert!(state.saved_scroll_state.is_none());
    }
    
    #[test]
    fn keyboard_navigation_restores_from_below_viewport() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(50);
        let visible_lines = 10;
        
        // Setup: cursor at line 20, scroll up until it disappears
        state.top_line = 15;
        state.cursor_line = 5;
        
        let scroll_up = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        
        // Scroll up twice to make cursor disappear below viewport
        handle_mouse_event(&mut state, &lines, scroll_up, visible_lines);
        handle_mouse_event(&mut state, &lines, scroll_up, visible_lines);
        
        assert!(!state.is_cursor_visible(&lines, visible_lines, 80));
        assert_eq!(state.saved_scroll_state, Some((12, 8)));
        
        // Press Down arrow key
        let moved = handle_navigation(&mut state, &lines, KeyCode::Down, visible_lines);
        
        assert!(moved);
        // Cursor should be restored to saved scroll state (top=12, cursor=8), then moved down
        // After Down: cursor=9, absolute=21
        assert_eq!(state.top_line, 12);
        assert_eq!(state.cursor_line, 9);
        assert_eq!(state.absolute_line(), 21);
        assert!(state.is_cursor_visible(&lines, visible_lines, 80));
        assert!(state.saved_absolute_cursor.is_none());
        assert!(state.saved_scroll_state.is_none());
    }
    
    #[test]
    fn keyboard_navigation_up_from_below_viewport() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(50);
        let visible_lines = 10;
        
        // Setup: cursor at line 20, scroll up until it disappears
        state.top_line = 15;
        state.cursor_line = 5;
        
        let scroll_up = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        
        // Scroll up twice to make cursor disappear below viewport
        handle_mouse_event(&mut state, &lines, scroll_up, visible_lines);
        handle_mouse_event(&mut state, &lines, scroll_up, visible_lines);
        
        assert!(!state.is_cursor_visible(&lines, visible_lines, 80));
        
        // Press Up arrow key
        let moved = handle_navigation(&mut state, &lines, KeyCode::Up, visible_lines);
        
        assert!(moved);
        // Cursor should be restored to saved scroll state, then moved up
        // Restore to top=12, cursor=8, then Up: cursor=7, absolute=19
        assert_eq!(state.top_line, 12);
        assert_eq!(state.cursor_line, 7);
        assert_eq!(state.absolute_line(), 19);
        assert!(state.is_cursor_visible(&lines, visible_lines, 80));
        assert!(state.saved_absolute_cursor.is_none());
    }
    
    #[test]
    fn complete_scroll_up_scenario() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(50);
        let visible_lines = 10;
        
        // Setup: cursor at line 20, viewport shows lines 15-25
        state.top_line = 15;
        state.cursor_line = 5;
        assert_eq!(state.absolute_line(), 20);
        
        let scroll_up = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        let scroll_down = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        
        // Scroll up once: cursor still visible but lower in viewport
        handle_mouse_event(&mut state, &lines, scroll_up, visible_lines);
        assert_eq!(state.top_line, 12);
        assert_eq!(state.cursor_line, 8);
        assert!(state.is_cursor_visible(&lines, visible_lines, 80));
        
        // Scroll up again: cursor disappears below viewport
        handle_mouse_event(&mut state, &lines, scroll_up, visible_lines);
        assert_eq!(state.top_line, 9);
        assert!(!state.is_cursor_visible(&lines, visible_lines, 80));
        assert_eq!(state.saved_absolute_cursor, Some(20));
        assert_eq!(state.saved_scroll_state, Some((12, 8)));
        
        // Scroll up more: cursor stays hidden at same absolute position
        handle_mouse_event(&mut state, &lines, scroll_up, visible_lines);
        assert_eq!(state.top_line, 6);
        assert_eq!(state.absolute_line(), 20);
        assert!(!state.is_cursor_visible(&lines, visible_lines, 80));
        
        // Scroll down: cursor still hidden
        handle_mouse_event(&mut state, &lines, scroll_down, visible_lines);
        assert_eq!(state.top_line, 9);
        assert!(!state.is_cursor_visible(&lines, visible_lines, 80));
        
        // Scroll down again: cursor comes back into view
        handle_mouse_event(&mut state, &lines, scroll_down, visible_lines);
        assert_eq!(state.top_line, 12);
        assert_eq!(state.cursor_line, 8);
        assert_eq!(state.absolute_line(), 20);
        assert!(state.is_cursor_visible(&lines, visible_lines, 80));
        assert!(state.saved_absolute_cursor.is_none());
        assert!(state.saved_scroll_state.is_none());
    }
    
    #[test]
    fn page_down_restores_cursor_from_saved_position() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(50);
        let visible_lines = 10;
        
        // Setup: cursor at line 10, scroll down until it disappears
        state.top_line = 5;
        state.cursor_line = 5;
        
        let scroll_down = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        
        // Scroll twice to make cursor disappear above viewport
        handle_mouse_event(&mut state, &lines, scroll_down, visible_lines);
        handle_mouse_event(&mut state, &lines, scroll_down, visible_lines);
        
        assert!(!state.is_cursor_visible(&lines, visible_lines, 80));
        assert_eq!(state.saved_scroll_state, Some((8, 2)));
        
        // Press PageDown
        let moved = handle_navigation(&mut state, &lines, KeyCode::PageDown, visible_lines);
        
        assert!(moved);
        // Cursor should be restored first, then PageDown moves viewport
        // Restore to top=8, then PageDown moves to top=18
        assert_eq!(state.top_line, 18);
        assert!(state.is_cursor_visible(&lines, visible_lines, 80));
        assert!(state.saved_absolute_cursor.is_none());
    }
    
    #[test]
    fn page_up_restores_cursor_from_saved_position() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(20); // Small file
        let visible_lines = 10;
        
        // Setup: cursor at line 19 (last line), viewport at 10-20
        state.top_line = 10;
        state.cursor_line = 9;
        assert_eq!(state.absolute_line(), 19);
        
        let scroll_up = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        };
        
        // Scroll up - cursor moves down in viewport
        handle_mouse_event(&mut state, &lines, scroll_up, visible_lines);
        assert_eq!(state.top_line, 7);
        assert_eq!(state.cursor_line, 12); // Beyond visible_lines, should be hidden
        assert!(!state.is_cursor_visible(&lines, visible_lines, 80));
        assert_eq!(state.saved_absolute_cursor, Some(19));
        assert_eq!(state.saved_scroll_state, Some((10, 9)));
    }
    
    #[test]
    fn navigate_down_within_wrapped_line() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        // Create a long line that will wrap (80 char width - 4 for line numbers = 76 chars available)
        let long_line = "a".repeat(150); // Will wrap into 2 lines
        let lines = vec![long_line];
        
        state.top_line = 0;
        state.cursor_line = 0;
        state.cursor_col = 0;
        
        // Move down - should stay on same logical line but move to second wrap
        let moved = handle_navigation(&mut state, &lines, KeyCode::Down, 10);
        assert!(moved);
        assert_eq!(state.cursor_line, 0); // Still on line 0
        assert_eq!(state.cursor_col, 76); // Moved to second wrap portion (76 chars in)
    }
    
    #[test]
    fn navigate_up_within_wrapped_line() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let long_line = "a".repeat(150);
        let lines = vec![long_line];
        
        state.top_line = 0;
        state.cursor_line = 0;
        state.cursor_col = 100; // On second wrap portion
        
        // Move up - should stay on same logical line but move to first wrap
        let moved = handle_navigation(&mut state, &lines, KeyCode::Up, 10);
        assert!(moved);
        assert_eq!(state.cursor_line, 0); // Still on line 0
        assert_eq!(state.cursor_col, 24); // Moved back one visual line (100 - 76 = 24)
    }
    
    #[test]
    fn navigate_down_from_wrapped_to_next_line() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let long_line = "a".repeat(150);
        let short_line = "b".repeat(20);
        let lines = vec![long_line, short_line];
        
        state.top_line = 0;
        state.cursor_line = 0;
        state.cursor_col = 100; // On second wrap of first line
        
        // Move down - should go to next logical line
        let moved = handle_navigation(&mut state, &lines, KeyCode::Down, 10);
        assert!(moved);
        assert_eq!(state.cursor_line, 1); // Moved to next line
        assert_eq!(state.cursor_col, 20); // Clamped to line length // Column maintained (100 % 76 = 24)
    }
    
    #[test]
    fn navigate_up_from_first_wrap_to_previous_line() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let long_line = "a".repeat(150);
        let short_line = "b".repeat(20);
        let lines = vec![long_line, short_line];
        
        state.top_line = 0;
        state.cursor_line = 1;
        state.cursor_col = 10;
        
        // Move up - should go to last wrap of previous line
        let moved = handle_navigation(&mut state, &lines, KeyCode::Up, 10);
        assert!(moved);
        assert_eq!(state.cursor_line, 0); // Moved to previous line
        assert_eq!(state.cursor_col, 86); // On last wrap portion (76 + 10 = 86)
    }
    
    #[test]
    fn navigate_wrapped_line_with_tabs() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        // Line with tabs - each tab is 4 spaces, so "\t\t" = 8 visual chars
        let line_with_tabs = format!("\t\t{}", "a".repeat(140));
        let lines = vec![line_with_tabs];
        
        state.top_line = 0;
        state.cursor_line = 0;
        state.cursor_col = 0; // At first tab
        
        // Move down - should move to next visual line
        let moved = handle_navigation(&mut state, &lines, KeyCode::Down, 10);
        assert!(moved);
        assert_eq!(state.cursor_line, 0); // Still on same logical line
        // Cursor should have moved down one visual line (76 chars)
        assert!(state.cursor_col > 0);
    }
    
    #[test]
    fn navigate_multiple_wrapped_lines() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let very_long_line = "a".repeat(300); // Will wrap into 4 visual lines
        let lines = vec![very_long_line];
        
        state.top_line = 0;
        state.cursor_line = 0;
        state.cursor_col = 0;
        
        // Move down three times - should move through wraps
        handle_navigation(&mut state, &lines, KeyCode::Down, 10);
        assert_eq!(state.cursor_col, 76);
        
        handle_navigation(&mut state, &lines, KeyCode::Down, 10);
        assert_eq!(state.cursor_col, 152);
        
        handle_navigation(&mut state, &lines, KeyCode::Down, 10);
        assert_eq!(state.cursor_col, 228);
        
        // Move up three times - should return to start
        handle_navigation(&mut state, &lines, KeyCode::Up, 10);
        assert_eq!(state.cursor_col, 152);
        
        handle_navigation(&mut state, &lines, KeyCode::Up, 10);
        assert_eq!(state.cursor_col, 76);
        
        handle_navigation(&mut state, &lines, KeyCode::Up, 10);
        assert_eq!(state.cursor_col, 0);
    }
    
    #[test]
    fn wrapped_line_scrolling_when_moving_down() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let visible_lines = 5;
        
        // Create multiple long lines that wrap
        let mut lines = Vec::new();
        for _ in 0..10 {
            lines.push("a".repeat(150)); // Each wraps into 2 visual lines
        }
        
        state.top_line = 0;
        state.cursor_line = 4; // Near bottom of visible area
        state.cursor_col = 0;
        
        // Moving down multiple times should eventually scroll
        let initial_top = state.top_line;
        
        // Move down several times
        for _ in 0..10 {
            handle_navigation(&mut state, &lines, KeyCode::Down, visible_lines);
        }
        
        // Should have scrolled
        assert!(state.top_line > initial_top);
    }
    
    #[test]
    fn empty_line_navigation() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = vec!["".to_string(), "a".repeat(150), "".to_string()];
        
        state.top_line = 0;
        state.cursor_line = 0;
        state.cursor_col = 0;
        
        // Move down from empty line
        handle_navigation(&mut state, &lines, KeyCode::Down, 10);
        assert_eq!(state.cursor_line, 1);
        assert_eq!(state.cursor_col, 0);
        
        // Move down within wrapped line
        handle_navigation(&mut state, &lines, KeyCode::Down, 10);
        assert_eq!(state.cursor_line, 1);
        assert_eq!(state.cursor_col, 76);
    }
}
