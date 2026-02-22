use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::io::Write;
use std::path::Path;

/// Check if a file has unsaved changes by reading its undo history.
pub(crate) fn check_file_has_unsaved_changes(file_path: &Path) -> bool {
    let file_str = file_path.to_string_lossy();
    let Ok(undo_path) = crate::undo::UndoHistory::history_path_for(&file_str) else {
        return false;
    };
    if !undo_path.exists() {
        return false;
    }
    let Ok(content) = std::fs::read_to_string(&undo_path) else {
        return false;
    };
    serde_json::from_str::<crate::undo::UndoHistory>(&content)
        .map(|h| h.modified)
        .unwrap_or(false)
}

/// Menu item types.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MenuItem {
    /// A selectable action with a label.
    Action { label: String, action: MenuAction },
    /// A toggleable item with a checkmark.
    Checkable { label: String, action: MenuAction, checked: bool },
    /// A visual divider between groups of items.
    Separator,
}

/// Actions that can be triggered from the menu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum MenuAction {
    // File menu
    FileNew,
    FileOpenDialog,
    #[allow(dead_code)] // Used in ui.rs (binary)
    FileOpenRecent(usize),
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
    // Internal
    FileRemove(usize), // Remove file at index from recent files (Ctrl+W)
}

/// A single drop-down menu with a label and list of items.
#[derive(Debug, Clone)]
pub(crate) struct Menu {
    pub(crate) label: String,
    pub(crate) items: Vec<MenuItem>,
    pub(crate) hotkey: char,
}

impl Menu {
    pub(crate) fn new(label: &str, hotkey: char, items: Vec<MenuItem>) -> Self {
        Self { label: label.to_string(), items, hotkey }
    }
}

// File menu layout constants.
const FILE_MENU_INDEX: usize = 0;
// Static items: New, Open, Save, Close, Close all, Separator — files start after these.
const FILE_SECTION_START_IDX: usize = 6;

/// Helper to create an action menu item.
fn action(label: &str, action: MenuAction) -> MenuItem {
    MenuItem::Action { label: label.to_string(), action }
}

/// Helper to create a checkable menu item.
fn checkable(label: &str, action: MenuAction, checked: bool) -> MenuItem {
    MenuItem::Checkable { label: label.to_string(), action, checked }
}

/// Count file entries in the file section of the File menu.
fn count_files_in_menu(menu: &Menu) -> usize {
    menu.items
        .iter()
        .skip(FILE_SECTION_START_IDX)
        .take_while(|item| !matches!(item, MenuItem::Separator))
        .filter(|item| matches!(item, MenuItem::Action { .. }))
        .count()
}

/// Calculate the rendered width of a dropdown menu.
fn menu_display_width(menu: &Menu) -> usize {
    let max_label_width = menu
        .items
        .iter()
        .map(|item| match item {
            MenuItem::Action { label, .. } | MenuItem::Checkable { label, .. } => label.len() + 4,
            MenuItem::Separator => 3,
        })
        .max()
        .unwrap_or(0)
        .max(menu.label.len());
    max_label_width + 4
}

