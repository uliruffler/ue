use serial_test::serial;
use std::path::PathBuf;
use tempfile::TempDir;
use ue::editor_state::FileViewerState;
use ue::settings::Settings;
use ue::undo::UndoHistory;

/// Helper to set up test environment with temp home directory
fn setup_test_env() -> (TempDir, PathBuf) {
    let tmp_dir = TempDir::new().expect("Failed to create temp dir");
    let home_path = tmp_dir.path().to_path_buf();
    unsafe {
        std::env::set_var("UE_TEST_HOME", home_path.to_str().unwrap());
    }
    (tmp_dir, home_path)
}

/// Test that h-scrollbar is shown when line wrapping is off and lines are long
#[test]
#[serial]
fn h_scrollbar_shown_when_wrapping_off_and_lines_long() {
    let (_tmp, _home) = setup_test_env();
    let settings = Settings::load().unwrap();
    let undo_history = UndoHistory::new();
    let mut state = FileViewerState::new_for_test(80, undo_history, &settings);

    // Create lines longer than terminal width
    let lines: Vec<String> = vec![
        "a".repeat(100), // Line longer than 80 chars
        "short".to_string(),
    ];

    // Disable line wrapping
    state.toggle_line_wrapping_for_test();
    assert!(!state.is_line_wrapping_enabled_for_test());

    // Check that h-scrollbar should be shown
    assert!(state.should_show_h_scrollbar_for_test(&lines, 20));
}

/// Test that h-scrollbar is NOT shown when line wrapping is on
#[test]
#[serial]
fn h_scrollbar_not_shown_when_wrapping_on() {
    let (_tmp, _home) = setup_test_env();
    let settings = Settings::load().unwrap();
    let undo_history = UndoHistory::new();
    let state = FileViewerState::new_for_test(80, undo_history, &settings);

    // Create lines longer than terminal width
    let lines: Vec<String> = vec![
        "a".repeat(100),
        "short".to_string(),
    ];

    // Line wrapping is on by default
    assert!(state.is_line_wrapping_enabled_for_test());

    // H-scrollbar should NOT be shown
    assert!(!state.should_show_h_scrollbar_for_test(&lines, 20));
}

/// Test that effective_visible_lines is reduced when h-scrollbar is shown
#[test]
#[serial]
fn effective_visible_lines_reduced_when_h_scrollbar_shown() {
    let (_tmp, _home) = setup_test_env();
    let settings = Settings::load().unwrap();
    let undo_history = UndoHistory::new();
    let mut state = FileViewerState::new_for_test(80, undo_history, &settings);

    let lines_long: Vec<String> = vec!["a".repeat(100)];
    let lines_short: Vec<String> = vec!["short".to_string()];

    // Disable line wrapping
    state.toggle_line_wrapping_for_test();

    // With long lines, effective visible lines should be visible_lines - 1
    assert_eq!(state.effective_visible_lines_for_test(&lines_long, 20), 19);

    // With short lines, effective visible lines should be visible_lines
    assert_eq!(state.effective_visible_lines_for_test(&lines_short, 20), 20);
}

/// Test that cursor remains visible when wrapping is off and document has long lines
#[test]
#[serial]
fn cursor_visibility_with_long_lines_wrapping_off() {
    let (_tmp, _home) = setup_test_env();
    let settings = Settings::load().unwrap();
    let undo_history = UndoHistory::new();
    let mut state = FileViewerState::new_for_test(80, undo_history, &settings);

    // Create document with long first line (would wrap to 3 visual lines if wrapping was on)
    // Plus enough short lines to fill the screen
    let mut lines: Vec<String> = Vec::new();
    lines.push("a".repeat(161)); // First line: 161 chars = would be 3 wrapped lines (80+80+1)
    for i in 1..25 {
        lines.push(format!("Line {}", i)); // Short lines
    }

    // Disable line wrapping
    state.toggle_line_wrapping_for_test();
    assert!(!state.is_line_wrapping_enabled_for_test());

    let visible_lines = 20;
    let text_width = 70; // After line numbers

    // Verify h-scrollbar will be shown (long line exceeds width)
    assert!(state.should_show_h_scrollbar_for_test(&lines, visible_lines));

    // Effective visible lines should be 19 (20 - 1 for h-scrollbar)
    assert_eq!(state.effective_visible_lines_for_test(&lines, visible_lines), 19);

    // Test cursor visibility as we move down through the document
    state.set_top_line(0);
    state.set_cursor_col_test(0);

    // Cursor at line 0 (the long line) should be visible
    state.set_cursor_line(0);
    assert!(state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
            "Cursor at line 0 should be visible");

    // Cursor at line 17 should be visible (within effective_visible_lines of 19)
    state.set_cursor_line(17);
    assert!(state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
            "Cursor at line 17 should be visible");

    // Cursor at line 18 (last visible line when h-scrollbar shown) should be visible
    state.set_cursor_line(18);
    assert!(state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
            "Cursor at line 18 should be visible");

    // Cursor at line 19 (beyond effective_visible_lines) should NOT be visible
    state.set_cursor_line(19);
    assert!(!state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
            "Cursor at line 19 should NOT be visible (needs scroll)");
}

