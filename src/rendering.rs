use std::io::Write;
use crossterm::{
    cursor,
    execute,
    style::{Stylize, ResetColor, SetBackgroundColor},
    terminal::{self, ClearType},
};

use crate::coordinates::{calculate_cursor_visual_line, calculate_wrapped_lines_for_line, line_number_width, visual_width_up_to};
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
    let mode_info = if state.has_selection() { " [SELECTING]" } else { "" };
    let modified_char = if state.modified { '*' } else { ' ' };
    let path = std::path::Path::new(file);
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or(file);
    let parent = path.parent().and_then(|p| p.to_str()).unwrap_or(".");
    if let Some(color) = crate::settings::Settings::parse_color(&state.settings.appearance.header_bg) {
        execute!(stdout, SetBackgroundColor(color))?;
    }
    if state.settings.appearance.line_number_digits > 0 {
        let modulus = 10usize.pow(state.settings.appearance.line_number_digits as u32);
        let top_number = (state.top_line / modulus) * modulus;
        // Add trailing space explicitly after the block number
        write!(stdout, "{:width$} ", top_number, width = state.settings.appearance.line_number_digits as usize)?;
    }
    write!(stdout, "{} {} ({}){}", modified_char, filename, parent, mode_info)?;
    execute!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
    execute!(stdout, ResetColor)?;
    write!(stdout, "\r\n")?;
    Ok(())
}

fn render_footer(
    stdout: &mut impl Write,
    state: &FileViewerState,
    lines: &[String],
    visible_lines: usize,
) -> Result<(), std::io::Error> {
    if let Some(color) = crate::settings::Settings::parse_color(&state.settings.appearance.footer_bg) {
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
            if let Some(color) = crate::settings::Settings::parse_color(&state.settings.appearance.footer_bg) {
                execute!(stdout, SetBackgroundColor(color))?;
            }
        }
        write!(stdout, "{}", prompt)?;
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
    let position_info = format!("{}:{}", line_num, col_num);
    let total_width = state.term_width as usize;
    let digits = state.settings.appearance.line_number_digits as usize;
    let mut bottom_number_str = String::new();
    if digits > 0 {
        let modulus = 10usize.pow(digits as u32);
        let mut last_visible_line = state.top_line;
        let mut remaining = visible_lines;
        let text_width = state.term_width.saturating_sub(line_number_width(state.settings));
        let tab_width = state.settings.tab_width;
        while remaining > 0 && last_visible_line < lines.len() {
            let wrapped = calculate_wrapped_lines_for_line(lines, last_visible_line, text_width, tab_width) as usize;
            if wrapped <= remaining { remaining -= wrapped; last_visible_line += 1; } else { break; }
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
        if let Some(color) = crate::settings::Settings::parse_color(&state.settings.appearance.footer_bg) {
            execute!(stdout, SetBackgroundColor(color))?;
        }
    } else if position_info.len() >= remaining_width {
        let truncated = &position_info[position_info.len() - remaining_width..];
        write!(stdout, "{}", truncated)?;
    } else {
        let pad = remaining_width - position_info.len();
        for _ in 0..pad { write!(stdout, " ")?; }
        write!(stdout, "{}", position_info)?;
    }
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
    let line_num_width = line_number_width(state.settings);
    let text_width_u16 = state.term_width.saturating_sub(line_num_width);
    let _text_width_usize = text_width_u16 as usize;
    
    // Calculate which visual line the cursor is on
    let cursor_visual_line = calculate_cursor_visual_line(lines, state, text_width_u16);
    
    let ctx = RenderContext { lines, state };
    
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
            if let Some(color) = crate::settings::Settings::parse_color(&state.settings.appearance.line_numbers_bg) {
                execute!(stdout, SetBackgroundColor(color))?;
            }
            write!(stdout, "{:width$} ", "", width = state.settings.appearance.line_number_digits as usize)?;
            execute!(stdout, ResetColor)?;
        }
        execute!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
        write!(stdout, "\r\n")?;
        visual_lines_rendered += 1;
    }
    
    Ok(())
}

