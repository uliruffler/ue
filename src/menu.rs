use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::io::Write;
use std::path::Path;

/// Check if a file has unsaved changes by reading its undo history
pub(crate) fn check_file_has_unsaved_changes(file_path: &Path) -> bool {
    // Get the undo history file path for this file
    let file_str = file_path.to_string_lossy();
    if let Ok(undo_path) = crate::undo::UndoHistory::history_path_for(&file_str) {
        if undo_path.exists() {
            // Try to read and deserialize the undo history file
            if let Ok(content) = std::fs::read_to_string(&undo_path) {
                if let Ok(history) = serde_json::from_str::<crate::undo::UndoHistory>(&content) {
                    return history.modified;
                }
            }
        }
    }
    false
}



/// Menu item types
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MenuItem {
    /// Action item with label and action identifier
    Action { label: String, action: MenuAction },
    /// Checkable item with label, action identifier, and checked state
    Checkable { label: String, action: MenuAction, checked: bool },
    /// Separator line
    Separator,
}

/// Menu actions that can be triggered
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum MenuAction {
    // File menu
    FileNew,
    FileOpenDialog,
    #[allow(dead_code)] // Only used in ui.rs (binary)
    FileOpenRecent(usize), // Index into recent files list
    FileSave,
    FileClose,
    FileCloseAll,
    FileQuit,
    // Edit menu
    EditUndo,
    EditRedo,
    EditCopy,
    EditCut,
    EditPaste,
    EditFind,
    // View menu
    ViewLineWrap,
    // Help menu
    HelpEditor,
    HelpFind,
    HelpAbout,
    // Special actions
    FileRemove(usize), // Remove file at index from recent files (triggered by Ctrl+W)
}

/// Top-level menu definition
#[derive(Debug, Clone)]
pub(crate) struct Menu {
    pub(crate) label: String,
    pub(crate) items: Vec<MenuItem>,
    pub(crate) hotkey: char, // First letter for Alt+<letter> activation
}

impl Menu {
    pub(crate) fn new(label: &str, hotkey: char, items: Vec<MenuItem>) -> Self {
        Self {
            label: label.to_string(),
            items,
            hotkey,
        }
    }
}

/// Helper to create action menu items
fn action(label: &str, action: MenuAction) -> MenuItem {
    MenuItem::Action {
        label: label.to_string(),
        action,
    }
}

/// Helper to create checkable menu items
fn checkable(label: &str, action: MenuAction, checked: bool) -> MenuItem {
    MenuItem::Checkable {
        label: label.to_string(),
        action,
        checked,
    }
}

/// Count files in the file section of a menu
fn count_files_in_menu(menu: &Menu) -> usize {
    let mut count = 0;
    for (idx, item) in menu.items.iter().enumerate() {
        if idx >= FILE_SECTION_START_IDX {
            if matches!(item, MenuItem::Separator) {
                break;
            }
            if matches!(item, MenuItem::Action { .. }) {
                count += 1;
            }
        }
    }
    count
}

/// Menu bar state
#[derive(Debug)]
pub(crate) struct MenuBar {
    pub(crate) menus: Vec<Menu>,
    pub(crate) active: bool,
    pub(crate) dropdown_open: bool, // True when dropdown menu is shown
    pub(crate) selected_menu_index: usize,
    pub(crate) selected_item_index: usize,
    pub(crate) file_section_scroll_offset: usize, // Scroll offset for file section only (not entire menu)
    pub(crate) needs_redraw: bool, // True when menu needs to be redrawn
    pub(crate) max_visible_files: usize, // Maximum number of files to show in menu (from settings)
}

// Constants for File menu structure
const FILE_MENU_INDEX: usize = 0;
const FILE_SECTION_START_IDX: usize = 6; // First file after "New, Open, Save, Close, Close all, Separator"
impl MenuBar {
    pub(crate) fn new() -> Self {
        let menus = vec![
            Menu::new(
                "File",
                'f',
                vec![
                    action("New", MenuAction::FileNew),
                    action("Open...", MenuAction::FileOpenDialog),
                    action("Save", MenuAction::FileSave),
                    action("Close", MenuAction::FileClose),
                    action("Close all", MenuAction::FileCloseAll),
                    MenuItem::Separator,
                    action("Quit", MenuAction::FileQuit),
                ],
            ),
            Menu::new(
                "Edit",
                'e',
                vec![
                    action("Undo", MenuAction::EditUndo),
                    action("Redo", MenuAction::EditRedo),
                    MenuItem::Separator,
                    action("Copy", MenuAction::EditCopy),
                    action("Cut", MenuAction::EditCut),
                    action("Paste", MenuAction::EditPaste),
                    MenuItem::Separator,
                    action("Find", MenuAction::EditFind),
                ],
            ),
            Menu::new(
                "View",
                'v',
                vec![
                    checkable("Line Wrap", MenuAction::ViewLineWrap, false),
                ],
            ),
            Menu::new(
                "Help",
                ' ',
                vec![
                    action("Editor Help", MenuAction::HelpEditor),
                    action("Find Help", MenuAction::HelpFind),
                    MenuItem::Separator,
                    action("About", MenuAction::HelpAbout),
                ],
            ),
        ];

        Self {
            menus,
            active: false,
            dropdown_open: false,
            selected_menu_index: 0,
            selected_item_index: 0,
            file_section_scroll_offset: 0,
            needs_redraw: false,
            max_visible_files: 5, // Default value, updated from settings
        }
    }

