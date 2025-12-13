use crate::recent::get_recent_files;
use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// File entry with metadata
#[derive(Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub has_unsaved_changes: bool,
}

/// Get list of all files in ~/.ue/files/** (prefers UE_TEST_HOME for tests)
pub fn get_tracked_files() -> io::Result<Vec<FileEntry>> {
    let home = std::env::var("UE_TEST_HOME")
        .or_else(|_| std::env::var("HOME"))
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
    let ue_files_dir = PathBuf::from(home).join(".ue").join("files");
    if !ue_files_dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    collect_files_recursive(&ue_files_dir, &ue_files_dir, &mut files)?;

    // Load recent list and build ranking map (path -> index)
    let recent = get_recent_files().unwrap_or_default();
    use std::collections::HashMap;
    let mut rank: HashMap<String, usize> = HashMap::new();
    for (i, p) in recent.iter().enumerate() {
        rank.insert(p.to_string_lossy().to_string(), i);
    }

    // Sort: first by presence in recent (lower index means more recent), then alphabetically
    files.sort_by(|a, b| {
        let a_str = a.path.to_string_lossy();
        let b_str = b.path.to_string_lossy();
        match (rank.get(a_str.as_ref()), rank.get(b_str.as_ref())) {
            (Some(ra), Some(rb)) => ra.cmp(rb),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.path.cmp(&b.path),
        }
    });
    Ok(files)
}

/// Recursively collect all .undo.json files and derive original file paths
fn collect_files_recursive(
    base_dir: &PathBuf,
    current_dir: &PathBuf,
    files: &mut Vec<FileEntry>,
) -> io::Result<()> {
    if !current_dir.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(current_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_files_recursive(base_dir, &path, files)?;
        } else if let Some(file_name) = path.file_name() {
            let file_name_str = file_name.to_string_lossy();
            // Only include .*.ue files (hidden files ending with .ue)
            if file_name_str.starts_with('.') && file_name_str.ends_with(".ue") {
                // Compatibility: legacy format stored artificial leading dot for non-hidden files (.name.ext.ue)
                let without_suffix = file_name_str.trim_end_matches(".ue");
                let legacy_candidate = &without_suffix[1..]; // strip the first dot
                let is_hidden_original = {
                    // Heuristic: hidden original typically has no additional dot (e.g. .bashrc) OR begins with '.' followed by a config prefix like '.env'
                    let after_first = legacy_candidate;
                    !after_first.contains('.')
                };
                let original_filename = if is_hidden_original {
                    without_suffix
                } else {
                    legacy_candidate
                };
                let relative_dir = path
                    .parent()
                    .and_then(|p| p.strip_prefix(base_dir).ok())
                    .and_then(|p| p.to_str());
                if let Some(dir) = relative_dir {
                    let original_path = PathBuf::from("/").join(dir).join(original_filename);
                    let has_unsaved = check_unsaved_changes(&path);
                    files.push(FileEntry {
                        path: original_path,
                        has_unsaved_changes: has_unsaved,
                    });
                }
            } else if file_name_str.ends_with(".ue") {
                // Normal file: strip .ue
                let original_filename = file_name_str.trim_end_matches(".ue");
                let relative_dir = path
                    .parent()
                    .and_then(|p| p.strip_prefix(base_dir).ok())
                    .and_then(|p| p.to_str());
                if let Some(dir) = relative_dir {
                    let original_path = PathBuf::from("/").join(dir).join(original_filename);
                    let has_unsaved = check_unsaved_changes(&path);
                    files.push(FileEntry {
                        path: original_path,
                        has_unsaved_changes: has_unsaved,
                    });
                }
            }
        }
    }

    Ok(())
}

/// Check if a file has unsaved changes by reading its undo history
fn check_unsaved_changes(undo_file: &PathBuf) -> bool {
    // Try to read and deserialize the undo history file
    if let Ok(content) = fs::read_to_string(undo_file)
        && let Ok(history) = serde_json::from_str::<crate::undo::UndoHistory>(&content)
    {
        return history.modified;
    }
    false
}

