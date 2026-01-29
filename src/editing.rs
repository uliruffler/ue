use crate::editor_state::{FileViewerState, Position};
use crate::undo::Edit;
use std::fs;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static GLOBAL_CLIPBOARD: OnceLock<Mutex<Option<arboard::Clipboard>>> = OnceLock::new();
fn get_clipboard() -> &'static Mutex<Option<arboard::Clipboard>> {
    GLOBAL_CLIPBOARD.get_or_init(|| Mutex::new(arboard::Clipboard::new().ok()))
}

/// Get the character length of a string (not byte length)
/// This is important for Unicode support
#[inline]
fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// Convert character index to byte index in a string
/// Returns the byte index that corresponds to the character at char_idx
/// If char_idx is beyond the string, returns the byte length of the string
fn char_index_to_byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(byte_idx, _)| byte_idx)
        .unwrap_or(s.len())
}

/// Save undo history and record the save timestamp to prevent reload loops
fn save_undo_with_timestamp(state: &mut FileViewerState, filename: &str) {
    // Update undo history with current find history before saving
    state.undo_history.find_history = state.find_history.clone();
    let _ = state.undo_history.save(filename);
    state.last_save_time = Some(Instant::now());
}

pub(crate) fn handle_copy(state: &FileViewerState, lines: &[String]) -> Result<(), std::io::Error> {
    if let (Some(sel_start), Some(sel_end)) = (state.selection_start, state.selection_end) {
        let lines_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        let selected_text = if state.block_selection {
            let (start, end) = normalize_selection(sel_start, sel_end);
            extract_block_selection(&lines_refs, start.0, start.1, end.0, end.1)
        } else {
            extract_selection(&lines_refs, sel_start, sel_end)
        };
        let mut clipboard_guard = get_clipboard().lock().unwrap();
        if let Some(ref mut cb) = *clipboard_guard
            && let Err(e) = cb.set_text(selected_text)
        {
            eprintln!("Failed to copy to clipboard: {}", e);
        }
        let _ = copy_to_clipboard("");
    }
    Ok(())
}

pub(crate) fn handle_paste(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    filename: &str,
) -> bool {
    let text = {
        let mut lock = get_clipboard().lock().unwrap();
        if let Some(cb) = lock.as_mut() {
            cb.get_text().unwrap_or_default()
        } else {
            String::new()
        }
    };
    let _ = paste_from_clipboard();
    if text.is_empty() {
        return false;
    }

    // We'll accumulate all edits to push as one composite for proper undo behavior
    let mut edits: Vec<Edit> = Vec::new();

    // If there's a selection, delete it first (inline, without creating separate undo entry)
    if state.has_selection() {
        let (sel_start, sel_end) = {
            let s = state.selection_start.unwrap();
            let e = state.selection_end.unwrap();
            if s.0 < e.0 || (s.0 == e.0 && s.1 <= e.1) {
                (s, e)
            } else {
                (e, s)
            }
        };
        let (s_line, s_col) = sel_start;
        let (e_line, e_col) = sel_end;

        if s_line < lines.len() && e_line < lines.len() {
            // Record deletion edits in the composite
            if s_line == e_line {
                // Single-line deletion
                let line = &lines[s_line];
                let end_col = e_col.min(line.len());
                if s_col < end_col {
                    let removed: Vec<char> = line[s_col..end_col].chars().collect();
                    for (i, ch) in removed.into_iter().enumerate().rev() {
                        edits.push(Edit::DeleteChar {
                            line: s_line,
                            col: s_col + i,
                            ch,
                        });
                    }
                    lines[s_line].replace_range(s_col..end_col, "");
                }
            } else {
                // Multi-line deletion
                // Tail of start line
                if s_col < lines[s_line].len() {
                    let tail: Vec<char> = lines[s_line][s_col..].chars().collect();
                    for (i, ch) in tail.into_iter().enumerate().rev() {
                        edits.push(Edit::DeleteChar {
                            line: s_line,
                            col: s_col + i,
                            ch,
                        });
                    }
                    lines[s_line].truncate(s_col);
                }
                // Middle full lines
                for (line_idx, content) in lines.iter().enumerate().take(e_line).skip(s_line + 1) {
                    edits.push(Edit::DeleteLine {
                        line: line_idx,
                        content: content.clone(),
                    });
                }
                // Head of end line
                let end_line_len = lines[e_line].len();
                let head_limit = e_col.min(end_line_len);
                let head: Vec<char> = lines[e_line][..head_limit].chars().collect();
                for (i, ch) in head.into_iter().enumerate().rev() {
                    edits.push(Edit::DeleteChar {
                        line: e_line,
                        col: i,
                        ch,
                    });
                }
                // Remove head portion
                if head_limit <= end_line_len {
                    lines[e_line].replace_range(..head_limit, "");
                }
                // Merge start and remaining end line
                let first_snapshot = lines[s_line].clone();
                let second_snapshot = lines[e_line].clone();
                edits.push(Edit::MergeLine {
                    line: s_line,
                    first: first_snapshot,
                    second: second_snapshot.clone(),
                });
                lines[s_line].push_str(&second_snapshot);
                // Remove intervening + original end line
                for _ in s_line + 1..=e_line {
                    lines.remove(s_line + 1);
                }
            }
            // Position cursor at selection start for paste
            state.cursor_line = s_line.saturating_sub(state.top_line);
            state.set_cursor_col(s_col, lines);
        }
        state.clear_selection();
    }

    let idx = state.absolute_line();
    if idx >= lines.len() {
        return false;
    }

    // Check if the pasted text ends with a newline (indicating complete lines)
    let text_ends_with_newline = text.ends_with('\n');

    let mut paste_lines: Vec<&str> = text.lines().collect();
    if paste_lines.is_empty() {
        return false;
    }

    // If text ends with newline, add an empty string to represent the final newline
    if text_ends_with_newline && !paste_lines.is_empty() {
        paste_lines.push("");
    }

    if paste_lines.len() == 1 {
        let paste_text = paste_lines[0];
        for (i, ch) in paste_text.chars().enumerate() {
            edits.push(Edit::InsertChar {
                line: idx,
                col: state.cursor_col + i,
                ch,
            });
        }
        lines[idx].insert_str(state.cursor_col, paste_text);
        state.cursor_col += paste_text.len();
        state.desired_cursor_col = state.cursor_col;
        state.modified = true;
    } else {
        let current_line = &lines[idx];
        let before = current_line[..state.cursor_col].to_string();
        let after = current_line[state.cursor_col..].to_string();
        let first_paste_line = paste_lines[0].to_string();
        edits.push(Edit::SplitLine {
            line: idx,
            col: state.cursor_col,
            before: before.clone(),
            after: after.clone(),
        });
        lines[idx] = before.clone() + &first_paste_line;
        for (i, paste_line) in paste_lines[1..paste_lines.len() - 1].iter().enumerate() {
            edits.push(Edit::InsertLine {
                line: idx + 1 + i,
                content: paste_line.to_string(),
            });
            lines.insert(idx + 1 + i, paste_line.to_string());
        }
        let last_paste_line = paste_lines.last().unwrap().to_string();
        let final_line = last_paste_line.clone() + &after;
        edits.push(Edit::InsertLine {
            line: idx + paste_lines.len() - 1,
            content: final_line.clone(),
        });
        lines.insert(idx + paste_lines.len() - 1, final_line);
        state.cursor_line = (idx + paste_lines.len() - 1).saturating_sub(state.top_line);
        state.set_cursor_col(last_paste_line.len(), lines);
        state.modified = true;
    }

    // Push all edits (selection deletion + paste) as a single composite so Ctrl+Z undoes the entire operation
    let absolute_line = state.absolute_line();
    let undo_cursor = Some((absolute_line, state.cursor_col, state.multi_cursors.clone()));
    state.undo_history.push_composite(edits, undo_cursor);

    state.undo_history.update_state(
        state.top_line,
        absolute_line,
        state.cursor_col,
        lines.clone(),
    );
    save_undo_with_timestamp(state, filename);

    // Validate cursor after paste operation (debug only)
    state.validate_cursor_invariants(lines);

    true
}

