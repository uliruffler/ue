use crossterm::event::{KeyCode, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct KeyBindings {
    pub(crate) quit: String,
    pub(crate) copy: String,
    pub(crate) paste: String,
    pub(crate) cut: String,
    pub(crate) close: String,
    pub(crate) save: String,
    pub(crate) undo: String,
    pub(crate) redo: String,
    #[serde(default = "default_file_selector")]
    pub(crate) file_selector: String,
    #[serde(default = "default_open_dialog")]
    pub(crate) open_dialog: String,
    pub(crate) find: String,
    pub(crate) find_next: String,
    pub(crate) find_previous: String,
    #[serde(default = "default_replace")]
    pub(crate) replace: String,
    #[serde(default = "default_replace_current")]
    pub(crate) replace_current: String,
    #[serde(default = "default_replace_all")]
    pub(crate) replace_all: String,
    pub(crate) goto_line: String,
    #[serde(default = "default_help")]
    pub(crate) help: String,
    #[serde(default = "default_save_and_quit")]
    pub(crate) save_and_quit: String,
    #[serde(default = "default_toggle_line_wrap")]
    pub(crate) toggle_line_wrap: String,
    #[serde(default = "default_new_file")]
    pub(crate) new_file: String,
    #[serde(default = "default_cursor_down")]
    pub(crate) cursor_down: String,
    #[serde(default = "default_cursor_up")]
    pub(crate) cursor_up: String,
    #[serde(default = "default_cursor_left")]
    pub(crate) cursor_left: String,
    #[serde(default = "default_cursor_right")]
    pub(crate) cursor_right: String,
    #[serde(default = "default_numpad_enter")]
    pub(crate) numpad_enter: String,
}

fn default_new_file() -> String {
    "Ctrl+n".into()
}

fn default_cursor_down() -> String {
    "Alt+j".into()
}

fn default_cursor_up() -> String {
    "Alt+k".into()
}

fn default_cursor_left() -> String {
    "Alt+h".into()
}

fn default_cursor_right() -> String {
    "Alt+l".into()
}

fn default_numpad_enter() -> String {
    "Ctrl+j".into()
}

fn default_replace() -> String {
    "Ctrl+h".into()
}

fn default_replace_current() -> String {
    "Ctrl+r".into()
}

fn default_replace_all() -> String {
    "Ctrl+Alt+r".into()
}

fn default_save_and_quit() -> String {
    "Ctrl+q".into()
}

fn default_help() -> String {
    "F1".into()
}

fn default_toggle_line_wrap() -> String {
    "Alt+w".into()
}

fn default_file_selector() -> String {
    "".into() // Empty - no longer used, Esc opens menu instead
}

fn default_open_dialog() -> String {
    "Ctrl+o".into()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct AppearanceSettings {
    #[serde(default = "default_line_number_digits")]
    pub(crate) line_number_digits: u8,
    #[serde(default = "default_header_bg")]
    pub(crate) header_bg: String,
    #[serde(default = "default_footer_bg")]
    pub(crate) footer_bg: String,
    #[serde(default = "default_line_numbers_bg")]
    pub(crate) line_numbers_bg: String,
    #[serde(default = "default_cursor_shape")]
    pub(crate) cursor_shape: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub(crate) keybindings: KeyBindings,
    #[serde(default = "default_tab_width")]
    pub(crate) tab_width: usize,
    #[serde(default = "default_double_tap_speed_ms")]
    pub(crate) double_tap_speed_ms: u64,
    #[serde(default = "default_keyboard_scroll_lines")]
    pub(crate) keyboard_scroll_lines: usize,
    #[serde(default = "default_mouse_scroll_lines")]
    pub(crate) mouse_scroll_lines: usize,
    #[serde(default = "default_line_wrapping")]
    pub(crate) line_wrapping: bool,
    #[serde(default = "default_horizontal_auto_scroll_speed")]
    pub(crate) horizontal_auto_scroll_speed: usize,
    #[serde(default = "default_horizontal_scroll_speed")]
    pub(crate) horizontal_scroll_speed: usize,
    #[serde(default = "default_appearance")]
    pub(crate) appearance: AppearanceSettings,
    #[serde(default = "default_max_menu_files")]
    pub(crate) max_menu_files: usize,
}

fn default_tab_width() -> usize {
    4
}
fn default_double_tap_speed_ms() -> u64 {
    300
}
fn default_cursor_shape() -> String {
    "bar".into()
}
fn default_keyboard_scroll_lines() -> usize {
    3
}
fn default_mouse_scroll_lines() -> usize {
    3
}
fn default_line_wrapping() -> bool {
    true
}
fn default_horizontal_auto_scroll_speed() -> usize {
    3
}
fn default_horizontal_scroll_speed() -> usize {
    5
}

fn default_max_menu_files() -> usize {
    5
}

fn default_line_number_digits() -> u8 {
    2
}
fn default_header_bg() -> String {
    "#001848".into()
}
fn default_footer_bg() -> String {
    "#001848".into()
}
fn default_line_numbers_bg() -> String {
    "#001848".into()
}
fn default_appearance() -> AppearanceSettings {
    AppearanceSettings {
        line_number_digits: default_line_number_digits(),
        header_bg: default_header_bg(),
        footer_bg: default_footer_bg(),
        line_numbers_bg: default_line_numbers_bg(),
        cursor_shape: default_cursor_shape(),
    }
}

impl Settings {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = Self::config_path()?;

        // Create directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // If config file doesn't exist, create it with defaults
        if !config_path.exists() {
            Self::write_default_config(&config_path)?;
        }

        // Read config (either existing or just created)
        let content = fs::read_to_string(&config_path)?;
        let settings: Settings = toml::from_str(&content)?;
        Ok(settings)
    }

    fn write_default_config(path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        const DEFAULT_CONFIG: &str = include_str!("../defaults/settings.toml");
        let mut file = fs::File::create(path)?;
        file.write_all(DEFAULT_CONFIG.as_bytes())?;
        Ok(())
    }

    fn config_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let home = std::env::var("UE_TEST_HOME")
            .or_else(|_| std::env::var("HOME"))
            .or_else(|_| std::env::var("USERPROFILE"))?;
        Ok(PathBuf::from(home).join(".ue").join("settings.toml"))
    }
}

