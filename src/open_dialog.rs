use crossterm::{
    cursor::MoveTo,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{Clear, ClearType},
};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Result from open dialog interaction
#[derive(Debug)]
#[allow(dead_code)] // Quit variant reserved for future use
pub(crate) enum OpenDialogResult {
    Selected(PathBuf),
    Cancelled,
    Quit,
}

/// Focus mode for the dialog
#[derive(Debug, Clone, Copy, PartialEq)]
enum FocusMode {
    Tree,
    Input,
}

/// Dialog mode - Open or Save As
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)] // SaveAs variant is used in ui.rs for untitled file save handling
pub(crate) enum DialogMode {
    Open,
    SaveAs,
}

/// Tree node representing a file or directory
#[derive(Debug, Clone)]
struct TreeNode {
    path: PathBuf,
    name: String,
    is_directory: bool,
    is_expanded: bool,
    depth: usize,
}

/// State for the open dialog
struct OpenDialogState {
    nodes: Vec<TreeNode>,
    selected_index: usize,
    scroll_offset: usize,
    focus: FocusMode,
    input_buffer: String,
    input_cursor: usize,
    show_hidden: bool,
    #[allow(dead_code)] // Used in event loop via conditional rendering
    help_active: bool,
    #[allow(dead_code)] // Used in event loop for help scrolling
    help_scroll_offset: usize,
    mode: DialogMode,
}

impl OpenDialogState {
    fn new(current_file: Option<&Path>, show_hidden: bool, mode: DialogMode) -> io::Result<Self> {
        // Determine the starting directory
        let start_dir = if matches!(mode, DialogMode::SaveAs) {
            // In SaveAs mode, always use current working directory
            // (the current_file parameter might be "untitled" or a relative path)
            if let Some(file) = current_file {
                let file_path = PathBuf::from(file);
                // If the file has a real parent directory that exists, use it
                if let Some(parent) = file_path.parent() {
                    if parent.exists() && parent.is_dir() {
                        parent.to_path_buf()
                    } else {
                        // Parent doesn't exist (e.g., "untitled"), use current_dir
                        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                    }
                } else {
                    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                }
            } else {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            }
        } else {
            // In Open mode, use the file's parent directory or current_dir
            if let Some(file) = current_file {
                file.parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."))
            } else {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            }
        };

        let mut state = Self {
            nodes: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            focus: FocusMode::Tree,
            input_buffer: String::new(),
            input_cursor: 0,
            show_hidden,
            help_active: false,
            help_scroll_offset: 0,
            mode,
        };

        state.build_tree(&start_dir, current_file)?;
        Ok(state)
    }

    /// Build the tree starting from root, showing the path to current file
    fn build_tree(&mut self, start_dir: &Path, current_file: Option<&Path>) -> io::Result<()> {
        self.nodes.clear();

        // Canonicalize the start directory
        let start_dir = start_dir.canonicalize().unwrap_or_else(|_| start_dir.to_path_buf());

        // Build the ancestor path from root to start_dir
        let mut ancestors = Vec::new();
        let mut current = start_dir.as_path();
        while let Some(parent) = current.parent() {
            ancestors.push(parent.to_path_buf());
            current = parent;
        }
        ancestors.reverse(); // Now we have [/, /home, /home/user, /home/user/project]

        // In SaveAs mode, we want to expand the target directory and select it
        // In Open mode, we want to select the current file
        let expand_target = matches!(self.mode, DialogMode::SaveAs);
        let select_target = if matches!(self.mode, DialogMode::SaveAs) {
            Some(start_dir.as_path())
        } else {
            current_file
        };

        // Start from root
        let mut current_selected = None;
        self.build_path_tree(&PathBuf::from("/"), &ancestors, &start_dir, select_target, &mut current_selected, 0, expand_target)?;

        if let Some(idx) = current_selected {
            self.selected_index = idx;
            self.scroll_offset = idx.saturating_sub(10);
        }

        Ok(())
    }

