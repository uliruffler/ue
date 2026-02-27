use crossterm::event::{KeyCode, KeyEvent};
use std::path::PathBuf;

/// Help page content for different contexts
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HelpContext {
    Editor,
    Find,
}

/// Return the absolute path to the deployed help file for a given context.
/// The file lives in `~/.ue/help/<name>.md`.
pub fn get_help_file_path(context: HelpContext) -> Option<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()?;
    let name = match context {
        HelpContext::Editor => "editor.md",
        HelpContext::Find => "find.md",
    };
    Some(PathBuf::from(home).join(".ue").join("help").join(name))
}

/// Deploy help files to `~/.ue/help/` with keybinding placeholders replaced.
/// Always overwrites existing files so keybinding changes take effect immediately.
pub fn deploy_help_files(settings: &crate::settings::Settings) {
    let home = match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        Ok(h) => h,
        Err(_) => return,
    };
    let help_dir = PathBuf::from(&home).join(".ue").join("help");
    if std::fs::create_dir_all(&help_dir).is_err() {
        return;
    }

    let files: &[(&str, &str)] = &[
        ("editor.md", include_str!("../defaults/help-editor.md")),
        ("find.md", include_str!("../defaults/help-find.md")),
        ("file-selector.md", include_str!("../defaults/help-file-selector.md")),
        ("open-dialog.md", include_str!("../defaults/help-open-dialog.md")),
    ];

    for (name, content) in files {
        let path = help_dir.join(name);
        let replaced = replace_keybindings(content, settings);
        let _ = std::fs::write(&path, replaced);
    }
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
        .replace("{replace}", &settings.keybindings.replace)
        .replace("{replace_current}", &settings.keybindings.replace_current)
        .replace("{replace_all}", &settings.keybindings.replace_all)
        .replace("{save}", &settings.keybindings.save)
        .replace("{close}", &settings.keybindings.close)
        .replace("{quit}", &settings.keybindings.quit)
        .replace("{toggle_line_wrap}", &settings.keybindings.toggle_line_wrap)
        .replace("{render_toggle}", &settings.keybindings.render_toggle)
        .replace(
            "{double_tap_speed_ms}",
            &settings.double_tap_speed_ms.to_string(),
        )
}

/// Compute the render width for markdown, given the full terminal width and editor state.
/// The gutter (line-number digits + separator) and the scrollbar column are subtracted so
/// that rendered text fits exactly in the available content area.
pub(crate) fn markdown_render_width(term_width: usize, state: &crate::editor_state::FileViewerState, line_count: usize) -> usize {
    let gutter = crate::coordinates::line_number_display_width(state.settings, line_count) as usize;
    let scrollbar = 1;
    term_width.saturating_sub(gutter + scrollbar)
}

/// Render markdown document content to display lines using the configured renderer.
/// No keybinding substitution is performed — the content is rendered as-is.
/// Suitable for showing `.md` files in "rendered" view mode.
///
/// Returns a `Vec<String>` of display lines that may contain ANSI escape codes.
pub(crate) fn render_markdown_to_lines(content_lines: &[String], term_width: usize) -> Vec<String> {
    let content = content_lines.join("\n");
    crate::markdown_renderer::default_renderer().render(&content, term_width)
}

/// Load and format help content from a markdown string.
/// Replaces keybinding placeholders, then renders to terminal display lines.
fn load_help_from_md(
    content: &str,
    settings: &crate::settings::Settings,
    term_width: usize,
) -> Vec<String> {
    let replaced = replace_keybindings(content, settings);
    crate::markdown_renderer::default_renderer().render(&replaced, term_width)
}

/// Public re-export of `replace_keybindings` for use in tests inside other modules.
#[cfg(test)]
pub(crate) fn replace_keybindings_pub(content: &str, settings: &crate::settings::Settings) -> String {
    replace_keybindings(content, settings)
}

/// Get help content for the given context (used in tests; production now opens deployed files)
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn get_help_content(
    context: HelpContext,
    settings: &crate::settings::Settings,
    term_width: usize,
) -> Vec<String> {
    match context {
        HelpContext::Editor => load_help_from_md(
            include_str!("../defaults/help-editor.md"),
            settings,
            term_width,
        ),
        HelpContext::Find => load_help_from_md(
            include_str!("../defaults/help-find.md"),
            settings,
            term_width,
        ),
    }
}


/// Get help content for open dialog
pub(crate) fn get_open_dialog_help(
    settings: &crate::settings::Settings,
    term_width: usize,
) -> Vec<String> {
    load_help_from_md(
        include_str!("../defaults/help-open-dialog.md"),
        settings,
        term_width,
    )
}

/// Truncate a rendered (ANSI-escaped) line to fit within `max_width` visual columns.
/// ANSI escape sequences are counted as zero-width.
/// This is the same logic as the private `truncate_to_width` but exposed for use by
/// `rendering.rs` when displaying rendered markdown in the editor content area.
pub(crate) fn truncate_rendered_line(text: &str, max_width: usize) -> String {
    truncate_to_width(text, max_width)
}