/// Format file path as "FILENAME (/PATH/TO/DIRECTORY/)"
fn format_file_display(path: &Path) -> String {
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    let directory = path.parent().and_then(|p| p.to_str()).unwrap_or("/");

    format!(
        "{} ({}{})",
        filename,
        directory,
        if directory.ends_with('/') { "" } else { "/" }
    )
}

/// Show file selection screen and return selected file
pub(crate) fn select_file() -> io::Result<Option<String>> {
    let files = get_tracked_files()?;
    if files.is_empty() {
        eprintln!("No tracked files found in ~/.ue/files/");
        eprintln!("Please provide a filename as argument.");
        return Ok(None);
    }

    // Load settings for help keybindings
    let settings = crate::settings::Settings::load().unwrap_or_else(|_| Default::default());

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, Hide)?;
    let result = run_file_selector(&files, &settings);

    // Clean up based on result
    match &result {
        Ok(Some(_)) => {
            // File selected - clear screen but stay in alternate screen for smooth transition
            execute!(
                stdout,
                crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
            )?;
            // Don't show cursor or leave alternate screen - let editor handle it
        }
        _ => {
            // Cancelled - full cleanup
            execute!(stdout, Show, LeaveAlternateScreen)?;
            disable_raw_mode()?;
        }
    }

    result
}

pub fn remove_tracked_file(path: &Path) -> io::Result<()> {
    // Remove the tracked file by deleting its .ue undo history file
    // Uses the same mapping logic as get_tracked_files to locate undo file
    let home = std::env::var("UE_TEST_HOME")
        .or_else(|_| std::env::var("HOME"))
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
    let base_dir = PathBuf::from(home).join(".ue").join("files");

    // Compute relative path from root and construct undo path
    // The file structure mirrors absolute path under ~/.ue/files
    // We need to add only the directory part, not the filename
    let mut undo_dir = base_dir.clone();
    if let Some(parent) = path.parent() {
        for comp in parent.components() {
            use std::path::Component;
            match comp {
                Component::RootDir => {}
                _ => {
                    undo_dir.push(comp.as_os_str());
                }
            }
        }
    }
    // There are two variants: hidden originals (.name.ue) or normal (name.ue)
    // We try normal first, then hidden variant
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let mut undo_file = undo_dir.join(format!("{}.ue", filename));
    if !undo_file.exists() {
        // Hidden original mapping: .name -> .name.ue
        if filename.starts_with('.') {
            undo_file = undo_dir.join(format!("{}.ue", filename));
        } else {
            // Legacy mapping with artificial dot prefix: .name.ext.ue
            undo_file = undo_dir.join(format!(".{}.ue", filename));
        }
    }

    if undo_file.exists() {
        fs::remove_file(&undo_file)?;
    }
    Ok(())
}