    /// Refresh the tree while preserving expansion states and selection
    fn refresh_tree(&mut self) -> io::Result<()> {
        // Save current expansion states
        let expanded_paths: std::collections::HashSet<PathBuf> = self.nodes.iter()
            .enumerate()
            .filter(|(idx, node)| {
                // Check if node has children (is actually expanded)
                node.is_directory
                    && *idx + 1 < self.nodes.len()
                    && self.nodes[*idx + 1].depth == node.depth + 1
            })
            .map(|(_, node)| node.path.clone())
            .collect();

        // Save current selection path
        let selected_path = self.nodes.get(self.selected_index).map(|n| n.path.clone());

        // Clear and rebuild from root
        self.nodes.clear();
        self.refresh_tree_recursive(&PathBuf::from("/"), 0, &expanded_paths)?;

        // Restore selection to same path (or closest match)
        if let Some(target_path) = selected_path {
            if let Some(idx) = self.nodes.iter().position(|n| n.path == target_path) {
                self.selected_index = idx;
                // Adjust scroll to keep selection visible
                if self.selected_index < self.scroll_offset {
                    self.scroll_offset = self.selected_index;
                } else if self.selected_index >= self.scroll_offset + 20 {
                    self.scroll_offset = self.selected_index.saturating_sub(10);
                }
            }
        }

        Ok(())
    }

