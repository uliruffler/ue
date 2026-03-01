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
    /// True if only the footer needs to be redrawn (avoids full screen redraw)
    pub(crate) needs_footer_redraw: bool,
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
    /// Logical position (line, col) where the user clicked to start a selection-drag.
    /// Used to move the cursor and clear the selection when the user clicks inside a
    /// selection without actually dragging (i.e., drag_target remains None on mouse-up).
    pub(crate) drag_click_logical_pos: Option<Position>,
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
    /// Help mode active (legacy — kept for tests)
    pub(crate) help_active: bool,
    /// Help context (what help to show) — legacy, kept for tests
    #[allow(dead_code)]
    pub(crate) help_context: crate::help::HelpContext,
    /// Help scroll offset — legacy, kept for tests
    #[allow(dead_code)]
    pub(crate) help_scroll_offset: usize,
    /// Signals that the editing loop should open the help file for the given context.
    /// Set by F1 / menu help actions; consumed by ui.rs to launch the viewer.
    pub(crate) open_help_requested: Option<crate::help::HelpContext>,
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
    /// Close all confirmation prompt active
    pub(crate) close_all_confirmation_active: bool,
    /// Set to true when user confirms close all (Enter pressed)
    pub(crate) close_all_confirmed: bool,
    /// Whether this is an untitled file that hasn't been saved to disk yet
    pub(crate) is_untitled: bool,
    /// Whether this file is read-only (no write permission)
    /// In read-only mode, editing operations are blocked but navigation/copy/find still work
    pub(crate) is_read_only: bool,
    /// Whether the editor was started with elevated privileges (sudo/root)
    pub(crate) is_sudo: bool,
    /// Whether the current file is displayed in rendered markdown mode.
    /// When true, `rendered_lines` are shown instead of the raw source lines.
    /// Only active for markdown files (.md / .markdown).
    pub(crate) markdown_rendered: bool,
    /// Pre-rendered markdown display lines (populated when `markdown_rendered` is true).
    /// Contains ANSI-escaped text produced by termimad; treated as read-only display content.
    pub(crate) rendered_lines: Vec<String>,
    /// Selection start within rendered_lines (line_index, visual_col) — only valid in rendered mode.
    pub(crate) rendered_selection_start: Option<(usize, usize)>,
    /// Selection end within rendered_lines (line_index, visual_col) — only valid in rendered mode.
    pub(crate) rendered_selection_end: Option<(usize, usize)>,
    /// Whether a mouse drag selection is active in rendered mode.
    pub(crate) rendered_mouse_dragging: bool,
    /// When cursor is at a wrap point, this tracks whether it's visually at the end of the
    /// previous segment (true) or at the start of the next segment (false)
    /// Only meaningful when cursor_col is exactly at a wrap point
    pub(crate) cursor_at_wrap_end: bool,
    /// Status message to show in the footer (e.g., warnings, errors)
    #[allow(dead_code)] // Read in rendering.rs (binary)
    pub(crate) status_message: Option<String>,
    /// True when the current mouse drag was initiated by clicking on the line number area.
    /// Used to distinguish line-number drags from text-area drags that move over line numbers.
    pub(crate) line_number_drag_active: bool,
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
            needs_footer_redraw: false,
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
            drag_click_logical_pos: None,
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
            open_help_requested: None,
            horizontal_scroll_offset: 0,
            line_wrapping_override: None,
            last_click_time: None,
            last_click_pos: None,
            click_count: 0,
            last_drag_position: None,
            menu_bar: crate::menu::MenuBar::new(),
            pending_menu_action: None,
            close_all_confirmation_active: false,
            close_all_confirmed: false,
            is_untitled: false,
            is_read_only: false,
            is_sudo: false,
            markdown_rendered: false,
            rendered_lines: Vec::new(),
            rendered_selection_start: None,
            rendered_selection_end: None,
            rendered_mouse_dragging: false,
            cursor_at_wrap_end: false,
            status_message: None,
            line_number_drag_active: false,
        }
    }

    pub(crate) fn current_position(&self) -> Position {
        (self.top_line + self.cursor_line, self.cursor_col)
    }

    /// Returns the effective background color for the UI chrome (header, footer, line
    /// numbers, menu, scrollbar) based on sudo, read-only, and rendered-markdown state:
    ///
    /// | sudo | read-only / rendered | color      |
    /// |------|----------------------|------------|
    /// | no   | no                   | configured |
    /// | no   | yes                  | pale blue  |
    /// | yes  | no                   | deep red   |
    /// | yes  | yes                  | pale red   |
    pub(crate) fn effective_theme_bg(&self) -> crossterm::style::Color {
        use crossterm::style::Color;
        let effectively_read_only = self.is_read_only || self.markdown_rendered;
        match (self.is_sudo, effectively_read_only) {
            (false, false) => {
                crate::settings::Settings::parse_color(&self.settings.appearance.header_bg)
                    .unwrap_or(Color::Rgb { r: 0, g: 24, b: 72 })
            }
            (false, true) => Color::Rgb { r: 30, g: 77, b: 122 },
            (true, false) => Color::Rgb { r: 90, g: 0, b: 0 },
            (true, true)  => Color::Rgb { r: 120, g: 80, b: 80 },
        }
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
        self.drag_click_logical_pos = None;
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

    /// Check if editing is blocked.
    /// Editing is blocked when the file is read-only OR when the rendered markdown view is
    /// active (the rendered view is intentionally read-only for now).
    pub(crate) fn is_editing_blocked(&self) -> bool {
        self.is_read_only || self.markdown_rendered
    }

    /// Clear the rendered-mode selection.
    pub(crate) fn clear_rendered_selection(&mut self) {
        self.rendered_selection_start = None;
        self.rendered_selection_end = None;
        self.rendered_mouse_dragging = false;
    }

    /// Return the normalized (start <= end) rendered selection, if any.
    pub(crate) fn rendered_selection_normalized(&self) -> Option<((usize, usize), (usize, usize))> {
        let start = self.rendered_selection_start?;
        let end = self.rendered_selection_end?;
        if start.0 < end.0 || (start.0 == end.0 && start.1 <= end.1) {
            Some((start, end))
        } else {
            Some((end, start))
        }
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
            let line_char_len = line.chars().count();
            if self.cursor_col < line_char_len {
                // Check wrap point logic
                if self.is_line_wrapping_enabled() {
                    let text_width = crate::coordinates::calculate_text_width(self, lines, visible_lines);
                    let wrap_points = crate::coordinates::calculate_word_wrap_points(line, text_width as usize, self.settings.tab_width);

                    // If we're AT a wrap point with wrap_end=true, move to start of next segment (same position, clear flag)
                    if wrap_points.contains(&self.cursor_col) && self.cursor_at_wrap_end {
                        self.cursor_at_wrap_end = false;
                        self.desired_cursor_col = self.cursor_col;
                        return true;
                    }

                    let next_pos = self.cursor_col + 1;

                    // If moving TO a wrap point from the left, land on wrap indicator (wrap_end=true)
                    if wrap_points.contains(&next_pos) && !self.cursor_at_wrap_end {
                        self.cursor_col = next_pos;
                        self.cursor_at_wrap_end = true;
                        self.desired_cursor_col = self.cursor_col;
                        return true;
                    }
                }

                // Normal move right within current line
                self.cursor_col += 1;
                self.cursor_at_wrap_end = false;
                self.desired_cursor_col = self.cursor_col;
                return true;
            }

            // At end of line - try to move to next line
            if absolute_line + 1 < lines.len() {
                let effective_visible_lines = self.effective_visible_lines(lines, visible_lines);
                self.cursor_line += 1;
                self.cursor_col = 0;
                self.cursor_at_wrap_end = false;
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
        let absolute_line = self.absolute_line();

        // Check if we're AT a wrap point (start of segment) and should move to wrap indicator
        if self.is_line_wrapping_enabled() && !self.cursor_at_wrap_end && self.cursor_col > 0 {
            if let Some(line) = lines.get(absolute_line) {
                let visible_lines = 10; // Default value
                let text_width = crate::coordinates::calculate_text_width(self, lines, visible_lines);
                let wrap_points = crate::coordinates::calculate_word_wrap_points(line, text_width as usize, self.settings.tab_width);

                // If we're AT a wrap point (start of segment), move to wrap indicator (same position, set flag)
                if wrap_points.contains(&self.cursor_col) {
                    self.cursor_at_wrap_end = true;
                    self.desired_cursor_col = self.cursor_col;
                    return true;
                }
            }
        }

        if self.cursor_col > 0 {
            // Normal left movement (also handles moving left from wrap_end position)
            self.cursor_col -= 1;
            self.cursor_at_wrap_end = false;
            self.desired_cursor_col = self.cursor_col;
            return true;
        }

        // At start of line - try to move to previous line
        if absolute_line > 0 {
            if self.cursor_line > 0 {
                self.cursor_line -= 1;
            } else if self.top_line > 0 {
                self.top_line -= 1;
            }

            let new_absolute = self.absolute_line();
            if let Some(line) = lines.get(new_absolute) {
                self.cursor_col = line.chars().count();
                self.cursor_at_wrap_end = false;
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
            target_col.min(lines[target_line].chars().count())
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
            let line_char_len = line.chars().count();
            if self.cursor_col > line_char_len {
                self.cursor_col = line_char_len;
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
            let line_char_len = line.chars().count();
            let clamped_col = col.min(line_char_len);
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
    fn wrap_cursor_move_right_past_indicator() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let undo_history = UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);

        // Create a long line with no spaces (will use character wrapping)
        let lines = vec!["x".repeat(100)];
        let visible_lines = 10;

        // Enable line wrapping
        state.toggle_line_wrapping();

        // Calculate text width
        let text_width = crate::coordinates::calculate_text_width(&state, &lines, visible_lines);
        let usable_width = (text_width as usize).saturating_sub(1);

        // Check wrap points
        let wrap_points = crate::coordinates::calculate_word_wrap_points(&lines[0], text_width as usize, 4);
        assert!(!wrap_points.is_empty(), "Should have wrap points for long line");

        // Start at position 0
        state.cursor_col = 0;

        // Move right one character at a time and check cursor never stays at wrap indicator position
        for _ in 0..80 {
            let old_col = state.cursor_col;

            let moved = state.move_cursor_right(&lines, visible_lines);

            if !moved {
                break;
            }

            // Check that cursor moved forward
            assert!(state.cursor_col > old_col, "Cursor should advance on move_right");

            // Check cursor's visual position within its segment
            let visual_col_after = crate::coordinates::visual_width_up_to(&lines[0], state.cursor_col, 4);
            let wrap_points = crate::coordinates::calculate_word_wrap_points(&lines[0], text_width as usize, 4);

            // Find which segment cursor is in
            let mut segment_start = 0;
            for &wp in &wrap_points {
                if state.cursor_col < wp {
                    break;
                }
                segment_start = wp;
            }

            let segment_start_visual = crate::coordinates::visual_width_up_to(&lines[0], segment_start, 4);
            let offset_in_segment = visual_col_after - segment_start_visual;

            // Cursor should never be at usable_width offset (where wrap indicator shows)
            // unless it's at the end of the line
            if state.cursor_col < lines[0].len() {
                assert!(offset_in_segment < usable_width,
                       "Cursor at col {} should not be at wrap indicator position (offset {} >= usable {})",
                       state.cursor_col, offset_in_segment, usable_width);
            }
        }
    }

    #[test]
    fn cursor_at_wrap_point_behavior() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let undo_history = UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);

        // Create a simple line with a known wrap point at position 10
        let lines = vec!["x".repeat(100)];
        let visible_lines = 10;

        // Enable line wrapping
        state.toggle_line_wrapping();

        // Manually test wrap_end behavior at a wrap point
        // Simulate being at position 10 which could be a wrap point
        state.cursor_col = 10;
        state.cursor_at_wrap_end = false;

        // Test that cursor_at_wrap_end flag can be set and cleared
        state.cursor_at_wrap_end = true;
        assert!(state.cursor_at_wrap_end);

        state.cursor_at_wrap_end = false;
        assert!(!state.cursor_at_wrap_end);

        // Test that typing clears the flag
        let _ = crate::editing::insert_char(&mut state, &mut vec![lines[0].clone()], 'a', "test", visible_lines);
        assert!(!state.cursor_at_wrap_end, "Typing should clear cursor_at_wrap_end");
    }

    #[test]
    fn cursor_at_wrap_point_from_right() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let undo_history = UndoHistory::new();
        let mut state = FileViewerState::new(80, undo_history, settings);

        // Create a line with wrapping
        let lines = vec!["x".repeat(100)];
        let visible_lines = 10;

        // Enable line wrapping
        state.toggle_line_wrapping();

        // Get wrap point
        let text_width = crate::coordinates::calculate_text_width(&state, &lines, visible_lines);
        let wrap_points = crate::coordinates::calculate_word_wrap_points(&lines[0], text_width as usize, 4);
        let first_wrap = wrap_points[0];

        // Position cursor after wrap point
        state.cursor_col = first_wrap + 1;
        state.cursor_at_wrap_end = false;

        // Move left - should land at wrap point at start of segment (not wrap_end)
        let moved = state.move_cursor_left(&lines);
        assert!(moved);
        assert_eq!(state.cursor_col, first_wrap);
        assert!(!state.cursor_at_wrap_end, "Moving left to wrap point should be at start of segment");

        // Move left again - should move to previous character
        let moved2 = state.move_cursor_left(&lines);
        assert!(moved2);
        assert_eq!(state.cursor_col, first_wrap - 1);
        assert!(!state.cursor_at_wrap_end);
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





