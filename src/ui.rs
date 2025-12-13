use std::fs;
use std::io;
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
fn run_file_selector_overlay(
    current_file: &str,
    visible_lines: &mut usize,
    settings: &Settings,
) -> crossterm::Result<SelectorResult> {
    use crossterm::event::{Event, KeyCode, KeyModifiers};
    let mut stdout = io::stdout();
    let mut tracked = crate::file_selector::get_tracked_files().unwrap_or_default();
    if tracked.is_empty() {
        return Ok(SelectorResult::Cancelled);
    }

    let current_canon = std::fs::canonicalize(current_file)
        .unwrap_or_else(|_| std::path::PathBuf::from(current_file));
    let current_str = current_canon.to_string_lossy();
    let mut selected_index = tracked
        .iter()
        .position(|e| e.path.to_string_lossy() == current_str)
        .unwrap_or(0);
    let mut scroll_offset = 0usize;
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
                    KeyCode::Char('w') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                        // Close selected file (remove its tracked undo entry)
                        if let Some(entry) = tracked.get(selected_index) {
                            let _ = crate::file_selector::remove_tracked_file(&entry.path);
                            tracked.remove(selected_index);
                            // Adjust selection and scroll
                            if selected_index >= tracked.len() && selected_index > 0 {
                                selected_index -= 1;
                            }
                            if scroll_offset > 0 && scroll_offset + vis > tracked.len() {
                                scroll_offset = scroll_offset.saturating_sub(1);
                            }
                            // If all files are closed, exit overlay
                            if tracked.is_empty() {
                                execute!(stdout, Show)?;
                                return Ok(SelectorResult::Cancelled);
                            }
                            // Full redraw after removal
                            crate::file_selector::render_file_list(&tracked, selected_index, scroll_offset, vis)?;
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if selected_index > 0 {
                            let prev = selected_index;
                            selected_index -= 1;
                            crate::file_selector::render_selection_change(
                                &tracked,
                                prev,
                                selected_index,
                                scroll_offset,
                                vis,
                            )?;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if selected_index + 1 < tracked.len() {
                            let prev = selected_index;
                            selected_index += 1;
                            crate::file_selector::render_selection_change(
                                &tracked,
                                prev,
                                selected_index,
                                scroll_offset,
                                vis,
                            )?;
                        }
                    }
                    KeyCode::Enter => {
                        execute!(stdout, Show)?;
                        return Ok(SelectorResult::Selected(
                            tracked[selected_index].path.to_string_lossy().to_string(),
                        ));
                    }
                    _ => {}
                }
            }
            Event::Resize(_, h) => {
                *visible_lines = (h as usize).saturating_sub(2);
                let vis_new = (h as usize).saturating_sub(1);
                crate::file_selector::render_file_list(
                    &tracked,
                    selected_index,
                    scroll_offset,
                    vis_new,
                )?;
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

                    // Always show file selector after closing a file
                    // The closed file has already been removed from the tracked files list (undo history deleted)
                    // Exit alternate screen to show full file selector
                    execute!(
                        stdout,
                        SetCursorStyle::DefaultUserShape,
                        DisableMouseCapture,
                        LeaveAlternateScreen
                    )?;
                    terminal::disable_raw_mode()?;

                    // Show full file selector
                    match crate::file_selector::select_file()? {
                        Some(selected_file) => {
                            // Re-enter raw mode and alternate screen for editing
                            terminal::enable_raw_mode()?;
                            execute!(
                                stdout,
                                EnterAlternateScreen,
                                EnableMouseCapture,
                                SetCursorStyle::BlinkingBar,
                                terminal::Clear(ClearType::All)
                            )?;

                            // Find or add the selected file
                            if let Some(pos) =
                                current_files.iter().position(|f| f == &selected_file)
                            {
                                idx = pos;
                            } else {
                                current_files.insert(0, selected_file);
                                idx = 0;
                            }
                            continue;
                        }
                        None => {
                            // User quit from file selector
                            if let Err(e) = crate::session::save_selector_session() {
                                eprintln!("Warning: failed to save selector session: {}", e);
                            }
                            return Ok(()); // Exit editor
                        }
                    }
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
                    execute!(
                        stdout,
                        SetCursorStyle::DefaultUserShape,
                        DisableMouseCapture,
                        LeaveAlternateScreen
                    )?;
                    terminal::disable_raw_mode()?;
                    match crate::file_selector::select_file()? {
                        Some(selected_file) => {
                            terminal::enable_raw_mode()?;
                            execute!(
                                stdout,
                                EnterAlternateScreen,
                                EnableMouseCapture,
                                SetCursorStyle::BlinkingBar,
                                terminal::Clear(ClearType::All)
                            )?;
                            if let Some(pos) = current_files.iter().position(|f| f == &selected_file) {
                                idx = pos;
                            } else {
                                current_files.insert(0, selected_file);
                                idx = 0;
                            }
                            continue;
                        }
                        None => {
                            if let Err(e) = crate::session::save_selector_session() {
                                eprintln!("Warning: failed to save selector session: {}", e);
                            }
                            return Ok(());
                        }
                    }
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

    execute!(
        stdout,
        SetCursorStyle::DefaultUserShape,
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal::disable_raw_mode()?;
    if !unsaved.is_empty() {
        println!(
            "Warning: Unsaved changes (not saved) for: {}",
            unsaved.join(", ")
        );
    }

    // Note: Session is already saved in event_handlers.rs when quitting from editor
    // (via is_exit_command or save_and_quit handlers, or double-Esc in persist_editor_state).
    // Only save selector session if we explicitly switch to the selector or have no files.
    // If we reached here normally (quit=true from a single file), the editor session was already saved.

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
        SelectorResult::Selected(selected_file) => Ok((false, Some(selected_file), false, false)),
        SelectorResult::Quit => {
            if let Err(e) = crate::session::save_selector_session() {
                eprintln!("Warning: failed to save selector session: {}", e);
            }
            Ok((false, None, true, false))
        }
        SelectorResult::Cancelled => Ok((false, None, true, false)),
    }
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
                }
            }

            // Ensure cursor is visible after reload (similar to undo/redo)
            state.ensure_cursor_visible(visible_lines);
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