    /// Recursively rebuild tree with preserved expansion states
    fn refresh_tree_recursive(
        &mut self,
        dir: &Path,
        depth: usize,
        expanded_paths: &std::collections::HashSet<PathBuf>,
    ) -> io::Result<()> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => return Ok(()),
        };

        let mut items: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                if !self.show_hidden {
                    e.file_name()
                        .to_str()
                        .map(|s| !s.starts_with('.'))
                        .unwrap_or(true)
                } else {
                    true
                }
            })
            .collect();

        // Sort: directories first, then alphabetically (case-insensitive)
        items.sort_by(|a, b| {
            let a_is_dir = a.path().is_dir();
            let b_is_dir = b.path().is_dir();

            match (a_is_dir, b_is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => {
                    let a_name = a.file_name().to_string_lossy().to_lowercase();
                    let b_name = b.file_name().to_string_lossy().to_lowercase();
                    a_name.cmp(&b_name)
                }
            }
        });

        for entry in items {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let is_directory = path.is_dir();
            let was_expanded = expanded_paths.contains(&path);

            self.nodes.push(TreeNode {
                path: path.clone(),
                name,
                is_directory,
                is_expanded: was_expanded,
                depth,
            });

            // Recursively expand if this directory was previously expanded
            if is_directory && was_expanded {
                self.refresh_tree_recursive(&path, depth + 1, expanded_paths)?;
            }
        }

        Ok(())
    }

    /// Build tree showing path from root to target, with lazy loading
    fn build_path_tree(
        &mut self,
        current_dir: &Path,
        ancestors: &[PathBuf],
        target_dir: &Path,
        select_target: Option<&Path>,
        current_selected: &mut Option<usize>,
        depth: usize,
        expand_target: bool,
    ) -> io::Result<()> {
        // Read directory entries
        let entries = match fs::read_dir(current_dir) {
            Ok(entries) => entries,
            Err(_) => return Ok(()),
        };

        let mut items: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                if !self.show_hidden {
                    e.file_name()
                        .to_str()
                        .map(|s| !s.starts_with('.'))
                        .unwrap_or(true)
                } else {
                    true
                }
            })
            .collect();

        // Sort: directories first, then alphabetically (case-insensitive)
        items.sort_by(|a, b| {
            let a_is_dir = a.path().is_dir();
            let b_is_dir = b.path().is_dir();

            match (a_is_dir, b_is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => {
                    let a_name = a.file_name().to_string_lossy().to_lowercase();
                    let b_name = b.file_name().to_string_lossy().to_lowercase();
                    a_name.cmp(&b_name)
                }
            }
        });

        for entry in items {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let is_directory = path.is_dir();

            // Check if this directory is on the path to target
            let is_on_path = ancestors.contains(&path) || path == target_dir;

            // Should this directory be expanded?
            // In SaveAs mode with expand_target, expand the target directory itself
            let should_expand = if expand_target && path == target_dir {
                true
            } else {
                is_on_path && path != target_dir
            };

            let node_index = self.nodes.len();

            // Check if this is the target to select
            if let Some(target) = select_target {
                if path == target {
                    *current_selected = Some(node_index);
                }
            }

            self.nodes.push(TreeNode {
                path: path.clone(),
                name: name.clone(),
                is_directory,
                is_expanded: should_expand,
                depth,
            });

            // Recursively expand directories on path or the target if expand_target is true
            if is_directory && (is_on_path || (expand_target && path == target_dir)) {
                self.build_path_tree(&path, ancestors, target_dir, select_target, current_selected, depth + 1, expand_target)?;
            }
        }

        Ok(())
    }


    /// Expand or collapse a directory node
    fn toggle_expand(&mut self, index: usize) -> io::Result<()> {
        if index >= self.nodes.len() {
            return Ok(());
        }

        let node = &self.nodes[index];
        if !node.is_directory {
            return Ok(());
        }

        let path = node.path.clone();
        let depth = node.depth;

        // Check if children already exist (next node is a child with depth = current depth + 1)
        let has_children = index + 1 < self.nodes.len()
            && self.nodes[index + 1].depth == depth + 1;

        if has_children {
            // Children exist, so collapse and remove them
            self.nodes[index].is_expanded = false;
            let i = index + 1;
            while i < self.nodes.len() && self.nodes[i].depth > depth {
                self.nodes.remove(i);
                // Don't increment i because removal shifts elements down
            }
        } else {
            // No children exist, so expand and add them
            self.nodes[index].is_expanded = true;
            let mut new_nodes = Vec::new();
            let mut dummy_selected = None;
            self.add_directory_children(&path, depth + 1, &mut new_nodes, &mut dummy_selected)?;

            // Insert new nodes after current
            for (offset, node) in new_nodes.into_iter().enumerate() {
                self.nodes.insert(index + 1 + offset, node);
            }
        }

        Ok(())
    }

    /// Add immediate children of a directory (non-recursive for expansion)
    fn add_directory_children(
        &mut self,
        dir: &Path,
        depth: usize,
        out_nodes: &mut Vec<TreeNode>,
        _current_selected: &mut Option<usize>,
    ) -> io::Result<()> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => return Ok(()),
        };

        let mut items: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                if !self.show_hidden {
                    e.file_name()
                        .to_str()
                        .map(|s| !s.starts_with('.'))
                        .unwrap_or(true)
                } else {
                    true
                }
            })
            .collect();

        items.sort_by(|a, b| {
            let a_is_dir = a.path().is_dir();
            let b_is_dir = b.path().is_dir();

            match (a_is_dir, b_is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => {
                    let a_name = a.file_name().to_string_lossy().to_lowercase();
                    let b_name = b.file_name().to_string_lossy().to_lowercase();
                    a_name.cmp(&b_name)
                }
            }
        });

        for entry in items {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let is_directory = path.is_dir();

            out_nodes.push(TreeNode {
                path,
                name,
                is_directory,
                is_expanded: false,
                depth,
            });
        }

        Ok(())
    }

    /// Navigate up in the tree
    fn move_up(&mut self, _visible_lines: usize) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            // Adjust scroll if selection goes above visible area
            if self.selected_index < self.scroll_offset {
                self.scroll_offset = self.selected_index;
            }
        }
    }

    /// Navigate down in the tree
    fn move_down(&mut self, visible_lines: usize) {
        if self.selected_index + 1 < self.nodes.len() {
            self.selected_index += 1;
            // Adjust scroll if selection goes below visible area (scroll before it goes off screen)
            if self.selected_index > self.scroll_offset + visible_lines - 1 {
                self.scroll_offset = self.selected_index - visible_lines + 1;
            }
        }
    }

    /// Navigate left: move to parent
    fn move_left(&mut self, visible_lines: usize) -> io::Result<()> {
        if self.selected_index >= self.nodes.len() {
            return Ok(());
        }

        let node = &self.nodes[self.selected_index];
        let depth = node.depth;

        // Move to parent (if not at root level)
        if depth > 0 {
            let parent_depth = depth - 1;
            for i in (0..self.selected_index).rev() {
                if self.nodes[i].depth == parent_depth && self.nodes[i].is_directory {
                    self.selected_index = i;
                    // Adjust scroll to keep parent visible if it's above the visible area
                    if self.selected_index < self.scroll_offset {
                        self.scroll_offset = self.selected_index;
                    }
                    // Also adjust if it would be below visible area (though less likely)
                    else if self.selected_index > self.scroll_offset + visible_lines - 1 {
                        self.scroll_offset = self.selected_index - visible_lines + 1;
                    }
                    break;
                }
            }
        }

        Ok(())
    }

    /// Navigate right: expand directory
    fn move_right(&mut self) -> io::Result<()> {
        if self.selected_index >= self.nodes.len() {
            return Ok(());
        }

        let node = &self.nodes[self.selected_index];

        if node.is_directory {
            let depth = node.depth;

            // Check if directory already has children
            let has_children = self.selected_index + 1 < self.nodes.len()
                && self.nodes[self.selected_index + 1].depth == depth + 1;

            if !has_children {
                // Directory is closed, expand it
                self.toggle_expand(self.selected_index)?;

                // Now check if children were added and move to first child if exists
                if self.selected_index + 1 < self.nodes.len() {
                    let next_node = &self.nodes[self.selected_index + 1];
                    if next_node.depth == depth + 1 {
                        self.selected_index += 1;
                    }
                }
            } else {
                // Directory was already open, just move to first child
                self.selected_index += 1;
            }
        }

        Ok(())
    }

    /// Get the currently selected path
    fn get_selected_path(&self) -> Option<PathBuf> {
        self.nodes.get(self.selected_index).map(|n| n.path.clone())
    }

    /// Switch focus to input and optionally set initial text
    fn focus_input(&mut self, initial_text: Option<String>) {
        self.focus = FocusMode::Input;
        if let Some(text) = initial_text {
            self.input_buffer = text;
            self.input_cursor = self.input_buffer.len();
        }
    }

    /// Handle input field key event
    fn handle_input_key(&mut self, key: KeyEvent) -> io::Result<Option<OpenDialogResult>> {
        match key.code {
            KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) && c == 'v' => {
                // Paste from clipboard
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    if let Ok(text) = clipboard.get_text() {
                        self.input_buffer.insert_str(self.input_cursor, &text);
                        self.input_cursor += text.len();
                    }
                }
            }
            KeyCode::Char(c) => {
                self.input_buffer.insert(self.input_cursor, c);
                self.input_cursor += 1;
            }
            KeyCode::Backspace => {
                if self.input_cursor > 0 {
                    self.input_buffer.remove(self.input_cursor - 1);
                    self.input_cursor -= 1;
                }
            }
            KeyCode::Delete => {
                if self.input_cursor < self.input_buffer.len() {
                    self.input_buffer.remove(self.input_cursor);
                }
            }
            KeyCode::Left => {
                if self.input_cursor > 0 {
                    self.input_cursor -= 1;
                }
            }
            KeyCode::Right => {
                if self.input_cursor < self.input_buffer.len() {
                    self.input_cursor += 1;
                }
            }
            KeyCode::Home => {
                self.input_cursor = 0;
            }
            KeyCode::End => {
                self.input_cursor = self.input_buffer.len();
            }
            KeyCode::Enter => {
                // Try to open the path from input
                let path = PathBuf::from(&self.input_buffer);
                // Allow both existing files and new file paths (for save-as)
                if !self.input_buffer.is_empty() {
                    // If path exists and is a file, select it
                    if path.exists() && path.is_file() {
                        return Ok(Some(OpenDialogResult::Selected(path)));
                    }
                    // If path doesn't exist, allow it (for creating new files)
                    // Make it absolute if it's relative
                    let absolute_path = if path.is_absolute() {
                        path
                    } else {
                        // Use the selected directory from the tree, not current_dir
                        let base_dir = if let Some(selected) = self.get_selected_path() {
                            if selected.is_dir() {
                                selected
                            } else {
                                // Selected is a file, use its parent
                                selected.parent()
                                    .map(|p| p.to_path_buf())
                                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
                            }
                        } else {
                            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                        };
                        base_dir.join(&path)
                    };
                    return Ok(Some(OpenDialogResult::Selected(absolute_path)));
                }
            }
            KeyCode::Tab => {
                // Switch focus back to tree
                self.focus = FocusMode::Tree;
            }
            KeyCode::Esc => {
                if self.input_buffer.is_empty() {
                    // Empty input - cancel dialog
                    return Ok(Some(OpenDialogResult::Cancelled));
                } else {
                    // Clear input and return to tree
                    self.input_buffer.clear();
                    self.input_cursor = 0;
                    self.focus = FocusMode::Tree;
                }
            }
            _ => {}
        }
        Ok(None)
    }
}

