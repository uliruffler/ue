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
    if let Some(color) = crate::settings::Settings::parse_color(&state.settings.appearance.footer_bg) {
        execute!(stdout, SetBackgroundColor(color))?;
    }
    write!(stdout, "\r{}", bottom_number_str)?;
    let left_len = bottom_number_str.len();
    let remaining = total_width.saturating_sub(left_len);
    if position_info.len() >= remaining {
        let truncated = &position_info[position_info.len() - remaining..];
        write!(stdout, "{}", truncated)?;
    } else {
        let pad = remaining - position_info.len();
        for _ in 0..pad { write!(stdout, " ")?; }
        write!(stdout, "{}", position_info)?;
    }
    execute!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
    execute!(stdout, ResetColor)?;
    Ok(())
}

fn render_visible_lines(
    stdout: &mut impl Write,
    file: &str,
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
    
    while visual_lines_rendered < visible_lines && logical_line_index < lines.len() {
        let lines_for_this_logical = render_line(
            stdout,
            file,
            lines,
            state,
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

fn render_line(
    stdout: &mut impl Write,
    file: &str,
    lines: &[String],
    state: &FileViewerState,
    logical_line_index: usize,
    _cursor_visual_line: usize,
    _current_visual_line: usize,
    remaining_visible_lines: usize,
) -> Result<usize, std::io::Error> {
    if logical_line_index >= lines.len() {
        return Ok(0);
    }
    
    let line = &lines[logical_line_index];
    let line_num_width = line_number_width(state.settings);
    let available_width = state.term_width.saturating_sub(line_num_width) as usize;
    let tab_width = state.settings.tab_width;
    
    // Expand tabs to spaces for display
    let expanded_line = expand_tabs(line, tab_width);
    let chars: Vec<char> = expanded_line.chars().collect();
    
    let num_wrapped_lines = calculate_wrapped_lines_for_line(lines, logical_line_index, state.term_width.saturating_sub(line_num_width), tab_width);
    
    let lines_to_render = (num_wrapped_lines as usize).min(remaining_visible_lines);
    
    for wrap_index in 0..lines_to_render {
        if wrap_index > 0 {
            write!(stdout, "\r\n")?;
        }
        
        // Show line number only if line_number_digits > 0
        if state.settings.appearance.line_number_digits > 0 {
            // Show line number only on first wrapped line, spaces on continuation lines
            if wrap_index == 0 {
                // Calculate line number to display (modulo based on digits)
                let modulus = 10usize.pow(state.settings.appearance.line_number_digits as u32);
                let line_num = (logical_line_index + 1) % modulus;
                if let Some(color) = crate::settings::Settings::parse_color(&state.settings.appearance.line_numbers_bg) { execute!(stdout, SetBackgroundColor(color))?; }
                write!(stdout, "{:width$} ", line_num, width = state.settings.appearance.line_number_digits as usize)?; execute!(stdout, ResetColor)?;
            } else {
                if let Some(color) = crate::settings::Settings::parse_color(&state.settings.appearance.line_numbers_bg) { execute!(stdout, SetBackgroundColor(color))?; }
                write!(stdout, "{:width$} ", "", width = state.settings.appearance.line_number_digits as usize)?; execute!(stdout, ResetColor)?;
            }
        }
        
        let start_visual = wrap_index * available_width;
        let end_visual = ((wrap_index + 1) * available_width).min(chars.len());
        
        if start_visual < chars.len() {
            if let (Some(sel_start), Some(sel_end)) = (state.selection_start, state.selection_end) {
                render_line_segment_with_selection_expanded(stdout, &chars, line, logical_line_index, sel_start, sel_end, start_visual, end_visual, file, state, tab_width)?;
            } else {
                render_line_segment_expanded(stdout, &chars, line, start_visual, end_visual, file, state, tab_width)?;
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
    let line_num_width = line_number_width(state.settings);
    let text_width = state.term_width.saturating_sub(line_num_width);
    if state.dragging_selection_active {
        if let Some((target_line, target_col)) = state.drag_target {
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
    start_visual: usize,
    end_visual: usize,
    file: &str,
    state: &FileViewerState,
    tab_width: usize,
) -> Result<(), std::io::Error> {
    let line_segment: String = expanded_chars[start_visual..end_visual].iter().collect();
    
    if state.settings.syntax.enable {
        let spans = state.highlighter.highlight_line(original_line, file, state.settings);
        if !spans.is_empty() {
            // Inline former render_with_highlighting_expanded
            let mut visual_to_char = Vec::new();
            let mut visual_pos = 0;
            for (char_idx, ch) in original_line.chars().enumerate() {
                if ch == '\t' {
                    let spaces = tab_width - (visual_pos % tab_width);
                    for _ in 0..spaces { visual_to_char.push(char_idx); visual_pos += 1; }
                } else { visual_to_char.push(char_idx); visual_pos += 1; }
            }
            for visual_i in start_visual..end_visual { if visual_i >= expanded_chars.len() { break; } let ch = expanded_chars[visual_i]; let orig_char_idx = if visual_i < visual_to_char.len() { visual_to_char[visual_i] } else { original_line.chars().count() }; if let Some(span) = spans.iter().find(|s| orig_char_idx >= s.start && orig_char_idx < s.end) { span.apply_to_stdout(stdout)?; write!(stdout, "{}", ch)?; execute!(stdout, ResetColor)?; } else { write!(stdout, "{}", ch)?; } } return Ok(());
        }
    }
    write!(stdout, "{}", line_segment)?;
    Ok(())
}

/// Render a line segment with expanded tabs and selection
fn render_line_segment_with_selection_expanded(
    stdout: &mut impl Write,
    expanded_chars: &[char],
    original_line: &str,
    line_index: usize,
    sel_start: Position,
    sel_end: Position,
    start_visual: usize,
    end_visual: usize,
    file: &str,
    state: &FileViewerState,
    tab_width: usize,
) -> Result<(), std::io::Error> {
    let (start, end) = normalize_selection(sel_start, sel_end);
    let (start_line, start_col) = start;
    let (end_line, end_col) = end;

    // Outside selection range -> normal rendering
    if line_index < start_line || line_index > end_line {
        return render_line_segment_expanded(stdout, expanded_chars, original_line, start_visual, end_visual, file, state, tab_width);
    }

    // Convert selection character indices to visual column range
    let start_visual_col = if line_index == start_line { visual_width_up_to(original_line, start_col, tab_width) } else { 0 };
    let end_visual_col = if line_index == end_line { visual_width_up_to(original_line, end_col, tab_width) } else { usize::MAX };

    // Optional syntax spans
    let spans_opt = if state.settings.syntax.enable {
        let spans = state.highlighter.highlight_line(original_line, file, state.settings);
        if spans.is_empty() { None } else { Some(spans) }
    } else { None };

    // Build mapping from visual position to original character index (tabs expand)
    let mut visual_to_char = Vec::new();
    let mut vis = 0;
    for (idx, ch) in original_line.chars().enumerate() {
        if ch == '\t' {
            let spaces = tab_width - (vis % tab_width);
            for _ in 0..spaces { visual_to_char.push(idx); vis += 1; }
        } else {
            visual_to_char.push(idx); vis += 1;
        }
    }

    for visual_i in start_visual..end_visual {
        if visual_i >= expanded_chars.len() { break; }
        let ch = expanded_chars[visual_i];
        let is_selected = visual_i >= start_visual_col && visual_i < end_visual_col;
        let orig_idx = if visual_i < visual_to_char.len() { visual_to_char[visual_i] } else { original_line.chars().count() };

        if is_selected {
            // Apply syntax color if available, then reverse
            if let Some(ref spans) = spans_opt {
                if let Some(span) = spans.iter().find(|s| orig_idx >= s.start && orig_idx < s.end) {
                    span.apply_to_stdout(stdout)?;
                }
            }
            write!(stdout, "{}", ch.to_string().reverse())?;
            execute!(stdout, ResetColor)?;
        } else if let Some(ref spans) = spans_opt {
            if let Some(span) = spans.iter().find(|s| orig_idx >= s.start && orig_idx < s.end) {
                span.apply_to_stdout(stdout)?;
                write!(stdout, "{}", ch)?;
                execute!(stdout, ResetColor)?;
            } else {
                write!(stdout, "{}", ch)?;
            }
        } else {
            write!(stdout, "{}", ch)?;
        }
    }

    Ok(())
}

