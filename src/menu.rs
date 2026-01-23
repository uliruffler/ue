use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::io::Write;
use std::path::Path;

/// Check if a file has unsaved changes by reading its undo history
fn check_file_has_unsaved_changes(file_path: &Path) -> bool {
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
    FileQuit,
    // Edit menu
    EditUndo,
    EditRedo,
    EditCopy,
    EditCut,
    EditPaste,
    EditFind,
    // View menu
    ViewFileSelector,
    ViewLineWrap,
    // Help menu
    HelpEditor,
    HelpFind,
    HelpFileSelector,
    HelpAbout,
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

/// Menu bar state
#[derive(Debug)]
pub(crate) struct MenuBar {
    pub(crate) menus: Vec<Menu>,
    pub(crate) active: bool,
    pub(crate) dropdown_open: bool, // True when dropdown menu is shown
    pub(crate) selected_menu_index: usize,
    pub(crate) selected_item_index: usize,
}

impl MenuBar {
    pub(crate) fn new() -> Self {
        let menus = vec![
            Menu::new(
                "File",
                'f',
                vec![
                    MenuItem::Action {
                        label: "New".to_string(),
                        action: MenuAction::FileNew,
                    },
                    MenuItem::Action {
                        label: "Open...".to_string(),
                        action: MenuAction::FileOpenDialog,
                    },
                    MenuItem::Action {
                        label: "Save".to_string(),
                        action: MenuAction::FileSave,
                    },
                    MenuItem::Action {
                        label: "Close".to_string(),
                        action: MenuAction::FileClose,
                    },
                    MenuItem::Separator,
                    MenuItem::Action {
                        label: "Quit".to_string(),
                        action: MenuAction::FileQuit,
                    },
                ],
            ),
            Menu::new(
                "Edit",
                'e',
                vec![
                    MenuItem::Action {
                        label: "Undo".to_string(),
                        action: MenuAction::EditUndo,
                    },
                    MenuItem::Action {
                        label: "Redo".to_string(),
                        action: MenuAction::EditRedo,
                    },
                    MenuItem::Separator,
                    MenuItem::Action {
                        label: "Copy".to_string(),
                        action: MenuAction::EditCopy,
                    },
                    MenuItem::Action {
                        label: "Cut".to_string(),
                        action: MenuAction::EditCut,
                    },
                    MenuItem::Action {
                        label: "Paste".to_string(),
                        action: MenuAction::EditPaste,
                    },
                    MenuItem::Separator,
                    MenuItem::Action {
                        label: "Find".to_string(),
                        action: MenuAction::EditFind,
                    },
                ],
            ),
            Menu::new(
                "View",
                'v',
                vec![
                    MenuItem::Action {
                        label: "File Selector".to_string(),
                        action: MenuAction::ViewFileSelector,
                    },
                    MenuItem::Checkable {
                        label: "Line Wrap".to_string(),
                        action: MenuAction::ViewLineWrap,
                        checked: false, // Will be updated dynamically
                    },
                ],
            ),
            Menu::new(
                "Help",
                ' ',
                vec![
                    MenuItem::Action {
                        label: "Editor Help".to_string(),
                        action: MenuAction::HelpEditor,
                    },
                    MenuItem::Action {
                        label: "Find Help".to_string(),
                        action: MenuAction::HelpFind,
                    },
                    MenuItem::Action {
                        label: "File Selector Help".to_string(),
                        action: MenuAction::HelpFileSelector,
                    },
                    MenuItem::Separator,
                    MenuItem::Action {
                        label: "About".to_string(),
                        action: MenuAction::HelpAbout,
                    },
                ],
            ),
        ];

        Self {
            menus,
            active: false,
            dropdown_open: false,
            selected_menu_index: 0,
            selected_item_index: 0,
        }
    }

    /// Open menu bar (activate first menu)
    pub(crate) fn open(&mut self) {
        self.active = true;
        self.dropdown_open = false; // Don't open dropdown initially
        self.selected_menu_index = 0;
        self.selected_item_index = 0;
    }

    /// Open dropdown for currently selected menu
    pub(crate) fn open_dropdown(&mut self) {
        self.dropdown_open = true;
        self.selected_item_index = 0; // Reset to first item
    }

    /// Close menu bar
    pub(crate) fn close(&mut self) {
        self.active = false;
        self.dropdown_open = false;
    }

    /// Move to next menu
    pub(crate) fn next_menu(&mut self) {
        let was_dropdown_open = self.dropdown_open;
        self.selected_menu_index = (self.selected_menu_index + 1) % self.menus.len();
        self.selected_item_index = 0;
        // If dropdown was open, keep it open for the new menu
        if was_dropdown_open {
            self.dropdown_open = true;
        } else {
            self.dropdown_open = false;
        }
    }

    /// Move to previous menu
    pub(crate) fn prev_menu(&mut self) {
        let was_dropdown_open = self.dropdown_open;
        if self.selected_menu_index == 0 {
            self.selected_menu_index = self.menus.len() - 1;
        } else {
            self.selected_menu_index -= 1;
        }
        self.selected_item_index = 0;
        // If dropdown was open, keep it open for the new menu
        if was_dropdown_open {
            self.dropdown_open = true;
        } else {
            self.dropdown_open = false;
        }
    }

    /// Move to next item in current menu (skip separators)
    pub(crate) fn next_item(&mut self) {
        let menu = &self.menus[self.selected_menu_index];
        let mut next = (self.selected_item_index + 1) % menu.items.len();

        // Skip separators
        while matches!(menu.items[next], MenuItem::Separator) {
            next = (next + 1) % menu.items.len();
            if next == self.selected_item_index {
                break; // All items are separators (shouldn't happen)
            }
        }

        self.selected_item_index = next;
    }

    /// Move to previous item in current menu (skip separators)
    pub(crate) fn prev_item(&mut self) {
        let menu = &self.menus[self.selected_menu_index];
        let mut prev = if self.selected_item_index == 0 {
            menu.items.len() - 1
        } else {
            self.selected_item_index - 1
        };

        // Skip separators
        while matches!(menu.items[prev], MenuItem::Separator) {
            if prev == 0 {
                prev = menu.items.len() - 1;
            } else {
                prev -= 1;
            }
            if prev == self.selected_item_index {
                break; // All items are separators
            }
        }

        self.selected_item_index = prev;
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

    /// Update File menu with current tracked files
    #[allow(dead_code)] // Only used in ui.rs (binary)
    pub(crate) fn update_file_menu(&mut self, max_files: usize, current_file: &str, is_current_modified: bool) {
        let files = crate::recent::get_recent_files().unwrap_or_default();
        let show_more = files.len() > max_files;
        let files_to_show = if show_more { max_files } else { files.len() };

        let current_canonical = std::path::PathBuf::from(current_file)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(current_file));

        let file_labels = Self::create_file_labels(
            &files,
            files_to_show,
            &current_canonical,
            is_current_modified,
        );

        let items = Self::build_file_menu_items(file_labels, show_more);
        self.menus[0] = Menu::new("File", 'f', items);
    }

    /// Create labeled list of files with unsaved markers
    fn create_file_labels(
        files: &[std::path::PathBuf],
        count: usize,
        current_canonical: &std::path::Path,
        is_current_modified: bool,
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

                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_else(|| file.to_str().unwrap_or("???"));

                if is_modified {
                    format!("* {}", filename)
                } else {
                    filename.to_string()
                }
            })
            .collect()
    }

    /// Build complete File menu items including static items and recent files
    fn build_file_menu_items(file_labels: Vec<String>, show_more: bool) -> Vec<MenuItem> {
        let mut items = vec![
            MenuItem::Action { label: "New".to_string(), action: MenuAction::FileNew },
            MenuItem::Action { label: "Open...".to_string(), action: MenuAction::FileOpenDialog },
            MenuItem::Action { label: "Save".to_string(), action: MenuAction::FileSave },
            MenuItem::Action { label: "Close".to_string(), action: MenuAction::FileClose },
        ];

        if !file_labels.is_empty() {
            items.push(MenuItem::Separator);

            for (idx, label) in file_labels.iter().enumerate() {
                items.push(MenuItem::Action {
                    label: label.clone(),
                    action: MenuAction::FileOpenRecent(idx),
                });
            }

            if show_more {
                items.push(MenuItem::Action {
                    label: "...".to_string(),
                    action: MenuAction::ViewFileSelector,
                });
            }
        }

        items.push(MenuItem::Separator);
        items.push(MenuItem::Action { label: "Quit".to_string(), action: MenuAction::FileQuit });

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
}


