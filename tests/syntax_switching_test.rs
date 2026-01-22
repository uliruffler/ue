// Integration test for syntax switching feature
// Run with: cargo test --test syntax_switching_test

use serial_test::serial;

#[test]
#[serial]
fn test_markdown_rust_code_block() {
    let content = r#"# Test Markdown

Regular text.

```rs
fn main() {
    println!("Hello");
}
```

More text.
"#;

    // Create temp file
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test.md");
    std::fs::write(&file_path, content).unwrap();

    // Set syntax for the file
    ue::syntax_set_current_file(file_path.to_str().unwrap());

    // Test highlighting line by line
    let lines: Vec<&str> = content.lines().collect();

    // Line 0: "# Test Markdown" - should use markdown syntax
    let (_highlights, switch) = ue::syntax_highlight_line(lines[0]);
    // Note: Highlights may be empty if syntax file has no matching patterns for this line
    assert!(switch.is_none()); // No switch on header line

    // Line 4: "```rs" - should trigger switch to rust
    let (_highlights, switch) = ue::syntax_highlight_line(lines[4]);
    assert!(switch.is_some());
    let (is_switch_back, ext) = switch.unwrap();
    assert!(!is_switch_back);
    assert_eq!(ext, "rs");

    // Apply the switch
    ue::syntax_push("rs");

    // Line 5: "fn main() {" - should use rust syntax
    let (_highlights, switch) = ue::syntax_highlight_line(lines[5]);
    // Note: The key test is that we switched syntax, not that we got specific highlights
    assert!(switch.is_none());

    // Line 8: "```" - should trigger switch back
    let (_highlights, switch) = ue::syntax_highlight_line(lines[8]);
    assert!(switch.is_some());
    let (is_switch_back, _) = switch.unwrap();
    assert!(is_switch_back);

    // Apply the switch back
    ue::syntax_pop();

    // Line 10: "More text." - should use markdown syntax again
    let (_highlights, switch) = ue::syntax_highlight_line(lines[10]);
    assert!(switch.is_none());
}

#[test]
#[serial]
fn test_language_aliases() {
    let content = r#"```bash
echo "test"
```"#;

    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test.md");
    std::fs::write(&file_path, content).unwrap();

    ue::syntax_set_current_file(file_path.to_str().unwrap());

    let lines: Vec<&str> = content.lines().collect();

    // "```bash" should trigger switch to "sh" (alias)
    let (_highlights, switch) = ue::syntax_highlight_line(lines[0]);
    assert!(switch.is_some());
    let (is_switch_back, ext) = switch.unwrap();
    assert!(!is_switch_back);
    assert_eq!(ext, "bash"); // Captured as "bash" from regex

    // After pushing, it should resolve to "sh"
    ue::syntax_push(&ext);
    // (The resolve_alias happens in get_or_load, so "bash" is loaded as "sh" internally)
}

#[test]
#[serial]
fn test_nested_syntax_switching() {
    ue::syntax_clear_stack();
    ue::syntax_set_current_file("test.md");

    // Start with markdown
    ue::syntax_push("html");
    ue::syntax_push("js");
    ue::syntax_push("css");

    // Should be using CSS now
    // (Can't easily test without actually loading syntax files, but we can test the stack)

    ue::syntax_pop(); // back to JS
    ue::syntax_pop(); // back to HTML
    ue::syntax_pop(); // back to markdown

    // After clearing should be back to base
    ue::syntax_clear_stack();
}

#[test]
#[serial]
fn test_multiple_code_blocks() {
    let content = r#"# Document

```rs
fn rust_func() {}
```

```py
def python_func():
    pass
```

```cs
public void CSharpFunc() {}
```
"#;

    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test.md");
    std::fs::write(&file_path, content).unwrap();

    ue::syntax_set_current_file(file_path.to_str().unwrap());
    ue::syntax_clear_stack();

    let lines: Vec<&str> = content.lines().collect();

    // Process each line and track switches
    for (_i, line) in lines.iter().enumerate() {
        let (_highlights, switch) = ue::syntax_highlight_line(line);
        if let Some((is_switch_back, ext)) = switch {
            if is_switch_back {
                ue::syntax_pop();
            } else {
                ue::syntax_push(&ext);
            }
        }
    }

    // After processing all lines, stack should be back to empty (all blocks closed)
    // (Can't directly test stack size, but verify no panic occurred)
}
