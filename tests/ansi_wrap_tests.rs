//! Integration tests: word-wrap must ignore ANSI SGR escape sequences.
//!
//! Files that contain ANSI color codes in their content (e.g. colourised log
//! files, shell-script output, or any text with embedded `\x1b[…m` sequences)
//! were previously wrapped too early because the non-printable escape
//! characters were counted as visible terminal columns.  These tests verify
//! the corrected behaviour via the public `calculate_wrapped_lines_for_line`
//! function.  Low-level `calculate_word_wrap_points` unit tests live in
//! `src/coordinates.rs` (they access the `pub(crate)` function directly).

use ue::coordinates::calculate_wrapped_lines_for_line;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Wrap every space-delimited word in an arbitrary SGR colour/reset pair so
/// the resulting string contains many ANSI escape characters that must NOT
/// affect the visible width used for wrapping decisions.
fn colour(plain: &str) -> String {
    plain
        .split(' ')
        .enumerate()
        .map(|(i, word)| {
            let code = 31 + (i % 7); // cycle through red…white
            format!("\x1b[{}m{}\x1b[0m", code, word)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn wrapped_lines_short_ansi_equals_plain() {
    let plain_lines = vec!["hello world".to_string()];
    let ansi_lines = vec![colour("hello world")];

    assert_eq!(
        calculate_wrapped_lines_for_line(&plain_lines, 0, 80, 4),
        calculate_wrapped_lines_for_line(&ansi_lines, 0, 80, 4),
        "short coloured line must occupy the same number of visual lines as the plain version"
    );
}

#[test]
fn wrapped_lines_long_ansi_equals_plain() {
    // 160 visible characters should wrap into 3 visual lines (usable width = 79).
    let plain_160 = "x".repeat(160);
    // Inject colour codes at several positions – total visible chars stay 160.
    let ansi_160 = "\x1b[31m".to_string()
        + &"x".repeat(40)
        + "\x1b[0m\x1b[32m"
        + &"x".repeat(40)
        + "\x1b[0m\x1b[33m"
        + &"x".repeat(40)
        + "\x1b[0m\x1b[34m"
        + &"x".repeat(40)
        + "\x1b[0m";

    let plain_lines = vec![plain_160.clone()];
    let ansi_lines = vec![ansi_160.clone()];

    let plain_visual = calculate_wrapped_lines_for_line(&plain_lines, 0, 80, 4);
    let ansi_visual = calculate_wrapped_lines_for_line(&ansi_lines, 0, 80, 4);

    assert_eq!(
        plain_visual, ansi_visual,
        "160-char line with embedded ANSI codes must wrap the same as the plain version.\n\
         plain visual lines={plain_visual}, ansi visual lines={ansi_visual}"
    );
}

#[test]
fn wrapped_lines_ansi_only_is_one() {
    let lines = vec!["\x1b[1m\x1b[31m\x1b[0m\x1b[42m\x1b[0m".to_string()];
    assert_eq!(
        calculate_wrapped_lines_for_line(&lines, 0, 80, 4),
        1,
        "a line containing only ANSI escapes must count as exactly one visual line"
    );
}

#[test]
fn wrapped_lines_word_boundary_preserved_with_ansi() {
    // Simulate a typical coloured log line: "[INFO] Some message here"
    // Each token is wrapped in a colour code.  Width=20 should still break at
    // word boundaries, not in the middle of escape sequences.
    let plain = "[INFO] Some message here that is long enough to wrap";
    let colored = colour(plain);

    let plain_lines = vec![plain.to_string()];
    let ansi_lines = vec![colored.clone()];

    let plain_visual = calculate_wrapped_lines_for_line(&plain_lines, 0, 20, 4);
    let ansi_visual = calculate_wrapped_lines_for_line(&ansi_lines, 0, 20, 4);

    assert_eq!(
        plain_visual, ansi_visual,
        "Word-wrapped coloured log line must use the same number of visual lines as plain.\n\
         plain={plain:?}\ncolored={colored:?}"
    );
}