fn run_file_selector(
    files: &[FileEntry],
    settings: &crate::settings::Settings,
) -> io::Result<Option<String>> {
    let mut selected_index = 0;
    let mut scroll_offset = 0;
    let mut prev_selected_index = 0;
    let mut prev_scroll_offset = 0;
    let mut needs_full_redraw = true;
    let mut help_active = false;
    let mut help_scroll_offset = 0;
    let mut current_files: Vec<FileEntry> = files.to_vec();

    loop {
        let (term_width, term_height) = crossterm::terminal::size()?;
        let visible_lines = (term_height as usize).saturating_sub(1); // only footer reserved

        let files_view = &current_files;
        if help_active {
            // Render help screen
            let help_content = crate::help::get_file_selector_help(settings, term_width as usize);
            crate::help::render_help(
                &mut io::stdout(),
                &help_content,
                help_scroll_offset,
                term_width,
                term_height,
            )?;
        } else if needs_full_redraw || scroll_offset != prev_scroll_offset {
            // Full redraw needed (first time or scrolling occurred)
            render_file_list(files_view, selected_index, scroll_offset, visible_lines)?;
            needs_full_redraw = false;
        } else if selected_index != prev_selected_index {
            // Only selection changed, redraw affected lines
            render_selection_change(
                files_view,
                prev_selected_index,
                selected_index,
                scroll_offset,
                visible_lines,
            )?;
        }

        prev_selected_index = selected_index;
        prev_scroll_offset = scroll_offset;

        if let Event::Key(key_event) = event::read()? {
            // F1 toggles help
            if matches!(key_event.code, KeyCode::F(1)) {
                help_active = !help_active;
                help_scroll_offset = 0;
                needs_full_redraw = true;
                continue;
            }

            // If in help mode, handle help navigation
            if help_active {
                if crate::help::handle_help_input(key_event) {
                    help_active = false;
                    needs_full_redraw = true;
                } else {
                    match key_event.code {
                        KeyCode::Up => {
                            help_scroll_offset = help_scroll_offset.saturating_sub(1);
                        }
                        KeyCode::Down => {
                            help_scroll_offset = help_scroll_offset.saturating_add(1);
                        }
                        KeyCode::PageUp => {
                            help_scroll_offset = help_scroll_offset.saturating_sub(visible_lines);
                        }
                        KeyCode::PageDown => {
                            help_scroll_offset = help_scroll_offset.saturating_add(visible_lines);
                        }
                        KeyCode::Home => {
                            help_scroll_offset = 0;
                        }
                        _ => {}
                    }
                }
                continue;
            }

            match key_event.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    let _ = crate::session::save_selector_session();
                    return Ok(None);
                }
                KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                    let _ = crate::session::save_selector_session();
                    return Ok(None);
                }
                KeyCode::Enter => {
                    if let Some(entry) = current_files.get(selected_index) {
                        return Ok(Some(entry.path.to_string_lossy().to_string()));
                    }
                }
                KeyCode::Char('w') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                    // Close selected file (remove its tracked undo entry)
                    if let Some(entry) = current_files.get(selected_index) {
                        let _ = remove_tracked_file(&entry.path);
                        current_files.remove(selected_index);
                        // Adjust selection and scroll
                        if selected_index >= current_files.len() && selected_index > 0 {
                            selected_index -= 1;
                        }
                        if scroll_offset > 0 && scroll_offset + visible_lines > current_files.len() {
                            scroll_offset = scroll_offset.saturating_sub(1);
                        }
                        needs_full_redraw = true;
                        // Persist selector session
                        let _ = crate::session::save_selector_session();
                        // If all files are closed, exit selector
                        if current_files.is_empty() {
                            return Ok(None);
                        }
                        continue;
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if selected_index > 0 {
                        selected_index -= 1;
                        if selected_index < scroll_offset {
                            scroll_offset = selected_index;
                        }
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if selected_index + 1 < current_files.len() {
                        selected_index += 1;
                        if selected_index >= scroll_offset + visible_lines {
                            scroll_offset = selected_index - visible_lines + 1;
                        }
                    }
                }
                KeyCode::PageUp => {
                    selected_index = selected_index.saturating_sub(visible_lines);
                    scroll_offset = scroll_offset.saturating_sub(visible_lines);
                }
                KeyCode::PageDown => {
                    selected_index =
                        (selected_index + visible_lines).min(current_files.len().saturating_sub(1));
                    scroll_offset += visible_lines;
                    if scroll_offset + visible_lines > current_files.len() {
                        scroll_offset = current_files.len().saturating_sub(visible_lines);
                    }
                }
                KeyCode::Home => {
                    selected_index = 0;
                    scroll_offset = 0;
                }
                KeyCode::Char('g') if !key_event.modifiers.contains(KeyModifiers::SHIFT) => {
                    selected_index = 0;
                    scroll_offset = 0;
                }
                KeyCode::End => {
                    selected_index = current_files.len().saturating_sub(1);
                    scroll_offset = current_files.len().saturating_sub(visible_lines);
                }
                KeyCode::Char('G') if key_event.modifiers.contains(KeyModifiers::SHIFT) => {
                    selected_index = current_files.len().saturating_sub(1);
                    scroll_offset = current_files.len().saturating_sub(visible_lines);
                }
                _ => {}
            }
        }
    }
}

