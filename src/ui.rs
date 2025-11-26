use std::fs;
use std::io;
use std::time::{Duration, Instant};

use crossterm::{
    cursor::{SetCursorStyle, Hide, Show},
    event::{self, Event, EnableMouseCapture, DisableMouseCapture},
    execute,
    terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen, size},
};

use crate::coordinates::adjust_view_for_resize;
use crate::editor_state::FileViewerState;
use crate::event_handlers::{handle_key_event, handle_mouse_event, show_undo_conflict_confirmation};
use crate::rendering::render_screen;
use crate::settings::Settings;
use crate::undo::{UndoHistory, ValidationResult};
use crate::double_esc::{DoubleEscDetector, EscResult};

// Type alias for file selector result: (modified, next_file, quit, close)
type FileSelectorResult = Option<(bool, Option<String>, bool, bool)>;

// Constants to eliminate magic numbers
const DEFAULT_VISIBLE_LINES: usize = 20;
const STATUS_LINE_HEIGHT: usize = 2;
const CURSOR_CONTEXT_LINES: usize = 5;

// File watching constants for multi-instance synchronization
// 
// UNDO_FILE_CHECK_INTERVAL_MS: How often to poll the undo file for changes from other instances.
// - 150ms provides responsive updates without excessive I/O overhead
// - Filesystem mtime resolution is typically 1ms on ext4/NTFS, so we can detect changes reliably
// - Value is small enough that users won't notice lag when switching between instances
//
// SAVE_GRACE_PERIOD_MS: Time window after our own save to ignore undo file changes.
// - 200ms prevents reload loops where we detect our own save as an "external" change
// - Must be larger than UNDO_FILE_CHECK_INTERVAL_MS to ensure at least one poll skip
// - Accounts for filesystem flush delays and clock skew between file mtime and Instant::now()
const UNDO_FILE_CHECK_INTERVAL_MS: u64 = 150;
const SAVE_GRACE_PERIOD_MS: u64 = 200;

/// Result of file selector overlay: Selected(path), Cancelled, or Quit
enum SelectorResult {
    Selected(String),
    Cancelled,
    Quit,
}

/// Run file selector overlay and return selected file path if confirmed (None if cancelled)
fn run_file_selector_overlay(current_file: &str, visible_lines: &mut usize, settings: &Settings) -> crossterm::Result<SelectorResult> {
    use crossterm::event::{KeyCode, Event};
    let mut stdout = io::stdout();
    let tracked = crate::file_selector::get_tracked_files().unwrap_or_default();
    if tracked.is_empty() {
        return Ok(SelectorResult::Cancelled);
    }
    
    let current_canon = std::fs::canonicalize(current_file)
        .unwrap_or_else(|_| std::path::PathBuf::from(current_file));
    let current_str = current_canon.to_string_lossy();
    let mut selected_index = tracked.iter()
        .position(|e| e.path.to_string_lossy() == current_str)
        .unwrap_or(0);
    let scroll_offset = 0usize;
    let (_, th) = terminal::size()?;
    let vis = (th as usize).saturating_sub(1);
    execute!(stdout, Hide)?;
    crate::file_selector::render_file_list(&tracked, selected_index, scroll_offset, vis)?;

    // Use DoubleEscDetector for consistent double-Esc handling
    let mut last_esc = DoubleEscDetector::new(settings.double_tap_speed_ms);

    loop {
        // Check if first Esc timed out -> cancel overlay
        if last_esc.timed_out() {
            last_esc.clear();
            execute!(stdout, Show)?;
            return Ok(SelectorResult::Cancelled);
        }

        // Poll with timeout to detect when to cancel
        let timeout = last_esc.remaining_timeout();
        
        if !event::poll(timeout)? {
            // Timeout elapsed, check again at top of loop
            continue;
        }

        match event::read()? {
            Event::Key(k) => {
                // Process Esc key with DoubleEscDetector
                match last_esc.process_key(&k) {
                    EscResult::Double => {
                        execute!(stdout, Show)?;
                        return Ok(SelectorResult::Quit);
                    }
                    EscResult::First => {
                        continue; // Wait for second Esc or timeout
                    }
                    EscResult::None => {
                        // Normal key handling
                    }
                }
                
                match k.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        if selected_index > 0 {
                            let prev = selected_index;
                            selected_index -= 1;
                            crate::file_selector::render_selection_change(
                                &tracked, prev, selected_index, scroll_offset, vis
                            )?;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if selected_index + 1 < tracked.len() {
                            let prev = selected_index;
                            selected_index += 1;
                            crate::file_selector::render_selection_change(
                                &tracked, prev, selected_index, scroll_offset, vis
                            )?;
                        }
                    }
                    KeyCode::Enter => {
                        execute!(stdout, Show)?;
                        return Ok(SelectorResult::Selected(
                            tracked[selected_index].path.to_string_lossy().to_string()
                        ));
                    }
                    _ => {}
                }
            }
            Event::Resize(_, h) => {
                *visible_lines = (h as usize).saturating_sub(2);
                let vis_new = (h as usize).saturating_sub(1);
                crate::file_selector::render_file_list(&tracked, selected_index, scroll_offset, vis_new)?;
            }
            Event::Mouse(_) => { /* ignore mouse */ }
            _ => {}
        }
    }
}

