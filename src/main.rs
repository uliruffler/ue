#![deny(warnings)]

use clap::Parser;
use ue::*;

#[derive(Parser)]
#[clap(name = "ue", version = env!("CARGO_PKG_VERSION"), about = "Simple terminal editor")]
struct Cli {
    /// Files to be processed
    files: Vec<String>,
}

fn main() -> std::io::Result<()> {
    // Deploy default syntax files if they don't exist
    let _ = default_syntax::deploy_default_syntax_files();

    let cli = Cli::parse();
    let mut files = cli.files.clone();

    if files.is_empty() {
        // Try last session first
        if let Ok(Some(last)) = session::load_last_session() {
            match last.mode {
                session::SessionMode::Editor => {
                    if let Some(f) = last.file.as_ref() {
                        // Open last file even if it doesn't exist (new buffer support)
                        files = vec![f.to_string_lossy().to_string()];
                    } else {
                        // No file recorded - fall back to selector
                        match file_selector::select_file()? {
                            Some(f) => files = vec![f],
                            None => {
                                // No tracked files - create new untitled document
                                files = vec![generate_untitled_filename()];
                            }
                        }
                    }
                }
                session::SessionMode::Selector => match file_selector::select_file()? {
                    Some(f) => files = vec![f],
                    None => {
                        // No tracked files - create new untitled document
                        files = vec![generate_untitled_filename()];
                    }
                },
            }
        } else {
            // No previous session - check if there are tracked files
            match file_selector::select_file()? {
                Some(f) => files = vec![f],
                None => {
                    // No tracked files - create new untitled document
                    files = vec![generate_untitled_filename()];
                }
            }
        }
    }

    // Update recent list for each file chosen
    for f in &files {
        let _ = recent::update_recent_file(f);
    }

    ui::show(&files)?;
    // Removed automatic editor session save; handled in event handlers on quit.
    Ok(())
}
