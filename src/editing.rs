use std::{fs};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use crate::editor_state::{FileViewerState, Position};
use crate::undo::Edit;


static GLOBAL_CLIPBOARD: OnceLock<Mutex<Option<arboard::Clipboard>>> = OnceLock::new();
fn get_clipboard() -> &'static Mutex<Option<arboard::Clipboard>> { GLOBAL_CLIPBOARD.get_or_init(|| Mutex::new(arboard::Clipboard::new().ok())) }

/// Save undo history and record the save timestamp to prevent reload loops
fn save_undo_with_timestamp(state: &mut FileViewerState, filename: &str) {
    let _ = state.undo_history.save(filename);
    state.last_save_time = Some(Instant::now());
}


pub(crate) fn handle_copy(state: &FileViewerState, lines: &[String]) -> Result<(), std::io::Error> {
    if let (Some(sel_start), Some(sel_end)) = (state.selection_start, state.selection_end) {
        let lines_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        let selected_text = extract_selection(&lines_refs, sel_start, sel_end);
        let mut clipboard_guard = get_clipboard().lock().unwrap();
        if let Some(ref mut cb) = *clipboard_guard { if let Err(e) = cb.set_text(selected_text) { eprintln!("Failed to copy to clipboard: {}", e); } }
        let _ = copy_to_clipboard("");
    }
    Ok(())
}

pub(crate) fn handle_paste(state: &mut FileViewerState, lines: &mut Vec<String>, filename: &str) -> bool {
    let text = {
        let mut lock = get_clipboard().lock().unwrap();
        if let Some(cb) = lock.as_mut() { cb.get_text().unwrap_or_default() } else { String::new() }
    };
    let _ = paste_from_clipboard();
    if text.is_empty() { return false; }
    state.clear_selection();
    let idx = state.absolute_line(); if idx >= lines.len() { return false; }
    let paste_lines: Vec<&str> = text.lines().collect(); if paste_lines.is_empty() { return false; }
    if paste_lines.len() == 1 {
        let paste_text = paste_lines[0];
        for (i, ch) in paste_text.chars().enumerate() { state.undo_history.push(Edit::InsertChar { line: idx, col: state.cursor_col + i, ch }); }
        lines[idx].insert_str(state.cursor_col, paste_text);
        state.cursor_col += paste_text.len(); state.modified = true;
    } else {
        let current_line = &lines[idx];
        let before = current_line[..state.cursor_col].to_string();
        let after = current_line[state.cursor_col..].to_string();
        let first_paste_line = paste_lines[0].to_string();
        state.undo_history.push(Edit::SplitLine { line: idx, col: state.cursor_col, before: before.clone(), after: after.clone() });
        lines[idx] = before.clone() + &first_paste_line;
        for (i, paste_line) in paste_lines[1..paste_lines.len()-1].iter().enumerate() {
            state.undo_history.push(Edit::InsertLine { line: idx + 1 + i, content: paste_line.to_string() });
            lines.insert(idx + 1 + i, paste_line.to_string());
        }
        let last_paste_line = paste_lines.last().unwrap().to_string();
        let final_line = last_paste_line.clone() + &after;
        state.undo_history.push(Edit::InsertLine { line: idx + paste_lines.len() - 1, content: final_line.clone() });
        lines.insert(idx + paste_lines.len() - 1, final_line);
        state.cursor_line = (idx + paste_lines.len() - 1).saturating_sub(state.top_line);
        state.cursor_col = last_paste_line.len(); state.modified = true;
    }
    let absolute_line = state.absolute_line();
    state.undo_history.update_state(state.top_line, absolute_line, state.cursor_col, lines.clone());
    save_undo_with_timestamp(state, filename);
    true
}

