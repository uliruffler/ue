use crate::settings::Settings;
use crate::undo::UndoHistory;
use std::time::Instant;

/// Type alias for cursor/selection position (line, column)
pub(crate) type Position = (usize, usize);

pub struct FileViewerState<'a> {
    pub(crate) top_line: usize,
    pub(crate) cursor_line: usize,
    pub(crate) cursor_col: usize,
    /// Desired column position for vertical navigation (remembers position through short lines)
    pub(crate) desired_cursor_col: usize,
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
    /// Find mode: true = regex, false = wildcard (* and ? only)
    pub(crate) find_regex_mode: bool,
    /// True if find mode was entered via replace keybinding (auto-enter replace after search)
    pub(crate) find_via_replace: bool,
    /// Filter mode active (shows only lines with search hits)
    pub(crate) filter_active: bool,
    /// Number of context lines to show before each match in filter mode
    pub(crate) filter_context_before: usize,
    /// Number of context lines to show after each match in filter mode
    pub(crate) filter_context_after: usize,
    /// Current find pattern being entered
    pub(crate) find_pattern: String,
    /// Cursor position within find pattern (character index, not byte index)
    pub(crate) find_cursor_pos: usize,
    /// Selection in find pattern: (start_pos, end_pos) in character indices
    pub(crate) find_selection: Option<(usize, usize)>,
    /// Error message for invalid regex
    pub(crate) find_error: Option<String>,
    /// Find history (last 100 searches)
    pub(crate) find_history: Vec<String>,
    /// Current position in find history (when navigating with Up/Down)
    pub(crate) find_history_index: Option<usize>,
    /// Last successful search pattern (for F3/Shift+F3)
    pub(crate) last_search_pattern: Option<String>,
    /// Whether the last search pattern used regex mode (true) or wildcard mode (false)
    pub(crate) last_search_regex_mode: bool,
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
    /// Total number of search hits in the document
    pub(crate) search_hit_count: usize,
    /// Current hit index (1-based, 0 means cursor not on a hit)
    pub(crate) search_current_hit: usize,
    /// Replace mode active (entered from find mode with Ctrl+H)
    pub(crate) replace_active: bool,
    /// Replacement text being entered
    pub(crate) replace_pattern: String,
    /// Cursor position within replace pattern (character index, not byte index)
    pub(crate) replace_cursor_pos: usize,
    /// Selection in replace pattern: (start_pos, end_pos) in character indices
    pub(crate) replace_selection: Option<(usize, usize)>,
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
    /// Horizontal scrollbar dragging active
    pub(crate) h_scrollbar_dragging: bool,
    /// Original horizontal_scroll_offset when h_scrollbar drag started
    pub(crate) h_scrollbar_drag_start_offset: usize,
    /// Mouse X position when h_scrollbar drag started
    pub(crate) h_scrollbar_drag_start_x: u16,
    /// Offset within the h_scrollbar bar when dragging started
    pub(crate) h_scrollbar_drag_bar_offset: usize,
    /// Help mode active
    pub(crate) help_active: bool,
    /// Help context (what help to show)
    pub(crate) help_context: crate::help::HelpContext,
    /// Help scroll offset
    pub(crate) help_scroll_offset: usize,
    /// Horizontal scroll offset (character offset from line start)
    /// Only used when line_wrapping is false
    pub(crate) horizontal_scroll_offset: usize,
    /// Runtime line wrapping toggle (overrides settings.line_wrapping)
    /// None means use settings.line_wrapping, Some means user toggled at runtime
    pub(crate) line_wrapping_override: Option<bool>,
    /// Last mouse click time for detecting double/triple clicks
    #[allow(dead_code)]
    pub(crate) last_click_time: Option<Instant>,
    /// Last mouse click position (logical line, column)
    #[allow(dead_code)]
    pub(crate) last_click_pos: Option<Position>,
    /// Number of consecutive clicks at the same position
    #[allow(dead_code)]
    pub(crate) click_count: usize,
    /// Last mouse drag position (visual_line, column) for continuous auto-scroll
    pub(crate) last_drag_position: Option<(usize, u16)>,
    /// Menu bar state
    pub(crate) menu_bar: crate::menu::MenuBar,
    /// Pending menu action to execute
    pub(crate) pending_menu_action: Option<crate::menu::MenuAction>,
    /// Whether this is an untitled file that hasn't been saved to disk yet
    pub(crate) is_untitled: bool,
}

