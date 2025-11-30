use crossterm::event::{KeyCode, KeyEvent};

/// Help page content for different contexts
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum HelpContext {
    Editor,
    Find,
}

/// Replace keybinding placeholders with actual values from settings
fn replace_keybindings(content: &str, settings: &crate::settings::Settings) -> String {
    content
        .replace("{help}", &settings.keybindings.help)
        .replace("{goto_line}", &settings.keybindings.goto_line)
        .replace("{undo}", &settings.keybindings.undo)
        .replace("{redo}", &settings.keybindings.redo)
        .replace("{copy}", &settings.keybindings.copy)
        .replace("{cut}", &settings.keybindings.cut)
        .replace("{paste}", &settings.keybindings.paste)
        .replace("{find}", &settings.keybindings.find)
        .replace("{find_next}", &settings.keybindings.find_next)
        .replace("{find_previous}", &settings.keybindings.find_previous)
        .replace("{save}", &settings.keybindings.save)
        .replace("{close}", &settings.keybindings.close)
        .replace("{quit}", &settings.keybindings.quit)
        .replace("{double_tap_speed_ms}", &settings.double_tap_speed_ms.to_string())
}

/// Load and format help content from markdown file
fn load_help_from_md(content: &str, settings: &crate::settings::Settings) -> Vec<String> {
    let replaced = replace_keybindings(content, settings);
    replaced.lines().map(|line| line.to_string()).collect()
}

/// Get help content for the given context
pub(crate) fn get_help_content(context: HelpContext, settings: &crate::settings::Settings) -> Vec<String> {
    match context {
        HelpContext::Editor => load_help_from_md(include_str!("../defaults/help-editor.md"), settings),
        HelpContext::Find => load_help_from_md(include_str!("../defaults/help-find.md"), settings),
    }
}

/// Get help content for file selector
pub(crate) fn get_file_selector_help(settings: &crate::settings::Settings) -> Vec<String> {
    load_help_from_md(include_str!("../defaults/help-file-selector.md"), settings)
}

/// Handle help mode key events
/// Returns true if help mode should exit
pub(crate) fn handle_help_input(key_event: KeyEvent) -> bool {
    let KeyEvent { code, .. } = key_event;
    
    match code {
        KeyCode::Esc | KeyCode::F(1) => true,
        _ => false,
    }
}

