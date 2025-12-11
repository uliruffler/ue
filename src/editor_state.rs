use crate::settings::Settings;
use crate::undo::UndoHistory;
use std::time::Instant;

/// Type alias for cursor/selection position (line, column)
pub(crate) type Position = (usize, usize);

pub(crate) struct FileViewerState<'a> {
    pub(crate) top_line: usize,
    pub(crate) cursor_line: usize,
    pub(crate) cursor_col: usize,
    pub(crate) selection_start: Option<Position>,
    pub(crate) selection_end: Option<Position>,
    /// Anchor point for selection - the fixed point when extending selection with Shift+arrows
    pub(crate) selection_anchor: Option<Position>,
    /// True if selection is block-wise (column selection), false for normal line-wise
    pub(crate) block_selection: bool,
    /// Multiple cursor positions (for Alt+Down multi-cursor mode)
    /// When non-empty, typing inserts at all cursor positions
    pub(crate) multi_cursors: Vec<Position>,
    /// Cursor blink state for multi-cursor indicators (true = visible/inverted, false = hidden/normal)
    pub(crate) cursor_blink_state: bool,
    /// Last cursor blink toggle time
    pub(crate) last_blink_time: Option<Instant>,
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
    /// Timestamp of last save to prevent reload loops when current instance saves
    pub(crate) last_save_time: Option<Instant>,
    /// Find mode active
    pub(crate) find_active: bool,
    /// Current find pattern being entered
    pub(crate) find_pattern: String,
    /// Cursor position within find pattern (character index, not byte index)
    pub(crate) find_cursor_pos: usize,
    /// Error message for invalid regex
    pub(crate) find_error: Option<String>,
    /// Find history (last 100 searches)
    pub(crate) find_history: Vec<String>,
    /// Current position in find history (when navigating with Up/Down)
    pub(crate) find_history_index: Option<usize>,
    /// Last successful search pattern (for F3/Shift+F3)
    pub(crate) last_search_pattern: Option<String>,
    /// Search pattern saved before entering find mode (to restore on Esc)
    pub(crate) saved_search_pattern: Option<String>,
    /// Whether we've wrapped around in search
    pub(crate) search_wrapped: bool,
    /// Whether we're showing a wrap warning (waiting for second press to actually wrap)
    /// None = no warning, Some("next") = warning for next, Some("prev") = warning for prev
    pub(crate) wrap_warning_pending: Option<String>,
    /// Search scope when find mode is activated with a selection
    /// If set, find operations only search within this range (normalized start, end)
    pub(crate) find_scope: Option<(Position, Position)>,
    /// Go to line mode active
    pub(crate) goto_line_active: bool,
    /// Input buffer for go to line
    pub(crate) goto_line_input: String,
    /// Cursor position in goto_line_input (character index)
    pub(crate) goto_line_cursor_pos: usize,
    /// Whether user has started typing in goto_line mode (to replace pre-filled value)
    pub(crate) goto_line_typing_started: bool,
    /// Scrollbar dragging state
    pub(crate) scrollbar_dragging: bool,
    /// Original top_line when scrollbar drag started (to calculate relative movement)
    pub(crate) scrollbar_drag_start_top_line: usize,
    /// Mouse Y position when scrollbar drag started
    pub(crate) scrollbar_drag_start_y: u16,
    /// Offset within the scrollbar bar when dragging started (0 = top of bar, bar_height-1 = bottom)
    pub(crate) scrollbar_drag_bar_offset: usize,
    /// Help mode active
    pub(crate) help_active: bool,
    /// Help context (what help to show)
    pub(crate) help_context: crate::help::HelpContext,
    /// Help scroll offset
    pub(crate) help_scroll_offset: usize,
    /// Last mouse click time for detecting double/triple clicks
    #[allow(dead_code)]
    pub(crate) last_click_time: Option<Instant>,
    /// Last mouse click position (logical line, column)
    #[allow(dead_code)]
    pub(crate) last_click_pos: Option<Position>,
    /// Number of consecutive clicks at the same position
    #[allow(dead_code)]
    pub(crate) click_count: usize,
}