/// The menu bar state, owning all menus and interaction state.
#[derive(Debug)]
pub(crate) struct MenuBar {
    pub(crate) menus: Vec<Menu>,
    pub(crate) active: bool,
    pub(crate) dropdown_open: bool,
    pub(crate) selected_menu_index: usize,
    pub(crate) selected_item_index: usize,
    pub(crate) file_section_scroll_offset: usize,
    pub(crate) needs_redraw: bool,
    pub(crate) max_visible_files: usize,
}

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
                vec![checkable("Line Wrap", MenuAction::ViewLineWrap, false)],
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
            max_visible_files: 5,
        }
    }

    /// Activate the menu bar (highlight first menu, no dropdown yet).
    pub(crate) fn open(&mut self) {
        self.active = true;
        self.dropdown_open = false;
        self.selected_menu_index = 0;
        self.selected_item_index = 0;
        self.needs_redraw = true;
    }

    /// Open the dropdown for the currently highlighted menu.
    pub(crate) fn open_dropdown(&mut self) {
        self.dropdown_open = true;
        self.selected_item_index = 0;
        self.file_section_scroll_offset = 0;
        self.needs_redraw = true;
    }

    /// Close the dropdown but keep the menu bar active.
    fn close_dropdown(&mut self) {
        self.dropdown_open = false;
        self.file_section_scroll_offset = 0;
        self.needs_redraw = true;
    }

    /// Deactivate the menu bar entirely.
    pub(crate) fn close(&mut self) {
        self.active = false;
        self.dropdown_open = false;
        self.file_section_scroll_offset = 0;
        self.needs_redraw = true;
    }

    /// Switch to the next menu (wraps around).
    pub(crate) fn next_menu(&mut self) {
        self.switch_menu((self.selected_menu_index + 1) % self.menus.len());
    }

    /// Switch to the previous menu (wraps around).
    pub(crate) fn prev_menu(&mut self) {
        let new_index = if self.selected_menu_index == 0 {
            self.menus.len() - 1
        } else {
            self.selected_menu_index - 1
        };
        self.switch_menu(new_index);
    }

    /// Switch to a specific menu, preserving dropdown-open state.
    fn switch_menu(&mut self, new_index: usize) {
        let was_open = self.dropdown_open;
        self.selected_menu_index = new_index;
        self.selected_item_index = 0;
        self.file_section_scroll_offset = 0;
        self.dropdown_open = was_open;
        self.needs_redraw = true;
    }

    /// Move selection to the next non-separator item.
    pub(crate) fn next_item(&mut self) {
        let len = self.menus[self.selected_menu_index].items.len();
        self.selected_item_index = self.find_next_non_separator(self.selected_item_index, len, true);
        self.ensure_selected_visible();
        self.needs_redraw = true;
    }

    /// Move selection to the previous non-separator item.
    pub(crate) fn prev_item(&mut self) {
        let len = self.menus[self.selected_menu_index].items.len();
        self.selected_item_index = self.find_next_non_separator(self.selected_item_index, len, false);
        self.ensure_selected_visible();
        self.needs_redraw = true;
    }

    /// Find the next (or previous) non-separator item index, wrapping around.
    fn find_next_non_separator(&self, current: usize, len: usize, forward: bool) -> usize {
        let menu = &self.menus[self.selected_menu_index];
        let step = |i: usize| {
            if forward { (i + 1) % len } else if i == 0 { len - 1 } else { i - 1 }
        };
        let mut next = step(current);
        let start = next;
        while matches!(menu.items[next], MenuItem::Separator) {
            next = step(next);
            if next == start {
                break; // All items are separators — shouldn't happen.
            }
        }
        next
    }

    /// Scroll the file section so that the selected item is visible.
    fn ensure_selected_visible(&mut self) {
        if self.selected_menu_index != FILE_MENU_INDEX
            || self.selected_item_index < FILE_SECTION_START_IDX
        {
            return;
        }

        let (file_count, file_end_idx) = self.get_file_section_bounds();
        if self.selected_item_index >= file_end_idx || file_count <= self.max_visible_files {
            self.file_section_scroll_offset = 0;
            return;
        }

        let file_idx = self.selected_item_index - FILE_SECTION_START_IDX;
        if file_idx < self.file_section_scroll_offset {
            self.file_section_scroll_offset = file_idx;
        } else if file_idx >= self.file_section_scroll_offset + self.max_visible_files {
            self.file_section_scroll_offset = file_idx - self.max_visible_files + 1;
        }
    }

    /// Return `(file_count, file_end_idx)` for the file section of the File menu.
    fn get_file_section_bounds(&self) -> (usize, usize) {
        let menu = &self.menus[FILE_MENU_INDEX];
        let mut file_count = 0;
        let mut file_end_idx = menu.items.len(); // Default: no trailing separator found.

        for idx in FILE_SECTION_START_IDX..menu.items.len() {
            if matches!(menu.items[idx], MenuItem::Separator) {
                file_end_idx = idx;
                break;
            }
            file_count += 1;
        }

        (file_count, file_end_idx)
    }

    /// Return the action for the currently highlighted item, if any.
    pub(crate) fn get_selected_action(&self) -> Option<MenuAction> {
        match &self.menus[self.selected_menu_index].items[self.selected_item_index] {
            MenuItem::Action { action, .. } | MenuItem::Checkable { action, .. } => Some(*action),
            MenuItem::Separator => None,
        }
    }

    /// Update the checked state of a checkable item (e.g. line-wrap toggle).
    pub(crate) fn update_checkable(&mut self, target: MenuAction, checked: bool) {
        for menu in &mut self.menus {
            for item in &mut menu.items {
                if let MenuItem::Checkable { action, checked: item_checked, .. } = item {
                    if *action == target {
                        *item_checked = checked;
                    }
                }
            }
        }
    }

    /// Refresh the File menu with the current list of recent files.
    #[allow(dead_code)] // Used in ui.rs (binary)
    pub(crate) fn update_file_menu(
        &mut self,
        current_file: &str,
        is_current_modified: bool,
        is_current_read_only: bool,
    ) {
        let files = crate::recent::get_recent_files().unwrap_or_default();
        let current_canonical = std::path::PathBuf::from(current_file)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(current_file));

        let file_labels =
            Self::build_file_labels(&files, &current_canonical, is_current_modified, is_current_read_only);

        self.menus[0] = Menu::new("File", 'f', Self::build_file_menu_items(file_labels));
        self.needs_redraw = true;
    }

    /// Build display labels for each recent file, prefixing status indicators where needed.
    /// - `⚿` for read-only files
    /// - `*` for files with unsaved changes
    fn build_file_labels(
        files: &[std::path::PathBuf],
        current_canonical: &std::path::Path,
        is_current_modified: bool,
        is_current_read_only: bool,
    ) -> Vec<String> {
        files
            .iter()
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
                    path.exists()
                        && std::fs::OpenOptions::new().write(true).open(&path).is_err()
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

    /// Assemble the full set of File menu items from static entries and the file list.
    fn build_file_menu_items(file_labels: Vec<String>) -> Vec<MenuItem> {
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

    /// Activate a menu by its Alt+<hotkey> character. Returns true if a match was found.
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

    /// Jump selection to the last item of the section above the current one (Ctrl+Up).
    pub(crate) fn jump_to_prev_section(&mut self) {
        let menu = &self.menus[self.selected_menu_index];
        if self.selected_item_index == 0 {
            return;
        }

        let mut idx = self.selected_item_index;
        while idx > 0 {
            idx -= 1;
            if matches!(menu.items[idx], MenuItem::Separator) {
                if idx == 0 {
                    return;
                }
                idx -= 1;
                while idx > 0 && matches!(menu.items[idx], MenuItem::Separator) {
                    idx -= 1;
                }
                self.selected_item_index = idx;
                self.ensure_selected_visible();
                self.needs_redraw = true;
                return;
            }
        }

        // Already in the first section — go to first non-separator item.
        self.selected_item_index =
            self.find_next_non_separator(0, menu.items.len(), true);
        self.needs_redraw = true;
    }

    /// Jump selection to the first item of the section below the current one (Ctrl+Down).
    pub(crate) fn jump_to_next_section(&mut self) {
        let menu = &self.menus[self.selected_menu_index];
        let mut idx = self.selected_item_index + 1;

        while idx < menu.items.len() {
            if matches!(menu.items[idx], MenuItem::Separator) {
                // Skip consecutive separators.
                idx += 1;
                while idx < menu.items.len() && matches!(menu.items[idx], MenuItem::Separator) {
                    idx += 1;
                }
                if idx < menu.items.len() {
                    self.selected_item_index = idx;
                    self.ensure_selected_visible();
                    self.needs_redraw = true;
                }
                return;
            }
            idx += 1;
        }
    }

    /// Update `max_visible_files` from settings.
    pub(crate) fn update_max_visible_files(&mut self, max_visible_files: usize) {
        self.max_visible_files = max_visible_files;
    }
}

/// Render the open dropdown for the currently selected menu.
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
    let menu_x = menu_x_position(menu_bar, state, lines);
    let max_width = menu_display_width(menu);

    let menu_bg_color = crate::settings::Settings::parse_color(&state.settings.appearance.header_bg)
        .unwrap_or(Color::DarkBlue);
    let selection_color = Color::Rgb { r: 100, g: 149, b: 237 };

    if menu_bar.selected_menu_index == FILE_MENU_INDEX {
        render_file_menu_dropdown(
            stdout, menu, menu_bar, menu_x, max_width, menu_bg_color, selection_color,
            state.settings.max_menu_files,
        )?;
    } else {
        render_simple_dropdown(
            stdout, menu, menu_bar, menu_x, max_width, menu_bg_color, selection_color,
        )?;
    }

    Ok(())
}

