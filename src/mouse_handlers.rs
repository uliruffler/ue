use crossterm::event::{MouseEvent, MouseEventKind, MouseButton, KeyModifiers};
use crate::coordinates::visual_to_logical_position;
use crate::editor_state::FileViewerState;

/// Handle mouse click on scrollbar
fn handle_scrollbar_click(
    state: &mut FileViewerState,
    lines: &[String],
    visual_line: usize,
    row: u16,
    visible_lines: usize,
) {
    if lines.len() <= visible_lines {
        return; // No scrolling needed
    }
    
    // Calculate scrollbar dimensions (same as in rendering.rs)
    let total_lines = lines.len();
    let scrollbar_height = visible_lines;
    let bar_height = (visible_lines * visible_lines / total_lines).max(1);
    
    let max_scroll = total_lines.saturating_sub(visible_lines);
    let scroll_progress = if max_scroll == 0 {
        0.0
    } else {
        state.top_line as f64 / max_scroll as f64
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

/// Handle scrollbar dragging
fn handle_scrollbar_drag(
    state: &mut FileViewerState,
    lines: &[String],
    row: u16,
    visible_lines: usize,
) {
    if !state.scrollbar_dragging || lines.len() <= visible_lines {
        return;
    }
    
    let total_lines = lines.len();
    let max_scroll = total_lines.saturating_sub(visible_lines);
    let scrollbar_height = visible_lines;
    let bar_height = (visible_lines * visible_lines / total_lines).max(1);
    
    // Convert mouse row to visual line (accounting for header)
    let mouse_visual_line = (row as usize).saturating_sub(1);
    
    // For very small bars (1 character), position the bar directly at mouse position
    if bar_height == 1 {
        // For 1-character bars, calculate scroll position so that the bar renders at mouse position
        // We need to work backwards: if bar should be at mouse_visual_line, what scroll position gives us that?
        let available_scroll_space = visible_lines.saturating_sub(bar_height);
        let target_bar_position = mouse_visual_line.min(available_scroll_space);
        
        // Calculate the top_line that will render the bar at target_bar_position
        // Rendering: bar_position = ((scrollbar_height - bar_height) * (top_line / max_scroll)) as usize
        // We want: target_bar_position <= (available_scroll_space * (top_line / max_scroll)) < target_bar_position + 1
        // So: top_line >= (target_bar_position * max_scroll) / available_scroll_space
        // And: top_line < ((target_bar_position + 1) * max_scroll) / available_scroll_space
        // To ensure we get exactly target_bar_position when rounded down, we use the lower bound
        let new_top_line = if available_scroll_space > 0 && max_scroll > 0 {
            // Use the minimum top_line that will render to target_bar_position
            let min_top_line = (target_bar_position * max_scroll).div_ceil(available_scroll_space);
            min_top_line.min(max_scroll)
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
        
        // Convert scroll progress to actual top_line
        let new_top_line = (scroll_progress * max_scroll as f64) as usize;
        let new_top_line = new_top_line.min(max_scroll);
        
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
    // Ignore clicks beyond visible content, but allow scrollbar events to reach the boundary
    let scrollbar_column = state.term_width - 1;
    let scrollbar_visible = lines.len() > visible_lines;
    let is_scrollbar_event = scrollbar_visible && column == scrollbar_column;
    
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
                    let pos_opt = visual_to_logical_position(state, lines, visual_line, column, visible_lines);
                    if let Some((logical_line, col)) = pos_opt {
                        let clicked = (logical_line, col.min(lines[logical_line].len()));
                        if state.is_point_in_selection(clicked) {
                            // Start drag operation
                            state.start_drag();
                        } else {
                            // Normal cursor move
                            handle_mouse_click(state, lines, visual_line, column, visible_lines);
                        }
                    }
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if state.scrollbar_dragging {
                // Handle scrollbar dragging
                handle_scrollbar_drag(state, lines, row, visible_lines);
            } else if state.dragging_selection_active {
                if let Some((logical_line, col)) = visual_to_logical_position(state, lines, visual_line, column, visible_lines) {
                    state.drag_target = Some((logical_line, col.min(lines[logical_line].len())));
                    state.needs_redraw = true; // could render a placeholder caret
                }
            } else {
                // Check if dragging on line number area
                let line_num_width = crate::coordinates::line_number_width(state.settings);
                if column < line_num_width {
                    // Dragging on line number - extend line selection
                    handle_line_number_drag(state, lines, visual_line, visible_lines);
                } else {
                    handle_mouse_drag(state, lines, visual_line, column, visible_lines);
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if state.scrollbar_dragging {
                state.scrollbar_dragging = false;
                state.needs_redraw = true;
            } else if state.dragging_selection_active {
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
    visible_lines: usize,
) {
    if let Some((logical_line, col)) = visual_to_logical_position(state, lines, visual_line, column, visible_lines)
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
    visible_lines: usize,
) {
    if !state.mouse_dragging {
        return;
    }
    // Initialize selection on first drag
    if state.selection_start.is_none() {
        state.selection_start = Some(state.current_position());
    }
    if let Some((logical_line, col)) = visual_to_logical_position(state, lines, visual_line, column, visible_lines)
        && logical_line < lines.len() {
        restore_cursor_to_screen(state);
        state.cursor_line = logical_line.saturating_sub(state.top_line);
        state.cursor_col = col;
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
    if let Some(logical_line) = visual_line_to_logical_line(state, lines, visual_line, visible_lines)
        && logical_line < lines.len() {
        restore_cursor_to_screen(state);
        
        // Select the entire line
        // Start of line: (logical_line, 0)
        // End of line: (logical_line, line_length) or start of next line
        state.selection_start = Some((logical_line, 0));
        
        // Position cursor at end of line
        state.cursor_line = logical_line.saturating_sub(state.top_line);
        state.cursor_col = lines[logical_line].len();
        
        // Set selection end to include the entire line
        // If there's a next line, go to start of it; otherwise end of current line
        if logical_line + 1 < lines.len() {
            state.selection_end = Some((logical_line + 1, 0));
        } else {
            state.selection_end = Some((logical_line, lines[logical_line].len()));
        }
        
        state.mouse_dragging = true;
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
    if let Some(logical_line) = visual_line_to_logical_line(state, lines, visual_line, visible_lines)
        && logical_line < lines.len() {
        restore_cursor_to_screen(state);
        
        // Extend selection to include entire lines
        if let Some(start) = state.selection_start {
            let start_line = start.0;
            
            if logical_line >= start_line {
                // Dragging downward - select from start of start_line to end of current line
                state.selection_start = Some((start_line, 0));
                
                // Position cursor at end of dragged line
                state.cursor_line = logical_line.saturating_sub(state.top_line);
                state.cursor_col = lines[logical_line].len();
                
                // Extend to start of next line or end of current line
                if logical_line + 1 < lines.len() {
                    state.selection_end = Some((logical_line + 1, 0));
                } else {
                    state.selection_end = Some((logical_line, lines[logical_line].len()));
                }
            } else {
                // Dragging upward - select from start of current line to end of start_line
                state.selection_start = Some((logical_line, 0));
                
                // Position cursor at start of dragged line
                state.cursor_line = logical_line.saturating_sub(state.top_line);
                state.cursor_col = 0;
                
                // Extend to start of line after start_line or end of start_line
                if start_line + 1 < lines.len() {
                    state.selection_end = Some((start_line + 1, 0));
                } else {
                    state.selection_end = Some((start_line, lines[start_line].len()));
                }
            }
            
            state.needs_redraw = true;
        }
    }
}
/// Convert visual line to logical line index
fn visual_line_to_logical_line(
    state: &FileViewerState,
    lines: &[String],
    visual_line: usize,
    visible_lines: usize,
) -> Option<usize> {
    use crate::coordinates::{calculate_wrapped_lines_for_line};
    let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines);
    let tab_width = state.settings.tab_width;
    
    let mut current_visual_line = 0;
    let mut logical_line = state.top_line;
    
    while logical_line < lines.len() {
        let wrapped_lines = calculate_wrapped_lines_for_line(lines, logical_line, text_width, tab_width) as usize;
        
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
        // Can scroll to show last line at top (max_scroll = lines.len() - 1 = 2)
        assert_eq!(state.top_line, 2);
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

    #[test]
    fn mouse_click_on_line_number_selects_entire_line() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
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
            row: 2, // Second line (row 0 is header, row 1 is first line)
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
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
        let mut state = create_test_state(settings);
        let mut lines = vec![
            "first line".to_string(),
            "second line".to_string(),
            "third line".to_string(),
            "fourth line".to_string(),
        ];
        
        // First click on line 1
        state.selection_start = Some((1, 0));
        state.selection_end = Some((2, 0));
        state.mouse_dragging = true;
        
        // Drag to line 3
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: 0, // In line number area
            row: 4, // Fourth line (row 0 is header)
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
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
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
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
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
            row: 1, // First visible line
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
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
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
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
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
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
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
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
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
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
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
        assert_eq!(absolute_cursor_after, absolute_cursor_before,
            "Cursor absolute position should remain at line {} but is now at line {}",
            absolute_cursor_before, absolute_cursor_after);
    }

    #[test]
    fn scrollbar_not_visible_when_few_lines() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
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
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
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
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
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
            assert_eq!(new_top_line, expected_top_line,
                "Mouse row {} (visual line {}) -> bar position {} -> expected top_line {}, got {}",
                mouse_row, mouse_visual_line, target_bar_position, expected_top_line, new_top_line);
            
            // Verify the scroll position is monotonically increasing (or stays same at end)
            if let Some(prev) = previous_top_line {
                assert!(new_top_line >= prev, 
                    "Scroll position should increase: previous {}, current {}", prev, new_top_line);
            }
            
            // Calculate where the bar should be rendered at this scroll position
            let actual_scroll_progress = if max_scroll > 0 {
                new_top_line as f64 / max_scroll as f64
            } else {
                0.0
            };
            let rendered_bar_position = (available_scroll_space as f64 * actual_scroll_progress) as usize;
            
            // The rendered bar position should match our target (within rounding)
            let position_diff = (rendered_bar_position as i32 - target_bar_position as i32).abs();
            assert!(position_diff <= 1,
                "Bar position mismatch: target {}, rendered {} (diff {})",
                target_bar_position, rendered_bar_position, position_diff);
            
            println!("✓ Mouse row {} -> visual line {} -> bar pos {} -> top_line {} -> rendered at {}",
                mouse_row, mouse_visual_line, target_bar_position, new_top_line, rendered_bar_position);
            
            previous_top_line = Some(new_top_line);
        }
        
        // Verify we reached the bottom
        let final_scroll_progress = state.top_line as f64 / max_scroll as f64;
        assert!(final_scroll_progress >= 0.9, 
            "Should reach near bottom, got scroll progress {:.3}", final_scroll_progress);
        
        // Verify all intermediate positions were unique (no skipping)
        // Test a few key positions to ensure smooth progression
        let test_positions = [
            (1, 0.0),   // Top
            (4, 0.3),   // ~30%
            (7, 0.6),   // ~60%
            (10, 0.9),  // ~90%
            (11, 1.0),  // Bottom (clamped)
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
            
            assert!(progress_diff < 0.15, // Allow 15% tolerance for rounding
                "Position {} should give ~{:.1}% progress, got {:.3} (diff {:.3})",
                row, expected_progress * 100.0, actual_progress, progress_diff);
        }
        
        println!("✓ All scrollbar positions reached successfully!");
    }

    // ...existing tests...
}