// ...existing code...

pub(crate) fn handle_cut(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    filename: &str,
) -> bool {
    if state.has_selection() {
        let (sel_start, sel_end) = (state.selection_start.unwrap(), state.selection_end.unwrap());
        let lines_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        let selected_text = extract_selection(&lines_refs, sel_start, sel_end);
        let mut clipboard_guard = get_clipboard().lock().unwrap();
        if let Some(ref mut cb) = *clipboard_guard {
            let _ = cb.set_text(selected_text);
        }
        let removed = remove_selection(state, lines, filename);
        return removed;
    }
    let abs = state.absolute_line();
    if abs >= lines.len() {
        return false;
    }
    let line_content = lines[abs].clone();
    let mut to_clip = line_content.clone();
    to_clip.push('\n');
    let mut clipboard_guard = get_clipboard().lock().unwrap();
    if let Some(ref mut cb) = *clipboard_guard {
        let _ = cb.set_text(to_clip);
    }
    state.undo_history.push(Edit::DeleteLine {
        line: abs,
        content: line_content.clone(),
    });
    lines.remove(abs);
    if abs >= lines.len() && abs > 0 {
        state.cursor_line = (abs - 1).saturating_sub(state.top_line);
        state.cursor_col = lines
            .get(abs - 1)
            .map(|l| char_len(l).min(state.cursor_col))
            .unwrap_or(0);
    } else {
        state.cursor_line = abs.saturating_sub(state.top_line);
        state.cursor_col = 0;
    }
    state.modified = true;
    let absolute_line = state.absolute_line();
    state.undo_history.update_state(
        state.top_line,
        absolute_line,
        state.cursor_col,
        lines.clone(),
    );
    save_undo_with_timestamp(state, filename);
    state.needs_redraw = true;
    true
}

pub(crate) fn remove_selection(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    filename: &str,
) -> bool {
    if !state.has_selection() {
        return false;
    }
    let (sel_start, sel_end) = {
        let s = state.selection_start.unwrap();
        let e = state.selection_end.unwrap();
        if s.0 < e.0 || (s.0 == e.0 && s.1 <= e.1) {
            (s, e)
        } else {
            (e, s)
        }
    };
    let (s_line, s_col) = sel_start;
    let (e_line, e_col) = sel_end;
    if s_line >= lines.len() || e_line >= lines.len() {
        state.clear_selection();
        return false;
    }
    if s_line == e_line && s_col == e_col {
        state.clear_selection();
        return false;
    }

    if state.block_selection {
        // Block selection deletion - remove column range from each line
        let mut edits = Vec::new();

        for line_idx in s_line..=e_line {
            if line_idx >= lines.len() {
                break;
            }
            let line = &mut lines[line_idx];
            let chars: Vec<char> = line.chars().collect();
            let line_start = s_col.min(chars.len());
            let line_end = e_col.min(chars.len());

            if line_start < line_end {
                let removed: Vec<char> = chars[line_start..line_end].to_vec();
                // Record deletes in reverse order
                for (i, ch) in removed.into_iter().enumerate().rev() {
                    edits.push(Edit::DeleteChar {
                        line: line_idx,
                        col: line_start + i,
                        ch,
                    });
                }
                // Rebuild line without the removed range
                let new_line: String = chars[..line_start]
                    .iter()
                    .chain(chars[line_end..].iter())
                    .collect();
                *line = new_line;
            }
        }

        let undo_cursor = Some((s_line, s_col, state.multi_cursors.clone()));
        // Push all deletes as a single composite edit
        state.undo_history.push_composite(edits, undo_cursor);

        // Position cursor at start of selection
        state.cursor_line = s_line.saturating_sub(state.top_line);
        state.cursor_col = s_col;
        state.clear_selection();
        state.modified = true;
        let absolute_line = state.absolute_line();
        state.undo_history.update_state(
            state.top_line,
            absolute_line,
            state.cursor_col,
            lines.clone(),
        );
        save_undo_with_timestamp(state, filename);
        state.needs_redraw = true;
        return true;
    }

    if s_line == e_line {
        // Single-line removal
        let line = &mut lines[s_line];
        let end_col = e_col.min(line.len());
        if s_col >= end_col {
            state.clear_selection();
            return false;
        }
        let removed: Vec<char> = line[s_col..end_col].chars().collect();
        // Record deletes in reverse order as composite
        let mut edits = Vec::new();
        for (i, ch) in removed.into_iter().enumerate().rev() {
            edits.push(Edit::DeleteChar {
                line: s_line,
                col: s_col + i,
                ch,
            });
        }
        let undo_cursor = Some((s_line, s_col, state.multi_cursors.clone()));
        state.undo_history.push_composite(edits, undo_cursor);
        line.replace_range(s_col..end_col, "");
    } else {
        // Multi-line removal
        let mut edits = Vec::new();

        // Tail of start line
        if s_col < lines[s_line].len() {
            let tail: Vec<char> = lines[s_line][s_col..].chars().collect();
            for (i, ch) in tail.into_iter().enumerate().rev() {
                edits.push(Edit::DeleteChar {
                    line: s_line,
                    col: s_col + i,
                    ch,
                });
            }
            lines[s_line].truncate(s_col);
        }
        // Middle full lines
        for (line_idx, content) in lines.iter().enumerate().take(e_line).skip(s_line + 1) {
            edits.push(Edit::DeleteLine {
                line: line_idx,
                content: content.clone(),
            });
        }
        // Head of end line
        let end_line_len = lines[e_line].len();
        let head_limit = e_col.min(end_line_len);
        let head: Vec<char> = lines[e_line][..head_limit].chars().collect();
        for (i, ch) in head.into_iter().enumerate().rev() {
            edits.push(Edit::DeleteChar {
                line: e_line,
                col: i,
                ch,
            });
        }
        // Remove head portion
        if head_limit <= end_line_len {
            lines[e_line].replace_range(..head_limit, "");
        }
        // Merge start and remaining end line
        let first_snapshot = lines[s_line].clone();
        let second_snapshot = lines[e_line].clone();
        edits.push(Edit::MergeLine {
            line: s_line,
            first: first_snapshot,
            second: second_snapshot.clone(),
        });

        let undo_cursor = Some((s_line, s_col, state.multi_cursors.clone()));
        // Push all edits as composite
        state.undo_history.push_composite(edits, undo_cursor);

        lines[s_line].push_str(&second_snapshot);
        // Remove intervening + original end line
        for _ in s_line + 1..=e_line {
            lines.remove(s_line + 1);
        }
    }

    state.cursor_line = s_line.saturating_sub(state.top_line);
    state.cursor_col = s_col;
    state.clear_selection();
    state.modified = true;
    let absolute_line = state.top_line + state.cursor_line;
    state.undo_history.update_state(
        state.top_line,
        absolute_line,
        state.cursor_col,
        lines.clone(),
    );
    save_undo_with_timestamp(state, filename);
    state.needs_redraw = true;
    true
}

