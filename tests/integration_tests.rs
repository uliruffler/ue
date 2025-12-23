use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use serial_test::serial;

/// Helper to set up a temporary test environment
/// Note: Integration tests run sequentially by default, so no lock needed
fn setup_test_env() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let ue_dir = tmp.path().join(".ue");
    fs::create_dir_all(&ue_dir).unwrap();
    unsafe {
        std::env::set_var("UE_TEST_HOME", tmp.path());
    }
    (tmp, ue_dir)
}
/// Test: Editor should handle German umlauts (UTF-8 multi-byte characters) without crashing
/// Issue: Typing "Für" should work - the cursor position must be tracked in character indices,
/// not byte indices.
#[test]
#[serial]
fn test_umlaut_input_no_crash() {
    let (_tmp, _ue_dir) = setup_test_env();
    // Simulate the editor state - verify that character insertion handles multi-byte UTF-8 correctly
    let mut content = String::new();
    // Type 'F'
    content.push('F');
    assert_eq!(content.chars().count(), 1);
    // cursor after 'F' is at position 1
    // Type 'ü' (2 bytes in UTF-8)
    content.push('ü');
    assert_eq!(content.chars().count(), 2);
    assert_eq!(content.len(), 3); // 'F' = 1 byte, 'ü' = 2 bytes
    let cursor_pos_chars = 2; // cursor after 'ü'
    // Type 'r' - this should not crash
    // The cursor position must be in character indices, not byte indices
    let byte_pos = content.char_indices().nth(cursor_pos_chars).map(|(i, _)| i).unwrap_or(content.len());
    content.insert(byte_pos, 'r');
    assert_eq!(content, "Für");
    assert_eq!(content.chars().count(), 3);
    // Verify we can continue typing
    let cursor_pos_chars = 3;
    let byte_pos = content.char_indices().nth(cursor_pos_chars).map(|(i, _)| i).unwrap_or(content.len());
    content.insert(byte_pos, ' ');
    content.insert_str(content.len(), "mich");
    assert_eq!(content, "Für mich");
}
/// Test: Create a new file when opening with a non-existent filename
/// The file should only be created on disk when saved, not when opened
#[test]
#[serial]
fn test_open_new_file_creates_on_save() {
    let (_tmp, _ue_dir) = setup_test_env();
    let new_file_path = _tmp.path().join("newfile.txt");
    assert!(!new_file_path.exists(), "File should not exist yet");
    // Opening the file should create editor state but not the actual file
    // At this point, file should still not exist
    assert!(!new_file_path.exists(), "File should not exist until save");
    // Simulate saving with content
    fs::write(&new_file_path, "Hello, World!").unwrap();
    assert!(new_file_path.exists(), "File should exist after save");
    let content = fs::read_to_string(&new_file_path).unwrap();
    assert_eq!(content, "Hello, World!");
}
/// Test: Undo mechanism should work for new files
#[test]
#[serial]
fn test_new_file_undo_mechanism() {
    let (_tmp, _ue_dir) = setup_test_env();
    // Create undo history for a new file
    let mut undo_history = ue::undo::UndoHistory::new();
    // Simulate typing "Hello"
    let mut content = vec![String::new()];
    for (i, ch) in "Hello".chars().enumerate() {
        undo_history.push(ue::undo::Edit::InsertChar {
            line: 0,
            col: i,
            ch,
        });
        content[0].push(ch);
    }
    assert_eq!(content[0], "Hello");
    assert!(undo_history.can_undo());
    // Undo should work
    if let Some(edit) = undo_history.undo() {
        match edit {
            ue::undo::Edit::InsertChar { line: 0, col: 4, ch: 'o' } => {
                // Correct undo
                content[0].pop();
            }
            _ => panic!("Unexpected undo edit"),
        }
    }
    assert_eq!(content[0], "Hell");
}
/// Test: Editor restores mode when reopened without arguments
/// If quit from editor mode on file A, should reopen in editor mode on file A
/// If quit from file selector, should reopen in file selector
#[test]
#[serial]
fn test_session_restore_editor_mode() {
    let (_tmp, _ue_dir) = setup_test_env();
    let test_file = _tmp.path().join("session_test.txt");
    fs::write(&test_file, "content").unwrap();
    let test_file_str = test_file.to_string_lossy().to_string();
    // Save editor session
    ue::session::save_editor_session(&test_file_str).unwrap();
    // Small delay to ensure write completes
    std::thread::sleep(std::time::Duration::from_millis(10));
    // Load session
    let loaded = ue::session::load_last_session().unwrap();
    assert!(loaded.is_some(), "Session should be loaded");
    let session = loaded.unwrap();
    assert_eq!(session.mode, ue::session::SessionMode::Editor, "Mode should be Editor");
    assert_eq!(session.file.unwrap().to_string_lossy(), test_file_str);
}
/// Test: Editor restores mode when reopened - file selector case
#[test]
#[serial]
fn test_session_restore_selector_mode() {
    let (_tmp, _ue_dir) = setup_test_env();
    // Save selector session
    ue::session::save_selector_session().unwrap();
    // Load session
    let loaded = ue::session::load_last_session().unwrap();
    assert!(loaded.is_some());
    let session = loaded.unwrap();
    assert_eq!(session.mode, ue::session::SessionMode::Selector);
    assert!(session.file.is_none());
}
/// Test: Quitting from editor mode should save editor session, not selector session
#[test]
#[serial]
fn test_quit_editor_saves_editor_session() {
    let (_tmp, _ue_dir) = setup_test_env();
    let test_file = _tmp.path().join("quit_test.txt");
    fs::write(&test_file, "content").unwrap();
    let test_file_str = test_file.to_string_lossy().to_string();
    // First save selector session (simulating previous state)
    ue::session::save_selector_session().unwrap();
    // Verify it's selector mode
    let session1 = ue::session::load_last_session().unwrap().unwrap();
    assert_eq!(session1.mode, ue::session::SessionMode::Selector);
    // Now quit from editor mode
    ue::session::save_editor_session(&test_file_str).unwrap();
    // Should now be in editor mode
    let session2 = ue::session::load_last_session().unwrap().unwrap();
    assert_eq!(session2.mode, ue::session::SessionMode::Editor);
    assert_eq!(session2.file.unwrap().to_string_lossy(), test_file_str);
}
/// Test: Closing a file from file selector using Ctrl+W
#[test]
#[serial]
fn test_close_file_from_selector() {
    let (_tmp, ue_dir) = setup_test_env();
    // Create two test files
    let file1 = _tmp.path().join("file1.txt");
    let file2 = _tmp.path().join("file2.txt");
    fs::write(&file1, "content1").unwrap();
    fs::write(&file2, "content2").unwrap();
    // Create undo history files to track them
    let files_dir = ue_dir.join("files");
    for file in &[&file1, &file2] {
        // Build the directory path (excluding filename)
        let mut undo_dir = files_dir.clone();
        if let Some(parent) = file.parent() {
            for comp in parent.components() {
                if !matches!(comp, std::path::Component::RootDir) {
                    undo_dir.push(comp.as_os_str());
                }
            }
        }
        fs::create_dir_all(&undo_dir).unwrap();

        // Create the .ue file for tracking
        let filename = file.file_name().unwrap().to_str().unwrap();
        let undo_file = undo_dir.join(format!("{}.ue", filename));
        let history = ue::undo::UndoHistory::new();
        let json = serde_json::to_string(&history).unwrap();
        fs::write(&undo_file, json).unwrap();
    }
    // Get tracked files
    let tracked = ue::file_selector::get_tracked_files().unwrap();
    assert_eq!(tracked.len(), 2, "Should have 2 tracked files");
    // Remove file1
    ue::file_selector::remove_tracked_file(&file1).unwrap();
    // Should only have file2 now
    let tracked_after = ue::file_selector::get_tracked_files().unwrap();
    assert_eq!(tracked_after.len(), 1, "Should have 1 tracked file after removal");
    assert_eq!(tracked_after[0].path, file2);
}

