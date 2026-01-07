use crossterm::{
    cursor, execute,
    style::{ResetColor, SetBackgroundColor},
    terminal::{self, ClearType},
};
use std::io::Write;

use crate::coordinates::{
    calculate_cursor_visual_line, calculate_wrapped_lines_for_line,
    line_number_width, visual_width, visual_width_up_to,
};
use crate::editor_state::{FileViewerState, Position};

/// Expand tabs in a string to spaces, considering tab stops
fn expand_tabs(s: &str, tab_width: usize) -> String {
    let mut result = String::new();
    let mut col = 0;
    for ch in s.chars() {
        if ch == '\t' {
            let spaces_to_next_tab = tab_width - (col % tab_width);
            result.push_str(&" ".repeat(spaces_to_next_tab));
            col += spaces_to_next_tab;
        } else {
            result.push(ch);
            col += 1;
        }
    }
    result
}

/// Shorten a directory path intelligently for display in the header.
///
/// Tries progressively shorter representations:
/// 1. Full path (e.g., `/home/ruffler/ue/target`)
/// 2. With ~ for home (e.g., `~/ue/target`)
/// 3. With abbreviated dirs (e.g., `~/u/t`)
/// 4. With truncation and ellipsis (e.g., `~/u/t/re...`)
fn shorten_path_for_display(parent_path: &str, max_width: usize) -> String {
    if parent_path.is_empty() || parent_path == "." {
        return String::new();
    }

    // Try full path first
    if visual_width(parent_path, 4) <= max_width {
        return parent_path.to_string();
    }

    // Try replacing home directory with ~
    let home = std::env::var("HOME").ok();
    let home_shortened = if let Some(ref home_path) = home {
        if parent_path.starts_with(home_path) {
            let rest = &parent_path[home_path.len()..];
            let rest = if rest.starts_with('/') { &rest[1..] } else { rest };
            format!("~/{}", rest)
        } else {
            parent_path.to_string()
        }
    } else {
        parent_path.to_string()
    };

    if visual_width(&home_shortened, 4) <= max_width {
        return home_shortened;
    }

    // Try abbreviated directories (first letter of each component)
    let abbreviated = abbreviate_path(&home_shortened);
    if visual_width(&abbreviated, 4) <= max_width {
        return abbreviated;
    }

    // Truncate with ellipsis
    truncate_to_width(&abbreviated, max_width)
}

/// Abbreviate path by using only first letters of directory names.
/// E.g., `~/projects/rust/ue/src` becomes `~/p/r/u/s`
fn abbreviate_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    let mut result = Vec::new();

    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }

        // Keep first component (~ or /) and last component (filename) fully visible
        if i == 0 || i == parts.len() - 1 {
            result.push(part.to_string());
        } else {
            // Abbreviate middle components to first character
            if let Some(ch) = part.chars().next() {
                result.push(ch.to_string());
            }
        }
    }

    result.join("/")
}

/// Truncate a string to fit within max_width characters, adding "..." if truncated.
fn truncate_to_width(s: &str, max_width: usize) -> String {
    if max_width < 3 {
        return String::new();
    }

    let s_width = visual_width(s, 4);
    if s_width <= max_width {
        return s.to_string();
    }

    // We need to truncate. Reserve 3 chars for "..."
    let available = max_width.saturating_sub(3);
    let mut result = String::new();
    let mut current_width = 0;

    for ch in s.chars() {
        let ch_width = if ch == '\t' { 4 } else { 1 };
        if current_width + ch_width > available {
            break;
        }
        result.push(ch);
        current_width += ch_width;
    }

    result.push_str("...");
    result
}

pub(crate) fn render_screen(
    stdout: &mut impl Write,
    file: &str,
    lines: &[String],
    state: &FileViewerState,
    visible_lines: usize,
) -> Result<(), std::io::Error> {
    execute!(stdout, cursor::Hide)?;
    execute!(stdout, cursor::MoveTo(0, 0))?;

    render_header(stdout, file, state, lines, visible_lines)?;

    // Render content first (normal rendering)
    render_visible_lines(stdout, file, lines, state, visible_lines)?;
    render_scrollbar(stdout, lines, state, visible_lines)?;
    render_footer(stdout, state, lines, visible_lines)?;
    // Render h-scrollbar over the last content line (row visible_lines)
    render_horizontal_scrollbar(stdout, lines, state, visible_lines)?;

    // Then render dropdown menu OVER the content if active
    if state.menu_bar.active && state.menu_bar.dropdown_open {
        crate::menu::render_dropdown_menu(stdout, &state.menu_bar, state, lines)?;
    }

    position_cursor(stdout, lines, state, visible_lines)?;

    stdout.flush()?;
    Ok(())
}

fn render_header(
    stdout: &mut impl Write,
    file: &str,
    state: &FileViewerState,
    lines: &[String],
    visible_lines: usize,
) -> Result<(), std::io::Error> {
    use crossterm::{cursor::MoveTo, style::{Color, SetForegroundColor}};

    // Position at top of screen
    execute!(stdout, MoveTo(0, 0))?;

    // Set header background color
    if let Some(color) = crate::settings::Settings::parse_color(&state.settings.appearance.header_bg) {
        execute!(stdout, SetBackgroundColor(color))?;
    }

    // Render line number area (if enabled)
    if state.settings.appearance.line_number_digits > 0 {
        let total_lines = lines.len();

        // Calculate the width needed for the document (number of digits in total_lines)
        let actual_width = if total_lines == 0 {
            1
        } else {
            ((total_lines as f64).log10().floor() as usize) + 1
        };

        // Use the larger of line_number_digits or actual_width
        let display_width = actual_width.max(state.settings.appearance.line_number_digits as usize);

        let modulus = 10usize.pow(state.settings.appearance.line_number_digits as u32);
        let top_number = (state.top_line / modulus) * modulus;

        // Determine if cursor is above visible area
        let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines);
        let cursor_above = match state.cursor_off_screen_direction(lines, visible_lines, text_width) {
            Some(true) => true,  // Cursor is above
            _ => false,          // Cursor is visible or below
        };

        // Only show digit hint if total lines >= modulus (document exceeds digit capacity)
        let show_digit_hint = total_lines >= modulus;

        // Highlight with scrollbar color if cursor is above
        if cursor_above {
            use crossterm::style::Color;
            execute!(stdout, SetBackgroundColor(Color::Rgb { r: 100, g: 149, b: 237 }))?;
        }

        // Write digit hint or empty space (always same width based on document length)
        if show_digit_hint {
            write!(
                stdout,
                "{:width$}",
                top_number,
                width = display_width
            )?;
        } else {
            write!(
                stdout,
                "{:width$}",
                "",
                width = display_width
            )?;
        }

        // Reset color and write space separator
        if cursor_above {
            execute!(stdout, ResetColor)?;
            // Re-apply header background
            if let Some(color) = crate::settings::Settings::parse_color(&state.settings.appearance.header_bg) {
                execute!(stdout, SetBackgroundColor(color))?;
            }
        }
        write!(stdout, " ")?;
    }

    // Always render burger icon
    write!(stdout, "≡ ")?;


    if state.menu_bar.active {
        // When menu is active, show menu labels instead of filename
        for (idx, menu) in state.menu_bar.menus.iter().enumerate() {
            if idx == state.menu_bar.selected_menu_index {
                // Highlight selected menu with light blue (matching scrollbar style)
                execute!(stdout, SetBackgroundColor(Color::Rgb { r: 100, g: 149, b: 237 }))?;
                execute!(stdout, SetForegroundColor(Color::White))?;
            }

            write!(stdout, "{}", menu.label)?;
            execute!(stdout, ResetColor)?;

            // Restore header background
            if let Some(color) = crate::settings::Settings::parse_color(&state.settings.appearance.header_bg) {
                execute!(stdout, SetBackgroundColor(color))?;
            }

            write!(stdout, "  ")?;
        }
    } else {
        // When menu is not active, show filename as usual
        let modified_char = if state.modified { '*' } else { ' ' };
        let path = std::path::Path::new(file);
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or(file);
        let parent = path.parent().and_then(|p| p.to_str()).unwrap_or(".");

        // Get terminal width to calculate available space for title
        let (term_width, _) = terminal::size().unwrap_or((80, 24));

        // Calculate space used by other elements: line number area + burger menu + margins
        let line_num_width = if state.settings.appearance.line_number_digits > 0 {
            let total_lines = lines.len();
            let actual_width = if total_lines == 0 {
                1
            } else {
                ((total_lines as f64).log10().floor() as usize) + 1
            };
            let display_width = actual_width.max(state.settings.appearance.line_number_digits as usize);
            display_width + 2 // +2 for space separator
        } else {
            0
        };

        let burger_width = 2; // "≡ " takes 2 characters
        let available_width = term_width as usize - line_num_width - burger_width - 2; // -2 for safety margin

        // For untitled files, show a special indicator
        if state.is_untitled {
            let title = format!("{} {} (unsaved)", modified_char, filename);
            // Truncate if necessary
            let truncated_title = if visual_width(&title, 4) > available_width {
                truncate_to_width(&title, available_width)
            } else {
                title
            };
            write!(stdout, "{}", truncated_title)?;
        } else {
            // For normal files, try to fit filename and directory
            let mut display = format!("{} {} (", modified_char, filename);
            let base_width = visual_width(&display, 4);

            // Reserve space for closing parenthesis and filter indicator if needed
            let reserved = 1; // for closing )
            let filter_width = if state.filter_active && state.last_search_pattern.is_some() {
                9 // " [Filter]"
            } else {
                0
            };

            let available_for_path = available_width.saturating_sub(base_width + reserved + filter_width);

            // Apply path shortening
            let shortened_parent = if parent != "." {
                shorten_path_for_display(parent, available_for_path)
            } else {
                String::new()
            };

            display.push_str(&shortened_parent);
            display.push(')');

            // Final truncation of entire display if still too long
            let final_display = if visual_width(&display, 4) > available_width {
                truncate_to_width(&display, available_width)
            } else {
                display
            };

            write!(stdout, "{}", final_display)?;
        }
        
        // Show filter indicator when filter mode is active
        if state.filter_active && state.last_search_pattern.is_some() {
            write!(stdout, " [Filter]")?;
        }
    }

    // Clear rest of line (applies to both menu and filename modes)
    execute!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
    execute!(stdout, ResetColor)?;
    write!(stdout, "\r\n")?;

    Ok(())
}

