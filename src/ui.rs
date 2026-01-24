use std::fs;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::{
    cursor::{Hide, SetCursorStyle, Show},
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{self, ClearType, EnterAlternateScreen, LeaveAlternateScreen, size},
};

use crate::coordinates::adjust_view_for_resize;
use crate::double_esc::{DoubleEscDetector, EscResult};
use crate::editor_state::FileViewerState;
use crate::event_handlers::{
    handle_key_event, handle_mouse_event, show_undo_conflict_confirmation,
};
use crate::rendering::render_screen;
use crate::settings::Settings;
use crate::undo::{UndoHistory, ValidationResult};

// Type alias for file selector result: (modified, next_file, quit, close)
type FileSelectorResult = Option<(bool, Option<String>, bool, bool)>;

// Constants to eliminate magic numbers
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

/// Generate a unique untitled filename (untitled, untitled-2, untitled-3, etc.)
pub fn generate_untitled_filename() -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(1);

    // Try to find a unique untitled-N name
    loop {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let filename = if n == 1 {
            "untitled".to_string()
        } else {
            format!("untitled-{}", n)
        };

        // Check if this file already exists in recent files
        let recent = crate::recent::get_recent_files().unwrap_or_default();
        let filename_lower = filename.to_lowercase();
        let exists = recent.iter().any(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_lowercase() == filename_lower)
                .unwrap_or(false)
        });

        if !exists {
            return filename;
        }
    }
}


/// Helper to fully restore terminal state on exit or when switching out of the editor
fn restore_terminal(stdout: &mut impl Write) -> io::Result<()> {
    // Ensure the cursor is visible and restore default user shape
    execute!(
        stdout,
        SetCursorStyle::DefaultUserShape,
        Show,
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    // Best-effort: raw mode might already be disabled in some flows
    let _ = terminal::disable_raw_mode();
    Ok(())
}

pub fn show(files: &[String]) -> std::io::Result<()> {
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
        if idx >= current_files.len() {
            break;
        }
        let file = current_files[idx].clone();
        // Update recent list so selector orders most recent first
        let _ = crate::recent::update_recent_file(&file);
        match fs::read_to_string(&file) {
            Ok(content) => {
                let (modified, next, quit, close_file) =
                    editing_session(&file, content, &settings)?;
                if modified {
                    if !unsaved.contains(&file) {
                        unsaved.push(file.clone());
                    }
                } else {
                    unsaved.retain(|f| f != &file);
                }

                // Handle close file signal
                if close_file {
                    // Remove from current files list and unsaved tracking
                    current_files.remove(idx);
                    unsaved.retain(|f| f != &file);

                    // Get first recent file or create new one
                    let recent_files = crate::recent::get_recent_files().unwrap_or_default();
                    let next_file = if let Some(first) = recent_files.first() {
                        first.to_string_lossy().to_string()
                    } else {
                        // No recent files - create new untitled file
                        generate_untitled_filename()
                    };

                    // Find or add the next file
                    if let Some(pos) = current_files.iter().position(|f| f == &next_file) {
                        idx = pos;
                    } else {
                        current_files.insert(0, next_file);
                        idx = 0;
                    }
                    continue;
                }

                if let Some(target) = next {
                    // Switch to selected file
                    if let Some(pos) = current_files.iter().position(|f| f == &target) {
                        idx = pos;
                    } else {
                        current_files.insert(0, target.clone());
                        idx = 0;
                    }
                    continue; // start next session immediately
                }
                if quit {
                    break;
                }
                // Advance to next originally provided file if any
                if idx + 1 < current_files.len() {
                    idx += 1
                } else {
                    break;
                }
            }
            Err(_e) => {
                // Treat missing/unreadable file as a new buffer with empty content
                let (modified, next, quit, close_file) =
                    editing_session(&file, String::new(), &settings)?;
                if modified {
                    if !unsaved.contains(&file) {
                        unsaved.push(file.clone());
                    }
                } else {
                    unsaved.retain(|f| f != &file);
                }

                if close_file {
                    current_files.remove(idx);
                    unsaved.retain(|f| f != &file);

                    // Get first recent file or create new one
                    let recent_files = crate::recent::get_recent_files().unwrap_or_default();
                    let next_file = if let Some(first) = recent_files.first() {
                        first.to_string_lossy().to_string()
                    } else {
                        // No recent files - create new untitled file
                        generate_untitled_filename()
                    };

                    // Find or add the next file
                    if let Some(pos) = current_files.iter().position(|f| f == &next_file) {
                        idx = pos;
                    } else {
                        current_files.insert(0, next_file);
                        idx = 0;
                    }
                    continue;
                }

                if let Some(target) = next {
                    if let Some(pos) = current_files.iter().position(|f| f == &target) {
                        idx = pos;
                    } else {
                        current_files.insert(0, target.clone());
                        idx = 0;
                    }
                    continue;
                }
                if quit {
                    break;
                }
                if idx + 1 < current_files.len() {
                    idx += 1
                } else {
                    break;
                }
            }
        }
    }

    restore_terminal(&mut stdout)?;
    if !unsaved.is_empty() {
        println!(
            "Warning: Unsaved changes for: {}",
            unsaved.join(", ")
        );
    }

    // Note: Session is already saved in event_handlers.rs when quitting from editor
    // (via is_exit_command or save_and_quit handlers, or double-Esc in persist_editor_state).
    // Only save selector session if we explicitly switch to the selector or have no files.
    // If we reached here normally (quit=true from a single file), the editor session was already saved.

    Ok(())
}