/// Run the open dialog and return the result
pub(crate) fn run_open_dialog(
    current_file: Option<&str>,
    settings: &crate::settings::Settings,
    mode: DialogMode,
) -> io::Result<OpenDialogResult> {
    let current_path = current_file.map(PathBuf::from);
    let mut state = OpenDialogState::new(current_path.as_deref(), false, mode)?;

    loop {
        let (term_width, term_height) = crossterm::terminal::size()?;
        let visible_lines = (term_height as usize).saturating_sub(2); // Header (1) + tree + input/help (1)

        if state.help_active {
            // Render help screen
            let help_content = crate::help::get_open_dialog_help(settings, term_width as usize);
            crate::help::render_help(
                &mut io::stdout(),
                &help_content,
                state.help_scroll_offset,
                term_width,
                term_height,
            )?;
        } else {
            render_dialog(&state, term_width, term_height)?;
        }

        if let Event::Key(key) = event::read()? {
            // Normalize key event to handle num-pad Enter
            let key = crate::event_handlers::normalize_key_event(key, settings);
            
            // Check for help key
            if settings.keybindings.help_matches(&key) {
                state.help_active = !state.help_active;
                state.help_scroll_offset = 0;
                continue;
            }

            // Handle help mode separately
            if state.help_active {
                if crate::help::handle_help_input(key) {
                    state.help_active = false;
                    state.help_scroll_offset = 0;
                } else {
                    // Handle scrolling in help
                    match key.code {
                        KeyCode::Up | KeyCode::Char('k') => {
                            state.help_scroll_offset = state.help_scroll_offset.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            state.help_scroll_offset += 1;
                        }
                        KeyCode::PageUp => {
                            state.help_scroll_offset = state.help_scroll_offset.saturating_sub(10);
                        }
                        KeyCode::PageDown => {
                            state.help_scroll_offset += 10;
                        }
                        _ => {}
                    }
                }
                continue;
            }

            match state.focus {
                FocusMode::Tree => {
                    match key.code {
                        KeyCode::Up | KeyCode::Char('k') => {
                            state.move_up(visible_lines);
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            state.move_down(visible_lines);
                        }
                        KeyCode::Left | KeyCode::Char('h') => {
                            state.move_left(visible_lines)?;
                        }
                        KeyCode::Right | KeyCode::Char('l') => {
                            state.move_right()?;
                        }
                        KeyCode::Enter => {
                            if let Some(path) = state.get_selected_path() {
                                if path.is_file() {
                                    return Ok(OpenDialogResult::Selected(path));
                                } else if path.is_dir() {
                                    // Check if this is the ".." entry
                                    if state.selected_index < state.nodes.len()
                                        && state.nodes[state.selected_index].name == ".." {
                                        // Navigate to parent directory
                                        let current = state.get_selected_path();
                                        state.build_tree(&path, current.as_deref())?;
                                    } else {
                                        // Toggle expand on Enter for regular directories
                                        state.toggle_expand(state.selected_index)?;
                                    }
                                }
                            }
                        }
                        KeyCode::Tab => {
                            state.focus = FocusMode::Input;
                        }
                        KeyCode::Char('.') => {
                            // Toggle hidden files
                            state.show_hidden = !state.show_hidden;
                            // Refresh tree while preserving expansion states and selection
                            state.refresh_tree()?;
                        }
                        KeyCode::Esc => {
                            return Ok(OpenDialogResult::Cancelled);
                        }
                        KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) && c == 'v' => {
                            // Switch to input on paste
                            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                if let Ok(text) = clipboard.get_text() {
                                    state.focus_input(Some(text));
                                }
                            }
                        }
                        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) => {
                            // Switch to input on typing
                            state.focus_input(Some(c.to_string()));
                        }
                        _ => {}
                    }
                }
                FocusMode::Input => {
                    if let Some(result) = state.handle_input_key(key)? {
                        return Ok(result);
                    }
                }
            }
        }
    }
}