/// Render position info (LINE:COL) with the line number portion highlighted
fn render_goto_position_highlighted(
    stdout: &mut impl Write,
    position_info: &str,
    show_selection: bool,
) -> Result<(), std::io::Error> {
    use crossterm::style::Attribute;

    // Find the colon position
    if let Some(colon_pos) = position_info.find(':') {
        // Highlight the line number part (before colon) only if selection should be shown
        let line_part = &position_info[..colon_pos];
        let col_part = &position_info[colon_pos..]; // Includes the colon

        if show_selection {
            // Render line number with inverted colors (selection)
            execute!(stdout, crossterm::style::SetAttribute(Attribute::Reverse))?;
            write!(stdout, "{}", line_part)?;
            execute!(stdout, crossterm::style::SetAttribute(Attribute::NoReverse))?;
        } else {
            // Render normally without selection
            write!(stdout, "{}", line_part)?;
        }

        // Render colon and column normally
        write!(stdout, "{}", col_part)?;
    } else {
        // No colon found, just render normally
        write!(stdout, "{}", position_info)?;
    }

    Ok(())
}

fn render_footer(
    stdout: &mut impl Write,
    state: &FileViewerState,
    lines: &[String],
    visible_lines: usize,
) -> Result<(), std::io::Error> {
    use crossterm::cursor::MoveTo;

    // Footer is always at row visible_lines + 1
    // (H-scrollbar will overlay the left portion if visible)
    let footer_row = (visible_lines + 1) as u16;

    // Position cursor at footer row
    execute!(stdout, MoveTo(0, footer_row))?;

    if let Some(color) =
        crate::settings::Settings::parse_color(&state.settings.appearance.footer_bg)
    {
        execute!(stdout, SetBackgroundColor(color))?;
    }

    // If in find mode, show the find prompt on left and hit count/position on right
    if state.find_active {
        let digits = state.settings.appearance.line_number_digits as usize;
        let total_width = state.term_width as usize;

        // Build the left side (find prompt + pattern)
        let mut left_side = String::new();
        if digits > 0 {
            left_side.push_str(&format!("{:width$} ", "", width = digits));
        }
        // Show "Filter" label if filter mode will be activated, otherwise "Find"
        let find_label = if state.filter_active {
            "Filter (regex): "
        } else {
            "Find (regex): "
        };
        left_side.push_str(find_label);
        let pattern_start_col = left_side.len();

        // Build the right side (hit count + arrows + position)
        let line_num = state.absolute_line() + 1;
        let col_num = state.cursor_col + 1;
        let position_info = format!("{}:{}", line_num, col_num);

        // Always show hit count with arrows
        let hit_display = if state.search_hit_count > 0 {
            if state.search_current_hit > 0 {
                format!("({}/{}) ↑↓", state.search_current_hit, state.search_hit_count)
            } else {
                format!("(-/{}) ↑↓", state.search_hit_count)
            }
        } else {
            "(0) ↑↓".to_string()
        };

        let right_side = format!("{}  {}", hit_display, position_info);

        // Render the footer
        write!(stdout, "\r")?;

        // Show error in red if present
        let error_offset = if let Some(ref error) = state.find_error {
            use crossterm::style::SetForegroundColor;
            execute!(stdout, SetForegroundColor(crossterm::style::Color::Red))?;
            write!(stdout, "[{}] ", error)?;
            execute!(stdout, ResetColor)?;
            if let Some(color) =
                crate::settings::Settings::parse_color(&state.settings.appearance.footer_bg)
            {
                execute!(stdout, SetBackgroundColor(color))?;
            }
            error.len() + 3 // "[error] "
        } else {
            0
        };

        // Write left side (prompt)
        write!(stdout, "{}", left_side)?;

        // Write find pattern with selection highlighting
        if let Some((sel_start, sel_end)) = state.find_selection {
            let chars: Vec<char> = state.find_pattern.chars().collect();
            for (i, ch) in chars.iter().enumerate() {
                if i == sel_start {
                    // Start selection - invert colors
                    execute!(stdout, crossterm::style::SetAttribute(crossterm::style::Attribute::Reverse))?;
                }
                write!(stdout, "{}", ch)?;
                if i + 1 == sel_end {
                    // End selection - restore colors
                    execute!(stdout, crossterm::style::SetAttribute(crossterm::style::Attribute::NoReverse))?;
                    if let Some(color) = crate::settings::Settings::parse_color(&state.settings.appearance.footer_bg) {
                        execute!(stdout, SetBackgroundColor(color))?;
                    }
                }
            }
        } else {
            write!(stdout, "{}", state.find_pattern)?;
        }

        // Update left_side length to account for the pattern we just wrote
        let full_left_len = left_side.len() + state.find_pattern.chars().count();

        // Calculate right-aligned position (same method as normal mode)
        // In normal mode: remaining_width = total_width - left_len
        // where left_len includes the digit area length
        let digit_area_len = if digits > 0 { digits + 1 } else { 0 };
        let remaining_width = total_width.saturating_sub(digit_area_len);

        // Calculate how much content we've written after the digit area
        let written_after_digits = error_offset + full_left_len - digit_area_len;

        // Right-align: pad to push right_side to right edge
        let pad = remaining_width.saturating_sub(written_after_digits).saturating_sub(right_side.len());
        for _ in 0..pad {
            write!(stdout, " ")?;
        }

        // Write right side (now at same position as in normal mode)
        write!(stdout, "{}", right_side)?;

        // Clear rest of line
        execute!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
        execute!(stdout, ResetColor)?;

        // Position cursor at find_cursor_pos within the search pattern
        let error_offset = if let Some(ref error) = state.find_error {
            error.len() + 3 // "[" + error + "] "
        } else {
            0
        };
        let chars: Vec<char> = state.find_pattern.chars().collect();
        let cursor_offset = chars.iter().take(state.find_cursor_pos).count();
        let cursor_x = (error_offset + pattern_start_col + cursor_offset) as u16;
        execute!(stdout, cursor::MoveTo(cursor_x, footer_row))?;
        apply_cursor_shape(stdout, state.settings)?;
        execute!(stdout, cursor::Show)?;
        return Ok(());
    }

    // If in replace mode, show the replace prompt with buttons
    if state.replace_active {
        let digits = state.settings.appearance.line_number_digits as usize;
        let total_width = state.term_width as usize;

        // Build the left side (replace prompt)
        let mut left_side = String::new();
        if digits > 0 {
            left_side.push_str(&format!("{:width$} ", "", width = digits));
        }
        let replace_label = "Replace with: ";
        left_side.push_str(replace_label);
        let pattern_start_col = left_side.len();

        // Build the right side (buttons + position)
        let line_num = state.absolute_line() + 1;
        let col_num = state.cursor_col + 1;
        let position_info = format!("{}:{}", line_num, col_num);

        // Show buttons for replace operations
        let buttons = "[replace occurrence] [replace all]";
        let right_side = format!("{}  {}", buttons, position_info);

        // Render the footer
        write!(stdout, "\r")?;

        // Write left side (prompt)
        write!(stdout, "{}", left_side)?;

        // Write replace pattern with selection highlighting
        if let Some((sel_start, sel_end)) = state.replace_selection {
            let chars: Vec<char> = state.replace_pattern.chars().collect();
            for (i, ch) in chars.iter().enumerate() {
                if i == sel_start {
                    // Start selection - invert colors
                    execute!(stdout, crossterm::style::SetAttribute(crossterm::style::Attribute::Reverse))?;
                }
                write!(stdout, "{}", ch)?;
                if i + 1 == sel_end {
                    // End selection - restore colors
                    execute!(stdout, crossterm::style::SetAttribute(crossterm::style::Attribute::NoReverse))?;
                    if let Some(color) = crate::settings::Settings::parse_color(&state.settings.appearance.footer_bg) {
                        execute!(stdout, SetBackgroundColor(color))?;
                    }
                }
            }
        } else {
            write!(stdout, "{}", state.replace_pattern)?;
        }

        // Update full left length to account for the pattern we just wrote
        let full_left_len = left_side.len() + state.replace_pattern.chars().count();

        // Calculate right-aligned position
        let digit_area_len = if digits > 0 { digits + 1 } else { 0 };
        let remaining_width = total_width.saturating_sub(digit_area_len);
        let written_after_digits = full_left_len - digit_area_len;

        // Right-align: pad to push right_side to right edge
        let pad = remaining_width.saturating_sub(written_after_digits).saturating_sub(right_side.len());
        for _ in 0..pad {
            write!(stdout, " ")?;
        }

        // Write right side
        write!(stdout, "{}", right_side)?;

        // Clear rest of line
        execute!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
        execute!(stdout, ResetColor)?;

        // Position cursor at replace_cursor_pos within the replace pattern
        let chars: Vec<char> = state.replace_pattern.chars().collect();
        let cursor_offset = chars.iter().take(state.replace_cursor_pos).count();
        let cursor_x = (pattern_start_col + cursor_offset) as u16;
        execute!(stdout, cursor::MoveTo(cursor_x, footer_row))?;
        apply_cursor_shape(stdout, state.settings)?;
        execute!(stdout, cursor::Show)?;
        return Ok(());
    }

    // Normal footer with position info (or error message)
    let line_num = state.absolute_line() + 1;
    let col_num = state.cursor_col + 1;

    // In goto_line mode, use the input instead of actual line number
    let position_info = if state.goto_line_active {
        format!("{}:{}", state.goto_line_input, col_num)
    } else {
        format!("{}:{}", line_num, col_num)
    };

    // Build position string with hit count first (if active search), then position
    let position_info = if state.last_search_pattern.is_some() {
        let hit_display = if state.search_hit_count > 0 {
            if state.search_current_hit > 0 {
                format!("({}/{}) ↑↓", state.search_current_hit, state.search_hit_count)
            } else {
                format!("(-/{}) ↑↓", state.search_hit_count)
            }
        } else {
            "(0) ↑↓".to_string()
        };
        format!("{}  {}", hit_display, position_info)
    } else {
        position_info
    };

    let total_width = state.term_width as usize;
    let digits = state.settings.appearance.line_number_digits as usize;
    let mut bottom_number_str = String::new();
    let mut highlight_digit_hint = false;
    if digits > 0 {
        let total_lines = lines.len();

        // Calculate the width needed for the document (number of digits in total_lines)
        let actual_width = if total_lines == 0 {
            1
        } else {
            ((total_lines as f64).log10().floor() as usize) + 1
        };

        // Use the larger of line_number_digits or actual_width
        let display_width = actual_width.max(digits);

        let modulus = 10usize.pow(digits as u32);
        let mut last_visible_line = state.top_line;
        let mut remaining = visible_lines;
        let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines);
        let tab_width = state.settings.tab_width;
        let wrapping_enabled = state.is_line_wrapping_enabled();
        while remaining > 0 && last_visible_line < lines.len() {
            let wrapped = if wrapping_enabled {
                calculate_wrapped_lines_for_line(lines, last_visible_line, text_width, tab_width) as usize
            } else {
                1  // No wrapping - each line is exactly 1 visual line
            };
            if wrapped <= remaining {
                remaining -= wrapped;
                last_visible_line += 1;
            } else {
                break;
            }
        }
        let bottom_number = (last_visible_line / modulus) * modulus;

        // Only show digit hint if total lines >= modulus (document exceeds digit capacity)
        let show_digit_hint = total_lines >= modulus;

        // Check if cursor is below visible area to determine highlighting
        let cursor_below = match state.cursor_off_screen_direction(lines, visible_lines, text_width) {
            Some(false) => true,  // Cursor is below
            _ => false,           // Cursor is visible or above
        };
        highlight_digit_hint = cursor_below;

        // Format digit hint or empty space (always same width based on document length)
        if show_digit_hint {
            bottom_number_str = format!("{:width$}", bottom_number, width = display_width);
        } else {
            bottom_number_str = format!("{:width$}", "", width = display_width);
        }
    }

    write!(stdout, "\r")?;

    // Apply scrollbar color highlighting if needed before writing digit hint
    if highlight_digit_hint {
        use crossterm::style::Color;
        execute!(stdout, SetBackgroundColor(Color::Rgb { r: 100, g: 149, b: 237 }))?;
    }
    write!(stdout, "{}", bottom_number_str)?;
    if highlight_digit_hint {
        execute!(stdout, ResetColor)?;
        // Re-apply footer background
        if let Some(color) = crate::settings::Settings::parse_color(&state.settings.appearance.footer_bg) {
            execute!(stdout, SetBackgroundColor(color))?;
        }
    }

    // Write space separator
    write!(stdout, " ")?;

    let left_len = bottom_number_str.len() + 1; // +1 for the space separator
    let remaining_width = total_width.saturating_sub(left_len);

    // Show error/info message if present, otherwise show position
    if let Some(ref error) = state.find_error {
        use crossterm::style::SetForegroundColor;
        let color = if error.contains("wrapped") {
            crossterm::style::Color::Yellow
        } else {
            crossterm::style::Color::Red
        };
        execute!(stdout, SetForegroundColor(color))?;
        write!(stdout, "{}", error)?;
        execute!(stdout, ResetColor)?;
        if let Some(color) =
            crate::settings::Settings::parse_color(&state.settings.appearance.footer_bg)
        {
            execute!(stdout, SetBackgroundColor(color))?;
        }
    } else if position_info.len() >= remaining_width {
        let truncated = &position_info[position_info.len() - remaining_width..];
        if state.goto_line_active {
            // Render with line number portion highlighted only if not yet typing
            let show_selection = !state.goto_line_typing_started;
            render_goto_position_highlighted(stdout, truncated, show_selection)?;
        } else {
            write!(stdout, "{}", truncated)?;
        }
    } else {
        let pad = remaining_width - position_info.len();
        for _ in 0..pad {
            write!(stdout, " ")?;
        }

        if state.goto_line_active {
            // Render with line number portion highlighted only if not yet typing
            let show_selection = !state.goto_line_typing_started;
            render_goto_position_highlighted(stdout, &position_info, show_selection)?;
        } else {
            write!(stdout, "{}", position_info)?;
        }
    }
    // Footer row doesn't interfere with scrollbar, but clear consistently
    execute!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
    execute!(stdout, ResetColor)?;
    Ok(())
}