struct RenderContext<'a> {
    lines: &'a [String],
    state: &'a FileViewerState<'a>,
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
    let line_num_width = line_number_width(ctx.state.settings);
    let available_width = ctx.state.term_width.saturating_sub(line_num_width) as usize;
    let tab_width = ctx.state.settings.tab_width;
    
    // Expand tabs to spaces for display
    let expanded_line = expand_tabs(line, tab_width);
    let chars: Vec<char> = expanded_line.chars().collect();
    
    let num_wrapped_lines = calculate_wrapped_lines_for_line(ctx.lines, logical_line_index, ctx.state.term_width.saturating_sub(line_num_width), tab_width);
    
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
                if let Some(color) = crate::settings::Settings::parse_color(&ctx.state.settings.appearance.line_numbers_bg) { execute!(stdout, SetBackgroundColor(color))?; }
                write!(stdout, "{:width$} ", line_num, width = ctx.state.settings.appearance.line_number_digits as usize)?; execute!(stdout, ResetColor)?;
            } else {
                if let Some(color) = crate::settings::Settings::parse_color(&ctx.state.settings.appearance.line_numbers_bg) { execute!(stdout, SetBackgroundColor(color))?; }
                write!(stdout, "{:width$} ", "", width = ctx.state.settings.appearance.line_number_digits as usize)?; execute!(stdout, ResetColor)?;
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
            
            if let (Some(sel_start), Some(sel_end)) = (ctx.state.selection_start, ctx.state.selection_end) {
                render_line_segment_with_selection_expanded(stdout, &chars, line, sel_start, sel_end, ctx, &segment)?;
            } else {
                render_line_segment_expanded(stdout, &chars, line, ctx, &segment)?;
            }
        }
        
        execute!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
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

/// Get character ranges for search matches in a line
fn get_search_matches(line: &str, pattern: &str) -> Vec<(usize, usize)> {
    if pattern.is_empty() {
        return vec![];
    }
    
    if let Ok(regex) = regex::Regex::new(pattern) {
        regex.find_iter(line)
            .map(|m| {
                // Convert byte positions to character positions
                let char_start = line[..m.start()].chars().count();
                let char_end = line[..m.end()].chars().count();
                (char_start, char_end)
            })
            .collect()
    } else {
        vec![]
    }
}

