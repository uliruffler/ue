use crate::settings::Settings;
use crate::undo::UndoHistory;

/// Type alias for cursor/selection position (line, column)
pub(crate) type Position = (usize, usize);

pub(crate) struct FileViewerState<'a> {
    pub(crate) top_line: usize,
    pub(crate) cursor_line: usize,
    pub(crate) cursor_col: usize,
    pub(crate) selection_start: Option<Position>,
    pub(crate) selection_end: Option<Position>,
    pub(crate) needs_redraw: bool,
    pub(crate) modified: bool,
    pub(crate) term_width: u16,
    pub(crate) undo_history: UndoHistory,
    pub(crate) settings: &'a Settings,
    pub(crate) mouse_dragging: bool,
    /// Saved absolute cursor line when cursor is scrolled off-screen
    /// None means cursor is on-screen, Some means cursor is off-screen at that absolute line
    pub(crate) saved_absolute_cursor: Option<usize>,
    /// Saved scroll state (top_line, cursor_line) when cursor first goes off-screen
    /// Used to restore original viewport when navigating back
    pub(crate) saved_scroll_state: Option<(usize, usize)>,
    pub(crate) drag_source_start: Option<Position>,
    pub(crate) drag_source_end: Option<Position>,
    pub(crate) drag_text: Option<String>,
    pub(crate) dragging_selection_active: bool,
    pub(crate) drag_target: Option<Position>,
}

impl<'a> FileViewerState<'a> {
    pub(crate) fn new(term_width: u16, undo_history: UndoHistory, settings: &'a Settings) -> Self {
        Self {
            top_line: 0,
            cursor_line: 0,
            cursor_col: 0,
            selection_start: None,
            selection_end: None,
            needs_redraw: true,
            modified: false,
            term_width,
            undo_history,
            settings,
            mouse_dragging: false,
            saved_absolute_cursor: None,
            saved_scroll_state: None,
            dragging_selection_active: false,
            drag_source_start: None,
            drag_source_end: None,
            drag_text: None,
            drag_target: None,
        }
    }

    pub(crate) fn current_position(&self) -> Position {
        (self.top_line + self.cursor_line, self.cursor_col)
    }

    pub(crate) fn absolute_line(&self) -> usize {
        // If cursor is saved (off-screen), use the saved position
        // Otherwise calculate from top_line + cursor_line
        self.saved_absolute_cursor.unwrap_or(self.top_line + self.cursor_line)
    }

    pub(crate) fn has_selection(&self) -> bool {
        self.selection_start.is_some() && self.selection_end.is_some()
    }

    pub(crate) fn start_selection(&mut self) {
        if self.selection_start.is_none() {
            self.selection_start = Some(self.current_position());
        }
    }

    pub(crate) fn update_selection(&mut self) {
        self.selection_end = Some(self.current_position());
        self.needs_redraw = true;
    }

    pub(crate) fn clear_selection(&mut self) {
        if self.selection_start.is_some() || self.selection_end.is_some() {
            self.selection_start = None;
            self.selection_end = None;
            self.needs_redraw = true;
        }
    }

    pub(crate) fn adjust_cursor_col(&mut self, lines: &[&str]) {
        if let Some(line) = lines.get(self.absolute_line()) {
            if self.cursor_col > line.len() {
                self.cursor_col = line.len();
            }
        }
    }

    /// Check if cursor is currently visible within the viewport
    /// This accounts for line wrapping - wrapped lines consume multiple visual lines
    pub(crate) fn is_cursor_visible(&self, lines: &[String], visible_lines: usize, text_width: u16) -> bool {
        use crate::coordinates::calculate_visual_lines_to_cursor;
        
        // Cursor is visible if not saved (off-screen)
        if self.saved_absolute_cursor.is_some() {
            return false;
        }
        
        // Calculate how many visual lines are consumed from top_line through cursor_line
        let visual_lines_consumed = calculate_visual_lines_to_cursor(lines, self, text_width);
        
        // Cursor is visible if consumed lines don't exceed available space
        visual_lines_consumed <= visible_lines
    }

    /// Ensure cursor is visible after undo/redo operations
    /// If cursor would be off-screen, move it to the first or last visible line
    pub(crate) fn ensure_cursor_visible(&mut self, visible_lines: usize) {
        // Get the absolute cursor position (may use saved value)
        let absolute = self.absolute_line();
        
        // Clear saved cursor - we're bringing it back on screen
        self.saved_absolute_cursor = None;
        self.saved_scroll_state = None;
        
        // If cursor is below visible area, move it to last visible line
        if self.cursor_line >= visible_lines {
            self.cursor_line = visible_lines.saturating_sub(1);
            self.top_line = absolute.saturating_sub(self.cursor_line);
        }
        // If cursor is above visible area (when top_line was increased)
        // This happens when cursor_line calculation would go negative or when saved_absolute_cursor < top_line
        else if absolute < self.top_line {
            self.cursor_line = 0;
            self.top_line = absolute;
        }
    }