pub(crate) fn handle_cut(state: &mut FileViewerState, lines: &mut Vec<String>, filename: &str) -> bool {
    if state.has_selection() {
        let (sel_start, sel_end) = (state.selection_start.unwrap(), state.selection_end.unwrap());
        let lines_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        let selected_text = extract_selection(&lines_refs, sel_start, sel_end);
        let mut clipboard_guard = get_clipboard().lock().unwrap();
        if let Some(ref mut cb) = *clipboard_guard { let _ = cb.set_text(selected_text); }
        let removed = remove_selection(state, lines, filename);
        return removed;
    }
    let abs = state.absolute_line(); if abs >= lines.len() { return false; }
    let line_content = lines[abs].clone();
    let mut to_clip = line_content.clone(); to_clip.push('\n');
    let mut clipboard_guard = get_clipboard().lock().unwrap();
    if let Some(ref mut cb) = *clipboard_guard { let _ = cb.set_text(to_clip); }
    state.undo_history.push(Edit::DeleteLine { line: abs, content: line_content.clone() });
    lines.remove(abs);
    if abs >= lines.len() && abs > 0 { state.cursor_line = (abs - 1).saturating_sub(state.top_line); state.cursor_col = lines.get(abs - 1).map(|l| l.len().min(state.cursor_col)).unwrap_or(0); }
    else { state.cursor_line = abs.saturating_sub(state.top_line); state.cursor_col = 0; }
    state.modified = true;
    let absolute_line = state.absolute_line();
    state.undo_history.update_state(state.top_line, absolute_line, state.cursor_col, lines.clone());
    save_undo_with_timestamp(state, filename);
    state.needs_redraw = true;
    true
}

pub(crate) fn remove_selection(state: &mut FileViewerState, lines: &mut Vec<String>, filename: &str) -> bool {
    if !state.has_selection() { return false; }
    let (sel_start, sel_end) = {
        let s = state.selection_start.unwrap();
        let e = state.selection_end.unwrap();
        if s.0 < e.0 || (s.0 == e.0 && s.1 <= e.1) { (s, e) } else { (e, s) }
    };
    let (s_line, s_col) = sel_start;
    let (e_line, e_col) = sel_end;
    if s_line >= lines.len() || e_line >= lines.len() { state.clear_selection(); return false; }
    if s_line == e_line && s_col == e_col { state.clear_selection(); return false; }

    if s_line == e_line {
        // Single-line removal
        let line = &mut lines[s_line];
        let end_col = e_col.min(line.len());
        if s_col >= end_col { state.clear_selection(); return false; }
        let removed: Vec<char> = line[s_col..end_col].chars().collect();
        // Record deletes in reverse order
        for (i, ch) in removed.into_iter().enumerate().rev() {
            state.undo_history.push(Edit::DeleteChar { line: s_line, col: s_col + i, ch });
        }
        line.replace_range(s_col..end_col, "");
    } else {
        // Multi-line removal
        // Tail of start line
        if s_col < lines[s_line].len() {
            let tail: Vec<char> = lines[s_line][s_col..].chars().collect();
            for (i, ch) in tail.into_iter().enumerate().rev() {
                state.undo_history.push(Edit::DeleteChar { line: s_line, col: s_col + i, ch });
            }
            lines[s_line].truncate(s_col);
        }
        // Middle full lines
        for line_idx in (s_line + 1)..e_line {
            let content = lines[line_idx].clone();
            state.undo_history.push(Edit::DeleteLine { line: line_idx, content: content.clone() });
        }
        // Head of end line
        let end_line_len = lines[e_line].len();
        let head_limit = e_col.min(end_line_len);
        let head: Vec<char> = lines[e_line][..head_limit].chars().collect();
        for (i, ch) in head.into_iter().enumerate().rev() {
            state.undo_history.push(Edit::DeleteChar { line: e_line, col: i, ch });
        }
        // Remove head portion
        if head_limit <= end_line_len { lines[e_line].replace_range(..head_limit, ""); }
        // Merge start and remaining end line
        let first_snapshot = lines[s_line].clone();
        let second_snapshot = lines[e_line].clone();
        state.undo_history.push(Edit::MergeLine { line: s_line, first: first_snapshot, second: second_snapshot.clone() });
        lines[s_line].push_str(&second_snapshot);
        // Remove intervening + original end line
        for _ in s_line + 1..=e_line { lines.remove(s_line + 1); }
    }

    state.cursor_line = s_line.saturating_sub(state.top_line);
    state.cursor_col = s_col;
    state.clear_selection();
    state.modified = true;
    let absolute_line = state.top_line + state.cursor_line;
    state.undo_history.update_state(state.top_line, absolute_line, state.cursor_col, lines.clone());
    save_undo_with_timestamp(state, filename);
    state.needs_redraw = true;
    true
}