impl Default for Settings {
    fn default() -> Self {
        const DEFAULT_CONFIG: &str = include_str!("../defaults/settings.toml");
        toml::from_str(DEFAULT_CONFIG).expect("default settings should be valid")
    }
}

impl Settings {
    /// Get tab width (for testing)
    #[allow(dead_code)]
    pub fn get_tab_width(&self) -> usize {
        self.tab_width
    }

    /// Get horizontal auto scroll speed (for testing)
    #[allow(dead_code)]
    pub fn get_horizontal_auto_scroll_speed(&self) -> usize {
        self.horizontal_auto_scroll_speed
    }
}

impl KeyBindings {
    pub fn quit_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.quit, code, modifiers)
    }
    pub fn copy_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.copy, code, modifiers)
    }
    pub fn paste_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.paste, code, modifiers)
    }
    pub fn cut_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.cut, code, modifiers)
    }
    pub fn close_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.close, code, modifiers)
    }
    pub fn save_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.save, code, modifiers)
    }
    pub fn undo_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.undo, code, modifiers)
    }
    pub fn redo_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.redo, code, modifiers)
    }
    pub fn find_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.find, code, modifiers)
    }
    pub fn find_next_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.find_next, code, modifiers)
    }
    pub fn find_previous_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.find_previous, code, modifiers)
    }
    pub fn replace_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.replace, code, modifiers)
    }
    pub fn replace_current_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.replace_current, code, modifiers)
    }
    pub fn replace_all_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.replace_all, code, modifiers)
    }
    pub fn goto_line_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.goto_line, code, modifiers)
    }
    pub fn save_and_quit_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.save_and_quit, code, modifiers)
    }
    pub fn toggle_line_wrap_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.toggle_line_wrap, code, modifiers)
    }
    pub fn cursor_down_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.cursor_down, code, modifiers)
    }
    pub fn cursor_up_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.cursor_up, code, modifiers)
    }
    pub fn cursor_left_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.cursor_left, code, modifiers)
    }
    pub fn cursor_right_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.cursor_right, code, modifiers)
    }
    pub fn numpad_enter_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.numpad_enter, code, modifiers)
    }

    pub fn new_file_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.new_file, code, modifiers)
    }

    pub fn open_dialog_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.open_dialog, code, modifiers)
    }

    pub fn help_matches(&self, key: &crossterm::event::KeyEvent) -> bool {
        parse_keybinding(&self.help, &key.code, &key.modifiers)
    }

    #[allow(dead_code)] // Used for custom keybindings, not in default double-Esc implementation
    pub fn file_selector_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
        parse_keybinding(&self.file_selector, code, modifiers)
    }
}

