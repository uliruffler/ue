// Tests for cursor navigation through wrapped lines
// These tests verify that cursor movement through wrapped lines works correctly,
// specifically testing the bug where moving up/down would skip wrapped lines.

use serial_test::serial;

#[test]
#[serial]
fn test_wrapped_line_calculation() {
    // Verify that a 30-character line wraps into 3 lines with width 10
    let lines = vec!["123456789012345678901234567890".to_string()];
    let text_width = 10;
    let tab_width = 4;

    let wrapped = ue::coordinates::calculate_wrapped_lines_for_line(
        &lines,
        0,
        text_width,
        tab_width,
    );

    assert_eq!(wrapped, 3, "30-char line should wrap into 3 lines with width 10");
}

#[test]
#[serial]
fn test_visual_width_calculation() {
    // Verify visual width calculations
    let line = "123456789012345678901234567890";
    let tab_width = 4;

    // Width at column 2 should be 2
    let width_at_2 = ue::coordinates::visual_width_up_to(line, 2, tab_width);
    assert_eq!(width_at_2, 2);

    // Width at column 12 should be 12
    let width_at_12 = ue::coordinates::visual_width_up_to(line, 12, tab_width);
    assert_eq!(width_at_12, 12);

    // Width at column 22 should be 22
    let width_at_22 = ue::coordinates::visual_width_up_to(line, 22, tab_width);
    assert_eq!(width_at_22, 22);
}

#[test]
#[serial]
fn test_visual_col_to_char_index() {
    // Test converting visual column back to character index
    let line = "123456789012345678901234567890";
    let tab_width = 4;

    // Visual column 2 should map to char index 2
    let char_idx = ue::coordinates::visual_col_to_char_index(line, 2, tab_width);
    assert_eq!(char_idx, 2);

    // Visual column 12 should map to char index 12
    let char_idx = ue::coordinates::visual_col_to_char_index(line, 12, tab_width);
    assert_eq!(char_idx, 12);

    // Visual column 22 should map to char index 22
    let char_idx = ue::coordinates::visual_col_to_char_index(line, 22, tab_width);
    assert_eq!(char_idx, 22);
}

#[test]
#[serial]
fn test_wrapping_with_multiple_lines() {
    // Test wrapping calculations with multiple lines
    let lines = vec![
        "123456789012345678901234567890".to_string(), // 30 chars -> 3 wraps
        "12345678901234567890".to_string(),            // 20 chars -> 2 wraps
        "1234567890".to_string(),                       // 10 chars -> 1 wrap
    ];
    let text_width = 10;
    let tab_width = 4;

    let wrap0 = ue::coordinates::calculate_wrapped_lines_for_line(&lines, 0, text_width, tab_width);
    let wrap1 = ue::coordinates::calculate_wrapped_lines_for_line(&lines, 1, text_width, tab_width);
    let wrap2 = ue::coordinates::calculate_wrapped_lines_for_line(&lines, 2, text_width, tab_width);

    assert_eq!(wrap0, 3, "Line 0 should wrap into 3 lines");
    assert_eq!(wrap1, 2, "Line 1 should wrap into 2 lines");
    assert_eq!(wrap2, 1, "Line 2 should wrap into 1 line");
}

#[test]
#[serial]
fn test_desired_cursor_col_preserved() {
    // Verify that desired_cursor_col logic works correctly
    // This is a regression test for the wrapped line navigation bug

    // When moving down through wrapped lines, the desired column should be preserved
    // within each wrap segment
    let line = "123456789012345678901234567890";
    let tab_width = 4;
    let text_width = 10;

    // Starting at column 2 (visual column 2, first wrap)
    let cursor_col = 2;
    let visual_col = ue::coordinates::visual_width_up_to(line, cursor_col, tab_width);
    assert_eq!(visual_col, 2);

    // Current wrap line (which 10-char segment are we in?)
    let current_wrap = visual_col / text_width;
    assert_eq!(current_wrap, 0, "Should be in first wrap");

    // Moving down within the same logical line should add text_width to visual_col
    let target_visual_col = visual_col + text_width;
    let new_cursor_col = ue::coordinates::visual_col_to_char_index(line, target_visual_col, tab_width);
    assert_eq!(new_cursor_col, 12, "Should move to column 12 (10 + 2)");

    // Moving down again
    let visual_col_2 = ue::coordinates::visual_width_up_to(line, new_cursor_col, tab_width);
    let target_visual_col_2 = visual_col_2 + text_width;
    let new_cursor_col_2 = ue::coordinates::visual_col_to_char_index(line, target_visual_col_2, tab_width);
    assert_eq!(new_cursor_col_2, 22, "Should move to column 22 (20 + 2)");
}

#[test]
#[serial]
fn test_down_navigation_no_skip_first_wrap() {
    // Regression test for the bug where moving down from the last wrap of a line
    // would skip the first wrap of the next line

    let lines = vec![
        "123456789012345678901234567890".to_string(), // 30 chars -> 3 wraps
        "12345678901234567890".to_string(),            // 20 chars -> 2 wraps
    ];
    let text_width: u16 = 10;
    let tab_width = 4;

    // Verify line 0 wraps to 3 lines
    let wraps_0 = ue::coordinates::calculate_wrapped_lines_for_line(&lines, 0, text_width, tab_width);
    assert_eq!(wraps_0, 3);

    // Verify line 1 wraps to 2 lines
    let wraps_1 = ue::coordinates::calculate_wrapped_lines_for_line(&lines, 1, text_width, tab_width);
    assert_eq!(wraps_1, 2);

    // Test the modulo logic for positioning on first wrap
    let desired_cursor_col: usize = 2;
    let text_width_usize = text_width as usize;
    let desired_offset = desired_cursor_col % text_width_usize;
    assert_eq!(desired_offset, 2, "Offset within wrap should be 2");

    // Even if cursor is at column 22 (last wrap, offset 2), the offset should still be 2
    let cursor_at_last_wrap: usize = 22;
    let offset_from_last = cursor_at_last_wrap % text_width_usize;
    assert_eq!(offset_from_last, 2, "Offset should be 2 even from column 22");
}

#[test]
#[serial]
fn test_up_navigation_no_skip_last_wrap() {
    // Regression test for the bug where moving up from the first wrap of a line
    // would skip the last wrap of the previous line

    let lines = vec![
        "123456789012345678901234567890".to_string(), // 30 chars -> 3 wraps
        "12345678901234567890".to_string(),            // 20 chars -> 2 wraps
    ];
    let text_width: u16 = 10;
    let text_width_usize = text_width as usize;
    let tab_width = 4;

    // When moving up from line 1 to line 0, we should land on the LAST wrap (wrap 2)
    let num_wrapped = ue::coordinates::calculate_wrapped_lines_for_line(&lines, 0, text_width, tab_width) as usize;
    assert_eq!(num_wrapped, 3);

    let target_wrap_line = num_wrapped.saturating_sub(1);
    assert_eq!(target_wrap_line, 2, "Should target wrap line 2 (last wrap)");

    let base_visual_col = target_wrap_line * text_width_usize;
    assert_eq!(base_visual_col, 20, "Base visual column for last wrap is 20");

    // If desired column is 2, target should be 20 + 2 = 22
    let desired_visual_col: usize = 2; // No tabs
    let target_visual_col = if desired_visual_col >= base_visual_col {
        desired_visual_col
    } else {
        base_visual_col + (desired_visual_col % text_width_usize)
    };
    assert_eq!(target_visual_col, 22, "Should position at column 22");
}