pub(crate) fn insert_char(state: &mut FileViewerState, lines: &mut [String], c: char, filename: &str) -> bool {
    let idx = state.absolute_line();
    if idx < lines.len() && state.cursor_col <= lines[idx].len() {
        lines[idx].insert(state.cursor_col, c);
        state.undo_history.push(Edit::InsertChar { line: idx, col: state.cursor_col, ch: c });
        state.cursor_col += 1;
        state.undo_history.update_state(state.top_line, idx, state.cursor_col, lines.to_vec());
        save_undo_with_timestamp(state, filename);
        true
    } else { false }
}

pub(crate) fn split_line(state: &mut FileViewerState, lines: &mut Vec<String>, visible_lines: usize, filename: &str) -> bool {
    let idx = state.absolute_line();
    if idx >= lines.len() { return false; }
    let split_at = state.cursor_col.min(lines[idx].len());
    let line_clone = lines[idx].clone();
    let (before, after) = line_clone.split_at(split_at);
    state.undo_history.push(Edit::SplitLine { line: idx, col: split_at, before: before.to_string(), after: after.to_string() });
    lines[idx] = before.to_string();
    lines.insert(idx + 1, after.to_string());
    if state.cursor_line + 1 < visible_lines { state.cursor_line += 1; } else { state.top_line += 1; }
    state.cursor_col = 0;
    let absolute_line = state.absolute_line();
    state.undo_history.update_state(state.top_line, absolute_line, state.cursor_col, lines.clone());
    save_undo_with_timestamp(state, filename);
    true
}

pub(crate) fn delete_backward(state: &mut FileViewerState, lines: &mut Vec<String>, filename: &str) -> bool {
    let idx = state.absolute_line();
    if idx >= lines.len() { return false; }
    if state.cursor_col > 0 && state.cursor_col <= lines[idx].len() {
        let ch = lines[idx].chars().nth(state.cursor_col - 1).unwrap();
        lines[idx].remove(state.cursor_col - 1);
        state.undo_history.push(Edit::DeleteChar { line: idx, col: state.cursor_col - 1, ch });
        state.cursor_col -= 1;
        state.undo_history.update_state(state.top_line, idx, state.cursor_col, lines.to_vec());
        save_undo_with_timestamp(state, filename);
        true
    } else if idx > 0 {
        let current = lines.remove(idx);
        let prev_len = lines[idx - 1].len();
        let first_snapshot = lines[idx - 1].clone();
        lines[idx - 1].push_str(&current);
        state.undo_history.push(Edit::MergeLine { line: idx - 1, first: first_snapshot, second: current });
        if state.cursor_line > 0 { state.cursor_line -= 1; } else { state.top_line = state.top_line.saturating_sub(1); }
        state.cursor_col = prev_len;
        let absolute_line = state.absolute_line();
        state.undo_history.update_state(state.top_line, absolute_line, state.cursor_col, lines.clone());
        save_undo_with_timestamp(state, filename);
        true
    } else { false }
}

pub(crate) fn delete_forward(state: &mut FileViewerState, lines: &mut Vec<String>, filename: &str) -> bool {
    let idx = state.absolute_line();
    if idx >= lines.len() { return false; }
    if state.cursor_col < lines[idx].len() {
        let ch = lines[idx].chars().nth(state.cursor_col).unwrap();
        lines[idx].remove(state.cursor_col);
        state.undo_history.push(Edit::DeleteChar { line: idx, col: state.cursor_col, ch });
        state.undo_history.update_state(state.top_line, idx, state.cursor_col, lines.to_vec());
        save_undo_with_timestamp(state, filename);
        true
    } else if idx + 1 < lines.len() {
        let next_line = lines.remove(idx + 1);
        let first_snapshot = lines[idx].clone();
        lines[idx].push_str(&next_line);
        state.undo_history.push(Edit::MergeLine { line: idx, first: first_snapshot, second: next_line });
        state.undo_history.update_state(state.top_line, idx, state.cursor_col, lines.clone());
        save_undo_with_timestamp(state, filename);
        true
    } else { false }
}

