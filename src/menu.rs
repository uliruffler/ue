use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::io::Write;

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
    FileOpen,
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
                        label: "Open".to_string(),
                        action: MenuAction::FileOpen,
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
                'h',
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
        self.selected_menu_index = (self.selected_menu_index + 1) % self.menus.len();
        self.selected_item_index = 0;
        self.dropdown_open = false; // Close dropdown when switching menus
    }

    /// Move to previous menu
    pub(crate) fn prev_menu(&mut self) {
        if self.selected_menu_index == 0 {
            self.selected_menu_index = self.menus.len() - 1;
        } else {
            self.selected_menu_index -= 1;
        }
        self.selected_item_index = 0;
        self.dropdown_open = false; // Close dropdown when switching menus
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
) -> Result<(), std::io::Error> {
    use crossterm::{cursor::MoveTo, execute, style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor}};

    if !menu_bar.active || !menu_bar.dropdown_open {
        return Ok(());
    }

    let menu = &menu_bar.menus[menu_bar.selected_menu_index];

    // Calculate menu horizontal position (after burger icon and before selected menu label)
    // Menu should align roughly under its label in the header
    let mut menu_x = 2; // After burger icon "≡ "
    for i in 0..menu_bar.selected_menu_index {
        menu_x += menu_bar.menus[i].label.len() + 2; // +2 for spacing
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

    // Alt+letter opens specific menu
    if modifiers.contains(KeyModifiers::ALT) {
        if let KeyCode::Char(c) = code {
            if menu_bar.try_activate_by_hotkey(c) {
                return (None, true); // Menu opened, needs full redraw
            }
        }
        // Don't handle other Alt+ combinations here (like Alt+Arrow for block selection)
        // Those should be handled by the normal editor logic
    }

    if !menu_bar.active {
        return (None, false);
    }

    match code {
        KeyCode::Esc => {
            menu_bar.close();
            (None, true) // Menu closed, needs full redraw
        }
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
            // Check if clicking on burger icon (≡ ) which is after line numbers
            // Burger icon is at columns [line_number_width, line_number_width + 2)
            let burger_start = line_number_width as usize;
            let burger_end = burger_start + 2; // "≡ " is 2 characters wide

            if row == 0 && col >= burger_start && col < burger_end {
                // Toggle menu on burger click
                if menu_bar.active {
                    menu_bar.close();
                } else {
                    menu_bar.open();
                }
                return (None, true); // Menu toggled, needs full redraw
            }

            // Check if clicking on menu labels (only when menu is active)
            if row == 0 && menu_bar.active {
                let mut x = burger_end; // Start after burger icon
                for (idx, menu) in menu_bar.menus.iter().enumerate() {
                    if col >= x && col < x + menu.label.len() {
                        // Clicked on this menu label
                        if menu_bar.selected_menu_index == idx {
                            // Clicking same menu toggles dropdown
                            if menu_bar.dropdown_open {
                                menu_bar.dropdown_open = false;
                            } else {
                                menu_bar.open_dropdown();
                            }
                        } else {
                            // Switch to different menu and open its dropdown
                            menu_bar.selected_menu_index = idx;
                            menu_bar.open_dropdown();
                        }
                        return (None, true); // Menu changed, needs full redraw
                    }
                    x += menu.label.len() + 2; // +2 for spacing
                }

                // Clicked on menu bar but not on any menu -> close
                menu_bar.close();
                return (None, true); // Menu closed, needs full redraw
            }

            // Check if clicking on dropdown menu items
            if menu_bar.active && menu_bar.dropdown_open && row > 0 {
                let menu = &menu_bar.menus[menu_bar.selected_menu_index];

                if row - 1 < menu.items.len() {
                    let item_idx = row - 1;
                    if !matches!(menu.items[item_idx], MenuItem::Separator) {
                        menu_bar.selected_item_index = item_idx;
                        let action = menu_bar.get_selected_action();
                        menu_bar.close();
                        return (action, true); // Action selected and menu closed, needs full redraw
                    }
                }
            }

            (None, false)
        }
        MouseEventKind::Moved if menu_bar.active && menu_bar.dropdown_open => {
            // Handle hover over dropdown items - update selection but don't trigger full redraw
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
        _ => (None, false),
    }
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
}