/// Render help screen
pub(crate) fn render_help(
    stdout: &mut impl std::io::Write,
    help_lines: &[String],
    scroll_offset: usize,
    term_width: u16,
    term_height: u16,
) -> Result<(), std::io::Error> {
    use crossterm::{cursor, execute, terminal, style::ResetColor};
    
    execute!(stdout, cursor::Hide)?;
    execute!(stdout, cursor::MoveTo(0, 0))?;
    execute!(stdout, terminal::Clear(terminal::ClearType::All))?;
    
    let visible_lines = (term_height as usize).saturating_sub(1); // Leave room for footer
    
    // Render help content
    for (i, line) in help_lines.iter().skip(scroll_offset).take(visible_lines).enumerate() {
        execute!(stdout, cursor::MoveTo(0, i as u16))?;
        // Truncate line if too wide
        let display_line = if line.len() > term_width as usize {
            &line[..term_width as usize]
        } else {
            line
        };
        write!(stdout, "{}", display_line)?;
        execute!(stdout, terminal::Clear(terminal::ClearType::UntilNewLine))?;
    }
    
    // Clear remaining lines
    for i in help_lines.len().saturating_sub(scroll_offset)..visible_lines {
        execute!(stdout, cursor::MoveTo(0, i as u16))?;
        execute!(stdout, terminal::Clear(terminal::ClearType::UntilNewLine))?;
    }
    
    // Render footer
    execute!(stdout, cursor::MoveTo(0, term_height - 1))?;
    let footer = if help_lines.len() > visible_lines {
        format!(" Line {}/{} - Use Up/Down to scroll, ESC/F1 to close ", 
                scroll_offset.min(help_lines.len().saturating_sub(visible_lines)) + 1,
                help_lines.len().saturating_sub(visible_lines).max(1))
    } else {
        " Press ESC or F1 to close help ".to_string()
    };
    write!(stdout, "{}", footer)?;
    execute!(stdout, terminal::Clear(terminal::ClearType::UntilNewLine))?;
    execute!(stdout, ResetColor)?;
    
    stdout.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_contexts_have_content() {
        let settings = Default::default();
        let editor_help = get_help_content(HelpContext::Editor, &settings);
        assert!(!editor_help.is_empty());
        assert!(editor_help.iter().any(|line| line.contains("Navigation") || line.contains("NAVIGATION")));
        
        let find_help = get_help_content(HelpContext::Find, &settings);
        assert!(!find_help.is_empty());
        assert!(find_help.iter().any(|line| line.contains("Find Mode") || line.contains("FIND MODE")));
    }

    #[test]
    fn test_file_selector_help_has_content() {
        let settings = Default::default();
        let selector_help = get_file_selector_help(&settings);
        assert!(!selector_help.is_empty());
        assert!(selector_help.iter().any(|line| line.contains("File Selector") || line.contains("FILE SELECTOR")));
    }

    #[test]
    fn test_help_input_handling() {
        // ESC should exit help
        let esc_event = KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE);
        assert!(handle_help_input(esc_event));
        
        // F1 should exit help
        let f1_event = KeyEvent::new(KeyCode::F(1), crossterm::event::KeyModifiers::NONE);
        assert!(handle_help_input(f1_event));
        
        // Other keys should not exit help
        let other_event = KeyEvent::new(KeyCode::Char('a'), crossterm::event::KeyModifiers::NONE);
        assert!(!handle_help_input(other_event));
    }
    
    #[test]
    fn test_help_loads_from_markdown_files() {
        let settings = Default::default();
        // Editor help should be loaded from markdown
        let editor_help = get_help_content(HelpContext::Editor, &settings);
        // Check for markdown-style headers or table separators
        assert!(editor_help.iter().any(|line| line.starts_with('#') || line.contains('|')));
        
        // Find help should be loaded from markdown
        let find_help = get_help_content(HelpContext::Find, &settings);
        assert!(find_help.iter().any(|line| line.starts_with('#') || line.contains('|')));
        
        // File selector help should be loaded from markdown
        let selector_help = get_file_selector_help(&settings);
        assert!(selector_help.iter().any(|line| line.starts_with('#') || line.contains('|')));
    }
    
    #[test]
    fn test_help_exit_with_esc_only_closes_help() {
        // This test verifies that ESC in help mode only closes help
        // The actual prevention of file selector triggering is tested in ui module
        let esc_event = KeyEvent::new(KeyCode::Esc, crossterm::event::KeyModifiers::NONE);
        assert!(handle_help_input(esc_event), "ESC should signal help exit");
    }
    
    #[test]
    fn test_help_exit_with_f1_only_closes_help() {
        // This test verifies that F1 in help mode only closes help
        let f1_event = KeyEvent::new(KeyCode::F(1), crossterm::event::KeyModifiers::NONE);
        assert!(handle_help_input(f1_event), "F1 should signal help exit");
    }
    
    #[test]
    fn test_keybinding_replacement() {
        let settings = Default::default();
        let editor_help = get_help_content(HelpContext::Editor, &settings);
        
        // Verify that placeholders were replaced with actual keybindings
        assert!(editor_help.iter().any(|line| line.contains(&settings.keybindings.help)));
        assert!(editor_help.iter().any(|line| line.contains(&settings.keybindings.find)));
        assert!(editor_help.iter().any(|line| line.contains(&settings.keybindings.save)));
        
        // Verify no placeholders remain
        assert!(!editor_help.iter().any(|line| line.contains("{help}")));
        assert!(!editor_help.iter().any(|line| line.contains("{find}")));
        assert!(!editor_help.iter().any(|line| line.contains("{save}")));
    }
}