pub(crate) fn insert_tab(state: &mut FileViewerState, lines: &mut [String], filename: &str) -> bool {
    let idx = state.absolute_line();
    let tab_width = state.settings.tab_width;
    if idx < lines.len() && state.cursor_col <= lines[idx].len() {
        let spaces = " ".repeat(tab_width);
        lines[idx].insert_str(state.cursor_col, &spaces);
        for (i, _) in spaces.chars().enumerate() {
            state.undo_history.push(Edit::InsertChar { line: idx, col: state.cursor_col + i, ch: ' ' });
        }
        state.cursor_col += tab_width;
        state.undo_history.update_state(state.top_line, idx, state.cursor_col, lines.to_vec());
        save_undo_with_timestamp(state, filename);
        true
    } else { false }
}

/// Delete the undo history file for the given file path and remove empty parent directories
pub(crate) fn delete_file_history(file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let history_path = crate::undo::UndoHistory::history_path_for(file_path)?;
    if history_path.exists() { fs::remove_file(&history_path)?; }
    Ok(())
}

/// Save file content to disk
pub(crate) fn save_file(path: &str, lines: &[String]) -> Result<(), std::io::Error> {
    // Construct content with newlines preserved; assume lines vector does not include trailing newline for last line
    let mut content = String::new();
    for (i, line) in lines.iter().enumerate() {
        content.push_str(line);
        if i + 1 < lines.len() { content.push('\n'); }
    }
    fs::write(path, content)?;
    Ok(())
}

pub(crate) fn apply_undo(state: &mut FileViewerState, lines: &mut Vec<String>, filename: &str, visible_lines: usize) -> bool {
    if let Some(edit) = state.undo_history.undo() {
        let result = match edit {
            Edit::InsertChar { line, col, .. } => {
                // Undo insert: delete the character
                if line < lines.len() && col < lines[line].len() {
                    lines[line].remove(col);
                    state.cursor_col = col;
                    state.cursor_line = line.saturating_sub(state.top_line);
                    true
                } else {
                    false
                }
            }
            Edit::DeleteChar { line, col, ch } => {
                // Undo delete: insert the character back
                if line < lines.len() && col <= lines[line].len() {
                    lines[line].insert(col, ch);
                    state.cursor_col = col + 1;
                    state.cursor_line = line.saturating_sub(state.top_line);
                    true
                } else {
                    false
                }
            }
            Edit::SplitLine { line, col, before, .. } => {
                // Undo split: merge the lines back
                if line < lines.len() && line + 1 < lines.len() {
                    let after = lines.remove(line + 1);
                    lines[line] = format!("{}{}", before, after);
                    state.cursor_col = col;
                    state.cursor_line = line.saturating_sub(state.top_line);
                    true
                } else {
                    false
                }
            }
            Edit::MergeLine { line, first, second } => {
                // Undo merge: split the lines back
                if line < lines.len() {
                    lines[line] = first;
                    lines.insert(line + 1, second);
                    state.cursor_col = 0;
                    state.cursor_line = (line + 1).saturating_sub(state.top_line);
                    true
                } else {
                    false
                }
            }
            Edit::InsertLine { line, .. } => {
                // Undo insert line: delete the line
                if line < lines.len() {
                    lines.remove(line);
                    state.cursor_line = line.saturating_sub(state.top_line);
                    state.cursor_col = 0;
                    true
                } else {
                    false
                }
            }
            Edit::DeleteLine { line, content } => {
                // Undo delete line: insert the line back
                if line <= lines.len() {
                    lines.insert(line, content);
                    state.cursor_line = line.saturating_sub(state.top_line);
                    state.cursor_col = 0;
                    true
                } else {
                    false
                }
            }
            Edit::DragBlock { before, .. } => {
                *lines = before.clone();
                // Cursor remains; ensure visibility
                true
            }
        };
        
        if result {
            // Ensure cursor is visible after undo operation
            state.ensure_cursor_visible(visible_lines);
            
            // Persist content changes (but not scroll/cursor separately) using update_state
            let absolute_line = state.absolute_line();
            state.undo_history.update_state(state.top_line, absolute_line, state.cursor_col, lines.clone());
            // Sync modified flag from undo history
            state.modified = state.undo_history.modified;
            save_undo_with_timestamp(state, filename);
        }
        result
    } else {
        false
    }
}