    /// Open menu bar (activate first menu)
    pub(crate) fn open(&mut self) {
        self.active = true;
        self.dropdown_open = false; // Don't open dropdown initially
        self.selected_menu_index = 0;
        self.selected_item_index = 0;
        self.needs_redraw = true;
    }

    /// Open dropdown for currently selected menu
    pub(crate) fn open_dropdown(&mut self) {
        self.dropdown_open = true;
        self.selected_item_index = 0; // Reset to first item
        self.file_section_scroll_offset = 0; // Reset scroll state when opening dropdown
        self.needs_redraw = true;
    }

    /// Close dropdown only (keep menu bar active)
    fn close_dropdown(&mut self) {
        self.dropdown_open = false;
        self.file_section_scroll_offset = 0; // Reset scroll state when closing dropdown
        self.needs_redraw = true;
    }

    /// Close menu bar
    pub(crate) fn close(&mut self) {
        self.active = false;
        self.dropdown_open = false;
        self.file_section_scroll_offset = 0; // Reset scroll state
        self.needs_redraw = true;
    }

    /// Move to next menu
    pub(crate) fn next_menu(&mut self) {
        self.switch_menu((self.selected_menu_index + 1) % self.menus.len());
    }

    /// Move to previous menu
    pub(crate) fn prev_menu(&mut self) {
        let new_index = if self.selected_menu_index == 0 {
            self.menus.len() - 1
        } else {
            self.selected_menu_index - 1
        };
        self.switch_menu(new_index);
    }

    /// Common logic for switching menus
    fn switch_menu(&mut self, new_index: usize) {
        let was_dropdown_open = self.dropdown_open;
        self.selected_menu_index = new_index;
        self.selected_item_index = 0;
        self.file_section_scroll_offset = 0;
        self.dropdown_open = was_dropdown_open;
        self.needs_redraw = true;
    }

    /// Move to next item in current menu (skip separators)
    pub(crate) fn next_item(&mut self) {
        let menu = &self.menus[self.selected_menu_index];
        self.selected_item_index = self.find_next_non_separator(
            self.selected_item_index,
            menu.items.len(),
            true
        );
        self.ensure_selected_visible();
        self.needs_redraw = true;
    }

    /// Move to previous item in current menu (skip separators)
    pub(crate) fn prev_item(&mut self) {
        let menu = &self.menus[self.selected_menu_index];
        self.selected_item_index = self.find_next_non_separator(
            self.selected_item_index,
            menu.items.len(),
            false
        );
        self.ensure_selected_visible();
        self.needs_redraw = true;
    }

    /// Find next/previous non-separator item index
    fn find_next_non_separator(&self, current: usize, len: usize, forward: bool) -> usize {
        let menu = &self.menus[self.selected_menu_index];
        let mut next = if forward {
            (current + 1) % len
        } else if current == 0 {
            len - 1
        } else {
            current - 1
        };

        let start = next;
        // Skip separators
        while matches!(menu.items[next], MenuItem::Separator) {
            next = if forward {
                (next + 1) % len
            } else if next == 0 {
                len - 1
            } else {
                next - 1
            };
            if next == start {
                break; // All items are separators (shouldn't happen)
            }
        }
        next
    }

    /// Ensure selected item is visible by adjusting scroll offset (for file section only)
    fn ensure_selected_visible(&mut self) {
        // Only adjust scroll for file section items in File menu
        if self.selected_menu_index != FILE_MENU_INDEX || self.selected_item_index < FILE_SECTION_START_IDX {
            return;
        }

        let (file_count, file_end_idx) = self.get_file_section_bounds();

        // If selected item is beyond the file section, don't scroll
        if self.selected_item_index >= file_end_idx {
            return;
        }

        // If we have fewer files than max, no scrolling needed
        if file_count <= self.max_visible_files {
            self.file_section_scroll_offset = 0;
            return;
        }

        // Calculate file index within the file section (0-based)
        let file_idx = self.selected_item_index - FILE_SECTION_START_IDX;

        // Adjust scroll if selected file is above visible area
        if file_idx < self.file_section_scroll_offset {
            self.file_section_scroll_offset = file_idx;
        }

        // Adjust scroll if selected file is below visible area
        if file_idx >= self.file_section_scroll_offset + self.max_visible_files {
            self.file_section_scroll_offset = file_idx - self.max_visible_files + 1;
        }
    }

    /// Get file section boundaries in File menu
    /// Returns (file_count, file_end_idx)
    fn get_file_section_bounds(&self) -> (usize, usize) {
        let menu = &self.menus[FILE_MENU_INDEX];
        let mut file_count = 0;
        let mut file_end_idx = FILE_SECTION_START_IDX;

        for idx in FILE_SECTION_START_IDX..menu.items.len() {
            if matches!(menu.items[idx], MenuItem::Separator) {
                file_end_idx = idx;
                break;
            }
            file_count += 1;
        }

        // If we didn't find a separator, all remaining items are files
        if file_end_idx == FILE_SECTION_START_IDX && file_count > 0 {
            file_end_idx = menu.items.len();
        }

        (file_count, file_end_idx)
    }

    /// Get currently selected menu action (if any)
    pub(crate) fn get_selected_action(&self) -> Option<MenuAction> {
        let menu = &self.menus[self.selected_menu_index];
        match &menu.items[self.selected_item_index] {
            MenuItem::Action { action, .. } => Some(*action),
            MenuItem::Checkable { action, .. } => Some(*action),
            MenuItem::Separator => None,
        }
    }

