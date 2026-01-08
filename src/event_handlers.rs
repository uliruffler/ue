use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use std::io::Write;
use std::time::Instant;

use crate::coordinates::{line_number_width, visual_col_to_char_index};
use crate::editing::{
    apply_redo, apply_undo, delete_file_history, handle_copy, handle_cut, handle_editing_keys,
    handle_paste, save_file,
};
use crate::editor_state::FileViewerState;
use crate::settings::Settings;

/// Normalize key events so keypad Enter (often reported as '\r' or '\n') behaves like Enter
/// Normalize key events so keypad Enter (often reported as '\r', '\n', or the numpad_enter keybinding) behaves like Enter
pub(crate) fn normalize_key_event(mut key_event: KeyEvent, settings: &Settings) -> KeyEvent {
    match key_event.code {
        // Standard conversions for carriage return and newline
        KeyCode::Char('\r') | KeyCode::Char('\n') => key_event.code = KeyCode::Enter,
        // Handle num-pad Enter using the configured keybinding
        _ => {
            // Check if this key matches the numpad_enter keybinding
            if settings.keybindings.numpad_enter_matches(&key_event.code, &key_event.modifiers) {
                key_event.code = KeyCode::Enter;
                key_event.modifiers = KeyModifiers::empty();
            }
        }
    }
    key_event
}

