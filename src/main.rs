use clap::{Parser};

mod coordinates;
mod editor_state;
mod editing;
mod event_handlers;
mod file_selector;
mod rendering;
mod settings;
mod syntax;
mod ui;
mod undo;
mod recent;
#[cfg(test)] mod env; // only compile env helpers for tests
// #[cfg(test)] mod syntax_integration_tests;
// #[cfg(test)] mod syntax_loading_tests;

#[derive(Parser)]
#[clap(name = "ue", version = env!("CARGO_PKG_VERSION"), about = "Simple terminal editor")]
struct Cli {
    /// Files to be processed
    files: Vec<String>,
}

fn main() -> crossterm::Result<()> {
    let cli = Cli::parse();

    let files = if cli.files.is_empty() {
        // No files provided - show file selector
        match file_selector::select_file()? {
            Some(file) => vec![file],
            None => return Ok(()), // User cancelled selection
        }
    } else {
        cli.files
    };

    // Update recent list for each file chosen
    for f in &files { let _ = recent::update_recent_file(f); }

    ui::show(&files)?;
    Ok(())
}