    pub(crate) fn selection_range(&self) -> Option<(Position, Position)> {
        if let (Some(s), Some(e)) = (self.selection_start, self.selection_end) {
            let (start, end) = if s.0 < e.0 || (s.0 == e.0 && s.1 <= e.1) { (s, e) } else { (e, s) };
            Some((start, end))
        } else { None }
    }

    pub(crate) fn is_point_in_selection(&self, pos: Position) -> bool {
        if let Some((start, end)) = self.selection_range() {
            let (l,c) = pos; let (sl, sc) = start; let (el, ec) = end;
            if l < sl || l > el { return false; }
            if sl == el { return c >= sc && c < ec; }
            if l == sl { return c >= sc; }
            if l == el { return c < ec; }
            true
        } else { false }
    }

    pub(crate) fn start_drag(&mut self) {
        if let Some((start, end)) = self.selection_range() {
            self.drag_source_start = Some(start);
            self.drag_source_end = Some(end);
            self.dragging_selection_active = true;
            self.drag_target = None;
        }
    }

    pub(crate) fn clear_drag(&mut self) {
        self.drag_source_start = None;
        self.drag_source_end = None;
        self.drag_text = None;
        self.dragging_selection_active = false;
        self.drag_target = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::set_temp_home;
    
    #[test]
    fn cursor_visible_when_on_screen() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
        let undo_history = UndoHistory::new();
        let state = FileViewerState::new(80, undo_history, settings);
        let lines: Vec<String> = vec!["test".to_string(); 20];
        
        assert!(state.is_cursor_visible(&lines, 10, 80));
        assert_eq!(state.absolute_line(), 0);
    }
    
    #[test]
    fn cursor_invisible_when_saved_off_screen() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
        let undo_history = UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);
        let lines: Vec<String> = vec!["test".to_string(); 20];
        
        state.top_line = 5;
        state.cursor_line = 3;
        state.saved_absolute_cursor = Some(3); // Cursor at line 3, but saved as off-screen
        
        assert!(!state.is_cursor_visible(&lines, 10, 80));
        assert_eq!(state.absolute_line(), 3); // Should return saved position
    }
    
    #[test]
    fn cursor_invisible_when_below_visible_area() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
        let undo_history = UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);
        let lines: Vec<String> = vec!["test".to_string(); 30];
        
        state.top_line = 5;
        state.cursor_line = 15; // Beyond visible area
        
        assert!(!state.is_cursor_visible(&lines, 10, 80));
    }
    
    #[test]
    fn absolute_line_uses_saved_position() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
        let undo_history = UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);
        
        state.top_line = 20;
        state.cursor_line = 0;
        state.saved_absolute_cursor = Some(10);
        
        // Should use saved position, not top_line + cursor_line
        assert_eq!(state.absolute_line(), 10);
    }
    
    #[test]
    fn absolute_line_calculates_normally_when_not_saved() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
        let undo_history = UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);
        
        state.top_line = 20;
        state.cursor_line = 5;
        state.saved_absolute_cursor = None;
        
        assert_eq!(state.absolute_line(), 25);
    }
    
    #[test]
    fn ensure_cursor_visible_brings_below_cursor_to_bottom() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
        let undo_history = UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);
        
        state.top_line = 10;
        state.cursor_line = 15; // Beyond visible area of 10 lines
        
        state.ensure_cursor_visible(10);
        
        // Cursor should be at bottom of visible area
        assert_eq!(state.cursor_line, 9);
        assert_eq!(state.top_line, 16); // 25 - 9
        assert!(state.saved_absolute_cursor.is_none());
    }
    
    #[test]
    fn ensure_cursor_visible_brings_above_cursor_to_top() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
        let undo_history = UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);
        
        state.top_line = 20;
        state.cursor_line = 0;
        state.saved_absolute_cursor = Some(15); // Cursor at line 15, but top_line is 20
        
        state.ensure_cursor_visible(10);
        
        // Cursor should be at top of visible area
        assert_eq!(state.cursor_line, 0);
        assert_eq!(state.top_line, 15);
        assert!(state.saved_absolute_cursor.is_none());
    }
    
    #[test]
    fn ensure_cursor_visible_clears_saved_position() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
        let undo_history = UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);
        
        state.top_line = 5;
        state.cursor_line = 3;
        state.saved_absolute_cursor = Some(8);
        
        state.ensure_cursor_visible(10);
        
        assert!(state.saved_absolute_cursor.is_none());
    }
}