/// Helper function to update undo history timestamp to current file time
fn update_undo_timestamp(undo_history: &mut UndoHistory, file: &str) {
    use std::time::SystemTime;
    if let Ok(metadata) = std::fs::metadata(file)
        && let Ok(modified) = metadata.modified()
        && let Ok(duration) = modified.duration_since(SystemTime::UNIX_EPOCH)
    {
        undo_history.file_timestamp = Some(duration.as_secs());
        // find_history is already in undo_history, no need to update it here
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
    let within_grace_period = state
        .last_save_time
        .map(|save_time| {
            now.duration_since(save_time) < Duration::from_millis(SAVE_GRACE_PERIOD_MS)
        })
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
        // Check if content or undo history actually changed
        // Don't reload cursor position if only the cursor moved (common case when we saved after an edit)
        let content_changed = *lines != *new_content;
        let undo_changed = state.undo_history.edits != new_history.edits
            || state.undo_history.current != new_history.current;

        if content_changed {
            // Content changed - do a full reload
            *lines = new_content.clone();

            // Restore cursor and scroll position from the new history
            state.top_line = new_history.scroll_top.min(lines.len());
            let new_cursor_line = new_history.cursor_line;
            let new_cursor_col = new_history.cursor_col;

            if new_cursor_line < lines.len() {
                state.cursor_line = new_cursor_line.saturating_sub(state.top_line);
                if new_cursor_col <= lines[new_cursor_line].len() {
                    state.cursor_col = new_cursor_col;
                    state.desired_cursor_col = new_cursor_col;
                }
            }

            // Ensure cursor is visible after reload (similar to undo/redo)
            state.ensure_cursor_visible(visible_lines, &lines);
        } else if undo_changed {
            // Only undo history changed, not content - update history but keep cursor position
            // This handles the case where another instance did undo/redo
            // Keep current cursor position as the user may have moved it since the last save
        }

        // Always update the undo history and metadata
        state.undo_history = new_history.clone();
        state.find_history = new_history.find_history.clone(); // Sync find history
        state.modified = state.undo_history.modified;

        if content_changed {
            state.needs_redraw = true;
        }

        (content_changed, Some(current_mtime))
    } else {
        // No file content (e.g., after save in another instance)
        // But we should still sync the modified flag and other metadata
        let undo_changed = state.undo_history.edits != new_history.edits
            || state.undo_history.current != new_history.current;

        state.undo_history = new_history.clone();
        state.find_history = new_history.find_history.clone(); // Sync find history
        state.modified = state.undo_history.modified;

        if undo_changed {
            state.needs_redraw = true;
        }

        (false, Some(current_mtime))
    }
}

/// Persist editor state (undo history and session) to disk
/// This consolidates the common pattern of saving both undo history and editor session
fn persist_editor_state(state: &mut FileViewerState, file: &str) {
    state
        .undo_history
        .update_cursor(state.top_line, state.absolute_line(), state.cursor_col);
    state.undo_history.find_history = state.find_history.clone(); // Save find history
    if let Err(e) = state.undo_history.save(file) {
        eprintln!("Warning: failed to save undo history: {}", e);
    }
    state.last_save_time = Some(Instant::now());
    if let Err(e) = crate::session::save_editor_session(file) {
        eprintln!("Warning: failed to save editor session: {}", e);
    }
}