    /// Update checkable item state (e.g., for line wrap toggle)
    pub(crate) fn update_checkable(&mut self, action: MenuAction, checked: bool) {
        for menu in &mut self.menus {
            for item in &mut menu.items {
                if let MenuItem::Checkable { action: item_action, checked: item_checked, .. } = item {
                    if *item_action == action {
                        *item_checked = checked;
                    }
                }
            }
        }
    }

    /// Update max visible files setting (called when settings are loaded)
    pub(crate) fn update_max_visible_files(&mut self, max_visible_files: usize) {
        self.max_visible_files = max_visible_files;
    }

    /// Update File menu with current tracked files
    #[allow(dead_code)] // Only used in ui.rs (binary)
    pub(crate) fn update_file_menu(&mut self, max_files: usize, current_file: &str, is_current_modified: bool, is_current_read_only: bool) {
        let files = crate::recent::get_recent_files().unwrap_or_default();
        // Show all files for scrolling support (max_files parameter kept for API compatibility)
        let _ = max_files; // Suppress unused warning

        let current_canonical = std::path::PathBuf::from(current_file)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(current_file));

        let file_labels = Self::create_file_labels(
            &files,
            files.len(), // Show all files
            &current_canonical,
            is_current_modified,
            is_current_read_only,
        );

        let items = Self::build_file_menu_items(file_labels, false); // Never show "..."
        self.menus[0] = Menu::new("File", 'f', items);
        self.needs_redraw = true;
    }

    /// Create labeled list of files with unsaved markers
    fn create_file_labels(
        files: &[std::path::PathBuf],
        count: usize,
        current_canonical: &std::path::Path,
        is_current_modified: bool,
        is_current_read_only: bool,
    ) -> Vec<String> {
        files
            .iter()
            .take(count)
            .map(|file| {
                let path = std::path::PathBuf::from(file);
                let is_current = path == current_canonical;
                let is_modified = if is_current {
                    is_current_modified
                } else {
                    check_file_has_unsaved_changes(&path)
                };
                let is_read_only = if is_current {
                    is_current_read_only
                } else {
                    path.exists() && std::fs::OpenOptions::new().write(true).open(&path).is_err()
                };

                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_else(|| file.to_str().unwrap_or("???"));

                if is_read_only {
                    format!("⚿ {}", filename)
                } else if is_modified {
                    format!("* {}", filename)
                } else {
                    filename.to_string()
                }
            })
            .collect()
    }

    /// Build complete File menu items including static items and recent files
    fn build_file_menu_items(file_labels: Vec<String>, _show_more: bool) -> Vec<MenuItem> {
        let mut items = vec![
            action("New", MenuAction::FileNew),
            action("Open...", MenuAction::FileOpenDialog),
            action("Save", MenuAction::FileSave),
            action("Close", MenuAction::FileClose),
            action("Close all", MenuAction::FileCloseAll),
        ];

        if !file_labels.is_empty() {
            items.push(MenuItem::Separator);

            for (idx, label) in file_labels.iter().enumerate() {
                items.push(action(label, MenuAction::FileOpenRecent(idx)));
            }
        }

        items.push(MenuItem::Separator);
        items.push(action("Quit", MenuAction::FileQuit));

        items
    }

    /// Try to activate menu by Alt+hotkey
    pub(crate) fn try_activate_by_hotkey(&mut self, key: char) -> bool {
        let key_lower = key.to_lowercase().next().unwrap_or(key);
        for (idx, menu) in self.menus.iter().enumerate() {
            if menu.hotkey == key_lower {
                self.active = true;
                self.selected_menu_index = idx;
                self.selected_item_index = 0;
                return true;
            }
        }
        false
    }


    /// Jump to previous section (Ctrl+Up) - finds previous separator and selects last item in that section
    pub(crate) fn jump_to_prev_section(&mut self) {
        let menu = &self.menus[self.selected_menu_index];

        if self.selected_item_index == 0 {
            return;
        }

        // Move backwards to find separator before current section
        let mut idx = self.selected_item_index;
        while idx > 0 {
            idx -= 1;
            if matches!(menu.items[idx], MenuItem::Separator) {
                // Found separator, now find last non-separator item before it
                if idx == 0 {
                    return;
                }
                idx -= 1;
                // Skip any additional consecutive separators
                while idx > 0 && matches!(menu.items[idx], MenuItem::Separator) {
                    idx -= 1;
                }
                self.selected_item_index = idx;
                self.adjust_scroll_for_file_section(idx);
                self.needs_redraw = true;
                return;
            }
        }

        // No separator found, we're in the first section, stay at first non-separator item
        self.selected_item_index = self.find_next_non_separator(0, menu.items.len(), true);
        self.needs_redraw = true;
    }

    /// Jump to next section (Ctrl+Down) - finds next separator and selects first item in that section
    pub(crate) fn jump_to_next_section(&mut self) {
        let menu = &self.menus[self.selected_menu_index];

        // Start from current position and search forward for separator
        let mut idx = self.selected_item_index + 1;

        while idx < menu.items.len() {
            if matches!(menu.items[idx], MenuItem::Separator) {
                // Found separator, move to first non-separator after it
                idx += 1;
                while idx < menu.items.len() && matches!(menu.items[idx], MenuItem::Separator) {
                    idx += 1;
                }
                if idx < menu.items.len() {
                    self.selected_item_index = idx;
                    self.adjust_scroll_for_file_section(idx);
                    self.needs_redraw = true;
                }
                return;
            }
            idx += 1;
        }
    }

    /// Adjust scroll offset if the index is in the file section
    fn adjust_scroll_for_file_section(&mut self, idx: usize) {
        if self.selected_menu_index != FILE_MENU_INDEX || idx < FILE_SECTION_START_IDX {
            return;
        }

        let (_, file_end_idx) = self.get_file_section_bounds();

        // Check if we're within the file section (not at separator/quit)
        if idx >= file_end_idx {
            return;
        }

        // We're in file section, ensure visibility
        self.ensure_selected_visible();
    }
}