pub(crate) fn apply_redo(state: &mut FileViewerState, lines: &mut Vec<String>, filename: &str, visible_lines: usize) -> bool {
    if let Some(edit) = state.undo_history.redo() {
        let result = match edit {
            Edit::InsertChar { line, col, ch } => {
                // Redo insert: insert the character
                if line < lines.len() && col <= lines[line].len() {
                    lines[line].insert(col, ch);
                    state.cursor_col = col + 1;
                    state.cursor_line = line.saturating_sub(state.top_line);
                    true
                } else {
                    false
                }
            }
            Edit::DeleteChar { line, col, .. } => {
                // Redo delete: delete the character
                if line < lines.len() && col < lines[line].len() {
                    lines[line].remove(col);
                    state.cursor_col = col;
                    state.cursor_line = line.saturating_sub(state.top_line);
                    true
                } else {
                    false
                }
            }
            Edit::SplitLine { line, col: _, before, after } => {
                // Redo split: split the line
                if line < lines.len() {
                    lines[line] = before;
                    lines.insert(line + 1, after);
                    state.cursor_col = 0;
                    state.cursor_line = (line + 1).saturating_sub(state.top_line);
                    true
                } else {
                    false
                }
            }
            Edit::MergeLine { line, .. } => {
                // Redo merge: merge the lines
                if line < lines.len() && line + 1 < lines.len() {
                    let next = lines.remove(line + 1);
                    let prev_len = lines[line].len();
                    lines[line].push_str(&next);
                    state.cursor_col = prev_len;
                    state.cursor_line = line.saturating_sub(state.top_line);
                    true
                } else {
                    false
                }
            }
            Edit::InsertLine { line, content } => {
                // Redo insert line: insert the line
                if line <= lines.len() {
                    lines.insert(line, content);
                    state.cursor_line = line.saturating_sub(state.top_line);
                    state.cursor_col = 0;
                    true
                } else {
                    false
                }
            }
            Edit::DeleteLine { line, .. } => {
                // Redo delete line: delete the line
                if line < lines.len() {
                    lines.remove(line);
                    state.cursor_line = line.saturating_sub(state.top_line);
                    state.cursor_col = 0;
                    true
                } else {
                    false
                }
            }
            Edit::DragBlock { after, .. } => {
                *lines = after.clone();
                true
            }
        };
        
        if result {
            // Ensure cursor is visible after redo operation
            state.ensure_cursor_visible(visible_lines);
            
            let absolute_line = state.absolute_line();
            state.undo_history.update_state(state.top_line, absolute_line, state.cursor_col, lines.clone());
            // Sync modified flag from undo history
            state.modified = state.undo_history.modified;
            save_undo_with_timestamp(state, filename);
        }
        result
    } else {
        false
    }
}

pub(crate) fn handle_editing_keys(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    code: &crossterm::event::KeyCode,
    modifiers: &crossterm::event::KeyModifiers,
    visible_lines: usize,
    filename: &str,
) -> bool {
    use crossterm::event::{KeyCode, KeyModifiers};
    
    let selection_active = state.has_selection();
    match code {
        KeyCode::Backspace | KeyCode::Delete if selection_active => remove_selection(state, lines, filename),
        KeyCode::Char(c) if !modifiers.contains(KeyModifiers::CONTROL) && !modifiers.contains(KeyModifiers::ALT) => { if selection_active { remove_selection(state, lines, filename); } insert_char(state, lines, *c, filename) }
        KeyCode::Enter => { if selection_active { remove_selection(state, lines, filename); } split_line(state, lines, visible_lines, filename) }
        KeyCode::Tab => { if selection_active { remove_selection(state, lines, filename); } insert_tab(state, lines, filename) }
        KeyCode::Backspace => delete_backward(state, lines, filename),
        KeyCode::Delete => delete_forward(state, lines, filename),
        _ => false,
    }
}