pub fn show(files: &[String]) -> crossterm::Result<()> {
    let settings = Settings::load().expect("Failed to load settings");
    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        SetCursorStyle::BlinkingBar,
        terminal::Clear(ClearType::All)
    )?;

    let mut current_files: Vec<String> = files.to_vec();
    let mut unsaved: Vec<String> = Vec::new();
    let mut idx: usize = 0;

    loop {
        if idx >= current_files.len() { break; }
        let file = current_files[idx].clone();
        match fs::read_to_string(&file) {
            Ok(content) => {
                let (modified, next, quit, close_file) = editing_session(&file, content, &settings)?;
                if modified { if !unsaved.contains(&file) { unsaved.push(file.clone()); } } else { unsaved.retain(|f| f != &file); }
                
                // Handle close file signal
                if close_file {
                    // Remove from current files list and unsaved tracking
                    current_files.remove(idx);
                    unsaved.retain(|f| f != &file);
                    
                    // If there are still files, show file selector to choose next
                    if !current_files.is_empty() {
                        let mut visible_lines_temp = DEFAULT_VISIBLE_LINES;
                        // Use first remaining file as context (doesn't matter which)
                        let context_file = &current_files[idx.min(current_files.len() - 1)];
                        match run_file_selector_overlay(context_file, &mut visible_lines_temp, &settings)? {
                            SelectorResult::Selected(selected_file) => {
                                // Find and open the selected file
                                if let Some(pos) = current_files.iter().position(|f| f == &selected_file) {
                                    idx = pos;
                                } else {
                                    // Selected file not in list, add it
                                    current_files.insert(0, selected_file);
                                    idx = 0;
                                }
                                continue;
                            }
                            SelectorResult::Quit => {
                                break; // User chose to quit
                            }
                            SelectorResult::Cancelled => {
                                // User cancelled, just continue with next file or quit
                                if idx >= current_files.len() && idx > 0 {
                                    idx -= 1;
                                }
                                if idx >= current_files.len() {
                                    break;
                                }
                                continue;
                            }
                        }
                    } else {
                        // No more files, exit
                        break;
                    }
                }
                
                if let Some(target) = next {
                    // Switch to selected file
                    if let Some(pos) = current_files.iter().position(|f| f == &target) { idx = pos; } else { current_files.insert(0, target.clone()); idx = 0; }
                    continue; // start next session immediately
                }
                if quit { break; }
                // Advance to next originally provided file if any
                if idx + 1 < current_files.len() { idx += 1 } else { break }
            }
            Err(e) => { eprintln!("Could not read file {}: {}", file, e); if idx + 1 < current_files.len() { idx += 1; continue; } else { break; } }
        }
    }

    execute!(
        stdout,
        SetCursorStyle::DefaultUserShape,
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal::disable_raw_mode()?;
    if !unsaved.is_empty() { println!("Warning: Unsaved changes (not saved) for: {}", unsaved.join(", ")); }
    Ok(())
}