/// Render the complete dialog
fn render_dialog(state: &OpenDialogState, width: u16, height: u16) -> io::Result<()> {
    let mut stdout = io::stdout();

    execute!(stdout, Clear(ClearType::All))?;

    // Calculate areas - header (1) + tree + input at bottom (1)
    let tree_height = height.saturating_sub(2) as usize;

    // Render header with appropriate title based on mode
    let title = match state.mode {
        DialogMode::Open => "Open File",
        DialogMode::SaveAs => "Save As",
    };

    execute!(
        stdout,
        MoveTo(0, 0),
        SetBackgroundColor(Color::Rgb { r: 0, g: 24, b: 72 }),
        SetForegroundColor(Color::White),
    )?;
    let header = format!("{:width$}", title, width = width as usize);
    execute!(stdout, Print(header), ResetColor)?;

    // Render tree
    render_tree(state, 1, tree_height, width)?;

    // Render input field at bottom
    let input_y = (height - 1) as u16;
    render_input_field(state, input_y, width)?;


    stdout.flush()?;
    Ok(())
}

/// Render the tree view
fn render_tree(state: &OpenDialogState, start_y: u16, visible_lines: usize, width: u16) -> io::Result<()> {
    let mut stdout = io::stdout();


    for (i, node) in state.nodes.iter()
        .skip(state.scroll_offset)
        .take(visible_lines)
        .enumerate()
    {
        let y = start_y + i as u16;
        let abs_index = state.scroll_offset + i;
        let is_selected = abs_index == state.selected_index;

        execute!(stdout, MoveTo(0, y))?;

        if is_selected && state.focus == FocusMode::Tree {
            // Use same color as editor scrollbar
            execute!(stdout, SetBackgroundColor(Color::Rgb { r: 100, g: 149, b: 237 }), SetForegroundColor(Color::White))?;
        }

        // Build tree prefix with proper lines
        let mut prefix = String::new();

        // For each depth level, determine if we need a vertical line or space
        for d in 0..node.depth {
            // Check if there are more siblings at depth d after the current node's subtree
            // We need to find if there's another node at depth d that comes after this entire subtree
            let mut has_more_at_depth = false;

            for n in state.nodes.iter().skip(abs_index + 1) {
                if n.depth < d {
                    // We've gone back to a shallower level, no more siblings at depth d
                    break;
                } else if n.depth == d {
                    // Found a sibling at the same depth d
                    has_more_at_depth = true;
                    break;
                }
                // If n.depth > d, continue searching (we're still in the subtree)
            }

            if has_more_at_depth {
                prefix.push_str("│  ");
            } else {
                prefix.push_str("   ");
            }
        }

        // Add tree branch character
        if node.depth > 0 {
            // Check if this is the last child at this level
            // Look ahead to see if there are more siblings at the same depth
            let is_last = !state.nodes.iter()
                .skip(abs_index + 1)
                .take_while(|n| n.depth >= node.depth)  // Stop when we go shallower
                .any(|n| n.depth == node.depth);

            if is_last {
                prefix.push_str("└─ ");
            } else {
                prefix.push_str("├─ ");
            }
        }

        // Add directory indicator
        let icon = if node.is_directory {
            // Check if directory actually has children in the tree (is actually expanded)
            let has_children = abs_index + 1 < state.nodes.len()
                && state.nodes[abs_index + 1].depth == node.depth + 1;
            if has_children { "▼ " } else { "▶ " }
        } else {
            "  "
        };

        let line = format!("{}{}{}", prefix, icon, node.name);
        let line = if line.len() > width as usize {
            &line[..width as usize]
        } else {
            &line
        };

        execute!(stdout, Print(format!("{:width$}", line, width = width as usize)))?;

        if is_selected {
            execute!(stdout, ResetColor)?;
        }
    }

    Ok(())
}