/// Render dropdown menu for currently selected menu (below the header)
pub(crate) fn render_dropdown_menu(
    stdout: &mut impl Write,
    menu_bar: &MenuBar,
    state: &crate::editor_state::FileViewerState,
    lines: &[String],
) -> Result<(), std::io::Error> {
    use crossterm::style::Color;

    if !menu_bar.active || !menu_bar.dropdown_open {
        return Ok(());
    }

    let menu = &menu_bar.menus[menu_bar.selected_menu_index];

    // Calculate menu horizontal position
    let line_num_width = crate::coordinates::line_number_display_width(state.settings, lines.len()) as usize;
    let mut menu_x = line_num_width + 2;
    for i in 0..menu_bar.selected_menu_index {
        menu_x += menu_bar.menus[i].label.len() + 2;
    }

    // Find longest item label for menu width
    let mut max_width = menu.label.len();
    for item in &menu.items {
        let width = match item {
            MenuItem::Action { label, .. } | MenuItem::Checkable { label, .. } => label.len() + 4,
            MenuItem::Separator => 3,
        };
        if width > max_width {
            max_width = width;
        }
    }
    max_width += 4;

    // Get colors
    let menu_bg_color = crate::settings::Settings::parse_color(&state.settings.appearance.header_bg)
        .unwrap_or(Color::DarkBlue);
    let selection_color = Color::Rgb { r: 100, g: 149, b: 237 };

    // For File menu, separate items into sections
    if menu_bar.selected_menu_index == FILE_MENU_INDEX {
        let max_visible_files = state.settings.max_menu_files;

        // Find file section boundaries.
        // Default to menu end so that when there are no files we don't re-render the
        // trailing Separator+Quit items a second time in the "bottom section" loop.
        let mut file_end_idx = menu.items.len();
        let mut files = Vec::new();

        for (idx, item) in menu.items.iter().enumerate() {
            if idx >= FILE_SECTION_START_IDX {
                if matches!(item, MenuItem::Separator) {
                    file_end_idx = idx;
                    break;
                }
                // Only treat FileOpenRecent actions as file entries; any other item
                // (e.g., Quit when no files are present) marks the end of the section.
                if matches!(item, MenuItem::Action { action: MenuAction::FileOpenRecent(_), .. }) {
                    files.push((idx, item));
                } else {
                    file_end_idx = idx;
                    break;
                }
            }
        }

        let total_files = files.len();
        let scroll_offset = menu_bar.file_section_scroll_offset;

        // Render static items at top (New, Open, Save, Close, Close all, Separator)
        let mut display_row = 1;
        for idx in 0..FILE_SECTION_START_IDX.min(menu.items.len()) {
            let item = &menu.items[idx];
            render_menu_item_at_row(
                stdout,
                item,
                idx == menu_bar.selected_item_index,
                menu_x,
                display_row,
                max_width,
                menu_bg_color,
                selection_color,
            )?;
            display_row += 1;
        }

        // Render visible files with scrolling (always render max_visible_files rows)
        let actual_visible_files = max_visible_files.min(total_files);
        let show_scrollbar = total_files > max_visible_files;

        if total_files > 0 {
            let visible_start = scroll_offset.min(total_files.saturating_sub(max_visible_files));
            let visible_end = (visible_start + actual_visible_files).min(total_files);

            for file_idx in visible_start..visible_end {
                if let Some((idx, item)) = files.get(file_idx) {
                    render_menu_item_at_row(
                        stdout,
                        item,
                        *idx == menu_bar.selected_item_index,
                        menu_x,
                        display_row,
                        max_width,
                        menu_bg_color,
                        selection_color,
                    )?;

                    // Render scrollbar for this row if needed
                    if show_scrollbar {
                        render_file_scrollbar_row(
                            stdout,
                            file_idx - visible_start,
                            visible_end - visible_start,
                            scroll_offset,
                            total_files,
                            max_visible_files,
                            menu_x + max_width - 1,
                            display_row,
                        )?;
                    }

                    display_row += 1;
                }
            }
        }

        // Render remaining items at bottom (Separator, Quit)
        for idx in file_end_idx..menu.items.len() {
            let item = &menu.items[idx];
            render_menu_item_at_row(
                stdout,
                item,
                idx == menu_bar.selected_item_index,
                menu_x,
                display_row,
                max_width,
                menu_bg_color,
                selection_color,
            )?;
            display_row += 1;
        }
    } else {
        // Other menus: render all items normally
        let mut display_row = 1;
        for (idx, item) in menu.items.iter().enumerate() {
            render_menu_item_at_row(
                stdout,
                item,
                idx == menu_bar.selected_item_index,
                menu_x,
                display_row,
                max_width,
                menu_bg_color,
                selection_color,
            )?;
            display_row += 1;
        }
    }

    Ok(())
}