/// Render a plain dropdown (all items without scrolling).
fn render_simple_dropdown(
    stdout: &mut impl Write,
    menu: &Menu,
    menu_bar: &MenuBar,
    menu_x: usize,
    max_width: usize,
    bg_color: crossterm::style::Color,
    selection_color: crossterm::style::Color,
) -> Result<(), std::io::Error> {
    for (idx, item) in menu.items.iter().enumerate() {
        render_menu_item_at_row(
            stdout, item, idx == menu_bar.selected_item_index,
            menu_x, (idx + 1) as u16, max_width, bg_color, selection_color,
        )?;
    }
    Ok(())
}

/// Render the File menu dropdown, which has a scrollable file section in the middle.
fn render_file_menu_dropdown(
    stdout: &mut impl Write,
    menu: &Menu,
    menu_bar: &MenuBar,
    menu_x: usize,
    max_width: usize,
    bg_color: crossterm::style::Color,
    selection_color: crossterm::style::Color,
    max_visible_files: usize,
) -> Result<(), std::io::Error> {
    // Collect file entries (FileOpenRecent items only).
    let mut file_end_idx = menu.items.len();
    let mut files: Vec<(usize, &MenuItem)> = Vec::new();

    for (idx, item) in menu.items.iter().enumerate() {
        if idx < FILE_SECTION_START_IDX {
            continue;
        }
        if matches!(item, MenuItem::Separator) {
            file_end_idx = idx;
            break;
        }
        if matches!(item, MenuItem::Action { action: MenuAction::FileOpenRecent(_), .. }) {
            files.push((idx, item));
        } else {
            file_end_idx = idx;
            break;
        }
    }

    let total_files = files.len();
    let scroll_offset = menu_bar.file_section_scroll_offset;
    let mut display_row = 1u16;

    // Render static items above the file section.
    for idx in 0..FILE_SECTION_START_IDX.min(menu.items.len()) {
        render_menu_item_at_row(
            stdout, &menu.items[idx], idx == menu_bar.selected_item_index,
            menu_x, display_row, max_width, bg_color, selection_color,
        )?;
        display_row += 1;
    }

    // Render the scrollable file section.
    if total_files > 0 {
        let actual_visible = max_visible_files.min(total_files);
        let visible_start = scroll_offset.min(total_files.saturating_sub(max_visible_files));
        let visible_end = (visible_start + actual_visible).min(total_files);
        let show_scrollbar = total_files > max_visible_files;

        for file_idx in visible_start..visible_end {
            if let Some((idx, item)) = files.get(file_idx) {
                render_menu_item_at_row(
                    stdout, item, *idx == menu_bar.selected_item_index,
                    menu_x, display_row, max_width, bg_color, selection_color,
                )?;

                if show_scrollbar {
                    let row_in_view = file_idx - visible_start;
                    render_file_scrollbar_row(
                        stdout, row_in_view, scroll_offset, total_files,
                        max_visible_files, menu_x + max_width - 1, display_row,
                    )?;
                }

                display_row += 1;
            }
        }
    }

    // Render static items below the file section (Separator, Quit).
    for idx in file_end_idx..menu.items.len() {
        render_menu_item_at_row(
            stdout, &menu.items[idx], idx == menu_bar.selected_item_index,
            menu_x, display_row, max_width, bg_color, selection_color,
        )?;
        display_row += 1;
    }

    Ok(())
}