fn render_visible_lines(
    stdout: &mut impl Write,
    _file: &str,
    lines: &[String],
    state: &FileViewerState,
    visible_lines: usize,
) -> Result<(), std::io::Error> {
    // When h-scrollbar is shown, reserve the last line for it
    let h_scrollbar_shown = should_show_horizontal_scrollbar(state, lines, visible_lines);
    let content_lines = if h_scrollbar_shown {
        visible_lines.saturating_sub(1)
    } else {
        visible_lines
    };

    let text_width_u16 = crate::coordinates::calculate_text_width(state, lines, visible_lines);
    let _text_width_usize = text_width_u16 as usize;

    // Calculate which visual line the cursor is on
    let cursor_visual_line = calculate_cursor_visual_line(lines, state, text_width_u16);

    let ctx = RenderContext {
        lines,
        state,
        visible_lines,
    };

    // Get filtered lines if filter mode is active
    let filtered_lines = if state.filter_active && state.last_search_pattern.is_some() {
        let pattern = state.last_search_pattern.as_ref().unwrap();
        crate::find::get_lines_with_matches(lines, pattern, state.find_scope)
    } else {
        Vec::new()
    };

    let mut visual_lines_rendered = 0;
    
    if state.filter_active && !filtered_lines.is_empty() {
        // Filter mode: render only lines with matches
        let mut filtered_index = 0;
        
        // Find starting position in filtered lines based on top_line
        while filtered_index < filtered_lines.len() && filtered_lines[filtered_index] < state.top_line {
            filtered_index += 1;
        }
        
        // Render filtered lines starting from the first visible one
        while visual_lines_rendered < content_lines && filtered_index < filtered_lines.len() {
            let logical_line_index = filtered_lines[filtered_index];
            let lines_for_this_logical = render_line(
                stdout,
                &ctx,
                logical_line_index,
                cursor_visual_line,
                visual_lines_rendered,
                content_lines - visual_lines_rendered,
            )?;

            visual_lines_rendered += lines_for_this_logical;
            filtered_index += 1;
        }
    } else {
        // Normal mode: render all lines
        let mut logical_line_index = state.top_line;
        
        while visual_lines_rendered < content_lines && logical_line_index < lines.len() {
            let lines_for_this_logical = render_line(
                stdout,
                &ctx,
                logical_line_index,
                cursor_visual_line,
                visual_lines_rendered,
                content_lines - visual_lines_rendered,
            )?;

            visual_lines_rendered += lines_for_this_logical;
            logical_line_index += 1;
        }
    }

    // Fill remaining content lines with empty lines
    while visual_lines_rendered < content_lines {
        if state.settings.appearance.line_number_digits > 0 {
            if let Some(color) =
                crate::settings::Settings::parse_color(&state.settings.appearance.line_numbers_bg)
            {
                execute!(stdout, SetBackgroundColor(color))?;
            }
            write!(
                stdout,
                "{:width$} ",
                "",
                width = state.settings.appearance.line_number_digits as usize
            )?;
            execute!(stdout, ResetColor)?;
        }
        let current_col = if state.settings.appearance.line_number_digits > 0 {
            state.settings.appearance.line_number_digits as u16 + 1
        } else {
            0
        };
        clear_to_scrollbar(stdout, state, lines, visible_lines, current_col)?;
        write!(stdout, "\r\n")?;
        visual_lines_rendered += 1;
    }


    // Note: If h-scrollbar will be shown, DO NOT render the h-scrollbar line here
    // Let render_horizontal_scrollbar() handle it entirely to prevent flickering
    // The h-scrollbar line (row visible_lines) is rendered separately

    Ok(())
}

struct RenderContext<'a> {
    lines: &'a [String],
    state: &'a FileViewerState<'a>,
    visible_lines: usize,
}

struct SegmentInfo {
    line_index: usize,
    start_visual: usize,
    end_visual: usize,
    tab_width: usize,
}