/// Render a single menu item at a specific position with selection highlighting
fn render_menu_item_at_row(
    stdout: &mut impl Write,
    item: &MenuItem,
    is_selected: bool,
    x: usize,
    y: u16,
    max_width: usize,
    bg_color: crossterm::style::Color,
    selection_color: crossterm::style::Color,
) -> Result<(), std::io::Error> {
    use crossterm::{cursor::MoveTo, execute, style::{Color, ResetColor, SetBackgroundColor, SetForegroundColor}};

    execute!(stdout, MoveTo(x as u16, y))?;

    if is_selected {
        execute!(stdout, SetBackgroundColor(selection_color))?;
        execute!(stdout, SetForegroundColor(Color::White))?;
    } else {
        execute!(stdout, SetBackgroundColor(bg_color))?;
    }

    render_menu_item(stdout, item, max_width)?;
    execute!(stdout, ResetColor)?;


    Ok(())
}

/// Render a single menu item
fn render_menu_item(
    stdout: &mut impl Write,
    item: &MenuItem,
    max_width: usize,
) -> Result<(), std::io::Error> {
    use crossterm::{execute, style::Print};

    match item {
        MenuItem::Action { label, .. } => {
            let text = format!(" {:<width$} ", label, width = max_width - 2);
            execute!(stdout, Print(text))?;
        }
        MenuItem::Checkable { label, checked, .. } => {
            let check = if *checked { "✓" } else { " " };
            let text = format!(" [{}] {:<width$} ", check, label, width = max_width - 6);
            execute!(stdout, Print(text))?;
        }
        MenuItem::Separator => {
            let text = format!(" {:<width$} ", "─".repeat(max_width - 2), width = max_width - 2);
            execute!(stdout, Print(text))?;
        }
    }
    Ok(())
}

/// Render one row of the file section scrollbar
fn render_file_scrollbar_row(
    stdout: &mut impl Write,
    display_row: usize,
    _visible_count: usize,
    scroll_offset: usize,
    total_files: usize,
    max_visible: usize,
    x: usize,
    y: u16,
) -> Result<(), std::io::Error> {
    use crossterm::{cursor::MoveTo, execute, style::{Color, Print, SetBackgroundColor, SetForegroundColor, ResetColor}};

    // Calculate scrollbar position and size
    let scrollbar_height = max_visible;
    let bar_size = ((max_visible as f64 / total_files as f64) * scrollbar_height as f64).max(1.0) as usize;
    let bar_position = ((scroll_offset as f64 / total_files as f64) * scrollbar_height as f64) as usize;

    let track_color = Color::Rgb { r: 50, g: 50, b: 50 };
    let bar_color = Color::Rgb { r: 100, g: 149, b: 237 };

    execute!(stdout, MoveTo(x as u16, y))?;

    if display_row >= bar_position && display_row < bar_position + bar_size {
        // Scrollbar bar
        execute!(stdout, SetBackgroundColor(bar_color))?;
        execute!(stdout, SetForegroundColor(bar_color))?;
        execute!(stdout, Print("█"))?;
    } else {
        // Scrollbar track
        execute!(stdout, SetBackgroundColor(track_color))?;
        execute!(stdout, SetForegroundColor(track_color))?;
        execute!(stdout, Print("░"))?;
    }
    execute!(stdout, ResetColor)?;

    Ok(())
}


/// Handle keyboard input for menu system
/// Returns (Option<action>, needs_full_redraw)
/// - action: Some if an action was selected, None otherwise
/// - needs_full_redraw: true if full screen redraw needed (open/close), false if only menu overlay needs update
pub(crate) fn handle_menu_key(
    menu_bar: &mut MenuBar,
    key_event: KeyEvent,
) -> (Option<MenuAction>, bool) {
    let code = key_event.code;
    let modifiers = key_event.modifiers;

    // Alt+letter opens specific menu (still supported)
    if modifiers.contains(KeyModifiers::ALT) {
        if let KeyCode::Char(c) = code {
            if menu_bar.try_activate_by_hotkey(c) {
                return (None, true); // Menu opened, needs full redraw
            }
        }
        // Don't handle other Alt+ combinations here (like Alt+Arrow for block selection)
        // Those should be handled by the normal editor logic
    }

    // Esc opens menu (File menu with dropdown) or closes it if already open
    if code == KeyCode::Esc && !modifiers.contains(KeyModifiers::ALT) && !modifiers.contains(KeyModifiers::CONTROL) {
        if menu_bar.active {
            // Close menu if already open
            menu_bar.close();
            return (None, true); // Menu closed, needs full redraw
        } else {
            // Open menu on File menu with dropdown
            menu_bar.active = true;
            menu_bar.selected_menu_index = FILE_MENU_INDEX;

            // Select second recent file if available, for quick Esc+Enter switching
            let file_menu = &menu_bar.menus[FILE_MENU_INDEX];

            // Count actual files in file section
            let file_count = count_files_in_menu(file_menu);

            menu_bar.selected_item_index = if file_count >= 2 {
                FILE_SECTION_START_IDX + 1 // Select second file
            } else if file_count >= 1 {
                FILE_SECTION_START_IDX // Select first (only) file
            } else {
                0 // No files, select New
            };

            menu_bar.dropdown_open = true;
            return (None, true); // Menu opened, needs full redraw
        }
    }

    if !menu_bar.active {
        return (None, false);
    }

    // Handle Ctrl+W to remove file from menu (only when dropdown is open and on File menu)
    if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('w') {
        if menu_bar.dropdown_open && menu_bar.selected_menu_index == FILE_MENU_INDEX {
            // Check if we're on a recent file item
            if menu_bar.selected_item_index >= FILE_SECTION_START_IDX {
                let menu = &menu_bar.menus[FILE_MENU_INDEX];
                if let Some(MenuItem::Action { action: MenuAction::FileOpenRecent(idx), .. })
                    = menu.items.get(menu_bar.selected_item_index) {
                    return (Some(MenuAction::FileRemove(*idx)), false);
                }
            }
        }
    }

    // Get terminal height for scroll calculations
    let (_, term_height) = crossterm::terminal::size().unwrap_or((80, 24));
    let _ = term_height; // Suppress unused warning for now

    match code {
        KeyCode::Left => {
            menu_bar.prev_menu();
            (None, true) // Menu switched, needs full redraw (header + dropdown area)
        }
        KeyCode::Right => {
            menu_bar.next_menu();
            (None, true) // Menu switched, needs full redraw (header + dropdown area)
        }
        KeyCode::Down => {
            if modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+Down: Jump to next section
                if menu_bar.dropdown_open {
                    menu_bar.jump_to_next_section();
                    return (None, false);
                }
            } else if menu_bar.dropdown_open {
                // Normal Down: Move selection
                menu_bar.next_item();
                return (None, false);
            } else {
                // Open dropdown when Down is pressed on menu bar
                menu_bar.open_dropdown();
                return (None, true);
            }
            (None, false)
        }
        KeyCode::Up => {
            if modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+Up: Jump to previous section
                if menu_bar.dropdown_open {
                    menu_bar.jump_to_prev_section();
                    return (None, false);
                }
            } else if menu_bar.dropdown_open {
                // Normal Up: Move selection
                menu_bar.prev_item();
                return (None, false);
            }
            (None, false)
        }
        KeyCode::Enter => {
            if menu_bar.dropdown_open {
                // Select current item
                let action = menu_bar.get_selected_action();
                menu_bar.close();
                (action, true) // Menu closed and action selected, needs full redraw
            } else {
                // Open dropdown
                menu_bar.open_dropdown();
                (None, true) // Dropdown opened, needs full redraw
            }
        }
        _ => (None, false),
    }
}