impl<'a> FileViewerState<'a> {
    pub(crate) fn new(term_width: u16, undo_history: UndoHistory, settings: &'a Settings) -> Self {
        Self {
            top_line: 0,
            cursor_line: 0,
            cursor_col: 0,
            selection_start: None,
            selection_end: None,
            selection_anchor: None,
            block_selection: false,
            multi_cursors: Vec::new(),
            cursor_blink_state: true,
            last_blink_time: None,
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
            last_save_time: None,
            find_active: false,
            find_pattern: String::new(),
            find_cursor_pos: 0,
            find_error: None,
            find_history: Vec::new(),
            find_history_index: None,
            last_search_pattern: None,
            saved_search_pattern: None,
            search_wrapped: false,
            wrap_warning_pending: None,
            find_scope: None,
            goto_line_active: false,
            goto_line_input: String::new(),
            goto_line_cursor_pos: 0,
            goto_line_typing_started: false,
            scrollbar_dragging: false,
            scrollbar_drag_start_top_line: 0,
            scrollbar_drag_start_y: 0,
            scrollbar_drag_bar_offset: 0,
            help_active: false,
            help_context: crate::help::HelpContext::Editor,
            help_scroll_offset: 0,
            last_click_time: None,
            last_click_pos: None,
            click_count: 0,
        }
    }

    pub(crate) fn current_position(&self) -> Position {
        (self.top_line + self.cursor_line, self.cursor_col)
    }

    pub(crate) fn absolute_line(&self) -> usize {
        // If cursor is saved (off-screen), use the saved position
        // Otherwise calculate from top_line + cursor_line
        self.saved_absolute_cursor
            .unwrap_or(self.top_line + self.cursor_line)
    }

    pub(crate) fn has_selection(&self) -> bool {
        self.selection_start.is_some() && self.selection_end.is_some()
    }

    pub(crate) fn start_selection(&mut self) {
        if self.selection_anchor.is_none() {
            // Set anchor at current position
            self.selection_anchor = Some(self.current_position());
            self.selection_start = Some(self.current_position());
            self.selection_end = Some(self.current_position());
        }
    }

    pub(crate) fn update_selection(&mut self) {
        if let Some(anchor) = self.selection_anchor {
            let cursor = self.current_position();

            if self.block_selection {
                // For block selection, handle line and column ranges independently
                let (start_line, end_line) = if anchor.0 <= cursor.0 {
                    (anchor.0, cursor.0)
                } else {
                    (cursor.0, anchor.0)
                };

                let (start_col, end_col) = if anchor.1 <= cursor.1 {
                    (anchor.1, cursor.1)
                } else {
                    (cursor.1, anchor.1)
                };

                self.selection_start = Some((start_line, start_col));
                self.selection_end = Some((end_line, end_col));
            } else {
                // For normal line-wise selection, use overall position comparison
                if anchor.0 < cursor.0 || (anchor.0 == cursor.0 && anchor.1 <= cursor.1) {
                    self.selection_start = Some(anchor);
                    self.selection_end = Some(cursor);
                } else {
                    self.selection_start = Some(cursor);
                    self.selection_end = Some(anchor);
                }
            }
        } else {
            // Fallback if no anchor (shouldn't happen in normal use)
            self.selection_end = Some(self.current_position());
        }
        self.needs_redraw = true;
    }

    pub(crate) fn clear_selection(&mut self) {
        if self.selection_start.is_some() || self.selection_end.is_some() {
            self.selection_start = None;
            self.selection_end = None;
            self.selection_anchor = None;
            self.block_selection = false;
            self.needs_redraw = true;
        }
    }

    pub(crate) fn adjust_cursor_col(&mut self, lines: &[&str]) {
        if let Some(line) = lines.get(self.absolute_line())
            && self.cursor_col > line.len()
        {
            self.cursor_col = line.len();
        }
    }