fn extract_selection(lines: &[&str], sel_start: Position, sel_end: Position) -> String {
    let (start, end) = normalize_selection(sel_start, sel_end);
    let (start_line, start_col) = start;
    let (end_line, end_col) = end;
    
    if start_line == end_line {
        return extract_single_line_selection(lines, start_line, start_col, end_col);
    }
    
    extract_multi_line_selection(lines, start_line, start_col, end_line, end_col)
}

fn extract_single_line_selection(
    lines: &[&str],
    line_idx: usize,
    start_col: usize,
    end_col: usize,
) -> String {
    lines.get(line_idx)
        .map(|line| {
            let chars: Vec<char> = line.chars().collect();
            chars[start_col..end_col.min(chars.len())].iter().collect()
        })
        .unwrap_or_default()
}

fn extract_multi_line_selection(
    lines: &[&str],
    start_line: usize,
    start_col: usize,
    end_line: usize,
    end_col: usize,
) -> String {
    let mut result = String::new();
    
    for line_idx in start_line..=end_line {
        if let Some(line) = lines.get(line_idx) {
            if line_idx == start_line {
                let chars: Vec<char> = line.chars().collect();
                result.extend(&chars[start_col..]);
                result.push('\n');
            } else if line_idx == end_line {
                let chars: Vec<char> = line.chars().collect();
                result.extend(&chars[..end_col.min(chars.len())]);
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }
    }
    
    result
}

fn normalize_selection(a: Position, b: Position) -> (Position, Position) {
    if a.0 < b.0 || (a.0 == b.0 && a.1 <= b.1) { (a,b) } else { (b,a) }
}

pub(crate) fn apply_drag(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    sel_start: Position,
    sel_end: Position,
    dest: Position,
    copy: bool,
) {
    if state.is_point_in_selection(dest) { return; }
    let before_snapshot = lines.clone();
    let (start, end) = normalize_selection(sel_start, sel_end);
    let lines_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    let dragged_text = extract_selection(&lines_refs, start, end);
    if dragged_text.is_empty() { return; }
    let removed_lines = end.0 - start.0;
    // Remove original if move
    if !copy {
        let hl = Box::leak(Box::new(crate::syntax::SyntectHighlighter::new()));
        let mut tmp_state = FileViewerState::new(state.term_width, state.undo_history.clone(), state.settings, hl);
        tmp_state.selection_start = Some(start); tmp_state.selection_end = Some(end);
        remove_selection(&mut tmp_state, lines, "__drag__");
        // Adjust destination line if original block removed above
        if dest.0 > start.0 { state.cursor_line = (dest.0 - removed_lines).saturating_sub(state.top_line); }
    }
    // Compute insertion location after potential removal adjustment
    let insert_line = if dest.0 > lines.len() { lines.len().saturating_sub(1) } else { dest.0 };
    if insert_line >= lines.len() { lines.push(String::new()); }
    let insert_col = dest.1.min(lines[insert_line].len());
    let current_line = lines[insert_line].clone();
    let before = current_line[..insert_col].to_string();
    let after = current_line[insert_col..].to_string();
    let drag_lines: Vec<&str> = dragged_text.lines().collect();
    if drag_lines.len() == 1 {
        lines[insert_line] = format!("{}{}{}", before, drag_lines[0], after);
        state.cursor_line = insert_line.saturating_sub(state.top_line);
        state.cursor_col = before.len() + drag_lines[0].len();
    } else {
        lines[insert_line] = format!("{}{}", before, drag_lines[0]);
        let mut idx = insert_line + 1;
        for mid in drag_lines.iter().skip(1).take(drag_lines.len()-2) { lines.insert(idx, mid.to_string()); idx += 1; }
        lines.insert(idx, format!("{}{}", drag_lines.last().unwrap(), after));
        state.cursor_line = idx.saturating_sub(state.top_line);
        state.cursor_col = drag_lines.last().unwrap().len();
    }
    state.selection_start = None; state.selection_end = None; state.modified = true; state.needs_redraw = true;
    let abs = state.absolute_line();
    state.undo_history.update_state(state.top_line, abs, state.cursor_col, lines.clone());
    state.undo_history.push(Edit::DragBlock { before: before_snapshot, after: lines.clone(), source_start: sel_start, source_end: sel_end, dest, copy });
    save_undo_with_timestamp(state, "__drag__");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::set_temp_home;
    use crate::settings::Settings;
    use crate::undo::UndoHistory;
    use crate::editor_state::FileViewerState;

    fn create_test_state() -> FileViewerState<'static> {
        let settings = Box::leak(Box::new(Settings::load().expect("Failed to load test settings")));
        let undo_history = UndoHistory::new();
        let hl = Box::leak(Box::new(crate::syntax::SyntectHighlighter::new()));
        FileViewerState::new(80, undo_history, settings, hl)
    }

    #[test]
    fn insert_char_basic() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello".to_string()];
        state.cursor_col = 5;
        
        assert!(insert_char(&mut state, &mut lines, '!', "test.txt"));
        assert_eq!(lines[0], "hello!");
        assert_eq!(state.cursor_col, 6);
        assert_eq!(state.undo_history.edits.len(), 1);
    }

    #[test]
    fn insert_char_middle() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello".to_string()];
        state.cursor_col = 2;
        
        assert!(insert_char(&mut state, &mut lines, 'X', "test.txt"));
        assert_eq!(lines[0], "heXllo");
        assert_eq!(state.cursor_col, 3);
    }

    #[test]
    fn delete_backward_char() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello".to_string()];
        state.cursor_col = 5;
        
        assert!(delete_backward(&mut state, &mut lines, "test.txt"));
        assert_eq!(lines[0], "hell");
        assert_eq!(state.cursor_col, 4);
    }

    #[test]
    fn delete_backward_merge_lines() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello".to_string(), "world".to_string()];
        state.cursor_line = 1;
        state.cursor_col = 0;
        
        assert!(delete_backward(&mut state, &mut lines, "test.txt"));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "helloworld");
        assert_eq!(state.cursor_col, 5);
    }

    #[test]
    fn delete_forward_char() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello".to_string()];
        state.cursor_col = 0;
        
        assert!(delete_forward(&mut state, &mut lines, "test.txt"));
        assert_eq!(lines[0], "ello");
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn delete_forward_merge_lines() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello".to_string(), "world".to_string()];
        state.cursor_col = 5;
        
        assert!(delete_forward(&mut state, &mut lines, "test.txt"));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "helloworld");
        assert_eq!(state.cursor_col, 5);
    }

    #[test]
    fn split_line_basic() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["helloworld".to_string()];
        state.cursor_col = 5;
        
        assert!(split_line(&mut state, &mut lines, 10, "test.txt"));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "hello");
        assert_eq!(lines[1], "world");
        assert_eq!(state.cursor_line, 1);
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn insert_tab_adds_spaces() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello".to_string()];
        state.cursor_col = 0;
        
        assert!(insert_tab(&mut state, &mut lines, "test.txt"));
        assert_eq!(lines[0], "    hello");
        assert_eq!(state.cursor_col, 4);
    }

    #[test]
    fn undo_insert_char() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello".to_string()];
        state.cursor_col = 5;
        
        insert_char(&mut state, &mut lines, '!', "test.txt");
        assert_eq!(lines[0], "hello!");
        
        assert!(apply_undo(&mut state, &mut lines, "test.txt", 10));
        assert_eq!(lines[0], "hello");
        assert_eq!(state.cursor_col, 5);
    }

    #[test]
    fn undo_delete_backward() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello".to_string()];
        state.cursor_col = 5;
        
        delete_backward(&mut state, &mut lines, "test.txt");
        assert_eq!(lines[0], "hell");
        
        assert!(apply_undo(&mut state, &mut lines, "test.txt", 10));
        assert_eq!(lines[0], "hello");
        assert_eq!(state.cursor_col, 5);
    }

    #[test]
    fn undo_split_line() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["helloworld".to_string()];
        state.cursor_col = 5;
        
        split_line(&mut state, &mut lines, 10, "test.txt");
        assert_eq!(lines.len(), 2);
        
        assert!(apply_undo(&mut state, &mut lines, "test.txt", 10));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "helloworld");
        assert_eq!(state.cursor_col, 5);
    }

    #[test]
    fn redo_insert_char() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello".to_string()];
        state.cursor_col = 5;
        
        insert_char(&mut state, &mut lines, '!', "test.txt");
        apply_undo(&mut state, &mut lines, "test.txt", 10);
        
        assert!(apply_redo(&mut state, &mut lines, "test.txt", 10));
        assert_eq!(lines[0], "hello!");
        assert_eq!(state.cursor_col, 6);
    }

    #[test]
    fn remove_selection_single_line() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello world".to_string()];
        state.selection_start = Some((0, 0));
        state.selection_end = Some((0, 5));
        
        assert!(remove_selection(&mut state, &mut lines, "test.txt"));
        assert_eq!(lines[0], " world");
        assert_eq!(state.cursor_col, 0);
        assert!(!state.has_selection());
    }

    #[test]
    fn remove_selection_multi_line() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello".to_string(), "beautiful".to_string(), "world".to_string()];
        state.selection_start = Some((0, 2));
        state.selection_end = Some((2, 3));
        
        assert!(remove_selection(&mut state, &mut lines, "test.txt"));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "held");
        assert_eq!(state.cursor_col, 2);
    }

    #[test]
    fn extract_selection_single_line() {
        let lines = vec!["hello world"];
        let result = extract_selection(&lines, (0, 0), (0, 5));
        assert_eq!(result, "hello");
    }

    #[test]
    fn extract_selection_multi_line() {
        let lines = vec!["hello", "beautiful", "world"];
        let result = extract_selection(&lines, (0, 2), (2, 3));
        assert_eq!(result, "llo\nbeautiful\nwor");
    }

    #[test]
    fn normalize_selection_already_normalized() {
        let start = (0, 5);
        let end = (1, 3);
        let (s, e) = normalize_selection(start, end);
        assert_eq!(s, (0, 5));
        assert_eq!(e, (1, 3));
    }

    #[test]
    fn normalize_selection_reversed() {
        let start = (2, 8);
        let end = (1, 3);
        let (s, e) = normalize_selection(start, end);
        assert_eq!(s, (1, 3));
        assert_eq!(e, (2, 8));
    }
    #[test]
    fn delete_file_history_removes_file() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        
        // Create a test file and save some content to create the .ue file
        let test_file = "/tmp/test_delete.txt";
        let mut lines = vec!["test content".to_string()];
        insert_char(&mut state, &mut lines, 'x', test_file);
        
        // Verify .ue file exists
        let history_result = crate::undo::UndoHistory::load(test_file);
        assert!(history_result.is_ok());
        
        // Delete the file history
        let result = delete_file_history(test_file);
        assert!(result.is_ok());
        
        // Verify it was deleted (load should return new/empty history)
        let history_after = crate::undo::UndoHistory::load(test_file).unwrap();
        assert_eq!(history_after.edits.len(), 0);
    }
    
    #[test]
    fn delete_file_history_handles_nonexistent_file() {
        let (_tmp, _guard) = set_temp_home();
        
        // Deleting non-existent file should not error
        let result = delete_file_history("/tmp/nonexistent_file_12345.txt");
        assert!(result.is_ok());
    }

    #[test]
    fn clipboard_helpers_used() {
        let _ = copy_to_clipboard("test");
        let _ = paste_from_clipboard();
    }
}

#[allow(dead_code)] fn copy_to_clipboard(_text: &str) -> Result<(), Box<dyn std::error::Error>> { Ok(()) }
#[allow(dead_code)] fn paste_from_clipboard() -> Result<String, Box<dyn std::error::Error>> { Ok(String::new()) }
