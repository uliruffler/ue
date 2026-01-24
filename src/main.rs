#![deny(warnings)]

use clap::Parser;
use ue::*;

#[derive(Parser)]
#[clap(name = "ue", version = env!("CARGO_PKG_VERSION"), about = "Simple terminal editor")]
struct Cli {
    /// Print all keyboard inputs with modifiers (for testing keybindings)
    #[clap(long)]
    print_keys: bool,

    /// Files to be processed
    files: Vec<String>,
}

fn main() -> std::io::Result<()> {
    // Deploy default syntax files if they don't exist
    let _ = default_syntax::deploy_default_syntax_files();

    let cli = Cli::parse();

    // Handle --print-keys mode
    if cli.print_keys {
        return print_keys_mode();
    }

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
                        // No file recorded - get first recent file or create new
                        let recent_files = recent::get_recent_files().unwrap_or_default();
                        files = if let Some(first) = recent_files.first() {
                            vec![first.to_string_lossy().to_string()]
                        } else {
                            vec![generate_untitled_filename()]
                        };
                    }
                }
                session::SessionMode::Selector => {
                    // Selector mode - get first recent file or create new
                    let recent_files = recent::get_recent_files().unwrap_or_default();
                    files = if let Some(first) = recent_files.first() {
                        vec![first.to_string_lossy().to_string()]
                    } else {
                        vec![generate_untitled_filename()]
                    };
                }
            }
        } else {
            // No previous session - get first recent file or create new
            let recent_files = recent::get_recent_files().unwrap_or_default();
            files = if let Some(first) = recent_files.first() {
                vec![first.to_string_lossy().to_string()]
            } else {
                vec![generate_untitled_filename()]
            };
        }
    }

    // Canonicalize file paths to absolute paths for consistent display
    // but skip untitled files (they are just simple filenames)
    let files: Vec<String> = files
        .into_iter()
        .map(|f| {
            // Check if this is an untitled file (simple filename like "untitled" or "untitled-2")
            let is_untitled = {
                let path = std::path::Path::new(&f);
                let filename_lower = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                // Untitled files are simple filenames starting with "untitled" (no path separators)
                !f.contains('/') && !f.contains('\\') && filename_lower.starts_with("untitled")
            };

            if is_untitled {
                // Keep untitled files as-is
                f
            } else {
                // Canonicalize other files to absolute paths
                std::fs::canonicalize(&f)
                    .unwrap_or_else(|_| {
                        // If canonicalization fails (file doesn't exist yet), convert to absolute path manually
                        let path = std::path::PathBuf::from(&f);
                        if path.is_absolute() {
                            path
                        } else {
                            std::env::current_dir()
                                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                                .join(path)
                        }
                    })
                    .to_string_lossy()
                    .to_string()
            }
        })
        .collect();

    // Update recent list for each file chosen
    for f in &files {
        let _ = recent::update_recent_file(f);
    }

    ui::show(&files)?;
    // Removed automatic editor session save; handled in event handlers on quit.
    Ok(())
}