/// Render a single menu item at a specific screen position with optional selection highlight.
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
        execute!(stdout, SetBackgroundColor(selection_color), SetForegroundColor(Color::White))?;
    } else {
        execute!(stdout, SetBackgroundColor(bg_color))?;
    }
    render_menu_item(stdout, item, max_width)?;
    execute!(stdout, ResetColor)?;
    Ok(())
}

/// Print a single menu item's text content.
fn render_menu_item(
    stdout: &mut impl Write,
    item: &MenuItem,
    max_width: usize,
) -> Result<(), std::io::Error> {
    use crossterm::{execute, style::Print};

    match item {
        MenuItem::Action { label, .. } => {
            execute!(stdout, Print(format!(" {:<width$} ", label, width = max_width - 2)))?;
        }
        MenuItem::Checkable { label, checked, .. } => {
            let check = if *checked { "✓" } else { " " };
            execute!(stdout, Print(format!(" [{}] {:<width$} ", check, label, width = max_width - 6)))?;
        }
        MenuItem::Separator => {
            execute!(stdout, Print(format!(" {} ", "─".repeat(max_width - 2))))?;
        }
    }
    Ok(())
}

/// Render one row of the scrollbar for the file section.
fn render_file_scrollbar_row(
    stdout: &mut impl Write,
    row_in_view: usize,
    scroll_offset: usize,
    total_files: usize,
    max_visible: usize,
    x: usize,
    y: u16,
) -> Result<(), std::io::Error> {
    use crossterm::{cursor::MoveTo, execute, style::{Color, Print, SetBackgroundColor, SetForegroundColor, ResetColor}};

    let bar_size = ((max_visible as f64 / total_files as f64) * max_visible as f64).max(1.0) as usize;
    let bar_start = ((scroll_offset as f64 / total_files as f64) * max_visible as f64) as usize;
    let in_bar = row_in_view >= bar_start && row_in_view < bar_start + bar_size;

    let color = if in_bar {
        Color::Rgb { r: 100, g: 149, b: 237 }
    } else {
        Color::Rgb { r: 50, g: 50, b: 50 }
    };
    let glyph = if in_bar { "█" } else { "░" };

    execute!(stdout, MoveTo(x as u16, y))?;
    execute!(stdout, SetBackgroundColor(color), SetForegroundColor(color))?;
    execute!(stdout, Print(glyph))?;
    execute!(stdout, ResetColor)?;
    Ok(())
}