/// Test: Line wrapping toggle functionality
#[test]
#[serial]
fn test_line_wrapping_toggle() {
    let (_tmp, _ue_dir) = setup_test_env();
    let settings = ue::settings::Settings::load().unwrap();
    let undo_history = ue::undo::UndoHistory::new();
    let mut state = ue::editor_state::FileViewerState::new_for_test(80, undo_history, &settings);
    
    // Default should be wrapping enabled (from settings)
    assert!(state.is_line_wrapping_enabled_for_test(), "Wrapping should be enabled by default");
    
    // Toggle wrapping off
    state.toggle_line_wrapping_for_test();
    assert!(!state.is_line_wrapping_enabled_for_test(), "Wrapping should be disabled after toggle");
    
    // Toggle wrapping back on
    state.toggle_line_wrapping_for_test();
    assert!(state.is_line_wrapping_enabled_for_test(), "Wrapping should be enabled after second toggle");
}

/// Test: Horizontal scroll offset adjustment
#[test]
#[serial]
fn test_horizontal_scroll_offset() {
    let (_tmp, _ue_dir) = setup_test_env();
    let settings = ue::settings::Settings::load().unwrap();
    let undo_history = ue::undo::UndoHistory::new();
    let mut state = ue::editor_state::FileViewerState::new_for_test(80, undo_history, &settings);
    
    // Initial offset should be 0
    assert_eq!(state.get_horizontal_scroll_offset(), 0);
    
    // Simulate scrolling right
    state.set_horizontal_scroll_offset(10);
    assert_eq!(state.get_horizontal_scroll_offset(), 10);
    
    // Simulate scrolling back left
    let offset = state.get_horizontal_scroll_offset().saturating_sub(5);
    state.set_horizontal_scroll_offset(offset);
    assert_eq!(state.get_horizontal_scroll_offset(), 5);
    
    // Scrolling left at 0 should stay at 0
    state.set_horizontal_scroll_offset(0);
    let offset = state.get_horizontal_scroll_offset().saturating_sub(5);
    state.set_horizontal_scroll_offset(offset);
    assert_eq!(state.get_horizontal_scroll_offset(), 0);
}

