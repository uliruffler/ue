/// Split pane layout management for multi-document support
use crate::editor_state::FileViewerState;
use crate::settings::Settings;
use crate::undo::UndoHistory;

/// Direction of a split
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SplitDirection {
    Horizontal, // Left/Right split
    Vertical,   // Up/Down split
}

/// Rectangle in terminal coordinates (0-based, inclusive)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rect {
    pub(crate) x: u16,
    pub(crate) y: u16,
    pub(crate) width: u16,
    pub(crate) height: u16,
}

impl Rect {
    pub(crate) fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self { x, y, width, height }
    }

    /// Check if a point (x, y) is inside this rectangle
    pub(crate) fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

/// A pane in the split tree - either a leaf (editor) or a split node
pub(crate) enum Pane<'settings> {
    /// Leaf node: actual editor with state, lines, and filename
    Leaf {
        state: FileViewerState<'settings>,
        lines: Vec<String>,
        filename: String,
        rect: Rect,
    },
    /// Split node: divides space into two children
    Split {
        direction: SplitDirection,
        /// First child (left or top)
        first: Box<Pane<'settings>>,
        /// Second child (right or bottom)
        second: Box<Pane<'settings>>,
        /// Split ratio (0.0 to 1.0) - how much of the space goes to first child
        ratio: f32,
        rect: Rect,
    },
}

#[allow(dead_code)]
impl<'settings> Pane<'settings> {
    /// Create a new leaf pane with the given filename and content
    pub(crate) fn new_leaf(
        filename: String,
        content: String,
        settings: &'settings Settings,
        rect: Rect,
    ) -> Self {
        let undo_history = UndoHistory::load(&filename).unwrap_or_else(|_| UndoHistory::new());

        let lines: Vec<String> = if let Some(saved) = &undo_history.file_content {
            saved.clone()
        } else {
            let mut l: Vec<String> = content.lines().map(String::from).collect();
            if l.is_empty() {
                l.push(String::new());
            }
            l
        };

        let mut state = FileViewerState::new(rect.width, undo_history.clone(), settings);
        state.modified = state.undo_history.modified;
        state.top_line = state.undo_history.scroll_top.min(lines.len());
        state.find_history = state.undo_history.find_history.clone();

        // Check if untitled file
        let filename_lower = std::path::Path::new(&filename)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        state.is_untitled = filename_lower.starts_with("untitled") && !std::path::Path::new(&filename).exists();

        // Update file menu
        state.menu_bar.update_file_menu(settings.max_menu_files, &filename, state.modified);

        // Restore cursor position
        let saved_cursor_line = state.undo_history.cursor_line;
        let saved_cursor_col = state.undo_history.cursor_col;
        if saved_cursor_line < lines.len() {
            state.cursor_line = saved_cursor_line.saturating_sub(state.top_line);
            if saved_cursor_col <= lines[saved_cursor_line].len() {
                state.cursor_col = saved_cursor_col;
                state.desired_cursor_col = saved_cursor_col;
            }
        }

        Pane::Leaf {
            state,
            lines,
            filename,
            rect,
        }
    }

    /// Get the rectangle occupied by this pane
    pub(crate) fn rect(&self) -> Rect {
        match self {
            Pane::Leaf { rect, .. } => *rect,
            Pane::Split { rect, .. } => *rect,
        }
    }

    /// Update the rectangle and propagate to children
    pub(crate) fn set_rect(&mut self, new_rect: Rect) {
        match self {
            Pane::Leaf { rect, state, .. } => {
                *rect = new_rect;
                state.term_width = new_rect.width;
            }
            Pane::Split {
                rect,
                direction,
                first,
                second,
                ratio,
            } => {
                *rect = new_rect;
                let (first_rect, second_rect) = Self::calculate_split_rects(new_rect, *direction, *ratio);
                first.set_rect(first_rect);
                second.set_rect(second_rect);
            }
        }
    }

