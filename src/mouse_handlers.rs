use crossterm::event::{MouseEvent, MouseEventKind, MouseButton, KeyModifiers};
use crate::coordinates::visual_to_logical_position;
use crate::editor_state::FileViewerState;
/// Main entry point for all mouse event handling
pub(crate) fn handle_mouse_event(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    mouse_event: MouseEvent,
    visible_lines: usize,
) {
    let MouseEvent { kind, column, row, modifiers, .. } = mouse_event;
    // Ignore clicks on header row
    if row == 0 {
        return;
    }
    let visual_line = (row as usize).saturating_sub(1);
    // Ignore clicks beyond visible content
    if visual_line >= visible_lines {
        return;
    }
    match kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let pos_opt = visual_to_logical_position(state, lines, visual_line, column);
            if let Some((logical_line, col)) = pos_opt {
                let clicked = (logical_line, col.min(lines[logical_line].len()));
                if state.is_point_in_selection(clicked) {
                    // Start drag operation
                    state.start_drag();
                } else {
                    // Normal cursor move
                    handle_mouse_click(state, lines, visual_line, column);
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if state.dragging_selection_active {
                if let Some((logical_line, col)) = visual_to_logical_position(state, lines, visual_line, column) {
                    state.drag_target = Some((logical_line, col.min(lines[logical_line].len())));
                    state.needs_redraw = true; // could render a placeholder caret
                }
            } else {
                handle_mouse_drag(state, lines, visual_line, column);
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if state.dragging_selection_active {
                finalize_drag(state, lines, modifiers.contains(KeyModifiers::CONTROL));
            }
            state.mouse_dragging = false;
        }
        MouseEventKind::ScrollDown => {
            let scroll_amount = state.settings.mouse_scroll_lines;
            handle_mouse_scroll_down(state, lines, visible_lines, scroll_amount);
        }
        MouseEventKind::ScrollUp => {
            let scroll_amount = state.settings.mouse_scroll_lines;
            handle_mouse_scroll_up(state, lines, visible_lines, scroll_amount);
        }
        _ => {}
    }
}
/// Handle mouse click to position cursor
fn handle_mouse_click(
    state: &mut FileViewerState,
    lines: &[String],
    visual_line: usize,
    column: u16,
) {
    if let Some((logical_line, col)) = visual_to_logical_position(state, lines, visual_line, column)
        && logical_line < lines.len() {
        restore_cursor_to_screen(state);
        state.cursor_line = logical_line.saturating_sub(state.top_line);
        state.cursor_col = col.min(lines[logical_line].len());
        state.clear_selection();
        state.mouse_dragging = true;
        state.needs_redraw = true;
    }
}
/// Handle mouse drag for text selection
fn handle_mouse_drag(
    state: &mut FileViewerState,
    lines: &[String],
    visual_line: usize,
    column: u16,
) {
    if !state.mouse_dragging {
        return;
    }
    // Initialize selection on first drag
    if state.selection_start.is_none() {
        state.selection_start = Some(state.current_position());
    }
    if let Some((logical_line, col)) = visual_to_logical_position(state, lines, visual_line, column)
        && logical_line < lines.len() {
        restore_cursor_to_screen(state);
        state.cursor_line = logical_line.saturating_sub(state.top_line);
        state.cursor_col = col;
        state.update_selection();
        state.needs_redraw = true;
    }
}
/// Handle mouse scroll down event
fn handle_mouse_scroll_down(
    state: &mut FileViewerState,
    lines: &[String],
    visible_lines: usize,
    scroll_amount: usize,
) {
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
fn save_cursor_state_if_needed(state: &mut FileViewerState, old_top: usize, old_cursor_line: usize) {
    if state.saved_scroll_state.is_none() {
        state.saved_scroll_state = Some((old_top, old_cursor_line));
    }
}
/// Restore cursor to on-screen state
fn restore_cursor_to_screen(state: &mut FileViewerState) {
    state.saved_absolute_cursor = None;
    state.saved_scroll_state = None;
}
/// Finalize a drag operation: move or copy selected text to drag_target
fn finalize_drag(state: &mut FileViewerState, lines: &mut Vec<String>, copy: bool) {
    use crate::editing::apply_drag;
    if let (Some(start), Some(end), Some(dest)) = (state.drag_source_start, state.drag_source_end, state.drag_target) {
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
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
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
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
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
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
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
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
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
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
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
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
        let mut state = create_test_state(settings);
        let mut lines = vec!["line 1".to_string(), "line 2".to_string(), "line 3".to_string()];
        state.top_line = 2;
        
        let mouse_event = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::empty(),
        };
        
        handle_mouse_event(&mut state, &mut lines, mouse_event, 20);
        assert_eq!(state.top_line, 2); // Can't scroll past last line
    }

    #[test]
    fn restore_cursor_to_screen_clears_saved_state() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
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
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
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
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
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
}