/// Render the input field
fn render_input_field(state: &OpenDialogState, y: u16, width: u16) -> io::Result<()> {
    let mut stdout = io::stdout();

    execute!(
        stdout,
        MoveTo(0, y),
        SetBackgroundColor(Color::Rgb { r: 0, g: 24, b: 72 }),
        SetForegroundColor(Color::White),
    )?;

    match state.focus {
        FocusMode::Tree => {
            // Show help text when tree is focused
            let help_text = "↑↓:Navigate  ←:Parent  →:Child  Enter:Toggle  Tab:Input  .:Hidden  Esc:Cancel";
            let line = format!("{:width$}", help_text, width = width as usize);
            execute!(stdout, Print(line))?;
        }
        FocusMode::Input => {
            // Show input field when input is focused
            // If user is typing a relative path (doesn't start with /), show selected directory first
            let prefix = if !state.input_buffer.starts_with('/') && !state.input_buffer.is_empty() {
                // Get the selected path from the tree
                if let Some(selected) = state.get_selected_path() {
                    let base_dir = if selected.is_dir() {
                        selected
                    } else {
                        // Selected is a file, use its parent
                        selected.parent()
                            .map(|p| p.to_path_buf())
                            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
                    };
                    format!("{}/", base_dir.display())
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            
            let label = if prefix.is_empty() {
                "Path: ".to_string()
            } else {
                format!("Path: {}", prefix)
            };
            
            execute!(stdout, Print(&label))?;

            let available_width = (width as usize).saturating_sub(label.len());
            let display_text = if state.input_buffer.len() > available_width {
                &state.input_buffer[state.input_buffer.len() - available_width..]
            } else {
                &state.input_buffer
            };

            execute!(stdout, Print(display_text))?;

            // Pad the rest of the line
            let remaining = available_width.saturating_sub(display_text.len());
            execute!(stdout, Print(" ".repeat(remaining)))?;

            // Position cursor in input field
            let cursor_x = label.len() + state.input_cursor.min(available_width);
            execute!(stdout, MoveTo(cursor_x as u16, y))?;
        }
    }

    execute!(stdout, ResetColor)?;

    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_node_creation() {
        let node = TreeNode {
            path: PathBuf::from("/test"),
            name: "test".to_string(),
            is_directory: true,
            is_expanded: false,
            depth: 0,
        };
        assert_eq!(node.name, "test");
        assert!(node.is_directory);
        assert!(!node.is_expanded);
    }
}