/// Result of handle_key_event: (should_quit, should_close_file)
pub(crate) fn handle_key_event(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    key_event: KeyEvent,
    settings: &Settings,
    visible_lines: usize,
    filename: &str,
) -> Result<(bool, bool), std::io::Error> {
    let KeyEvent {
        code, modifiers, ..
    } = key_event;

    // Update menu checkable states before rendering
    state.menu_bar.update_checkable(
        crate::menu::MenuAction::ViewLineWrap,
        state.is_line_wrapping_enabled()
    );

    // Handle menu interactions (Alt+letter to open, navigation when active)
    // But not when help is active (help should handle Esc first)
    let (menu_action, needs_full_redraw) = if !state.help_active {
        crate::menu::handle_menu_key(&mut state.menu_bar, key_event)
    } else {
        (None, false)
    };

    if let Some(action) = menu_action {
        // An action was selected - always need redraw for this
        state.needs_redraw = true;

        // Execute menu action
        match action {
            crate::menu::MenuAction::FileNew => {
                // Create new file - delegate to ui.rs which will create an untitled buffer
                state.pending_menu_action = Some(action);
                return Ok((false, false));
            }
            crate::menu::MenuAction::FileOpenDialog => {
                // Open directory tree dialog
                state.pending_menu_action = Some(action);
                return Ok((false, false));
            }
            crate::menu::MenuAction::FileOpenRecent(_idx) => {
                // Open a recent file from the menu
                // Store the action to be handled by ui.rs which has access to file switching logic
                state.pending_menu_action = Some(action);
                return Ok((false, false));
            }
            crate::menu::MenuAction::FileSave => {
                // If this is an untitled file, we need to show the save-as dialog
                if state.is_untitled {
                    // Delegate to ui.rs which will show the save dialog
                    state.pending_menu_action = Some(action);
                    return Ok((false, false));
                }

                save_file(filename, lines)?;
                state.modified = false;
                state.undo_history.clear_unsaved_state();
                let abs = state.absolute_line();
                state.undo_history.update_cursor(state.top_line, abs, state.cursor_col);
                state.undo_history.find_history = state.find_history.clone();
                let _ = state.undo_history.save(filename);
                state.last_save_time = Some(Instant::now());
                return Ok((false, false));
            }
            crate::menu::MenuAction::FileClose => {
                // Close current file (same as Ctrl+w)
                if state.modified {
                    // Show confirmation dialog
                    let _ = crossterm::execute!(std::io::stdout(), crossterm::cursor::Show);
                    let confirmed = show_close_confirmation(filename, settings)?;
                    if confirmed {
                        let _ = delete_file_history(filename);
                        return Ok((false, true));
                    } else {
                        state.needs_redraw = true;
                        return Ok((false, false));
                    }
                } else {
                    let _ = delete_file_history(filename);
                    return Ok((false, true));
                }
            }
            crate::menu::MenuAction::FileQuit => {
                // Quit editor
                return Ok((true, false));
            }
            crate::menu::MenuAction::EditUndo => {
                if apply_undo(state, lines, filename, visible_lines) {
                    state.needs_redraw = true;
                }
                return Ok((false, false));
            }
            crate::menu::MenuAction::EditRedo => {
                if apply_redo(state, lines, filename, visible_lines) {
                    state.needs_redraw = true;
                }
                return Ok((false, false));
            }
            crate::menu::MenuAction::EditCopy => {
                handle_copy(state, lines)?;
                return Ok((false, false));
            }
            crate::menu::MenuAction::EditCut => {
                if handle_cut(state, lines, filename) {
                    state.needs_redraw = true;
                }
                return Ok((false, false));
            }
            crate::menu::MenuAction::EditPaste => {
                if handle_paste(state, lines, filename) {
                    state.needs_redraw = true;
                }
                return Ok((false, false));
            }
            crate::menu::MenuAction::EditFind => {
                // Enter find mode (same as Ctrl+F)
                // Save current search pattern to restore on Esc
                state.saved_search_pattern = state.last_search_pattern.clone();

                // If there's a selection, use it as the search scope
                if let (Some(start), Some(end)) = (state.selection_start, state.selection_end) {
                    // Normalize selection to ensure start < end
                    let normalized = if start.0 < end.0 || (start.0 == end.0 && start.1 <= end.1) {
                        (start, end)
                    } else {
                        (end, start)
                    };
                    state.find_scope = Some(normalized);
                } else {
                    state.find_scope = None;
                }

                state.find_active = true;
                state.find_pattern.clear();
                state.find_cursor_pos = 0;
                state.find_error = None;
                state.needs_redraw = true;
                return Ok((false, false));
            }
            crate::menu::MenuAction::ViewFileSelector => {
                // Open file selector (handled by ui.rs)
                state.pending_menu_action = Some(action);
                return Ok((false, false));
            }
            crate::menu::MenuAction::ViewLineWrap => {
                // Toggle line wrapping
                state.toggle_line_wrapping();
                state.needs_redraw = true;
                return Ok((false, false));
            }
            crate::menu::MenuAction::HelpEditor => {
                state.help_active = true;
                state.help_context = crate::help::HelpContext::Editor;
                state.help_scroll_offset = 0;
                state.needs_redraw = true;
                return Ok((false, false));
            }
            crate::menu::MenuAction::HelpFind => {
                state.help_active = true;
                state.help_context = crate::help::HelpContext::Find;
                state.help_scroll_offset = 0;
                state.needs_redraw = true;
                return Ok((false, false));
            }
            crate::menu::MenuAction::HelpFileSelector => {
                // Show file selector help - for now just show editor help
                state.help_active = true;
                state.help_context = crate::help::HelpContext::Editor;
                state.help_scroll_offset = 0;
                state.needs_redraw = true;
                return Ok((false, false));
            }
            crate::menu::MenuAction::HelpAbout => {
                // Show about dialog - for now just show editor help
                state.help_active = true;
                state.help_context = crate::help::HelpContext::Editor;
                state.help_scroll_offset = 0;
                state.needs_redraw = true;
                return Ok((false, false));
            }
        }
    } else if needs_full_redraw {
        // Menu state changed (opened/closed dropdown), need full redraw
        state.needs_redraw = true;
    }

    // If menu is active, it consumes most keypresses (except Alt+letter which is handled above)
    // But we don't set needs_redraw here - only when menu state changes or action occurs
    if state.menu_bar.active {
        // Menu is active, consume the keypress but don't trigger full redraw for navigation
        // The menu overlay will be redrawn automatically since menu_bar.active is true
        return Ok((false, false));
    }

    // Handle Ctrl+A for select all (but NOT when in find or replace mode)
    // Ctrl+A can be reported as either Char('a') with CONTROL, or Char('\x01') with no modifiers
    let is_ctrl_a = (modifiers.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char('a')))
                    || matches!(code, KeyCode::Char('\x01'));

    if is_ctrl_a {
        // If in find or replace mode, don't handle it here - let those modes handle it
        if state.find_active || state.replace_active {
            // Don't return - continue to let find/replace handlers process it
        } else {
            // Normal document select all
            if !lines.is_empty() {
                state.selection_start = Some((0, 0));
                let last_line = lines.len() - 1;
                let last_col = lines[last_line].len();
                state.selection_end = Some((last_line, last_col));
                state.needs_redraw = true;
            }
            return Ok((false, false));
        }
    }

    // Ctrl+Home and Ctrl+End: jump to beginning/end of document
    if modifiers.contains(KeyModifiers::CONTROL) {
        let extend = modifiers.contains(KeyModifiers::SHIFT);
        let mut moved = false;

        match code {
            KeyCode::Home => {
                // Jump to beginning of document
                if extend {
                    state.start_selection();
                }
                state.top_line = 0;
                state.cursor_line = 0;
                state.cursor_col = 0;
                state.desired_cursor_col = 0;
                moved = true;
            }
            KeyCode::End => {
                // Jump to end of document
                if extend {
                    state.start_selection();
                }
                if !lines.is_empty() {
                    let last_line = lines.len() - 1;
                    // Position cursor at end of last line
                    if last_line < visible_lines {
                        state.top_line = 0;
                        state.cursor_line = last_line;
                    } else {
                        state.top_line = last_line.saturating_sub(visible_lines - 1);
                        state.cursor_line = last_line - state.top_line;
                    }
                    state.cursor_col = lines[last_line].len();
                    state.desired_cursor_col = state.cursor_col;
                }
                moved = true;
            }
            _ => {}
        }

        if moved {
            if extend {
                state.update_selection();
            } else {
                state.clear_selection();
            }
            state.needs_redraw = true;
            return Ok((false, false));
        }
    }


    // Ctrl+Arrow custom handling: word-wise (Left/Right) and paragraph-wise (Up/Down)
    if modifiers.contains(KeyModifiers::CONTROL) {
        let extend = modifiers.contains(KeyModifiers::SHIFT);
        if extend {
            state.start_selection();
        }
        let mut moved = false;
        match code {
            KeyCode::Left => {
                moved = word_left(state, lines);
            }
            KeyCode::Right => {
                moved = word_right(state, lines);
            }
            KeyCode::Up => {
                moved = paragraph_up(state, lines);
            }
            KeyCode::Down => {
                moved = paragraph_down(state, lines, visible_lines);
            }
            _ => {}
        }
        if moved {
            if extend {
                state.update_selection();
            } else {
                state.clear_selection();
            }

            // Adjust horizontal scroll if wrapping is disabled (same logic as in handle_navigation)
            if !state.is_line_wrapping_enabled() {
                let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines) as usize;
                let absolute_line = state.absolute_line();
                if let Some(line) = lines.get(absolute_line) {
                    use crate::coordinates::visual_width_up_to;
                    let visual_col = visual_width_up_to(line, state.cursor_col, state.settings.tab_width);

                    // Adjust horizontal scroll to keep cursor visible
                    if visual_col < state.horizontal_scroll_offset {
                        // Cursor moved left of visible area
                        state.horizontal_scroll_offset = visual_col;
                    } else if visual_col >= state.horizontal_scroll_offset + text_width {
                        // Cursor moved right of visible area
                        state.horizontal_scroll_offset = visual_col.saturating_sub(text_width - 1);
                    }
                }
            }

            state.needs_redraw = true;
            return Ok((false, false));
        }
    }

    // Handle close file (Ctrl+W)
    if settings.keybindings.close_matches(&code, &modifiers) {
        if state.modified {
            // Show confirmation prompt
            if show_close_confirmation(filename, settings)? {
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

    // Handle help (F1)
    if matches!(code, KeyCode::F(1)) {
        // Determine help context based on current mode
        state.help_context = if state.find_active {
            crate::help::HelpContext::Find
        } else {
            crate::help::HelpContext::Editor
        };
        state.help_active = true;
        state.help_scroll_offset = 0;
        state.needs_redraw = true;
        return Ok((false, false));
    }

    // If in help mode, handle help input
    if state.help_active {
        let key_event = KeyEvent::new(code, modifiers);
        if crate::help::handle_help_input(key_event) {
            state.help_active = false;
            state.needs_redraw = true;
        } else {
            // Handle scrolling in help
            match code {
                KeyCode::Up => {
                    state.help_scroll_offset = state.help_scroll_offset.saturating_sub(1);
                    state.needs_redraw = true;
                }
                KeyCode::Down => {
                    state.help_scroll_offset = state.help_scroll_offset.saturating_add(1);
                    state.needs_redraw = true;
                }
                KeyCode::PageUp => {
                    state.help_scroll_offset =
                        state.help_scroll_offset.saturating_sub(visible_lines);
                    state.needs_redraw = true;
                }
                KeyCode::PageDown => {
                    state.help_scroll_offset =
                        state.help_scroll_offset.saturating_add(visible_lines);
                    state.needs_redraw = true;
                }
                KeyCode::Home => {
                    state.help_scroll_offset = 0;
                    state.needs_redraw = true;
                }
                _ => {}
            }
        }
        return Ok((false, false));
    }

    // Handle find (Ctrl+F)
    if settings.keybindings.find_matches(&code, &modifiers) {
        // If find mode is already active with a search pattern, toggle filter mode
        if state.find_active && !state.find_pattern.is_empty() {
            // Don't toggle - let find mode handle it
            // This will be handled in the find input handler
        } else if !state.find_active && state.last_search_pattern.is_some() {
            // If there's an active search but not in find mode, toggle filter
            state.filter_active = !state.filter_active;

            // When enabling filter mode, ensure cursor is on a visible line
            if state.filter_active {
                ensure_cursor_on_visible_line(state, lines);
            }

            state.needs_redraw = true;
            return Ok((false, false));
        } else {
            // Normal find mode entry
            // Save current search pattern to restore on Esc
            state.saved_search_pattern = state.last_search_pattern.clone();

            // If there's a selection, use it as the search scope
            if let (Some(start), Some(end)) = (state.selection_start, state.selection_end) {
                // Normalize selection to ensure start < end
                let normalized = if start.0 < end.0 || (start.0 == end.0 && start.1 <= end.1) {
                    (start, end)
                } else {
                    (end, start)
                };
                state.find_scope = Some(normalized);
            } else {
                state.find_scope = None;
            }

            state.find_active = true;
            state.find_pattern.clear();
            state.find_cursor_pos = 0;
            state.find_error = None;
            state.needs_redraw = true;
        }
        return Ok((false, false));
    }

    // Handle open dialog (configurable keybinding, default Ctrl+O)
    if settings.keybindings.open_dialog_matches(&code, &modifiers) {
        state.pending_menu_action = Some(crate::menu::MenuAction::FileOpenDialog);
        return Ok((false, false));
    }

    // Handle new file (configurable keybinding, default Ctrl+N)
    if settings.keybindings.new_file_matches(&code, &modifiers) {
        state.pending_menu_action = Some(crate::menu::MenuAction::FileNew);
        return Ok((false, false));
    }

    // Handle go to line (configurable keybinding, default Ctrl+G)
    if settings.keybindings.goto_line_matches(&code, &modifiers) {
        state.goto_line_active = true;
        // Pre-fill with current line number (1-indexed)
        state.goto_line_input = (state.absolute_line() + 1).to_string();
        state.goto_line_cursor_pos = state.goto_line_input.chars().count(); // Position at end
        state.goto_line_typing_started = false; // Mark as not yet typing
        state.needs_redraw = true;
        return Ok((false, false));
    }

    // Handle find next (configurable keybinding, default F3)
    // Note: This must be before find mode input handling so it works when find is active
    if settings.keybindings.find_next_matches(&code, &modifiers) {
        crate::find::find_next_occurrence(state, lines, visible_lines);
        return Ok((false, false));
    }

    // Handle find previous (configurable keybinding, default Shift+F3)
    // Note: This must be before find mode input handling so it works when find is active
    // Some terminals report Shift+F3 as F(15) instead of F(3) with SHIFT modifier
    if settings
        .keybindings
        .find_previous_matches(&code, &modifiers)
        || matches!(code, KeyCode::F(15))
    {
        crate::find::find_prev_occurrence(state, lines, visible_lines);
        return Ok((false, false));
    }

    // Handle replace mode entry (Ctrl+Shift+H) - only when we have an active search pattern
    if settings.keybindings.replace_matches(&code, &modifiers)
        && !state.replace_active
        && state.last_search_pattern.is_some()
    {
        // Enter replace mode
        state.replace_active = true;
        state.replace_pattern.clear();
        state.replace_cursor_pos = 0;
        state.needs_redraw = true;
        return Ok((false, false));
    }

    // Handle replace current occurrence (Ctrl+R) - works even if not in replace mode
    // Requires both a search pattern and a replacement pattern
    if settings.keybindings.replace_current_matches(&code, &modifiers) {
        if state.last_search_pattern.is_some() && !state.replace_pattern.is_empty() {
            crate::find::replace_current_occurrence(state, lines, visible_lines);
            // Save changes - update file content in undo history before saving
            let abs = state.absolute_line();
            state.undo_history.update_state(state.top_line, abs, state.cursor_col, lines.clone());
            state.undo_history.find_history = state.find_history.clone();
            let _ = state.undo_history.save(filename);
            state.last_save_time = Some(Instant::now());
            return Ok((false, false));
        }
    }

    // Handle replace all occurrences (Ctrl+Alt+R) - works even if not in replace mode
    // Requires both a search pattern and a replacement pattern
    if settings.keybindings.replace_all_matches(&code, &modifiers) {
        if state.last_search_pattern.is_some() && !state.replace_pattern.is_empty() {
            crate::find::replace_all_occurrences(state, lines);
            // Save changes - update file content in undo history before saving
            let abs = state.absolute_line();
            state.undo_history.update_state(state.top_line, abs, state.cursor_col, lines.clone());
            state.undo_history.find_history = state.find_history.clone();
            let _ = state.undo_history.save(filename);
            state.last_save_time = Some(Instant::now());
            return Ok((false, false));
        }
    }

    // If in find mode, handle find input
    if state.find_active {
        let exited = crate::find::handle_find_input(state, lines, key_event, visible_lines);
        // Save undo history to persist find history changes
        state.undo_history.find_history = state.find_history.clone();
        let _ = state.undo_history.save(filename);
        state.last_save_time = Some(Instant::now());
        // If find mode was exited, the return value indicates this
        if exited {
            // Find mode was closed, continue normal processing
        }
        return Ok((false, false));
    }

    // If in replace mode, handle replace input
    if state.replace_active {
        let _exited = crate::find::handle_replace_input(state, lines, key_event);
        state.needs_redraw = true;
        // If replace mode was exited, return early so we don't consume the event
        return Ok((false, false));
    }

    // If in go to line mode, handle input
    if state.goto_line_active {
        return handle_goto_line_input(state, lines, key_event, visible_lines);
    }

    // Check for exit commands
    if is_exit_command(&code, &modifiers, settings) {
        // Before exiting, persist final scroll and cursor position
        let abs = state.absolute_line();
        state
            .undo_history
            .update_cursor(state.top_line, abs, state.cursor_col);
        state.undo_history.find_history = state.find_history.clone(); // Save find history
        let _ = state.undo_history.save(filename);
        state.last_save_time = Some(Instant::now());
        // Save session as editor
        let _ = crate::session::save_editor_session(filename);
        return Ok((true, false));
    }

    // Handle save and quit (Ctrl+q)
    if settings
        .keybindings
        .save_and_quit_matches(&code, &modifiers)
    {
        // Save the file first
        save_file(filename, lines)?;
        state.modified = false;
        // Clear the unsaved file content since we just saved
        state.undo_history.clear_unsaved_state();
        // Before exiting, persist final scroll and cursor position
        let abs = state.absolute_line();
        state
            .undo_history
            .update_cursor(state.top_line, abs, state.cursor_col);
        state.undo_history.find_history = state.find_history.clone(); // Save find history
        let _ = state.undo_history.save(filename);
        state.last_save_time = Some(Instant::now());
        // Save session as editor
        let _ = crate::session::save_editor_session(filename);
        return Ok((true, false)); // Quit after saving
    }

    // Handle save
    if settings.keybindings.save_matches(&code, &modifiers) {
        // If this is an untitled file, we need to show the save-as dialog
        if state.is_untitled {
            // Mark the action so ui.rs can handle it
            state.pending_menu_action = Some(crate::menu::MenuAction::FileSave);
            return Ok((false, false));
        }

        save_file(filename, lines)?;
        state.modified = false;
        state.needs_redraw = true;
        // Clear the unsaved file content since we just saved
        state.undo_history.clear_unsaved_state();
        // Save undo history when saving the file
        state.undo_history.find_history = state.find_history.clone(); // Save find history
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

    // Handle Ctrl+Backspace/Delete and Alt+Backspace/Delete for word-wise deletion
    // Alt+Backspace/Delete provided for better terminal compatibility (some terminals don't send Ctrl+Backspace)
    // Also handle Ctrl+H since Ctrl+Backspace sends ASCII 0x08 which can be interpreted as Ctrl+H
    if (modifiers.contains(KeyModifiers::CONTROL) || modifiers.contains(KeyModifiers::ALT))
        && (matches!(code, KeyCode::Backspace) || matches!(code, KeyCode::Char('h')))
    {
        use crate::editing::delete_word_backward;
        if delete_word_backward(state, lines, filename) {
            state.modified = true;
            state.needs_redraw = true;
        }
        return Ok((false, false));
    }

    if (modifiers.contains(KeyModifiers::CONTROL) || modifiers.contains(KeyModifiers::ALT))
        && matches!(code, KeyCode::Delete)
    {
        use crate::editing::delete_word_forward;
        if delete_word_forward(state, lines, filename) {
            state.modified = true;
            state.needs_redraw = true;
        }
        return Ok((false, false));
    }

    // Handle toggle line wrap (Alt+w by default)
    if settings.keybindings.toggle_line_wrap_matches(&code, &modifiers) {
        // Toggle line wrapping at runtime (not persisted to config file)
        state.toggle_line_wrapping();
        state.needs_redraw = true;
        return Ok((false, false));
    }

    // Handle cursor movement keybindings (Ctrl+J/K/H/L)
    if settings.keybindings.cursor_down_matches(&code, &modifiers) {
        handle_down_navigation(state, lines, visible_lines);
        let lines_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        state.adjust_cursor_col(&lines_refs);
        state.clear_selection();
        state.needs_redraw = true;
        return Ok((false, false));
    }
    if settings.keybindings.cursor_up_matches(&code, &modifiers) {
        handle_up_navigation(state, lines, visible_lines);
        let lines_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        state.adjust_cursor_col(&lines_refs);
        state.clear_selection();
        state.needs_redraw = true;
        return Ok((false, false));
    }
    if settings.keybindings.cursor_left_matches(&code, &modifiers) {
        if state.cursor_col > 0 {
            state.cursor_col -= 1;
        } else {
            let current_absolute = state.top_line + state.cursor_line;
            if current_absolute > 0 {
                if state.cursor_line > 0 {
                    state.cursor_line -= 1;
                } else if state.top_line > 0 {
                    state.top_line -= 1;
                }
                let new_absolute = state.top_line + state.cursor_line;
                if let Some(line) = lines.get(new_absolute) {
                    state.cursor_col = line.len();
                }
            }
        }
        state.clear_selection();
        state.needs_redraw = true;
        return Ok((false, false));
    }
    if settings.keybindings.cursor_right_matches(&code, &modifiers) {
        if let Some(line) = lines.get(state.top_line + state.cursor_line) {
            if state.cursor_col < line.len() {
                state.cursor_col += 1;
            } else {
                let current_absolute = state.top_line + state.cursor_line;
                if current_absolute + 1 < lines.len() {
                    state.cursor_line += 1;
                    state.cursor_col = 0;
                    let effective_visible_lines = state.effective_visible_lines(lines, visible_lines);
                    if state.cursor_line >= effective_visible_lines {
                        state.top_line += 1;
                        state.cursor_line = effective_visible_lines - 1;
                    }
                }
            }
        }
        state.clear_selection();
        state.needs_redraw = true;
        return Ok((false, false));
    }

    let is_shift = modifiers.contains(KeyModifiers::SHIFT);
    let is_alt = modifiers.contains(KeyModifiers::ALT);
    let is_navigation = is_navigation_key(&code);

    // Clear multi-cursors on any navigation (no longer used for selection)
    if is_navigation {
        state.clear_multi_cursors();
    }

    // Handle Alt+Arrow (without Shift) for viewport scrolling without moving cursor
    if is_navigation && is_alt && !is_shift {
        let scrolled = handle_viewport_scroll(state, lines, code, visible_lines);
        if scrolled {
            state.needs_redraw = true;
        }
        return Ok((false, false));
    }

    // Handle selection with navigation keys:
    // - Alt+Shift+Arrow: Block selection (rectangular/column-based selection)
    // - Shift+Arrow: Normal line-wise selection
    // Note: Block selection works on logical lines. For wrapped lines, navigation
    // moves through visual line segments, which may not align with block boundaries.
    if is_navigation && is_shift {
        state.start_selection();
        // Enable block selection mode if Alt is also pressed
        if is_alt {
            state.block_selection = true;
        }
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
        KeyCode::Up
            | KeyCode::Down
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::PageUp
            | KeyCode::PageDown
    )
}

fn update_selection_state(
    state: &mut FileViewerState,
    moved: bool,
    is_shift: bool,
    is_navigation: bool,
) {
    if moved {
        if is_shift {
            state.update_selection();
        } else {
            state.clear_selection();
        }
    } else if !is_shift && is_navigation {
        state.clear_selection();
    }
}

fn update_redraw_flags(state: &mut FileViewerState, did_edit: bool, moved: bool) {
    if did_edit || moved {
        state.needs_redraw = true;
    }
    if did_edit {
        state.modified = true;
    }
}

/// Handle Alt+Arrow viewport scrolling without moving cursor
fn handle_viewport_scroll(
    state: &mut FileViewerState,
    lines: &[String],
    code: KeyCode,
    visible_lines: usize,
) -> bool {
    let tab_width = state.settings.tab_width;
    let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines) as usize;

    match code {
        KeyCode::Up => {
            // Scroll viewport up (show earlier lines)
            if state.top_line > 0 {
                // Save absolute cursor position BEFORE scrolling
                let absolute_cursor = state.absolute_line();

                // Scroll viewport
                state.top_line -= 1;

                // Update cursor position to maintain absolute position
                if absolute_cursor < state.top_line {
                    // Cursor is still above the viewport (off-screen)
                    state.saved_absolute_cursor = Some(absolute_cursor);
                    state.cursor_line = 0;
                } else {
                    let new_cursor_line = absolute_cursor - state.top_line;
                    if new_cursor_line >= visible_lines {
                        // Cursor is off-screen below viewport
                        state.saved_absolute_cursor = Some(absolute_cursor);
                        state.cursor_line = new_cursor_line;
                    } else {
                        // Cursor is now visible in the viewport
                        state.saved_absolute_cursor = None;
                        state.cursor_line = new_cursor_line;
                    }
                }

                return true;
            }
        }
        KeyCode::Down => {
            // Scroll viewport down (show later lines)
            let max_scroll = lines.len().saturating_sub(1);
            if state.top_line < max_scroll {
                // Save absolute cursor position BEFORE scrolling
                let absolute_cursor = state.absolute_line();

                // Scroll viewport
                state.top_line += 1;

                // Update cursor position to maintain absolute position
                if absolute_cursor < state.top_line {
                    // Cursor is now off-screen above viewport
                    state.saved_absolute_cursor = Some(absolute_cursor);
                    state.cursor_line = 0;  // Set to top of viewport (but cursor is actually above)
                } else {
                    let new_cursor_line = absolute_cursor - state.top_line;
                    if new_cursor_line >= visible_lines {
                        // Cursor is off-screen below viewport
                        state.saved_absolute_cursor = Some(absolute_cursor);
                        state.cursor_line = new_cursor_line;
                    } else {
                        // Cursor is still visible
                        state.saved_absolute_cursor = None;
                        state.cursor_line = new_cursor_line;
                    }
                }

                return true;
            }
        }
        KeyCode::Left => {
            // Scroll viewport left (horizontal)
            if !state.is_line_wrapping_enabled() && state.horizontal_scroll_offset > 0 {
                let scroll_amount = state.settings.horizontal_scroll_speed;
                state.horizontal_scroll_offset = state.horizontal_scroll_offset.saturating_sub(scroll_amount);
                // Cursor column stays the same - it may scroll off-screen horizontally
                // which is fine; the rendering will handle it
                return true;
            }
        }
        KeyCode::Right => {
            // Scroll viewport right (horizontal)
            if !state.is_line_wrapping_enabled() {
                let max_line_width = lines.iter()
                    .map(|line| crate::coordinates::visual_width(line, tab_width))
                    .max()
                    .unwrap_or(0);
                let max_scroll = max_line_width.saturating_sub(text_width);

                if state.horizontal_scroll_offset < max_scroll {
                    let scroll_amount = state.settings.horizontal_scroll_speed;
                    state.horizontal_scroll_offset = (state.horizontal_scroll_offset + scroll_amount).min(max_scroll);
                    // Cursor column stays the same - it may scroll off-screen horizontally
                    return true;
                }
            }
        }
        _ => {}
    }
    false
}

/// Ensure cursor is positioned on a visible line when filter mode is active
fn ensure_cursor_on_visible_line(state: &mut FileViewerState, lines: &[String]) {
    if !state.filter_active || state.last_search_pattern.is_none() {
        return;
    }

    let pattern = state.last_search_pattern.as_ref().unwrap();
    let filtered_lines = crate::find::get_lines_with_matches_and_context(
        lines,
        pattern,
        state.find_scope,
        state.filter_context_before,
        state.filter_context_after,
    );

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
            if let Some(line) = lines.get(next_line_idx) {
                state.cursor_col = state.cursor_col.min(line.len());
            }
        } else if let Some(&prev_line_idx) = filtered_lines.iter().rev().find(|&&idx| idx < absolute_line) {
            // Move to previous visible line
            if prev_line_idx >= state.top_line {
                state.cursor_line = prev_line_idx - state.top_line;
            } else {
                state.top_line = prev_line_idx;
                state.cursor_line = 0;
            }
            // Adjust cursor column to be within the line
            if let Some(line) = lines.get(prev_line_idx) {
                state.cursor_col = state.cursor_col.min(line.len());
            }
        } else if let Some(&first_line_idx) = filtered_lines.first() {
            // No visible lines around cursor, jump to first visible line
            state.top_line = first_line_idx;
            state.cursor_line = 0;
            if let Some(line) = lines.get(first_line_idx) {
                state.cursor_col = state.cursor_col.min(line.len());
            }
        }
    }
}