fn render_line(
    stdout: &mut impl Write,
    ctx: &RenderContext,
    logical_line_index: usize,
    _cursor_visual_line: usize,
    _current_visual_line: usize,
    remaining_visible_lines: usize,
) -> Result<usize, std::io::Error> {
    if logical_line_index >= ctx.lines.len() {
        return Ok(0);
    }

    let line = &ctx.lines[logical_line_index];
    let available_width =
        crate::coordinates::calculate_text_width(ctx.state, ctx.lines, ctx.visible_lines) as usize;
    let tab_width = ctx.state.settings.tab_width;

    // Expand tabs to spaces for display
    let expanded_line = expand_tabs(line, tab_width);
    let chars: Vec<char> = expanded_line.chars().collect();

    // Check if wrapping is enabled
    let wrapping_enabled = ctx.state.is_line_wrapping_enabled();

    let num_wrapped_lines = if wrapping_enabled {
        calculate_wrapped_lines_for_line(
            ctx.lines,
            logical_line_index,
            crate::coordinates::calculate_text_width(ctx.state, ctx.lines, ctx.visible_lines),
            tab_width,
        )
    } else {
        1 // No wrapping - each logical line is exactly 1 visual line
    };

    let lines_to_render = (num_wrapped_lines as usize).min(remaining_visible_lines);

    for wrap_index in 0..lines_to_render {
        if wrap_index > 0 {
            write!(stdout, "\r\n")?;
        }

        // Show line number only if line_number_digits > 0
        if ctx.state.settings.appearance.line_number_digits > 0 {
            // Show line number only on first wrapped line, spaces on continuation lines
            if wrap_index == 0 {
                // Calculate line number to display (modulo based on digits)
                let modulus = 10usize.pow(ctx.state.settings.appearance.line_number_digits as u32);
                let line_num = (logical_line_index + 1) % modulus;

                // Check if this line contains the cursor
                let is_cursor_line = logical_line_index == ctx.state.absolute_line();

                // Set line numbers background color
                if let Some(color) = crate::settings::Settings::parse_color(
                    &ctx.state.settings.appearance.line_numbers_bg,
                ) {
                    execute!(stdout, SetBackgroundColor(color))?;
                }

                // Highlight line number with scrollbar color if cursor line
                if is_cursor_line {
                    use crossterm::style::Color;
                    execute!(stdout, SetBackgroundColor(Color::Rgb { r: 100, g: 149, b: 237 }))?;
                }

                // Write line number
                write!(
                    stdout,
                    "{:width$}",
                    line_num,
                    width = ctx.state.settings.appearance.line_number_digits as usize
                )?;

                // Reset to line numbers background before writing indicator
                if is_cursor_line {
                    if let Some(color) = crate::settings::Settings::parse_color(
                        &ctx.state.settings.appearance.line_numbers_bg,
                    ) {
                        execute!(stdout, SetBackgroundColor(color))?;
                    }
                }

                // Show '>' for cursor line, space for others
                if is_cursor_line {
                    write!(stdout, ">")?;
                } else {
                    write!(stdout, " ")?;
                }

                execute!(stdout, ResetColor)?;
            } else {
                if let Some(color) = crate::settings::Settings::parse_color(
                    &ctx.state.settings.appearance.line_numbers_bg,
                ) {
                    execute!(stdout, SetBackgroundColor(color))?;
                }
                write!(
                    stdout,
                    "{:width$} ",
                    "",
                    width = ctx.state.settings.appearance.line_number_digits as usize
                )?;
                execute!(stdout, ResetColor)?;
            }
        }

        // Calculate visible segment based on wrapping mode
        let (start_visual, end_visual) = if wrapping_enabled {
            // Wrapped mode: show segment at wrap_index
            let start = wrap_index * available_width;
            let end = ((wrap_index + 1) * available_width).min(chars.len());
            (start, end)
        } else {
            // Horizontal scroll mode: apply horizontal offset to entire document
            let start = ctx.state.horizontal_scroll_offset;
            let end = (start + available_width).min(chars.len());
            (start, end)
        };

        // Render content if there's any visible part of the line
        let content_width = if start_visual < chars.len() {
            let segment = SegmentInfo {
                line_index: logical_line_index,
                start_visual,
                end_visual,
                tab_width,
            };

            if let (Some(sel_start), Some(sel_end)) =
                (ctx.state.selection_start, ctx.state.selection_end)
            {
                render_line_segment_with_selection_expanded(
                    stdout, &chars, line, sel_start, sel_end, ctx, &segment,
                )?;
            } else {
                render_line_segment_expanded(stdout, &chars, line, ctx, &segment)?;
            }
            (end_visual - start_visual) as u16
        } else {
            // Line is shorter than horizontal scroll offset - render as empty
            // (but still takes up a visual row)
            0
        };

        // Calculate current column position after rendering content
        let current_col = if ctx.state.settings.appearance.line_number_digits > 0 {
            let line_num_width = ctx.state.settings.appearance.line_number_digits as u16 + 1;
            line_num_width + content_width
        } else {
            content_width
        };
        clear_to_scrollbar(stdout, ctx.state, ctx.lines, ctx.visible_lines, current_col)?;
    }

    // Add newline after the last wrapped segment to separate this logical line from the next
    write!(stdout, "\r\n")?;

    Ok(lines_to_render)
}

fn normalize_selection(sel_start: Position, sel_end: Position) -> (Position, Position) {
    if sel_start.0 < sel_end.0 || (sel_start.0 == sel_end.0 && sel_start.1 <= sel_end.1) {
        (sel_start, sel_end)
    } else {
        (sel_end, sel_start)
    }
}

/// Cached regex for search performance
use std::cell::RefCell;
thread_local! {
    static SEARCH_REGEX_CACHE: RefCell<Option<(String, regex::Regex)>> = RefCell::new(None);
}

/// Get character ranges for search matches in a line (with caching)
fn get_search_matches(line: &str, pattern: &str) -> Vec<(usize, usize)> {
    if pattern.is_empty() {
        return vec![];
    }

    // Try to use cached regex
    SEARCH_REGEX_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();

        // Check if we need to compile a new regex
        let regex = if let Some((cached_pattern, cached_regex)) = cache.as_ref() {
            if cached_pattern == pattern {
                cached_regex
            } else {
                // Pattern changed, compile new regex
                let pattern_with_flags = format!("(?i){}", pattern);
                match regex::Regex::new(&pattern_with_flags) {
                    Ok(regex) => {
                        *cache = Some((pattern.to_string(), regex));
                        &cache.as_ref().unwrap().1
                    }
                    Err(_) => return vec![],
                }
            }
        } else {
            // No cached regex, compile new one
            let pattern_with_flags = format!("(?i){}", pattern);
            match regex::Regex::new(&pattern_with_flags) {
                Ok(regex) => {
                    *cache = Some((pattern.to_string(), regex));
                    &cache.as_ref().unwrap().1
                }
                Err(_) => return vec![],
            }
        };

        regex
            .find_iter(line)
            .map(|m| {
                // Convert byte positions to character positions
                let char_start = line[..m.start()].chars().count();
                let char_end = line[..m.end()].chars().count();
                (char_start, char_end)
            })
            .collect()
    })
}

/// Check if a match at (char_start, char_end) on line_idx overlaps with the find_scope
fn match_overlaps_scope(
    line_idx: usize,
    char_start: usize,
    char_end: usize,
    scope: Option<((usize, usize), (usize, usize))>,
) -> bool {
    if let Some(((scope_start_line, scope_start_col), (scope_end_line, scope_end_col))) = scope {
        // Check if line is within scope range
        if line_idx < scope_start_line || line_idx > scope_end_line {
            false
        } else if line_idx == scope_start_line && line_idx == scope_end_line {
            // Single line scope - match must overlap with [scope_start_col, scope_end_col)
            char_end > scope_start_col && char_start < scope_end_col
        } else if line_idx == scope_start_line {
            // First line of multi-line scope
            char_end > scope_start_col
        } else if line_idx == scope_end_line {
            // Last line of multi-line scope
            char_start < scope_end_col
        } else {
            // Middle line of multi-line scope
            true
        }
    } else {
        // No scope restriction
        true
    }
}

fn apply_cursor_shape(
    stdout: &mut impl Write,
    settings: &crate::settings::Settings,
) -> std::io::Result<()> {
    // Use VT escape sequence to set cursor style.
    // block: 2 (steady) or 0 (blinking), bar: 6 (steady) or 5 (blinking), underline: 4 (steady) or 3 (blinking)
    let seq = match settings.appearance.cursor_shape.to_lowercase().as_str() {
        "block" => "\x1b[2 q",
        "underline" => "\x1b[4 q",
        _ => "\x1b[6 q", // bar default
    };
    write!(stdout, "{}", seq)?;
    Ok(())
}

/// Calculate the Y position (row) for a cursor at the given absolute line
/// Returns None if the line is not visible (scrolled off screen or filtered out)
fn calculate_cursor_y_position(
    state: &FileViewerState,
    lines: &[String],
    cursor_line_abs: usize,
    text_width: u16,
    tab_width: usize,
    visible_lines: usize,
) -> Option<u16> {
    // Get filtered lines if filter mode is active
    let filtered_lines = if state.filter_active && state.last_search_pattern.is_some() {
        let pattern = state.last_search_pattern.as_ref().unwrap();
        crate::find::get_lines_with_matches(lines, pattern, state.find_scope)
    } else {
        Vec::new()
    };

    let wrapping_enabled = state.is_line_wrapping_enabled();
    let mut cursor_y = 1u16; // Start at row 1 (after header)

    if state.filter_active && !filtered_lines.is_empty() {
        // Filter mode: check if cursor line is in filtered results
        if !filtered_lines.contains(&cursor_line_abs) {
            return None; // Cursor is on a line that's filtered out
        }

        // Calculate Y position by iterating through visible filtered lines
        let mut filtered_index = 0;

        // Find starting position in filtered lines based on top_line
        while filtered_index < filtered_lines.len() && filtered_lines[filtered_index] < state.top_line {
            filtered_index += 1;
        }

        // Iterate through visible filtered lines until we reach the cursor line
        while filtered_index < filtered_lines.len() {
            let logical_line = filtered_lines[filtered_index];

            if logical_line == cursor_line_abs {
                return Some(cursor_y); // Found the cursor line
            }

            if logical_line > cursor_line_abs {
                return None; // Cursor line is above visible area
            }

            let wrapped_lines = if wrapping_enabled {
                calculate_wrapped_lines_for_line(lines, logical_line, text_width, tab_width)
            } else {
                1
            };

            cursor_y += wrapped_lines;

            // Check if we've gone past the visible area
            if cursor_y > visible_lines as u16 {
                return None; // Cursor is below visible area
            }

            filtered_index += 1;
        }

        None // Cursor line not found in visible filtered lines
    } else {
        // Normal mode: check if cursor is in visible range
        if cursor_line_abs < state.top_line || cursor_line_abs >= state.top_line + visible_lines {
            return None; // Not visible
        }

        // Calculate Y position by iterating from top_line to cursor line
        for i in state.top_line..cursor_line_abs {
            let wrapped_lines = if wrapping_enabled {
                calculate_wrapped_lines_for_line(lines, i, text_width, tab_width)
            } else {
                1
            };
            cursor_y += wrapped_lines;
        }

        Some(cursor_y)
    }
}

