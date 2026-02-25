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
    let _ = default_syntax::deploy_default_syntax_files();

    // Deploy help files to ~/.ue/help/ with keybinding substitutions applied.
    // This is done before the terminal takes over so file I/O doesn't race with rendering.
    if let Ok(settings) = ue::settings::Settings::load() {
        ue::help::deploy_help_files(&settings);
    }

    let cli = Cli::parse();

    if cli.print_keys {
        return print_keys_mode();
    }

    let mut files = cli.files.clone();

    if files.is_empty() {
        if let Ok(Some(last)) = session::load_last_session() {
            // Restore the last file regardless of mode (editor or selector).
            // For selector mode we still need a file open underneath.
            if let Some(f) = last.file.as_ref() {
                files = vec![f.to_string_lossy().to_string()];
            } else {
                files = vec![first_recent_or_untitled()];
            }
        } else {
            files = vec![first_recent_or_untitled()];
        }
    }

    // Resolve all paths to absolute form for consistent display.
    // Untitled buffers (simple names starting with "untitled", no path separators)
    // are kept as-is since they don't correspond to real filesystem paths yet.
    let files: Vec<String> = files
        .into_iter()
        .map(|f| {
            let is_untitled = {
                let lower = std::path::Path::new(&f)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                !f.contains('/') && !f.contains('\\') && lower.starts_with("untitled")
            };

            if is_untitled {
                f
            } else {
                std::fs::canonicalize(&f)
                    .unwrap_or_else(|_| {
                        // File doesn't exist yet — build an absolute path manually.
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

    for f in &files {
        let _ = recent::update_recent_file(f);
    }

    ui::show(&files)
}

/// Return the most recently used file, or a fresh untitled buffer if there are none.
fn first_recent_or_untitled() -> String {
    recent::get_recent_files()
        .unwrap_or_default()
        .into_iter()
        .next()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(generate_untitled_filename)
}