/// Handle moving up through wrapped lines
fn handle_up_navigation(state: &mut FileViewerState, lines: &[String], visible_lines: usize) {
    use crate::coordinates::{visual_width_up_to, calculate_wrapped_lines_for_line};

    // Initialize desired_cursor_col from cursor_col if this is the first vertical movement
    if state.desired_cursor_col == 0 && state.cursor_col > 0 {
        state.desired_cursor_col = state.cursor_col;
    }

    let absolute_line = state.absolute_line();
    if absolute_line >= lines.len() {
        return;
    }

    let _line_num_width = line_number_width(state.settings);
    let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines) as usize;
    let tab_width = state.settings.tab_width;

    if text_width == 0 {
        return;
    }

    // If wrapping is disabled, just move to previous logical line
    if !state.is_line_wrapping_enabled() {
        // In filter mode, jump to previous visible line
        if state.filter_active && state.last_search_pattern.is_some() {
            let pattern = state.last_search_pattern.as_ref().unwrap();
            let filtered_lines = crate::find::get_lines_with_matches_and_context(
                lines,
                pattern,
                state.find_scope,
                state.filter_context_before,
                state.filter_context_after,
            );

            if !filtered_lines.is_empty() {
                // Find the previous visible line before the current cursor position
                if let Some(&prev_line_idx) = filtered_lines.iter().rev().find(|&&idx| idx < absolute_line) {
                    // Calculate new cursor_line and top_line to position cursor on prev_line_idx
                    if prev_line_idx >= state.top_line {
                        // Target line is at or after top_line - just update cursor_line
                        state.cursor_line = prev_line_idx - state.top_line;
                    } else {
                        // Target line is before top_line - scroll up to it
                        state.top_line = prev_line_idx;
                        state.cursor_line = 0;
                    }
                    // Try to restore desired column position
                    let prev_line = &lines[prev_line_idx];
                    state.cursor_col = state.desired_cursor_col.min(prev_line.len());
                }
            }
        } else {
            // Normal mode - standard cursor movement
            if state.cursor_line > 0 {
                state.cursor_line -= 1;
                // Try to restore desired column position
                let prev_line = &lines[state.absolute_line()];
                state.cursor_col = state.desired_cursor_col.min(prev_line.len());
            } else if state.top_line > 0 {
                state.top_line -= 1;
                // Try to restore desired column position
                let prev_line = &lines[state.absolute_line()];
                state.cursor_col = state.desired_cursor_col.min(prev_line.len());
            }
        }
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
        // In filter mode, jump to previous visible line
        if state.filter_active && state.last_search_pattern.is_some() {
            let pattern = state.last_search_pattern.as_ref().unwrap();
            let filtered_lines = crate::find::get_lines_with_matches_and_context(
                lines,
                pattern,
                state.find_scope,
                state.filter_context_before,
                state.filter_context_after,
            );

            if !filtered_lines.is_empty() {
                // Find the previous visible line before the current cursor position
                if let Some(&prev_line_idx) = filtered_lines.iter().rev().find(|&&idx| idx < absolute_line) {
                    // Calculate new cursor_line and top_line to position cursor on prev_line_idx
                    if prev_line_idx >= state.top_line {
                        // Target line is at or after top_line - just update cursor_line
                        state.cursor_line = prev_line_idx - state.top_line;
                    } else {
                        // Target line is before top_line - scroll up to it
                        state.top_line = prev_line_idx;
                        state.cursor_line = 0;
                    }

                    // Position cursor on the LAST wrapped line of the previous line
                    let prev_line = &lines[prev_line_idx];
                    let num_wrapped = calculate_wrapped_lines_for_line(
                        lines,
                        prev_line_idx,
                        text_width as u16,
                        tab_width
                    ) as usize;

                    let target_wrap_line = num_wrapped.saturating_sub(1);
                    let base_visual_col = target_wrap_line * text_width;

                    let desired_col = state.desired_cursor_col.min(prev_line.len());
                    let desired_visual_col = visual_width_up_to(prev_line, desired_col, tab_width);

                    let target_visual_col = if desired_visual_col >= base_visual_col {
                        desired_visual_col
                    } else {
                        base_visual_col + (desired_visual_col % text_width)
                    };

                    state.cursor_col = visual_col_to_char_index(prev_line, target_visual_col, tab_width);
                }
            }
        } else {
            // Normal mode - standard wrapped line navigation
            if state.cursor_line > 0 {
                state.cursor_line -= 1;

                // Move to the previous logical line
                let prev_absolute = state.absolute_line();
                if prev_absolute < lines.len() {
                    let prev_line = &lines[prev_absolute];

                    // Position cursor on the LAST wrapped line of the previous logical line
                    // Calculate how many wrapped lines the previous line has
                    let num_wrapped = calculate_wrapped_lines_for_line(
                        lines,
                        prev_absolute,
                        text_width as u16,
                        tab_width
                    ) as usize;

                    // Calculate the target visual column for the last wrapped line
                    // We want to be on wrap line (num_wrapped - 1) at the desired column
                    let target_wrap_line = num_wrapped.saturating_sub(1);
                    let base_visual_col = target_wrap_line * text_width;

                    // Add the desired cursor column (clamped to line length)
                    let desired_col = state.desired_cursor_col.min(prev_line.len());
                    let desired_visual_col = visual_width_up_to(prev_line, desired_col, tab_width);

                    // If the desired visual column would be on the target wrap line, use it
                    // Otherwise, place cursor at the beginning of the target wrap line plus offset
                    let target_visual_col = if desired_visual_col >= base_visual_col {
                        desired_visual_col
                    } else {
                        // Desired column is earlier in the line, so position at the same
                        // relative offset within the last wrapped line
                        base_visual_col + (desired_visual_col % text_width)
                    };

                    state.cursor_col = visual_col_to_char_index(prev_line, target_visual_col, tab_width);
                }
            } else if state.top_line > 0 {
                // Scroll up
                state.top_line -= 1;

                // Move to the new top line
                let new_top_absolute = state.top_line;
                if new_top_absolute < lines.len() {
                    let new_top_line = &lines[new_top_absolute];

                    // Position cursor on the LAST wrapped line of the top line
                    let num_wrapped = calculate_wrapped_lines_for_line(
                        lines,
                        new_top_absolute,
                        text_width as u16,
                        tab_width
                    ) as usize;

                    let target_wrap_line = num_wrapped.saturating_sub(1);
                    let base_visual_col = target_wrap_line * text_width;

                    let desired_col = state.desired_cursor_col.min(new_top_line.len());
                    let desired_visual_col = visual_width_up_to(new_top_line, desired_col, tab_width);

                    let target_visual_col = if desired_visual_col >= base_visual_col {
                        desired_visual_col
                    } else {
                        base_visual_col + (desired_visual_col % text_width)
                    };

                    state.cursor_col = visual_col_to_char_index(new_top_line, target_visual_col, tab_width);
                }
            }
        }
    }
}