/// Helper function to show file selector and return the result
/// Eliminates code duplication across multiple validation branches
fn show_file_selector_and_return(
    file: &str,
    settings: &Settings,
) -> crossterm::Result<(bool, Option<String>, bool, bool)> {
    let mut visible_lines = DEFAULT_VISIBLE_LINES;
    match run_file_selector_overlay(file, &mut visible_lines, settings)? {
        SelectorResult::Selected(selected_file) => {
            Ok((false, Some(selected_file), false, false))
        }
        SelectorResult::Quit => {
            if let Err(e) = crate::session::save_selector_session() {
                eprintln!("Warning: failed to save selector session: {}", e);
            }
            Ok((false, None, true, false))
        }
        SelectorResult::Cancelled => {
            Ok((false, None, true, false))
        }
    }
}

/// Helper function to update undo history timestamp to current file time
fn update_undo_timestamp(undo_history: &mut UndoHistory, file: &str) {
    use std::time::SystemTime;
    if let Ok(metadata) = std::fs::metadata(file)
        && let Ok(modified) = metadata.modified()
        && let Ok(duration) = modified.duration_since(SystemTime::UNIX_EPOCH) {
        undo_history.file_timestamp = Some(duration.as_secs());
        let _ = undo_history.save(file);
    }
}

/// Try to reload undo history if it was modified by another instance
/// Returns true if reload occurred, false otherwise
fn try_reload_undo_from_external_change(
    state: &mut FileViewerState,
    lines: &mut Vec<String>,
    file: &str,
    last_known_mtime: Option<u128>,
    visible_lines: usize,
) -> (bool, Option<u128>) {
    let current_mtime = match UndoHistory::get_undo_file_mtime(file) {
        Some(mtime) => mtime,
        None => return (false, last_known_mtime),
    };

    // First time seeing an mtime - just record it
    let Some(last_mtime) = last_known_mtime else {
        return (false, Some(current_mtime));
    };

    // No change detected
    if current_mtime == last_mtime {
        return (false, last_known_mtime);
    }

    // Check if we're within grace period of our own save
    let now = Instant::now();
    let within_grace_period = state.last_save_time
        .map(|save_time| now.duration_since(save_time) < Duration::from_millis(SAVE_GRACE_PERIOD_MS))
        .unwrap_or(false);

    if within_grace_period {
        return (false, last_known_mtime);
    }

    // Undo file was modified externally - reload it
    let new_history = match UndoHistory::load(file) {
        Ok(h) => h,
        Err(_) => return (false, Some(current_mtime)),
    };

    // Check if there's file content to restore
    if let Some(new_content) = &new_history.file_content {
        // Update document content
        *lines = new_content.clone();

        // Restore cursor and scroll position from the new history
        state.top_line = new_history.scroll_top.min(lines.len());
        let new_cursor_line = new_history.cursor_line;
        let new_cursor_col = new_history.cursor_col;

        if new_cursor_line < lines.len() {
            state.cursor_line = new_cursor_line.saturating_sub(state.top_line);
            if new_cursor_col <= lines[new_cursor_line].len() {
                state.cursor_col = new_cursor_col;
            }
        }

        // Ensure cursor is visible after reload (similar to undo/redo)
        state.ensure_cursor_visible(visible_lines);

        // Update the undo history in state
        state.undo_history = new_history;
        state.modified = state.undo_history.modified;
        state.needs_redraw = true;

        (true, Some(current_mtime))
    } else {
        // No file content (e.g., after save in another instance)
        // But we should still sync the modified flag and other metadata
        state.undo_history = new_history;
        state.modified = state.undo_history.modified;
        state.needs_redraw = true;
        
        (false, Some(current_mtime))
    }
}