    /// Find the leaf pane at the given screen coordinates
    pub(crate) fn find_pane_at(&mut self, x: u16, y: u16) -> Option<&mut Pane<'settings>> {
        match self {
            Pane::Leaf { rect, .. } => {
                if rect.contains(x, y) {
                    Some(self)
                } else {
                    None
                }
            }
            Pane::Split { first, second, .. } => {
                first.find_pane_at(x, y).or_else(|| second.find_pane_at(x, y))
            }
        }
    }

    /// Split this pane in the given direction, creating a new untitled document
    /// Returns true if the split was successful
    pub(crate) fn split(&mut self, direction: SplitDirection, settings: &'settings Settings) -> bool {
        match self {
            Pane::Leaf { rect, .. } => {
                // Check minimum size for split
                let min_size = 10u16; // Minimum width/height for a usable pane
                match direction {
                    SplitDirection::Horizontal if rect.width < min_size * 2 => return false,
                    SplitDirection::Vertical if rect.height < min_size * 2 => return false,
                    _ => {}
                }

                // Take ownership of current pane's data
                let old_pane = std::mem::replace(
                    self,
                    Pane::Leaf {
                        state: FileViewerState::new(1, UndoHistory::new(), settings),
                        lines: vec![String::new()],
                        filename: String::new(),
                        rect: Rect::new(0, 0, 1, 1),
                    },
                );

                // Generate untitled filename
                let untitled_filename = crate::ui::generate_untitled_filename();
                let untitled_path = std::env::var("UE_TEST_HOME")
                    .or_else(|_| std::env::var("HOME"))
                    .unwrap_or_else(|_| ".".to_string());
                let untitled_full_path = format!("{}/{}", untitled_path, untitled_filename);

                // Calculate split rectangles
                let old_rect = old_pane.rect();
                let ratio = 0.5;
                let (first_rect, second_rect) = Self::calculate_split_rects(old_rect, direction, ratio);

                // Create new untitled pane
                let new_pane = Pane::new_leaf(untitled_full_path, String::new(), settings, second_rect);

                // Update old pane's rectangle
                let mut first_pane = old_pane;
                first_pane.set_rect(first_rect);

                // Replace self with a split node
                *self = Pane::Split {
                    direction,
                    first: Box::new(first_pane),
                    second: Box::new(new_pane),
                    ratio,
                    rect: old_rect,
                };

                true
            }
            Pane::Split { .. } => {
                // Should not happen - we only split leaves
                false
            }
        }
    }

    /// Calculate the two rectangles resulting from a split
    fn calculate_split_rects(rect: Rect, direction: SplitDirection, ratio: f32) -> (Rect, Rect) {
        match direction {
            SplitDirection::Horizontal => {
                // Left/Right split
                let split_x = rect.x + (rect.width as f32 * ratio) as u16;
                let first_width = split_x.saturating_sub(rect.x);
                let second_width = rect.width.saturating_sub(first_width).saturating_sub(1); // -1 for divider

                let first = Rect::new(rect.x, rect.y, first_width, rect.height);
                let second = Rect::new(split_x + 1, rect.y, second_width, rect.height); // +1 to skip divider

                (first, second)
            }
            SplitDirection::Vertical => {
                // Top/Bottom split
                let split_y = rect.y + (rect.height as f32 * ratio) as u16;
                let first_height = split_y.saturating_sub(rect.y);
                let second_height = rect.height.saturating_sub(first_height).saturating_sub(1); // -1 for divider

                let first = Rect::new(rect.x, rect.y, rect.width, first_height);
                let second = Rect::new(rect.x, split_y + 1, rect.width, second_height); // +1 to skip divider

                (first, second)
            }
        }
    }

    /// Find the focused leaf pane (the one with cursor)
    #[allow(dead_code)]
    pub(crate) fn find_focused_leaf(&mut self) -> Option<&mut Pane<'settings>> {
        match self {
            Pane::Leaf { .. } => Some(self),
            Pane::Split { first, second, .. } => {
                // For now, we'll use a simple heuristic: check which pane has needs_redraw or modified
                // In practice, we'll track focus explicitly in the parent structure
                first.find_focused_leaf().or_else(|| second.find_focused_leaf())
            }
        }
    }

    /// Count total number of leaf panes
    pub(crate) fn count_leaves(&self) -> usize {
        match self {
            Pane::Leaf { .. } => 1,
            Pane::Split { first, second, .. } => first.count_leaves() + second.count_leaves(),
        }
    }

    /// Visit all leaf panes (mutable)
    pub(crate) fn visit_leaves_mut<F>(&mut self, f: &mut F)
    where
        F: FnMut(&mut FileViewerState<'settings>, &mut Vec<String>, &String, Rect),
    {
        match self {
            Pane::Leaf { state, lines, filename, rect } => {
                f(state, lines, filename, *rect);
            }
            Pane::Split { first, second, .. } => {
                first.visit_leaves_mut(f);
                second.visit_leaves_mut(f);
            }
        }
    }

    /// Visit all leaf panes (immutable)
    #[allow(dead_code)]
    pub(crate) fn visit_leaves<F>(&self, f: &mut F)
    where
        F: FnMut(&FileViewerState<'settings>, &[String], &str, Rect),
    {
        match self {
            Pane::Leaf { state, lines, filename, rect } => {
                f(state, lines, filename, *rect);
            }
            Pane::Split { first, second, .. } => {
                first.visit_leaves(f);
                second.visit_leaves(f);
            }
        }
    }

    /// Try to remove a leaf pane at the given position
    /// Returns (RemoveResult, optional replacement pane to avoid borrowing issues)
    pub(crate) fn try_remove_leaf_at(&mut self, x: u16, y: u16, settings: &'settings Settings) -> (RemoveResult, Option<Box<Pane<'settings>>>) {
        match self {
            Pane::Leaf { rect, .. } => {
                if rect.contains(x, y) {
                    (RemoveResult::RemoveThis, None)
                } else {
                    (RemoveResult::NotFound, None)
                }
            }
            Pane::Split { first, second, rect, .. } => {
                let (first_result, _) = first.try_remove_leaf_at(x, y, settings);
                match first_result {
                    RemoveResult::RemoveThis => {
                        // First child should be removed - return second child as replacement
                        let rect_copy = *rect;
                        let mut replacement = std::mem::replace(second, Box::new(create_dummy_pane(settings)));
                        replacement.set_rect(rect_copy);
                        (RemoveResult::Collapsed, Some(replacement))
                    }
                    RemoveResult::Collapsed => (RemoveResult::Collapsed, None),
                    RemoveResult::NotFound => {
                        let (second_result, _) = second.try_remove_leaf_at(x, y, settings);
                        match second_result {
                            RemoveResult::RemoveThis => {
                                // Second child should be removed - return first child as replacement
                                let rect_copy = *rect;
                                let mut replacement = std::mem::replace(first, Box::new(create_dummy_pane(settings)));
                                replacement.set_rect(rect_copy);
                                (RemoveResult::Collapsed, Some(replacement))
                            }
                            other => (other, None),
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoveResult {
    /// The target leaf was found and should be removed
    RemoveThis,
    /// A child was removed and the split collapsed
    Collapsed,
    /// The target was not found in this subtree
    NotFound,
}

/// Helper to create a dummy pane (for swap operations)
fn create_dummy_pane(settings: &Settings) -> Pane<'_> {
    Pane::Leaf {
        state: FileViewerState::new(1, UndoHistory::new(), settings),
        lines: vec![String::new()],
        filename: String::new(),
        rect: Rect::new(0, 0, 1, 1),
    }
}

/// Root container for split panes
pub(crate) struct SplitContainer<'settings> {
    pub(crate) root: Pane<'settings>,
    /// Which pane currently has focus (tracked by screen coordinates)
    pub(crate) focus_x: u16,
    pub(crate) focus_y: u16,
}

impl<'settings> SplitContainer<'settings> {
    pub(crate) fn new(filename: String, content: String, settings: &'settings Settings, rect: Rect) -> Self {
        let root = Pane::new_leaf(filename, content, settings, rect);
        Self {
            root,
            focus_x: rect.x,
            focus_y: rect.y,
        }
    }

    /// Get the currently focused pane
    pub(crate) fn focused_pane(&mut self) -> Option<&mut Pane<'settings>> {
        self.root.find_pane_at(self.focus_x, self.focus_y)
    }

    /// Set focus to the pane at the given coordinates
    pub(crate) fn set_focus(&mut self, x: u16, y: u16) -> bool {
        if let Some(_pane) = self.root.find_pane_at(x, y) {
            self.focus_x = x;
            self.focus_y = y;
            true
        } else {
            false
        }
    }

    /// Split the currently focused pane
    pub(crate) fn split_focused(&mut self, direction: SplitDirection, settings: &'settings Settings) -> bool {
        if let Some(pane) = self.focused_pane() {
            let result = pane.split(direction, settings);
            if result {
                // Update focus to the newly created pane (second child)
                if let Pane::Split { second, .. } = pane {
                    let new_rect = second.rect();
                    self.focus_x = new_rect.x;
                    self.focus_y = new_rect.y;
                }
            }
            result
        } else {
            false
        }
    }

    /// Close the currently focused pane
    /// Returns true if a pane was closed
    pub(crate) fn close_focused(&mut self, settings: &'settings Settings) -> bool {
        let (result, replacement_opt) = self.root.try_remove_leaf_at(self.focus_x, self.focus_y, settings);

        if let Some(replacement) = replacement_opt {
            self.root = *replacement;
        }

        match result {
            RemoveResult::RemoveThis | RemoveResult::Collapsed => {
                // Update focus to root position
                let rect = self.root.rect();
                self.focus_x = rect.x;
                self.focus_y = rect.y;
                true
            }
            RemoveResult::NotFound => false,
        }
    }

    /// Update the root rectangle and propagate to all panes
    pub(crate) fn set_rect(&mut self, rect: Rect) {
        self.root.set_rect(rect);
    }

    /// Count total leaf panes
    pub(crate) fn count_panes(&self) -> usize {
        self.root.count_leaves()
    }
}