impl<'a> FileViewerState<'a> {
    pub(crate) fn new(term_width: u16, undo_history: UndoHistory, settings: &'a Settings) -> Self {
        Self {
            top_line: 0,
            cursor_line: 0,
            cursor_col: 0,
            desired_cursor_col: 0,
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
            find_regex_mode: true,
            find_via_replace: false,
            filter_active: false,
            filter_context_before: settings.filter_context_before,
            filter_context_after: settings.filter_context_after,
            find_pattern: String::new(),
            find_cursor_pos: 0,
            find_selection: None,
            find_error: None,
            find_history: Vec::new(),
            find_history_index: None,
            last_search_pattern: None,
            last_search_regex_mode: true,
            saved_search_pattern: None,
            search_wrapped: false,
            wrap_warning_pending: None,
            find_scope: None,
            search_hit_count: 0,
            search_current_hit: 0,
            replace_active: false,
            replace_pattern: String::new(),
            replace_cursor_pos: 0,
            replace_selection: None,
            goto_line_active: false,
            goto_line_input: String::new(),
            goto_line_cursor_pos: 0,
            goto_line_typing_started: false,
            scrollbar_dragging: false,
            scrollbar_drag_start_top_line: 0,
            scrollbar_drag_start_y: 0,
            scrollbar_drag_bar_offset: 0,
            h_scrollbar_dragging: false,
            h_scrollbar_drag_start_offset: 0,
            h_scrollbar_drag_start_x: 0,
            h_scrollbar_drag_bar_offset: 0,
            help_active: false,
            help_context: crate::help::HelpContext::Editor,
            help_scroll_offset: 0,
            horizontal_scroll_offset: 0,
            line_wrapping_override: None,
            last_click_time: None,
            last_click_pos: None,
            click_count: 0,
            last_drag_position: None,
            menu_bar: crate::menu::MenuBar::new(),
            pending_menu_action: None,
            is_untitled: false,
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

    /// Determine cursor position relative to visible area
    /// Returns: Some(true) if above, Some(false) if below, None if visible
    pub(crate) fn cursor_off_screen_direction(
        &self,
        lines: &[String],
        visible_lines: usize,
        text_width: u16,
    ) -> Option<bool> {
        if self.is_cursor_visible(lines, visible_lines, text_width) {
            return None; // Cursor is visible
        }

        let absolute = self.absolute_line();

        // If cursor is above top_line, it's above visible area
        if absolute < self.top_line {
            return Some(true); // Above
        }

        // Otherwise it's below visible area
        Some(false) // Below
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

        // Calculate effective visible lines (reduced by 1 if h-scrollbar is shown)
        let effective_visible_lines = if self.should_show_h_scrollbar(lines, visible_lines) {
            visible_lines.saturating_sub(1)
        } else {
            visible_lines
        };

        // Calculate how many visual lines are consumed from top_line through cursor_line
        let visual_lines_consumed = calculate_visual_lines_to_cursor(lines, self, text_width);

        // Check vertical visibility
        if visual_lines_consumed > effective_visible_lines {
            return false;
        }

        // Check horizontal visibility when line wrapping is disabled
        if !self.is_line_wrapping_enabled() {
            use crate::coordinates::visual_width_up_to;
            let cursor_line_idx = self.absolute_line();
            if cursor_line_idx < lines.len() {
                let visual_col = visual_width_up_to(
                    &lines[cursor_line_idx],
                    self.cursor_col,
                    self.settings.tab_width,
                );

                // Check if cursor is scrolled off to the left
                if visual_col < self.horizontal_scroll_offset {
                    return false;
                }

                // Check if cursor is scrolled off to the right
                let visible_width = text_width as usize;
                if visual_col >= self.horizontal_scroll_offset + visible_width {
                    return false;
                }
            }
        }

        true
    }

    /// Ensure cursor is visible after undo/redo operations
    /// If cursor would be off-screen, move it to the first or last visible line
    pub(crate) fn ensure_cursor_visible(&mut self, visible_lines: usize, lines: &[String]) {
        // Get the absolute cursor position (may use saved value)
        let absolute = self.absolute_line();

        // Calculate effective visible lines (reduced by 1 if h-scrollbar is shown)
        let effective_visible_lines = if self.should_show_h_scrollbar(lines, visible_lines) {
            visible_lines.saturating_sub(1)
        } else {
            visible_lines
        };

        // Clear saved cursor - we're bringing it back on screen
        self.saved_absolute_cursor = None;
        self.saved_scroll_state = None;

        // If cursor is below visible area, move it to last visible line
        if self.cursor_line >= effective_visible_lines {
            self.cursor_line = effective_visible_lines.saturating_sub(1);
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

    /// Check if line wrapping is currently enabled (considers runtime override)
    pub(crate) fn is_line_wrapping_enabled(&self) -> bool {
        self.line_wrapping_override.unwrap_or(self.settings.line_wrapping)
    }

    /// Toggle line wrapping at runtime
    pub(crate) fn toggle_line_wrapping(&mut self) {
        let current = self.is_line_wrapping_enabled();
        self.line_wrapping_override = Some(!current);
        // Reset horizontal scroll when enabling wrapping
        if !current {
            self.horizontal_scroll_offset = 0;
        }
    }

    /// Check if horizontal scrollbar should be shown
    pub(crate) fn should_show_h_scrollbar(&self, lines: &[String], visible_lines: usize) -> bool {
        // Only show horizontal scrollbar when:
        // 1. Line wrapping is disabled
        // 2. At least one line exceeds the visible width
        if self.is_line_wrapping_enabled() {
            return false;
        }

        use crate::coordinates::{calculate_text_width, visual_width};
        let text_width = calculate_text_width(self, lines, visible_lines) as usize;
        let tab_width = self.settings.tab_width;

        let max_line_width = lines.iter()
            .map(|line| visual_width(line, tab_width))
            .max()
            .unwrap_or(0);

        // Only show scrollbar if there's content wider than the visible area
        max_line_width > text_width
    }

    /// Calculate effective visible lines for content (reduced by 1 if h-scrollbar is shown)
    pub(crate) fn effective_visible_lines(&self, lines: &[String], visible_lines: usize) -> usize {
        if self.should_show_h_scrollbar(lines, visible_lines) {
            visible_lines.saturating_sub(1)
        } else {
            visible_lines
        }
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

    // ===== Cursor Movement Helpers =====
    // These functions encapsulate common cursor movement patterns and ensure
    // invariants are maintained (bounds checking, desired_cursor_col updates, etc.)
    
    /// Move cursor right by one character, handling line boundaries
    /// Returns true if cursor moved
    pub(crate) fn move_cursor_right(&mut self, lines: &[String], visible_lines: usize) -> bool {
        let absolute_line = self.absolute_line();

        if let Some(line) = lines.get(absolute_line) {
            if self.cursor_col < line.len() {
                // Move right within current line
                self.cursor_col += 1;
                self.desired_cursor_col = self.cursor_col;
                return true;
            }

            // At end of line - try to move to next line
            if absolute_line + 1 < lines.len() {
                let effective_visible_lines = self.effective_visible_lines(lines, visible_lines);
                self.cursor_line += 1;
                self.cursor_col = 0;
                self.desired_cursor_col = 0;

                // Check if we need to scroll
                if self.cursor_line >= effective_visible_lines {
                    self.top_line += 1;
                    self.cursor_line = effective_visible_lines - 1;
                }
                return true;
            }
        }
        false
    }

    /// Move cursor left by one character, handling line boundaries
    /// Returns true if cursor moved
    pub(crate) fn move_cursor_left(&mut self, lines: &[String]) -> bool {
        if self.cursor_col > 0 {
            // Move left within current line
            self.cursor_col -= 1;
            self.desired_cursor_col = self.cursor_col;
            return true;
        }

        // At start of line - try to move to previous line
        let absolute_line = self.absolute_line();
        if absolute_line > 0 {
            if self.cursor_line > 0 {
                self.cursor_line -= 1;
            } else if self.top_line > 0 {
                self.top_line -= 1;
            }

            let new_absolute = self.absolute_line();
            if let Some(line) = lines.get(new_absolute) {
                self.cursor_col = line.len();
                self.desired_cursor_col = self.cursor_col;
            }
            return true;
        }
        false
    }

    /// Set cursor to a specific position with bounds checking and viewport adjustment
    /// This is the safe way to jump to a position (used by find, goto, etc.)
    pub(crate) fn set_cursor_position(
        &mut self,
        target_line: usize,
        target_col: usize,
        lines: &[String],
        visible_lines: usize,
    ) {
        // Clamp line to valid range
        let target_line = target_line.min(lines.len().saturating_sub(1));

        // Clamp column to line length
        let target_col = if target_line < lines.len() {
            target_col.min(lines[target_line].len())
        } else {
            0
        };

        // Clear any off-screen state
        self.saved_absolute_cursor = None;
        self.saved_scroll_state = None;

        // Adjust viewport if target is outside visible area
        if target_line < self.top_line {
            // Target is above viewport - scroll up
            self.top_line = target_line;
            self.cursor_line = 0;
        } else if target_line >= self.top_line + visible_lines {
            // Target is below viewport - scroll down to center target
            self.top_line = target_line.saturating_sub(visible_lines / 2);
            self.cursor_line = target_line - self.top_line;
        } else {
            // Target is within viewport - just adjust cursor_line
            self.cursor_line = target_line - self.top_line;
        }

        // Set cursor column
        self.cursor_col = target_col;
        self.desired_cursor_col = target_col;

        self.needs_redraw = true;
    }

    /// Ensure cursor column is within bounds for current line
    /// Call this after any operation that might leave cursor past end of line
    pub(crate) fn clamp_cursor_to_line_bounds(&mut self, lines: &[String]) {
        let absolute_line = self.absolute_line();
        if let Some(line) = lines.get(absolute_line) {
            if self.cursor_col > line.len() {
                self.cursor_col = line.len();
                self.desired_cursor_col = self.cursor_col;
            }
        }
    }

    /// Set cursor column within current line with bounds checking
    /// Does NOT move between lines - use move_cursor_right/left for that
    /// Updates both cursor_col and desired_cursor_col
    pub(crate) fn set_cursor_col(&mut self, col: usize, lines: &[String]) {
        let absolute_line = self.absolute_line();
        if let Some(line) = lines.get(absolute_line) {
            let clamped_col = col.min(line.len());
            self.cursor_col = clamped_col;
            self.desired_cursor_col = clamped_col;
        }
    }

    /// Debug-only validation of cursor invariants
    /// In debug builds, call this after cursor mutations to catch bugs early
    #[cfg(debug_assertions)]
    pub(crate) fn validate_cursor_invariants(&self, lines: &[String]) {
        let abs = self.absolute_line();

        // Check absolute line is in bounds
        assert!(
            abs < lines.len() || lines.is_empty(),
            "Cursor absolute line {} out of bounds (total lines: {})",
            abs,
            lines.len()
        );

        // Check cursor column is in bounds
        if abs < lines.len() {
            assert!(
                self.cursor_col <= lines[abs].len(),
                "Cursor col {} out of bounds for line {} (len: {})",
                self.cursor_col,
                abs,
                lines[abs].len()
            );
        }
    }

    /// No-op in release builds
    #[cfg(not(debug_assertions))]
    #[inline]
    pub(crate) fn validate_cursor_invariants(&self, _lines: &[String]) {
        // Debug-only, no-op in release
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
        let lines: Vec<String> = vec!["test".to_string(); 30];

        state.top_line = 10;
        state.cursor_line = 15; // Beyond visible area of 10 lines

        state.ensure_cursor_visible(10, &lines);

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
        let lines: Vec<String> = vec!["test".to_string(); 30];

        state.top_line = 20;
        state.cursor_line = 0;
        state.saved_absolute_cursor = Some(15); // Cursor at line 15, but top_line is 20

        state.ensure_cursor_visible(10, &lines);

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
        let lines: Vec<String> = vec!["test".to_string(); 30];

        state.top_line = 5;
        state.cursor_line = 3;
        state.saved_absolute_cursor = Some(8);

        state.ensure_cursor_visible(10, &lines);

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
    fn zero_width_block_selection_shows_as_multi_line_cursors() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let undo_history = UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);

        // Initially no selection
        assert!(!state.block_selection);
        assert!(state.selection_start.is_none());

        // Create zero-width block selection (lines 1-3, column 5)
        state.block_selection = true;
        state.selection_start = Some((1, 5));
        state.selection_end = Some((3, 5));

        // Should be detected as zero-width block
        let is_zero_width = state.block_selection
            && if let Some((start, end)) = state.selection_range() {
                start.1 == end.1 && start.0 != end.0
            } else {
                false
            };
        assert!(is_zero_width, "Should be detected as zero-width block selection");

        // Clear selection
        state.clear_selection();
        assert!(!state.block_selection);
        assert!(state.selection_start.is_none());
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

// Public test helper methods (only exposed for integration tests via lib.rs)
impl<'a> FileViewerState<'a> {
    /// Create new state (for testing)
    #[allow(dead_code)]
    pub fn new_for_test(term_width: u16, undo_history: UndoHistory, settings: &'a Settings) -> Self {
        Self::new(term_width, undo_history, settings)
    }

    /// Toggle line wrapping (for testing)
    #[allow(dead_code)]
    pub fn toggle_line_wrapping_for_test(&mut self) {
        self.toggle_line_wrapping();
    }

    /// Check if line wrapping is enabled (for testing) - public wrapper
    #[allow(dead_code)]
    pub fn is_line_wrapping_enabled_for_test(&self) -> bool {
        self.is_line_wrapping_enabled()
    }

    /// Get horizontal scroll offset (for testing)
    #[allow(dead_code)]
    pub fn get_horizontal_scroll_offset(&self) -> usize {
        self.horizontal_scroll_offset
    }

    /// Set horizontal scroll offset (for testing)
    #[allow(dead_code)]
    pub fn set_horizontal_scroll_offset(&mut self, offset: usize) {
        self.horizontal_scroll_offset = offset;
    }

    /// Get cursor column (for testing)
    #[allow(dead_code)]
    pub fn get_cursor_col(&self) -> usize {
        self.cursor_col
    }

    /// Set cursor column (for testing) - direct mutation without bounds checking
    #[allow(dead_code)]
    pub fn set_cursor_col_test(&mut self, col: usize) {
        self.cursor_col = col;
    }

    /// Check if h-scrollbar should be shown (for testing)
    #[allow(dead_code)]
    pub fn should_show_h_scrollbar_for_test(&self, lines: &[String], visible_lines: usize) -> bool {
        self.should_show_h_scrollbar(lines, visible_lines)
    }

    /// Get effective visible lines (for testing)
    #[allow(dead_code)]
    pub fn effective_visible_lines_for_test(&self, lines: &[String], visible_lines: usize) -> usize {
        self.effective_visible_lines(lines, visible_lines)
    }

    /// Check if cursor is visible (for testing)
    #[allow(dead_code)]
    pub fn is_cursor_visible_for_test(&self, lines: &[String], visible_lines: usize, text_width: u16) -> bool {
        self.is_cursor_visible(lines, visible_lines, text_width)
    }

    /// Ensure cursor visible (for testing)
    #[allow(dead_code)]
    pub fn ensure_cursor_visible_for_test(&mut self, visible_lines: usize, lines: &[String]) {
        self.ensure_cursor_visible(visible_lines, lines)
    }

    /// Get cursor line (for testing)
    #[allow(dead_code)]
    pub fn get_cursor_line(&self) -> usize {
        self.cursor_line
    }

    /// Set cursor line (for testing)
    #[allow(dead_code)]
    pub fn set_cursor_line(&mut self, line: usize) {
        self.cursor_line = line;
    }

    /// Get top line (for testing)
    #[allow(dead_code)]
    pub fn get_top_line(&self) -> usize {
        self.top_line
    }

    /// Set top line (for testing)
    #[allow(dead_code)]
    pub fn set_top_line(&mut self, line: usize) {
        self.top_line = line;
    }
}