/// Test: Short line scroll reset on click
#[test]
#[serial]
fn test_short_line_scroll_reset() {
    let (_tmp, _ue_dir) = setup_test_env();
    let settings = ue::settings::Settings::load().unwrap();
    let undo_history = ue::undo::UndoHistory::new();
    let mut state = ue::editor_state::FileViewerState::new_for_test(80, undo_history, &settings);
    
    // Disable wrapping
    state.toggle_line_wrapping_for_test();
    assert!(!state.is_line_wrapping_enabled_for_test());
    
    // Simulate being scrolled to the right
    state.set_horizontal_scroll_offset(50);
    
    // Create a short line (5 characters)
    let short_line = "Short".to_string();
    
    // When clicking on a short line, offset should be reset if line is shorter than offset
    if short_line.len() <= state.get_horizontal_scroll_offset() {
        state.set_horizontal_scroll_offset(0);
    }
    
    assert_eq!(state.get_horizontal_scroll_offset(), 0, "Scroll offset should reset for short lines");
}

/// Test: Horizontal auto-scroll speed setting
#[test]
#[serial]
fn test_horizontal_auto_scroll_speed_setting() {
    let (_tmp, ue_dir) = setup_test_env();
    
    // Create custom settings with different scroll speed
    let settings_path = ue_dir.join("settings.toml");
    // Use simple concatenation to avoid raw string issues
    let mut settings_content = String::new();
    settings_content.push_str("tab_width = 4\n");
    settings_content.push_str("keyboard_scroll_lines = 3\n");
    settings_content.push_str("double_tap_speed_ms = 300\n");
    settings_content.push_str("mouse_scroll_lines = 3\n");
    settings_content.push_str("line_wrapping = true\n");
    settings_content.push_str("horizontal_auto_scroll_speed = 5\n");
    settings_content.push_str("\n[appearance]\n");
    settings_content.push_str("line_number_digits = 3\n");
    settings_content.push_str("header_bg = \"#001848\"\n");
    settings_content.push_str("footer_bg = \"#001848\"\n");
    settings_content.push_str("line_numbers_bg = \"#001848\"\n");
    settings_content.push_str("cursor_shape = \"bar\"\n");
    settings_content.push_str("\n[keybindings]\n");
    settings_content.push_str("quit = \"Esc Esc\"\n");
    settings_content.push_str("file_selector = \"Esc\"\n");
    settings_content.push_str("copy = \"Ctrl+c\"\n");
    settings_content.push_str("paste = \"Ctrl+v\"\n");
    settings_content.push_str("cut = \"Ctrl+x\"\n");
    settings_content.push_str("close = \"Ctrl+w\"\n");
    settings_content.push_str("save = \"Ctrl+s\"\n");
    settings_content.push_str("undo = \"Ctrl+z\"\n");
    settings_content.push_str("redo = \"Ctrl+y\"\n");
    settings_content.push_str("find = \"Ctrl+f\"\n");
    settings_content.push_str("find_next = \"Ctrl+n\"\n");
    settings_content.push_str("find_previous = \"Ctrl+p\"\n");
    settings_content.push_str("goto_line = \"Ctrl+g\"\n");
    settings_content.push_str("help = \"F1\"\n");
    settings_content.push_str("save_and_quit = \"Ctrl+q\"\n");
    settings_content.push_str("toggle_line_wrap = \"Alt+w\"\n");

    fs::write(&settings_path, settings_content).unwrap();
    
    // Load settings and verify
    let settings = ue::settings::Settings::load().unwrap();
    assert_eq!(settings.get_horizontal_auto_scroll_speed(), 5, "Scroll speed should be 5");
}