/// Handle a keyboard event for the menu system.
///
/// Returns `(action, needs_full_redraw)`:
/// - `action`: the selected `MenuAction`, if any
/// - `needs_full_redraw`: `true` when the whole screen must be redrawn (menu opened/closed),
///   `false` when only the menu overlay needs updating
pub(crate) fn handle_menu_key(
    menu_bar: &mut MenuBar,
    key_event: KeyEvent,
) -> (Option<MenuAction>, bool) {
    let code = key_event.code;
    let modifiers = key_event.modifiers;

    // Alt+letter opens a specific menu.
    if modifiers.contains(KeyModifiers::ALT) {
        if let KeyCode::Char(c) = code {
            if menu_bar.try_activate_by_hotkey(c) {
                return (None, true);
            }
        }
        // Other Alt+ combinations (e.g. Alt+Arrow for block selection) fall through.
    }

    // Esc toggles the menu open/closed.
    if code == KeyCode::Esc
        && !modifiers.contains(KeyModifiers::ALT)
        && !modifiers.contains(KeyModifiers::CONTROL)
    {
        if menu_bar.active {
            menu_bar.close();
            return (None, true);
        } else {
            menu_bar.active = true;
            menu_bar.selected_menu_index = FILE_MENU_INDEX;
            menu_bar.dropdown_open = true;

            // Pre-select the second recent file for quick Esc+Enter switching.
            let file_count = count_files_in_menu(&menu_bar.menus[FILE_MENU_INDEX]);
            menu_bar.selected_item_index = if file_count >= 2 {
                FILE_SECTION_START_IDX + 1
            } else if file_count >= 1 {
                FILE_SECTION_START_IDX
            } else {
                0
            };

            return (None, true);
        }
    }

    if !menu_bar.active {
        return (None, false);
    }

    // Ctrl+W removes the highlighted file from the recent list.
    if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('w') {
        if menu_bar.dropdown_open && menu_bar.selected_menu_index == FILE_MENU_INDEX
            && menu_bar.selected_item_index >= FILE_SECTION_START_IDX
        {
            if let Some(MenuItem::Action { action: MenuAction::FileOpenRecent(idx), .. }) =
                menu_bar.menus[FILE_MENU_INDEX].items.get(menu_bar.selected_item_index)
            {
                return (Some(MenuAction::FileRemove(*idx)), false);
            }
        }
    }

    match code {
        KeyCode::Left => {
            menu_bar.prev_menu();
            (None, true)
        }
        KeyCode::Right => {
            menu_bar.next_menu();
            (None, true)
        }
        KeyCode::Down => {
            if modifiers.contains(KeyModifiers::CONTROL) && menu_bar.dropdown_open {
                menu_bar.jump_to_next_section();
                (None, false)
            } else if menu_bar.dropdown_open {
                menu_bar.next_item();
                (None, false)
            } else {
                menu_bar.open_dropdown();
                (None, true)
            }
        }
        KeyCode::Up => {
            if modifiers.contains(KeyModifiers::CONTROL) && menu_bar.dropdown_open {
                menu_bar.jump_to_prev_section();
            } else if menu_bar.dropdown_open {
                menu_bar.prev_item();
            }
            (None, false)
        }
        KeyCode::Enter => {
            if menu_bar.dropdown_open {
                let action = menu_bar.get_selected_action();
                menu_bar.close();
                (action, true)
            } else {
                menu_bar.open_dropdown();
                (None, true)
            }
        }
        _ => (None, false),
    }
}

