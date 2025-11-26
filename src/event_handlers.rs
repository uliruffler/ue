use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use std::io::Write;
use std::time::Instant;

use crate::coordinates::{line_number_width, visual_col_to_char_index};
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

    // Ctrl+Arrow custom handling: word-wise (Left/Right) and scroll (Up/Down)
    if modifiers.contains(KeyModifiers::CONTROL) {
        let extend = modifiers.contains(KeyModifiers::SHIFT);
        if extend { state.start_selection(); }
        let mut moved = false;
        let scroll_delta = settings.keyboard_scroll_lines as isize;
        match code {
            KeyCode::Left => { moved = word_left(state, lines); }
            KeyCode::Right => { moved = word_right(state, lines); }
            KeyCode::Up => { moved = scroll_without_cursor(state, lines, visible_lines, -scroll_delta); }
            KeyCode::Down => { moved = scroll_without_cursor(state, lines, visible_lines, scroll_delta); }
            _ => {}
        }
        if moved {
            if extend { state.update_selection(); } else { state.clear_selection(); }
            state.needs_redraw = true;
            return Ok((false, false));
        }
    }
    
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
        state.last_save_time = Some(Instant::now());
        // Save session as editor
        let _ = crate::session::save_editor_session(filename);
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
        state.last_save_time = Some(Instant::now());
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

/// Delegate mouse event handling to mouse_handlers module
pub(crate) use crate::mouse_handlers::handle_mouse_event; // now takes &mut Vec<String>

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

/// Show confirmation prompt when undo file has unsaved changes but source file was modified externally
/// Returns true if user confirms opening file anyway (Enter), false if user wants to discard (Esc)
pub(crate) fn show_undo_conflict_confirmation() -> Result<bool, std::io::Error> {
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
    write!(stdout, "File was modified. Open anyway? [Enter=Yes, Esc=No]")?;
    execute!(stdout, crossterm::style::ResetColor)?;
    stdout.flush()?;
    
    // Wait for user response
    loop {
        if let event::Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Enter => {
                    return Ok(true); // User confirmed - open file
                }
                KeyCode::Esc => {
                    return Ok(false); // User declined - exit to selector
                }
                _ => {
                    // Ignore other keys, wait for Enter or Esc
                }
            }
        }
    }
}

fn word_left(state: &mut FileViewerState, lines: &[String]) -> bool {
    let abs = state.absolute_line(); if abs >= lines.len() { return false; }
    if state.cursor_col == 0 {
        if abs == 0 { return false; }
        // Move to previous line end
        if state.cursor_line > 0 { state.cursor_line -= 1; } else { state.top_line = state.top_line.saturating_sub(1); }
        let new_abs = state.absolute_line(); if new_abs < lines.len() { state.cursor_col = lines[new_abs].len(); }
        return true;
    }
    let line = &lines[abs]; let mut i = state.cursor_col;
    // First skip any non-word characters (including whitespace & punctuation)
    while i > 0 {
        let c = line.chars().nth(i-1).unwrap_or(' ');
        if is_word_char(c) { break; }
        i -= 1;
    }
    // Then skip the word characters
    while i > 0 {
        let c = line.chars().nth(i-1).unwrap_or(' ');
        if !is_word_char(c) { break; }
        i -= 1;
    }
    state.cursor_col = i; true
}
fn word_right(state: &mut FileViewerState, lines: &[String]) -> bool {
    let abs = state.absolute_line(); if abs >= lines.len() { return false; }
    let line = &lines[abs]; let len = line.len();
    if state.cursor_col >= len {
        if abs + 1 >= lines.len() { return false; }
        // Move to next line start
        if state.cursor_line + 1 < lines.len().saturating_sub(state.top_line) { state.cursor_line += 1; }
        else { state.top_line += 1; }
        state.cursor_col = 0; return true;
    }
    let mut i = state.cursor_col;
    // Skip any non-word (whitespace / punctuation)
    while i < len {
        let c = line.chars().nth(i).unwrap_or(' ');
        if is_word_char(c) { break; }
        i += 1;
    }
    // Skip the word
    while i < len {
        let c = line.chars().nth(i).unwrap_or(' ');
        if !is_word_char(c) { break; }
        i += 1;
    }
    state.cursor_col = i; true
}
fn is_word_char(c: char) -> bool { c.is_alphanumeric() || c == '_' }
fn scroll_without_cursor(state: &mut FileViewerState, lines: &[String], visible_lines: usize, delta: isize) -> bool {
    if delta == 0 { return false; }
    let old_top = state.top_line;
    // Capture absolute cursor BEFORE changing top_line so we can preserve it
    let absolute_cursor = state.absolute_line();
    if delta > 0 { state.top_line = (state.top_line + delta as usize).min(lines.len().saturating_sub(1)); }
    else { state.top_line = state.top_line.saturating_sub((-delta) as usize); }
    if absolute_cursor < state.top_line || absolute_cursor >= state.top_line + visible_lines {
        if state.saved_scroll_state.is_none() { state.saved_scroll_state = Some((old_top, state.cursor_line)); }
        state.saved_absolute_cursor = Some(absolute_cursor);
    } else {
        state.saved_absolute_cursor = None; state.saved_scroll_state = None; state.cursor_line = absolute_cursor - state.top_line;
    }
    state.top_line != old_top
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use crate::undo::UndoHistory;
    use crate::env::set_temp_home;
    use crate::syntax::SyntectHighlighter;

    fn create_test_state() -> FileViewerState<'static> {
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
        let undo_history = UndoHistory::new();
        let hl = Box::leak(Box::new(SyntectHighlighter::new()));
        FileViewerState::new(80, undo_history, settings, hl)
    }
    fn create_test_lines(count: usize) -> Vec<String> { (0..count).map(|i| format!("Line {}", i)).collect() }

    #[test]
    fn ctrl_scroll_preserves_absolute_cursor() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(100);
        state.top_line = 10; state.cursor_line = 5; // absolute 15
        let abs_before = state.absolute_line();
        assert_eq!(abs_before, 15);
        // simulate Ctrl+Down scroll (delta +3)
        super::scroll_without_cursor(&mut state, &lines, 20, 3);
        assert_eq!(state.absolute_line(), 15, "Absolute cursor should remain after scroll down");
        super::scroll_without_cursor(&mut state, &lines, 20, -3);
        assert_eq!(state.absolute_line(), 15, "Absolute cursor should remain after scroll up");
    }
}
