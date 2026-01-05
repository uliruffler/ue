use serial_test::serial;

#[test]
#[serial]
fn test_filter_mode_shows_only_matching_lines() {
    let lines = vec![
        "line with match".to_string(),
        "line without".to_string(),
        "another match".to_string(),
        "no hit here".to_string(),
        "final match line".to_string(),
    ];

    let pattern = "match";

    // Get lines with matches 
    let matching_lines = ue::find::get_lines_with_matches(&lines, pattern, None);

    // Should return indices 0, 2, 4
    assert_eq!(matching_lines.len(), 3);
    assert_eq!(matching_lines[0], 0);
    assert_eq!(matching_lines[1], 2);
    assert_eq!(matching_lines[2], 4);
}

#[test]
#[serial]
fn test_filter_mode_with_scope() {
    let lines = vec![
        "match at start".to_string(),
        "line without".to_string(),
        "another match".to_string(),
        "more match here".to_string(),
        "final match".to_string(),
    ];

    let pattern = "match";

    // Limit scope to lines 1-3
    let scope = Some(((1, 0), (3, 20)));

    // Get lines with matches within scope
    let matching_lines = ue::find::get_lines_with_matches(&lines, pattern, scope);

    // Should only return lines 2 and 3 (within scope range)
    assert_eq!(matching_lines.len(), 2);
    assert_eq!(matching_lines[0], 2);
    assert_eq!(matching_lines[1], 3);
}

#[test]
#[serial]
fn test_filter_mode_case_insensitive() {
    let lines = vec![
        "MATCH in caps".to_string(),
        "Match mixed case".to_string(),
        "match lowercase".to_string(),
        "no hit".to_string(),
    ];

    let pattern = "match";

    // Get lines with matches (should be case-insensitive by default)
    let matching_lines = ue::find::get_lines_with_matches(&lines, pattern, None);

    // Should find all three variants
    assert_eq!(matching_lines.len(), 3);
    assert_eq!(matching_lines[0], 0);
    assert_eq!(matching_lines[1], 1);
    assert_eq!(matching_lines[2], 2);
}

#[test]
#[serial]
fn test_filter_mode_empty_pattern() {
    let lines = vec![
        "line one".to_string(),
        "line two".to_string(),
    ];

    let pattern = "nomatch";

    // Get lines with matches
    let matching_lines = ue::find::get_lines_with_matches(&lines, pattern, None);

    // Should return empty vec
    assert_eq!(matching_lines.len(), 0);
}

#[test]
#[serial]
fn test_filter_mode_regex_pattern() {
    let lines = vec![
        "test123".to_string(),
        "test".to_string(),
        "test456".to_string(),
        "just text".to_string(),
    ];

    let pattern = r"test\d+";

    // Get lines with matches (regex pattern)
    let matching_lines = ue::find::get_lines_with_matches(&lines, pattern, None);

    // Should match lines with test followed by digits
    assert_eq!(matching_lines.len(), 2);
    assert_eq!(matching_lines[0], 0);
    assert_eq!(matching_lines[1], 2);
}

#[test]
#[serial]
fn test_filtered_lines_exclude_non_matches() {
    // Verify that get_lines_with_matches returns correct line indices
    let lines = vec![
        "line 0 with hit".to_string(),
        "line 1 no result".to_string(),
        "line 2 with hit".to_string(),
        "line 3 no result".to_string(),
        "line 4 with hit".to_string(),
        "line 5 no result".to_string(),
    ];

    let pattern = "hit";
    let filtered_lines = ue::find::get_lines_with_matches(&lines, pattern, None);

    // Should return only lines 0, 2, 4 (matching lines)
    assert_eq!(filtered_lines.len(), 3);
    assert_eq!(filtered_lines[0], 0);
    assert_eq!(filtered_lines[1], 2);
    assert_eq!(filtered_lines[2], 4);

    // Verify non-matching lines are excluded
    assert!(!filtered_lines.contains(&1));
    assert!(!filtered_lines.contains(&3));
    assert!(!filtered_lines.contains(&5));
}

#[test]
#[serial]
fn test_find_next_visible_line() {
    // Test finding the next visible line in a filtered set
    let lines = vec![
        "line 0 no result".to_string(),
        "line 1 no result".to_string(),
        "line 2 with hit".to_string(),
        "line 3 no result".to_string(),
        "line 4 with hit".to_string(),
    ];

    let pattern = "hit";
    let filtered_lines = ue::find::get_lines_with_matches(&lines, pattern, None);

    // From line 0, next visible should be 2
    let next_from_0 = filtered_lines.iter().find(|&&idx| idx > 0);
    assert_eq!(next_from_0, Some(&2));

    // From line 2, next visible should be 4
    let next_from_2 = filtered_lines.iter().find(|&&idx| idx > 2);
    assert_eq!(next_from_2, Some(&4));

    // From line 4, there is no next visible
    let next_from_4 = filtered_lines.iter().find(|&&idx| idx > 4);
    assert_eq!(next_from_4, None);
}

#[test]
#[serial]
fn test_find_previous_visible_line() {
    // Test finding the previous visible line in a filtered set
    let lines = vec![
        "line 0 with hit".to_string(),
        "line 1 no result".to_string(),
        "line 2 with hit".to_string(),
        "line 3 no result".to_string(),
        "line 4 with hit".to_string(),
    ];

    let pattern = "hit";
    let filtered_lines = ue::find::get_lines_with_matches(&lines, pattern, None);

    // From line 4, previous visible should be 2
    let prev_from_4 = filtered_lines.iter().rev().find(|&&idx| idx < 4);
    assert_eq!(prev_from_4, Some(&2));

    // From line 2, previous visible should be 0
    let prev_from_2 = filtered_lines.iter().rev().find(|&&idx| idx < 2);
    assert_eq!(prev_from_2, Some(&0));
}