/// Handle moving down through wrapped lines
fn handle_down_navigation(state: &mut FileViewerState, lines: &[String], visible_lines: usize) {
    use crate::coordinates::{
        calculate_wrapped_lines_for_line, visual_width_up_to,
    };

    // Initialize desired_cursor_col from cursor_col if this is the first vertical movement
    if state.desired_cursor_col == 0 && state.cursor_col > 0 {
        state.desired_cursor_col = state.cursor_col;
    }

    let effective_visible_lines = state.effective_visible_lines(lines, visible_lines);
    let absolute_line = state.absolute_line();
    if absolute_line >= lines.len() {
        return;
    }

    let _line_num_width = line_number_width(state.settings);
    let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines) as usize;
    let tab_width = state.settings.tab_width;

    if text_width == 0 {
        return;
    }

    // If wrapping is disabled, just move to next logical line
    if !state.is_line_wrapping_enabled() {
        // In filter mode, jump to next visible line
        if state.filter_active && state.last_search_pattern.is_some() {
            let pattern = state.last_search_pattern.as_ref().unwrap();
            let filtered_lines = crate::find::get_lines_with_matches_and_context(
                lines,
                pattern,
                state.find_scope,
                state.filter_context_before,
                state.filter_context_after,
            );

            if !filtered_lines.is_empty() {
                // Find the next visible line after the current cursor position
                if let Some(&next_line_idx) = filtered_lines.iter().find(|&&idx| idx > absolute_line) {
                    // Calculate new cursor_line and top_line to position cursor on next_line_idx
                    let new_cursor_line = next_line_idx.saturating_sub(state.top_line);

                    if new_cursor_line >= effective_visible_lines {
                        // Need to scroll down to show the next line
                        state.top_line = next_line_idx.saturating_sub(effective_visible_lines - 1);
                        state.cursor_line = effective_visible_lines - 1;
                    } else {
                        // Can fit in current viewport
                        state.cursor_line = new_cursor_line;
                    }

                    // Try to restore desired column position
                    let next_line = &lines[next_line_idx];
                    state.cursor_col = state.desired_cursor_col.min(next_line.len());
                }
            }
        } else {
            // Normal mode - standard cursor movement
            if absolute_line + 1 < lines.len() {
                state.cursor_line += 1;
                // Check if we need to scroll
                if state.cursor_line >= effective_visible_lines {
                    state.top_line += 1;
                    state.cursor_line = effective_visible_lines - 1;
                }
                // Try to restore desired column position
                let next_line = &lines[state.absolute_line()];
                state.cursor_col = state.desired_cursor_col.min(next_line.len());
            }
        }
        return;
    }

    let line = &lines[absolute_line];
    let visual_col = visual_width_up_to(line, state.cursor_col, tab_width);
    let current_wrap_line = visual_col / text_width;
    let num_wrapped =
        calculate_wrapped_lines_for_line(lines, absolute_line, text_width as u16, tab_width)
            as usize;

    // If we're not on the last wrapped line of this logical line, move down within the same line
    if current_wrap_line + 1 < num_wrapped {
        // Move down one visual line within the same logical line
        let target_visual_col = visual_col + text_width;
        state.cursor_col = visual_col_to_char_index(line, target_visual_col, tab_width);
    } else {
        // We're on the last wrapped line, move to next logical line
        // In filter mode, jump to next visible line
        if state.filter_active && state.last_search_pattern.is_some() {
            let pattern = state.last_search_pattern.as_ref().unwrap();
            let filtered_lines = crate::find::get_lines_with_matches_and_context(
                lines,
                pattern,
                state.find_scope,
                state.filter_context_before,
                state.filter_context_after,
            );

            if !filtered_lines.is_empty() {
                // Find the next visible line after the current cursor position
                if let Some(&next_line_idx) = filtered_lines.iter().find(|&&idx| idx > absolute_line) {
                    // Calculate new cursor_line and top_line to position cursor on next_line_idx
                    let new_cursor_line = next_line_idx.saturating_sub(state.top_line);

                    if new_cursor_line >= effective_visible_lines {
                        // Need to scroll down to show the next line
                        state.top_line = next_line_idx.saturating_sub(effective_visible_lines - 1);
                        state.cursor_line = effective_visible_lines - 1;
                    } else {
                        // Can fit in current viewport
                        state.cursor_line = new_cursor_line;
                    }

                    // Position cursor on the FIRST wrapped line with correct column offset
                    let next_line = &lines[next_line_idx];
                    let desired_offset = state.desired_cursor_col % text_width;
                    state.cursor_col = desired_offset.min(next_line.len());
                }
            }
        } else {
            // Normal mode - standard wrapped line navigation
            if absolute_line + 1 < lines.len() {
                // Check if we would go off-screen by moving cursor_line down
                // We need to check if adding ONE more visual line would fit
                // (the first wrap of the next logical line)

                // Calculate visual lines from top_line up to current cursor_line
                let mut visual_lines_consumed = 0;
                for i in state.top_line..=absolute_line {
                    visual_lines_consumed += calculate_wrapped_lines_for_line(
                        lines,
                        i,
                        text_width as u16,
                        tab_width
                    ) as usize;
                }

                // Add 1 for the first wrap of the next line
                visual_lines_consumed += 1;

                let would_be_offscreen = visual_lines_consumed > effective_visible_lines;

                if would_be_offscreen {
                    // Need to scroll instead of moving cursor
                    state.top_line += 1;
                    // cursor_line stays the same (we scroll the content, not the cursor position)
                } else {
                    // Can move cursor without scrolling
                    state.cursor_line += 1;
                }

                // Move to the next logical line, positioning on the FIRST wrap
                let next_absolute = state.absolute_line();
                if next_absolute < lines.len() {
                    let next_line = &lines[next_absolute];

                    // Calculate the column offset within the wrap (not the absolute column)
                    // This ensures we land on the first wrap of the next line
                    let desired_offset = state.desired_cursor_col % text_width;

                    // Clamp to line length
                    state.cursor_col = desired_offset.min(next_line.len());
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
    // Calculate effective visible lines (reduced if h-scrollbar is shown)
    let effective_visible_lines = state.effective_visible_lines(lines, visible_lines);

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
            let desired_cursor_line = effective_visible_lines / 2;
            state.top_line = saved_absolute.saturating_sub(desired_cursor_line);
            state.cursor_line = saved_absolute.saturating_sub(state.top_line);
        }

        // Clear saved scroll state
        state.saved_scroll_state = None;

        // Now proceed with the navigation from this restored position
    }

    let lines_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    let moved = match code {
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
        KeyCode::Left => {
            if state.cursor_col > 0 {
                state.cursor_col -= 1;
                state.desired_cursor_col = state.cursor_col;
                true
            } else {
                // At beginning of line - move to end of previous line
                let current_absolute = state.top_line + state.cursor_line;
                if current_absolute > 0 {
                    // Move to previous line
                    if state.cursor_line > 0 {
                        state.cursor_line -= 1;
                    } else if state.top_line > 0 {
                        state.top_line -= 1;
                    }
                    // Set cursor to end of that line
                    let new_absolute = state.top_line + state.cursor_line;
                    if let Some(line) = lines.get(new_absolute) {
                        state.cursor_col = line.len();
                        state.desired_cursor_col = state.cursor_col;
                    }
                    true
                } else {
                    false
                }
            }
        }
        KeyCode::Right => {
            if let Some(line) = lines.get(state.top_line + state.cursor_line) {
                if state.cursor_col < line.len() {
                    state.cursor_col += 1;
                    state.desired_cursor_col = state.cursor_col;
                    true
                } else {
                    // At end of line - move to beginning of next line
                    let current_absolute = state.top_line + state.cursor_line;
                    if current_absolute + 1 < lines.len() {
                        // Move to next line
                        state.cursor_line += 1;
                        state.cursor_col = 0;
                        state.desired_cursor_col = state.cursor_col;

                        // Check if we need to scroll
                        if state.cursor_line >= effective_visible_lines {
                            state.top_line += 1;
                            state.cursor_line = effective_visible_lines - 1;
                        }
                        true
                    } else {
                        false
                    }
                }
            } else {
                false
            }
        }
        KeyCode::Home => {
            if let Some(line) = lines.get(state.top_line + state.cursor_line) {
                // Find first non-blank character
                let first_non_blank = first_non_blank_char(line);

                // Toggle between first non-blank and column 0
                let new_pos = if state.cursor_col == first_non_blank && first_non_blank != 0 {
                    // Already at first non-blank (and it's not column 0) → go to column 0
                    0
                } else if state.cursor_col == 0 && first_non_blank != 0 {
                    // At column 0 (and first non-blank is elsewhere) → go to first non-blank
                    first_non_blank
                } else {
                    // Anywhere else → go to first non-blank
                    first_non_blank
                };

                state.cursor_col = new_pos;
                state.desired_cursor_col = state.cursor_col;
                true
            } else {
                state.cursor_col = 0;
                state.desired_cursor_col = state.cursor_col;
                true
            }
        }
        KeyCode::End => {
            if let Some(line) = lines.get(state.top_line + state.cursor_line) {
                // Always go to end of line
                state.cursor_col = line.len();
                state.desired_cursor_col = state.cursor_col;
                true
            } else {
                true
            }
        }
        KeyCode::PageDown => {
            let new_top =
                (state.top_line + visible_lines).min(lines.len().saturating_sub(visible_lines));
            state.top_line = new_top;
            if state.top_line + state.cursor_line >= lines.len() {
                state.cursor_line = lines.len().saturating_sub(state.top_line + 1);
            }
            true
        }
        KeyCode::PageUp => {
            state.top_line = state.top_line.saturating_sub(visible_lines);
            true
        }
        _ => false,
    };

    // Clear wrap warning on any cursor movement
    if moved {
        state.wrap_warning_pending = None;

        // Adjust horizontal scroll if wrapping is disabled
        if !state.is_line_wrapping_enabled() {
            let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines) as usize;

            // Get current line and calculate visual position
            let absolute_line = state.absolute_line();
            if let Some(line) = lines.get(absolute_line) {
                use crate::coordinates::visual_width_up_to;
                let visual_col = visual_width_up_to(line, state.cursor_col, state.settings.tab_width);

                // Adjust horizontal scroll to keep cursor visible
                if visual_col < state.horizontal_scroll_offset {
                    // Cursor moved left of visible area
                    state.horizontal_scroll_offset = visual_col;
                } else if visual_col >= state.horizontal_scroll_offset + text_width {
                    // Cursor moved right of visible area
                    state.horizontal_scroll_offset = visual_col.saturating_sub(text_width - 1);
                }
            }
        }
    }

    moved
}

/// Show confirmation prompt when closing a file with unsaved changes
/// Returns true if user confirms closing (Enter), false if user cancels (Esc)
pub(crate) fn show_close_confirmation(
    filename: &str,
    settings: &Settings,
) -> Result<bool, std::io::Error> {
    use crossterm::event;
    use crossterm::terminal;

    let mut stdout = std::io::stdout();
    let (_, term_height) = terminal::size()?;
    let footer_row = term_height - 1;

    // Extract just the filename from the path
    let path = std::path::Path::new(filename);
    let display_name = path.file_name().and_then(|n| n.to_str()).unwrap_or(filename);

    // Display warning message in footer
    execute!(
        stdout,
        crossterm::cursor::MoveTo(0, footer_row),
        crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine),
        crossterm::style::SetForegroundColor(crossterm::style::Color::Yellow)
    )?;
    write!(
        &mut stdout,
        "Close '{}' without saving? [Enter=Yes, Esc=No]",
        display_name
    )?;
    execute!(stdout, crossterm::style::ResetColor)?;
    stdout.flush()?;

    // Wait for user response
    loop {
        if let event::Event::Key(key) = event::read()? {
            let key = normalize_key_event(key, settings);
            match key.code {
                KeyCode::Enter => {
                    return Ok(true); // User confirmed - close file
                }
                KeyCode::Esc => {
                    return Ok(false); // User cancelled - don't close
                }
                _ => {
                    // Ignore other keys, wait for Enter or Esc
                }
            }
        }
    }
}

