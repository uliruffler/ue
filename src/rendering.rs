use std::io::Write;
use crossterm::{
    cursor,
    execute,
    style::{Stylize, ResetColor, SetBackgroundColor},
    terminal::{self, ClearType},
};

use crate::coordinates::{calculate_cursor_visual_line, calculate_wrapped_lines_for_line, line_number_width, visual_width_up_to};
use crate::editor_state::{FileViewerState, Position};
use crate::syntax::{highlight_line, StyledSpan};

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
    if let Some(color) = crate::settings::Settings::parse_color(&state.settings.header_bg) {
        execute!(stdout, SetBackgroundColor(color))?;
    }
    if state.settings.line_number_digits > 0 {
        let modulus = 10usize.pow(state.settings.line_number_digits as u32);
        let top_number = (state.top_line / modulus) * modulus;
        // Add trailing space explicitly after the block number
        write!(stdout, "{:width$} ", top_number, width = state.settings.line_number_digits as usize)?;
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
    let digits = state.settings.line_number_digits as usize;
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
    if let Some(color) = crate::settings::Settings::parse_color(&state.settings.footer_bg) {
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
        if state.settings.line_number_digits > 0 {
            if let Some(color) = crate::settings::Settings::parse_color(&state.settings.line_numbers_bg) {
                execute!(stdout, SetBackgroundColor(color))?;
            }
            write!(stdout, "{:width$} ", "", width = state.settings.line_number_digits as usize)?;
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
        if state.settings.line_number_digits > 0 {
            // Show line number only on first wrapped line, spaces on continuation lines
            if wrap_index == 0 {
                // Calculate line number to display (modulo based on digits)
                let modulus = 10usize.pow(state.settings.line_number_digits as u32);
                let line_num = (logical_line_index + 1) % modulus;
                if let Some(color) = crate::settings::Settings::parse_color(&state.settings.line_numbers_bg) {
                    execute!(stdout, SetBackgroundColor(color))?;
                }
                write!(stdout, "{:width$} ", line_num, width = state.settings.line_number_digits as usize)?;
                execute!(stdout, ResetColor)?;
            } else {
                if let Some(color) = crate::settings::Settings::parse_color(&state.settings.line_numbers_bg) {
                    execute!(stdout, SetBackgroundColor(color))?;
                }
                write!(stdout, "{:width$} ", "", width = state.settings.line_number_digits as usize)?;
                execute!(stdout, ResetColor)?;
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



fn get_highlight_spans(line: &str, file: &str) -> Vec<StyledSpan> {
    highlight_line(line, file)
}


fn normalize_selection(sel_start: Position, sel_end: Position) -> (Position, Position) {
    if sel_start.0 < sel_end.0 || (sel_start.0 == sel_end.0 && sel_start.1 <= sel_end.1) {
        (sel_start, sel_end)
    } else {
        (sel_end, sel_start)
    }
}

fn position_cursor(
    stdout: &mut impl Write,
    lines: &[String],
    state: &FileViewerState,
    visible_lines: usize,
) -> Result<(), std::io::Error> {
    let line_num_width = line_number_width(state.settings);
    let text_width = state.term_width.saturating_sub(line_num_width);
    
    // Only show cursor if it's within the visible area
    if !state.is_cursor_visible(lines, visible_lines, text_width) {
        // Cursor is off-screen, keep it hidden
        return Ok(());
    }
    let tab_width = state.settings.tab_width;
    
    // Start with Y position after header (line 1)
    let mut cursor_y = 1u16;
    
    // Calculate how many visual lines are used by wrapped lines before the cursor line
    for i in 0..state.cursor_line {
        cursor_y += calculate_wrapped_lines_for_line(lines, state.top_line + i, text_width, tab_width);
    }
    
    // Calculate the visual column of the cursor (accounting for tabs)
    let cursor_line_idx = state.absolute_line();
    let visual_col = if cursor_line_idx < lines.len() {
        visual_width_up_to(&lines[cursor_line_idx], state.cursor_col, tab_width)
    } else {
        0
    };
    
    // Calculate which wrapped line the cursor is on
    let cursor_wrapped_line = visual_col / (text_width as usize);
    cursor_y += cursor_wrapped_line as u16;
    
    let cursor_x = (visual_col % (text_width as usize)) as u16 + line_num_width;
    
    execute!(stdout, cursor::MoveTo(cursor_x, cursor_y))?;
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
    
    // Apply syntax highlighting if enabled
    // Note: We need to map visual positions back to original positions for highlighting
    if state.settings.enable_syntax_highlighting {
        let spans = get_highlight_spans(original_line, file);
        if !spans.is_empty() {
            return render_with_highlighting_expanded(stdout, expanded_chars, original_line, start_visual, end_visual, &spans, tab_width);
        }
    }
    
    // Fallback: no highlighting
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
    
    // If this line is not in the selection range, just render normally
    if line_index < start_line || line_index > end_line {
        return render_line_segment_expanded(stdout, expanded_chars, original_line, start_visual, end_visual, file, state, tab_width);
    }
    
    // Convert character-based selection to visual positions
    let start_visual_col = if line_index == start_line {
        visual_width_up_to(original_line, start_col, tab_width)
    } else {
        0
    };
    
    let end_visual_col = if line_index == end_line {
        visual_width_up_to(original_line, end_col, tab_width)
    } else {
        usize::MAX
    };
    
    // Get syntax highlighting spans once for the entire line if enabled
    let spans = if state.settings.enable_syntax_highlighting {
        let s = get_highlight_spans(original_line, file);
        if s.is_empty() { None } else { Some(s) }
    } else {
        None
    };
    
    // Render each character in the segment, applying selection styling where appropriate
    for visual_i in start_visual..end_visual {
        if visual_i >= expanded_chars.len() {
            break;
        }
        
        let ch = expanded_chars[visual_i];
        let is_selected = visual_i >= start_visual_col && visual_i < end_visual_col;
        
        if is_selected {
            // Selection overrides syntax highlighting
            write!(stdout, "{}", ch.to_string().reverse())?;
        } else if let Some(ref _spans) = spans {
            // For now, just render without highlighting when we have tabs
            // Proper implementation would need to map visual positions back to original
            write!(stdout, "{}", ch)?;
        } else {
            write!(stdout, "{}", ch)?;
        }
    }
    
    Ok(())
}

/// Render with syntax highlighting on expanded text
fn render_with_highlighting_expanded(
    stdout: &mut impl Write,
    expanded_chars: &[char],
    original_line: &str,
    start_visual: usize,
    end_visual: usize,
    spans: &[StyledSpan],
    tab_width: usize,
) -> Result<(), std::io::Error> {
    // Build a mapping from visual position to original character index
    let mut visual_to_char = Vec::new();
    let mut visual_pos = 0;
    for (char_idx, ch) in original_line.chars().enumerate() {
        if ch == '\t' {
            let spaces = tab_width - (visual_pos % tab_width);
            for _ in 0..spaces {
                visual_to_char.push(char_idx);
                visual_pos += 1;
            }
        } else {
            visual_to_char.push(char_idx);
            visual_pos += 1;
        }
    }
    
    for visual_i in start_visual..end_visual {
        if visual_i >= expanded_chars.len() {
            break;
        }
        
        let ch = expanded_chars[visual_i];
        
        // Find the original character index for this visual position
        let orig_char_idx = if visual_i < visual_to_char.len() {
            visual_to_char[visual_i]
        } else {
            original_line.chars().count()
        };
        
        // Find if this character is in a highlighted span
        if let Some(span) = spans.iter().find(|s| orig_char_idx >= s.start && orig_char_idx < s.end) {
            span.apply_to_stdout(stdout)?;
            write!(stdout, "{}", ch)?;
            execute!(stdout, ResetColor)?;
        } else {
            write!(stdout, "{}", ch)?;
        }
    }
    
    Ok(())
}
