//! Internal module re-exports for integration tests.
//!
//! This crate is **not** a public library. All items here are re-exported as
//! `pub` solely so that the integration tests in `tests/` can access them.
//! Production code uses `pub(crate)` visibility throughout.

// Re-export all modules so integration tests in tests/ can reach them.
// dead_code warnings are suppressed because some items are only used by the binary.
pub mod coordinates;
pub mod default_syntax;
pub mod double_esc;
pub mod editing;
pub mod editor_state;
pub mod env;
pub mod event_handlers;
pub mod find;
pub mod help;
pub mod menu;
pub mod mouse_handlers;
pub mod open_dialog;
pub mod recent;
pub mod rendering;
pub mod session;
pub mod settings;
pub mod syntax;
pub mod ui;
pub mod undo;

// Re-export commonly used functions for binary
pub use ui::{generate_untitled_filename, print_keys_mode};

// Test helper functions for syntax switching
pub fn syntax_set_current_file(filepath: &str) {
    syntax::set_current_file(filepath);
}

pub fn syntax_highlight_line(line: &str) -> (Vec<(usize, usize, crossterm::style::Color)>, Option<(bool, String)>) {
    syntax::highlight_line(line)
}

pub fn syntax_push(extension: &str) {
    syntax::push_syntax(extension);
}

pub fn syntax_pop() {
    syntax::pop_syntax();
}

pub fn syntax_clear_stack() {
    syntax::clear_syntax_stack();
}