pub(crate) fn insert_char(
    state: &mut FileViewerState,
    lines: &mut [String],
    c: char,
    filename: &str,
    _visible_lines: usize,
) -> bool {
    let idx = state.absolute_line();
    if idx < lines.len() {
        let line_char_len = char_len(&lines[idx]);
        if state.cursor_col <= line_char_len {
            let byte_idx = char_index_to_byte_index(&lines[idx], state.cursor_col);
            lines[idx].insert(byte_idx, c);

            state.undo_history.push(Edit::InsertChar {
                line: idx,
                col: state.cursor_col,
                ch: c,
            });
            state.cursor_col += 1;
            state.cursor_at_wrap_end = false; // Clear wrap end flag after typing
            state.desired_cursor_col = state.cursor_col;
            state
                .undo_history
                .update_state(state.top_line, idx, state.cursor_col, lines.to_vec());
            save_undo_with_timestamp(state, filename);

            // Ensure cursor is within bounds and validate invariants (debug only)
            state.clamp_cursor_to_line_bounds(lines);
            state.validate_cursor_invariants(lines);

            true
        } else {
            false
        }
    } else {
        false
    }
}

// Silence dead_code warning for insert_char_block used only in tests
#[allow(dead_code)]
/// Insert character on multiple lines for zero-width block selection
fn insert_char_block(
    state: &mut FileViewerState,
    lines: &mut [String],
    c: char,
    filename: &str,
) -> bool {
    if !state.has_selection() || !state.block_selection {
        return false;
    }

    let (sel_start, sel_end) = {
        let s = state.selection_start.unwrap();
        let e = state.selection_end.unwrap();
        if s.0 < e.0 || (s.0 == e.0 && s.1 <= e.1) {
            (s, e)
        } else {
            (e, s)
        }
    };

    let (s_line, s_col) = sel_start;
    let (e_line, e_col) = sel_end;

    // Only handle zero-width selections (multi-cursor)
    if s_col != e_col {
        return false;
    }

    // Insert character on each line in the block
    let mut inserted = false;
    for line_idx in s_line..=e_line {
        if line_idx < lines.len() {
            let line = &mut lines[line_idx];
            let insert_col = s_col.min(line.len());
            line.insert(insert_col, c);
            state.undo_history.push(Edit::InsertChar {
                line: line_idx,
                col: insert_col,
                ch: c,
            });
            inserted = true;
        }
    }

    if inserted {
        // Move cursor and selection one column to the right
        state.cursor_col = s_col + 1;
        state.selection_start = Some((s_line, s_col + 1));
        state.selection_end = Some((e_line, e_col + 1));

        let absolute_line = state.absolute_line();
        state.undo_history.update_state(
            state.top_line,
            absolute_line,
            state.cursor_col,
            lines.to_vec(),
        );
        save_undo_with_timestamp(state, filename);
        true
    } else {
        false
    }
}

pub(crate) fn split_line(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    visible_lines: usize,
    filename: &str,
) -> bool {
    let idx = state.absolute_line();
    if idx >= lines.len() {
        return false;
    }
    let split_at_char = state.cursor_col.min(char_len(&lines[idx]));
    let split_at_byte = char_index_to_byte_index(&lines[idx], split_at_char);
    let line_clone = lines[idx].clone();
    let (before, after) = line_clone.split_at(split_at_byte);
    state.undo_history.push(Edit::SplitLine {
        line: idx,
        col: split_at_char,
        before: before.to_string(),
        after: after.to_string(),
    });
    lines[idx] = before.to_string();
    lines.insert(idx + 1, after.to_string());
    if state.cursor_line + 1 < visible_lines {
        state.cursor_line += 1;
    } else {
        state.top_line += 1;
    }
    state.cursor_col = 0;
    state.desired_cursor_col = 0;
    let absolute_line = state.absolute_line();
    state.undo_history.update_state(
        state.top_line,
        absolute_line,
        state.cursor_col,
        lines.clone(),
    );
    save_undo_with_timestamp(state, filename);
    true
}

pub(crate) fn delete_backward(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    filename: &str,
) -> bool {
    let idx = state.absolute_line();
    if idx >= lines.len() {
        return false;
    }
    if state.cursor_col > 0 && state.cursor_col <= char_len(&lines[idx]) {
        let ch = lines[idx].chars().nth(state.cursor_col - 1).unwrap();
        let byte_idx = char_index_to_byte_index(&lines[idx], state.cursor_col - 1);
        lines[idx].remove(byte_idx);
        state.undo_history.push(Edit::DeleteChar {
            line: idx,
            col: state.cursor_col - 1,
            ch,
        });
        state.cursor_col -= 1;
        state.desired_cursor_col = state.cursor_col;
        state
            .undo_history
            .update_state(state.top_line, idx, state.cursor_col, lines.to_vec());
        save_undo_with_timestamp(state, filename);
        true
    } else if idx > 0 {
        let current = lines.remove(idx);
        let prev_len = char_len(&lines[idx - 1]);
        let first_snapshot = lines[idx - 1].clone();
        lines[idx - 1].push_str(&current);
        state.undo_history.push(Edit::MergeLine {
            line: idx - 1,
            first: first_snapshot,
            second: current,
        });
        if state.cursor_line > 0 {
            state.cursor_line -= 1;
        } else {
            state.top_line = state.top_line.saturating_sub(1);
        }
        state.cursor_col = prev_len;
        state.desired_cursor_col = state.cursor_col;
        let absolute_line = state.absolute_line();
        state.undo_history.update_state(
            state.top_line,
            absolute_line,
            state.cursor_col,
            lines.clone(),
        );
        save_undo_with_timestamp(state, filename);

        // Ensure cursor is within bounds and validate invariants (debug only)
        state.clamp_cursor_to_line_bounds(lines);
        state.validate_cursor_invariants(lines);

        true
    } else {
        false
    }
}