fn apply_cursor_shape(stdout: &mut impl Write, settings: &crate::settings::Settings) -> std::io::Result<()> {
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
    
    let line_num_width = line_number_width(state.settings);
    let text_width = state.term_width.saturating_sub(line_num_width);
    if state.dragging_selection_active
        && let Some((target_line, target_col)) = state.drag_target {
            let tab_width = state.settings.tab_width;
            if target_line < lines.len() {
                let mut cursor_y = 1u16;
                for logical in state.top_line..target_line { cursor_y += calculate_wrapped_lines_for_line(lines, logical, text_width, tab_width); }
                let visual_col = visual_width_up_to(&lines[target_line], target_col.min(lines[target_line].len()), tab_width);
                let wrapped_line = visual_col / (text_width as usize);
                cursor_y += wrapped_line as u16;
                let cursor_x = (visual_col % (text_width as usize)) as u16 + line_num_width;
                execute!(stdout, cursor::MoveTo(cursor_x, cursor_y))?;
                apply_cursor_shape(stdout, state.settings)?;
                execute!(stdout, cursor::Show)?;
                return Ok(());
            }
        }
    if !state.is_cursor_visible(lines, visible_lines, text_width) { return Ok(()); }
    let tab_width = state.settings.tab_width;
    let mut cursor_y = 1u16;
    for i in 0..state.cursor_line { cursor_y += calculate_wrapped_lines_for_line(lines, state.top_line + i, text_width, tab_width); }
    let cursor_line_idx = state.absolute_line();
    let visual_col = if cursor_line_idx < lines.len() { visual_width_up_to(&lines[cursor_line_idx], state.cursor_col, tab_width) } else { 0 };
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
    use crossterm::style::{SetForegroundColor, SetBackgroundColor, ResetColor};
    
    // Get syntax highlighting for the original line
    let highlights = crate::syntax::highlight_line(original_line);
    
    // Convert byte positions to visual positions for the expanded line
    let mut visual_to_color: Vec<Option<crossterm::style::Color>> = vec![None; expanded_chars.len()];
    let mut visual_to_search_match: Vec<bool> = vec![false; expanded_chars.len()];
    
    // Apply syntax highlighting
    for (byte_start, byte_end, color) in highlights {
        // Convert byte positions to character positions in original line
        let char_start = original_line[..byte_start.min(original_line.len())].chars().count();
        let char_end = original_line[..byte_end.min(original_line.len())].chars().count();
        
        // Convert character positions to visual positions (accounting for tabs)
        let visual_start = crate::coordinates::visual_width_up_to(original_line, char_start, segment.tab_width);
        let visual_end = crate::coordinates::visual_width_up_to(original_line, char_end, segment.tab_width);
        
        // Mark visual positions with color
        for i in visual_start..visual_end.min(visual_to_color.len()) {
            visual_to_color[i] = Some(color);
        }
    }
    
    // Apply search match highlighting
    if let Some(ref pattern) = ctx.state.last_search_pattern {
        let matches = get_search_matches(original_line, pattern);
        let cursor_pos = ctx.state.current_position();
        let is_cursor_line = segment.line_index == cursor_pos.0;
        let cursor_col = if is_cursor_line { cursor_pos.1 } else { usize::MAX };
        
        for (char_start, char_end) in matches {
            let visual_start = crate::coordinates::visual_width_up_to(original_line, char_start, segment.tab_width);
            let visual_end = crate::coordinates::visual_width_up_to(original_line, char_end, segment.tab_width);
            
            // Check if cursor is within this match
            let is_current_match = is_cursor_line && cursor_col >= char_start && cursor_col < char_end;
            
            // Only mark as search match if NOT the current match
            if !is_current_match {
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
        let is_search_match = visual_to_search_match.get(visual_i).copied().unwrap_or(false);
        
        // Determine if this is the current match (cursor within match)
        // We need to check all matches again to determine this
        let cursor_pos = ctx.state.current_position();
        let is_cursor_line = segment.line_index == cursor_pos.0;
        let mut is_current_match = false;
        
        if is_cursor_line && let Some(ref pattern) = ctx.state.last_search_pattern {
            let matches = get_search_matches(original_line, pattern);
            let cursor_col = cursor_pos.1;
            
            for (char_start, char_end) in matches {
                if cursor_col >= char_start && cursor_col < char_end {
                    // This is the current match - check if visual_i is in it
                    let visual_start = crate::coordinates::visual_width_up_to(original_line, char_start, segment.tab_width);
                    let visual_end = crate::coordinates::visual_width_up_to(original_line, char_end, segment.tab_width);
                    if visual_i >= visual_start && visual_i < visual_end {
                        is_current_match = true;
                        break;
                    }
                }
            }
        }
        
        // Apply background color for search matches
        let new_bg_state = is_search_match || is_current_match;
        if new_bg_state != current_bg {
            if new_bg_state {
                if is_current_match {
                    // Darker blue background for current match
                    execute!(stdout, SetBackgroundColor(crossterm::style::Color::Rgb { r: 50, g: 100, b: 200 }))?;
                } else {
                    // Light blue background for other matches
                    execute!(stdout, SetBackgroundColor(crossterm::style::Color::Rgb { r: 100, g: 150, b: 200 }))?;
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
                execute!(stdout, SetBackgroundColor(crossterm::style::Color::Rgb { r: 50, g: 100, b: 200 }))?;
            } else if is_search_match {
                execute!(stdout, SetBackgroundColor(crossterm::style::Color::Rgb { r: 100, g: 150, b: 200 }))?;
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
    use crossterm::style::{SetForegroundColor, SetBackgroundColor, ResetColor};
    
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
    let mut visual_to_color: Vec<Option<crossterm::style::Color>> = vec![None; expanded_chars.len()];
    let mut visual_to_search_match: Vec<bool> = vec![false; expanded_chars.len()];
    
    // Apply syntax highlighting
    for (byte_start, byte_end, color) in highlights {
        // Convert byte positions to character positions in original line
        let char_start = original_line[..byte_start.min(original_line.len())].chars().count();
        let char_end = original_line[..byte_end.min(original_line.len())].chars().count();
        
        // Convert character positions to visual positions (accounting for tabs)
        let visual_start = crate::coordinates::visual_width_up_to(original_line, char_start, segment.tab_width);
        let visual_end = crate::coordinates::visual_width_up_to(original_line, char_end, segment.tab_width);
        
        // Mark visual positions with color
        for i in visual_start..visual_end.min(visual_to_color.len()) {
            visual_to_color[i] = Some(color);
        }
    }
    
    // Apply search match highlighting
    if let Some(ref pattern) = ctx.state.last_search_pattern {
        let matches = get_search_matches(original_line, pattern);
        let cursor_pos = ctx.state.current_position();
        let is_cursor_line = segment.line_index == cursor_pos.0;
        let cursor_col = if is_cursor_line { cursor_pos.1 } else { usize::MAX };
        
        for (char_start, char_end) in matches {
            let visual_start = crate::coordinates::visual_width_up_to(original_line, char_start, segment.tab_width);
            let visual_end = crate::coordinates::visual_width_up_to(original_line, char_end, segment.tab_width);
            
            // Check if cursor is within this match
            let is_current_match = is_cursor_line && cursor_col >= char_start && cursor_col < char_end;
            
            // Only mark as search match if NOT the current match
            if !is_current_match {
                for i in visual_start..visual_end.min(visual_to_search_match.len()) {
                    visual_to_search_match[i] = true;
                }
            }
        }
    }

    // Convert selection character indices to visual column range
    let start_visual_col = if segment.line_index == start_line { visual_width_up_to(original_line, start_col, segment.tab_width) } else { 0 };
    let end_visual_col = if segment.line_index == end_line { visual_width_up_to(original_line, end_col, segment.tab_width) } else { usize::MAX };

    let mut current_color: Option<crossterm::style::Color> = None;
    let mut current_bg: Option<&str> = None; // Track background: None, "search", "current", or "selection"

    for visual_i in segment.start_visual..segment.end_visual {
        if visual_i >= expanded_chars.len() { break; }
        let ch = expanded_chars[visual_i];
        let is_selected = visual_i >= start_visual_col && visual_i < end_visual_col;
        let is_search_match = visual_to_search_match.get(visual_i).copied().unwrap_or(false);
        
        // Check if this is the current match (cursor within match)
        let cursor_pos = ctx.state.current_position();
        let is_cursor_line = segment.line_index == cursor_pos.0;
        let mut is_current_match = false;
        
        if is_cursor_line && let Some(ref pattern) = ctx.state.last_search_pattern {
            let matches = get_search_matches(original_line, pattern);
            let cursor_col = cursor_pos.1;
            
            for (char_start, char_end) in matches {
                if cursor_col >= char_start && cursor_col < char_end {
                    let visual_start = crate::coordinates::visual_width_up_to(original_line, char_start, segment.tab_width);
                    let visual_end = crate::coordinates::visual_width_up_to(original_line, char_end, segment.tab_width);
                    if visual_i >= visual_start && visual_i < visual_end {
                        is_current_match = true;
                        break;
                    }
                }
            }
        }
        
        // Determine background (selection takes priority over search match)
        let desired_bg = if is_selected {
            Some("selection")
        } else if is_current_match {
            Some("current")
        } else if is_search_match {
            Some("search")
        } else {
            None
        };
        
        // Apply background if it changed
        if desired_bg != current_bg {
            match desired_bg {
                Some("selection") => {
                    // Reset color before applying reverse video
                    if current_color.is_some() || current_bg.is_some() {
                        execute!(stdout, ResetColor)?;
                        current_color = None;
                    }
                }
                Some("current") => {
                    // Darker blue background for current match
                    execute!(stdout, SetBackgroundColor(crossterm::style::Color::Rgb { r: 50, g: 100, b: 200 }))?;
                }
                Some("search") => {
                    // Light blue background for other matches
                    execute!(stdout, SetBackgroundColor(crossterm::style::Color::Rgb { r: 100, g: 150, b: 200 }))?;
                }
                _ => {
                    execute!(stdout, ResetColor)?;
                    current_color = None;
                }
            }
            current_bg = desired_bg;
        }

        if is_selected {
            write!(stdout, "{}", ch.to_string().reverse())?;
            execute!(stdout, ResetColor)?;
            current_color = None;
            current_bg = None;
        } else {
            let desired_color = visual_to_color.get(visual_i).copied().flatten();
            
            // Change foreground color if needed
            if desired_color != current_color {
                if let Some(color) = desired_color {
                    execute!(stdout, SetForegroundColor(color))?;
                } else if !is_search_match {
                    execute!(stdout, ResetColor)?;
                    if is_search_match {
                        execute!(stdout, SetBackgroundColor(crossterm::style::Color::Rgb { r: 100, g: 150, b: 200 }))?;
                    }
                }
                current_color = desired_color;
            }
            
            write!(stdout, "{}", ch)?;
        }
    }
    
    // Reset color at end
    if current_color.is_some() || current_bg.is_some() {
        execute!(stdout, ResetColor)?;
    }

    Ok(())
}