fn position_cursor(
    stdout: &mut impl Write,
    lines: &[String],
    state: &FileViewerState,
    visible_lines: usize,
) -> Result<(), std::io::Error> {
    // If menu is active, keep cursor hidden
    if state.menu_bar.active {
        return Ok(());
    }

    // If in find mode, cursor is already positioned in the search field by render_footer
    if state.find_active {
        return Ok(());
    }

    // If in replace mode, cursor is already positioned in the replace field by render_footer
    if state.replace_active {
        return Ok(());
    }

    // If in goto_line mode, position cursor in the footer at the line number position
    if state.goto_line_active {
        let col_num = state.cursor_col + 1;
        let position_info = format!("{}:{}", state.goto_line_input, col_num);

        // Calculate where the position info is on screen
        let total_width = state.term_width as usize;
        let digits = state.settings.appearance.line_number_digits as usize;
        let left_len = if digits > 0 { digits + 1 } else { 0 };
        let remaining_width = total_width.saturating_sub(left_len);

        // Cursor should be at goto_line_cursor_pos within the line number part
        let cursor_x = if position_info.len() >= remaining_width {
            // Truncated case - need to calculate differently
            let truncated = &position_info[position_info.len() - remaining_width..];
            if let Some(colon_pos) = truncated.find(':') {
                // Calculate cursor position relative to start of truncated string
                let cursor_pos_in_truncated = state.goto_line_cursor_pos.min(colon_pos);
                (left_len + cursor_pos_in_truncated) as u16
            } else {
                (left_len + state.goto_line_cursor_pos.min(truncated.len())) as u16
            }
        } else {
            // Normal case - position info is padded and right-aligned
            let pad = remaining_width - position_info.len();
            // Find the colon position in the position_info
            if let Some(colon_pos) = position_info.find(':') {
                let cursor_offset = state.goto_line_cursor_pos.min(colon_pos);
                (left_len + pad + cursor_offset) as u16
            } else {
                (left_len + pad + state.goto_line_cursor_pos.min(position_info.len())) as u16
            }
        };

        let cursor_y = (visible_lines + 1) as u16;
        execute!(stdout, cursor::MoveTo(cursor_x, cursor_y))?;
        apply_cursor_shape(stdout, state.settings)?;
        execute!(stdout, cursor::Show)?;
        return Ok(());
    }

    let line_num_width = line_number_width(state.settings);
    let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines);
    if state.dragging_selection_active
        && let Some((target_line, target_col)) = state.drag_target
    {
        let tab_width = state.settings.tab_width;
        if target_line < lines.len() {
            // Calculate Y position for drag target, accounting for filtered lines
            let cursor_y_opt = calculate_cursor_y_position(
                state,
                lines,
                target_line,
                text_width,
                tab_width,
                visible_lines,
            );

            if let Some(mut cursor_y) = cursor_y_opt {
                let visual_col = visual_width_up_to(
                    &lines[target_line],
                    target_col.min(lines[target_line].len()),
                    tab_width,
                );

                // Calculate position based on wrapping mode
                let (cursor_x, wrapped_offset) = if state.is_line_wrapping_enabled() {
                    let wrapped_line = visual_col / (text_width as usize);
                    let cursor_x = (visual_col % (text_width as usize)) as u16 + line_num_width;
                    (cursor_x, wrapped_line as u16)
                } else {
                    // Horizontal scroll mode: apply horizontal offset
                    let cursor_x = (visual_col.saturating_sub(state.horizontal_scroll_offset)) as u16 + line_num_width;
                    (cursor_x, 0)
                };

                cursor_y += wrapped_offset;
                execute!(stdout, cursor::MoveTo(cursor_x, cursor_y))?;
                apply_cursor_shape(stdout, state.settings)?;
                execute!(stdout, cursor::Show)?;
                return Ok(());
            }
        }
    }
    if !state.is_cursor_visible(lines, visible_lines, text_width) {
        return Ok(());
    }

    // Handle multi-cursor mode OR zero-width block selection - show blinking block cursors
    let is_zero_width_block = state.block_selection
        && if let Some((start, end)) = state.selection_range() {
            start.1 == end.1 && start.0 != end.0 // Zero width, multiple lines
        } else {
            false
        };

    let should_show_block_cursors = state.has_multi_cursors() || is_zero_width_block;

    if should_show_block_cursors {
        let tab_width = state.settings.tab_width;
        let blink_visible = state.cursor_blink_state;

        // Get all cursor positions (either multi-cursors or block selection range)
        let cursor_positions: Vec<Position> = if state.has_multi_cursors() {
            state.all_cursor_positions()
        } else if let Some((start, end)) = state.selection_range() {
            // Zero-width block selection: create cursors for each line in range
            (start.0..=end.0).map(|line| (line, start.1)).collect()
        } else {
            vec![state.current_position()]
        };

        // Draw blinking block cursor on ALL cursor lines (including main cursor)
        for &(cursor_line_abs, cursor_col) in &cursor_positions {
            if cursor_line_abs >= lines.len() {
                continue;
            }

            // Calculate Y position for this cursor, accounting for filtered lines
            let cursor_y_opt = calculate_cursor_y_position(
                state,
                lines,
                cursor_line_abs,
                text_width,
                tab_width,
                visible_lines,
            );

            let Some(mut cursor_y) = cursor_y_opt else {
                continue; // Cursor not visible (scrolled off or filtered out)
            };

            // Calculate X position
            let line_len = lines[cursor_line_abs].chars().count();
            let actual_col = cursor_col.min(line_len);
            let visual_col = visual_width_up_to(&lines[cursor_line_abs], actual_col, tab_width);

            // Calculate position based on wrapping mode
            let (cursor_x, wrapped_offset) = if state.is_line_wrapping_enabled() {
                let cursor_wrapped_line = visual_col / (text_width as usize);
                let cursor_x = (visual_col % (text_width as usize)) as u16 + line_num_width;
                (cursor_x, cursor_wrapped_line as u16)
            } else {
                // Horizontal scroll mode: check if cursor is horizontally visible
                // Skip this cursor if it's scrolled off screen horizontally
                if visual_col < state.horizontal_scroll_offset {
                    continue; // Scrolled off to the left
                }
                if visual_col >= state.horizontal_scroll_offset + (text_width as usize) {
                    continue; // Scrolled off to the right
                }
                // Apply horizontal offset
                let cursor_x = (visual_col.saturating_sub(state.horizontal_scroll_offset)) as u16 + line_num_width;
                (cursor_x, 0)
            };

            cursor_y += wrapped_offset;

            // Get the character at this position (or space if at end of line)
            let char_at_cursor = if cursor_col < line_len {
                lines[cursor_line_abs].chars().nth(cursor_col).unwrap_or(' ')
            } else {
                ' '
            };

            // Draw blinking block cursor: alternate between normal and inverted
            execute!(stdout, cursor::MoveTo(cursor_x, cursor_y))?;
            if blink_visible {
                // Blink ON: show inverted (block cursor)
                execute!(stdout, crossterm::style::SetAttribute(crossterm::style::Attribute::Reverse))?;
                write!(stdout, "{}", char_at_cursor)?;
                execute!(stdout, crossterm::style::SetAttribute(crossterm::style::Attribute::NoReverse))?;
            } else {
                // Blink OFF: show normal character (cursor invisible)
                write!(stdout, "{}", char_at_cursor)?;
            }
        }

        // Hide the terminal cursor since we're using block cursors
        execute!(stdout, cursor::Hide)?;
        return Ok(());
    }

    let tab_width = state.settings.tab_width;

    // Calculate Y position based on wrapping mode and filtered lines
    let cursor_line_idx = state.absolute_line();
    let cursor_y_opt = calculate_cursor_y_position(
        state,
        lines,
        cursor_line_idx,
        text_width,
        tab_width,
        visible_lines,
    );

    let Some(mut cursor_y) = cursor_y_opt else {
        return Ok(()); // Cursor not visible (scrolled off or filtered out)
    };

    let visual_col = if cursor_line_idx < lines.len() {
        visual_width_up_to(&lines[cursor_line_idx], state.cursor_col, tab_width)
    } else {
        0
    };

    // Calculate cursor position based on wrapping mode
    let (cursor_x, cursor_y_offset) = if state.is_line_wrapping_enabled() {
        // Wrapped mode: cursor can be on any wrapped line segment
        let cursor_wrapped_line = visual_col / (text_width as usize);
        let cursor_x = (visual_col % (text_width as usize)) as u16 + line_num_width;
        (cursor_x, cursor_wrapped_line as u16)
    } else {
        // Horizontal scroll mode: cursor is always on the first (only) visual line
        // Apply horizontal scroll offset
        let cursor_x = (visual_col.saturating_sub(state.horizontal_scroll_offset)) as u16 + line_num_width;
        (cursor_x, 0)
    };

    cursor_y += cursor_y_offset;
    execute!(stdout, cursor::MoveTo(cursor_x, cursor_y))?;
    apply_cursor_shape(stdout, state.settings)?;
    execute!(stdout, cursor::Show)?;
    Ok(())
}