/// Test cursor visibility with long line in different positions
#[test]
#[serial]
fn cursor_visibility_long_line_at_line_2() {
    let (_tmp, _home) = setup_test_env();
    let settings = Settings::load().unwrap();
    let undo_history = UndoHistory::new();
    let mut state = FileViewerState::new_for_test(80, undo_history, &settings);

    // Create document with long line at position 2
    let mut lines: Vec<String> = Vec::new();
    lines.push("Short line 1".to_string());
    lines.push("a".repeat(161)); // Line 2: long line
    for i in 2..25 {
        lines.push(format!("Line {}", i));
    }

    // Disable line wrapping
    state.toggle_line_wrapping_for_test();

    let visible_lines = 20;
    let text_width = 70;

    state.set_top_line(0);
    state.set_cursor_col_test(0);

    // Move through lines - cursor should remain visible until line 19
    for line in 0..19 {
        state.set_cursor_line(line);
        assert!(state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
                "Cursor at line {} should be visible", line);
    }

    // Line 19 should NOT be visible (beyond effective_visible_lines of 19)
    state.set_cursor_line(19);
    assert!(!state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
            "Cursor at line 19 should NOT be visible");
}

/// Test that cursor visibility calculation correctly counts 1 line per logical line when wrapping is off
#[test]
#[serial]
fn visual_lines_calculation_with_wrapping_off() {
    let (_tmp, _home) = setup_test_env();
    let settings = Settings::load().unwrap();
    let undo_history = UndoHistory::new();
    let mut state = FileViewerState::new_for_test(80, undo_history, &settings);

    // Create document with alternating long and short lines
    let mut lines: Vec<String> = Vec::new();
    for i in 0..30 {
        if i % 2 == 0 {
            lines.push("a".repeat(200)); // Very long lines
        } else {
            lines.push(format!("Short {}", i));
        }
    }

    // Disable line wrapping
    state.toggle_line_wrapping_for_test();

    let visible_lines = 20;
    let text_width = 70;

    state.set_top_line(0);
    state.set_cursor_col_test(0);

    // When wrapping is off, moving down 18 lines should consume exactly 18 visual lines
    // (not more, even though some lines are very long)
    state.set_cursor_line(18);
    assert!(state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
            "With wrapping off, line 18 should be visible (counts as 18 visual lines, not more)");
}

/// Test exact scenario from bug report: long line at position 1
#[test]
#[serial]
fn cursor_visibility_long_line_position_1_bug_scenario() {
    let (_tmp, _home) = setup_test_env();
    let settings = Settings::load().unwrap();
    let undo_history = UndoHistory::new();
    let mut state = FileViewerState::new_for_test(80, undo_history, &settings);

    // Recreate exact bug scenario: first line is 1 char longer than visual line
    let mut lines: Vec<String> = Vec::new();
    lines.push("a".repeat(81)); // 81 chars = visual line width + 1
    for i in 1..25 {
        lines.push(format!("Short line {}", i));
    }

    state.toggle_line_wrapping_for_test();
    let visible_lines = 20;
    let text_width = 70;

    state.set_top_line(0);
    state.set_cursor_col_test(0);

    // Cursor should remain visible through line 18 (last visible with h-scrollbar)
    // Bug was: cursor would vanish 1 line before h-scrollbar
    for line in 0..19 {
        state.set_cursor_line(line);
        assert!(state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
                "Line {} should be visible (bug: would vanish 1 line before h-scrollbar)", line);
    }
}