/// Handle a mouse event for the menu system.
///
/// Returns `(action, needs_full_redraw)` — same semantics as `handle_menu_key`.
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

/// Handle a left-click anywhere (burger icon, menu labels, or dropdown items).
fn handle_menu_left_click(
    menu_bar: &mut MenuBar,
    col: usize,
    row: usize,
    line_number_width: u16,
) -> (Option<MenuAction>, bool) {
    let burger_start = line_number_width as usize;
    let burger_end = burger_start + 2; // "≡ " is 2 chars wide.

    if row == 0 && col >= burger_start && col < burger_end {
        if menu_bar.active { menu_bar.close(); } else { menu_bar.open(); }
        return (None, true);
    }

    if row == 0 && menu_bar.active {
        return handle_menu_label_click(menu_bar, col, burger_end);
    }

    if menu_bar.active && menu_bar.dropdown_open && row > 0 {
        return handle_dropdown_item_click(menu_bar, row);
    }

    (None, false)
}

/// Handle a click on one of the menu labels in the menu bar row.
fn handle_menu_label_click(
    menu_bar: &mut MenuBar,
    col: usize,
    start_x: usize,
) -> (Option<MenuAction>, bool) {
    let mut x = start_x;
    for (idx, menu) in menu_bar.menus.iter().enumerate() {
        if col >= x - 1 && col < x + menu.label.len() + 1 {
            if menu_bar.selected_menu_index == idx {
                if menu_bar.dropdown_open { menu_bar.close_dropdown(); } else { menu_bar.open_dropdown(); }
            } else {
                menu_bar.selected_menu_index = idx;
                menu_bar.open_dropdown();
            }
            return (None, true);
        }
        x += menu.label.len() + 2;
    }
    menu_bar.close();
    (None, true)
}

