use crossterm::{
    cursor, execute,
    style::{ResetColor, SetBackgroundColor},
    terminal::{self, ClearType},
};
use std::io::Write;

use crate::coordinates::{
    calculate_cursor_visual_line, calculate_wrapped_lines_for_line, line_number_width,
    visual_width_up_to,
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

pub(crate) fn render_screen(
    stdout: &mut impl Write,
    file: &str,
    lines: &[String],
    state: &FileViewerState,
    visible_lines: usize,
) -> Result<(), std::io::Error> {
    execute!(stdout, cursor::Hide)?;
    execute!(stdout, cursor::MoveTo(0, 0))?;

    render_header(stdout, file, state, lines.len())?;
    render_visible_lines(stdout, file, lines, state, visible_lines)?;
    render_scrollbar(stdout, lines, state, visible_lines)?;
    render_footer(stdout, state, lines, visible_lines)?;
    position_cursor(stdout, lines, state, visible_lines)?;

    stdout.flush()?;
    Ok(())
}

fn render_header(
    stdout: &mut impl Write,
    file: &str,
    state: &FileViewerState,
    _total_lines: usize,
) -> Result<(), std::io::Error> {
    let modified_char = if state.modified { '*' } else { ' ' };
    let path = std::path::Path::new(file);
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or(file);
    let parent = path.parent().and_then(|p| p.to_str()).unwrap_or(".");
    // If parent is "." (current directory), show empty string instead
    let parent_display = if parent == "." { "" } else { parent };
    if let Some(color) =
        crate::settings::Settings::parse_color(&state.settings.appearance.header_bg)
    {
        execute!(stdout, SetBackgroundColor(color))?;
    }
    if state.settings.appearance.line_number_digits > 0 {
        let modulus = 10usize.pow(state.settings.appearance.line_number_digits as u32);
        let top_number = (state.top_line / modulus) * modulus;
        // Add trailing space explicitly after the block number
        write!(
            stdout,
            "{:width$} ",
            top_number,
            width = state.settings.appearance.line_number_digits as usize
        )?;
    }
    write!(
        stdout,
        "{} {} ({})",
        modified_char, filename, parent_display
    )?;
    // Header row doesn't interfere with scrollbar, but clear consistently
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
    if let Some(color) =
        crate::settings::Settings::parse_color(&state.settings.appearance.footer_bg)
    {
        execute!(stdout, SetBackgroundColor(color))?;
    }

    // If in find mode, show the find prompt
    if state.find_active {
        let digits = state.settings.appearance.line_number_digits as usize;
        let mut prompt = String::new();

        // Add line number space if needed
        if digits > 0 {
            prompt.push_str(&format!("{:width$} ", "", width = digits));
        }

        // Add find prompt and input field
        let find_label = "Find (regex): ";
        prompt.push_str(find_label);
        let pattern_start_col = prompt.len();
        prompt.push_str(&state.find_pattern);

        // Show error in red if present, followed by the input field
        write!(stdout, "\r")?;
        if let Some(ref error) = state.find_error {
            use crossterm::style::SetForegroundColor;
            execute!(stdout, SetForegroundColor(crossterm::style::Color::Red))?;
            write!(stdout, "[{}] ", error)?;
            execute!(stdout, ResetColor)?;
            if let Some(color) =
                crate::settings::Settings::parse_color(&state.settings.appearance.footer_bg)
            {
                execute!(stdout, SetBackgroundColor(color))?;
            }
        }
        write!(stdout, "{}", prompt)?;
        // Footer row doesn't interfere with scrollbar, but clear consistently
        execute!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
        execute!(stdout, ResetColor)?;

        // Position cursor at find_cursor_pos within the search pattern
        // Footer is at row: 1 (header) + visible_lines
        // Account for error message length if present
        let error_offset = if let Some(ref error) = state.find_error {
            error.len() + 3 // "[" + error + "] "
        } else {
            0
        };
        // Calculate visual column for cursor position (accounting for multi-byte chars)
        let chars: Vec<char> = state.find_pattern.chars().collect();
        let cursor_offset = chars.iter().take(state.find_cursor_pos).count();
        let cursor_x = (error_offset + pattern_start_col + cursor_offset) as u16;
        let cursor_y = (visible_lines + 1) as u16;
        execute!(stdout, cursor::MoveTo(cursor_x, cursor_y))?;
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

    let total_width = state.term_width as usize;
    let digits = state.settings.appearance.line_number_digits as usize;
    let mut bottom_number_str = String::new();
    if digits > 0 {
        let modulus = 10usize.pow(digits as u32);
        let mut last_visible_line = state.top_line;
        let mut remaining = visible_lines;
        let text_width = crate::coordinates::calculate_text_width(state, lines, visible_lines);
        let tab_width = state.settings.tab_width;
        while remaining > 0 && last_visible_line < lines.len() {
            let wrapped =
                calculate_wrapped_lines_for_line(lines, last_visible_line, text_width, tab_width)
                    as usize;
            if wrapped <= remaining {
                remaining -= wrapped;
                last_visible_line += 1;
            } else {
                break;
            }
        }
        let bottom_number = (last_visible_line / modulus) * modulus;
        bottom_number_str = format!("{:width$} ", bottom_number, width = digits);
    }

    write!(stdout, "\r{}", bottom_number_str)?;
    let left_len = bottom_number_str.len();
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
    let mut visual_lines_rendered = 0;
    let mut logical_line_index = state.top_line;
    let text_width_u16 = crate::coordinates::calculate_text_width(state, lines, visible_lines);
    let _text_width_usize = text_width_u16 as usize;

    // Calculate which visual line the cursor is on
    let cursor_visual_line = calculate_cursor_visual_line(lines, state, text_width_u16);

    let ctx = RenderContext {
        lines,
        state,
        visible_lines,
    };

    while visual_lines_rendered < visible_lines && logical_line_index < lines.len() {
        let lines_for_this_logical = render_line(
            stdout,
            &ctx,
            logical_line_index,
            cursor_visual_line,
            visual_lines_rendered,
            visible_lines - visual_lines_rendered,
        )?;

        visual_lines_rendered += lines_for_this_logical;
        logical_line_index += 1;
    }

    // Fill remaining visible lines with empty lines
    while visual_lines_rendered < visible_lines {
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

    let num_wrapped_lines = calculate_wrapped_lines_for_line(
        ctx.lines,
        logical_line_index,
        crate::coordinates::calculate_text_width(ctx.state, ctx.lines, ctx.visible_lines),
        tab_width,
    );

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
                if let Some(color) = crate::settings::Settings::parse_color(
                    &ctx.state.settings.appearance.line_numbers_bg,
                ) {
                    execute!(stdout, SetBackgroundColor(color))?;
                }
                write!(
                    stdout,
                    "{:width$} ",
                    line_num,
                    width = ctx.state.settings.appearance.line_number_digits as usize
                )?;
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

        let start_visual = wrap_index * available_width;
        let end_visual = ((wrap_index + 1) * available_width).min(chars.len());

        if start_visual < chars.len() {
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
        }

        // Calculate current column position after rendering content
        let current_col = if ctx.state.settings.appearance.line_number_digits > 0 {
            let line_num_width = ctx.state.settings.appearance.line_number_digits as u16 + 1;
            let content_width = (end_visual - start_visual) as u16;
            line_num_width + content_width
        } else {
            (end_visual - start_visual) as u16
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

fn position_cursor(
    stdout: &mut impl Write,
    lines: &[String],
    state: &FileViewerState,
    visible_lines: usize,
) -> Result<(), std::io::Error> {
    // If in find mode, cursor is already positioned in the search field by render_footer
    if state.find_active {
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
            let mut cursor_y = 1u16;
            for logical in state.top_line..target_line {
                cursor_y += calculate_wrapped_lines_for_line(lines, logical, text_width, tab_width);
            }
            let visual_col = visual_width_up_to(
                &lines[target_line],
                target_col.min(lines[target_line].len()),
                tab_width,
            );
            let wrapped_line = visual_col / (text_width as usize);
            cursor_y += wrapped_line as u16;
            let cursor_x = (visual_col % (text_width as usize)) as u16 + line_num_width;
            execute!(stdout, cursor::MoveTo(cursor_x, cursor_y))?;
            apply_cursor_shape(stdout, state.settings)?;
            execute!(stdout, cursor::Show)?;
            return Ok(());
        }
    }
    if !state.is_cursor_visible(lines, visible_lines, text_width) {
        return Ok(());
    }
    let tab_width = state.settings.tab_width;
    let mut cursor_y = 1u16;
    for i in 0..state.cursor_line {
        cursor_y +=
            calculate_wrapped_lines_for_line(lines, state.top_line + i, text_width, tab_width);
    }
    let cursor_line_idx = state.absolute_line();
    let visual_col = if cursor_line_idx < lines.len() {
        visual_width_up_to(&lines[cursor_line_idx], state.cursor_col, tab_width)
    } else {
        0
    };
    let cursor_wrapped_line = visual_col / (text_width as usize);
    cursor_y += cursor_wrapped_line as u16;
    let cursor_x = (visual_col % (text_width as usize)) as u16 + line_num_width;
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
            let visual_end =
                crate::coordinates::visual_width_up_to(original_line, char_end, segment.tab_width);

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

        // Test with a relative file path (no directory component)
        let result = render_header(&mut output, "test.txt", &state, 10);
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

        // Test with a file path that includes a directory
        let result = render_header(&mut output, "/home/user/test.txt", &state, 10);
        assert!(result.is_ok());

        let output_str = String::from_utf8(output).unwrap();
        // Should show the parent directory
        assert!(output_str.contains("test.txt (/home/user)"));
    }
}