    /// Check if cursor is currently visible within the viewport
    /// This accounts for line wrapping - wrapped lines consume multiple visual lines
    pub(crate) fn is_cursor_visible(
        &self,
        lines: &[String],
        visible_lines: usize,
        text_width: u16,
    ) -> bool {
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
            let (start, end) = if s.0 < e.0 || (s.0 == e.0 && s.1 <= e.1) {
                (s, e)
            } else {
                (e, s)
            };
            Some((start, end))
        } else {
            None
        }
    }

    pub(crate) fn is_point_in_selection(&self, pos: Position) -> bool {
        if let Some((start, end)) = self.selection_range() {
            let (l, c) = pos;
            let (sl, sc) = start;
            let (el, ec) = end;

            if self.block_selection {
                // Block selection: check if line is in range and column is in range
                if l < sl || l > el {
                    return false;
                }
                // For block selection, column range applies to all lines
                c >= sc && c < ec
            } else {
                // Normal line-wise selection
                if l < sl || l > el {
                    return false;
                }
                if sl == el {
                    return c >= sc && c < ec;
                }
                if l == sl {
                    return c >= sc;
                }
                if l == el {
                    return c < ec;
                }
                true
            }
        } else {
            false
        }
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

    /// Add a cursor at the given position (for multi-cursor mode)
    pub(crate) fn add_cursor(&mut self, pos: Position) {
        if !self.multi_cursors.contains(&pos) {
            self.multi_cursors.push(pos);
            self.multi_cursors.sort();
            self.needs_redraw = true;
        }
    }

    /// Check if multi-cursor mode is active
    pub(crate) fn has_multi_cursors(&self) -> bool {
        !self.multi_cursors.is_empty()
    }

    /// Clear all multi-cursors
    pub(crate) fn clear_multi_cursors(&mut self) {
        if !self.multi_cursors.is_empty() {
            self.multi_cursors.clear();
            self.needs_redraw = true;
        }
    }

    /// Get all cursor positions (main cursor + multi-cursors)
    pub(crate) fn all_cursor_positions(&self) -> Vec<Position> {
        let mut positions = vec![self.current_position()];
        positions.extend(self.multi_cursors.iter().copied());
        positions.sort();
        positions.dedup();
        positions
    }

    /// Update cursor blink state (toggles every 500ms)
    /// Returns true if blink state changed and redraw is needed
    pub(crate) fn update_cursor_blink(&mut self) -> bool {
        // Check if we should blink: either multi-cursors or zero-width block selection
        let is_zero_width_block = self.block_selection
            && if let Some((start, end)) = self.selection_range() {
                start.1 == end.1 && start.0 != end.0 // Zero width, multiple lines
            } else {
                false
            };

        let should_blink = self.has_multi_cursors() || is_zero_width_block;

        if !should_blink {
            return false;
        }

        let now = Instant::now();
        let should_toggle = if let Some(last_time) = self.last_blink_time {
            now.duration_since(last_time).as_millis() >= 500
        } else {
            true // First time, initialize
        };

        if should_toggle {
            self.cursor_blink_state = !self.cursor_blink_state;
            self.last_blink_time = Some(now);
            true // Blink state changed, needs redraw
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::set_temp_home;

    #[test]
    fn cursor_visible_when_on_screen() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let undo_history = UndoHistory::new();
        let state = FileViewerState::new(80, undo_history, settings);
        let lines: Vec<String> = vec!["test".to_string(); 20];

        assert!(state.is_cursor_visible(&lines, 10, 80));
        assert_eq!(state.absolute_line(), 0);
    }

    #[test]
    fn cursor_invisible_when_saved_off_screen() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
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
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
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
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
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
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
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
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
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
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
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
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let undo_history = UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);

        state.top_line = 5;
        state.cursor_line = 3;
        state.saved_absolute_cursor = Some(8);

        state.ensure_cursor_visible(10);

        assert!(state.saved_absolute_cursor.is_none());
    }

    #[test]
    fn clear_selection_removes_selection_and_sets_redraw() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let undo_history = UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);

        // Set up a selection
        state.selection_start = Some((0, 0));
        state.selection_end = Some((0, 5));
        state.needs_redraw = false;

        assert!(state.has_selection());

        // Clear the selection
        state.clear_selection();

        // Verify selection is cleared and redraw flag is set
        assert!(!state.has_selection());
        assert!(state.needs_redraw);
        assert!(state.selection_start.is_none());
        assert!(state.selection_end.is_none());
    }

    #[test]
    fn multi_cursors_can_be_added_and_cleared() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let undo_history = UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);

        // Initially no multi-cursors
        assert!(!state.has_multi_cursors());

        // Add a cursor
        state.add_cursor((1, 5));
        assert!(state.has_multi_cursors());
        assert_eq!(state.multi_cursors.len(), 1);

        // Add another cursor
        state.add_cursor((3, 10));
        assert!(state.has_multi_cursors());
        assert_eq!(state.multi_cursors.len(), 2);

        // Clear multi-cursors
        state.clear_multi_cursors();
        assert!(!state.has_multi_cursors());
        assert_eq!(state.multi_cursors.len(), 0);
        assert!(state.needs_redraw);
    }

    #[test]
    fn block_selection_checks_column_range_only() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let undo_history = UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);

        // Set up a block selection from line 1, col 5 to line 3, col 10
        state.selection_start = Some((1, 5));
        state.selection_end = Some((3, 10));
        state.block_selection = true;

        // Points within the block (line 1-3, col 5-9)
        assert!(state.is_point_in_selection((1, 5))); // Start
        assert!(state.is_point_in_selection((1, 7))); // Middle of line 1
        assert!(state.is_point_in_selection((2, 5))); // Start of line 2
        assert!(state.is_point_in_selection((2, 9))); // Middle of line 2
        assert!(state.is_point_in_selection((3, 5))); // Start of line 3

        // Points outside the block
        assert!(!state.is_point_in_selection((0, 5))); // Line too early
        assert!(!state.is_point_in_selection((4, 5))); // Line too late
        assert!(!state.is_point_in_selection((2, 4))); // Column too early
        assert!(!state.is_point_in_selection((2, 10))); // Column at end (not included)
        assert!(!state.is_point_in_selection((2, 11))); // Column too late
    }

    #[test]
    fn clear_selection_resets_block_mode() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let undo_history = UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);

        // Set up a block selection
        state.selection_start = Some((0, 0));
        state.selection_end = Some((2, 5));
        state.block_selection = true;

        // Clear the selection
        state.clear_selection();

        // Verify block mode is reset
        assert!(!state.block_selection);
    }

    #[test]
    fn block_selection_direction_change_left() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let undo_history = UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);

        // Start at line 2, col 5
        state.top_line = 0;
        state.cursor_line = 2;
        state.cursor_col = 5;
        state.block_selection = true;

        // Start selection (anchor at 2,5)
        state.start_selection();
        assert_eq!(state.selection_anchor, Some((2, 5)));

        // Move down (line 3, col 5)
        state.cursor_line = 3;
        state.update_selection();
        assert_eq!(state.selection_start, Some((2, 5)));
        assert_eq!(state.selection_end, Some((3, 5)));

        // Move right (line 3, col 6)
        state.cursor_col = 6;
        state.update_selection();
        assert_eq!(state.selection_start, Some((2, 5)));
        assert_eq!(state.selection_end, Some((3, 6)));

        // Move left to anchor (line 3, col 5)
        state.cursor_col = 5;
        state.update_selection();
        // Should have zero-width selection (col 5-5)
        assert_eq!(state.selection_start, Some((2, 5)));
        assert_eq!(state.selection_end, Some((3, 5)));

        // Move left past anchor (line 3, col 4)
        state.cursor_col = 4;
        state.update_selection();
        // Should now select from col 4 to anchor col 5
        assert_eq!(state.selection_start, Some((2, 4)));
        assert_eq!(state.selection_end, Some((3, 5)));

        // Move left further (line 3, col 3)
        state.cursor_col = 3;
        state.update_selection();
        // Should select from col 3 to anchor col 5
        assert_eq!(state.selection_start, Some((2, 3)));
        assert_eq!(state.selection_end, Some((3, 5)));
    }
}