/// Select and activate the item at the given dropdown row, if it is not a separator.
fn handle_dropdown_item_click(
    menu_bar: &mut MenuBar,
    row: usize,
) -> (Option<MenuAction>, bool) {
    let item_idx = row - 1;
    if item_idx < menu_bar.menus[menu_bar.selected_menu_index].items.len()
        && !matches!(menu_bar.menus[menu_bar.selected_menu_index].items[item_idx], MenuItem::Separator)
    {
        menu_bar.selected_item_index = item_idx;
        let action = menu_bar.get_selected_action();
        menu_bar.close();
        return (action, true);
    }
    (None, false)
}

/// Highlight the item under the mouse cursor without activating it.
fn handle_dropdown_hover(
    menu_bar: &mut MenuBar,
    row: usize,
) -> (Option<MenuAction>, bool) {
    if row == 0 {
        return (None, false);
    }
    let item_idx = row - 1;
    let items = &menu_bar.menus[menu_bar.selected_menu_index].items;
    if item_idx < items.len()
        && !matches!(items[item_idx], MenuItem::Separator)
        && menu_bar.selected_item_index != item_idx
    {
        menu_bar.selected_item_index = item_idx;
        return (None, false);
    }
    (None, false)
}

/// Return the X column where the dropdown for the currently selected menu should be drawn.
fn menu_x_position(
    menu_bar: &MenuBar,
    state: &crate::editor_state::FileViewerState,
    lines: &[String],
) -> usize {
    let line_num_width =
        crate::coordinates::line_number_display_width(state.settings, lines.len()) as usize;
    let mut x = line_num_width + 2; // line-number gutter + "≡ "
    for i in 0..menu_bar.selected_menu_index {
        x += menu_bar.menus[i].label.len() + 2;
    }
    x
}