fn parse_keybinding(binding: &str, code: &KeyCode, modifiers: &KeyModifiers) -> bool {
    // Parse the binding string like "Ctrl+q" or "Alt+Shift+x" or "Esc"
    let parts: Vec<&str> = binding.split('+').map(|s| s.trim()).collect();

    if parts.is_empty() {
        return false;
    }

    // Last part is the key, everything else are modifiers
    let key = parts.last().unwrap().to_lowercase();
    let modifier_parts: Vec<&str> = parts[..parts.len() - 1].to_vec();

    // Check if the key matches
    let key_matches = match code {
        KeyCode::Char(c) => key == c.to_string().to_lowercase(),
        KeyCode::Esc => key == "esc" || key == "escape",
        KeyCode::Enter => key == "enter" || key == "return" || key == "numpadenter",
        KeyCode::Tab => key == "tab",
        KeyCode::Backspace => key == "backspace",
        KeyCode::Delete => key == "delete" || key == "del",
        KeyCode::F(n) => {
            // Match F1-F12 keys
            if let Some(num_str) = key.strip_prefix('f') {
                if let Ok(num) = num_str.parse::<u8>() {
                    num == *n
                } else {
                    false
                }
            } else {
                false
            }
        }
        _ => false,
    };

    if !key_matches {
        return false;
    }

    // Check if modifiers match
    let has_ctrl = modifiers.contains(KeyModifiers::CONTROL);
    let has_alt = modifiers.contains(KeyModifiers::ALT);
    let has_shift = modifiers.contains(KeyModifiers::SHIFT);

    let needs_ctrl = modifier_parts.iter().any(|m| {
        let m_lower = m.to_lowercase();
        m_lower == "ctrl" || m_lower == "control"
    });
    let needs_alt = modifier_parts.iter().any(|m| m.to_lowercase() == "alt");
    let needs_shift = modifier_parts.iter().any(|m| m.to_lowercase() == "shift");

    has_ctrl == needs_ctrl && has_alt == needs_alt && has_shift == needs_shift
}

