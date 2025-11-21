use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Edit {
    InsertChar { line: usize, col: usize, ch: char },
    DeleteChar { line: usize, col: usize, ch: char },
    InsertLine { line: usize, content: String },
    DeleteLine { line: usize, content: String },
    SplitLine { line: usize, col: usize, before: String, after: String },
    MergeLine { line: usize, first: String, second: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoHistory {
    pub edits: Vec<Edit>,
    pub current: usize,
    pub cursor_line: usize, // absolute line index
    pub cursor_col: usize,
    pub file_content: Option<Vec<String>>,
    #[serde(default)]
    pub modified: bool,
    #[serde(default)]
    pub scroll_top: usize, // persisted top_line (first visible logical line)
}

impl UndoHistory {
    pub fn new() -> Self {
        Self {
            edits: Vec::new(),
            current: 0,
            cursor_line: 0,
            cursor_col: 0,
            file_content: None,
            modified: false,
            scroll_top: 0,
        }
    }

    pub fn push(&mut self, edit: Edit) {
        // Remove any edits after current position (they were undone)
        self.edits.truncate(self.current);
        self.edits.push(edit);
        self.current = self.edits.len();
    }

    // Update cursor, scroll position, and unsaved file content (marks modified)
    pub fn update_state(&mut self, scroll_top: usize, cursor_line: usize, cursor_col: usize, file_content: Vec<String>) {
        self.scroll_top = scroll_top;
        self.cursor_line = cursor_line;
        self.cursor_col = cursor_col;
        self.file_content = Some(file_content);
        // Only mark as modified if there are pending edits (current > 0)
        self.modified = self.current > 0;
    }

    // Update only cursor & scroll (no content change)
    pub fn update_cursor(&mut self, scroll_top: usize, cursor_line: usize, cursor_col: usize) {
        self.scroll_top = scroll_top;
        self.cursor_line = cursor_line;
        self.cursor_col = cursor_col;
    }

    pub fn clear_unsaved_state(&mut self) {
        self.file_content = None;
        self.modified = false;
    }

    pub fn can_undo(&self) -> bool { self.current > 0 }
    pub fn can_redo(&self) -> bool { self.current < self.edits.len() }

    pub fn undo(&mut self) -> Option<Edit> {
        if self.can_undo() { self.current -= 1; self.edits.get(self.current).cloned() } else { None }
    }
    pub fn redo(&mut self) -> Option<Edit> {
        if self.can_redo() { let edit = self.edits.get(self.current).cloned(); self.current += 1; edit } else { None }
    }

    pub fn save(&self, file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let history_path = Self::history_path(file_path)?;
        // Create parent directories if they don't exist
        if let Some(parent) = history_path.parent() { fs::create_dir_all(parent)?; }
        let serialized = serde_json::to_string(self)?;
        fs::write(&history_path, serialized)?;
        Ok(())
    }
    pub fn load(file_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let history_path = Self::history_path(file_path)?;
        if history_path.exists() {
            let content = fs::read_to_string(&history_path)?;
            let history: UndoHistory = serde_json::from_str(&content)?;
            Ok(history)
        } else { Ok(Self::new()) }
    }

    pub(crate) fn history_path_for(file_path: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        Self::history_path(file_path)
    }

    fn history_path(file_path: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let home = std::env::var("UE_TEST_HOME")
            .or_else(|_| std::env::var("HOME"))
            .or_else(|_| std::env::var("USERPROFILE"))?;
        
        // Convert to absolute path if relative
        let absolute_path = if file_path.starts_with('/') {
            PathBuf::from(file_path)
        } else {
            // Relative path - make it absolute using current directory
            std::env::current_dir()?.join(file_path)
        };
        
        // Canonicalize to resolve symlinks and get clean absolute path
        let canonical_path = absolute_path.canonicalize().unwrap_or(absolute_path);
        
        // Get the path without leading /
        let path_str = canonical_path.to_string_lossy();
        let normalized_path = if path_str.starts_with('/') { &path_str[1..] } else { &*path_str };
        
        // Get the filename with extension
        let filename = canonical_path.file_name()
            .and_then(|n| n.to_str())
            .ok_or("Invalid filename")?;
        
        // Get directory path
        let home_path = PathBuf::from(&home);
        let dir_path = home_path
            .join(".ue")
            .join("files")
            .join(normalized_path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(&home).join(".ue").join("files"));
        
        // Create the FILENAME.ue format (removed leading dot)
        let ue_filename = format!("{}{}.ue", "", filename);
        
        Ok(dir_path.join(ue_filename))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::set_temp_home;

    #[test]
    fn push_and_undo_redo_cycle() {
        let (_tmp, _guard) = set_temp_home();
        let mut h = UndoHistory::new();
        assert!(!h.can_undo());
        h.push(Edit::InsertChar { line:0, col:0, ch:'a' });
        h.push(Edit::InsertChar { line:0, col:1, ch:'b' });
        assert!(h.can_undo());
        let e2 = h.undo().unwrap();
        assert!(matches!(e2, Edit::InsertChar { line:0, col:1, ch:'b' }));
        assert!(h.can_redo());
        let e2r = h.redo().unwrap();
        assert!(matches!(e2r, Edit::InsertChar { line:0, col:1, ch:'b' }));
    }

    #[test]
    fn branching_after_undo_truncates_redo_chain() {
        let (_tmp, _guard) = set_temp_home();
        let mut h = UndoHistory::new();
        h.push(Edit::InsertChar { line:0, col:0, ch:'a' });
        h.push(Edit::InsertChar { line:0, col:1, ch:'b' });
        h.push(Edit::InsertChar { line:0, col:2, ch:'c' });
        let _ = h.undo();
        let _ = h.undo();
        assert!(h.can_redo());
        h.push(Edit::InsertChar { line:0, col:1, ch:'X' });
        assert!(!h.can_redo());
        assert_eq!(h.edits.len(), 2);
    }

    #[test]
    fn save_and_load_persistence() {
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("test.txt");
        let file_str = file.to_string_lossy();
        let mut h = UndoHistory::new();
        h.push(Edit::InsertChar { line:0, col:0, ch:'a' });
        h.update_state(0, 0, 1, vec!["a".into()]);
        h.save(&file_str).expect("save");
        let loaded = UndoHistory::load(&file_str).expect("load");
        assert_eq!(loaded.edits.len(), 1);
        assert_eq!(loaded.cursor_col, 1);
        assert_eq!(loaded.file_content.as_ref().unwrap(), &vec!["a".to_string()]);
    }

    #[test]
    fn undo_redo_round_trip_persistence() {
        let (_tmp, _guard) = set_temp_home();
        let file_path = std::env::var("UE_TEST_HOME").unwrap();
        let file_path = std::path::Path::new(&file_path).join("session.txt");
        let file_str = file_path.to_string_lossy();

        let mut h = UndoHistory::new();
        h.push(Edit::InsertChar { line:0, col:0, ch:'a' });
        h.push(Edit::InsertChar { line:0, col:1, ch:'b' });
        h.update_state(0, 0, 2, vec!["ab".into()]);
        h.save(&file_str).unwrap();

        let mut loaded = UndoHistory::load(&file_str).unwrap();
        assert_eq!(loaded.edits.len(), 2);
        assert_eq!(loaded.cursor_col, 2);

        assert!(loaded.can_undo());
        let _ = loaded.undo();
        let _ = loaded.undo();
        assert!(!loaded.can_undo());
        assert!(loaded.can_redo());

        let _ = loaded.redo();
        let _ = loaded.redo();
        assert!(!loaded.can_redo());
    }

    #[test]
    fn clear_unsaved_state_removes_file_content() {
        let mut h = UndoHistory::new();
        h.update_state(5, 5, 10, vec!["line1".into(), "line2".into()]);
        assert!(h.file_content.is_some());
        assert_eq!(h.cursor_line, 5);
        assert_eq!(h.cursor_col, 10);
        
        h.clear_unsaved_state();
        
        assert!(h.file_content.is_none());
        assert_eq!(h.cursor_line, 5); // cursor position should remain
        assert_eq!(h.cursor_col, 10);
    }

    #[test]
    fn different_edit_types_preserve_correctly() {
        let mut h = UndoHistory::new();
        h.push(Edit::InsertLine { line: 0, content: "new line".into() });
        h.push(Edit::DeleteChar { line: 0, col: 5, ch: 'x' });
        h.push(Edit::SplitLine { line: 0, col: 3, before: "new".into(), after: " line".into() });
        
        assert_eq!(h.edits.len(), 3);
        
        let e = h.undo().unwrap();
        assert!(matches!(e, Edit::SplitLine { .. }));
        
        let e = h.undo().unwrap();
        assert!(matches!(e, Edit::DeleteChar { line: 0, col: 5, ch: 'x' }));
        
        let e = h.undo().unwrap();
        assert!(matches!(e, Edit::InsertLine { .. }));
    }

    #[test]
    fn load_nonexistent_file_returns_new_history() {
        let (_tmp, _guard) = set_temp_home();
        let result = UndoHistory::load("/nonexistent/file.txt");
        assert!(result.is_ok());
        let h = result.unwrap();
        assert_eq!(h.edits.len(), 0);
        assert_eq!(h.current, 0);
        assert!(h.file_content.is_none());
    }

    #[test]
    fn corrupted_history_file_returns_error() {
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("corrupted.txt");
        let file_str = file.to_string_lossy();
        
        // Create a valid history first to get the path
        let mut h = UndoHistory::new();
        h.push(Edit::InsertChar { line:0, col:0, ch:'a' });
        h.save(&file_str).expect("save");
        
        // Now corrupt the history file
        let history_path = UndoHistory::history_path(&file_str).unwrap();
        fs::write(&history_path, "{ this is not valid json ").expect("write corrupted");
        
        // Loading should fail
        let result = UndoHistory::load(&file_str);
        assert!(result.is_err());
    }

    #[test]
    fn history_path_handles_absolute_paths() {
        let (_tmp, _guard) = set_temp_home();
        let result = UndoHistory::history_path("/home/user/test.txt");
        assert!(result.is_ok());
        let path = result.unwrap();
        // Should be .ue/files/home/user/test.txt.ue (no leading dot before filename)
        assert!(path.to_string_lossy().contains(".ue/files/home/user/test.txt.ue"));
    }

    #[test]
    fn history_path_handles_relative_paths() {
        let (_tmp, _guard) = set_temp_home();
        let result = UndoHistory::history_path("documents/test.txt");
        assert!(result.is_ok());
        let path = result.unwrap();
        let path_str = path.to_string_lossy();
        assert!(path_str.contains(".ue/files"));
        assert!(path_str.ends_with("documents/test.txt.ue"));
    }

    #[test]
    fn modified_flag_resets_when_all_changes_undone() {
        let (_tmp, _guard) = set_temp_home();
        let mut h = UndoHistory::new();
        
        // Initially not modified
        assert!(!h.modified);
        
        // Make some edits
        h.push(Edit::InsertChar { line: 0, col: 0, ch: 'a' });
        h.update_state(0, 0, 1, vec!["a".into()]);
        assert!(h.modified);
        assert_eq!(h.current, 1);
        
        h.push(Edit::InsertChar { line: 0, col: 1, ch: 'b' });
        h.update_state(0, 0, 2, vec!["ab".into()]);
        assert!(h.modified);
        assert_eq!(h.current, 2);
        
        // Undo one change - should still be modified
        let _ = h.undo();
        h.update_state(0, 0, 1, vec!["a".into()]);
        assert!(h.modified);
        assert_eq!(h.current, 1);
        
        // Undo all changes - should not be modified
        let _ = h.undo();
        h.update_state(0, 0, 0, vec!["".into()]);
        assert!(!h.modified);
        assert_eq!(h.current, 0);
    }

    #[test]
    fn history_path_handles_hidden_filename() {
        let (_tmp, _guard) = set_temp_home();
        let path = UndoHistory::history_path(".env").unwrap();
        let s = path.to_string_lossy();
        assert!(s.ends_with(".env.ue")); // hidden original remains with dot
    }

    #[test]
    fn history_path_handles_no_extension() {
        let (_tmp, _guard) = set_temp_home();
        let path = UndoHistory::history_path("LICENSE").unwrap();
        let s = path.to_string_lossy();
        assert!(s.ends_with("LICENSE.ue"));
    }

    #[test]
    fn history_path_handles_unicode_filename() {
        let (_tmp, _guard) = set_temp_home();
        let path = UndoHistory::history_path("übergröße.txt").unwrap();
        let s = path.to_string_lossy();
        assert!(s.ends_with("übergröße.txt.ue"));
    }

    #[test]
    fn history_path_handles_dot_slash_relative() {
        let (_tmp, _guard) = set_temp_home();
        let rel = "./docs/readme.md";
        let path = UndoHistory::history_path(rel).unwrap();
        let s = path.to_string_lossy();
        assert!(s.contains(".ue/files"));
        assert!(s.ends_with("docs/readme.md.ue"));
    }
}