/// Helper to show open dialog and handle result in event loop context
/// Returns Some((modified, next_file, quit, close)) to exit loop, or None to continue
fn handle_open_dialog_in_loop(
    file: &str,
    state: &mut FileViewerState,
    settings: &Settings,
) -> std::io::Result<FileSelectorResult> {
    // Persist state before showing dialog
    state
        .undo_history
        .update_cursor(state.top_line, state.absolute_line(), state.cursor_col);
    if let Err(e) = state.undo_history.save(file) {
        eprintln!("Warning: failed to save undo history: {}", e);
    }
    state.last_save_time = Some(Instant::now());

    match crate::open_dialog::run_open_dialog(Some(file), settings, crate::open_dialog::DialogMode::Open)? {
        crate::open_dialog::OpenDialogResult::Selected(path) => {
            let path_str = path.to_string_lossy().to_string();
            Ok(Some((state.modified, Some(path_str), false, false)))
        }
        crate::open_dialog::OpenDialogResult::Quit => {
            if let Err(e) = crate::session::save_selector_session() {
                eprintln!("Warning: failed to save selector session: {}", e);
            }
            Ok(Some((state.modified, None, true, false)))
        }
        crate::open_dialog::OpenDialogResult::Cancelled => {
            state.needs_redraw = true;
            Ok(None) // Continue loop
        }
    }
}

/// Handle first Esc press in various modes
/// Returns true if handled (should continue waiting), false if in normal mode (should process Esc)
fn handle_first_esc(state: &mut FileViewerState) -> bool {

    // In help mode, ESC exits help
    if state.help_active {
        state.help_active = false;
        state.needs_redraw = true;
        return true;
    }

    // In menu mode, ESC closes menu
    if state.menu_bar.active {
        state.menu_bar.close();
        state.needs_redraw = true;
        return true;
    }

    // In find mode, exit find
    if state.find_active {
        state.find_active = false;
        state.find_pattern.clear();
        state.find_error = None;
        state.find_history_index = None;
        state.last_search_pattern = state.saved_search_pattern.clone();
        state.saved_search_pattern = None;
        state.needs_redraw = true;
        return true;
    }

    // In replace mode, exit replace and find
    if state.replace_active {
        state.replace_active = false;
        state.replace_pattern.clear();
        state.replace_cursor_pos = 0;
        // Also clear find mode and search highlights
        state.find_active = false;
        state.find_pattern.clear();
        state.find_error = None;
        state.find_history_index = None;
        state.last_search_pattern = None;
        state.saved_search_pattern = None;
        state.find_scope = None;
        state.search_hit_count = 0;
        state.search_current_hit = 0;
        state.filter_active = false; // Also clear filter mode
        state.needs_redraw = true;
        return true;
    }

    // Clear search highlights (after exiting find mode with Enter)
    if state.last_search_pattern.is_some() {
        state.last_search_pattern = None;
        state.find_scope = None;
        state.find_error = None;
        state.search_hit_count = 0;
        state.search_current_hit = 0;
        state.filter_active = false; // Also clear filter mode
        state.needs_redraw = true;
        return true;
    }

    // Exit goto_line mode
    if state.goto_line_active {
        state.goto_line_active = false;
        state.goto_line_input.clear();
        state.goto_line_cursor_pos = 0;
        state.goto_line_typing_started = false;
        state.needs_redraw = true;
        return true;
    }

    // Clear multi-cursors
    if state.has_multi_cursors() {
        state.clear_multi_cursors();
        state.needs_redraw = true;
        return true;
    }

    // Clear selection
    if state.has_selection() {
        state.clear_selection();
        state.needs_redraw = true;
        return true;
    }

    // In normal mode - Esc should open menu
    // Let it pass through to handle_key_event which will call handle_menu_key
    false
}