/// Return true if the given screen coordinate falls inside the open dropdown.
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
    let line_num_width = line_number_width as usize;
    let mut menu_x = line_num_width + 2;
    for i in 0..menu_bar.selected_menu_index {
        menu_x += menu_bar.menus[i].label.len() + 2;
    }

    let max_width = menu_display_width(menu);
    let dropdown_rows = 1..=menu.items.len();

    dropdown_rows.contains(&row) && col >= menu_x && col < menu_x + max_width
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

        menu_bar.selected_item_index = 0;
        for _ in 0..5 {
            menu_bar.next_item();
        }
        assert!(
            !matches!(menu_bar.menus[0].items[menu_bar.selected_item_index], MenuItem::Separator),
            "Should not land on separator"
        );
    }

    #[test]
    fn test_checkable_update() {
        let mut menu_bar = MenuBar::new();
        menu_bar.update_checkable(MenuAction::ViewLineWrap, true);

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

        let (action, needs_redraw) =
            handle_menu_key(&mut menu_bar, KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()));

        assert!(menu_bar.active, "Menu should be active");
        assert!(menu_bar.dropdown_open, "Dropdown should be open");
        assert_eq!(menu_bar.selected_menu_index, 0, "File menu should be selected");
        assert!(action.is_none());
        assert!(needs_redraw);
    }

    #[test]
    fn test_esc_closes_menu_when_active() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut menu_bar = MenuBar::new();
        menu_bar.open();
        menu_bar.open_dropdown();

        let (action, needs_redraw) =
            handle_menu_key(&mut menu_bar, KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()));

        assert!(!menu_bar.active, "Menu should be inactive");
        assert!(!menu_bar.dropdown_open, "Dropdown should be closed");
        assert!(action.is_none());
        assert!(needs_redraw);
    }

    #[test]
    fn test_left_right_switch_menus_with_dropdown_open() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut menu_bar = MenuBar::new();
        menu_bar.open();
        menu_bar.open_dropdown();
        assert_eq!(menu_bar.selected_menu_index, 0);

        let (action, _) =
            handle_menu_key(&mut menu_bar, KeyEvent::new(KeyCode::Right, KeyModifiers::empty()));
        assert_eq!(menu_bar.selected_menu_index, 1, "Should move to Edit menu");
        assert!(menu_bar.dropdown_open, "Dropdown should stay open");
        assert!(action.is_none());

        let (action, _) =
            handle_menu_key(&mut menu_bar, KeyEvent::new(KeyCode::Left, KeyModifiers::empty()));
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

        let file1 = tmp.path().join("file1.txt");
        let file2 = tmp.path().join("file2.txt");
        let file3 = tmp.path().join("file3.txt");

        fs::write(&file1, "content1").unwrap();
        fs::write(&file2, "content2").unwrap();
        fs::write(&file3, "content3").unwrap();

        crate::recent::update_recent_file(file1.to_str().unwrap()).unwrap();
        crate::recent::update_recent_file(file2.to_str().unwrap()).unwrap();
        crate::recent::update_recent_file(file3.to_str().unwrap()).unwrap();

        let mut history2 = UndoHistory::new();
        history2.modified = true;
        history2.save(file2.to_str().unwrap()).unwrap();

        let mut history3 = UndoHistory::new();
        history3.modified = false;
        history3.save(file3.to_str().unwrap()).unwrap();

        let mut menu_bar = MenuBar::new();
        menu_bar.update_file_menu(file1.to_str().unwrap(), false, false);

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

        for i in 1..=7 {
            let file = tmp.path().join(format!("file{}.txt", i));
            fs::write(&file, "content").unwrap();
            crate::recent::update_recent_file(file.to_str().unwrap()).unwrap();
        }

        let file1 = tmp.path().join("file1.txt");
        let mut menu_bar = MenuBar::new();
        menu_bar.update_file_menu(file1.to_str().unwrap(), false, false);

        let file_menu = &menu_bar.menus[0];
        let mut found_ellipsis = false;
        let mut file_count = 0;

        for item in &file_menu.items {
            if let MenuItem::Action { label, .. } = item {
                if label == "..." { found_ellipsis = true; }
                if label.contains("file") && label.contains(".txt") { file_count += 1; }
            }
        }

        assert!(!found_ellipsis, "Should NOT show '...' — all files shown with scrolling");
        assert_eq!(file_count, 7, "Should show all 7 files");
    }

    #[test]
    fn test_down_key_opens_dropdown_when_menu_active() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut menu_bar = MenuBar::new();
        menu_bar.open();
        assert!(!menu_bar.dropdown_open);

        let (action, needs_redraw) =
            handle_menu_key(&mut menu_bar, KeyEvent::new(KeyCode::Down, KeyModifiers::empty()));

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
        menu_bar.selected_item_index = 0;

        let (action, needs_redraw) =
            handle_menu_key(&mut menu_bar, KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()));

        assert_eq!(action.unwrap(), MenuAction::FileNew);
        assert!(!menu_bar.active, "Menu should close after selection");
        assert!(needs_redraw);
    }

    #[test]
    fn test_up_down_navigation_skips_separators() {
        let mut menu_bar = MenuBar::new();
        menu_bar.open_dropdown();

        // File menu: New, Open..., Save, Close, Close all, [Separator], Quit
        menu_bar.selected_item_index = 4; // "Close all"
        menu_bar.next_item(); // Should jump over separator to "Quit"

        assert!(
            !matches!(menu_bar.menus[0].items[menu_bar.selected_item_index], MenuItem::Separator),
            "Should not land on separator"
        );
        if let MenuItem::Action { label, .. } = &menu_bar.menus[0].items[menu_bar.selected_item_index] {
            assert_eq!(label, "Quit", "Should skip separator to reach Quit");
        } else {
            panic!("Expected an Action item");
        }
    }
}