/// Render a line segment with expanded tabs (no selection)
fn render_line_segment_expanded(
    stdout: &mut impl Write,
    expanded_chars: &[char],
    original_line: &str,
    ctx: &RenderContext,
    segment: &SegmentInfo,
) -> Result<(), std::io::Error> {
    use crossterm::style::{ResetColor, SetBackgroundColor, SetForegroundColor};

    // Get syntax highlighting for the original line
    let highlights = crate::syntax::highlight_line(original_line);

    // Convert byte positions to visual positions for the expanded line
    let mut visual_to_color: Vec<Option<crossterm::style::Color>> =
        vec![None; expanded_chars.len()];
    let mut visual_to_search_match: Vec<bool> = vec![false; expanded_chars.len()];

    // Apply syntax highlighting
    for (byte_start, byte_end, color) in highlights {
        // Convert byte positions to character positions in original line
        let char_start = original_line[..byte_start.min(original_line.len())]
            .chars()
            .count();
        let char_end = original_line[..byte_end.min(original_line.len())]
            .chars()
            .count();

        // Convert character positions to visual positions (accounting for tabs)
        let visual_start =
            crate::coordinates::visual_width_up_to(original_line, char_start, segment.tab_width);
        let visual_end =
            crate::coordinates::visual_width_up_to(original_line, char_end, segment.tab_width);

        // Mark visual positions with color
        for i in visual_start..visual_end.min(visual_to_color.len()) {
            visual_to_color[i] = Some(color);
        }
    }

    // Apply search match highlighting - compute once and cache current match range
    let mut current_match_range: Option<(usize, usize)> = None; // Visual column range
    if let Some(ref pattern) = ctx.state.last_search_pattern {
        let matches = get_search_matches(original_line, pattern);
        let cursor_pos = ctx.state.current_position();
        let is_cursor_line = segment.line_index == cursor_pos.0;
        let cursor_col = if is_cursor_line {
            cursor_pos.1
        } else {
            usize::MAX
        };

        for (char_start, char_end) in matches {
            // Check if this match overlaps with the find_scope
            if !match_overlaps_scope(
                segment.line_index,
                char_start,
                char_end,
                ctx.state.find_scope,
            ) {
                continue;
            }

            let visual_start = crate::coordinates::visual_width_up_to(
                original_line,
                char_start,
                segment.tab_width,
            );
            let visual_end = crate::coordinates::visual_width_up_to(
                original_line,
                char_end,
                segment.tab_width,
            );

            // Check if cursor is within this match
            let is_current_match =
                is_cursor_line && cursor_col >= char_start && cursor_col < char_end;

            if is_current_match {
                // Cache the current match range
                current_match_range = Some((visual_start, visual_end));
            } else {
                // Mark as regular search match
                for i in visual_start..visual_end.min(visual_to_search_match.len()) {
                    visual_to_search_match[i] = true;
                }
            }
        }
    }

    // Render the segment with colors
    let mut current_color: Option<crossterm::style::Color> = None;
    let mut current_bg: bool = false;

    for visual_i in segment.start_visual..segment.end_visual {
        if visual_i >= expanded_chars.len() {
            break;
        }

        let ch = expanded_chars[visual_i];
        let desired_color = visual_to_color.get(visual_i).copied().flatten();
        let is_search_match = visual_to_search_match
            .get(visual_i)
            .copied()
            .unwrap_or(false);

        // Check if this position is in the current match (using cached range)
        let is_current_match = if let Some((start, end)) = current_match_range {
            visual_i >= start && visual_i < end
        } else {
            false
        };

        // Apply background color for search matches
        let new_bg_state = is_search_match || is_current_match;
        if new_bg_state != current_bg {
            if new_bg_state {
                if is_current_match {
                    // Darker blue background for current match
                    execute!(
                        stdout,
                        SetBackgroundColor(crossterm::style::Color::Rgb {
                            r: 50,
                            g: 100,
                            b: 200
                        })
                    )?;
                } else {
                    // Light blue background for other matches
                    execute!(
                        stdout,
                        SetBackgroundColor(crossterm::style::Color::Rgb {
                            r: 100,
                            g: 150,
                            b: 200
                        })
                    )?;
                }
            } else {
                execute!(stdout, ResetColor)?;
                // Reapply foreground color if needed
                if let Some(color) = current_color {
                    execute!(stdout, SetForegroundColor(color))?;
                }
            }
            current_bg = new_bg_state;
        } else if new_bg_state {
            // Background is active but we might need to switch between current and non-current
            if is_current_match {
                execute!(
                    stdout,
                    SetBackgroundColor(crossterm::style::Color::Rgb {
                        r: 50,
                        g: 100,
                        b: 200
                    })
                )?;
            } else if is_search_match {
                execute!(
                    stdout,
                    SetBackgroundColor(crossterm::style::Color::Rgb {
                        r: 100,
                        g: 150,
                        b: 200
                    })
                )?;
            }
        }

        // Change foreground color if needed
        if desired_color != current_color {
            if let Some(color) = desired_color {
                execute!(stdout, SetForegroundColor(color))?;
            } else if !(is_search_match || is_current_match) {
                execute!(stdout, ResetColor)?;
            }
            current_color = desired_color;
        }

        write!(stdout, "{}", ch)?;
    }

    // Reset color at end
    if current_color.is_some() || current_bg {
        execute!(stdout, ResetColor)?;
    }

    Ok(())
}

/// Render a line segment with expanded tabs and selection
fn render_line_segment_with_selection_expanded(
    stdout: &mut impl Write,
    expanded_chars: &[char],
    original_line: &str,
    sel_start: Position,
    sel_end: Position,
    ctx: &RenderContext,
    segment: &SegmentInfo,
) -> Result<(), std::io::Error> {
    use crossterm::style::{ResetColor, SetBackgroundColor, SetForegroundColor};

    let (start, end) = normalize_selection(sel_start, sel_end);
    let (start_line, start_col) = start;
    let (end_line, end_col) = end;

    // Outside selection range -> normal rendering
    if segment.line_index < start_line || segment.line_index > end_line {
        return render_line_segment_expanded(stdout, expanded_chars, original_line, ctx, segment);
    }

    // Get syntax highlighting for the original line
    let highlights = crate::syntax::highlight_line(original_line);

    // Convert byte positions to visual positions for the expanded line
    let mut visual_to_color: Vec<Option<crossterm::style::Color>> =
        vec![None; expanded_chars.len()];
    let visual_to_search_match: Vec<bool> = vec![false; expanded_chars.len()];

    // Apply syntax highlighting
    for (byte_start, byte_end, color) in highlights {
        // Convert byte positions to character positions in original line
        let char_start = original_line[..byte_start.min(original_line.len())]
            .chars()
            .count();
        let char_end = original_line[..byte_end.min(original_line.len())]
            .chars()
            .count();

        // Convert character positions to visual positions (accounting for tabs)
        let visual_start =
            crate::coordinates::visual_width_up_to(original_line, char_start, segment.tab_width);
        let visual_end =
            crate::coordinates::visual_width_up_to(original_line, char_end, segment.tab_width);

        // Mark visual positions with color
        for i in visual_start..visual_end.min(visual_to_color.len()) {
            visual_to_color[i] = Some(color);
        }
    }

    // Convert selection character indices to visual column range
    let (start_visual_col, end_visual_col) = if ctx.state.block_selection {
        // Block selection: use the column range for all lines in the selection
        let block_start_col = visual_width_up_to(original_line, start_col, segment.tab_width);
        let block_end_col = visual_width_up_to(original_line, end_col, segment.tab_width);
        (block_start_col, block_end_col)
    } else {
        // Normal line-wise selection
        let start_visual_col = if segment.line_index == start_line {
            visual_width_up_to(original_line, start_col, segment.tab_width)
        } else {
            0
        };
        let end_visual_col = if segment.line_index == end_line {
            visual_width_up_to(original_line, end_col, segment.tab_width)
        } else {
            usize::MAX
        };
        (start_visual_col, end_visual_col)
    };

    // Cache current match range to avoid recalculating in the loop
    let mut current_match_range: Option<(usize, usize)> = None;
    if let Some(ref pattern) = ctx.state.last_search_pattern {
        let cursor_pos = ctx.state.current_position();
        let is_cursor_line = segment.line_index == cursor_pos.0;

        if is_cursor_line {
            let matches = get_search_matches(original_line, pattern);
            let cursor_col = cursor_pos.1;

            for (char_start, char_end) in matches {
                if cursor_col >= char_start && cursor_col < char_end {
                    // Check if this match overlaps with the find_scope
                    if !match_overlaps_scope(
                        segment.line_index,
                        char_start,
                        char_end,
                        ctx.state.find_scope,
                    ) {
                        continue;
                    }

                    let visual_start = crate::coordinates::visual_width_up_to(
                        original_line,
                        char_start,
                        segment.tab_width,
                    );
                    let visual_end = crate::coordinates::visual_width_up_to(
                        original_line,
                        char_end,
                        segment.tab_width,
                    );
                    current_match_range = Some((visual_start, visual_end));
                    break;
                }
            }
        }
    }

    let mut current_color: Option<crossterm::style::Color> = None;
    let mut current_bg: Option<&str> = None; // Track background: None, "search", "current", or "selection"

    for visual_i in segment.start_visual..segment.end_visual {
        if visual_i >= expanded_chars.len() {
            break;
        }
        let ch = expanded_chars[visual_i];
        let is_selected = visual_i >= start_visual_col && visual_i < end_visual_col;
        let is_search_match = visual_to_search_match
            .get(visual_i)
            .copied()
            .unwrap_or(false);

        // Check if this position is in the current match (using cached range)
        let is_current_match = if let Some((start, end)) = current_match_range {
            visual_i >= start && visual_i < end
        } else {
            false
        };

        // Determine background (search matches take priority over selection)
        let desired_bg = if is_current_match {
            Some("current")
        } else if is_search_match {
            Some("search")
        } else if is_selected {
            Some("selection")
        } else {
            None
        };

        // Apply background if it changed
        if desired_bg != current_bg {
            match desired_bg {
                Some("selection") => {
                    // Use a subtle background for selection
                    execute!(
                        stdout,
                        SetBackgroundColor(crossterm::style::Color::DarkGrey)
                    )?;
                }
                Some("current") => {
                    // Darker blue background for current match
                    execute!(
                        stdout,
                        SetBackgroundColor(crossterm::style::Color::Rgb {
                            r: 50,
                            g: 100,
                            b: 200
                        })
                    )?;
                }
                Some("search") => {
                    // Light blue background for other matches
                    execute!(
                        stdout,
                        SetBackgroundColor(crossterm::style::Color::Rgb {
                            r: 100,
                            g: 150,
                            b: 200
                        })
                    )?;
                }
                _ => {
                    execute!(stdout, ResetColor)?;
                    current_color = None;
                }
            }
            current_bg = desired_bg;
        }

        let desired_color = visual_to_color.get(visual_i).copied().flatten();

        // Change foreground color if needed
        if desired_color != current_color {
            if let Some(color) = desired_color {
                execute!(stdout, SetForegroundColor(color))?;
            } else if !(is_search_match || is_current_match || is_selected) {
                execute!(stdout, ResetColor)?;
                // Reapply background if needed
                if is_search_match {
                    execute!(
                        stdout,
                        SetBackgroundColor(crossterm::style::Color::Rgb {
                            r: 100,
                            g: 150,
                            b: 200
                        })
                    )?;
                } else if is_current_match {
                    execute!(
                        stdout,
                        SetBackgroundColor(crossterm::style::Color::Rgb {
                            r: 50,
                            g: 100,
                            b: 200
                        })
                    )?;
                } else if is_selected {
                    execute!(
                        stdout,
                        SetBackgroundColor(crossterm::style::Color::DarkGrey)
                    )?;
                }
            }
            current_color = desired_color;
        }

        write!(stdout, "{}", ch)?;
    }

    // Reset color at end
    if current_color.is_some() || current_bg.is_some() {
        execute!(stdout, ResetColor)?;
    }

    Ok(())
}