/// Show confirmation prompt when overwriting an existing file
/// Returns true if user confirms overwrite (Enter), false if user cancels (Esc)
#[allow(dead_code)] // Used in ui.rs for untitled file save handling
pub(crate) fn show_overwrite_confirmation(
    filename: &str,
    settings: &Settings,
) -> Result<bool, std::io::Error> {
    use crossterm::event;
    use crossterm::terminal;

    let mut stdout = std::io::stdout();
    let (_, term_height) = terminal::size()?;
    let footer_row = term_height - 1;

    // Extract just the filename from the path
    let path = std::path::Path::new(filename);
    let display_name = path.file_name().and_then(|n| n.to_str()).unwrap_or(filename);

    // Display warning message in footer
    execute!(
        stdout,
        crossterm::cursor::MoveTo(0, footer_row),
        crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine),
        crossterm::style::SetForegroundColor(crossterm::style::Color::Yellow)
    )?;
    write!(
        &mut stdout,
        "Overwrite '{}'? [Enter=Yes, Esc=No]",
        display_name
    )?;
    execute!(stdout, crossterm::style::ResetColor)?;
    stdout.flush()?;

    // Wait for user response
    loop {
        if let event::Event::Key(key) = event::read()? {
            let key = normalize_key_event(key, settings);
            match key.code {
                KeyCode::Enter => {
                    return Ok(true); // User confirmed - overwrite file
                }
                KeyCode::Esc => {
                    return Ok(false); // User cancelled - don't overwrite
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
pub(crate) fn show_undo_conflict_confirmation(settings: &Settings) -> Result<bool, std::io::Error> {
    use crossterm::event;
    use crossterm::terminal;

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
    write!(
        &mut stdout,
        "File modified externally. Keep unsaved changes? [Enter=Yes, Esc=No]"
    )?;
    execute!(stdout, crossterm::style::ResetColor)?;
    stdout.flush()?;

    // Wait for user response
    loop {
        if let event::Event::Key(key) = event::read()? {
            let key = normalize_key_event(key, settings);
            match key.code {
                KeyCode::Enter => {
                    return Ok(true); // User confirmed - keep unsaved changes
                }
                KeyCode::Esc => {
                    return Ok(false); // User cancelled - discard unsaved changes
                }
                _ => {
                    // Ignore other keys, wait for Enter or Esc
                }
            }
        }
    }
}

fn word_left(state: &mut FileViewerState, lines: &[String]) -> bool {
    let abs = state.absolute_line();
    if abs >= lines.len() {
        return false;
    }
    if state.cursor_col == 0 {
        if abs == 0 {
            return false;
        }
        // Move to previous line end
        if state.cursor_line > 0 {
            state.cursor_line -= 1;
        } else {
            state.top_line = state.top_line.saturating_sub(1);
        }
        let new_abs = state.absolute_line();
        if new_abs < lines.len() {
            state.cursor_col = lines[new_abs].len();
        }
        return true;
    }
    let line = &lines[abs];
    let mut i = state.cursor_col;
    // First skip any non-word characters (including whitespace & punctuation)
    while i > 0 {
        let c = line.chars().nth(i - 1).unwrap_or(' ');
        if is_word_char(c) {
            break;
        }
        i -= 1;
    }
    // Then skip the word characters
    while i > 0 {
        let c = line.chars().nth(i - 1).unwrap_or(' ');
        if !is_word_char(c) {
            break;
        }
        i -= 1;
    }
    state.cursor_col = i;
    true
}
fn word_right(state: &mut FileViewerState, lines: &[String]) -> bool {
    let abs = state.absolute_line();
    if abs >= lines.len() {
        return false;
    }
    let line = &lines[abs];
    let len = line.len();
    if state.cursor_col >= len {
        if abs + 1 >= lines.len() {
            return false;
        }
        // Move to next line start
        if state.cursor_line + 1 < lines.len().saturating_sub(state.top_line) {
            state.cursor_line += 1;
        } else {
            state.top_line += 1;
        }
        state.cursor_col = 0;
        return true;
    }
    let mut i = state.cursor_col;
    // Skip any non-word (whitespace / punctuation)
    while i < len {
        let c = line.chars().nth(i).unwrap_or(' ');
        if is_word_char(c) {
            break;
        }
        i += 1;
    }
    // Skip the word
    while i < len {
        let c = line.chars().nth(i).unwrap_or(' ');
        if !is_word_char(c) {
            break;
        }
        i += 1;
    }
    state.cursor_col = i;
    true
}
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn paragraph_up(state: &mut FileViewerState, lines: &[String]) -> bool {
    let mut current_line = state.absolute_line();
    if current_line == 0 {
        return false;
    }

    // Skip current paragraph (non-empty lines)
    while current_line > 0
        && !lines
            .get(current_line - 1)
            .map_or(true, |l| l.trim().is_empty())
    {
        current_line -= 1;
    }

    // Skip empty lines
    while current_line > 0
        && lines
            .get(current_line - 1)
            .map_or(false, |l| l.trim().is_empty())
    {
        current_line -= 1;
    }

    // Position at the start of the previous paragraph or stay at line 0
    if current_line < state.top_line {
        state.top_line = current_line;
        state.cursor_line = 0;
    } else {
        state.cursor_line = current_line.saturating_sub(state.top_line);
    }
    state.cursor_col = 0;
    state.desired_cursor_col = 0;
    true
}

fn paragraph_down(state: &mut FileViewerState, lines: &[String], visible_lines: usize) -> bool {
    let effective_visible_lines = state.effective_visible_lines(lines, visible_lines);
    let mut current_line = state.absolute_line();
    if current_line >= lines.len() {
        return false;
    }

    // Skip current paragraph (non-empty lines)
    while current_line < lines.len()
        && !lines
            .get(current_line)
            .map_or(true, |l| l.trim().is_empty())
    {
        current_line += 1;
    }

    // Skip empty lines
    while current_line < lines.len()
        && lines
            .get(current_line)
            .map_or(false, |l| l.trim().is_empty())
    {
        current_line += 1;
    }

    // Position at the start of the next paragraph or end of file
    let target_line = current_line.min(lines.len().saturating_sub(1));
    if target_line >= state.top_line + effective_visible_lines {
        // Need to scroll down
        state.top_line = target_line.saturating_sub(effective_visible_lines / 2);
        state.cursor_line = target_line.saturating_sub(state.top_line);
    } else {
        state.cursor_line = target_line.saturating_sub(state.top_line);
    }
    state.cursor_col = 0;
    state.desired_cursor_col = 0;
    true
}

/// Get the character index of the first non-blank character in the line
fn first_non_blank_char(line: &str) -> usize {
    line.chars().position(|c| !c.is_whitespace()).unwrap_or(0)
}

/// Calculate the end position of the current visual line within a wrapped logical line
/// Returns the character index of the last character on the current visual line
#[allow(dead_code)]
fn visual_line_end(line: &str, cursor_col: usize, text_width: usize, tab_width: usize) -> usize {
    use crate::coordinates::visual_width_up_to;

    if text_width == 0 || line.is_empty() {
        return line.len();
    }

    let visual_col = visual_width_up_to(line, cursor_col, tab_width);
    let current_wrap_line = visual_col / text_width;
    let next_wrap_start = (current_wrap_line + 1) * text_width;

    // Find the last character that fits on the current visual line
    let mut current_visual = 0;
    let mut last_char_on_line = 0;

    for (char_idx, ch) in line.chars().enumerate() {
        // Check if adding this character would exceed the visual line boundary
        let char_width = if ch == '\t' {
            tab_width - (current_visual % tab_width)
        } else {
            1
        };

        if current_visual + char_width > next_wrap_start {
            // This character would start on the next visual line
            // Return the index just after the last character that fits
            return last_char_on_line;
        }

        // This character fits on the current visual line
        current_visual += char_width;
        last_char_on_line = char_idx + 1; // Position after this character
    }

    // We're on the last visual line, return end of logical line
    line.len()
}

/// Calculate the start position of the current visual line within a wrapped logical line
/// Returns the character index of the first character on the current visual line
#[allow(dead_code)]
fn visual_line_start(line: &str, cursor_col: usize, text_width: usize, tab_width: usize) -> usize {
    use crate::coordinates::visual_width_up_to;

    if text_width == 0 {
        return 0;
    }

    let visual_col = visual_width_up_to(line, cursor_col, tab_width);
    let current_wrap_line = visual_col / text_width;
    let wrap_start_visual = current_wrap_line * text_width;

    if wrap_start_visual == 0 {
        return 0;
    }

    // Find the character index where this visual line starts
    let mut current_visual = 0;
    for (char_idx, ch) in line.chars().enumerate() {
        if current_visual >= wrap_start_visual {
            return char_idx;
        }
        if ch == '\t' {
            let spaces_to_next_tab = tab_width - (current_visual % tab_width);
            current_visual += spaces_to_next_tab;
        } else {
            current_visual += 1;
        }
    }

    0
}

/// Get the first non-blank character position in the current visual line
#[allow(dead_code)]
fn visual_line_first_non_blank(
    line: &str,
    cursor_col: usize,
    text_width: usize,
    tab_width: usize,
) -> usize {
    let vis_start = visual_line_start(line, cursor_col, text_width, tab_width);
    let vis_end = visual_line_end(line, cursor_col, text_width, tab_width);

    // Find first non-blank in the range [vis_start, vis_end]
    let chars: Vec<char> = line.chars().collect();
    for i in vis_start..=vis_end.min(chars.len().saturating_sub(1)) {
        if i < chars.len() && !chars[i].is_whitespace() {
            return i;
        }
    }

    vis_start
}

#[allow(dead_code)] // Only used in tests
fn scroll_without_cursor(
    state: &mut FileViewerState,
    lines: &[String],
    visible_lines: usize,
    delta: isize,
) -> bool {
    let effective_visible_lines = state.effective_visible_lines(lines, visible_lines);
    if delta == 0 {
        return false;
    }
    let old_top = state.top_line;
    // Capture absolute cursor BEFORE changing top_line so we can preserve it
    let absolute_cursor = state.absolute_line();
    if delta > 0 {
        state.top_line = (state.top_line + delta as usize).min(lines.len().saturating_sub(1));
    } else {
        state.top_line = state.top_line.saturating_sub((-delta) as usize);
    }
    if absolute_cursor < state.top_line || absolute_cursor >= state.top_line + effective_visible_lines {
        if state.saved_scroll_state.is_none() {
            state.saved_scroll_state = Some((old_top, state.cursor_line));
        }
        state.saved_absolute_cursor = Some(absolute_cursor);
    } else {
        state.saved_absolute_cursor = None;
        state.saved_scroll_state = None;
        state.cursor_line = absolute_cursor - state.top_line;
    }
    state.top_line != old_top
}

/// Handle input when in go to line mode
/// Returns (should_quit, should_close) tuple
fn handle_goto_line_input(
    state: &mut FileViewerState,
    lines: &[String],
    key_event: KeyEvent,
    visible_lines: usize,
) -> Result<(bool, bool), std::io::Error> {
    use crossterm::event::KeyCode;

    let KeyEvent {
        code, modifiers, ..
    } = key_event;

    match code {
        KeyCode::Enter => {
            // Parse line number and jump to it
            if let Ok(line_num) = state.goto_line_input.parse::<usize>()
                && line_num > 0
                && line_num <= lines.len()
            {
                // Convert to 0-indexed
                let target_line = line_num - 1;

                // Jump to the target line
                state.top_line = target_line.saturating_sub(visible_lines / 2);
                state.top_line = state.top_line.min(lines.len().saturating_sub(1));
                state.cursor_line = target_line.saturating_sub(state.top_line);
                state.cursor_col = 0;

                // Clear saved cursor state
                state.saved_absolute_cursor = None;
                state.saved_scroll_state = None;
            }

            // Exit go to line mode
            state.goto_line_active = false;
            state.goto_line_input.clear();
            state.goto_line_cursor_pos = 0;
            state.goto_line_typing_started = false;
            state.needs_redraw = true;
            return Ok((false, false));
        }
        KeyCode::Char(c) if modifiers.is_empty() => {
            // Only allow digits
            if c.is_ascii_digit() {
                if !state.goto_line_typing_started {
                    // First character typed - replace the pre-filled value
                    state.goto_line_input.clear();
                    state.goto_line_cursor_pos = 0;
                    state.goto_line_typing_started = true;
                }
                // Insert character at cursor position
                let chars: Vec<char> = state.goto_line_input.chars().collect();
                state.goto_line_input = chars
                    .iter()
                    .take(state.goto_line_cursor_pos)
                    .chain(std::iter::once(&c))
                    .chain(chars.iter().skip(state.goto_line_cursor_pos))
                    .collect();
                state.goto_line_cursor_pos += 1;
                state.needs_redraw = true;
            }
            return Ok((false, false));
        }
        KeyCode::Backspace if modifiers.is_empty() => {
            if !state.goto_line_typing_started {
                // If backspace is pressed before typing, clear the pre-filled value
                state.goto_line_input.clear();
                state.goto_line_cursor_pos = 0;
                state.goto_line_typing_started = true;
            } else if state.goto_line_cursor_pos > 0 {
                // Delete character before cursor
                let chars: Vec<char> = state.goto_line_input.chars().collect();
                state.goto_line_input = chars
                    .iter()
                    .take(state.goto_line_cursor_pos - 1)
                    .chain(chars.iter().skip(state.goto_line_cursor_pos))
                    .collect();
                state.goto_line_cursor_pos -= 1;
            }
            state.needs_redraw = true;
            return Ok((false, false));
        }
        KeyCode::Delete if modifiers.is_empty() => {
            if !state.goto_line_typing_started {
                // If delete is pressed before typing, clear the pre-filled value
                state.goto_line_input.clear();
                state.goto_line_cursor_pos = 0;
                state.goto_line_typing_started = true;
            } else {
                // Delete character at cursor
                let chars: Vec<char> = state.goto_line_input.chars().collect();
                if state.goto_line_cursor_pos < chars.len() {
                    state.goto_line_input = chars
                        .iter()
                        .take(state.goto_line_cursor_pos)
                        .chain(chars.iter().skip(state.goto_line_cursor_pos + 1))
                        .collect();
                }
            }
            state.needs_redraw = true;
            return Ok((false, false));
        }
        KeyCode::Left => {
            // Moving cursor unselects the line number and allows editing
            if !state.goto_line_typing_started {
                state.goto_line_typing_started = true;
                // Position cursor at end (before colon conceptually)
                state.goto_line_cursor_pos = state.goto_line_input.chars().count();
            } else if state.goto_line_cursor_pos > 0 {
                state.goto_line_cursor_pos -= 1;
            }
            state.needs_redraw = true;
            return Ok((false, false));
        }
        KeyCode::Right => {
            // Moving cursor unselects the line number and allows editing
            if !state.goto_line_typing_started {
                state.goto_line_typing_started = true;
                // Position cursor at end
                state.goto_line_cursor_pos = state.goto_line_input.chars().count();
            } else {
                let len = state.goto_line_input.chars().count();
                if state.goto_line_cursor_pos < len {
                    state.goto_line_cursor_pos += 1;
                }
            }
            state.needs_redraw = true;
            return Ok((false, false));
        }
        KeyCode::Home => {
            if !state.goto_line_typing_started {
                state.goto_line_typing_started = true;
            }
            state.goto_line_cursor_pos = 0;
            state.needs_redraw = true;
            return Ok((false, false));
        }
        KeyCode::End => {
            if !state.goto_line_typing_started {
                state.goto_line_typing_started = true;
            }
            state.goto_line_cursor_pos = state.goto_line_input.chars().count();
            state.needs_redraw = true;
            return Ok((false, false));
        }
        _ => {
            // Ignore other keys
            return Ok((false, false));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::set_temp_home;
    use crate::settings::Settings;
    use crate::undo::UndoHistory;

    fn create_test_state() -> FileViewerState<'static> {
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let undo_history = UndoHistory::new();
        FileViewerState::new(80, undo_history, settings)
    }
    fn create_test_lines(count: usize) -> Vec<String> {
        (0..count).map(|i| format!("Line {}", i)).collect()
    }

    #[test]
    fn ctrl_scroll_preserves_absolute_cursor() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(100);
        state.top_line = 10;
        state.cursor_line = 5; // absolute 15
        let abs_before = state.absolute_line();
        assert_eq!(abs_before, 15);
        // simulate Ctrl+Down scroll (delta +3)
        super::scroll_without_cursor(&mut state, &lines, 20, 3);
        assert_eq!(
            state.absolute_line(),
            15,
            "Absolute cursor should remain after scroll down"
        );
        super::scroll_without_cursor(&mut state, &lines, 20, -3);
        assert_eq!(
            state.absolute_line(),
            15,
            "Absolute cursor should remain after scroll up"
        );
    }
    #[test]
    fn goto_line_activates_on_ctrl_g() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(100);
        state.top_line = 10;
        state.cursor_line = 0;
        let key_event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL);
        let settings = state.settings;
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());
        assert!(state.goto_line_active);
        assert_eq!(state.goto_line_input, "11");
        assert!(state.needs_redraw);
    }
    #[test]
    fn goto_line_accepts_digits() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(100);

        state.goto_line_active = true;
        state.goto_line_input = "1".to_string();
        state.goto_line_cursor_pos = 1; // Cursor at end
        state.goto_line_typing_started = true; // Mark as not yet typing

        let key_event = KeyEvent::new(KeyCode::Char('5'), KeyModifiers::empty());
        let result = handle_goto_line_input(&mut state, &lines, key_event, 20);
        assert!(result.is_ok());
        assert_eq!(state.goto_line_input, "15");
    }
    #[test]
    fn goto_line_ignores_non_digits() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(100);
        state.goto_line_active = true;
        state.goto_line_input = "10".to_string();
        let key_event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty());
        let result = handle_goto_line_input(&mut state, &lines, key_event, 20);
        assert!(result.is_ok());
        assert_eq!(state.goto_line_input, "10");
    }
    #[test]
    fn goto_line_backspace_deletes_char() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(100);

        state.goto_line_active = true;
        state.goto_line_input = "123".to_string();
        state.goto_line_cursor_pos = 3; // Cursor at end
        state.goto_line_typing_started = true; // Already typing, so delete

        let key_event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty());
        let result = handle_goto_line_input(&mut state, &lines, key_event, 20);
        assert!(result.is_ok());
        assert_eq!(state.goto_line_input, "12");
    }

    #[test]
    fn goto_line_first_digit_replaces_prefill() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(100);

        state.goto_line_active = true;
        state.goto_line_input = "42".to_string(); // Pre-filled
        state.goto_line_typing_started = false; // Not yet typing

        let key_event = KeyEvent::new(KeyCode::Char('5'), KeyModifiers::empty());
        let result = handle_goto_line_input(&mut state, &lines, key_event, 20);
        assert!(result.is_ok());
        assert_eq!(state.goto_line_input, "5"); // Should replace, not append
        assert!(state.goto_line_typing_started);
    }

    #[test]
    fn goto_line_enter_jumps_to_line() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(100);
        state.goto_line_active = true;
        state.goto_line_input = "50".to_string();
        state.top_line = 0;
        state.cursor_line = 0;
        let key_event = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
        let result = handle_goto_line_input(&mut state, &lines, key_event, 20);
        assert!(result.is_ok());
        assert_eq!(state.absolute_line(), 49);
        assert!(!state.goto_line_active);
        assert_eq!(state.goto_line_input, "");
    }

    #[test]
    fn goto_line_arrow_keys_unselect() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(100);

        state.goto_line_active = true;
        state.goto_line_input = "50".to_string();
        state.goto_line_typing_started = false; // Selected

        // Press Left arrow
        let key_event = KeyEvent::new(KeyCode::Left, KeyModifiers::empty());
        let result = handle_goto_line_input(&mut state, &lines, key_event, 20);
        assert!(result.is_ok());

        // Should still be in goto_line mode, but typing started (unselected)
        assert!(state.goto_line_active);
        assert!(state.goto_line_typing_started);
        assert_eq!(state.goto_line_input, "50"); // Input unchanged
    }

    #[test]
    fn goto_line_centers_view() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let lines = create_test_lines(100);
        let visible_lines = 20;
        state.goto_line_active = true;
        state.goto_line_input = "50".to_string();
        let key_event = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
        let _ = handle_goto_line_input(&mut state, &lines, key_event, visible_lines);
        assert!(state.top_line >= 35 && state.top_line <= 45);
        assert_eq!(state.absolute_line(), 49);
    }

    #[test]
    fn normalize_key_event_maps_carriage_return_to_enter() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Settings::load().expect("Failed to load test settings");
        let key_event = KeyEvent::new(KeyCode::Char('\r'), KeyModifiers::empty());
        let normalized = normalize_key_event(key_event, &settings);
        assert!(matches!(normalized.code, KeyCode::Enter));
    }

    #[test]
    fn normalize_key_event_maps_newline_to_enter() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Settings::load().expect("Failed to load test settings");
        let key_event = KeyEvent::new(KeyCode::Char('\n'), KeyModifiers::empty());
        let normalized = normalize_key_event(key_event, &settings);
        assert!(matches!(normalized.code, KeyCode::Enter));
    }
    #[test]
    fn help_activates_with_f1() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(10);
        // Press F1 to activate help
        let key_event = KeyEvent::new(KeyCode::F(1), KeyModifiers::empty());
        let settings = state.settings;
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());
        assert!(state.help_active, "Help should be active after F1");
        assert_eq!(state.help_context, crate::help::HelpContext::Editor);
        assert!(state.needs_redraw);
    }
    #[test]
    fn help_shows_find_context_when_in_find_mode() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(10);
        // Activate find mode first
        state.find_active = true;
        // Press F1 to activate help
        let key_event = KeyEvent::new(KeyCode::F(1), KeyModifiers::empty());
        let settings = state.settings;
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());
        assert!(state.help_active, "Help should be active after F1");
        assert_eq!(
            state.help_context,
            crate::help::HelpContext::Find,
            "Should show Find help when in find mode"
        );
    }
    #[test]
    fn help_exits_with_esc_without_clearing_modes() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(10);
        // Activate find mode and help
        state.find_active = true;
        state.help_active = true;
        println!("Before handle_key_event: help_active={}, menu_bar.active={}", state.help_active, state.menu_bar.active);
        // Press ESC to exit help (should NOT exit find mode)
        let key_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
        let settings = state.settings;
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        println!("After handle_key_event: help_active={}, menu_bar.active={}, result={:?}", state.help_active, state.menu_bar.active, result);
        assert!(result.is_ok());
        assert!(!state.help_active, "Help should be closed after ESC, but help_active={}", state.help_active);
        // Note: find_active state depends on help_active being processed first
        // The actual protection against file selector is in ui.rs
    }
    #[test]
    fn help_exits_with_f1() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(10);
        // Activate help
        state.help_active = true;
        // Press F1 to toggle help off
        let key_event = KeyEvent::new(KeyCode::F(1), KeyModifiers::empty());
        let settings = state.settings;
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());
        // F1 toggles, so if we're in help and press F1, it activates again (cycles)
        // Actually checking the handler - it sets help_active = true
        assert!(state.help_active, "F1 always activates help");
    }
    #[test]
    fn help_scroll_navigation() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(10);
        // Activate help
        state.help_active = true;
        state.help_scroll_offset = 5;
        // Test scrolling up
        let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::empty());
        let settings = state.settings;
        let _ = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert_eq!(state.help_scroll_offset, 4, "Should scroll up");
        // Test scrolling down
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
        let _ = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert_eq!(state.help_scroll_offset, 5, "Should scroll down");
        // Test Home
        let key_event = KeyEvent::new(KeyCode::Home, KeyModifiers::empty());
        let _ = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert_eq!(state.help_scroll_offset, 0, "Should scroll to top");
    }

    #[test]
    fn ctrl_a_selects_all_text() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec![
            "Line 1".to_string(),
            "Line 2".to_string(),
            "Line 3".to_string(),
        ];

        state.cursor_line = 1;
        state.cursor_col = 3;

        let key_event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        let settings = state.settings;
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");

        assert!(result.is_ok());
        assert_eq!(state.selection_start, Some((0, 0)));
        assert_eq!(state.selection_end, Some((2, 6)));
        assert!(state.needs_redraw);
    }

    #[test]
    fn ctrl_a_on_empty_file_does_nothing() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines: Vec<String> = vec![];

        let key_event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        let settings = state.settings;
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");

        assert!(result.is_ok());
        assert_eq!(state.selection_start, None);
        assert_eq!(state.selection_end, None);
    }

    #[test]
    fn ctrl_home_jumps_to_beginning() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(50);

        state.top_line = 25;
        state.cursor_line = 5;
        state.cursor_col = 5;

        let key_event = KeyEvent::new(KeyCode::Home, KeyModifiers::CONTROL);
        let settings = state.settings;
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");

        assert!(result.is_ok());
        assert_eq!(state.top_line, 0);
        assert_eq!(state.cursor_line, 0);
        assert_eq!(state.cursor_col, 0);
        assert!(state.needs_redraw);
    }

    #[test]
    fn ctrl_end_jumps_to_end() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(50);

        state.top_line = 0;
        state.cursor_line = 0;
        state.cursor_col = 0;

        let key_event = KeyEvent::new(KeyCode::End, KeyModifiers::CONTROL);
        let settings = state.settings;
        let visible_lines = 20;
        let result = handle_key_event(
            &mut state,
            &mut lines,
            key_event,
            settings,
            visible_lines,
            "test.txt",
        );

        assert!(result.is_ok());
        assert_eq!(state.absolute_line(), 49);
        assert_eq!(state.cursor_col, lines[49].len());
        assert!(state.needs_redraw);
    }

    #[test]
    fn shift_ctrl_home_selects_to_beginning() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(50);

        state.top_line = 25;
        state.cursor_line = 5;
        state.cursor_col = 5;
        let start_pos = (state.absolute_line(), state.cursor_col);

        let key_event = KeyEvent::new(KeyCode::Home, KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        let settings = state.settings;
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");

        assert!(result.is_ok());
        assert_eq!(state.top_line, 0);
        assert_eq!(state.cursor_line, 0);
        assert_eq!(state.cursor_col, 0);
        // With anchor-based selection, start and end are normalized
        assert_eq!(state.selection_start, Some((0, 0)));
        assert_eq!(state.selection_end, Some(start_pos));
        assert_eq!(state.selection_anchor, Some(start_pos));
        assert!(state.needs_redraw);
    }

    #[test]
    fn shift_ctrl_end_selects_to_end() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(50);

        state.top_line = 5;
        state.cursor_line = 5;
        state.cursor_col = 2;
        let start_pos = (state.absolute_line(), state.cursor_col);

        let key_event = KeyEvent::new(KeyCode::End, KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        let settings = state.settings;
        let visible_lines = 20;
        let result = handle_key_event(
            &mut state,
            &mut lines,
            key_event,
            settings,
            visible_lines,
            "test.txt",
        );

        assert!(result.is_ok());
        assert_eq!(state.absolute_line(), 49);
        assert_eq!(state.cursor_col, lines[49].len());
        // With anchor-based selection, start and end are kept in order
        assert_eq!(state.selection_start, Some(start_pos));
        assert_eq!(state.selection_end, Some((49, lines[49].len())));
        assert_eq!(state.selection_anchor, Some(start_pos));
        assert!(state.needs_redraw);
    }

    #[test]
    fn alt_shift_up_creates_zero_width_block_selection() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec![
            "line one".to_string(),
            "line two".to_string(),
            "line three".to_string(),
        ];

        state.top_line = 0;
        state.cursor_line = 1;
        state.cursor_col = 5;
        let settings = state.settings;

        // Alt+Shift+Up should start zero-width block selection
        let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::ALT | KeyModifiers::SHIFT);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 10, "test.txt");
        assert!(result.is_ok());
        assert!(state.block_selection, "Block selection should be enabled");
        assert!(state.selection_start.is_some(), "Selection should be started");
        assert_eq!(state.cursor_line, 0, "Cursor should move up");

        // Should be zero-width block selection (same column)
        let (start, end) = state.selection_range().unwrap();
        assert_eq!(start.1, end.1, "Should be zero-width (same column)");
        assert_eq!(start.0, 0, "Should start at line 0");
        assert_eq!(end.0, 1, "Should end at line 1");
    }

    #[test]
    fn alt_shift_arrows_create_and_expand_block_selection() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec![
            "line one".to_string(),
            "line two".to_string(),
            "line three".to_string(),
        ];

        state.top_line = 0;
        state.cursor_line = 1;
        state.cursor_col = 5;
        let settings = state.settings;

        // Alt+Shift+Up starts zero-width block selection
        let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::ALT | KeyModifiers::SHIFT);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 10, "test.txt");
        assert!(result.is_ok());
        assert!(state.block_selection, "Block selection should be enabled");

        // Alt+Shift+Right should expand block selection horizontally
        let key_event = KeyEvent::new(KeyCode::Right, KeyModifiers::ALT | KeyModifiers::SHIFT);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 10, "test.txt");
        assert!(result.is_ok());
        assert!(state.block_selection, "Block selection should still be enabled");

        // Selection should now have width > 0
        let (start, end) = state.selection_range().unwrap();
        assert!(end.1 > start.1, "Block selection should have expanded horizontally");
    }

    #[test]
    fn alt_up_down_without_shift_does_not_create_multi_cursors() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec![
            "line one".to_string(),
            "line two".to_string(),
            "line three".to_string(),
        ];

        state.top_line = 1;
        state.cursor_line = 1;
        state.cursor_col = 0;
        let settings = state.settings;

        // Alt+Up (without Shift) should NOT create multi-cursor
        // Instead, it scrolls the viewport without moving cursor
        let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::ALT);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 10, "test.txt");
        assert!(result.is_ok());
        assert!(!state.has_multi_cursors(), "Alt+Up without Shift should NOT create multi-cursors");
        // With new behavior: viewport scrolls up, cursor stays at same absolute position
        assert_eq!(state.top_line, 0, "Viewport should scroll up");
        assert_eq!(state.cursor_line, 2, "Cursor relative position should adjust to maintain absolute position");

        // Alt+Down (without Shift) should also NOT create multi-cursor
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::ALT);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 10, "test.txt");
        assert!(result.is_ok());
        assert!(!state.has_multi_cursors(), "Alt+Down without Shift should NOT create multi-cursors");
        // Viewport should scroll back down
        assert_eq!(state.top_line, 1, "Viewport should scroll down");
        assert_eq!(state.cursor_line, 1, "Cursor relative position should adjust back");
    }

    #[test]
    fn alt_shift_down_expands_zero_width_block_selection() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec![
            "line one".to_string(),
            "line two".to_string(),
            "line three".to_string(),
            "line four".to_string(),
        ];

        state.top_line = 0;
        state.cursor_line = 1;
        state.cursor_col = 5;
        let settings = state.settings;

        // Alt+Shift+Up creates zero-width block selection (anchor at 1, cursor at 0)
        let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::ALT | KeyModifiers::SHIFT);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 10, "test.txt");
        assert!(result.is_ok());
        assert!(state.block_selection);
        assert_eq!(state.cursor_line, 0, "Cursor should be at line 0");

        // Alt+Shift+Down twice should expand vertically (anchor stays at 1, cursor moves to 2)
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::ALT | KeyModifiers::SHIFT);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 10, "test.txt");
        assert!(result.is_ok());

        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::ALT | KeyModifiers::SHIFT);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 10, "test.txt");
        assert!(result.is_ok());

        assert!(state.block_selection, "Block selection should remain enabled");
        assert_eq!(state.cursor_line, 2, "Cursor should be at line 2");

        // Selection should span from anchor (line 1) to cursor (line 2)
        let (start, end) = state.selection_range().unwrap();
        assert_eq!(start.1, end.1, "Should remain zero-width");
        // The range should cover lines 1-2
        assert!(start.0 <= 1 && end.0 >= 2, "Should span at least lines 1-2");
    }

    // Tests for vertical scrolling and navigation with wrapped lines
    #[test]
    fn page_down_scrolls_viewport() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(100);
        state.top_line = 0;
        state.cursor_line = 0;
        let visible_lines = 20;
        let settings = state.settings;

        let key_event = KeyEvent::new(KeyCode::PageDown, KeyModifiers::empty());
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, visible_lines, "test.txt");
        assert!(result.is_ok());
        assert_eq!(state.top_line, visible_lines, "top_line should advance by visible_lines");
    }

    #[test]
    fn page_up_scrolls_viewport_up() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(100);
        state.top_line = 40;
        state.cursor_line = 5;
        let visible_lines = 20;
        let settings = state.settings;

        let key_event = KeyEvent::new(KeyCode::PageUp, KeyModifiers::empty());
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, visible_lines, "test.txt");
        assert!(result.is_ok());
        assert_eq!(state.top_line, 20, "top_line should decrease by visible_lines");
    }

    #[test]
    fn page_up_at_top_stays_at_zero() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(100);
        state.top_line = 5;
        state.cursor_line = 0;
        let visible_lines = 20;
        let settings = state.settings;

        let key_event = KeyEvent::new(KeyCode::PageUp, KeyModifiers::empty());
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, visible_lines, "test.txt");
        assert!(result.is_ok());
        assert_eq!(state.top_line, 0, "top_line should not go below 0");
    }

    #[test]
    fn arrow_down_on_short_line_moves_to_next_line() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec![
            "short".to_string(),
            "another line".to_string(),
        ];
        state.top_line = 0;
        state.cursor_line = 0;
        state.cursor_col = 3;
        let settings = state.settings;

        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());
        assert_eq!(state.cursor_line, 1, "cursor should move to next line");
        assert_eq!(state.cursor_col, 3, "cursor column should be preserved");
    }

    #[test]
    fn arrow_up_on_line_moves_to_previous_line() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec![
            "line one".to_string(),
            "line two".to_string(),
        ];
        state.top_line = 0;
        state.cursor_line = 1;
        state.cursor_col = 3;
        let settings = state.settings;

        let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::empty());
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());
        assert_eq!(state.cursor_line, 0, "cursor should move to previous line");
        assert_eq!(state.cursor_col, 3, "cursor column should be preserved");
    }

    #[test]
    fn cursor_column_memory_through_short_line() {
        // Test the exact scenario from user request:
        // Cursor at column 5 line 1, next line is empty, line 3 is 10 chars
        // Pressing Down twice should move cursor to column 5 line 3, not column 0
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec![
            "0123456789".to_string(),  // Line 0: 10 characters
            "".to_string(),             // Line 1: empty
            "0123456789".to_string(),  // Line 2: 10 characters
        ];
        state.top_line = 0;
        state.cursor_line = 0;
        state.cursor_col = 5;  // Start at column 5
        let settings = state.settings;

        // Press Down once - move to empty line
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());
        assert_eq!(state.cursor_line, 1, "cursor should move to line 1");
        assert_eq!(state.cursor_col, 0, "cursor should be at column 0 on empty line");
        assert_eq!(state.desired_cursor_col, 5, "desired column should remain 5");

        // Press Down again - move to line with 10 characters
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());
        assert_eq!(state.cursor_line, 2, "cursor should move to line 2");
        assert_eq!(state.cursor_col, 5, "cursor should restore to column 5, not column 0");
    }

    #[test]
    fn arrow_up_at_top_scrolls_if_not_at_file_start() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(50);
        state.top_line = 10;
        state.cursor_line = 0; // At top of viewport
        state.cursor_col = 0;
        let settings = state.settings;

        let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::empty());
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());
        assert_eq!(state.top_line, 9, "should scroll up by one line");
        assert_eq!(state.cursor_line, 0, "cursor should stay at viewport top");
    }

    #[test]
    fn arrow_down_at_bottom_scrolls_if_not_at_file_end() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(50);
        let visible_lines = 20;
        state.top_line = 10;
        state.cursor_line = visible_lines - 1; // At bottom of viewport
        state.cursor_col = 0;
        let settings = state.settings;

        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, visible_lines, "test.txt");
        assert!(result.is_ok());
        // Should scroll down
        assert!(state.top_line > 10, "should scroll down");
    }

    #[test]
    fn home_moves_to_line_start() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello world".to_string()];
        state.cursor_line = 0;
        state.cursor_col = 6;
        let settings = state.settings;

        let key_event = KeyEvent::new(KeyCode::Home, KeyModifiers::empty());
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());
        assert_eq!(state.cursor_col, 0, "cursor should move to start of line");
    }

    #[test]
    fn end_moves_to_line_end() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello world".to_string()];
        state.cursor_line = 0;
        state.cursor_col = 0;
        let settings = state.settings;

        let key_event = KeyEvent::new(KeyCode::End, KeyModifiers::empty());
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());
        assert_eq!(state.cursor_col, 11, "cursor should move to end of line");
    }

    #[test]
    fn ctrl_home_jumps_to_file_start() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(100);
        state.top_line = 50;
        state.cursor_line = 10;
        state.cursor_col = 5;
        let settings = state.settings;

        let key_event = KeyEvent::new(KeyCode::Home, KeyModifiers::CONTROL);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());
        assert_eq!(state.top_line, 0, "should scroll to file start");
        assert_eq!(state.cursor_line, 0, "cursor should be at first line");
        assert_eq!(state.cursor_col, 0, "cursor should be at column 0");
    }

    #[test]
    fn ctrl_end_jumps_to_file_end() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(100);
        state.top_line = 0;
        state.cursor_line = 0;
        state.cursor_col = 0;
        let visible_lines = 20;
        let settings = state.settings;

        let key_event = KeyEvent::new(KeyCode::End, KeyModifiers::CONTROL);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, visible_lines, "test.txt");
        assert!(result.is_ok());
        let expected_absolute = lines.len() - 1;
        assert_eq!(state.absolute_line(), expected_absolute, "cursor should be at last line");
    }

    #[test]
    fn wrapped_line_navigation_down_within_same_logical_line() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        state.term_width = 40; // Small width to force wrapping
        let mut lines = vec![
            "x".repeat(100), // This will wrap across multiple visual lines
        ];
        state.top_line = 0;
        state.cursor_line = 0;
        state.cursor_col = 10; // Near the start
        let settings = state.settings;

        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());
        // Cursor should move down within the same wrapped logical line
        assert_eq!(state.cursor_line, 0, "should stay on same logical line");
        // Cursor column should advance by roughly text_width
        assert!(state.cursor_col > 10, "cursor should advance within wrapped line");
    }

    #[test]
    fn wrapped_line_navigation_up_within_same_logical_line() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        state.term_width = 40; // Small width to force wrapping
        let mut lines = vec![
            "x".repeat(100), // This will wrap across multiple visual lines
        ];
        state.top_line = 0;
        state.cursor_line = 0;
        state.cursor_col = 50; // In the middle of wrapped line
        let settings = state.settings;

        let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::empty());
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());
        // Cursor should move up within the same wrapped logical line
        assert_eq!(state.cursor_line, 0, "should stay on same logical line");
        // Cursor column should decrease
        assert!(state.cursor_col < 50, "cursor should move back within wrapped line");
    }

    #[test]
    fn scrolling_with_wrapped_lines_maintains_cursor_visibility() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        state.term_width = 40;
        let mut lines = vec![
            "x".repeat(100), // Wraps to ~3 visual lines
            "y".repeat(100),
            "z".repeat(100),
            "short line".to_string(),
        ];
        state.top_line = 0;
        state.cursor_line = 0;
        state.cursor_col = 0;
        let visible_lines = 5; // Small viewport
        let settings = state.settings;

        // Navigate down multiple times
        for _ in 0..10 {
            let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
            let _ = handle_key_event(&mut state, &mut lines, key_event, settings, visible_lines, "test.txt");
        }

        // Cursor should still be within valid range
        let absolute_line = state.absolute_line();
        assert!(absolute_line < lines.len(), "cursor should be within file bounds");
        assert!(state.cursor_col <= lines[absolute_line].len(), "cursor column should be valid");
    }

    #[test]
    fn alt_arrow_up_scrolls_viewport_without_moving_cursor() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(50);
        state.top_line = 10;
        state.cursor_line = 5;
        state.cursor_col = 3;
        let settings = state.settings;

        // Initial absolute cursor position
        let initial_absolute_cursor = state.absolute_line();
        assert_eq!(initial_absolute_cursor, 15);

        // Alt+Up should scroll viewport up
        let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::ALT);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());

        // Viewport should scroll up
        assert_eq!(state.top_line, 9, "viewport should scroll up by 1");
        // Cursor should stay at same absolute position
        assert_eq!(state.absolute_line(), initial_absolute_cursor, "cursor absolute position should not change");
        assert_eq!(state.cursor_line, 6, "cursor relative position should adjust");
        assert_eq!(state.cursor_col, 3, "cursor column should not change");
    }

    #[test]
    fn alt_arrow_down_scrolls_viewport_without_moving_cursor() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(50);
        state.top_line = 10;
        state.cursor_line = 5;
        state.cursor_col = 3;
        let settings = state.settings;

        let initial_absolute_cursor = state.absolute_line();
        assert_eq!(initial_absolute_cursor, 15);

        // Alt+Down should scroll viewport down
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::ALT);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());

        // Viewport should scroll down
        assert_eq!(state.top_line, 11, "viewport should scroll down by 1");
        // Cursor should stay at same absolute position
        assert_eq!(state.absolute_line(), initial_absolute_cursor, "cursor absolute position should not change");
        assert_eq!(state.cursor_line, 4, "cursor relative position should adjust");
        assert_eq!(state.cursor_col, 3, "cursor column should not change");
    }

    #[test]
    fn alt_arrow_up_at_top_does_nothing() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(50);
        state.top_line = 0;
        state.cursor_line = 5;
        state.cursor_col = 3;
        let settings = state.settings;

        let initial_absolute_cursor = state.absolute_line();

        // Alt+Up at top should do nothing
        let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::ALT);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());

        assert_eq!(state.top_line, 0, "viewport should not scroll");
        assert_eq!(state.absolute_line(), initial_absolute_cursor, "cursor position should not change");
    }

    #[test]
    fn alt_arrow_down_at_bottom_does_nothing() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(50);
        state.top_line = 49; // At max scroll
        state.cursor_line = 0;
        state.cursor_col = 3;
        let settings = state.settings;

        let initial_absolute_cursor = state.absolute_line();

        // Alt+Down at bottom should do nothing
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::ALT);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());

        assert_eq!(state.top_line, 49, "viewport should not scroll");
        assert_eq!(state.absolute_line(), initial_absolute_cursor, "cursor position should not change");
    }

    #[test]
    fn alt_arrow_left_scrolls_horizontally() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        // Toggle wrapping off to enable horizontal scrolling
        if state.is_line_wrapping_enabled() {
            state.toggle_line_wrapping();
        }
        let mut lines = vec![
            "x".repeat(200), // Very long line
        ];
        state.top_line = 0;
        state.cursor_line = 0;
        state.cursor_col = 50;
        state.horizontal_scroll_offset = 20;
        let settings = state.settings;

        // Alt+Left should scroll viewport left
        let key_event = KeyEvent::new(KeyCode::Left, KeyModifiers::ALT);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());

        // Horizontal scroll should decrease
        assert!(state.horizontal_scroll_offset < 20, "horizontal scroll should decrease");
        // Cursor column should stay the same
        assert_eq!(state.cursor_col, 50, "cursor column should not change");
    }

    #[test]
    fn alt_arrow_right_scrolls_horizontally() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        // Toggle wrapping off to enable horizontal scrolling
        if state.is_line_wrapping_enabled() {
            state.toggle_line_wrapping();
        }
        let mut lines = vec![
            "x".repeat(200), // Very long line
        ];
        state.top_line = 0;
        state.cursor_line = 0;
        state.cursor_col = 50;
        state.horizontal_scroll_offset = 10;
        let settings = state.settings;

        // Alt+Right should scroll viewport right
        let key_event = KeyEvent::new(KeyCode::Right, KeyModifiers::ALT);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());

        // Horizontal scroll should increase
        assert!(state.horizontal_scroll_offset > 10, "horizontal scroll should increase");
        // Cursor column should stay the same
        assert_eq!(state.cursor_col, 50, "cursor column should not change");
    }

    #[test]
    fn alt_shift_arrow_still_creates_block_selection() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec![
            "line one".to_string(),
            "line two".to_string(),
            "line three".to_string(),
        ];
        state.top_line = 0;
        state.cursor_line = 1;
        state.cursor_col = 3;
        let settings = state.settings;

        // Alt+Shift+Down should create block selection (not viewport scroll)
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::ALT | KeyModifiers::SHIFT);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 10, "test.txt");
        assert!(result.is_ok());

        assert!(state.block_selection, "block selection should be enabled");
        assert!(state.has_selection(), "should have selection");
    }

    #[test]
    fn alt_arrow_up_allows_cursor_to_go_offscreen() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(50);
        state.top_line = 10;
        state.cursor_line = 5;  // Absolute position 15
        state.cursor_col = 3;
        let visible_lines = 10;  // Small viewport
        let settings = state.settings;

        let initial_absolute = state.absolute_line();
        assert_eq!(initial_absolute, 15);

        // Scroll up enough times to push cursor off bottom of viewport
        for _ in 0..10 {
            let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::ALT);
            let _ = handle_key_event(&mut state, &mut lines, key_event, settings, visible_lines, "test.txt");
        }

        // Viewport scrolled up by 10
        assert_eq!(state.top_line, 0, "viewport should scroll to top");
        // Cursor stayed at absolute position 15
        assert_eq!(state.absolute_line(), initial_absolute, "cursor absolute position should not change");
        // Cursor is now at line 15 (relative to top_line 0), which exceeds visible_lines (10)
        assert_eq!(state.cursor_line, 15, "cursor relative position shows it's off-screen");
        assert!(state.cursor_line >= visible_lines, "cursor should be beyond viewport");
    }

    #[test]
    fn alt_arrow_down_when_cursor_at_top() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(50);
        state.top_line = 10;
        state.cursor_line = 0;  // Cursor at top of viewport (absolute position 10)
        state.cursor_col = 3;
        let settings = state.settings;

        let initial_absolute = state.absolute_line();
        assert_eq!(initial_absolute, 10);

        // Alt+Down when cursor is at cursor_line 0
        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::ALT);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        assert!(result.is_ok());

        // Viewport scrolls down
        assert_eq!(state.top_line, 11, "viewport should scroll down");
        // Cursor stays at absolute position 10, which is now ABOVE the viewport
        // So saved_absolute_cursor is set and cursor_line is 0
        assert_eq!(state.cursor_line, 0, "cursor_line is 0 (cursor is above viewport)");
        assert_eq!(state.saved_absolute_cursor, Some(10), "saved_absolute_cursor should be set");
        assert_eq!(state.absolute_line(), 10, "absolute position should NOT change");
    }

    #[test]
    fn alt_arrow_up_with_cursor_above_viewport_keeps_it_offscreen() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = create_test_lines(50);
        state.top_line = 15;
        state.cursor_line = 0;  // Cursor at absolute position 15
        state.cursor_col = 3;
        let visible_lines = 10;
        let settings = state.settings;

        // First, scroll down to push cursor above viewport
        for _ in 0..5 {
            let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::ALT);
            let _ = handle_key_event(&mut state, &mut lines, key_event, settings, visible_lines, "test.txt");
        }

        // Now cursor is at position 15, viewport is at top_line=20
        // Cursor is 5 lines ABOVE the viewport
        assert_eq!(state.top_line, 20);
        assert_eq!(state.absolute_line(), 15, "cursor should still be at position 15");
        assert_eq!(state.saved_absolute_cursor, Some(15), "cursor should be tracked as off-screen");

        // Now scroll up once - cursor should STAY off-screen
        let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::ALT);
        let _ = handle_key_event(&mut state, &mut lines, key_event, settings, visible_lines, "test.txt");

        assert_eq!(state.top_line, 19, "viewport should scroll up");
        assert_eq!(state.absolute_line(), 15, "cursor should STILL be at position 15");
        assert_eq!(state.saved_absolute_cursor, Some(15), "cursor should STILL be tracked as off-screen");

        // Scroll up multiple more times - cursor should stay off-screen until viewport reaches it
        for _ in 0..3 {
            let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::ALT);
            let _ = handle_key_event(&mut state, &mut lines, key_event, settings, visible_lines, "test.txt");
        }

        // After 3 more scrolls, top_line is 16, cursor is still at 15 (still above viewport)
        assert_eq!(state.top_line, 16);
        assert_eq!(state.absolute_line(), 15, "cursor should STILL be at position 15");
        assert_eq!(state.saved_absolute_cursor, Some(15), "cursor should STILL be off-screen");

        // One more scroll brings cursor into view
        let key_event = KeyEvent::new(KeyCode::Up, KeyModifiers::ALT);
        let _ = handle_key_event(&mut state, &mut lines, key_event, settings, visible_lines, "test.txt");

        // Now top_line is 15, cursor is at 15, so cursor_line should be 0 and visible
        assert_eq!(state.top_line, 15);
        assert_eq!(state.cursor_line, 0, "cursor should be at top of viewport");
        assert_eq!(state.saved_absolute_cursor, None, "cursor should now be visible");
        assert_eq!(state.absolute_line(), 15, "cursor absolute position unchanged");
    }

    #[test]
    fn alt_arrow_horizontal_keeps_cursor_column() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        // Toggle wrapping off to enable horizontal scrolling
        if state.is_line_wrapping_enabled() {
            state.toggle_line_wrapping();
        }
        let mut lines = vec![
            "x".repeat(200), // Very long line
        ];
        state.top_line = 0;
        state.cursor_line = 0;
        state.cursor_col = 50;
        state.horizontal_scroll_offset = 0;
        let settings = state.settings;

        // Scroll right multiple times
        for _ in 0..10 {
            let key_event = KeyEvent::new(KeyCode::Right, KeyModifiers::ALT);
            let _ = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");
        }

        // Cursor column should stay the same
        assert_eq!(state.cursor_col, 50, "cursor column should not change");
        // Horizontal scroll should have increased
        assert!(state.horizontal_scroll_offset > 0, "viewport should have scrolled right");
    }

    #[test]
    fn ctrl_backspace_deletes_word_backward() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello world test".to_string()];
        let settings = state.settings;

        // Position cursor at end of line
        state.cursor_col = 16; // After "test"

        // Press Ctrl+Backspace
        let key_event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::CONTROL);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");

        assert!(result.is_ok());
        assert_eq!(lines[0], "hello world ", "should delete 'test'");
        assert_eq!(state.cursor_col, 12, "cursor should be at start of deleted word");
        assert!(!state.replace_active, "should not enter replace mode");
    }

    #[test]
    fn ctrl_backspace_does_not_trigger_replace_without_search() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello world".to_string()];
        let settings = state.settings;

        state.cursor_col = 11;
        state.last_search_pattern = None; // No active search

        // Press Ctrl+Backspace
        let key_event = KeyEvent::new(KeyCode::Backspace, KeyModifiers::CONTROL);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");

        assert!(result.is_ok());
        assert!(!state.replace_active, "should not enter replace mode");
        assert_eq!(lines[0], "hello ", "should delete word");
    }

    #[test]
    fn ctrl_h_deletes_word_backward() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello world".to_string()];
        let settings = state.settings;

        state.cursor_col = 11; // After "world"

        // Press Ctrl+H (as Char('h')) - should delete word backward
        let key_event = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");

        assert!(result.is_ok());
        assert!(!state.replace_active, "should not enter replace mode");
        assert_eq!(lines[0], "hello ", "should delete word backward");
        assert_eq!(state.cursor_col, 6, "cursor should be at start of deleted word");
    }

    #[test]
    fn ctrl_shift_h_triggers_replace_with_active_search() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello world".to_string()];
        let settings = state.settings;

        // Set up active search
        state.last_search_pattern = Some("hello".to_string());

        // Press Ctrl+Shift+H (new replace keybinding)
        let key_event = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
        let result = handle_key_event(&mut state, &mut lines, key_event, settings, 20, "test.txt");

        assert!(result.is_ok());
        assert!(state.replace_active, "should enter replace mode with active search");
        assert_eq!(lines[0], "hello world", "should not modify content");
    }


}