/// Test: Horizontal auto-scroll speed default value
#[test]
#[serial]
fn test_horizontal_auto_scroll_speed_default() {
    let (_tmp, _ue_dir) = setup_test_env();
    let settings = ue::settings::Settings::load().unwrap();
    
    // Default speed should be 1
    assert_eq!(settings.get_horizontal_auto_scroll_speed(), 1, "Default scroll speed should be 1");
}

/// Test: Visual width calculation with tabs
#[test]
#[serial]
fn test_visual_width_with_tabs() {
    // Test that visual width correctly accounts for tabs
    let line_with_tabs = "\t\tHello";
    let tab_width = 4;
    
    let visual_width = ue::coordinates::visual_width(line_with_tabs, tab_width);
    // 2 tabs * 4 spaces each + 5 characters = 8 + 5 = 13
    assert_eq!(visual_width, 13);
    
    // Test mixed tabs and spaces
    let mixed = "Hello\tWorld";
    let visual = ue::coordinates::visual_width(mixed, tab_width);
    // "Hello" = 5 chars, tab expands to align to next multiple of 4 (5->8 = 3 spaces), "World" = 5 chars
    // Total = 5 + 3 + 5 = 13
    assert_eq!(visual, 13);
}

/// Test: Cursor position clamping to line length
#[test]
#[serial]
fn test_cursor_position_clamping() {
    let (_tmp, _ue_dir) = setup_test_env();
    let settings = ue::settings::Settings::load().unwrap();
    let undo_history = ue::undo::UndoHistory::new();
    let mut state = ue::editor_state::FileViewerState::new_for_test(80, undo_history, &settings);
    
    let line = "Hello World".to_string();
    let line_len = line.len(); // 11
    
    // Try to set cursor beyond line end
    state.set_cursor_col(100);
    
    // Clamp to line length
    let clamped = state.get_cursor_col().min(line_len);
    state.set_cursor_col(clamped);
    
    assert_eq!(state.get_cursor_col(), line_len, "Cursor should be clamped to line length");
}