/// Test exact scenario from bug report: long line at position 2
#[test]
#[serial]
fn cursor_visibility_long_line_position_2_bug_scenario() {
    let (_tmp, _home) = setup_test_env();
    let settings = Settings::load().unwrap();
    let undo_history = UndoHistory::new();
    let mut state = FileViewerState::new_for_test(80, undo_history, &settings);

    // Long line at position 2
    let mut lines: Vec<String> = Vec::new();
    lines.push("Short line 0".to_string());
    lines.push("a".repeat(81)); // Long line at position 1
    for i in 2..25 {
        lines.push(format!("Short line {}", i));
    }

    state.toggle_line_wrapping_for_test();
    let visible_lines = 20;
    let text_width = 70;

    state.set_top_line(0);
    state.set_cursor_col_test(0);

    // Bug was: cursor would vanish 1 line before h-scrollbar, take 2 downs to reappear
    for line in 0..19 {
        state.set_cursor_line(line);
        assert!(state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
                "Line {} should be visible", line);
    }
}

/// Test exact scenario from bug report: long line at position 3
#[test]
#[serial]
fn cursor_visibility_long_line_position_3_bug_scenario() {
    let (_tmp, _home) = setup_test_env();
    let settings = Settings::load().unwrap();
    let undo_history = UndoHistory::new();
    let mut state = FileViewerState::new_for_test(80, undo_history, &settings);

    // Long line at position 3
    let mut lines: Vec<String> = Vec::new();
    for i in 0..2 {
        lines.push(format!("Short line {}", i));
    }
    lines.push("a".repeat(81)); // Long line at position 2
    for i in 3..25 {
        lines.push(format!("Short line {}", i));
    }

    state.toggle_line_wrapping_for_test();
    let visible_lines = 20;
    let text_width = 70;

    state.set_top_line(0);
    state.set_cursor_col_test(0);

    // Bug was: cursor would vanish 1 line before h-scrollbar, take 3 downs to reappear
    for line in 0..19 {
        state.set_cursor_line(line);
        assert!(state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
                "Line {} should be visible", line);
    }
}

/// Test exact scenario from bug report: line that would wrap to 3 visual lines
#[test]
#[serial]
fn cursor_visibility_triple_wrap_line_bug_scenario() {
    let (_tmp, _home) = setup_test_env();
    let settings = Settings::load().unwrap();
    let undo_history = UndoHistory::new();
    let mut state = FileViewerState::new_for_test(80, undo_history, &settings);

    // First line is twice as long as visual line plus 1 (would wrap to 3 lines)
    let mut lines: Vec<String> = Vec::new();
    lines.push("a".repeat(161)); // 161 chars = 2 * 80 + 1 = would be 3 wrapped lines
    for i in 1..25 {
        lines.push(format!("Short line {}", i));
    }

    state.toggle_line_wrapping_for_test();
    let visible_lines = 20;
    let text_width = 70;

    state.set_top_line(0);
    state.set_cursor_col_test(0);

    // Bug was: cursor would vanish 2 lines before h-scrollbar, take 2 downs to reappear
    // Now: should remain visible through line 18
    for line in 0..19 {
        state.set_cursor_line(line);
        assert!(state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
                "Line {} should be visible (bug: would vanish 2 lines before h-scrollbar)", line);
    }
}

/// Test that wrapping calculation doesn't affect visibility when wrapping is off
#[test]
#[serial]
fn wrapping_disabled_ignores_line_length_for_visibility() {
    let (_tmp, _home) = setup_test_env();
    let settings = Settings::load().unwrap();
    let undo_history = UndoHistory::new();
    let mut state = FileViewerState::new_for_test(80, undo_history, &settings);

    // Create lines with wildly varying lengths
    let mut lines: Vec<String> = Vec::new();
    lines.push("a".repeat(500)); // Extremely long
    lines.push("b".to_string()); // Very short
    lines.push("c".repeat(1000)); // Even longer
    lines.push("d".to_string()); // Short
    for i in 4..25 {
        lines.push(format!("Line {}", i));
    }

    state.toggle_line_wrapping_for_test();
    let visible_lines = 20;
    let text_width = 70;

    state.set_top_line(0);
    state.set_cursor_col_test(0);

    // Despite wildly varying line lengths, each should count as exactly 1 visual line
    // So line 18 (19th line) should still be visible
    state.set_cursor_line(18);
    assert!(state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
            "Line 18 should be visible regardless of varying line lengths when wrapping is off");

    // Line 19 should NOT be visible (beyond effective_visible_lines)
    state.set_cursor_line(19);
    assert!(!state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
            "Line 19 should NOT be visible");
}