pub(crate) fn render_file_list(
    files: &[FileEntry],
    selected_index: usize,
    scroll_offset: usize,
    visible_lines: usize,
) -> io::Result<()> {
    let mut stdout = io::stdout();
    execute!(
        stdout,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        crossterm::cursor::MoveTo(0, 0)
    )?;
    // Removed header and instructions: just list files
    let end_index = (scroll_offset + visible_lines).min(files.len());
    for (idx, entry) in files
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(end_index - scroll_offset)
    {
        let display_path = format_file_display(&entry.path);
        let unsaved_indicator = if entry.has_unsaved_changes { "*" } else { " " };
        if idx == selected_index {
            write!(
                stdout,
                "\r {} {}{}{}\r\n",
                unsaved_indicator,
                crossterm::style::Attribute::Reverse,
                display_path,
                crossterm::style::Attribute::Reset
            )?;
        } else {
            write!(stdout, "\r {} {}\r\n", unsaved_indicator, display_path)?;
        }
    }
    // Always show position indicator footer
    let (_, term_height) = crossterm::terminal::size()?;
    execute!(stdout, crossterm::cursor::MoveTo(0, term_height - 1))?;
    write!(stdout, "\r  [{}/{}]", selected_index + 1, files.len())?;
    stdout.flush()?;
    Ok(())
}