impl Settings {
    pub(crate) fn parse_color(s: &str) -> Option<crossterm::style::Color> {
        use crossterm::style::Color;
        let name = s.trim().to_lowercase();
        match name.as_str() {
            "black" => Some(Color::Black),
            "blue" => Some(Color::Blue),
            "darkblue" => Some(Color::DarkBlue),
            "dark_grey" | "darkgrey" => Some(Color::DarkGrey),
            "grey" | "gray" => Some(Color::Grey),
            "white" => Some(Color::White),
            _ => {
                // Hex #RRGGBB
                if name.starts_with('#') && name.len() == 7 {
                    let r = u8::from_str_radix(&name[1..3], 16).ok()?;
                    let g = u8::from_str_radix(&name[3..5], 16).ok()?;
                    let b = u8::from_str_radix(&name[5..7], 16).ok()?;
                    Some(Color::Rgb { r, g, b })
                } else {
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::set_temp_home;
    use crossterm::event::{KeyCode, KeyModifiers}; // shared environment lock

    // Helper function to create test KeyBindings with default values
    fn create_test_keybindings() -> KeyBindings {
        KeyBindings {
            quit: "Esc".into(),
            copy: "Ctrl+c".into(),
            paste: "Ctrl+v".into(),
            cut: "Ctrl+x".into(),
            close: "Ctrl+w".into(),
            save: "Ctrl+s".into(),
            undo: "Ctrl+z".into(),
            redo: "Ctrl+y".into(),
            file_selector: "Esc".into(),
            open_dialog: "Ctrl+o".into(),
            find: "Ctrl+f".into(),
            find_next: "F3".into(),
            find_previous: "Shift+F3".into(),
            replace: "Ctrl+Shift+h".into(),
            replace_current: "Ctrl+r".into(),
            replace_all: "Ctrl+Alt+r".into(),
            goto_line: "Ctrl+g".into(),
            help: "F1".into(),
            save_and_quit: "Ctrl+q".into(),
            toggle_line_wrap: "Alt+w".into(),
            new_file: "Ctrl+n".into(),
            cursor_down: "Alt+j".into(),
            cursor_up: "Alt+k".into(),
            cursor_left: "Alt+h".into(),
            cursor_right: "Alt+l".into(),
            numpad_enter: "Ctrl+j".into(),
        }
    }

    #[test]
    fn ctrl_letter_matches() {
        let (_tmp, _guard) = set_temp_home();
        let kb = create_test_keybindings();
        assert!(kb.copy_matches(&KeyCode::Char('c'), &KeyModifiers::CONTROL));
        assert!(!kb.copy_matches(&KeyCode::Char('c'), &KeyModifiers::ALT));
    }

    #[test]
    fn esc_quit_variants() {
        let (_tmp, _guard) = set_temp_home();
        let kb = create_test_keybindings();
        assert!(kb.quit_matches(&KeyCode::Esc, &KeyModifiers::empty()));
        // file_selector no longer uses Esc - Esc opens menu instead
    }

    #[test]
    fn shift_modifier_detection() {
        let (_tmp, _guard) = set_temp_home();
        let mut kb = create_test_keybindings();
        kb.copy = "Ctrl+Shift+c".into();
        let mods = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
        assert!(kb.copy_matches(&KeyCode::Char('c'), &mods));
        let missing_shift = KeyModifiers::CONTROL;
        assert!(!kb.copy_matches(&KeyCode::Char('c'), &missing_shift));
    }

    #[test]
    fn default_settings_file_created() {
        let (_tmp, _guard) = set_temp_home();
        let settings = Settings::load().expect("load settings");
        assert_eq!(settings.appearance.line_number_digits, 3);
    }

    #[test]
    fn settings_default_creation_and_reload() {
        let (tmp, _guard) = set_temp_home();
        let settings_first = Settings::load().expect("first load");
        assert_eq!(settings_first.appearance.line_number_digits, 3);
        // Modify file to check reload
        let settings_path = tmp.path().join(".ue").join("settings.toml");
        let mut content = fs::read_to_string(&settings_path).unwrap();
        content = content.replace("line_number_digits = 3", "line_number_digits = 5");
        fs::write(&settings_path, content).unwrap();
        let settings_second = Settings::load().expect("second load");
        assert_eq!(settings_second.appearance.line_number_digits, 5);
    }

    #[test]
    fn double_esc_keybinding() {
        let (_tmp, _guard) = set_temp_home();
        let mut kb = create_test_keybindings();
        kb.quit = "Esc Esc".into();

        // Single Esc now opens menu (handled in UI layer), not file selector
        // file_selector keybinding is now empty/unused

        // Note: Double Esc detection is handled in the UI layer, not by keybinding parser
        // The quit keybinding "Esc Esc" is a special marker that the UI interprets
    }

    #[test]
    fn default_color_values_present() {
        let (_tmp, _guard) = crate::env::set_temp_home();
        let s = Settings::load().expect("load settings");
        assert_eq!(s.appearance.header_bg, "#001848");
        assert_eq!(s.appearance.footer_bg, "#001848");
        assert_eq!(s.appearance.line_numbers_bg, "#001848");
        assert!(Settings::parse_color(&s.appearance.header_bg).is_some());
    }

    #[test]
    fn parse_color_hex() {
        assert!(Settings::parse_color("#001848").is_some());
        assert!(Settings::parse_color("#ffffff").is_some());
        assert!(Settings::parse_color("#zzzzzz").is_none());
    }

    #[test]
    fn cursor_shape_default() {
        let (_tmp, _guard) = set_temp_home();
        let s = Settings::load().expect("load settings");
        assert_eq!(s.appearance.cursor_shape, "bar");
    }

    #[test]
    fn f_key_parsing() {
        let (_tmp, _guard) = set_temp_home();
        let kb = create_test_keybindings();

        // Test F3 for find next (no modifiers)
        assert!(kb.find_next_matches(&KeyCode::F(3), &KeyModifiers::empty()));

        // Test Shift+F3 for find previous
        assert!(kb.find_previous_matches(&KeyCode::F(3), &KeyModifiers::SHIFT));

        // Should not match with wrong modifiers
        assert!(!kb.find_next_matches(&KeyCode::F(3), &KeyModifiers::SHIFT));
        assert!(!kb.find_previous_matches(&KeyCode::F(3), &KeyModifiers::empty()));

        // Verify F3 without shift does NOT match find_previous
        assert!(!kb.find_previous_matches(&KeyCode::F(3), &KeyModifiers::empty()));
        // Verify Shift+F3 does NOT match find_next
        assert!(!kb.find_next_matches(&KeyCode::F(3), &KeyModifiers::SHIFT));
    }

    #[test]
    fn settings_color_validation() {
        // Test valid colors
        assert!(Settings::parse_color("#FF0000").is_some());
        assert!(Settings::parse_color("#00FF00").is_some());
        assert!(Settings::parse_color("#0000FF").is_some());
        assert!(Settings::parse_color("#123456").is_some());

        // Test invalid colors
        assert!(Settings::parse_color("").is_none());
        assert!(Settings::parse_color("FF0000").is_none()); // Missing #
        assert!(Settings::parse_color("#FF00").is_none()); // Too short
        assert!(Settings::parse_color("#FF00000").is_none()); // Too long
        assert!(Settings::parse_color("#GGGGGG").is_none()); // Invalid hex
        assert!(Settings::parse_color("rgb(255,0,0)").is_none()); // Wrong format
    }

    #[test]
    fn settings_tab_width_validation() {
        let settings = Settings::default();
        assert!(settings.tab_width > 0);
        assert!(settings.tab_width <= 16); // Reasonable max
    }

    #[test]
    fn settings_line_number_digits_validation() {
        let settings = Settings::default();
        // Should be 0 (disabled) or between 1 and 10
        assert!(settings.appearance.line_number_digits <= 10);
    }

    #[test]
    fn settings_cursor_shape_values() {
        let settings = Settings::default();
        let valid_shapes = ["bar", "block", "underline"];
        assert!(valid_shapes.contains(&settings.appearance.cursor_shape.to_lowercase().as_str()));
    }

    #[test]
    fn settings_all_keybindings_valid() {
        let settings = Settings::default();

        // Check that all keybindings are non-empty
        assert!(!settings.keybindings.quit.is_empty());
        // file_selector is now optional/empty - Esc opens menu instead
        assert!(!settings.keybindings.copy.is_empty());
        assert!(!settings.keybindings.paste.is_empty());
        assert!(!settings.keybindings.cut.is_empty());
        assert!(!settings.keybindings.close.is_empty());
        assert!(!settings.keybindings.save.is_empty());
        assert!(!settings.keybindings.undo.is_empty());
        assert!(!settings.keybindings.redo.is_empty());
        assert!(!settings.keybindings.find.is_empty());
        assert!(!settings.keybindings.find_next.is_empty());
        assert!(!settings.keybindings.find_previous.is_empty());
        assert!(!settings.keybindings.goto_line.is_empty());
    }

    #[test]
    fn goto_line_keybinding_matches() {
        let (_tmp, _guard) = set_temp_home();
        let kb = create_test_keybindings();

        // Test Ctrl+g matches goto_line
        assert!(kb.goto_line_matches(&KeyCode::Char('g'), &KeyModifiers::CONTROL));

        // Test wrong modifiers don't match
        assert!(!kb.goto_line_matches(&KeyCode::Char('g'), &KeyModifiers::empty()));
        assert!(!kb.goto_line_matches(&KeyCode::Char('g'), &KeyModifiers::ALT));
        assert!(!kb.goto_line_matches(&KeyCode::Char('g'), &KeyModifiers::SHIFT));
    }

    #[test]
    fn alt_modifier_parsing() {
        let (_tmp, _guard) = set_temp_home();
        let mut kb = create_test_keybindings();
        kb.copy = "Alt+c".into();

        // Should match Alt+c
        assert!(kb.copy_matches(&KeyCode::Char('c'), &KeyModifiers::ALT));

        // Should not match without Alt
        assert!(!kb.copy_matches(&KeyCode::Char('c'), &KeyModifiers::empty()));
        assert!(!kb.copy_matches(&KeyCode::Char('c'), &KeyModifiers::CONTROL));
    }

    #[test]
    fn case_insensitive_keybinding_parsing() {
        let (_tmp, _guard) = set_temp_home();
        let mut kb = create_test_keybindings();

        // Test uppercase modifiers work
        kb.copy = "CTRL+C".into();
        assert!(kb.copy_matches(&KeyCode::Char('c'), &KeyModifiers::CONTROL));

        // Test mixed case works
        kb.paste = "CtRl+V".into();
        assert!(kb.paste_matches(&KeyCode::Char('v'), &KeyModifiers::CONTROL));

        // Test uppercase key names work
        kb.quit = "ESC".into();
        assert!(kb.quit_matches(&KeyCode::Esc, &KeyModifiers::empty()));
    }

    #[test]
    fn invalid_keybinding_strings() {
        // Empty string should not match
        assert!(!parse_keybinding(
            "",
            &KeyCode::Char('a'),
            &KeyModifiers::empty()
        ));

        // Unknown key should not match
        assert!(!parse_keybinding(
            "Ctrl+unknown",
            &KeyCode::Char('a'),
            &KeyModifiers::CONTROL
        ));

        // Wrong key should not match
        assert!(!parse_keybinding(
            "Ctrl+b",
            &KeyCode::Char('a'),
            &KeyModifiers::CONTROL
        ));
    }

    #[test]
    fn named_colors_parsing() {
        // Test named colors
        assert!(Settings::parse_color("black").is_some());
        assert!(Settings::parse_color("blue").is_some());
        assert!(Settings::parse_color("darkblue").is_some());
        assert!(Settings::parse_color("dark_grey").is_some());
        assert!(Settings::parse_color("darkgrey").is_some());
        assert!(Settings::parse_color("grey").is_some());
        assert!(Settings::parse_color("gray").is_some());
        assert!(Settings::parse_color("white").is_some());

        // Test case insensitivity
        assert!(Settings::parse_color("BLACK").is_some());
        assert!(Settings::parse_color("Blue").is_some());
        assert!(Settings::parse_color("DARKBLUE").is_some());

        // Test whitespace trimming
        assert!(Settings::parse_color("  blue  ").is_some());
        assert!(Settings::parse_color("\tblack\t").is_some());
    }

    #[test]
    fn control_variants_parsing() {
        let (_tmp, _guard) = set_temp_home();
        let mut kb = create_test_keybindings();

        // Test "ctrl" variant
        kb.copy = "ctrl+c".into();
        assert!(kb.copy_matches(&KeyCode::Char('c'), &KeyModifiers::CONTROL));

        // Test "control" variant
        kb.paste = "control+v".into();
        assert!(kb.paste_matches(&KeyCode::Char('v'), &KeyModifiers::CONTROL));
    }

    #[test]
    fn multiple_modifier_combinations() {
        let (_tmp, _guard) = set_temp_home();
        let mut kb = create_test_keybindings();

        // Test Ctrl+Alt combination
        kb.copy = "Ctrl+Alt+c".into();
        let ctrl_alt = KeyModifiers::CONTROL | KeyModifiers::ALT;
        assert!(kb.copy_matches(&KeyCode::Char('c'), &ctrl_alt));
        assert!(!kb.copy_matches(&KeyCode::Char('c'), &KeyModifiers::CONTROL));
        assert!(!kb.copy_matches(&KeyCode::Char('c'), &KeyModifiers::ALT));

        // Test Ctrl+Shift+Alt combination
        kb.paste = "Ctrl+Shift+Alt+v".into();
        let all_mods = KeyModifiers::CONTROL | KeyModifiers::SHIFT | KeyModifiers::ALT;
        assert!(kb.paste_matches(&KeyCode::Char('v'), &all_mods));
        assert!(!kb.paste_matches(&KeyCode::Char('v'), &ctrl_alt));
    }

    #[test]
    fn special_keys_parsing() {
        let (_tmp, _guard) = set_temp_home();
        let mut kb = create_test_keybindings();

        // Test Enter key
        kb.copy = "enter".into();
        assert!(kb.copy_matches(&KeyCode::Enter, &KeyModifiers::empty()));

        // Test Return alias
        kb.paste = "return".into();
        assert!(kb.paste_matches(&KeyCode::Enter, &KeyModifiers::empty()));

        // Test NumpadEnter alias
        kb.cut = "numpadenter".into();
        assert!(kb.cut_matches(&KeyCode::Enter, &KeyModifiers::empty()));

        // Test Tab key
        kb.undo = "tab".into();
        assert!(kb.undo_matches(&KeyCode::Tab, &KeyModifiers::empty()));

        // Test Backspace key
        kb.redo = "backspace".into();
        assert!(kb.redo_matches(&KeyCode::Backspace, &KeyModifiers::empty()));

        // Test Delete key
        kb.close = "delete".into();
        assert!(kb.close_matches(&KeyCode::Delete, &KeyModifiers::empty()));

        // Test Del alias
        kb.save = "del".into();
        assert!(kb.save_matches(&KeyCode::Delete, &KeyModifiers::empty()));
    }
    #[test]
    fn test_missing_help_field_uses_default() {
        let toml_without_help = r#"
quit = "Esc Esc"
copy = "Ctrl+c"
paste = "Ctrl+v"
cut = "Ctrl+x"
close = "Ctrl+w"
save = "Ctrl+s"
undo = "Ctrl+z"
redo = "Ctrl+y"
file_selector = "Esc"
find = "Ctrl+f"
find_next = "Ctrl+n"
find_previous = "Ctrl+p"
goto_line = "Ctrl+b"
"#;
        let kb: KeyBindings = toml::from_str(toml_without_help).expect("should parse with default");
        assert_eq!(kb.help, "F1", "help field should default to F1");
    }
}
