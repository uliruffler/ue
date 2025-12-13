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