/// Persist editor state (undo history and session) to disk
/// This consolidates the common pattern of saving both undo history and editor session
fn persist_editor_state(state: &mut FileViewerState, file: &str) {
    state.undo_history.update_cursor(state.top_line, state.absolute_line(), state.cursor_col);
    if let Err(e) = state.undo_history.save(file) {
        eprintln!("Warning: failed to save undo history: {}", e);
    }
    state.last_save_time = Some(Instant::now());
    if let Err(e) = crate::session::save_editor_session(file) {
        eprintln!("Warning: failed to save editor session: {}", e);
    }
}

/// Helper to show file selector and handle result in event loop context
/// Returns Some((modified, next_file, quit, close)) to exit loop, or None to continue
fn handle_file_selector_in_loop(
    file: &str,
    state: &mut FileViewerState,
    visible_lines: &mut usize,
    settings: &Settings,
) -> crossterm::Result<FileSelectorResult> {
    // Persist state before showing selector
    state.undo_history.update_cursor(state.top_line, state.absolute_line(), state.cursor_col);
    if let Err(e) = state.undo_history.save(file) {
        eprintln!("Warning: failed to save undo history: {}", e);
    }
    state.last_save_time = Some(Instant::now());
    
    match run_file_selector_overlay(file, visible_lines, settings)? {
        SelectorResult::Selected(selected_file) => {
            Ok(Some((state.modified, Some(selected_file), false, false)))
        }
        SelectorResult::Quit => {
            if let Err(e) = crate::session::save_selector_session() {
                eprintln!("Warning: failed to save selector session: {}", e);
            }
            Ok(Some((state.modified, None, true, false)))
        }
        SelectorResult::Cancelled => {
            state.needs_redraw = true;
            Ok(None) // Continue loop
        }
    }
}

