// Test for cursor behavior at wrap boundary
// When cursor is at exactly text_width position, it should stay on current visual line
// Only when cursor goes beyond text_width should it wrap to next visual line

#[cfg(test)]
mod wrap_boundary_tests {
    // Test the formula: should a cursor at visual_col be on which visual line?
    fn calculate_visual_line_for_cursor(visual_col: usize, text_width: usize) -> usize {
        if text_width == 0 {
            return 0;
        }

        // Desired implementation:
        // Allow cursor to sit at position text_width on current visual line
        if visual_col <= text_width {
            0
        } else {
            (visual_col - 1) / text_width
        }
    }

    #[test]
    fn test_cursor_at_boundary_stays_on_current_line() {
        let text_width = 20;

        // Test cases: (visual_col, expected_visual_line_offset)
        let test_cases = vec![
            (0, 0),   // Start of line
            (19, 0),  // Before boundary
            (20, 0),  // AT boundary - should stay on visual line 0
            (21, 1),  // After boundary - wraps to visual line 1
            (40, 1),  // At second boundary - should stay on visual line 1
            (41, 2),  // After second boundary - wraps to visual line 2
            (60, 2),  // At third boundary
            (61, 3),  // After third boundary
        ];

        for (visual_col, expected_visual_line) in test_cases {
            let result = calculate_visual_line_for_cursor(visual_col, text_width);
            assert_eq!(
                result, expected_visual_line,
                "visual_col {} with text_width {} should map to visual line {}, but got {}",
                visual_col, text_width, expected_visual_line, result
            );
        }
    }

    #[test]
    fn test_cursor_x_position_calculation() {
        let text_width = 20;

        // Test x position calculation
        // Formula: if visual_col <= text_width { visual_col } else { (visual_col - 1) % text_width + 1 }
        fn calculate_cursor_x(visual_col: usize, text_width: usize) -> usize {
            if visual_col <= text_width {
                visual_col
            } else {
                (visual_col - 1) % text_width + 1
            }
        }

        let test_cases = vec![
            (0, 0),    // Start
            (19, 19),  // Before boundary
            (20, 20),  // AT boundary - x should be 20 (on scrollbar)
            (21, 1),   // After boundary - x should be 1 (start of next line)
            (40, 20),  // At second boundary - x should be 20
            (41, 1),   // After second boundary - x should be 1
        ];

        for (visual_col, expected_x) in test_cases {
            let result = calculate_cursor_x(visual_col, text_width);
            assert_eq!(
                result, expected_x,
                "visual_col {} with text_width {} should have x position {}, but got {}",
                visual_col, text_width, expected_x, result
            );
        }
    }
}