pub(crate) fn delete_forward(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    filename: &str,
) -> bool {
    let idx = state.absolute_line();
    if idx >= lines.len() {
        return false;
    }
    if state.cursor_col < char_len(&lines[idx]) {
        let ch = lines[idx].chars().nth(state.cursor_col).unwrap();
        let byte_idx = char_index_to_byte_index(&lines[idx], state.cursor_col);
        lines[idx].remove(byte_idx);
        state.undo_history.push(Edit::DeleteChar {
            line: idx,
            col: state.cursor_col,
            ch,
        });
        state
            .undo_history
            .update_state(state.top_line, idx, state.cursor_col, lines.to_vec());
        save_undo_with_timestamp(state, filename);
        true
    } else if idx + 1 < lines.len() {
        let next_line = lines.remove(idx + 1);
        let first_snapshot = lines[idx].clone();
        lines[idx].push_str(&next_line);
        state.undo_history.push(Edit::MergeLine {
            line: idx,
            first: first_snapshot,
            second: next_line,
        });
        state
            .undo_history
            .update_state(state.top_line, idx, state.cursor_col, lines.clone());
        save_undo_with_timestamp(state, filename);

        // Ensure cursor is within bounds and validate invariants (debug only)
        state.clamp_cursor_to_line_bounds(lines);
        state.validate_cursor_invariants(lines);

        true
    } else {
        false
    }
}

pub(crate) fn delete_word_backward(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    filename: &str,
) -> bool {
    let idx = state.absolute_line();
    if idx >= lines.len() {
        return false;
    }

    if state.cursor_col == 0 {
        // At beginning of line, behave like regular backspace (merge with previous line)
        return delete_backward(state, lines, filename);
    }

    let line = &lines[idx];
    let start_col = state.cursor_col;
    let mut end_col = start_col;

    // Find the start of the word to delete
    // First skip any non-word characters (whitespace/punctuation)
    while end_col > 0 {
        let c = line.chars().nth(end_col - 1).unwrap_or(' ');
        if is_word_char(c) {
            break;
        }
        end_col -= 1;
    }
    // Then skip word characters
    while end_col > 0 {
        let c = line.chars().nth(end_col - 1).unwrap_or(' ');
        if !is_word_char(c) {
            break;
        }
        end_col -= 1;
    }

    // Delete characters from end_col to start_col
    let deleted_text: String = line.chars().skip(end_col).take(start_col - end_col).collect();
    
    // Create single undo entry for the entire word deletion
    state.undo_history.push(Edit::DeleteWord {
        line: idx,
        col: end_col,
        text: deleted_text,
        forward: false,
    });
    
    // Convert character indices to byte indices for replace_range
    let start_byte = char_index_to_byte_index(&lines[idx], end_col);
    let end_byte = char_index_to_byte_index(&lines[idx], start_col);
    lines[idx].replace_range(start_byte..end_byte, "");
    state.cursor_col = end_col;
    state.desired_cursor_col = state.cursor_col;

    state
        .undo_history
        .update_state(state.top_line, idx, state.cursor_col, lines.to_vec());
    save_undo_with_timestamp(state, filename);
    true
}

