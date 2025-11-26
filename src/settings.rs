use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, io::Write};
use crossterm::event::{KeyCode, KeyModifiers};

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
    pub(crate) file_selector: String,
    pub(crate) find: String,
    pub(crate) find_next: String,
    pub(crate) find_previous: String,
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
pub(crate) struct Settings {
    pub(crate) keybindings: KeyBindings,
    #[serde(default = "default_tab_width")]
    pub(crate) tab_width: usize,
    #[serde(default = "default_double_tap_speed_ms")]
    pub(crate) double_tap_speed_ms: u64,
    #[serde(default = "default_keyboard_scroll_lines")]
    pub(crate) keyboard_scroll_lines: usize,
    #[serde(default = "default_mouse_scroll_lines")]
    pub(crate) mouse_scroll_lines: usize,
    #[serde(default = "default_appearance")]
    pub(crate) appearance: AppearanceSettings,
}

fn default_tab_width() -> usize { 4 }
fn default_double_tap_speed_ms() -> u64 { 300 }
fn default_cursor_shape() -> String { "bar".into() }
fn default_keyboard_scroll_lines() -> usize { 3 }
fn default_mouse_scroll_lines() -> usize { 3 }
fn default_line_number_digits() -> u8 { 3 }
fn default_header_bg() -> String { "#001848".into() }
fn default_footer_bg() -> String { "#001848".into() }
fn default_line_numbers_bg() -> String { "#001848".into() }
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


impl KeyBindings {
    pub fn quit_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool { parse_keybinding(&self.quit, code, modifiers) }
    pub fn copy_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool { parse_keybinding(&self.copy, code, modifiers) }
    pub fn paste_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool { parse_keybinding(&self.paste, code, modifiers) }
    pub fn cut_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool { parse_keybinding(&self.cut, code, modifiers) }
    pub fn close_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool { parse_keybinding(&self.close, code, modifiers) }
    pub fn save_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool { parse_keybinding(&self.save, code, modifiers) }
    pub fn undo_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool { parse_keybinding(&self.undo, code, modifiers) }
    pub fn redo_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool { parse_keybinding(&self.redo, code, modifiers) }
    pub fn find_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool { parse_keybinding(&self.find, code, modifiers) }
    pub fn find_next_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool { parse_keybinding(&self.find_next, code, modifiers) }
    pub fn find_previous_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool { parse_keybinding(&self.find_previous, code, modifiers) }
    
    #[allow(dead_code)] // Used for custom keybindings, not in default double-Esc implementation
    pub fn file_selector_matches(&self, code: &KeyCode, modifiers: &KeyModifiers) -> bool { parse_keybinding(&self.file_selector, code, modifiers) }
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
        KeyCode::Enter => key == "enter" || key == "return",
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
                } else { None }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};
    use crate::env::set_temp_home; // shared environment lock

    #[test]
    fn ctrl_letter_matches() {
        let (_tmp, _guard) = set_temp_home();
        let kb = KeyBindings { quit:"Esc".into(), copy:"Ctrl+c".into(), paste:"Ctrl+v".into(), cut:"Ctrl+x".into(), close:"Ctrl+w".into(), save:"Ctrl+s".into(), undo:"Ctrl+z".into(), redo:"Ctrl+y".into(), file_selector:"Esc".into(), find:"Ctrl+f".into(), find_next:"F3".into(), find_previous:"Shift+F3".into() };
        assert!(kb.copy_matches(&KeyCode::Char('c'), &KeyModifiers::CONTROL));
        assert!(!kb.copy_matches(&KeyCode::Char('c'), &KeyModifiers::ALT));
    }

    #[test]
    fn esc_quit_variants() {
        let (_tmp, _guard) = set_temp_home();
        let kb = KeyBindings { quit:"Escape".into(), copy:"Ctrl+c".into(), paste:"Ctrl+v".into(), cut:"Ctrl+x".into(), close:"Ctrl+w".into(), save:"Ctrl+s".into(), undo:"Ctrl+z".into(), redo:"Ctrl+y".into(), file_selector:"Esc".into(), find:"Ctrl+f".into(), find_next:"F3".into(), find_previous:"Shift+F3".into() };
        assert!(kb.quit_matches(&KeyCode::Esc, &KeyModifiers::empty()));
        assert!(kb.file_selector_matches(&KeyCode::Esc, &KeyModifiers::empty()));
    }

    #[test]
    fn shift_modifier_detection() {
        let (_tmp, _guard) = set_temp_home();
        let kb = KeyBindings { quit:"Esc".into(), copy:"Ctrl+Shift+c".into(), paste:"Ctrl+v".into(), cut:"Ctrl+x".into(), close:"Ctrl+w".into(), save:"Ctrl+s".into(), undo:"Ctrl+z".into(), redo:"Ctrl+y".into(), file_selector:"Esc".into(), find:"Ctrl+f".into(), find_next:"F3".into(), find_previous:"Shift+F3".into() };
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
        content = content.replace("line_number_digits = 3", "line_number_digits = 2");
        fs::write(&settings_path, content).unwrap();
        let settings_second = Settings::load().expect("second load");
        assert_eq!(settings_second.appearance.line_number_digits, 2);
    }

    #[test]
    fn double_esc_keybinding() {
        let (_tmp, _guard) = set_temp_home();
        let kb = KeyBindings { 
            quit: "Esc Esc".into(), 
            copy: "Ctrl+c".into(), 
            paste: "Ctrl+v".into(), 
            cut: "Ctrl+x".into(), 
            close: "Ctrl+w".into(), 
            save: "Ctrl+s".into(), 
            undo: "Ctrl+z".into(), 
            redo: "Ctrl+y".into(), 
            file_selector: "Esc".into(),
            find: "Ctrl+f".into(),
            find_next: "F3".into(),
            find_previous: "Shift+F3".into(),
        };
        
        // Esc without modifiers should open file selector
        assert!(kb.file_selector_matches(&KeyCode::Esc, &KeyModifiers::empty()));
        
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
        let kb = KeyBindings {
            quit: "Esc".into(),
            copy: "Ctrl+c".into(),
            paste: "Ctrl+v".into(),
            cut: "Ctrl+x".into(),
            close: "Ctrl+w".into(),
            save: "Ctrl+s".into(),
            undo: "Ctrl+z".into(),
            redo: "Ctrl+y".into(),
            file_selector: "Esc".into(),
            find: "Ctrl+f".into(),
            find_next: "F3".into(),
            find_previous: "Shift+F3".into(),
        };
        
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
}