/// Test cursor visibility with multiple long lines scattered throughout document
#[test]
#[serial]
fn cursor_visibility_multiple_long_lines_scattered() {
    let (_tmp, _home) = setup_test_env();
    let settings = Settings::load().unwrap();
    let undo_history = UndoHistory::new();
    let mut state = FileViewerState::new_for_test(80, undo_history, &settings);

    // Create document with long lines at positions 0, 5, 10, 15
    let mut lines: Vec<String> = Vec::new();
    for i in 0..25 {
        if i % 5 == 0 {
            lines.push("a".repeat(200)); // Long line
        } else {
            lines.push(format!("Short line {}", i));
        }
    }

    state.toggle_line_wrapping_for_test();
    let visible_lines = 20;
    let text_width = 70;

    state.set_top_line(0);
    state.set_cursor_col_test(0);

    // Even with multiple long lines, cursor should be visible through line 18
    for line in 0..19 {
        state.set_cursor_line(line);
        assert!(state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
                "Line {} should be visible with multiple long lines scattered", line);
    }
}

/// Test that cursor is hidden when scrolled off horizontally
#[test]
#[serial]
fn cursor_hidden_when_scrolled_off_horizontally() {
    let (_tmp, _home) = setup_test_env();
    let settings = Settings::load().unwrap();
    let undo_history = UndoHistory::new();
    let mut state = FileViewerState::new_for_test(80, undo_history, &settings);

    // Create a long line
    let lines: Vec<String> = vec!["a".repeat(200)];

    // Disable line wrapping
    state.toggle_line_wrapping_for_test();

    let visible_lines = 20;
    let text_width = 70;

    state.set_top_line(0);
    state.set_cursor_line(0);

    // Cursor at beginning (column 0) with no scroll - should be visible
    state.set_cursor_col_test(0);
    state.set_horizontal_scroll_offset(0);
    assert!(state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
            "Cursor at column 0 with no scroll should be visible");

    // Cursor at column 10 with scroll offset 20 - scrolled off to the left
    state.set_cursor_col_test(10);
    state.set_horizontal_scroll_offset(20);
    assert!(!state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
            "Cursor at column 10 with scroll offset 20 should be hidden (off to left)");

    // Cursor at column 50 with scroll offset 20 - should be visible (within 20-90 range)
    state.set_cursor_col_test(50);
    state.set_horizontal_scroll_offset(20);
    assert!(state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
            "Cursor at column 50 with scroll offset 20 should be visible");

    // Cursor at column 100 with scroll offset 20 - scrolled off to the right
    // (visible range is 20 to 20+70=90, cursor at 100 is beyond)
    state.set_cursor_col_test(100);
    state.set_horizontal_scroll_offset(20);
    assert!(!state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
            "Cursor at column 100 with scroll offset 20 should be hidden (off to right)");

    // Cursor at column 89 with scroll offset 20 - at right edge, should be visible
    state.set_cursor_col_test(89);
    state.set_horizontal_scroll_offset(20);
    assert!(state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
            "Cursor at column 89 (right edge of visible range) should be visible");

    // Cursor at column 90 with scroll offset 20 - just beyond right edge
    state.set_cursor_col_test(90);
    state.set_horizontal_scroll_offset(20);
    assert!(!state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
            "Cursor at column 90 (just beyond right edge) should be hidden");
}

/// Test cursor visibility with horizontal scroll and line wrapping enabled
#[test]
#[serial]
fn cursor_visibility_not_affected_by_horizontal_scroll_when_wrapping_on() {
    let (_tmp, _home) = setup_test_env();
    let settings = Settings::load().unwrap();
    let undo_history = UndoHistory::new();
    let mut state = FileViewerState::new_for_test(80, undo_history, &settings);

    let lines: Vec<String> = vec!["a".repeat(200)];

    // Line wrapping is on by default
    assert!(state.is_line_wrapping_enabled_for_test());

    let visible_lines = 20;
    let text_width = 70;

    state.set_top_line(0);
    state.set_cursor_line(0);
    state.set_cursor_col_test(50);

    // Even if horizontal_scroll_offset is set, it shouldn't affect visibility when wrapping is on
    state.set_horizontal_scroll_offset(100);
    assert!(state.is_cursor_visible_for_test(&lines, visible_lines, text_width),
            "Cursor should be visible when wrapping is on, regardless of horizontal scroll offset");
}