pub(crate) fn render_selection_change(
    files: &[FileEntry],
    prev_index: usize,
    new_index: usize,
    scroll_offset: usize,
    visible_lines: usize,
) -> io::Result<()> {
    let mut stdout = io::stdout();

    // Redraw previous selection (if visible)
    if prev_index >= scroll_offset && prev_index < scroll_offset + visible_lines {
        let screen_row = prev_index - scroll_offset;
        if let Some(entry) = files.get(prev_index) {
            execute!(stdout, crossterm::cursor::MoveTo(0, screen_row as u16))?;
            let display_path = format_file_display(&entry.path);
            let unsaved_indicator = if entry.has_unsaved_changes { "*" } else { " " };
            write!(stdout, "\r {} {}", unsaved_indicator, display_path)?;
            // Clear to end of line to remove any old reverse video
            execute!(
                stdout,
                crossterm::terminal::Clear(crossterm::terminal::ClearType::UntilNewLine)
            )?;
        }
    }

    // Redraw new selection (if visible)
    if new_index >= scroll_offset && new_index < scroll_offset + visible_lines {
        let screen_row = new_index - scroll_offset;
        if let Some(entry) = files.get(new_index) {
            execute!(stdout, crossterm::cursor::MoveTo(0, screen_row as u16))?;
            let display_path = format_file_display(&entry.path);
            let unsaved_indicator = if entry.has_unsaved_changes { "*" } else { " " };
            write!(
                stdout,
                "\r {} {}{}{}",
                unsaved_indicator,
                crossterm::style::Attribute::Reverse,
                display_path,
                crossterm::style::Attribute::Reset
            )?;
            execute!(
                stdout,
                crossterm::terminal::Clear(crossterm::terminal::ClearType::UntilNewLine)
            )?;
        }
    }

    // Update footer
    let (_, term_height) = crossterm::terminal::size()?;
    execute!(stdout, crossterm::cursor::MoveTo(0, term_height - 1))?;
    write!(stdout, "\r  [{}/{}]", new_index + 1, files.len())?;
    execute!(
        stdout,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::UntilNewLine)
    )?;

    stdout.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::set_temp_home;
    use crate::recent::update_recent_file;
    use crate::undo::UndoHistory;
    use std::fs;

    #[test]
    fn ed_test_home_precedence_over_home() {
        let (tmp, _guard) = set_temp_home();
        let ue_dir = tmp.path().join(".ue").join("files").join("a");
        fs::create_dir_all(&ue_dir).unwrap();
        fs::write(ue_dir.join("one.txt.ue"), "{}").unwrap();
        let files = get_tracked_files().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, PathBuf::from("/a/one.txt"));
    }

    #[test]
    fn hidden_original_filename_round_trip() {
        let (tmp, _guard) = set_temp_home();
        let ue_dir = tmp.path().join(".ue").join("files").join("home");
        fs::create_dir_all(&ue_dir).unwrap();
        // Hidden original file .bashrc -> undo file .bashrc.ue
        fs::write(ue_dir.join(".bashrc.ue"), "{}").unwrap();
        let files = get_tracked_files().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, PathBuf::from("/home/.bashrc"));
    }

    #[test]
    fn duplicate_filenames_in_different_directories() {
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("files");
        let d1 = base.join("proj1");
        let d2 = base.join("proj2");
        fs::create_dir_all(&d1).unwrap();
        fs::create_dir_all(&d2).unwrap();
        fs::write(d1.join("config.toml.ue"), "{}").unwrap();
        fs::write(d2.join("config.toml.ue"), "{}").unwrap();
        let mut files = get_tracked_files().unwrap();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, PathBuf::from("/proj1/config.toml"));
        assert_eq!(files[1].path, PathBuf::from("/proj2/config.toml"));
    }

    #[test]
    fn get_tracked_files_returns_empty_when_no_files_dir() {
        let (_tmp, _guard) = set_temp_home();
        let result = get_tracked_files();
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 0);
    }

    #[test]
    fn get_tracked_files_finds_ed_files() {
        let (tmp, _guard) = set_temp_home();
        let ue_dir = tmp
            .path()
            .join(".ue")
            .join("files")
            .join("home")
            .join("user");
        fs::create_dir_all(&ue_dir).unwrap();
        fs::write(ue_dir.join("test.txt.ue"), "{}").unwrap();
        let result = get_tracked_files();
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, PathBuf::from("/home/user/test.txt"));
    }

    #[test]
    fn get_tracked_files_ignores_non_ed_files() {
        let (tmp, _guard) = set_temp_home();
        let ue_dir = tmp
            .path()
            .join(".ue")
            .join("files")
            .join("home")
            .join("user");
        fs::create_dir_all(&ue_dir).unwrap();

        // Valid undo history files (new naming): with and without original extension
        fs::write(ue_dir.join("test.txt.ue"), "{}").unwrap(); // original: test.txt
        fs::write(ue_dir.join("test.ue"), "{}").unwrap(); // original: test (no extension)

        // Should be ignored:
        fs::write(ue_dir.join("test.txt"), "{}").unwrap(); // missing .ue suffix
        fs::write(ue_dir.join(".test.txt"), "{}").unwrap(); // legacy artificial dot but missing .ue suffix
        fs::write(ue_dir.join("random.bin"), "{}").unwrap(); // unrelated file

        let result = get_tracked_files();
        assert!(result.is_ok());
        let mut files = result.unwrap();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, PathBuf::from("/home/user/test"));
        assert_eq!(files[1].path, PathBuf::from("/home/user/test.txt"));
    }

    #[test]
    fn get_tracked_files_handles_nested_directories() {
        let (tmp, _guard) = set_temp_home();
        let base_dir = tmp.path().join(".ue").join("files");
        let dir1 = base_dir.join("home").join("user").join("documents");
        let dir2 = base_dir.join("tmp");
        fs::create_dir_all(&dir1).unwrap();
        fs::create_dir_all(&dir2).unwrap();
        fs::write(dir1.join("notes.txt.ue"), "{}").unwrap();
        fs::write(dir2.join("test.sh.ue"), "{}").unwrap();
        let result = get_tracked_files();
        assert!(result.is_ok());
        let mut files = result.unwrap();
        assert_eq!(files.len(), 2);
        files.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(
            files[0].path,
            PathBuf::from("/home/user/documents/notes.txt")
        );
        assert_eq!(files[1].path, PathBuf::from("/tmp/test.sh"));
    }

    #[test]
    fn get_tracked_files_detects_unsaved_changes() {
        let (tmp, _guard) = set_temp_home();
        let ue_dir = tmp
            .path()
            .join(".ue")
            .join("files")
            .join("home")
            .join("user");
        fs::create_dir_all(&ue_dir).unwrap();
        let mut history = UndoHistory::new();
        history.modified = true;
        fs::write(
            ue_dir.join("modified.txt.ue"),
            serde_json::to_string(&history).unwrap(),
        )
        .unwrap();
        let mut history2 = UndoHistory::new();
        history2.modified = false;
        fs::write(
            ue_dir.join("saved.txt.ue"),
            serde_json::to_string(&history2).unwrap(),
        )
        .unwrap();
        let result = get_tracked_files();
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 2);
        let modified_file = files
            .iter()
            .find(|f| f.path.file_name().unwrap() == "modified.txt")
            .unwrap();
        assert!(modified_file.has_unsaved_changes);
        let saved_file = files
            .iter()
            .find(|f| f.path.file_name().unwrap() == "saved.txt")
            .unwrap();
        assert!(!saved_file.has_unsaved_changes);
    }

    #[test]
    fn check_unsaved_changes_returns_false_for_invalid_json() {
        let (tmp, _guard) = set_temp_home();
        let test_file = tmp.path().join("invalid.json");
        fs::write(&test_file, "not valid json").unwrap();

        let result = check_unsaved_changes(&test_file);
        assert!(!result);
    }

    #[test]
    fn check_unsaved_changes_returns_false_for_nonexistent_file() {
        let (_tmp, _guard) = set_temp_home();
        let test_file = PathBuf::from("/nonexistent/file.ue");
        let result = check_unsaved_changes(&test_file);
        assert!(!result);
    }

    #[test]
    fn format_file_display_shows_filename_and_directory() {
        let path = PathBuf::from("/home/user/documents/notes.txt");
        let result = format_file_display(&path);
        assert_eq!(result, "notes.txt (/home/user/documents/)");
    }

    #[test]
    fn format_file_display_handles_root_directory() {
        let path = PathBuf::from("/test.txt");
        let result = format_file_display(&path);
        assert_eq!(result, "test.txt (/)");
    }

    #[test]
    fn format_file_display_adds_trailing_slash() {
        let path = PathBuf::from("/tmp/file.sh");
        let result = format_file_display(&path);
        assert!(result.contains("(/tmp/)"));
    }

    #[test]
    fn collect_files_recursive_skips_directories() {
        let (tmp, _guard) = set_temp_home();
        let base_dir = tmp.path().join(".ue").join("files");
        let test_dir = base_dir.join("home").join("user");
        fs::create_dir_all(&test_dir).unwrap();
        let sub_dir = test_dir.join("documents");
        fs::create_dir_all(&sub_dir).unwrap();
        fs::write(sub_dir.join("test.md.ue"), "{}").unwrap();
        let mut files = Vec::new();
        let result = collect_files_recursive(&base_dir, &base_dir, &mut files);
        assert!(result.is_ok());
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, PathBuf::from("/home/user/documents/test.md"));
    }

    #[test]
    fn file_entry_preserves_full_path() {
        let (tmp, _guard) = set_temp_home();
        let ue_dir = tmp
            .path()
            .join(".ue")
            .join("files")
            .join("etc")
            .join("config");
        fs::create_dir_all(&ue_dir).unwrap();
        fs::write(ue_dir.join("app.conf.ue"), "{}").unwrap();
        let result = get_tracked_files();
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, PathBuf::from("/etc/config/app.conf"));
    }

    #[test]
    fn get_tracked_files_sorts_alphabetically() {
        let (tmp, _guard) = set_temp_home();
        let base_dir = tmp.path().join(".ue").join("files").join("test");
        fs::create_dir_all(&base_dir).unwrap();
        fs::write(base_dir.join("zebra.txt.ue"), "{}").unwrap();
        fs::write(base_dir.join("apple.txt.ue"), "{}").unwrap();
        fs::write(base_dir.join("middle.txt.ue"), "{}").unwrap();
        let result = get_tracked_files();
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 3);
        assert!(files[0].path < files[1].path);
        assert!(files[1].path < files[2].path);
    }

    #[test]
    fn handles_files_with_multiple_dots() {
        let (tmp, _guard) = set_temp_home();
        let ue_dir = tmp.path().join(".ue").join("files").join("home");
        fs::create_dir_all(&ue_dir).unwrap();

        // File with multiple dots in name
        fs::write(ue_dir.join("my.config.yaml.ue"), "{}").unwrap();

        let result = get_tracked_files();
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, PathBuf::from("/home/my.config.yaml"));
    }

    #[test]
    fn empty_directory_structure_returns_empty_list() {
        let (tmp, _guard) = set_temp_home();
        let ue_dir = tmp.path().join(".ue").join("files").join("empty");
        fs::create_dir_all(&ue_dir).unwrap();

        let result = get_tracked_files();
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 0);
    }

    #[test]
    fn legacy_leading_dot_non_hidden_filename() {
        let (tmp, _guard) = set_temp_home();
        let ue_dir = tmp
            .path()
            .join(".ue")
            .join("files")
            .join("home")
            .join("user");
        fs::create_dir_all(&ue_dir).unwrap();
        fs::write(ue_dir.join(".legacy.txt.ue"), "{}").unwrap();
        let files = get_tracked_files().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, PathBuf::from("/home/user/legacy.txt"));
    }

    #[test]
    fn tracked_files_sorted_by_recent_first() {
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path().join(".ue").join("files").join("proj");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("a.txt.ue"), "{}").unwrap();
        std::fs::write(base.join("b.txt.ue"), "{}").unwrap();
        std::fs::write(base.join("c.txt.ue"), "{}").unwrap();
        // Simulate user opening /proj/b.txt then /proj/a.txt (original file paths)
        update_recent_file("/proj/b.txt").unwrap();
        update_recent_file("/proj/a.txt").unwrap();
        let files = get_tracked_files().unwrap();
        assert_eq!(files.len(), 3);
        // a is most recent, should appear before b; c has no recent entry comes last alphabetically
        assert_eq!(files[0].path.file_name().unwrap(), "a.txt");
        assert_eq!(files[1].path.file_name().unwrap(), "b.txt");
        assert_eq!(files[2].path.file_name().unwrap(), "c.txt");
    }

    #[test]
    fn remove_tracked_file_deletes_undo_history() {
        let (tmp, _guard) = set_temp_home();
        let ue_files = tmp.path().join(".ue").join("files").join("test");
        std::fs::create_dir_all(&ue_files).unwrap();

        // Create tracked files
        std::fs::write(ue_files.join("file1.txt.ue"), "{}").unwrap();
        std::fs::write(ue_files.join("file2.txt.ue"), "{}").unwrap();

        let files = get_tracked_files().unwrap();
        assert_eq!(files.len(), 2);

        // Close file1
        let file1_path = PathBuf::from("/test/file1.txt");
        remove_tracked_file(&file1_path).unwrap();

        // Verify file1 is gone
        let files_after = get_tracked_files().unwrap();
        assert_eq!(files_after.len(), 1);
        assert!(!files_after.iter().any(|f| f.path == file1_path));
        assert!(files_after
            .iter()
            .any(|f| f.path == PathBuf::from("/test/file2.txt")));
    }

    #[test]
    fn remove_tracked_file_handles_nonexistent() {
        let (_tmp, _guard) = set_temp_home();
        let nonexistent = PathBuf::from("/nonexistent/file.txt");

        // Should not error when trying to remove non-existent file
        let result = remove_tracked_file(&nonexistent);
        assert!(result.is_ok());
    }
}