/// Truncate a string to a maximum display width, handling UTF-8 and ANSI escape codes
/// Returns a string that fits within the width without breaking multi-byte characters
fn truncate_to_width(text: &str, max_width: usize) -> String {
    let mut visual_width = 0;
    let mut result = String::new();
    let mut chars = text.chars().peekable();
    let mut in_escape = false;

    while let Some(ch) = chars.next() {
        // Handle ANSI escape sequences (they don't contribute to visual width)
        if ch == '\x1b' {
            in_escape = true;
            result.push(ch);
            continue;
        }

        if in_escape {
            result.push(ch);
            // ANSI escape sequences end with a letter (simplified check)
            if ch.is_ascii_alphabetic() {
                in_escape = false;
            }
            continue;
        }

        // Count visual width (simplified - treats all chars as width 1)
        // In reality, we'd need unicode-width crate for accurate width
        visual_width += 1;

        if visual_width > max_width {
            break;
        }

        result.push(ch);
    }

    result
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
    use crossterm::{cursor, execute, style::ResetColor, terminal};

    execute!(stdout, cursor::Hide)?;
    execute!(stdout, cursor::MoveTo(0, 0))?;
    execute!(stdout, terminal::Clear(terminal::ClearType::All))?;

    let visible_lines = (term_height as usize).saturating_sub(1); // Leave room for footer

    // Render help content
    for (i, line) in help_lines
        .iter()
        .skip(scroll_offset)
        .take(visible_lines)
        .enumerate()
    {
        execute!(stdout, cursor::MoveTo(0, i as u16))?;
        // Truncate line if too wide - use char-aware truncation to avoid UTF-8 boundary errors
        let display_line = truncate_to_width(line, term_width as usize);
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
        format!(
            " Line {}/{} - Use Up/Down to scroll, ESC/F1 to close ",
            scroll_offset.min(help_lines.len().saturating_sub(visible_lines)) + 1,
            help_lines.len().saturating_sub(visible_lines).max(1)
        )
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
        let term_width = 80;
        let editor_help = get_help_content(HelpContext::Editor, &settings, term_width);
        assert!(!editor_help.is_empty());
        assert!(
            editor_help
                .iter()
                .any(|line| line.contains("Navigation") || line.contains("NAVIGATION"))
        );

        let find_help = get_help_content(HelpContext::Find, &settings, term_width);
        assert!(!find_help.is_empty());
        assert!(
            find_help
                .iter()
                .any(|line| line.contains("Find Mode") || line.contains("FIND MODE"))
        );
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
        let term_width = 80;

        // Editor help should be loaded and rendered from markdown
        let editor_help = get_help_content(HelpContext::Editor, &settings, term_width);
        // After rendering, raw markdown markers should not be present but content should be formatted
        assert!(editor_help.iter().any(|line| line.contains("Navigation")));

        // Find help should be loaded and rendered from markdown
        let find_help = get_help_content(HelpContext::Find, &settings, term_width);
        assert!(find_help.iter().any(|line| line.contains("Find Mode")));
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
        let term_width = 80;
        let editor_help = get_help_content(HelpContext::Editor, &settings, term_width);

        // Verify that placeholders were replaced with actual keybindings
        assert!(
            editor_help
                .iter()
                .any(|line| line.contains(&settings.keybindings.help))
        );
        assert!(
            editor_help
                .iter()
                .any(|line| line.contains(&settings.keybindings.find))
        );
        assert!(
            editor_help
                .iter()
                .any(|line| line.contains(&settings.keybindings.save))
        );

        // Verify no placeholders remain
        assert!(!editor_help.iter().any(|line| line.contains("{help}")));
        assert!(!editor_help.iter().any(|line| line.contains("{find}")));
        assert!(!editor_help.iter().any(|line| line.contains("{save}")));
    }

    #[test]
    fn test_tables_are_rendered() {
        let settings = Default::default();
        let term_width = 80;

        // Test editor help tables
        let editor_help = get_help_content(HelpContext::Editor, &settings, term_width);
        let help_text = editor_help.join("\n");

        // Tables should be rendered with content (not raw markdown pipes)
        // Look for table content that appears in help files
        assert!(help_text.contains("Arrow Keys") || help_text.contains("Move cursor"));
        assert!(help_text.contains("Navigation"));
        assert!(help_text.contains("Editing"));

        // Verify table content is present and formatted
        // The table should have multiple lines with content from the markdown tables
        let navigation_section = editor_help
            .iter()
            .skip_while(|line| !line.contains("Navigation"))
            .take(15)
            .collect::<Vec<_>>();
        assert!(
            !navigation_section.is_empty(),
            "Navigation section should exist"
        );

        // Test find help tables
        let find_help = get_help_content(HelpContext::Find, &settings, term_width);
        let find_text = find_help.join("\n");
        assert!(
            find_text.contains("Regex Examples")
                || find_text.contains("Pattern")
                || find_text.contains("Matches")
        );
    }

    #[test]
    fn test_table_content_is_formatted() {
        let settings = Default::default();
        let term_width = 100;

        // Get editor help which contains multiple tables
        let editor_help = get_help_content(HelpContext::Editor, &settings, term_width);

        // Verify specific table entries are present
        // These are from the Navigation table in help-editor.md
        assert!(editor_help.iter().any(|line| line.contains("Arrow Keys")));
        assert!(
            editor_help
                .iter()
                .any(|line| line.contains("Home") && line.contains("End"))
        );
        assert!(editor_help.iter().any(|line| line.contains("Page")));

        // Verify no raw markdown table syntax remains (| --- | --- |)
        assert!(!editor_help.iter().any(|line| line.contains("|--")));
        assert!(!editor_help.iter().any(|line| line.contains("--|")));
    }

    #[test]
    fn test_tables_respect_terminal_width() {
        let settings = Default::default();

        // Test with narrow terminal - the content should be rendered
        let narrow_help = get_help_content(HelpContext::Editor, &settings, 40);
        assert!(
            !narrow_help.is_empty(),
            "Help should have content even with narrow terminal"
        );

        // Test with wide terminal
        let wide_help = get_help_content(HelpContext::Editor, &settings, 120);
        assert!(
            !wide_help.is_empty(),
            "Help should have content with wide terminal"
        );

        // Both should contain the same basic content
        let narrow_text = narrow_help.join("\n");
        let wide_text = wide_help.join("\n");
        assert!(narrow_text.contains("Navigation") || narrow_text.contains("Help"));
        assert!(wide_text.contains("Navigation") || wide_text.contains("Help"));
    }

    #[test]
    fn test_table_rendering_in_all_help_contexts() {
        let settings = Default::default();
        let term_width = 80;

        // Editor help should have multiple tables
        let editor_help = get_help_content(HelpContext::Editor, &settings, term_width);
        assert!(editor_help.iter().any(|line| line.contains("Navigation")));
        assert!(editor_help.iter().any(|line| line.contains("Editing")));
        assert!(editor_help.iter().any(|line| line.contains("Selection")));
        assert!(editor_help.iter().any(|line| line.contains("Search")));

        // Find help should have tables
        let find_help = get_help_content(HelpContext::Find, &settings, term_width);
        assert!(
            find_help
                .iter()
                .any(|line| line.contains("Basic Usage") || line.contains("Find Mode"))
        );
    }

    #[test]
    fn test_markdown_table_formatting_sample() {
        // This test demonstrates that tables are properly formatted
        let settings = Default::default();
        let term_width = 80;

        let editor_help = get_help_content(HelpContext::Editor, &settings, term_width);
        let output = editor_help.join("\n");

        // Verify that markdown table delimiters are not present
        assert!(
            !output.contains("| Key | Action |"),
            "Raw table header should be rendered"
        );
        assert!(
            !output.contains("|-----|--------|"),
            "Table separator should be rendered"
        );

        // Verify actual table content is present (these are from the markdown)
        assert!(
            output.contains("Arrow Keys"),
            "Table content should be present"
        );
        assert!(
            output.contains("Move cursor") || output.contains("cursor"),
            "Table actions should be present"
        );

        // Print a sample for manual verification (visible with --nocapture)
        eprintln!("\n=== Sample of rendered help (first 30 lines) ===");
        for (i, line) in editor_help.iter().take(30).enumerate() {
            eprintln!("{:3}: {}", i + 1, line);
        }
        eprintln!("=== End of sample ===\n");
    }

    #[test]
    fn test_truncate_to_width_handles_utf8() {
        // Test with box-drawing characters (3 bytes each in UTF-8)
        let text = "├────────────┼──────────────────┤";
        let truncated = truncate_to_width(text, 10);
        // Should not panic and should produce valid UTF-8
        assert!(truncated.len() <= 30); // 10 chars * 3 bytes max
        assert!(truncated.is_empty() || truncated.chars().count() <= 10);
    }

    #[test]
    fn test_truncate_to_width_handles_ascii() {
        let text = "Hello, World!";
        let truncated = truncate_to_width(text, 5);
        assert_eq!(truncated, "Hello");
    }

    #[test]
    fn test_truncate_to_width_preserves_ansi_codes() {
        // Text with ANSI color codes
        let text = "\x1b[1mBold Text\x1b[0m";
        let truncated = truncate_to_width(text, 5);
        // Should preserve ANSI codes and truncate visible text
        assert!(truncated.contains("\x1b[1m"), "Should preserve ANSI codes");
        assert!(truncated.contains("Bold"), "Should have some content");
    }

    #[test]
    fn test_truncate_to_width_no_truncation_needed() {
        let text = "Short";
        let truncated = truncate_to_width(text, 100);
        assert_eq!(truncated, text);
    }

    #[test]
    fn test_truncate_to_width_empty_string() {
        let text = "";
        let truncated = truncate_to_width(text, 10);
        assert_eq!(truncated, "");
    }

    #[test]
    fn test_truncate_to_width_unicode_emoji() {
        let text = "Hello 👋 World 🌍";
        let truncated = truncate_to_width(text, 8);
        // Should handle multi-byte unicode without panicking
        assert!(truncated.len() > 0);
        assert!(truncated.chars().count() <= 8);
    }
}