/// Helper to show file selector and handle result in event loop context
/// Returns Some((modified, next_file, quit, close)) to exit loop, or None to continue
fn handle_file_selector_in_loop(
    file: &str,
    state: &mut FileViewerState,
    visible_lines: &mut usize,
    settings: &Settings,
) -> crossterm::Result<FileSelectorResult> {
    // Persist state before showing selector
    state
        .undo_history
        .update_cursor(state.top_line, state.absolute_line(), state.cursor_col);
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

fn editing_session(
    file: &str,
    content: String,
    settings: &Settings,
) -> crossterm::Result<(bool, Option<String>, bool, bool)> {
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
        }
    }
    let mut visible_lines = (term_height as usize).saturating_sub(STATUS_LINE_HEIGHT);
    state.needs_redraw = true;

    // Track last Esc press time for double-press detection
    let mut last_esc = DoubleEscDetector::new(settings.double_tap_speed_ms);
    let mut esc_was_in_normal_mode = false; // Track if first Esc was in normal mode

    // File watching state for multi-instance synchronization
    let mut last_undo_check = Instant::now();
    let mut last_known_undo_mtime = UndoHistory::get_undo_file_mtime(file);

    loop {
        if state.needs_redraw {
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

        // Check if we should open file selector after timeout
        if last_esc.timed_out() {
            last_esc.clear();
            if let Some(result) =
                handle_file_selector_in_loop(file, &mut state, &mut visible_lines, settings)?
            {
                return Ok(result);
            }
            continue;
        }

        // Use poll with timeout to detect when to open file selector
        // Cap timeout to file check interval so we wake up regularly to check for external changes
        let esc_timeout = last_esc.remaining_timeout();
        let file_check_timeout = Duration::from_millis(UNDO_FILE_CHECK_INTERVAL_MS);
        let timeout = if esc_timeout < file_check_timeout {
            esc_timeout
        } else {
            file_check_timeout
        };

        if !event::poll(timeout)? {
            // Update cursor blink state for multi-cursors
            if state.update_cursor_blink() {
                state.needs_redraw = true;
            }

            if last_esc.timed_out() {
                last_esc.clear();
                // Only open file selector if the first Esc was in normal mode
                // (not in find or selection mode, which we already exited)
                if esc_was_in_normal_mode {
                    esc_was_in_normal_mode = false;
                    if let Some(result) = handle_file_selector_in_loop(
                        file,
                        &mut state,
                        &mut visible_lines,
                        settings,
                    )? {
                        return Ok(result);
                    }
                }
                // If not in normal mode, value is already false, no need to reassign
            }
            continue;
        }

        match event::read()? {
            Event::Key(key_event) => {
                // Special handling for help mode - process before double-Esc detector
                if state.help_active {
                    // In help mode, ESC should just exit help, not trigger file selector
                    if matches!(key_event.code, KeyCode::Esc)
                        || matches!(key_event.code, KeyCode::F(1))
                    {
                        state.help_active = false;
                        state.needs_redraw = true;
                        continue;
                    }
                    // Handle other help navigation through regular key handler
                    // which will return early for help mode
                }

                // Always process Esc through double-Esc detector (except when in help mode, handled above)
                match last_esc.process_key(&key_event) {
                    EscResult::Double => {
                        // Double-Esc always exits the editor, regardless of mode
                        persist_editor_state(&mut state, file);
                        return Ok((state.modified, None, true, false));
                    }
                    EscResult::First => {
                        // First Esc: exit find/goto/selection/multi-cursor mode if active, but continue waiting for second Esc
                        if state.find_active {
                            // Exit find mode
                            state.find_active = false;
                            state.find_pattern.clear();
                            state.find_error = None;
                            state.find_history_index = None;
                            state.last_search_pattern = state.saved_search_pattern.clone();
                            state.saved_search_pattern = None;
                            state.needs_redraw = true;
                            esc_was_in_normal_mode = false; // Was in find mode
                        } else if state.goto_line_active {
                            // Exit goto_line mode
                            state.goto_line_active = false;
                            state.goto_line_input.clear();
                            state.goto_line_cursor_pos = 0;
                            state.goto_line_typing_started = false;
                            state.needs_redraw = true;
                            esc_was_in_normal_mode = false; // Was in goto mode
                        } else if state.has_multi_cursors() {
                            // Clear multi-cursors
                            state.clear_multi_cursors();
                            state.needs_redraw = true;
                            esc_was_in_normal_mode = false; // Was in multi-cursor mode
                        } else if state.has_selection() {
                            // Clear selection
                            state.clear_selection();
                            state.needs_redraw = true;
                            esc_was_in_normal_mode = false; // Was in selection mode
                        } else {
                            // In normal mode - mark for file selector on timeout
                            esc_was_in_normal_mode = true;
                        }
                        continue; // Wait for second Esc or timeout
                    }
                    EscResult::None => {
                        // Not an Esc key - normal handling
                        esc_was_in_normal_mode = false; // Clear the flag
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
                execute!(stdout, terminal::Clear(ClearType::All))?;
                state.needs_redraw = true;
            }
            Event::Mouse(mouse_event) => {
                handle_mouse_event(&mut state, &mut lines, mouse_event, visible_lines);
            }
            _ => {}
        }
    }
}