fn editing_session(file: &str, content: String, settings: &Settings) -> crossterm::Result<(bool, Option<String>, bool, bool)> {
    let mut stdout = io::stdout();
    let mut undo_history = UndoHistory::load(file).unwrap_or_else(|_| UndoHistory::new());
    
    // Validate undo file against current file modification time
    let validation_result = undo_history.validate(file);
    match validation_result {
        ValidationResult::Valid => {
            // Normal case - use undo file
        }
        ValidationResult::ModifiedNoUnsaved => {
            // File was modified externally and no unsaved changes - delete stale undo file and go to selector
            let _ = crate::editing::delete_file_history(file);
            return show_file_selector_and_return(file, settings);
        }
        ValidationResult::ModifiedWithUnsaved => {
            // File was modified externally but has unsaved changes - ask user
            if !show_undo_conflict_confirmation()? {
                // User pressed Esc (No) - go to file selector WITHOUT deleting undo file
                // This allows them to select the same file again and keep the undo history
                return show_file_selector_and_return(file, settings);
            } else {
                // User pressed Enter (Yes) - open file anyway with unsaved changes
                // Update the timestamp to current file time so future validations pass
                update_undo_timestamp(&mut undo_history, file);
            }
        }
    };
    
    let mut lines: Vec<String> = if let Some(saved) = &undo_history.file_content { 
        saved.clone() 
    } else { 
        content.lines().map(String::from).collect() 
    };
    
    let (term_width, term_height) = size()?;
    
    let mut state = FileViewerState::new(term_width, undo_history.clone(), settings);
    state.modified = state.undo_history.modified;
    state.top_line = undo_history.scroll_top.min(lines.len());
    let saved_cursor_line = undo_history.cursor_line;
    let saved_cursor_col = undo_history.cursor_col;
    if saved_cursor_line < lines.len() {
        if saved_cursor_line < state.top_line || saved_cursor_line >= state.top_line + (term_height as usize).saturating_sub(STATUS_LINE_HEIGHT) {
            state.top_line = saved_cursor_line.saturating_sub(CURSOR_CONTEXT_LINES);
        }
        state.cursor_line = saved_cursor_line.saturating_sub(state.top_line);
        if saved_cursor_col <= lines[saved_cursor_line].len() { state.cursor_col = saved_cursor_col; }
    }
    let mut visible_lines = (term_height as usize).saturating_sub(STATUS_LINE_HEIGHT);
    state.needs_redraw = true;

    // Track last Esc press time for double-press detection
    let mut last_esc = DoubleEscDetector::new(settings.double_tap_speed_ms);
    
    // File watching state for multi-instance synchronization
    let mut last_undo_check = Instant::now();
    let mut last_known_undo_mtime = UndoHistory::get_undo_file_mtime(file);

    loop {
        if state.needs_redraw { render_screen(&mut stdout, file, &lines, &state, visible_lines)?; state.needs_redraw = false; }
        
        // Check for external undo file changes (multi-instance editing)
        let now = Instant::now();
        if now.duration_since(last_undo_check) >= Duration::from_millis(UNDO_FILE_CHECK_INTERVAL_MS) {
            last_undo_check = now;
            let (_reloaded, new_mtime) = try_reload_undo_from_external_change(
                &mut state,
                &mut lines,
                file,
                last_known_undo_mtime,
                visible_lines,
            );
            last_known_undo_mtime = new_mtime;
        }
        
        // Check if we should open file selector after timeout
        if last_esc.timed_out() {
            last_esc.clear();
            if let Some(result) = handle_file_selector_in_loop(file, &mut state, &mut visible_lines, settings)? {
                return Ok(result);
            }
            continue;
        }
        
        // Use poll with timeout to detect when to open file selector
        // Cap timeout to file check interval so we wake up regularly to check for external changes
        let esc_timeout = last_esc.remaining_timeout();
        let file_check_timeout = Duration::from_millis(UNDO_FILE_CHECK_INTERVAL_MS);
        let timeout = if esc_timeout < file_check_timeout { esc_timeout } else { file_check_timeout };
        
        if !event::poll(timeout)? {
            if last_esc.timed_out() {
                last_esc.clear();
                if let Some(result) = handle_file_selector_in_loop(file, &mut state, &mut visible_lines, settings)? {
                    return Ok(result);
                }
            }
            continue;
        }
        
        match event::read()? {
            Event::Key(key_event) => {
                // Double-Esc processing
                match last_esc.process_key(&key_event) {
                    EscResult::Double => {
                        persist_editor_state(&mut state, file);
                        return Ok((state.modified, None, true, false));
                    }
                    EscResult::First => { continue; } // wait for second or timeout
                    EscResult::None => { /* normal key handling */ }
                }
                
                // Any other key clears the pending Esc
                last_esc.clear();
                
                // Handle key event and check for quit or close signals
                let (should_quit, should_close) = handle_key_event(&mut state, &mut lines, key_event, settings, visible_lines, file)?;
                if should_quit {
                    return Ok((state.modified, None, true, false));
                }
                if should_close {
                    return Ok((state.modified, None, false, true));
                }
            }
            Event::Resize(w, h) => {
                let absolute_cursor_line = state.absolute_line();
                let cursor_col = state.cursor_col;
                state.term_width = w;
                visible_lines = (h as usize).saturating_sub(STATUS_LINE_HEIGHT);
                let (new_top, rel_cursor) = adjust_view_for_resize(state.top_line, absolute_cursor_line, visible_lines, lines.len());
                state.top_line = new_top;
                state.cursor_line = rel_cursor;
                state.cursor_col = cursor_col;
                execute!(stdout, terminal::Clear(ClearType::All))?;
                state.needs_redraw = true;
            }
            Event::Mouse(mouse_event) => { handle_mouse_event(&mut state, &mut lines, mouse_event, visible_lines); }
            _ => {}
        }
    }
}