/// Handle mouse input for menu system
/// Returns (Option<action>, needs_full_redraw)
/// - action: Some if an action was selected, None otherwise
/// - needs_full_redraw: true if menu state changed (open/close), false if only hover/navigation
pub(crate) fn handle_menu_mouse(
    menu_bar: &mut MenuBar,
    mouse_event: MouseEvent,
    line_number_width: u16,
) -> (Option<MenuAction>, bool) {
    let col = mouse_event.column as usize;
    let row = mouse_event.row as usize;

    match mouse_event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            handle_menu_left_click(menu_bar, col, row, line_number_width)
        }
        MouseEventKind::Moved if menu_bar.active && menu_bar.dropdown_open => {
            handle_dropdown_hover(menu_bar, row)
        }
        _ => (None, false),
    }
}

/// Handle left click on menu bar or dropdown items
fn handle_menu_left_click(
    menu_bar: &mut MenuBar,
    col: usize,
    row: usize,
    line_number_width: u16,
) -> (Option<MenuAction>, bool) {
    let burger_start = line_number_width as usize;
    let burger_end = burger_start + 2; // "≡ " is 2 characters wide

    // Check burger icon click
    if row == 0 && col >= burger_start && col < burger_end {
        if menu_bar.active {
            menu_bar.close();
        } else {
            menu_bar.open();
        }
        return (None, true);
    }

    // Check menu label clicks
    if row == 0 && menu_bar.active {
        return handle_menu_label_click(menu_bar, col, burger_end);
    }

    // Check dropdown item clicks
    if menu_bar.active && menu_bar.dropdown_open && row > 0 {
        return handle_dropdown_item_click(menu_bar, row);
    }

    (None, false)
}

/// Handle click on menu labels in the menu bar
fn handle_menu_label_click(
    menu_bar: &mut MenuBar,
    col: usize,
    start_x: usize,
) -> (Option<MenuAction>, bool) {
    let mut x = start_x;

    for (idx, menu) in menu_bar.menus.iter().enumerate() {
        // Include both the label and the trailing 2 spaces in the clickable region
        if col >= x - 1 && col < x + menu.label.len() + 1 {
            if menu_bar.selected_menu_index == idx {
                // Toggle dropdown on same menu
                if menu_bar.dropdown_open {
                    menu_bar.close_dropdown();
                } else {
                    menu_bar.open_dropdown();
                }
            } else {
                // Switch to different menu and open dropdown
                menu_bar.selected_menu_index = idx;
                menu_bar.open_dropdown();
            }
            return (None, true);
        }
        x += menu.label.len() + 2; // Menu label length + 2 spaces (matching rendering)
    }

    // Clicked outside menu labels - close menu
    menu_bar.close();
    (None, true)
}

/// Handle click on dropdown menu item
fn handle_dropdown_item_click(
    menu_bar: &mut MenuBar,
    row: usize,
) -> (Option<MenuAction>, bool) {
    let menu = &menu_bar.menus[menu_bar.selected_menu_index];

    if row - 1 < menu.items.len() {
        let item_idx = row - 1;
        if !matches!(menu.items[item_idx], MenuItem::Separator) {
            menu_bar.selected_item_index = item_idx;
            let action = menu_bar.get_selected_action();
            menu_bar.close();
            return (action, true);
        }
    }

    (None, false)
}

/// Handle mouse hover over dropdown items
fn handle_dropdown_hover(
    menu_bar: &mut MenuBar,
    row: usize,
) -> (Option<MenuAction>, bool) {
    if row > 0 {
        let menu = &menu_bar.menus[menu_bar.selected_menu_index];
        if row - 1 < menu.items.len() {
            let item_idx = row - 1;
            if !matches!(menu.items[item_idx], MenuItem::Separator) {
                if menu_bar.selected_item_index != item_idx {
                    menu_bar.selected_item_index = item_idx;
                    return (None, false); // Hover changed, only menu needs redraw
                }
            }
        }
    }
    (None, false)
}