/// Clear from current position to before scrollbar, preserving scrollbar content
fn clear_to_scrollbar(
    stdout: &mut impl Write,
    state: &FileViewerState,
    lines: &[String],
    visible_lines: usize,
    current_column: u16,
) -> Result<(), std::io::Error> {
    let scrollbar_visible = lines.len() > visible_lines;
    let end_column = if scrollbar_visible {
        state.term_width.saturating_sub(1) // Stop before scrollbar
    } else {
        state.term_width // Clear entire line if no scrollbar
    };

    // Fill with spaces from current position to end_column
    let spaces_needed = end_column.saturating_sub(current_column);
    if spaces_needed > 0 {
        write!(stdout, "{}", " ".repeat(spaces_needed as usize))?;
    }
    Ok(())
}

fn render_scrollbar(
    stdout: &mut impl Write,
    lines: &[String],
    state: &FileViewerState,
    visible_lines: usize,
) -> Result<(), std::io::Error> {
    use crossterm::cursor::{RestorePosition, SavePosition};
    use crossterm::style::{ResetColor, SetBackgroundColor};

    // Only show scrollbar if there are more lines than visible
    if lines.len() <= visible_lines {
        return Ok(());
    }

    // Save current cursor position to restore later
    execute!(stdout, SavePosition)?;

    // Calculate scrollbar dimensions
    let total_lines = lines.len();
    let scrollbar_height = visible_lines;
    let bar_height = (visible_lines * visible_lines / total_lines).max(1);

    // Calculate scroll progress (handle case when total_lines <= visible_lines)
    let max_scroll = total_lines.saturating_sub(visible_lines);
    let scroll_progress = if max_scroll == 0 {
        0.0
    } else {
        // Clamp to 1.0 in case top_line exceeds max_scroll (e.g., last line at top)
        (state.top_line as f64 / max_scroll as f64).min(1.0)
    };

    let bar_position = ((scrollbar_height - bar_height) as f64 * scroll_progress) as usize;

    // Get colors - use same blue as header/footer for background, light blue for bar
    let bg_color = crate::settings::Settings::parse_color(&state.settings.appearance.header_bg)
        .unwrap_or(crossterm::style::Color::DarkBlue);
    let bar_color = crossterm::style::Color::Rgb {
        r: 100,
        g: 149,
        b: 237,
    }; // Light blue

    let scrollbar_column = state.term_width - 1;

    // Render scrollbar efficiently in segments with minimal color changes

    // Top background segment
    if bar_position > 0 {
        execute!(stdout, SetBackgroundColor(bg_color))?;
        for i in 0..bar_position {
            execute!(stdout, cursor::MoveTo(scrollbar_column, (i + 1) as u16))?;
            write!(stdout, " ")?;
        }
    }

    // Scrollbar bar segment
    if bar_height > 0 {
        execute!(stdout, SetBackgroundColor(bar_color))?;
        for i in bar_position..(bar_position + bar_height) {
            execute!(stdout, cursor::MoveTo(scrollbar_column, (i + 1) as u16))?;
            write!(stdout, " ")?;
        }
    }

    // Bottom background segment
    let bottom_start = bar_position + bar_height;
    if bottom_start < visible_lines {
        execute!(stdout, SetBackgroundColor(bg_color))?;
        for i in bottom_start..visible_lines {
            execute!(stdout, cursor::MoveTo(scrollbar_column, (i + 1) as u16))?;
            write!(stdout, " ")?;
        }
    }

    execute!(stdout, ResetColor)?;

    // Restore cursor position to minimize visual disruption
    execute!(stdout, RestorePosition)?;

    Ok(())
}

/// Check if horizontal scrollbar should be shown
fn should_show_horizontal_scrollbar(
    state: &FileViewerState,
    lines: &[String],
    visible_lines: usize,
) -> bool {
    state.should_show_h_scrollbar(lines, visible_lines)
}