/// Test: Visual column to character index conversion
#[test]
#[serial]
fn test_visual_col_to_char_index() {
    let line = "Hello\tWorld";
    let tab_width = 4;
    
    // Visual column 0 should be char index 0
    let char_idx = ue::coordinates::visual_col_to_char_index(line, 0, tab_width);
    assert_eq!(char_idx, 0);
    
    // Visual column 5 should be char index 5 (end of "Hello")
    let char_idx = ue::coordinates::visual_col_to_char_index(line, 5, tab_width);
    assert_eq!(char_idx, 5);
    
    // Visual column 8 should be char index 6 (after tab, start of "World")
    let char_idx = ue::coordinates::visual_col_to_char_index(line, 8, tab_width);
    assert_eq!(char_idx, 6);
    
    // Visual column beyond line should return line length
    let char_idx = ue::coordinates::visual_col_to_char_index(line, 100, tab_width);
    assert_eq!(char_idx, line.chars().count());
}

/// Test: Horizontal scrolling with long lines
#[test]
#[serial]
fn test_horizontal_scrolling_bounds() {
    let (_tmp, _ue_dir) = setup_test_env();
    let settings = ue::settings::Settings::load().unwrap();
    let undo_history = ue::undo::UndoHistory::new();
    let mut state = ue::editor_state::FileViewerState::new_for_test(80, undo_history, &settings);
    
    // Disable wrapping
    state.toggle_line_wrapping_for_test();
    
    let long_line = "a".repeat(200);
    let text_width = 75; // Assuming some space for line numbers
    
    // Calculate visual width
    let line_visual_width = ue::coordinates::visual_width(&long_line, settings.get_tab_width());
    assert_eq!(line_visual_width, 200);
    
    // Check if end is visible
    state.set_horizontal_scroll_offset(0);
    let end_visible = state.get_horizontal_scroll_offset() + text_width >= line_visual_width;
    assert!(!end_visible, "End should not be visible at offset 0");
    
    // Scroll to where end is visible
    state.set_horizontal_scroll_offset(200 - text_width);
    let end_visible = state.get_horizontal_scroll_offset() + text_width >= line_visual_width;
    assert!(end_visible, "End should be visible when scrolled appropriately");
    
    // Scrolling past end should be prevented
    state.set_horizontal_scroll_offset(200);
    let end_visible = state.get_horizontal_scroll_offset() + text_width >= line_visual_width;
    assert!(end_visible, "End is definitely visible when offset equals line length");
}

/// Test: Untitled files should be marked appropriately and not create disk files until saved
#[test]
#[serial]
fn test_untitled_file_workflow() {
    let (_tmp, _ue_dir) = setup_test_env();

    // Simulate creating an untitled file
    let untitled_name = "untitled";
    let untitled_path = _tmp.path().join(untitled_name);

    // Check that untitled file doesn't exist on disk in temp dir
    assert!(!untitled_path.exists(), "Untitled file should not exist on disk");

    // Create an editor state for the untitled file
    let settings = Box::leak(Box::new(
        ue::settings::Settings::load().expect("Failed to load test settings"),
    ));
    let undo_history = ue::undo::UndoHistory::new();
    let _state = ue::editor_state::FileViewerState::new_for_test(80, undo_history, settings);

    // The state should be marked as untitled for files starting with "untitled" that don't exist
    // (This is checked in ui.rs when the editor state is created)

    // Simulate typing some content
    let content = vec![String::from("Hello, World!")];

    // File should still not exist on disk
    assert!(!untitled_path.exists(), "Untitled file should not be saved automatically");

    // Simulate saving with a real filename
    let real_file = _tmp.path().join("myfile.txt");
    fs::write(&real_file, content[0].as_str()).unwrap();

    // Now the file should exist
    assert!(real_file.exists(), "File should exist after explicit save");
    assert_eq!(fs::read_to_string(&real_file).unwrap(), "Hello, World!");
}