fn editing_session(
    file: &str,
    content: String,
    settings: &Settings,
) -> std::io::Result<(bool, Option<String>, bool, bool)> {
    // Set the current file for syntax highlighting
    crate::syntax::set_current_file(file);

    let mut stdout = io::stdout();
    let mut undo_history = UndoHistory::load(file).unwrap_or_else(|_| UndoHistory::new());

    // Validate undo file against current file modification time
    let validation_result = undo_history.validate(file);
    match validation_result {
        ValidationResult::Valid => {
            // Normal case - use undo file
        }
        ValidationResult::ModifiedNoUnsaved => {
            // File was modified externally and no unsaved changes - delete stale undo file and quit
            let _ = crate::editing::delete_file_history(file);
            return Ok((false, None, true, false)); // quit
        }
        ValidationResult::ModifiedWithUnsaved => {
            // File was modified externally but has unsaved changes - ask user
            if !show_undo_conflict_confirmation(settings)? {
                // User pressed Esc (No) - quit to let them handle it
                return Ok((false, None, true, false)); // quit
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
        let mut l: Vec<String> = content.lines().map(String::from).collect();
        // Ensure at least one empty line for empty files
        if l.is_empty() {
            l.push(String::new());
        }
        l
    };

    let (term_width, term_height) = size()?;

    let mut state = FileViewerState::new(term_width, undo_history.clone(), settings);
    state.modified = state.undo_history.modified;
    state.top_line = undo_history.scroll_top.min(lines.len());
    state.find_history = undo_history.find_history.clone(); // Restore find history

    // Check if this is an untitled file (filename starts with "untitled" and doesn't exist on disk)
    let filename_lower = std::path::Path::new(file)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();
    state.is_untitled = filename_lower.starts_with("untitled") && !std::path::Path::new(file).exists();

    // Update menu bar settings from configuration
    state.menu_bar.update_max_visible_files(settings.max_menu_files);
    // Update file menu with current recent files
    state.menu_bar.update_file_menu(settings.max_menu_files, file, state.modified);

    let saved_cursor_line = undo_history.cursor_line;
    let saved_cursor_col = undo_history.cursor_col;
    if saved_cursor_line < lines.len() {
        if saved_cursor_line < state.top_line
            || saved_cursor_line
                >= state.top_line + (term_height as usize).saturating_sub(STATUS_LINE_HEIGHT)
        {
            state.top_line = saved_cursor_line.saturating_sub(CURSOR_CONTEXT_LINES);
        }
        state.cursor_line = saved_cursor_line.saturating_sub(state.top_line);
        if saved_cursor_col <= lines[saved_cursor_line].len() {
            state.cursor_col = saved_cursor_col;
            state.desired_cursor_col = saved_cursor_col;
        }
    }
    let mut visible_lines = (term_height as usize).saturating_sub(STATUS_LINE_HEIGHT);
    state.needs_redraw = true;

    // Track last Esc press time for double-press detection
    let mut last_esc = DoubleEscDetector::new(settings.double_tap_speed_ms);

    // File watching state for multi-instance synchronization
    let mut last_undo_check = Instant::now();
    let mut last_known_undo_mtime = UndoHistory::get_undo_file_mtime(file);

    loop {
        if state.needs_redraw {
            // Update menu checkable states if menu is active (for both help and editor modes)
            if state.menu_bar.active {
                state.menu_bar.update_checkable(
                    crate::menu::MenuAction::ViewLineWrap,
                    state.is_line_wrapping_enabled()
                );
            }

            if state.help_active {
                // Render help screen
                let (tw, th) = terminal::size()?;
                let help_content =
                    crate::help::get_help_content(state.help_context, settings, tw as usize);
                crate::help::render_help(
                    &mut stdout,
                    &help_content,
                    state.help_scroll_offset,
                    tw,
                    th,
                )?;
            } else {
                render_screen(&mut stdout, file, &lines, &state, visible_lines)?;
            }
            state.needs_redraw = false;
        } else if state.needs_footer_redraw {
            // Only redraw the footer (e.g., for status messages)
            crate::rendering::render_footer(&mut stdout, &state, &lines, visible_lines)?;
            stdout.flush()?;
            state.needs_footer_redraw = false;
        } else if state.menu_bar.active && state.menu_bar.dropdown_open && state.menu_bar.needs_redraw {
            // Update menu checkable states before rendering dropdown
            state.menu_bar.update_checkable(
                crate::menu::MenuAction::ViewLineWrap,
                state.is_line_wrapping_enabled()
            );
            
            // Menu is open and needs redraw - render the dropdown menu overlay
            crate::menu::render_dropdown_menu(&mut stdout, &state.menu_bar, &state, &lines)?;
            state.menu_bar.needs_redraw = false;
            stdout.flush()?;
        }

        // Check for external undo file changes (multi-instance editing)
        let now = Instant::now();
        if now.duration_since(last_undo_check) >= Duration::from_millis(UNDO_FILE_CHECK_INTERVAL_MS)
        {
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

        // Use poll with timeout for file check interval
        let file_check_timeout = Duration::from_millis(UNDO_FILE_CHECK_INTERVAL_MS);
        let timeout = file_check_timeout;

        if !event::poll(timeout)? {
            // Handle continuous horizontal auto-scroll during mouse drag
            if crate::mouse_handlers::handle_continuous_auto_scroll(&mut state, &lines, visible_lines) {
                // Scrolling occurred, continue to next iteration to render
                continue;
            }
            
            // Update cursor blink state for multi-cursors (but not when menu is active)
            if !state.menu_bar.active && state.update_cursor_blink() {
                state.needs_redraw = true;
            }

            continue;
        }

        match event::read()? {
            Event::Key(key_event) => {
                let key_event = crate::event_handlers::normalize_key_event(key_event, settings);
                // Process all Esc keys through double-Esc detector first
                match last_esc.process_key(&key_event) {
                    EscResult::Double => {
                        // Double-Esc always exits the editor, regardless of mode
                        persist_editor_state(&mut state, file);
                        return Ok((state.modified, None, true, false));
                    }
                    EscResult::First => {
                        // First Esc - handle based on current mode
                        let handled = handle_first_esc(&mut state);
                        if handled {
                            continue; // Wait for second Esc or timeout
                        }
                        // If not handled (normal mode), fall through to handle_key_event
                    }
                    EscResult::None => {
                        // Not an Esc key - normal handling

                        // Special handling for F1 in help mode
                        if state.help_active && matches!(key_event.code, KeyCode::F(1)) {
                            state.help_active = false;
                            state.needs_redraw = true;
                            continue;
                        }
                    }
                }

                // Handle key event and check for quit or close signals
                let (should_quit, should_close) = handle_key_event(
                    &mut state,
                    &mut lines,
                    key_event,
                    settings,
                    visible_lines,
                    file,
                )?;
                if should_quit {
                    return Ok((state.modified, None, true, false));
                }
                if should_close {
                    return Ok((state.modified, None, false, true));
                }

                // Handle close all confirmation
                if state.close_all_confirmed {
                    state.close_all_confirmed = false;

                    // Get all tracked files
                    let all_files = crate::recent::get_recent_files().unwrap_or_default();

                    // Check for unsaved changes
                    let mut saved_files = Vec::new();
                    let mut unsaved_files = Vec::new();

                    for file_path in &all_files {
                        if crate::menu::check_file_has_unsaved_changes(file_path) {
                            unsaved_files.push(file_path.clone());
                        } else {
                            saved_files.push(file_path.clone());
                        }
                    }

                    // Close saved files
                    for file_path in &saved_files {
                        let _ = crate::editing::delete_file_history(&file_path.to_string_lossy());
                    }

                    // Always show status message
                    if !unsaved_files.is_empty() {
                        // Show warning if there were unsaved files
                        state.status_message = Some(format!(
                            "Closed {} file(s). {} file(s) with unsaved changes not closed.",
                            saved_files.len(),
                            unsaved_files.len()
                        ));
                        state.needs_footer_redraw = true;
                    } else if !saved_files.is_empty() {
                        // All files were closed
                        state.status_message = Some(format!("Closed {} file(s).", saved_files.len()));
                        state.needs_footer_redraw = true;
                    }

                    // If all files were closed, check if current file was one of them
                    if saved_files.iter().any(|p| p.to_string_lossy() == file) {
                        // Current file was closed - need to switch to another file or quit
                        if !unsaved_files.is_empty() {
                            // Switch to first unsaved file
                            let next_file = unsaved_files[0].to_string_lossy().to_string();
                            return Ok((false, Some(next_file), false, false));
                        } else {
                            // All files closed, quit
                            return Ok((false, None, true, false));
                        }
                    } else if saved_files.is_empty() {
                        // No files were closed (all had unsaved changes)
                        // Stay on current file
                    }

                    state.needs_redraw = true;
                }

                // Handle pending menu action (e.g., FileOpenRecent)
                if let Some(action) = state.pending_menu_action.take() {
                    match action {
                        crate::menu::MenuAction::FileNew => {
                            // Create a new untitled file immediately
                            let untitled_name = generate_untitled_filename();
                            // Save current file state before switching
                            persist_editor_state(&mut state, file);
                            // Return to open the new untitled file
                            return Ok((state.modified, Some(untitled_name), false, false));
                        }
                        crate::menu::MenuAction::FileOpenRecent(idx) => {
                            // Get the file at the specified index from recent files
                            let recent_files = crate::recent::get_recent_files().unwrap_or_default();
                            if let Some(path) = recent_files.get(idx) {
                                // Save current state before switching
                                persist_editor_state(&mut state, file);
                                // Return to switch to the selected file
                                return Ok((state.modified, Some(path.to_string_lossy().to_string()), false, false));
                            }
                            // If index is out of bounds, just ignore
                        }
                        crate::menu::MenuAction::FileOpenDialog => {
                            // Open directory tree dialog
                            if let Some(result) = handle_open_dialog_in_loop(
                                file,
                                &mut state,
                                settings,
                            )? {
                                return Ok(result);
                            }
                        }
                        crate::menu::MenuAction::FileSave => {
                            // This is an untitled file - show save-as dialog
                            if state.is_untitled {
                                // Exit raw mode temporarily for the dialog
                                execute!(stdout, Show, DisableMouseCapture, LeaveAlternateScreen)?;
                                terminal::disable_raw_mode()?;

                                // Show open dialog to choose save location
                                terminal::enable_raw_mode()?;
                                execute!(stdout, EnterAlternateScreen, EnableMouseCapture, Hide)?;

                                match crate::open_dialog::run_open_dialog(Some(file), settings, crate::open_dialog::DialogMode::SaveAs)? {
                                    crate::open_dialog::OpenDialogResult::Selected(path) => {
                                        let target_path = path.to_str().unwrap_or(file);

                                        // Check if target file already exists and ask for confirmation
                                        if std::path::Path::new(target_path).exists() {
                                            use crate::event_handlers::show_overwrite_confirmation;
                                            // Show overwrite confirmation in footer
                                            if !show_overwrite_confirmation(target_path, settings)? {
                                                // User declined - redraw and continue editing
                                                state.needs_redraw = true;
                                                continue; // Go back to event loop
                                            }
                                        }

                                        // User selected a path - save the file there
                                        use crate::editing::{save_file, delete_file_history};

                                        // Delete the old untitled undo file and remove from recent files
                                        let _ = delete_file_history(file);

                                        save_file(target_path, &lines)?;
                                        state.modified = false;
                                        state.undo_history.clear_unsaved_state();
                                        let abs = state.absolute_line();
                                        state.undo_history.update_cursor(state.top_line, abs, state.cursor_col);
                                        state.undo_history.find_history = state.find_history.clone();

                                        // Save undo history to the NEW file location
                                        let _ = state.undo_history.save(target_path);
                                        state.last_save_time = Some(Instant::now());

                                        // Switch to the new filename - don't persist to old file, it's deleted
                                        return Ok((false, Some(path.to_string_lossy().to_string()), false, false));
                                    }
                                    crate::open_dialog::OpenDialogResult::Cancelled => {
                                        // User cancelled - just redraw
                                        state.needs_redraw = true;
                                    }
                                    crate::open_dialog::OpenDialogResult::Quit => {
                                        // User wants to quit
                                        return Ok((state.modified, None, true, false));
                                    }
                                }
                            }
                        }
                        crate::menu::MenuAction::FileCloseAll => {
                            // Close menu and show confirmation in footer
                            state.menu_bar.close();
                            state.close_all_confirmation_active = true;
                            state.needs_footer_redraw = true;
                        }
                        _ => {
                            // Other actions should have been handled in event_handlers.rs
                        }
                    }
                }
            }
            Event::Resize(w, h) => {
                let absolute_cursor_line = state.absolute_line();
                let cursor_col = state.cursor_col;
                state.term_width = w;
                visible_lines = (h as usize).saturating_sub(STATUS_LINE_HEIGHT);
                let (new_top, rel_cursor) = adjust_view_for_resize(
                    state.top_line,
                    absolute_cursor_line,
                    visible_lines,
                    lines.len(),
                );
                state.top_line = new_top;
                state.cursor_line = rel_cursor;
                state.cursor_col = cursor_col;
                state.desired_cursor_col = cursor_col;
                execute!(stdout, terminal::Clear(ClearType::All))?;
                state.needs_redraw = true;
            }
            Event::Mouse(mouse_event) => {
                handle_mouse_event(&mut state, &mut lines, mouse_event, visible_lines);

                // Process pending menu actions from mouse clicks
                if let Some(action) = state.pending_menu_action.take() {
                    // Execute the menu action (same logic as keyboard menu actions in event_handlers.rs)
                    use crate::menu::MenuAction;
                    use crate::editing::{save_file, delete_file_history, handle_copy, handle_cut, handle_paste, apply_undo, apply_redo};
                    use std::time::Instant;

                    state.needs_redraw = true;

                    match action {
                        MenuAction::FileNew => {
                            // Create a new untitled file immediately
                            let untitled_name = generate_untitled_filename();
                            // Save current file state before switching
                            persist_editor_state(&mut state, file);
                            // Return to open the new untitled file
                            return Ok((state.modified, Some(untitled_name), false, false));
                        }
                        MenuAction::FileOpenDialog => {
                            // Open directory tree dialog
                            if let Some(result) = handle_open_dialog_in_loop(
                                file,
                                &mut state,
                                settings,
                            )? {
                                return Ok(result);
                            }
                        }
                        MenuAction::FileOpenRecent(idx) => {
                            let recent_files = crate::recent::get_recent_files().unwrap_or_default();
                            if let Some(path) = recent_files.get(idx) {
                                persist_editor_state(&mut state, file);
                                return Ok((state.modified, Some(path.to_string_lossy().to_string()), false, false));
                            }
                        }
                        MenuAction::FileRemove(_idx) => {
                            // File removal is handled in event_handlers.rs
                            // This case is here for exhaustiveness but should not be reached
                        }
                        MenuAction::FileSave => {
                            // If this is an untitled file, show save-as dialog
                            if state.is_untitled {
                                // Exit raw mode temporarily for the dialog
                                execute!(stdout, Show, DisableMouseCapture, LeaveAlternateScreen)?;
                                terminal::disable_raw_mode()?;

                                // Show open dialog to choose save location
                                terminal::enable_raw_mode()?;
                                execute!(stdout, EnterAlternateScreen, EnableMouseCapture, Hide)?;

                                match crate::open_dialog::run_open_dialog(Some(file), settings, crate::open_dialog::DialogMode::SaveAs)? {
                                    crate::open_dialog::OpenDialogResult::Selected(path) => {
                                        let target_path = path.to_str().unwrap_or(file);

                                        // Check if target file already exists and ask for confirmation
                                        if std::path::Path::new(target_path).exists() {
                                            use crate::event_handlers::show_overwrite_confirmation;
                                            // Show overwrite confirmation in footer
                                            if !show_overwrite_confirmation(target_path, settings)? {
                                                // User declined - redraw and continue editing
                                                state.needs_redraw = true;
                                                continue; // Go back to event loop
                                            }
                                        }

                                        // User selected a path - save the file there
                                        use crate::editing::delete_file_history;

                                        // Delete the old untitled undo file and remove from recent files
                                        let _ = delete_file_history(file);

                                        save_file(target_path, &lines)?;
                                        state.modified = false;
                                        state.undo_history.clear_unsaved_state();
                                        let abs = state.absolute_line();
                                        state.undo_history.update_cursor(state.top_line, abs, state.cursor_col);
                                        state.undo_history.find_history = state.find_history.clone();

                                        // Save undo history to the NEW file location
                                        let _ = state.undo_history.save(target_path);
                                        state.last_save_time = Some(Instant::now());

                                        // Switch to the new filename - don't persist to old file, it's deleted
                                        return Ok((false, Some(path.to_string_lossy().to_string()), false, false));
                                    }
                                    crate::open_dialog::OpenDialogResult::Cancelled => {
                                        // User cancelled - just redraw
                                        state.needs_redraw = true;
                                    }
                                    crate::open_dialog::OpenDialogResult::Quit => {
                                        // User wants to quit
                                        return Ok((state.modified, None, true, false));
                                    }
                                }
                            } else {
                                // Normal file - just save
                                save_file(file, &mut lines)?;
                                state.modified = false;
                                state.undo_history.clear_unsaved_state();
                                let abs = state.absolute_line();
                                state.undo_history.update_cursor(state.top_line, abs, state.cursor_col);
                                state.undo_history.find_history = state.find_history.clone();
                                let _ = state.undo_history.save(file);
                                state.last_save_time = Some(Instant::now());
                            }
                        }
                        MenuAction::FileClose => {
                            if state.modified {
                                let _ = crossterm::execute!(std::io::stdout(), crossterm::cursor::Show);
                                // Show simple yes/no prompt
                                let _ = crossterm::terminal::disable_raw_mode();
                                print!("\nClose file with unsaved changes? (y/N): ");
                                let _ = std::io::stdout().flush();
                                let mut input = String::new();
                                let _ = std::io::stdin().read_line(&mut input);
                                let _ = crossterm::terminal::enable_raw_mode();
                                let confirmed = input.trim().eq_ignore_ascii_case("y");
                                if confirmed {
                                    let _ = delete_file_history(file);
                                    return Ok((state.modified, None, false, true));
                                }
                            } else {
                                let _ = delete_file_history(file);
                                return Ok((state.modified, None, false, true));
                            }
                        }
                        MenuAction::FileCloseAll => {
                            // Close menu and show confirmation in footer
                            state.menu_bar.close();
                            state.close_all_confirmation_active = true;
                            state.needs_footer_redraw = true;
                        }
                        MenuAction::FileQuit => {
                            return Ok((state.modified, None, true, false));
                        }
                        MenuAction::EditUndo => {
                            apply_undo(&mut state, &mut lines, file, visible_lines);
                        }
                        MenuAction::EditRedo => {
                            apply_redo(&mut state, &mut lines, file, visible_lines);
                        }
                        MenuAction::EditCopy => {
                            let _ = handle_copy(&state, &lines);
                        }
                        MenuAction::EditCut => {
                            handle_cut(&mut state, &mut lines, file);
                        }
                        MenuAction::EditPaste => {
                            handle_paste(&mut state, &mut lines, file);
                        }
                        MenuAction::EditFind => {
                            state.saved_search_pattern = state.last_search_pattern.clone();
                            if let (Some(start), Some(end)) = (state.selection_start, state.selection_end) {
                                let normalized = if start.0 < end.0 || (start.0 == end.0 && start.1 <= end.1) {
                                    (start, end)
                                } else {
                                    (end, start)
                                };
                                state.find_scope = Some(normalized);
                            } else {
                                state.find_scope = None;
                            }
                            state.find_active = true;
                            state.find_pattern.clear();
                            state.find_cursor_pos = 0;
                            state.find_error = None;
                        }
                        MenuAction::ViewLineWrap => {
                            state.toggle_line_wrapping();
                            // Update menu checkbox to reflect new state
                            state.menu_bar.update_checkable(
                                crate::menu::MenuAction::ViewLineWrap,
                                state.is_line_wrapping_enabled()
                            );
                        }
                        MenuAction::HelpEditor => {
                            state.help_active = true;
                            state.help_context = crate::help::HelpContext::Editor;
                            state.help_scroll_offset = 0;
                        }
                        MenuAction::HelpFind => {
                            state.help_active = true;
                            state.help_context = crate::help::HelpContext::Find;
                            state.help_scroll_offset = 0;
                        }
                        MenuAction::HelpAbout => {
                            state.help_active = true;
                            state.help_context = crate::help::HelpContext::Editor;
                            state.help_scroll_offset = 0;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Print keyboard events with modifiers (for testing keybindings)
/// Exit with double-Esc
pub fn print_keys_mode() -> std::io::Result<()> {
    let mut stdout = io::stdout();

    // Enter raw mode but don't use alternate screen
    terminal::enable_raw_mode()?;

    // Print instructions (use \r\n because we're in raw mode)
    print!("Keyboard Event Monitor\r\n");
    print!("======================\r\n");
    print!("Press any key to see its code and modifiers.\r\n");
    print!("Press Esc twice quickly to exit.\r\n\r\n");
    stdout.flush()?;

    let mut detector = DoubleEscDetector::new(300); // 300ms threshold like default

    loop {
        // Poll with timeout to handle double-esc timing
        let timeout = detector.remaining_timeout();

        if !event::poll(timeout)? {
            // Timeout elapsed, check if we need to handle first Esc timeout
            if detector.timed_out() {
                detector.clear();
            }
            continue;
        }

        if let Event::Key(key) = event::read()? {
            // Check for double-esc
            match detector.process_key(&key) {
                EscResult::Double => {
                    // Exit on double-esc
                    break;
                }
                EscResult::First => {
                    // First esc, continue waiting
                }
                EscResult::None => {
                    // Not an esc event, clear detector
                }
            }

            // Print the key event details
            print_key_event(&key)?;
            stdout.flush()?;
        }
    }

    // Clean up (use \r\n because we're in raw mode)
    print!("\r\nExiting keyboard monitor...\r\n");
    terminal::disable_raw_mode()?;

    Ok(())
}

/// Format and print a key event with all its details
fn print_key_event(key: &crossterm::event::KeyEvent) -> std::io::Result<()> {
    use crossterm::event::{KeyCode, KeyModifiers};

    let mut parts = Vec::new();

    // Add modifiers
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("Ctrl");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        parts.push("Alt");
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        parts.push("Shift");
    }
    if key.modifiers.contains(KeyModifiers::SUPER) {
        parts.push("Super");
    }
    if key.modifiers.contains(KeyModifiers::HYPER) {
        parts.push("Hyper");
    }
    if key.modifiers.contains(KeyModifiers::META) {
        parts.push("Meta");
    }

    // Add key code
    let key_str = match key.code {
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PageUp".to_string(),
        KeyCode::PageDown => "PageDown".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => "BackTab".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::Insert => "Insert".to_string(),
        KeyCode::F(n) => format!("F{}", n),
        KeyCode::Char(c) => {
            if c == ' ' {
                "Space".to_string()
            } else if c.is_control() {
                format!("Char({:?})", c)
            } else {
                format!("'{}'", c)
            }
        }
        KeyCode::Null => "Null".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::CapsLock => "CapsLock".to_string(),
        KeyCode::ScrollLock => "ScrollLock".to_string(),
        KeyCode::NumLock => "NumLock".to_string(),
        KeyCode::PrintScreen => "PrintScreen".to_string(),
        KeyCode::Pause => "Pause".to_string(),
        KeyCode::Menu => "Menu".to_string(),
        KeyCode::KeypadBegin => "KeypadBegin".to_string(),
        KeyCode::Media(media) => format!("Media({:?})", media),
        KeyCode::Modifier(modifier) => format!("Modifier({:?})", modifier),
    };

    parts.push(&key_str);

    // Format output
    let combined = if parts.len() > 1 {
        parts.join("+")
    } else {
        parts[0].to_string()
    };

    // Use explicit \r\n because we're in raw mode
    print!("Key: {}\r\n", combined);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_terminal_emits_show_cursor_and_leave_alt() {
        // Use a memory buffer to capture escape sequences
        let mut buf: Vec<u8> = Vec::new();
        // It's safe to call even if raw mode isn't enabled
        restore_terminal(&mut buf).unwrap();
        let s = String::from_utf8_lossy(&buf);
        // Cursor show sequence (CSI ?25h) and leave alt screen (CSI ?1049l)
        assert!(s.contains("[?25h"), "expected cursor show sequence in output: {}", s);
        assert!(s.contains("[?1049l"), "expected leave alt-screen sequence in output: {}", s);
    }
}
