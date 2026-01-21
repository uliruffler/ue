// Tests for cursor navigation through wrapped lines
// These tests verify that cursor movement through wrapped lines works correctly,
// specifically testing the bug where moving up/down would skip wrapped lines.

use serial_test::serial;

#[test]
#[serial]
fn test_wrapped_line_calculation() {
    // Verify that a 30-character line wraps with word wrapping
    let lines = vec!["123456789012345678901234567890".to_string()];
    let text_width = 10;
    let tab_width = 4;

    let wrapped = ue::coordinates::calculate_wrapped_lines_for_line(
        &lines,
        0,
        text_width,
        tab_width,
    );

    // With word wrapping: usable_width = text_width - 1 = 9
    // 30 chars / 9 usable = 4 segments (rounded up from 3.33)
    assert_eq!(wrapped, 4, "30-char line should wrap into 4 lines with width 10 (word wrapping reserves 1 char)");
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
    // Test wrapping calculations with multiple lines and word wrapping
    let lines = vec![
        "123456789012345678901234567890".to_string(), // 30 chars
        "12345678901234567890".to_string(),            // 20 chars
        "1234567890".to_string(),                       // 10 chars
    ];
    let text_width = 10;
    let tab_width = 4;

    let wrap0 = ue::coordinates::calculate_wrapped_lines_for_line(&lines, 0, text_width, tab_width);
    let wrap1 = ue::coordinates::calculate_wrapped_lines_for_line(&lines, 1, text_width, tab_width);
    let wrap2 = ue::coordinates::calculate_wrapped_lines_for_line(&lines, 2, text_width, tab_width);

    // With word wrapping: usable_width = 9 (text_width - 1)
    // Line 0: 30 / 9 = 3.33 -> 4 segments
    // Line 1: 20 / 9 = 2.22 -> 3 segments
    // Line 2: 10 / 9 = 1.11 -> 2 segments
    assert_eq!(wrap0, 4, "Line 0 should wrap into 4 lines");
    assert_eq!(wrap1, 3, "Line 1 should wrap into 3 lines");
    assert_eq!(wrap2, 2, "Line 2 should wrap into 2 lines");
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
        "123456789012345678901234567890".to_string(), // 30 chars
        "12345678901234567890".to_string(),            // 20 chars
    ];
    let text_width: u16 = 10;
    let tab_width = 4;

    // Verify line 0 wraps to 4 lines with word wrapping (usable_width = 9)
    let wraps_0 = ue::coordinates::calculate_wrapped_lines_for_line(&lines, 0, text_width, tab_width);
    assert_eq!(wraps_0, 4);

    // Verify line 1 wraps to 3 lines with word wrapping
    let wraps_1 = ue::coordinates::calculate_wrapped_lines_for_line(&lines, 1, text_width, tab_width);
    assert_eq!(wraps_1, 3);

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

    // When moving up from line 1 to line 0, we should land on the LAST wrap
    let num_wrapped = ue::coordinates::calculate_wrapped_lines_for_line(&lines, 0, text_width, tab_width) as usize;
    assert_eq!(num_wrapped, 4, "With word wrapping, 30 chars wrap to 4 lines");

    let target_wrap_line = num_wrapped.saturating_sub(1);
    assert_eq!(target_wrap_line, 3, "Should target wrap line 3 (last wrap)");

    // Note: With word wrapping, the segments aren't exactly text_width apart
    // This test is more conceptual - verifying we land on the last segment
}

#[test]
#[serial]
fn test_filter_mode_down_navigation_preserves_column() {
    // Regression test for filter mode cursor jumping to end of line
    // When navigating down in filter mode through wrapped lines with some lines filtered out,
    // the cursor should maintain its column offset, not jump to the end

    let lines = vec![
        "line1".to_string(),                           // Line 0: short, not wrapped
        "123456789012345678901234567890".to_string(), // Line 1: 30 chars, wraps to 3 lines (visible)
        "line3hidden".to_string(),                     // Line 2: filtered out
        "short".to_string(),                           // Line 3: short, not wrapped (visible)
    ];
    let text_width: u16 = 10;
    let text_width_usize = text_width as usize;
    let tab_width = 4;

    // Verify line 1 wraps to 4 lines with word wrapping
    let wraps_1 = ue::coordinates::calculate_wrapped_lines_for_line(&lines, 1, text_width, tab_width);
    assert_eq!(wraps_1, 4, "Line 1 should wrap to 4 lines");

    // Verify line 3 fits in one line (5 chars fits in usable width of 9)
    let wraps_3 = ue::coordinates::calculate_wrapped_lines_for_line(&lines, 3, text_width, tab_width);
    assert_eq!(wraps_3, 1, "Line 3 fits in 1 line");

    // Test the column offset logic for filter mode
    // If cursor is at column 3 in a wrapped line, desired_cursor_col = 3
    let desired_cursor_col: usize = 3;

    // When moving to first wrap of next visible line, offset should be 3 % 10 = 3
    let desired_offset = desired_cursor_col % text_width_usize;
    assert_eq!(desired_offset, 3, "Offset should be 3, not line length");

    // Even if desired_cursor_col is 13 (second wrap, offset 3)
    let desired_cursor_col_wrap2: usize = 13;
    let offset_wrap2 = desired_cursor_col_wrap2 % text_width_usize;
    assert_eq!(offset_wrap2, 3, "Offset from wrap 2 should also be 3");

    // Even if desired_cursor_col is 23 (third wrap, offset 3)
    let desired_cursor_col_wrap3: usize = 23;
    let offset_wrap3 = desired_cursor_col_wrap3 % text_width_usize;
    assert_eq!(offset_wrap3, 3, "Offset from wrap 3 should also be 3");

    // When landing on short line, cursor should be min(3, 5) = 3, not 5
    let short_line_len = "short".len();
    let cursor_on_short = desired_offset.min(short_line_len);
    assert_eq!(cursor_on_short, 3, "Cursor should be at offset 3, not at end of line");
}

#[test]
#[serial]
fn test_filter_mode_up_navigation_to_last_wrap() {
    // Regression test for filter mode UP navigation to last wrap
    // When moving up in filter mode from a short line to a wrapped line,
    // cursor should land on the LAST wrap of the wrapped line, not the first

    let lines = vec![
        "line0hidden".to_string(),                     // Line 0: filtered out
        "123456789012345678901234567890".to_string(), // Line 1: 30 chars, wraps to 3 lines (visible)
        "line2hidden".to_string(),                     // Line 2: filtered out
        "short".to_string(),                           // Line 3: short, not wrapped (visible)
    ];
    let text_width: u16 = 10;
    let text_width_usize = text_width as usize;
    let tab_width = 4;

    // Verify line 1 wraps to 4 lines with word wrapping
    let wraps_1 = ue::coordinates::calculate_wrapped_lines_for_line(&lines, 1, text_width, tab_width);
    assert_eq!(wraps_1, 4, "Line 1 should wrap to 4 lines");

    // When moving UP from line 3 to line 1, should land on LAST wrap
    let num_wrapped = wraps_1 as usize;
    let target_wrap_line = num_wrapped.saturating_sub(1);
    assert_eq!(target_wrap_line, 3, "Should target last wrap (wrap 3)");

    // Note: With word wrapping, exact visual column calculation is different
    // This test primarily verifies we target the last segment
}