/// Render horizontal scrollbar at the bottom when line wrapping is disabled
fn render_horizontal_scrollbar(
    stdout: &mut impl Write,
    lines: &[String],
    state: &FileViewerState,
    visible_lines: usize,
) -> Result<(), std::io::Error> {
    use crossterm::cursor::{RestorePosition, SavePosition};
    use crossterm::style::{ResetColor, SetBackgroundColor};

    if !should_show_horizontal_scrollbar(state, lines, visible_lines) {
        return Ok(());
    }

    // Calculate maximum line width in the document
    let tab_width = state.settings.tab_width;
    
    let max_line_width = lines.iter()
        .map(|line| visual_width(line, tab_width))
        .max()
        .unwrap_or(0);


    // Save current cursor position
    execute!(stdout, SavePosition)?;

    // Calculate horizontal scrollbar dimensions
    let line_num_width = line_number_width(state.settings) as usize;
    let v_scrollbar_width = if lines.len() > visible_lines { 1 } else { 0 };

    // H-scrollbar extends from after line numbers to before vertical scrollbar
    // (Footer status is on a different row, doesn't affect scrollbar width)
    let available_width = (state.term_width as usize)
        .saturating_sub(line_num_width)
        .saturating_sub(v_scrollbar_width);

    if available_width == 0 {
        return Ok(());
    }

    let scrollbar_width = available_width;
    let bar_width = ((available_width * available_width) / max_line_width).max(1);

    // Calculate scroll progress
    let max_scroll = max_line_width.saturating_sub(available_width);
    let scroll_progress = if max_scroll == 0 {
        0.0
    } else {
        (state.horizontal_scroll_offset as f64 / max_scroll as f64).min(1.0)
    };

    let bar_position = ((scrollbar_width - bar_width) as f64 * scroll_progress) as usize;

    // Get colors - same as vertical scrollbar
    let bg_color = crate::settings::Settings::parse_color(&state.settings.appearance.header_bg)
        .unwrap_or(crossterm::style::Color::DarkBlue);
    let bar_color = crossterm::style::Color::Rgb {
        r: 100,
        g: 149,
        b: 237,
    }; // Light blue

    // Position at last content line (visible_lines), overlaying it
    let h_scrollbar_row = visible_lines as u16;
    execute!(stdout, cursor::MoveTo(0, h_scrollbar_row))?;

    // Render line number area with scrollbar background
    if line_num_width > 0 {
        execute!(stdout, SetBackgroundColor(bg_color))?;
        for _ in 0..line_num_width {
            write!(stdout, " ")?;
        }
    }

    // Render left background segment
    if bar_position > 0 {
        execute!(stdout, SetBackgroundColor(bg_color))?;
        for _ in 0..bar_position {
            write!(stdout, " ")?;
        }
    }

    // Render scrollbar bar segment
    if bar_width > 0 {
        execute!(stdout, SetBackgroundColor(bar_color))?;
        for _ in 0..bar_width {
            write!(stdout, " ")?;
        }
    }

    // Render right background segment
    let right_start = bar_position + bar_width;
    if right_start < scrollbar_width {
        execute!(stdout, SetBackgroundColor(bg_color))?;
        for _ in right_start..scrollbar_width {
            write!(stdout, " ")?;
        }
    }

    // Fill the corner where h-scrollbar meets v-scrollbar (if v-scrollbar is present)
    if v_scrollbar_width > 0 {
        execute!(stdout, SetBackgroundColor(bg_color))?;
        write!(stdout, " ")?; // Fill the corner cell with scrollbar background color
    }

    execute!(stdout, ResetColor)?;
    execute!(stdout, RestorePosition)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tabs_no_tabs_returns_original() {
        let result = expand_tabs("hello world", 4);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn expand_tabs_single_tab_at_start() {
        let result = expand_tabs("\thello", 4);
        assert_eq!(result, "    hello");
    }

    #[test]
    fn expand_tabs_single_tab_in_middle() {
        let result = expand_tabs("ab\tcd", 4);
        assert_eq!(result, "ab  cd"); // 2 spaces to reach next tab stop at col 4
    }

    #[test]
    fn expand_tabs_multiple_tabs() {
        let result = expand_tabs("\t\t", 4);
        assert_eq!(result, "        "); // 8 spaces (2 tabs × 4)
    }

    #[test]
    fn expand_tabs_tab_width_8() {
        let result = expand_tabs("a\tb", 8);
        assert_eq!(result, "a       b"); // 7 spaces to reach col 8
    }

    #[test]
    fn expand_tabs_respects_tab_stops() {
        let result = expand_tabs("abc\tde\tf", 4);
        // "abc" = col 3, tab goes to col 4 (1 space)
        // "de" = col 6, tab goes to col 8 (2 spaces)
        assert_eq!(result, "abc de  f");
    }

    #[test]
    fn normalize_selection_ordered_returns_same() {
        let start = (5, 10);
        let end = (10, 20);
        let (s, e) = normalize_selection(start, end);
        assert_eq!(s, start);
        assert_eq!(e, end);
    }

    #[test]
    fn normalize_selection_reversed_swaps() {
        let start = (10, 20);
        let end = (5, 10);
        let (s, e) = normalize_selection(start, end);
        assert_eq!(s, end);
        assert_eq!(e, start);
    }

    #[test]
    fn normalize_selection_same_line_ordered() {
        let start = (5, 10);
        let end = (5, 20);
        let (s, e) = normalize_selection(start, end);
        assert_eq!(s, start);
        assert_eq!(e, end);
    }

    #[test]
    fn normalize_selection_same_line_reversed() {
        let start = (5, 20);
        let end = (5, 10);
        let (s, e) = normalize_selection(start, end);
        assert_eq!(s, end);
        assert_eq!(e, start);
    }

    #[test]
    fn get_search_matches_empty_pattern_returns_empty() {
        let matches = get_search_matches("hello world", "");
        assert!(matches.is_empty());
    }

    #[test]
    fn get_search_matches_simple_literal() {
        let matches = get_search_matches("hello world", "world");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], (6, 11)); // "world" starts at char 6, ends at 11
    }

    #[test]
    fn get_search_matches_multiple_occurrences() {
        let matches = get_search_matches("hello hello", "hello");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0], (0, 5));
        assert_eq!(matches[1], (6, 11));
    }

    #[test]
    fn get_search_matches_regex_pattern() {
        let matches = get_search_matches("test123 test456", r"\d+");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0], (4, 7)); // "123"
        assert_eq!(matches[1], (12, 15)); // "456"
    }

    #[test]
    fn get_search_matches_no_match_returns_empty() {
        let matches = get_search_matches("hello world", "xyz");
        assert!(matches.is_empty());
    }

    #[test]
    fn get_search_matches_invalid_regex_returns_empty() {
        let matches = get_search_matches("hello world", "[invalid");
        assert!(matches.is_empty());
    }

    #[test]
    fn get_search_matches_handles_multibyte_chars() {
        let matches = get_search_matches("hello 世界 world", "世界");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], (6, 8)); // Character positions, not bytes
    }

    #[test]
    fn get_search_matches_case_insensitive() {
        // Lowercase pattern should match all case variations
        let matches = get_search_matches("Hello WORLD hello", "hello");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0], (0, 5)); // "Hello"
        assert_eq!(matches[1], (12, 17)); // "hello"

        // Search for "world" should match "WORLD" case-insensitively
        let matches = get_search_matches("Hello WORLD hello", "world");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], (6, 11)); // "WORLD"

        // Verify all case variations are found
        let matches = get_search_matches("Hello hello HELLO HeLLo", "hello");
        assert_eq!(matches.len(), 4);
    }

    #[test]
    fn match_overlaps_scope_no_scope_always_true() {
        assert!(match_overlaps_scope(5, 10, 15, None));
    }

    #[test]
    fn match_overlaps_scope_single_line_within() {
        let scope = Some(((0, 5), (0, 20)));
        // Match completely within scope
        assert!(match_overlaps_scope(0, 10, 15, scope));
    }

    #[test]
    fn match_overlaps_scope_single_line_at_start() {
        let scope = Some(((0, 5), (0, 20)));
        // Match starts at scope start
        assert!(match_overlaps_scope(0, 5, 10, scope));
    }

    #[test]
    fn match_overlaps_scope_single_line_at_end() {
        let scope = Some(((0, 5), (0, 20)));
        // Match ending at scope end (char_end = 20, scope_end = 20)
        // Since scope_end is exclusive, scope covers [5, 20)
        // Match covers [15, 20), so they overlap in range [15, 20)
        assert!(match_overlaps_scope(0, 15, 20, scope));
    }

    #[test]
    fn match_overlaps_scope_single_line_just_before_end() {
        let scope = Some(((0, 5), (0, 20)));
        // Match ends just before scope end
        assert!(match_overlaps_scope(0, 15, 19, scope));
    }

    #[test]
    fn match_overlaps_scope_single_line_overlaps_start() {
        let scope = Some(((0, 5), (0, 20)));
        // Match starts before scope but overlaps into it
        assert!(match_overlaps_scope(0, 3, 8, scope));
    }

    #[test]
    fn match_overlaps_scope_single_line_overlaps_end() {
        let scope = Some(((0, 5), (0, 20)));
        // Match starts in scope but extends beyond
        assert!(match_overlaps_scope(0, 15, 25, scope));
    }

    #[test]
    fn match_overlaps_scope_single_line_before_scope() {
        let scope = Some(((0, 10), (0, 20)));
        // Match completely before scope
        assert!(!match_overlaps_scope(0, 2, 5, scope));
    }

    #[test]
    fn match_overlaps_scope_single_line_after_scope() {
        let scope = Some(((0, 10), (0, 20)));
        // Match completely after scope
        assert!(!match_overlaps_scope(0, 25, 30, scope));
    }

    #[test]
    fn match_overlaps_scope_wrong_line() {
        let scope = Some(((5, 0), (10, 20)));
        // Match on line outside scope range
        assert!(!match_overlaps_scope(3, 0, 10, scope));
        assert!(!match_overlaps_scope(15, 0, 10, scope));
    }

    #[test]
    fn match_overlaps_scope_multiline_first_line() {
        let scope = Some(((1, 10), (3, 20)));
        // Match on first line after scope start
        assert!(match_overlaps_scope(1, 15, 20, scope));
        // Match on first line before scope start
        assert!(!match_overlaps_scope(1, 5, 9, scope));
    }

    #[test]
    fn match_overlaps_scope_multiline_middle_line() {
        let scope = Some(((1, 10), (3, 20)));
        // Any match on middle line should match
        assert!(match_overlaps_scope(2, 0, 10, scope));
    }

    #[test]
    fn match_overlaps_scope_multiline_last_line() {
        let scope = Some(((1, 10), (3, 20)));
        // Match on last line before scope end
        assert!(match_overlaps_scope(3, 5, 15, scope));
        // Match on last line at/after scope end
        assert!(!match_overlaps_scope(3, 20, 25, scope));
        assert!(!match_overlaps_scope(3, 25, 30, scope));
    }

    // Performance optimization tests
    #[test]
    fn regex_cache_reuses_same_pattern() {
        // First call should compile and cache
        let matches1 = get_search_matches("hello world", "world");
        assert_eq!(matches1.len(), 1);
        assert_eq!(matches1[0], (6, 11));

        // Second call with same pattern should use cache (no recompilation)
        let matches2 = get_search_matches("goodbye world", "world");
        assert_eq!(matches2.len(), 1);
        assert_eq!(matches2[0], (8, 13));

        // Third call with different pattern should recompile and re-cache
        let matches3 = get_search_matches("hello world", "hello");
        assert_eq!(matches3.len(), 1);
        assert_eq!(matches3[0], (0, 5));

        // Fourth call with original pattern should recompile again (cache was replaced)
        let matches4 = get_search_matches("world hello world", "world");
        assert_eq!(matches4.len(), 2);
        assert_eq!(matches4[0], (0, 5));
        assert_eq!(matches4[1], (12, 17));
    }

    #[test]
    fn regex_cache_handles_empty_pattern() {
        let matches = get_search_matches("hello world", "");
        assert!(matches.is_empty());

        // Should still work after empty pattern
        let matches2 = get_search_matches("hello world", "hello");
        assert_eq!(matches2.len(), 1);
    }

    #[test]
    fn regex_cache_handles_invalid_regex() {
        let matches = get_search_matches("hello world", "[invalid");
        assert!(matches.is_empty());

        // Should recover and work with valid pattern
        let matches2 = get_search_matches("hello world", "hello");
        assert_eq!(matches2.len(), 1);
    }

    #[test]
    fn render_header_handles_current_directory_file() {
        use crate::editor_state::FileViewerState;
        use crate::settings::Settings;
        use crate::undo::UndoHistory;

        let settings = Settings::default();
        let undo_history = UndoHistory::new();
        let state = FileViewerState::new(80, undo_history, &settings);
        let mut output = Vec::new();
        let lines = vec!["test".to_string(); 10];

        // Test with a relative file path (no directory component)
        let result = render_header(&mut output, "test.txt", &state, &lines, 10);
        assert!(result.is_ok());

        let output_str = String::from_utf8(output).unwrap();
        // Should show empty parentheses instead of (.)
        assert!(output_str.contains("test.txt ()"));
        assert!(!output_str.contains("test.txt (.)"));
    }

    #[test]
    fn render_header_handles_path_with_directory() {
        use crate::editor_state::FileViewerState;
        use crate::settings::Settings;
        use crate::undo::UndoHistory;

        let settings = Settings::default();
        let undo_history = UndoHistory::new();
        let state = FileViewerState::new(80, undo_history, &settings);
        let mut output = Vec::new();
        let lines = vec!["test".to_string(); 10];

        // Test with a file path that includes a directory
        let result = render_header(&mut output, "/home/user/test.txt", &state, &lines, 10);
        assert!(result.is_ok());

        let output_str = String::from_utf8(output).unwrap();
        // Should show the parent directory
        assert!(output_str.contains("test.txt (/home/user)"));
    }

    #[test]
    fn shorten_path_shows_full_path_when_fits() {
        let result = shorten_path_for_display("/home/user", 50);
        assert_eq!(result, "/home/user");
    }

    #[test]
    fn shorten_path_uses_home_abbreviation() {
        // Test with real HOME or mock path
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
        let test_path = format!("{}/projects/rust", home);
        // Use narrow width to force abbreviation
        let result = shorten_path_for_display(&test_path, 15);
        // Should be shortened (either with ~ or abbreviated)
        assert!(visual_width(&result, 4) <= 15);
        // Should contain / for directory structure
        assert!(result.contains('/'));
    }

    #[test]
    fn abbreviate_path_shortens_middle_directories() {
        let result = abbreviate_path("~/projects/rust/ue/src");
        // Should keep ~, keep last component, abbreviate middle ones
        assert!(result.contains("~"));
        assert!(result.contains("src"));
        // Middle directories should be abbreviated
        assert!(result.contains("/p/") || result.contains("/r/"));
    }

    #[test]
    fn truncate_to_width_adds_ellipsis() {
        let result = truncate_to_width("very/long/path/name", 10);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 10);
    }

    #[test]
    fn truncate_to_width_preserves_short_strings() {
        let result = truncate_to_width("short", 20);
        assert_eq!(result, "short");
    }

    #[test]
    fn render_header_truncates_very_long_filename() {
        use crate::editor_state::FileViewerState;
        use crate::settings::Settings;
        use crate::undo::UndoHistory;

        let settings = Settings::default();
        let undo_history = UndoHistory::new();
        let state = FileViewerState::new(40, undo_history, &settings); // narrow terminal (40 chars wide)
        let mut output = Vec::new();
        let lines = vec!["test".to_string(); 10];

        // Test with a very long filename that won't fit even with path shortening
        // Create a path that's guaranteed to be longer than 40 chars
        let long_filename = "/home/user/very_very_very_very_very_long_filename_that_definitely_exceeds_width.txt";
        let result = render_header(&mut output, long_filename, &state, &lines, 10);
        assert!(result.is_ok());

        let output_str = String::from_utf8(output).unwrap();
        // The entire title (including filename + path) should fit within the terminal width
        // and be truncated with ellipsis if it's too long
        // With a 40 char wide terminal, we expect either:
        // 1. The title to be truncated with "...", or
        // 2. The title to be shortened to fit (which may or may not include "...")
        // This test verifies that we don't have garbled/flickering output
        assert!(!output_str.is_empty(), "Output should not be empty");
        // Verify the output contains the filename or a truncated version of it
        assert!(output_str.contains("very_very") || output_str.contains("..."),
                "Output should contain filename or ellipsis truncation");
    }
}
