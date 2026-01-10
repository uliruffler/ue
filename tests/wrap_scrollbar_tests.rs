// Manual test to verify scrollbar appears with line wrapping
// Manual test to verify scrollbar appears with line wrapping
// Build: cargo test --test wrap_scrollbar_tests -- --nocapture
// Run with: RUST_LOG=debug cargo run < test_input.txt

use ue::{coordinates, editor_state::FileViewerState, settings::Settings, undo::UndoHistory};
use serial_test::serial;

#[test]
#[serial]
fn test_scrollbar_appears_with_wrapped_lines() {
    // Scenario from the issue:
    // - 3 visible lines
    // - 2 logical lines
    // - First line is longer than width and gets wrapped
    // - When writing at the end of line 2, line 2 gets wrapped
    // - Expected: scrollbar should appear

    use std::env;
    let home_dir = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let ue_dir = format!("{}/.ue-test-wrap", home_dir);
    let _ = std::fs::remove_dir_all(&ue_dir);
    std::fs::create_dir_all(&ue_dir).unwrap();
    unsafe { env::set_var("UE_FILES_DIR", ue_dir); }

    let settings = Settings::default();
    let undo_history = UndoHistory::new();

    // Create state with 80-char terminal width
    let state = FileViewerState::new_for_test(80, undo_history, &settings);

    // Simulate:
    // Line 1: 150 'x' characters (wraps to 2 visual lines at 76 width)
    // Line 2: 150 'y' characters (wraps to 2 visual lines)
    let lines = vec![
        "x".repeat(150),  // Will wrap to 2 visual lines
        "y".repeat(150),  // Will wrap to 2 visual lines
    ];

    // Check state: should need scrollbar since 4 visual lines > 3 visible
    let text_width = coordinates::calculate_text_width(&state, &lines, 3);
    let total_visual = coordinates::calculate_total_visual_lines(&lines, &state, text_width);

    println!("Initial state:");
    println!("  Text width: {}", text_width);
    println!("  Logical lines: {}", lines.len());
    println!("  Total visual lines: {}", total_visual);
    println!("  Visible lines: 3");
    println!("  Should show scrollbar: {}", total_visual > 3);

    // 2 logical lines, each wrapping to 2 visual lines = 4 total > 3 visible
    assert_eq!(total_visual, 4, "Two 150-char lines should produce 4 visual lines");
    assert!(total_visual > 3, "Total visual lines should exceed visible lines due to wrapping");
}

#[test]
#[serial]
fn test_wrapped_lines_affect_scrollbar_decision() {
    use std::env;
    let home_dir = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let ue_dir = format!("{}/.ue-test-wrap2", home_dir);
    let _ = std::fs::remove_dir_all(&ue_dir);
    std::fs::create_dir_all(&ue_dir).unwrap();
    unsafe { env::set_var("UE_FILES_DIR", ue_dir); }

    let settings = Settings::default();
    let undo_history = UndoHistory::new();
    let state = FileViewerState::new_for_test(80, undo_history, &settings);

    // 2 lines, each 100 chars wide (will wrap to 2 visual lines each = 4 total)
    let lines = vec![
        "x".repeat(100),
        "y".repeat(100),
    ];

    let visible_lines = 3;  // Can only show 3 visual lines
    let text_width = coordinates::calculate_text_width(&state, &lines, visible_lines);
    let total_visual = coordinates::calculate_total_visual_lines(&lines, &state, text_width);

    println!("Test with 2 wrapped lines:");
    println!("  Total visual lines: {}", total_visual);
    println!("  Visible lines: {}", visible_lines);
    println!("  Should show scrollbar: {}", total_visual > visible_lines);

    // 2 logical lines, each wrapping to 2 visual lines = 4 total > 3 visible
    assert_eq!(total_visual, 4);
    assert!(total_visual > visible_lines);
}

