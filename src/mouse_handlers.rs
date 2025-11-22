use crossterm::event::{MouseEvent, MouseEventKind, MouseButton};
use crate::coordinates::visual_to_logical_position;
use crate::editor_state::FileViewerState;
/// Main entry point for all mouse event handling
pub(crate) fn handle_mouse_event(
    state: &mut FileViewerState,
    lines: &[String],
    mouse_event: MouseEvent,
    visible_lines: usize,
) {
    let MouseEvent { kind, column, row, .. } = mouse_event;
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
            handle_mouse_click(state, lines, visual_line, column);
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            handle_mouse_drag(state, lines, visual_line, column);
        }
        MouseEventKind::Up(MouseButton::Left) => {
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
    if let Some((logical_line, col)) = visual_to_logical_position(state, lines, visual_line, column) {
        if logical_line < lines.len() {
            restore_cursor_to_screen(state);
            state.cursor_line = logical_line.saturating_sub(state.top_line);
            state.cursor_col = col.min(lines[logical_line].len());
            state.clear_selection();
            state.mouse_dragging = true;
            state.needs_redraw = true;
        }
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
    if let Some((logical_line, col)) = visual_to_logical_position(state, lines, visual_line, column) {
        if logical_line < lines.len() {
            restore_cursor_to_screen(state);
            state.cursor_line = logical_line.saturating_sub(state.top_line);
            state.cursor_col = col.min(lines[logical_line].len());
            state.selection_end = Some(state.current_position());
            state.needs_redraw = true;
        }
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