/// Render dropdown menu for currently selected menu (below the header)
pub(crate) fn render_dropdown_menu(
    stdout: &mut impl Write,
    menu_bar: &MenuBar,
    state: &crate::editor_state::FileViewerState,
    lines: &[String],
) -> Result<(), std::io::Error> {
    use crossterm::{cursor::MoveTo, execute, style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor}};

    if !menu_bar.active || !menu_bar.dropdown_open {
        return Ok(());
    }

    let menu = &menu_bar.menus[menu_bar.selected_menu_index];

    // Calculate menu horizontal position (after burger icon and before selected menu label)
    // Menu should align under the selected menu label in the header
    let line_num_width = crate::coordinates::line_number_display_width(state.settings, lines.len()) as usize;
    let mut menu_x = line_num_width + 2; // line numbers + burger icon "≡ "
    for i in 0..menu_bar.selected_menu_index {
        menu_x += menu_bar.menus[i].label.len() + 2; // Menu label length + 2 spaces (matching rendering)
    }

    // Find longest item label for menu width
    let mut max_width = menu.label.len();
    for item in &menu.items {
        let width = match item {
            MenuItem::Action { label, .. } | MenuItem::Checkable { label, .. } => label.len() + 4, // " [✓] " for checkable
            MenuItem::Separator => 3,
        };
        if width > max_width {
            max_width = width;
        }
    }
    max_width += 4; // Padding

    // Get colors from settings
    let menu_bg_color = crate::settings::Settings::parse_color(&state.settings.appearance.header_bg)
        .unwrap_or(Color::DarkBlue);
    // Use light blue for selection (same as scrollbar bar)
    let selection_color = Color::Rgb { r: 100, g: 149, b: 237 };

    // Render each menu item starting from row 1 (below header at row 0)
    for (idx, item) in menu.items.iter().enumerate() {
        let row = (1 + idx) as u16;
        execute!(stdout, MoveTo(menu_x as u16, row))?;

        let is_selected = idx == menu_bar.selected_item_index;

        if is_selected {
            // Use light blue for selected item
            execute!(stdout, SetBackgroundColor(selection_color))?;
            execute!(stdout, SetForegroundColor(Color::White))?;
        } else {
            // Use header background color for normal items
            execute!(stdout, SetBackgroundColor(menu_bg_color))?;
            // Default text color (no SetForegroundColor call = use default)
        }

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

        execute!(stdout, ResetColor)?;
    }

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
            menu_bar.selected_menu_index = 0; // File menu

            // Select second recent file (index 6) if available, for quick Esc+Enter switching
            // File menu structure: New(0), Open(1), Save(2), Close(3), Separator(4), Recent1(5), Recent2(6), ...
            let file_menu = &menu_bar.menus[0];
            let has_two_recent_files = file_menu.items.len() >= 7; // At least 7 items means 2+ recent files
            menu_bar.selected_item_index = if has_two_recent_files { 6 } else { 0 };

            menu_bar.dropdown_open = true; // Open dropdown immediately
            return (None, true); // Menu opened, needs full redraw
        }
    }

    if !menu_bar.active {
        return (None, false);
    }

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
            if menu_bar.dropdown_open {
                menu_bar.next_item();
                (None, false) // Navigation only, no full redraw needed
            } else {
                // Open dropdown when Down is pressed on menu bar
                menu_bar.open_dropdown();
                (None, true) // Dropdown opened, needs full redraw
            }
        }
        KeyCode::Up => {
            if menu_bar.dropdown_open {
                menu_bar.prev_item();
                (None, false) // Navigation only, no full redraw needed
            } else {
                (None, false)
            }
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
                menu_bar.dropdown_open = !menu_bar.dropdown_open;
                if !menu_bar.dropdown_open {
                    // If closing, do nothing special
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
        menu_bar.update_file_menu(5, file1.to_str().unwrap(), false);

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
        menu_bar.update_file_menu(5, file1.to_str().unwrap(), false);

        let file_menu = &menu_bar.menus[0];
        let mut found_ellipsis = false;

        for item in &file_menu.items {
            if let MenuItem::Action { label, .. } = item {
                if label == "..." {
                    found_ellipsis = true;
                    break;
                }
            }
        }

        assert!(found_ellipsis, "Should show '...' when more files than max_files");
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

        // File menu: New, Open..., Save, Close, [Separator], Quit
        menu_bar.selected_item_index = 3; // Close
        menu_bar.next_item(); // Should skip separator and go to Quit

        let item = &menu_bar.menus[0].items[menu_bar.selected_item_index];
        if let MenuItem::Action { label, .. } = item {
            assert_eq!(label, "Quit", "Should skip separator");
        } else {
            panic!("Should be on an action item, not separator");
        }
    }
}