/// Check if a point (column, row) is within the dropdown menu bounds
/// Returns true if the dropdown is open and the point is inside it
pub(crate) fn is_point_in_dropdown(
    menu_bar: &MenuBar,
    col: usize,
    row: usize,
    line_number_width: u16,
) -> bool {
    if !menu_bar.active || !menu_bar.dropdown_open {
        return false;
    }

    let menu = &menu_bar.menus[menu_bar.selected_menu_index];

    // Calculate menu horizontal position (same logic as render_dropdown_menu)
    let line_num_width = line_number_width as usize;
    let mut menu_x = line_num_width + 2; // line numbers + burger icon "≡ "
    for i in 0..menu_bar.selected_menu_index {
        menu_x += menu_bar.menus[i].label.len() + 2; // Menu label length + 2 spaces (matching rendering)
    }

    // Find longest item label for menu width
    let mut max_width = menu.label.len();
    for item in &menu.items {
        let width = match item {
            MenuItem::Action { label, .. } | MenuItem::Checkable { label, .. } => label.len() + 4,
            MenuItem::Separator => 3,
        };
        if width > max_width {
            max_width = width;
        }
    }
    max_width += 4; // Padding

    // Dropdown starts at row 1 and extends for menu.items.len() rows
    let dropdown_start_row = 1;
    let dropdown_end_row = 1 + menu.items.len();

    // Check if point is within bounds
    row >= dropdown_start_row
        && row < dropdown_end_row
        && col >= menu_x
        && col < menu_x + max_width
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_bar_creation() {
        let menu_bar = MenuBar::new();
        assert_eq!(menu_bar.menus.len(), 4);
        assert_eq!(menu_bar.menus[0].label, "File");
        assert_eq!(menu_bar.menus[1].label, "Edit");
        assert_eq!(menu_bar.menus[2].label, "View");
        assert_eq!(menu_bar.menus[3].label, "Help");
    }

    #[test]
    fn test_menu_activation_by_hotkey() {
        let mut menu_bar = MenuBar::new();
        assert!(!menu_bar.active);

        assert!(menu_bar.try_activate_by_hotkey('f'));
        assert!(menu_bar.active);
        assert_eq!(menu_bar.selected_menu_index, 0);

        menu_bar.close();
        assert!(menu_bar.try_activate_by_hotkey('e'));
        assert_eq!(menu_bar.selected_menu_index, 1);
    }

    #[test]
    fn test_menu_navigation() {
        let mut menu_bar = MenuBar::new();
        menu_bar.open();

        assert_eq!(menu_bar.selected_menu_index, 0);
        menu_bar.next_menu();
        assert_eq!(menu_bar.selected_menu_index, 1);

        menu_bar.prev_menu();
        assert_eq!(menu_bar.selected_menu_index, 0);
    }

    #[test]
    fn test_item_navigation_skips_separators() {
        let mut menu_bar = MenuBar::new();
        menu_bar.open();

        // File menu has a separator before Quit
        menu_bar.selected_item_index = 0;
        for _ in 0..5 {
            menu_bar.next_item();
        }
        // Should skip separator
        if let MenuItem::Separator = menu_bar.menus[0].items[menu_bar.selected_item_index] {
            panic!("Should not land on separator");
        }
    }

    #[test]
    fn test_checkable_update() {
        let mut menu_bar = MenuBar::new();
        menu_bar.update_checkable(MenuAction::ViewLineWrap, true);

        // Find the line wrap item
        for menu in &menu_bar.menus {
            for item in &menu.items {
                if let MenuItem::Checkable { action, checked, .. } = item {
                    if *action == MenuAction::ViewLineWrap {
                        assert!(checked);
                    }
                }
            }
        }
    }

    #[test]
    fn test_esc_opens_menu_when_inactive() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut menu_bar = MenuBar::new();
        assert!(!menu_bar.active);

        // Press Esc - should open menu
        let key_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
        let (action, needs_redraw) = handle_menu_key(&mut menu_bar, key_event);

        assert!(menu_bar.active, "Menu should be active");
        assert!(menu_bar.dropdown_open, "Dropdown should be open");
        assert_eq!(menu_bar.selected_menu_index, 0, "File menu should be selected");
        assert!(action.is_none(), "No action should be returned");
        assert!(needs_redraw, "Should need redraw");
    }

    #[test]
    fn test_esc_closes_menu_when_active() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut menu_bar = MenuBar::new();
        menu_bar.open();
        menu_bar.open_dropdown();
        assert!(menu_bar.active);
        assert!(menu_bar.dropdown_open);

        // Press Esc - should close menu
        let key_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
        let (action, needs_redraw) = handle_menu_key(&mut menu_bar, key_event);

        assert!(!menu_bar.active, "Menu should be inactive");
        assert!(!menu_bar.dropdown_open, "Dropdown should be closed");
        assert!(action.is_none(), "No action should be returned");
        assert!(needs_redraw, "Should need redraw");
    }

    #[test]
    fn test_left_right_switch_menus_with_dropdown_open() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut menu_bar = MenuBar::new();
        menu_bar.open();
        menu_bar.open_dropdown();
        assert_eq!(menu_bar.selected_menu_index, 0);
        assert!(menu_bar.dropdown_open);

        // Press Right - should switch to next menu with dropdown still open
        let key_event = KeyEvent::new(KeyCode::Right, KeyModifiers::empty());
        let (action, _) = handle_menu_key(&mut menu_bar, key_event);

        assert_eq!(menu_bar.selected_menu_index, 1, "Should move to Edit menu");
        assert!(menu_bar.dropdown_open, "Dropdown should stay open");
        assert!(action.is_none());

        // Press Left - should go back to File menu
        let key_event = KeyEvent::new(KeyCode::Left, KeyModifiers::empty());
        let (action, _) = handle_menu_key(&mut menu_bar, key_event);

        assert_eq!(menu_bar.selected_menu_index, 0, "Should be back to File menu");
        assert!(menu_bar.dropdown_open, "Dropdown should stay open");
        assert!(action.is_none());
    }

    #[test]
    fn test_update_file_menu_detects_unsaved_changes_for_all_files() {
        use std::fs;
        use crate::env::set_temp_home;
        use crate::undo::UndoHistory;

        let (tmp, _guard) = set_temp_home();

        // Create test files
        let file1 = tmp.path().join("file1.txt");
        let file2 = tmp.path().join("file2.txt");
        let file3 = tmp.path().join("file3.txt");

        fs::write(&file1, "content1").unwrap();
        fs::write(&file2, "content2").unwrap();
        fs::write(&file3, "content3").unwrap();

        // Add files to recent list
        crate::recent::update_recent_file(file1.to_str().unwrap()).unwrap();
        crate::recent::update_recent_file(file2.to_str().unwrap()).unwrap();
        crate::recent::update_recent_file(file3.to_str().unwrap()).unwrap();

        // Mark file2 as modified in its undo history
        let mut history2 = UndoHistory::new();
        history2.modified = true;
        history2.save(file2.to_str().unwrap()).unwrap();

        // Mark file3 as not modified
        let mut history3 = UndoHistory::new();
        history3.modified = false;
        history3.save(file3.to_str().unwrap()).unwrap();

        // Update menu (file1 is "current" and not modified)
        let mut menu_bar = MenuBar::new();
        menu_bar.update_file_menu(5, file1.to_str().unwrap(), false, false);

        // Find the File menu items
        let file_menu = &menu_bar.menus[0];
        let mut file2_has_marker = false;
        let mut file3_no_marker = false;

        for item in &file_menu.items {
            if let MenuItem::Action { label, .. } = item {
                if label.contains("file2.txt") {
                    file2_has_marker = label.starts_with('*');
                } else if label.contains("file3.txt") {
                    file3_no_marker = !label.starts_with('*');
                }
            }
        }

        assert!(file2_has_marker, "file2 should have unsaved marker (*)");
        assert!(file3_no_marker, "file3 should NOT have unsaved marker");
    }

    #[test]
    fn test_update_file_menu_shows_ellipsis_when_too_many_files() {
        use std::fs;
        use crate::env::set_temp_home;

        let (tmp, _guard) = set_temp_home();

        // Create more files than max_files
        for i in 1..=7 {
            let file = tmp.path().join(format!("file{}.txt", i));
            fs::write(&file, "content").unwrap();
            crate::recent::update_recent_file(file.to_str().unwrap()).unwrap();
        }

        let file1 = tmp.path().join("file1.txt");
        let mut menu_bar = MenuBar::new();
        menu_bar.update_file_menu(5, file1.to_str().unwrap(), false, false);

        let file_menu = &menu_bar.menus[0];
        let mut found_ellipsis = false;
        let mut file_count = 0;

        for item in &file_menu.items {
            if let MenuItem::Action { label, .. } = item {
                if label == "..." {
                    found_ellipsis = true;
                }
                if label.contains("file") && label.contains(".txt") {
                    file_count += 1;
                }
            }
        }

        assert!(!found_ellipsis, "Should NOT show '...' - all files shown with scrolling support");
        assert_eq!(file_count, 7, "Should show all 7 files for scrolling");
    }

    #[test]
    fn test_down_key_opens_dropdown_when_menu_active() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut menu_bar = MenuBar::new();
        menu_bar.open(); // Open menu bar but not dropdown
        assert!(menu_bar.active);
        assert!(!menu_bar.dropdown_open);

        let key_event = KeyEvent::new(KeyCode::Down, KeyModifiers::empty());
        let (action, needs_redraw) = handle_menu_key(&mut menu_bar, key_event);

        assert!(menu_bar.dropdown_open, "Down should open dropdown");
        assert!(action.is_none());
        assert!(needs_redraw);
    }

    #[test]
    fn test_enter_key_selects_menu_item() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut menu_bar = MenuBar::new();
        menu_bar.open();
        menu_bar.open_dropdown();
        menu_bar.selected_item_index = 0; // New action

        let key_event = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
        let (action, needs_redraw) = handle_menu_key(&mut menu_bar, key_event);

        assert!(action.is_some(), "Enter should select an action");
        assert_eq!(action.unwrap(), MenuAction::FileNew);
        assert!(!menu_bar.active, "Menu should close after selection");
        assert!(needs_redraw);
    }

    #[test]
    fn test_up_down_navigation_skips_separators() {
        let mut menu_bar = MenuBar::new();
        menu_bar.open_dropdown();

        // File menu: New, Open..., Save, Close, Close all, [Separator], Quit
        menu_bar.selected_item_index = 4; // Close all
        menu_bar.next_item(); // Should skip separator and go to Quit

        let item = &menu_bar.menus[0].items[menu_bar.selected_item_index];
        if let MenuItem::Action { label, .. } = item {
            assert_eq!(label, "Quit", "Should skip separator");
        } else {
            panic!("Should be on an action item, not separator");
        }
    }
}


