// Tests for scrollbar with line wrapping
use serial_test::serial;
use std::path::PathBuf;
use tempfile::TempDir;
use ue::coordinates::{calculate_text_width, calculate_total_visual_lines};
use ue::editor_state::FileViewerState;
use ue::settings::Settings;
use ue::undo::UndoHistory;

/// Helper to set up test environment with temp home directory
fn setup_test_env() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let home_path = tmp.path().to_path_buf();
    unsafe {
        std::env::set_var("UE_FILES_DIR", home_path.join(".ue"));
    }
    (tmp, home_path)
}

#[test]
#[serial]
fn test_scrollbar_with_wrapped_lines() {
    let (_tmp, _home) = setup_test_env();
    let settings = Settings::load().unwrap();
    let undo_history = UndoHistory::new();

    let state = FileViewerState::new_for_test(80, undo_history, &settings);
    let visible_lines = 3;
    let text_width = calculate_text_width(&state, &[], visible_lines);

    // Two lines, both wrap to 2 visual lines each = 4 total
    let lines = vec![
        "x".repeat(150),
        "y".repeat(150),
    ];

    let total_visual = calculate_total_visual_lines(&lines, &state, text_width);
    assert_eq!(total_visual, 4, "Should have 4 visual lines");
    assert!(total_visual > visible_lines, "Should need scrollbar");
}

#[test]
#[serial]
fn test_scrollbar_detection_with_wrapped_lines() {
    let (_tmp, _home) = setup_test_env();
    let settings = Settings::load().unwrap();
    let undo_history = UndoHistory::new();

    let state = FileViewerState::new_for_test(80, undo_history, &settings);
    let visible_lines = 3;
    let text_width = calculate_text_width(&state, &[], visible_lines);

    // Scenario: 2 logical lines, but 4 visual lines due to wrapping
    let lines = vec![
        "x".repeat(150),  // Wraps to 2 visual lines
        "y".repeat(150),  // Wraps to 2 visual lines
    ];

    let total_visual = calculate_total_visual_lines(&lines, &state, text_width);

    // Verify that even though lines.len() (2) <= visible_lines (3),
    // total_visual_lines (4) > visible_lines (3), so scrollbar should appear
    assert!(lines.len() <= visible_lines, "Logical lines fit in visible area");
    assert!(total_visual > visible_lines, "But visual lines exceed visible area");
}

#[test]
#[serial]
fn test_no_scrollbar_when_all_lines_fit() {
    let (_tmp, _home) = setup_test_env();
    let settings = Settings::load().unwrap();
    let undo_history = UndoHistory::new();

    let state = FileViewerState::new_for_test(80, undo_history, &settings);
    let visible_lines = 5;
    let text_width = calculate_text_width(&state, &[], visible_lines);

    // Short lines that don't wrap
    let lines = vec![
        "line 1".to_string(),
        "line 2".to_string(),
        "line 3".to_string(),
    ];

    let total_visual = calculate_total_visual_lines(&lines, &state, text_width);

    // Both logical and visual lines fit
    assert_eq!(total_visual, 3, "3 short lines = 3 visual lines");
    assert!(total_visual <= visible_lines, "No scrollbar needed");
}

