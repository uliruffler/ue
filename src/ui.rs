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
use crate::event_handlers::{handle_key_event, handle_mouse_event};
use crate::rendering::render_screen;
use crate::settings::Settings;
use crate::undo::UndoHistory;
use crate::double_esc::{DoubleEscDetector, EscResult};
use crate::syntax::SyntectHighlighter;

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
    if tracked.is_empty() { return Ok(SelectorResult::Cancelled); }
    let current_canon = std::fs::canonicalize(current_file).unwrap_or_else(|_| std::path::PathBuf::from(current_file));
    let current_str = current_canon.to_string_lossy();
    let mut selected_index = tracked.iter().position(|e| e.path.to_string_lossy() == current_str).unwrap_or(0);
    let scroll_offset = 0usize;
    let (_, th) = terminal::size()?; let vis = (th as usize).saturating_sub(1);
    execute!(stdout, Hide)?; crate::file_selector::render_file_list(&tracked, selected_index, scroll_offset, vis)?;

    // Double Esc detection state
    let mut last_esc_press: Option<Instant> = None;
    let esc_threshold = Duration::from_millis(settings.double_tap_speed_ms);

    loop {
        // If first Esc was pressed and timeout elapsed without second Esc -> cancel overlay
        if let Some(t0) = last_esc_press { if Instant::now().duration_since(t0) >= esc_threshold { execute!(stdout, Show)?; return Ok(SelectorResult::Cancelled); } }

        // Determine poll timeout (remaining time until cancellation or long wait)
        let timeout = if let Some(t0) = last_esc_press {
            let elapsed = Instant::now().duration_since(t0);
            esc_threshold.checked_sub(elapsed).unwrap_or(Duration::from_millis(0))
        } else { Duration::from_secs(86400) };

        if !event::poll(timeout)? {
            // Timeout fired -> treat as cancellation (handled above); just continue loop to trigger branch
            continue;
        }

        match event::read()? {
            Event::Key(k) => {
                if k.code == KeyCode::Esc && k.modifiers.is_empty() {
                    let now = Instant::now();
                    if let Some(prev) = last_esc_press { if now.duration_since(prev) <= esc_threshold { execute!(stdout, Show)?; return Ok(SelectorResult::Quit); } }
                    // First Esc: record and wait for second or timeout
                    last_esc_press = Some(now); continue; // do not cancel yet
                }
                // Any non-Esc key clears pending first Esc
                last_esc_press = None;
                match k.code {
                    KeyCode::Up | KeyCode::Char('k') => { if selected_index > 0 { let prev = selected_index; selected_index -= 1; crate::file_selector::render_selection_change(&tracked, prev, selected_index, scroll_offset, vis)?; } }
                    KeyCode::Down | KeyCode::Char('j') => { if selected_index + 1 < tracked.len() { let prev = selected_index; selected_index += 1; crate::file_selector::render_selection_change(&tracked, prev, selected_index, scroll_offset, vis)?; } }
                    KeyCode::Enter => { execute!(stdout, Show)?; return Ok(SelectorResult::Selected(tracked[selected_index].path.to_string_lossy().to_string())); }
                    _ => {}
                }
            }
            Event::Resize(_, h) => { *visible_lines = (h as usize).saturating_sub(2); let vis_new = (h as usize).saturating_sub(1); crate::file_selector::render_file_list(&tracked, selected_index, scroll_offset, vis_new)?; }
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
                        let mut visible_lines_temp = 20; // temporary, will be updated
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

fn editing_session(file: &str, content: String, settings: &Settings) -> crossterm::Result<(bool, Option<String>, bool, bool)> {
    let mut stdout = io::stdout();
    let undo_history = UndoHistory::load(file).unwrap_or_else(|_| UndoHistory::new());
    let mut lines: Vec<String> = if let Some(saved) = &undo_history.file_content { saved.clone() } else { content.lines().map(String::from).collect() };
    let (term_width, term_height) = size()?;
    let hl = Box::leak(Box::new(SyntectHighlighter::new()));
    let mut state = FileViewerState::new(term_width, undo_history.clone(), settings, hl);
    state.modified = state.undo_history.modified;
    state.top_line = undo_history.scroll_top.min(lines.len());
    let saved_cursor_line = undo_history.cursor_line;
    let saved_cursor_col = undo_history.cursor_col;
    if saved_cursor_line < lines.len() {
        if saved_cursor_line < state.top_line || saved_cursor_line >= state.top_line + (term_height as usize).saturating_sub(2) {
            state.top_line = saved_cursor_line.saturating_sub(5);
        }
        state.cursor_line = saved_cursor_line.saturating_sub(state.top_line);
        if saved_cursor_col <= lines[saved_cursor_line].len() { state.cursor_col = saved_cursor_col; }
    }
    let mut visible_lines = (term_height as usize).saturating_sub(2);
    state.needs_redraw = true;

    // Track last Esc press time for double-press detection
    let mut last_esc = DoubleEscDetector::new(settings.double_tap_speed_ms);

    loop {
        if state.needs_redraw { render_screen(&mut stdout, file, &lines, &state, visible_lines)?; state.needs_redraw = false; }
        
        // Check if we should open file selector after timeout
        if last_esc.timed_out() {
            last_esc.clear();
            // Persist before selector
            state.undo_history.update_cursor(state.top_line, state.absolute_line(), state.cursor_col);
            if let Err(e) = state.undo_history.save(file) { eprintln!("Warning: failed to save undo history: {}", e); }
            match run_file_selector_overlay(file, &mut visible_lines, settings)? {
                SelectorResult::Selected(selected_file) => { return Ok((state.modified, Some(selected_file), false, false)); }
                SelectorResult::Quit => { if let Err(e) = crate::session::save_selector_session() { eprintln!("Warning: failed to save selector session: {}", e); } return Ok((state.modified, None, true, false)); }
                SelectorResult::Cancelled => { state.needs_redraw = true; continue; }
            }
        }
        
        // Use poll with timeout to detect when to open file selector
        let timeout = last_esc.remaining_timeout();
        
        if !event::poll(timeout)? {
            if last_esc.timed_out() {
                last_esc.clear();
                // Persist before selector
                state.undo_history.update_cursor(state.top_line, state.absolute_line(), state.cursor_col);
                if let Err(e) = state.undo_history.save(file) { eprintln!("Warning: failed to save undo history: {}", e); }
                match run_file_selector_overlay(file, &mut visible_lines, settings)? {
                    SelectorResult::Selected(selected_file) => { return Ok((state.modified, Some(selected_file), false, false)); }
                    SelectorResult::Quit => { if let Err(e) = crate::session::save_selector_session() { eprintln!("Warning: failed to save selector session: {}", e); } return Ok((state.modified, None, true, false)); }
                    SelectorResult::Cancelled => { state.needs_redraw = true; continue; }
                }
            }
            continue;
        }
        
        match event::read()? {
            Event::Key(key_event) => {
                // Double-Esc processing
                match last_esc.process_key(&key_event) {
                    EscResult::Double => {
                        state.undo_history.update_cursor(state.top_line, state.absolute_line(), state.cursor_col);
                        if let Err(e) = state.undo_history.save(file) { eprintln!("Warning: failed to save undo history: {}", e); }
                        if let Err(e) = crate::session::save_editor_session(file) { eprintln!("Warning: failed to save editor session: {}", e); }
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
                let absolute_cursor_line = state.absolute_line(); let cursor_col = state.cursor_col; state.term_width = w; visible_lines = (h as usize).saturating_sub(2);
                let (new_top, rel_cursor) = adjust_view_for_resize(state.top_line, absolute_cursor_line, visible_lines, lines.len());
                state.top_line = new_top; state.cursor_line = rel_cursor; state.cursor_col = cursor_col; execute!(stdout, terminal::Clear(ClearType::All))?; state.needs_redraw = true;
            }
            Event::Mouse(mouse_event) => { handle_mouse_event(&mut state, &mut lines, mouse_event, visible_lines); }
            _ => {}
        }
    }
}