pub(crate) fn delete_word_forward(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    filename: &str,
) -> bool {
    let idx = state.absolute_line();
    if idx >= lines.len() {
        return false;
    }

    let line = &lines[idx];
    let start_col = state.cursor_col;

    if start_col >= line.len() {
        // At end of line, behave like regular delete (merge with next line)
        return delete_forward(state, lines, filename);
    }

    let mut end_col = start_col;

    // Find the end of the word to delete
    // First skip any non-word characters (whitespace/punctuation)
    while end_col < line.len() {
        let c = line.chars().nth(end_col).unwrap_or(' ');
        if is_word_char(c) {
            break;
        }
        end_col += 1;
    }
    // Then skip word characters
    let line_char_len = char_len(line);
    while end_col < line_char_len {
        let c = line.chars().nth(end_col).unwrap_or(' ');
        if !is_word_char(c) {
            break;
        }
        end_col += 1;
    }

    // Delete characters from start_col to end_col
    let deleted_text: String = line.chars().skip(start_col).take(end_col - start_col).collect();
    
    // Create single undo entry for the entire word deletion
    state.undo_history.push(Edit::DeleteWord {
        line: idx,
        col: start_col,
        text: deleted_text,
        forward: true,
    });
    
    // Convert character indices to byte indices for replace_range
    let start_byte = char_index_to_byte_index(&lines[idx], start_col);
    let end_byte = char_index_to_byte_index(&lines[idx], end_col);
    lines[idx].replace_range(start_byte..end_byte, "");

    state
        .undo_history
        .update_state(state.top_line, idx, state.cursor_col, lines.to_vec());
    save_undo_with_timestamp(state, filename);
    true
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub(crate) fn insert_tab(
    state: &mut FileViewerState,
    lines: &mut [String],
    filename: &str,
) -> bool {
    let idx = state.absolute_line();
    let tab_width = state.settings.tab_width;
    if idx < lines.len() && state.cursor_col <= char_len(&lines[idx]) {
        let byte_idx = char_index_to_byte_index(&lines[idx], state.cursor_col);
        let spaces = " ".repeat(tab_width);
        lines[idx].insert_str(byte_idx, &spaces);
        for (i, _) in spaces.chars().enumerate() {
            state.undo_history.push(Edit::InsertChar {
                line: idx,
                col: state.cursor_col + i,
                ch: ' ',
            });
        }
        state.cursor_col += tab_width;
        state.desired_cursor_col = state.cursor_col;
        state
            .undo_history
            .update_state(state.top_line, idx, state.cursor_col, lines.to_vec());
        save_undo_with_timestamp(state, filename);
        true
    } else {
        false
    }
}

/// Delete the undo history file for the given file path and remove empty parent directories
pub fn delete_file_history(file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let history_path = crate::undo::UndoHistory::history_path_for(file_path)?;
    if history_path.exists() {
        fs::remove_file(&history_path)?;
    }
    // Also remove from recent files list to keep both in sync
    let _ = crate::recent::remove_recent_file(file_path);
    Ok(())
}

/// Save file content to disk
pub(crate) fn save_file(path: &str, lines: &[String]) -> Result<(), std::io::Error> {
    // Construct content with newlines preserved; assume lines vector does not include trailing newline for last line
    let mut content = String::new();
    for (i, line) in lines.iter().enumerate() {
        content.push_str(line);
        if i + 1 < lines.len() {
            content.push('\n');
        }
    }
    fs::write(path, content)?;
    Ok(())
}

pub(crate) fn apply_undo(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    filename: &str,
    visible_lines: usize,
) -> bool {
    if let Some(edit) = state.undo_history.undo() {
        let result = match &edit {
            Edit::CompositeEdit { edits, undo_cursor } => {
                // Undo composite edit: apply all edits in reverse order
                let mut success = true;
                for e in edits.iter().rev() {
                    if !apply_single_undo_edit(state, lines, e) {
                        success = false;
                    }
                }
                // Restore cursor & multi-cursors snapshot if present
                if let Some((line, col, multi)) = undo_cursor {
                    state.cursor_line = line.saturating_sub(state.top_line);
                    state.cursor_col = *col;
                    state.multi_cursors = multi.clone();
                }
                success
            }
            _ => apply_single_undo_edit(state, lines, &edit),
        };

        if result {
            state.ensure_cursor_visible(visible_lines, lines);
            let absolute_line = state.absolute_line();
            state
                .undo_history
                .update_state(state.top_line, absolute_line, state.cursor_col, lines.clone());
            state.modified = state.undo_history.modified;
            save_undo_with_timestamp(state, filename);
        }
        result
    } else {
        false
    }
}

fn apply_single_undo_edit(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    edit: &Edit,
) -> bool {
    match edit {
        Edit::InsertChar { line, col, .. } => {
            // Undo insert: delete the character
            if *line < lines.len() && *col < char_len(&lines[*line]) {
                let byte_idx = char_index_to_byte_index(&lines[*line], *col);
                lines[*line].remove(byte_idx);
                state.cursor_col = *col;
                state.cursor_line = line.saturating_sub(state.top_line);
                true
            } else {
                false
            }
        }
        Edit::DeleteChar { line, col, ch } => {
            // Undo delete: insert the character back
            if *line < lines.len() && *col <= char_len(&lines[*line]) {
                let byte_idx = char_index_to_byte_index(&lines[*line], *col);
                lines[*line].insert(byte_idx, *ch);
                state.cursor_col = col + 1;
                state.cursor_line = line.saturating_sub(state.top_line);
                true
            } else {
                false
            }
        }
        Edit::SplitLine {
            line, col, before, after
        } => {
            // Undo split: merge the lines back using stored before/after content
            // This handles the case where the "after" line may have been deleted already
            if *line < lines.len() {
                // Restore the original line by combining before + after
                lines[*line] = format!("{}{}", before, after);
                // Remove any lines that were created by the split
                // (they may have already been deleted by undo of subsequent insertions)
                if line + 1 < lines.len() && lines[line + 1] != *after {
                    // The next line is not our "after" content, so don't remove it
                } else if line + 1 < lines.len() {
                    // The next line matches our "after", remove it
                    lines.remove(line + 1);
                }
                state.cursor_col = *col;
                state.cursor_line = line.saturating_sub(state.top_line);
                true
            } else {
                false
            }
        }
        Edit::MergeLine {
            line,
            first,
            second,
        } => {
            // Undo merge: split the lines back
            if *line < lines.len() {
                lines[*line] = first.clone();
                lines.insert(line + 1, second.clone());
                state.cursor_col = 0;
                state.cursor_line = (line + 1).saturating_sub(state.top_line);
                true
            } else {
                false
            }
        }
        Edit::InsertLine { line, .. } => {
            // Undo insert line: delete the line
            if *line < lines.len() {
                lines.remove(*line);
                state.cursor_line = line.saturating_sub(state.top_line);
                state.cursor_col = 0;
                true
            } else {
                false
            }
        }
        Edit::DeleteLine { line, content } => {
            // Undo delete line: insert the line back
            if *line <= lines.len() {
                lines.insert(*line, content.clone());
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
        Edit::ReplaceLine { line, old_content, .. } => {
            // Undo replace: restore old content
            if *line < lines.len() {
                lines[*line] = old_content.clone();
                state.cursor_line = line.saturating_sub(state.top_line);
                state.cursor_col = 0;
                true
            } else {
                false
            }
        }
        Edit::DeleteWord { line, col, text, forward } => {
            // Undo word deletion: insert the word back
            if *line < lines.len() && *col <= lines[*line].len() {
                let line_text = &lines[*line];
                let new_line: String = line_text
                    .chars()
                    .take(*col)
                    .chain(text.chars())
                    .chain(line_text.chars().skip(*col))
                    .collect();
                lines[*line] = new_line;
                state.cursor_line = line.saturating_sub(state.top_line);
                // Restore cursor: for backward deletion, cursor goes to end of restored text
                // For forward deletion, cursor stays at col
                state.cursor_col = if *forward { *col } else { col + text.chars().count() };
                true
            } else {
                false
            }
        }
        Edit::CompositeEdit { .. } => {
            // Nested composite edits should not happen, but handle gracefully
            false
        }
    }
}

pub(crate) fn apply_redo(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    filename: &str,
    visible_lines: usize,
) -> bool {
    if let Some(edit) = state.undo_history.redo() {
        let result = match &edit {
            Edit::CompositeEdit { edits, undo_cursor } => {
                // Redo composite edit: apply all edits in forward order
                let mut success = true;
                for e in edits.iter() {
                    if !apply_single_redo_edit(state, lines, e) {
                        success = false;
                    }
                }
                // Restore cursor & multi-cursors snapshot if present
                if let Some((line, col, multi)) = undo_cursor {
                    state.cursor_line = line.saturating_sub(state.top_line);
                    state.cursor_col = *col;
                    state.multi_cursors = multi.clone();
                }
                success
            }
            _ => apply_single_redo_edit(state, lines, &edit),
        };

        if result {
            state.ensure_cursor_visible(visible_lines, lines);
            let absolute_line = state.absolute_line();
            state
                .undo_history
                .update_state(state.top_line, absolute_line, state.cursor_col, lines.clone());
            state.modified = state.undo_history.modified;
            save_undo_with_timestamp(state, filename);
        }
        result
    } else {
        false
    }
}

fn apply_single_redo_edit(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    edit: &Edit,
) -> bool {
    match edit {
        Edit::InsertChar { line, col, ch } => {
            // Redo insert: insert the character
            if *line < lines.len() && *col <= char_len(&lines[*line]) {
                let byte_idx = char_index_to_byte_index(&lines[*line], *col);
                lines[*line].insert(byte_idx, *ch);
                state.cursor_col = col + 1;
                state.cursor_line = line.saturating_sub(state.top_line);
                true
            } else {
                false
            }
        }
        Edit::DeleteChar { line, col, .. } => {
            // Redo delete: delete the character
            if *line < lines.len() && *col < char_len(&lines[*line]) {
                let byte_idx = char_index_to_byte_index(&lines[*line], *col);
                lines[*line].remove(byte_idx);
                state.cursor_col = *col;
                state.cursor_line = line.saturating_sub(state.top_line);
                true
            } else {
                false
            }
        }
        Edit::SplitLine {
            line,
            col: _,
            before,
            after,
        } => {
            // Redo split: split the line
            if *line < lines.len() {
                lines[*line] = before.clone();
                lines.insert(line + 1, after.clone());
                state.cursor_col = 0;
                state.cursor_line = (line + 1).saturating_sub(state.top_line);
                true
            } else {
                false
            }
        }
        Edit::MergeLine { line, .. } => {
            // Redo merge: merge the lines
            if *line < lines.len() && line + 1 < lines.len() {
                let next = lines.remove(line + 1);
                let prev_len = lines[*line].len();
                lines[*line].push_str(&next);
                state.cursor_col = prev_len;
                state.cursor_line = line.saturating_sub(state.top_line);
                true
            } else {
                false
            }
        }
        Edit::InsertLine { line, content } => {
            // Redo insert line: insert the line
            if *line <= lines.len() {
                lines.insert(*line, content.clone());
                state.cursor_line = line.saturating_sub(state.top_line);
                state.cursor_col = 0;
                true
            } else {
                false
            }
        }
        Edit::DeleteLine { line, .. } => {
            // Redo delete line: delete the line
            if *line < lines.len() {
                lines.remove(*line);
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
        Edit::ReplaceLine { line, new_content, .. } => {
            // Redo replace: apply new content
            if *line < lines.len() {
                lines[*line] = new_content.clone();
                state.cursor_line = line.saturating_sub(state.top_line);
                state.cursor_col = 0;
                true
            } else {
                false
            }
        }
        Edit::DeleteWord { line, col, text, .. } => {
            // Redo word deletion: delete the word again
            if *line < lines.len() {
                let text_len = text.chars().count();
                if *col + text_len <= lines[*line].chars().count() {
                    let line_text = &lines[*line];
                    let new_line: String = line_text
                        .chars()
                        .take(*col)
                        .chain(line_text.chars().skip(col + text_len))
                        .collect();
                    lines[*line] = new_line;
                    state.cursor_line = line.saturating_sub(state.top_line);
                    state.cursor_col = *col;
                    true
                } else {
                    false
                }
            } else {
                false
            }
        }
        Edit::CompositeEdit { .. } => {
            // Nested composite edits should not happen, but handle gracefully
            false
        }
    }
}

/// Convert a block selection into multi-cursor mode at the block's start column
fn activate_multi_cursor_from_block(
    state: &mut FileViewerState,
    start: Position,
    end: Position,
) {
    let (start_line, start_col) = start;
    let end_line = end.0.max(start_line);

    // Position main cursor on the first line of the block
    state.cursor_line = start_line.saturating_sub(state.top_line);
    state.cursor_col = start_col;

    // Populate multi-cursors for remaining lines in the block
    state.multi_cursors.clear();
    for line in start_line + 1..=end_line {
        state.multi_cursors.push((line, start_col));
    }

    // Exit block selection mode
    state.selection_start = None;
    state.selection_end = None;
    state.selection_anchor = None;
    state.block_selection = false;
    state.needs_redraw = true;
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


    // If a zero-width block selection is active, convert it to multi-cursors for editing keys
    if state.block_selection {
        if let Some((start, end)) = state.selection_range() {
            let is_zero_width_block = start.1 == end.1 && start.0 != end.0;
            if is_zero_width_block
                && matches!(code, KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete)
            {
                activate_multi_cursor_from_block(state, start, end);
            }
        }
    }

    // Handle multi-cursor typing
    if state.has_multi_cursors() {
        match code {
            KeyCode::Char(c)
                if !modifiers.contains(KeyModifiers::CONTROL)
                    && !modifiers.contains(KeyModifiers::ALT) =>
            {
                return insert_char_multi_cursor(state, lines, *c, filename);
            }
            KeyCode::Backspace => {
                return delete_backward_multi_cursor(state, lines, filename);
            }
            KeyCode::Delete => {
                return delete_forward_multi_cursor(state, lines, filename);
            }
            // Any other key clears multi-cursors
            _ => {
                state.clear_multi_cursors();
            }
        }
    }

    match code {
        KeyCode::Backspace | KeyCode::Delete if state.has_selection() && state.block_selection => {
            if let Some((start, end)) = state.selection_range() {
                let start_col = start.1;
                let start_line = start.0;
                let end_line = end.0;
                let removed = remove_selection(state, lines, filename);
                activate_multi_cursor_from_block(state, (start_line, start_col), (end_line, start_col));
                removed
            } else {
                false
            }
        }
        KeyCode::Backspace | KeyCode::Delete if state.has_selection() => {
            remove_selection(state, lines, filename)
        }
        KeyCode::Char(c)
            if !modifiers.contains(KeyModifiers::CONTROL)
                && !modifiers.contains(KeyModifiers::ALT) =>
        {
            if state.has_selection() && state.block_selection {
                if let Some((start, end)) = state.selection_range() {
                    if start.1 != end.1 {
                        let start_col = start.1;
                        let start_line = start.0;
                        let end_line = end.0;
                        let _ = remove_selection(state, lines, filename);
                        activate_multi_cursor_from_block(
                            state,
                            (start_line, start_col),
                            (end_line, start_col),
                        );
                        return insert_char_multi_cursor(state, lines, *c, filename);
                    }
                }
            } else if state.has_selection() {
                remove_selection(state, lines, filename);
            }
            insert_char(state, lines, *c, filename, visible_lines)
        }
        KeyCode::Enter => {
            if state.has_selection() {
                remove_selection(state, lines, filename);
            }
            split_line(state, lines, visible_lines, filename)
        }
        KeyCode::Tab => {
            if state.has_selection() {
                remove_selection(state, lines, filename);
            }
            insert_tab(state, lines, filename)
        }
        KeyCode::Backspace => delete_backward(state, lines, filename),
        KeyCode::Delete => delete_forward(state, lines, filename),
        _ => false,
    }
}

/// Insert character at all cursor positions for multi-cursor mode
fn insert_char_multi_cursor(
    state: &mut FileViewerState,
    lines: &mut [String],
    c: char,
    filename: &str,
) -> bool {
    let positions = state.all_cursor_positions();
    let mut inserted = false;
    let mut edits = Vec::new();

    // Capture cursor & multi-cursors BEFORE mutation for correct undo restoration
    let undo_cursor = Some((state.absolute_line(), state.cursor_col, state.multi_cursors.clone()));

    for &(line_idx, col) in positions.iter().rev() {
        if line_idx < lines.len() && col <= char_len(&lines[line_idx]) {
            let byte_idx = char_index_to_byte_index(&lines[line_idx], col);
            lines[line_idx].insert(byte_idx, c);
            edits.push(Edit::InsertChar { line: line_idx, col, ch: c });
            inserted = true;
        }
    }

    if inserted {
        state.undo_history.push_composite(edits, undo_cursor);
        // Advance all cursor positions by 1
        state.cursor_col += 1;
        for cursor in &mut state.multi_cursors { cursor.1 += 1; }
        let absolute_line = state.absolute_line();
        state.undo_history.update_state(state.top_line, absolute_line, state.cursor_col, lines.to_vec());
        save_undo_with_timestamp(state, filename);
        true
    } else {
        false
    }
}

/// Delete backward at all cursor positions for multi-cursor mode
fn delete_backward_multi_cursor(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    filename: &str,
) -> bool {
    let mut positions = state.all_cursor_positions();
    let mut deleted = false;
    let mut edits = Vec::new();

    // Capture cursor & multi-cursors BEFORE mutation for correct undo restoration
    let undo_cursor = Some((state.absolute_line(), state.cursor_col, state.multi_cursors.clone()));

    positions.sort_by(|a, b| b.cmp(a));
    for &(line_idx, col) in &positions {
        if line_idx < lines.len() && col > 0 {
            let line = &mut lines[line_idx];
            let line_char_len = char_len(line);
            if col <= line_char_len {
                let chars: Vec<char> = line.chars().collect();
                if col > 0 && col <= chars.len() {
                    let removed_char = chars[col - 1];
                    edits.push(Edit::DeleteChar { line: line_idx, col: col - 1, ch: removed_char });
                    let byte_idx = char_index_to_byte_index(line, col - 1);
                    line.remove(byte_idx);
                    deleted = true;
                }
            }
        }
    }

    if deleted {
        state.undo_history.push_composite(edits, undo_cursor);
        // Move all cursor positions left by 1
        if state.cursor_col > 0 { state.cursor_col -= 1; }
        for cursor in &mut state.multi_cursors { if cursor.1 > 0 { cursor.1 -= 1; } }
        let absolute_line = state.absolute_line();
        state.undo_history.update_state(state.top_line, absolute_line, state.cursor_col, lines.clone());
        save_undo_with_timestamp(state, filename);
        true
    } else {
        false
    }
}

/// Delete forward at all cursor positions for multi-cursor mode
fn delete_forward_multi_cursor(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    filename: &str,
) -> bool {
    let mut positions = state.all_cursor_positions();
    let mut deleted = false;
    let mut edits = Vec::new();

    // Capture cursor & multi-cursors BEFORE mutation for correct undo restoration
    let undo_cursor = Some((state.absolute_line(), state.cursor_col, state.multi_cursors.clone()));

    positions.sort_by(|a, b| b.cmp(a));
    for &(line_idx, col) in &positions {
        if line_idx < lines.len() {
            let line = &mut lines[line_idx];
            let chars: Vec<char> = line.chars().collect();
            if col < chars.len() {
                let removed_char = chars[col];
                edits.push(Edit::DeleteChar { line: line_idx, col, ch: removed_char });
                line.remove(col);
                deleted = true;
            }
        }
    }

    if deleted {
        state.undo_history.push_composite(edits, undo_cursor);
        let absolute_line = state.absolute_line();
        state.undo_history.update_state(state.top_line, absolute_line, state.cursor_col, lines.clone());
        save_undo_with_timestamp(state, filename);
        true
    } else { false }
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

fn extract_block_selection(
    lines: &[&str],
    start_line: usize,
    start_col: usize,
    end_line: usize,
    end_col: usize,
) -> String {
    let mut result = String::new();

    for line_idx in start_line..=end_line {
        if let Some(line) = lines.get(line_idx) {
            let chars: Vec<char> = line.chars().collect();
            let line_start = start_col.min(chars.len());
            let line_end = end_col.min(chars.len());

            if line_start < line_end {
                result.extend(&chars[line_start..line_end]);
            }
            // Always add newline except for the last line
            if line_idx < end_line {
                result.push('\n');
            }
        }
    }

    result
}

fn extract_single_line_selection(
    lines: &[&str],
    line_idx: usize,
    start_col: usize,
    end_col: usize,
) -> String {
    lines
        .get(line_idx)
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
    if a.0 < b.0 || (a.0 == b.0 && a.1 <= b.1) {
        (a, b)
    } else {
        (b, a)
    }
}

pub(crate) fn apply_drag(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    sel_start: Position,
    sel_end: Position,
    dest: Position,
    copy: bool,
) {
    if state.is_point_in_selection(dest) {
        return;
    }
    let before_snapshot = lines.clone();
    let (start, end) = normalize_selection(sel_start, sel_end);
    let lines_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    let dragged_text = extract_selection(&lines_refs, start, end);
    if dragged_text.is_empty() {
        return;
    }
    let removed_lines = end.0 - start.0;
    // Remove original if move
    if !copy {
        let mut tmp_state =
            FileViewerState::new(state.term_width, state.undo_history.clone(), state.settings);
        tmp_state.selection_start = Some(start);
        tmp_state.selection_end = Some(end);
        remove_selection(&mut tmp_state, lines, "__drag__");
        // Adjust destination line if original block removed above
        if dest.0 > start.0 {
            state.cursor_line = (dest.0 - removed_lines).saturating_sub(state.top_line);
        }
    }
    // Compute insertion location after potential removal adjustment
    let insert_line = if dest.0 > lines.len() {
        lines.len().saturating_sub(1)
    } else {
        dest.0
    };
    if insert_line >= lines.len() {
        lines.push(String::new());
    }
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
        for mid in drag_lines.iter().skip(1).take(drag_lines.len() - 2) {
            lines.insert(idx, mid.to_string());
            idx += 1;
        }
        lines.insert(idx, format!("{}{}", drag_lines.last().unwrap(), after));
        state.cursor_line = idx.saturating_sub(state.top_line);
        state.cursor_col = drag_lines.last().unwrap().len();
    }
    state.selection_start = None;
    state.selection_end = None;
    state.modified = true;
    state.needs_redraw = true;
    let abs = state.absolute_line();
    state
        .undo_history
        .update_state(state.top_line, abs, state.cursor_col, lines.clone());
    state.undo_history.push(Edit::DragBlock {
        before: before_snapshot,
        after: lines.clone(),
        source_start: sel_start,
        source_end: sel_end,
        dest,
        copy,
    });
    save_undo_with_timestamp(state, "__drag__");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor_state::FileViewerState;
    use crate::env::set_temp_home;
    use crate::settings::Settings;
    use crate::undo::UndoHistory;

    fn create_test_state() -> FileViewerState<'static> {
        let settings = Box::leak(Box::new(
            Settings::load().expect("Failed to load test settings"),
        ));
        let undo_history = UndoHistory::new();
        FileViewerState::new(80, undo_history, settings)
    }

    #[test]
    fn insert_char_basic() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello".to_string()];
        state.cursor_col = 5;

        assert!(insert_char(&mut state, &mut lines, '!', "test.txt", 10));
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

        assert!(insert_char(&mut state, &mut lines, 'X', "test.txt", 10));
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

        insert_char(&mut state, &mut lines, '!', "test.txt", 10);
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

        insert_char(&mut state, &mut lines, '!', "test.txt", 10);
        apply_undo(&mut state, &mut lines, "test.txt", 10);

        assert!(apply_redo(&mut state, &mut lines, "test.txt", 10));
        assert_eq!(lines[0], "hello!");
        assert_eq!(state.cursor_col, 6);
    }

    #[test]
    fn paste_replaces_selection() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello world".to_string()];
        state.cursor_line = 0;
        state.cursor_col = 6; // after "hello "
        state.selection_start = Some((0, 6));
        state.selection_end = Some((0, 11)); // select "world"

        // Put clipboard content
        {
            let mut lock = get_clipboard().lock().unwrap();
            *lock = arboard::Clipboard::new().ok();
            if let Some(cb) = lock.as_mut() {
                let _ = cb.set_text("UE");
            }
        }

        assert!(handle_paste(&mut state, &mut lines, "test.txt"));
        assert_eq!(lines[0], "hello UE", "paste should replace selection");
    }

    #[test]
    fn paste_multiline_is_single_undo_action() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["abc".to_string()];
        state.cursor_line = 0;
        state.cursor_col = 1; // a|bc
        state.top_line = 0;

        // Clipboard with multiple lines
        let clipboard_text = "X\nY\nZ";
        {
            let mut lock = get_clipboard().lock().unwrap();
            *lock = arboard::Clipboard::new().ok();
            if let Some(cb) = lock.as_mut() {
                let _ = cb.set_text(clipboard_text);
            }
        }

        assert!(handle_paste(&mut state, &mut lines, "test.txt"));
        // Expect: aX, Y, Zbc
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "aX");
        assert_eq!(lines[1], "Y");
        assert_eq!(lines[2], "Zbc");

        // Verify it's a composite edit
        assert_eq!(state.undo_history.edits.len(), 1);
        if let Edit::CompositeEdit { edits, .. } = &state.undo_history.edits[0] {
            // Should have: SplitLine + 2x InsertLine
            assert!(edits.len() >= 3, "should have at least 3 edits, got {}", edits.len());
        } else {
            panic!("should be composite edit");
        }
    }

    #[test]
    fn paste_complete_lines_with_trailing_newline() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["before".to_string(), "after".to_string()];
        state.cursor_line = 0;
        state.cursor_col = 6; // at end of first line
        state.top_line = 0;

        // Clipboard with single line ending with newline (like copying entire line)
        let clipboard_text = "copied\n";
        {
            let mut lock = get_clipboard().lock().unwrap();
            *lock = arboard::Clipboard::new().ok();
            if let Some(cb) = lock.as_mut() {
                let _ = cb.set_text(clipboard_text);
            }
        }

        assert!(handle_paste(&mut state, &mut lines, "test.txt"));
        // Expect: "beforecopied", "", "after"
        // The trailing newline causes an empty line to be created
        assert_eq!(lines.len(), 3, "should have created a new line");
        assert_eq!(lines[0], "beforecopied");
        assert_eq!(lines[1], "");
        assert_eq!(lines[2], "after");
    }

    #[test]
    fn paste_multiple_complete_lines_with_trailing_newline() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["before".to_string(), "after".to_string()];
        state.cursor_line = 0;
        state.cursor_col = 6; // at end of first line
        state.top_line = 0;

        // Clipboard with two lines, each ending with newline (like copying two entire lines)
        let clipboard_text = "line1\nline2\n";
        {
            let mut lock = get_clipboard().lock().unwrap();
            *lock = arboard::Clipboard::new().ok();
            if let Some(cb) = lock.as_mut() {
                let _ = cb.set_text(clipboard_text);
            }
        }

        assert!(handle_paste(&mut state, &mut lines, "test.txt"));
        // With "line1\nline2\n", paste_lines becomes ["line1", "line2", ""]
        // Line 0: "beforeline1"
        // Line 1: "line2" (middle line)
        // Line 2: "" (last empty line from trailing newline)
        // Line 3: "after"
        assert_eq!(lines.len(), 4, "should have created new lines");
        assert_eq!(lines[0], "beforeline1");
        assert_eq!(lines[1], "line2");
        assert_eq!(lines[2], "");
        assert_eq!(lines[3], "after");
    }

    #[test]
    fn delete_word_backward_single_undo() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello world test".to_string()];
        state.cursor_col = 16; // At end of "test"

        // Delete word backward (should delete "test")
        assert!(delete_word_backward(&mut state, &mut lines, "test.txt"));
        assert_eq!(lines[0], "hello world ");
        assert_eq!(state.cursor_col, 12);

        // Undo should restore entire word with single undo
        assert!(apply_undo(&mut state, &mut lines, "test.txt", 10));
        assert_eq!(lines[0], "hello world test");
        assert_eq!(state.cursor_col, 16);
        
        // Verify only one edit was created
        assert_eq!(state.undo_history.edits.len(), 1);
        if let Edit::DeleteWord { text, forward, .. } = &state.undo_history.edits[0] {
            assert_eq!(text, "test");
            assert_eq!(*forward, false);
        } else {
            panic!("Expected DeleteWord edit");
        }
    }

    #[test]
    fn delete_word_forward_single_undo() {
        let (_tmp, _guard) = set_temp_home();
        let mut state = create_test_state();
        let mut lines = vec!["hello world test".to_string()];
        state.cursor_col = 6; // After "hello "

        // Delete word forward (should delete "world")
        assert!(delete_word_forward(&mut state, &mut lines, "test.txt"));
        assert_eq!(lines[0], "hello  test");
        assert_eq!(state.cursor_col, 6);

        // Undo should restore entire word with single undo
        assert!(apply_undo(&mut state, &mut lines, "test.txt", 10));
        assert_eq!(lines[0], "hello world test");
        assert_eq!(state.cursor_col, 6);
        
        // Verify only one edit was created
        assert_eq!(state.undo_history.edits.len(), 1);
        if let Edit::DeleteWord { text, forward, .. } = &state.undo_history.edits[0] {
            assert_eq!(text, "world");
            assert_eq!(*forward, true);
        } else {
            panic!("Expected DeleteWord edit");
        }
    }
}


#[allow(dead_code)]
fn copy_to_clipboard(_text: &str) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
#[allow(dead_code)]
fn paste_from_clipboard() -> Result<String, Box<dyn std::error::Error>> {
    Ok(String::new())
}

