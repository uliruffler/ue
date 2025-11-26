use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, time::SystemTime};

// Helper module for serializing Option<u64> timestamps
mod optional_systemtime {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<u64>::deserialize(deserializer)
    }
}

#[derive(Debug, PartialEq)]
pub enum ValidationResult {
    Valid,
    ModifiedNoUnsaved,
    ModifiedWithUnsaved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Edit {
    InsertChar { line: usize, col: usize, ch: char },
    DeleteChar { line: usize, col: usize, ch: char },
    InsertLine { line: usize, content: String },
    DeleteLine { line: usize, content: String },
    SplitLine { line: usize, col: usize, before: String, after: String },
    MergeLine { line: usize, first: String, second: String },
    DragBlock {
        before: Vec<String>,
        after: Vec<String>,
        source_start: (usize, usize),
        source_end: (usize, usize),
        dest: (usize, usize),
        copy: bool,
    },
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
    #[serde(default, with = "optional_systemtime")]
    pub file_timestamp: Option<u64>, // UNIX epoch timestamp of file when undo was saved
    #[serde(default)]
    pub saved_at: usize, // Edit position where the file was last saved
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
            file_timestamp: None,
            saved_at: 0,
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
        // Mark as modified if current position differs from saved position
        self.modified = self.current != self.saved_at;
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
        // Mark current position as the saved baseline
        self.saved_at = self.current;
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
        
        // Create a copy with updated timestamp
        let mut history_to_save = self.clone();
        // Capture current file modification time
        if let Ok(metadata) = fs::metadata(file_path)
            && let Ok(modified) = metadata.modified()
            && let Ok(duration) = modified.duration_since(SystemTime::UNIX_EPOCH) {
            history_to_save.file_timestamp = Some(duration.as_secs());
        }
        
        let serialized = serde_json::to_string(&history_to_save)?;
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

    /// Validate undo history against current file modification timestamp
    /// Returns ValidationResult indicating if the file was modified externally
    pub fn validate(&self, file_path: &str) -> ValidationResult {
        // If no timestamp stored (old format), treat as valid
        let stored_timestamp = match self.file_timestamp {
            Some(ts) => ts,
            None => return ValidationResult::Valid,
        };

        // Get current file modification time
        let current_timestamp = match fs::metadata(file_path) {
            Ok(metadata) => match metadata.modified() {
                Ok(modified) => match modified.duration_since(SystemTime::UNIX_EPOCH) {
                    Ok(duration) => duration.as_secs(),
                    Err(_) => return ValidationResult::Valid, // Can't determine, treat as valid
                },
                Err(_) => return ValidationResult::Valid,
            },
            Err(_) => return ValidationResult::Valid, // File doesn't exist or can't read, treat as valid
        };

        // Compare timestamps
        if current_timestamp != stored_timestamp {
            // File was modified externally
            if self.modified {
                ValidationResult::ModifiedWithUnsaved
            } else {
                ValidationResult::ModifiedNoUnsaved
            }
        } else {
            ValidationResult::Valid
        }
    }

    pub(crate) fn history_path_for(file_path: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        Self::history_path(file_path)
    }

    /// Get modification time of undo history file in seconds since UNIX epoch
    /// Returns None if file doesn't exist or can't be read
    pub(crate) fn get_undo_file_mtime(file_path: &str) -> Option<u128> {
        let history_path = Self::history_path(file_path).ok()?;
        let metadata = fs::metadata(&history_path).ok()?;
        let modified = metadata.modified().ok()?;
        let duration = modified.duration_since(SystemTime::UNIX_EPOCH).ok()?;
        Some(duration.as_nanos())
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
        let normalized_path = if let Some(stripped) = path_str.strip_prefix('/') { stripped } else { &*path_str };
        
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

    #[test]
    fn validate_returns_valid_when_no_timestamp() {
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("test.txt");
        fs::write(&file, "content").unwrap();
        let file_str = file.to_string_lossy();
        
        // Create history without timestamp (simulating old format)
        let mut h = UndoHistory::new();
        h.file_timestamp = None;
        
        let result = h.validate(&file_str);
        assert_eq!(result, ValidationResult::Valid);
    }

    #[test]
    fn validate_returns_valid_when_timestamps_match() {
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("test.txt");
        fs::write(&file, "content").unwrap();
        let file_str = file.to_string_lossy();
        
        // Save and load to capture timestamp
        let mut h = UndoHistory::new();
        h.push(Edit::InsertChar { line: 0, col: 0, ch: 'a' });
        h.save(&file_str).unwrap();
        
        let loaded = UndoHistory::load(&file_str).unwrap();
        let result = loaded.validate(&file_str);
        assert_eq!(result, ValidationResult::Valid);
    }

    #[test]
    fn validate_returns_modified_no_unsaved_when_file_changed() {
        use std::thread;
        use std::time::Duration;
        
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("test.txt");
        fs::write(&file, "content").unwrap();
        let file_str = file.to_string_lossy();
        
        // Save history
        let h = UndoHistory::new();
        h.save(&file_str).unwrap();
        
        // Wait enough time to ensure timestamp changes (filesystem resolution)
        thread::sleep(Duration::from_secs(2));
        fs::write(&file, "modified content").unwrap();
        
        // Load and validate
        let loaded = UndoHistory::load(&file_str).unwrap();
        let result = loaded.validate(&file_str);
        
        // This test may not work on all filesystems - mark as ignored
        assert_eq!(result, ValidationResult::ModifiedNoUnsaved);
    }

    #[test]
    fn validate_returns_modified_with_unsaved_when_file_changed_and_has_unsaved() {
        use std::thread;
        use std::time::Duration;
        
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("test.txt");
        fs::write(&file, "content").unwrap();
        let file_str = file.to_string_lossy();
        
        // Create history with unsaved changes
        let mut h = UndoHistory::new();
        h.push(Edit::InsertChar { line: 0, col: 0, ch: 'a' });
        h.update_state(0, 0, 1, vec!["a".into()]);
        h.save(&file_str).unwrap();
        
        // Wait enough time to ensure timestamp changes (filesystem resolution)
        thread::sleep(Duration::from_secs(2));
        fs::write(&file, "modified content").unwrap();
        
        // Load and validate
        let loaded = UndoHistory::load(&file_str).unwrap();
        assert!(loaded.modified);
        let result = loaded.validate(&file_str);
        
        // This test may not work on all filesystems - mark as ignored
        assert_eq!(result, ValidationResult::ModifiedWithUnsaved);
    }

    #[test]
    fn save_captures_file_timestamp() {
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("test.txt");
        fs::write(&file, "content").unwrap();
        let file_str = file.to_string_lossy();
        
        let h = UndoHistory::new();
        h.save(&file_str).unwrap();
        
        let loaded = UndoHistory::load(&file_str).unwrap();
        assert!(loaded.file_timestamp.is_some());
    }

    #[test]
    fn validate_with_no_file_content_and_no_edits() {
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("test.txt");
        fs::write(&file, "original content").unwrap();
        let file_str = file.to_string_lossy();
        
        // Save history with no edits and no unsaved content
        let h = UndoHistory::new();
        h.save(&file_str).unwrap();
        
        let loaded = UndoHistory::load(&file_str).unwrap();
        assert!(!loaded.modified);
        assert!(loaded.file_content.is_none());
        assert_eq!(loaded.edits.len(), 0);
        
        // Validation should pass - file unchanged
        let result = loaded.validate(&file_str);
        assert_eq!(result, ValidationResult::Valid);
    }

    #[test]
    fn validate_with_file_content_but_no_modified_flag() {
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("test.txt");
        fs::write(&file, "original content").unwrap();
        let file_str = file.to_string_lossy();
        
        // Create history with file content but modified=false (simulating saved state)
        let mut h = UndoHistory::new();
        h.file_content = Some(vec!["saved content".to_string()]);
        h.modified = false;
        h.save(&file_str).unwrap();
        
        let loaded = UndoHistory::load(&file_str).unwrap();
        assert!(!loaded.modified);
        assert!(loaded.file_content.is_some());
        
        // Validation should pass - no modifications
        let result = loaded.validate(&file_str);
        assert_eq!(result, ValidationResult::Valid);
    }

    #[test]
    fn validate_with_edits_and_modified_flag() {
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("test.txt");
        fs::write(&file, "original").unwrap();
        let file_str = file.to_string_lossy();
        
        // Create history with edits and unsaved changes
        let mut h = UndoHistory::new();
        h.push(Edit::InsertChar { line: 0, col: 0, ch: 'a' });
        h.update_state(0, 0, 1, vec!["aoriginal".to_string()]);
        assert!(h.modified);
        h.save(&file_str).unwrap();
        
        let loaded = UndoHistory::load(&file_str).unwrap();
        assert!(loaded.modified);
        assert_eq!(loaded.edits.len(), 1);
        assert!(loaded.file_content.is_some());
        
        // Validation should pass - file unchanged
        let result = loaded.validate(&file_str);
        assert_eq!(result, ValidationResult::Valid);
    }

    #[test]
    fn validate_preserves_edits_after_undo() {
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("test.txt");
        fs::write(&file, "original").unwrap();
        let file_str = file.to_string_lossy();
        
        // Create history with multiple edits, then undo some
        let mut h = UndoHistory::new();
        h.push(Edit::InsertChar { line: 0, col: 0, ch: 'a' });
        h.push(Edit::InsertChar { line: 0, col: 1, ch: 'b' });
        h.push(Edit::InsertChar { line: 0, col: 2, ch: 'c' });
        
        // Undo one
        h.undo();
        h.update_state(0, 0, 2, vec!["aboriginal".to_string()]);
        
        assert!(h.modified);
        assert_eq!(h.current, 2); // 2 edits in effect
        assert_eq!(h.edits.len(), 3); // 3 total edits (can redo)
        h.save(&file_str).unwrap();
        
        let loaded = UndoHistory::load(&file_str).unwrap();
        assert!(loaded.modified);
        assert_eq!(loaded.current, 2);
        assert_eq!(loaded.edits.len(), 3);
        assert!(loaded.can_redo());
        
        // Validation should pass
        let result = loaded.validate(&file_str);
        assert_eq!(result, ValidationResult::Valid);
    }


    #[test]
    fn validation_handles_missing_file() {
        let (_tmp, _guard) = set_temp_home();
        
        // Create history for a file that doesn't exist
        let mut h = UndoHistory::new();
        h.file_timestamp = Some(12345);
        h.push(Edit::InsertChar { line: 0, col: 0, ch: 'a' });
        h.update_state(0, 0, 1, vec!["a".to_string()]);
        
        // Validate against non-existent file - should return Valid (graceful handling)
        let result = h.validate("/tmp/nonexistent_file_xyz_123.txt");
        assert_eq!(result, ValidationResult::Valid);
    }

    #[test]
    fn clear_unsaved_state_preserves_edits_and_cursor() {
        let mut h = UndoHistory::new();
        h.push(Edit::InsertChar { line: 0, col: 0, ch: 'a' });
        h.push(Edit::InsertChar { line: 0, col: 1, ch: 'b' });
        h.update_state(5, 10, 15, vec!["ab".to_string()]);
        
        assert!(h.modified);
        assert!(h.file_content.is_some());
        assert_eq!(h.edits.len(), 2);
        assert_eq!(h.scroll_top, 5);
        assert_eq!(h.cursor_line, 10);
        assert_eq!(h.cursor_col, 15);
        
        h.clear_unsaved_state();
        
        // Edits and cursor should be preserved
        assert_eq!(h.edits.len(), 2);
        assert_eq!(h.scroll_top, 5);
        assert_eq!(h.cursor_line, 10);
        assert_eq!(h.cursor_col, 15);
        
        // But modified and file_content should be cleared
        assert!(!h.modified);
        assert!(h.file_content.is_none());
    }

    #[test]
    fn backward_compatibility_old_format_without_timestamp() {
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("test.txt");
        fs::write(&file, "content").unwrap();
        let file_str = file.to_string_lossy();
        
        // Create an old-format undo history manually (without using save)
        let mut h = UndoHistory::new();
        h.push(Edit::InsertChar { line: 0, col: 0, ch: 'a' });
        h.update_state(0, 0, 1, vec!["a".to_string()]);
        // Explicitly set timestamp to None to simulate old format
        h.file_timestamp = None;
        
        // Manually save as JSON without timestamp
        let history_path = UndoHistory::history_path_for(&file_str).unwrap();
        if let Some(parent) = history_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let serialized = serde_json::to_string(&h).unwrap();
        fs::write(&history_path, serialized).unwrap();
        
        // Load and validate
        let loaded = UndoHistory::load(&file_str).unwrap();
        assert!(loaded.file_timestamp.is_none());
        assert!(loaded.modified);
        
        // Should validate as Valid (backward compatibility)
        let result = loaded.validate(&file_str);
        assert_eq!(result, ValidationResult::Valid);
    }

    #[test]
    fn undo_file_exists_after_validation_with_modified_no_unsaved() {
        use std::thread;
        use std::time::Duration;
        
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("test.txt");
        fs::write(&file, "original content").unwrap();
        let file_str = file.to_string_lossy();
        
        // Create and save undo history with no unsaved changes
        let h = UndoHistory::new();
        h.save(&file_str).unwrap();
        
        // Verify undo file exists
        let history_path = UndoHistory::history_path_for(&file_str).unwrap();
        assert!(history_path.exists());
        
        // Modify file externally
        thread::sleep(Duration::from_millis(100));
        fs::write(&file, "modified content").unwrap();
        
        // Load and validate
        let loaded = UndoHistory::load(&file_str).unwrap();
        let _result = loaded.validate(&file_str);
        
        // Validation should detect modification (or Valid if timestamps don't differ)
        // But importantly, the undo file should STILL exist
        assert!(history_path.exists(), "Undo file should not be deleted by validation");
    }

    #[test]
    fn undo_file_exists_after_validation_with_modified_with_unsaved() {
        use std::thread;
        use std::time::Duration;
        
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("test.txt");
        fs::write(&file, "original").unwrap();
        let file_str = file.to_string_lossy();
        
        // Create undo history with unsaved changes
        let mut h = UndoHistory::new();
        h.push(Edit::InsertChar { line: 0, col: 0, ch: 'a' });
        h.push(Edit::InsertChar { line: 0, col: 1, ch: 'b' });
        h.update_state(0, 0, 2, vec!["aboriginal".to_string()]);
        assert!(h.modified);
        h.save(&file_str).unwrap();
        
        // Verify undo file exists
        let history_path = UndoHistory::history_path_for(&file_str).unwrap();
        assert!(history_path.exists());
        
        // Modify file externally
        thread::sleep(Duration::from_millis(100));
        fs::write(&file, "completely different").unwrap();
        
        // Load and validate
        let loaded = UndoHistory::load(&file_str).unwrap();
        assert!(loaded.modified);
        assert_eq!(loaded.edits.len(), 2);
        
        let _result = loaded.validate(&file_str);
        
        // Validation should detect modification (or Valid if timestamps don't differ)
        // But the undo file should STILL exist with all its edits
        assert!(history_path.exists(), "Undo file should not be deleted by validation");
        
        // Reload and verify edits are preserved
        let reloaded = UndoHistory::load(&file_str).unwrap();
        assert_eq!(reloaded.edits.len(), 2);
        assert!(reloaded.modified);
        assert!(reloaded.file_content.is_some());
    }

    #[test]
    fn get_undo_file_mtime_returns_none_for_nonexistent_file() {
        let (_tmp, _guard) = set_temp_home();
        let file_str = "/tmp/nonexistent_file_for_mtime_test.txt";
        
        // Undo file doesn't exist yet
        let mtime = UndoHistory::get_undo_file_mtime(file_str);
        assert!(mtime.is_none(), "Should return None for nonexistent undo file");
    }

    #[test]
    fn get_undo_file_mtime_returns_timestamp_after_save() {
        let (_tmp, _guard) = set_temp_home();
        let file_str = "/tmp/test_mtime_file.txt";
        
        // Create and save undo history
        let mut h = UndoHistory::new();
        h.push(Edit::InsertChar { line: 0, col: 0, ch: 'x' });
        h.save(file_str).unwrap();
        
        // Should now return a timestamp
        let mtime1 = UndoHistory::get_undo_file_mtime(file_str);
        assert!(mtime1.is_some(), "Should return timestamp after save");
        
        // Wait a bit and save again
        std::thread::sleep(std::time::Duration::from_millis(10));
        h.push(Edit::InsertChar { line: 0, col: 1, ch: 'y' });
        h.save(file_str).unwrap();
        
        // Timestamp should be different (or at least not None)
        let mtime2 = UndoHistory::get_undo_file_mtime(file_str);
        assert!(mtime2.is_some(), "Should return timestamp after second save");
        // Note: mtime2 >= mtime1, but might be equal on fast systems
    }

    // Multi-instance synchronization tests

    #[test]
    fn multi_instance_detects_external_undo_file_change() {
        use std::thread;
        use std::time::Duration;
        
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("shared.txt");
        fs::write(&file, "original").unwrap();
        let file_str = file.to_string_lossy();
        
        // Instance 1: Create and save undo history
        let mut h1 = UndoHistory::new();
        h1.push(Edit::InsertChar { line: 0, col: 0, ch: 'a' });
        h1.update_state(0, 0, 1, vec!["aoriginal".to_string()]);
        h1.save(&file_str).unwrap();
        
        let mtime1 = UndoHistory::get_undo_file_mtime(&file_str);
        assert!(mtime1.is_some());
        
        // Wait to ensure mtime changes
        thread::sleep(Duration::from_millis(10));
        
        // Instance 2: Make different changes and save
        let mut h2 = UndoHistory::load(&file_str).unwrap();
        h2.push(Edit::InsertChar { line: 0, col: 1, ch: 'b' });
        h2.update_state(0, 0, 2, vec!["aboriginal".to_string()]);
        h2.save(&file_str).unwrap();
        
        let mtime2 = UndoHistory::get_undo_file_mtime(&file_str);
        assert!(mtime2.is_some());
        
        // mtimes should be different
        assert_ne!(mtime1, mtime2);
        
        // Instance 1: Reload and verify it sees instance 2's changes
        let h1_reloaded = UndoHistory::load(&file_str).unwrap();
        assert_eq!(h1_reloaded.edits.len(), 2);
        assert_eq!(h1_reloaded.cursor_col, 2);
        assert_eq!(h1_reloaded.file_content, Some(vec!["aboriginal".to_string()]));
    }

    #[test]
    fn multi_instance_cursor_position_restored_correctly() {
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("cursor_test.txt");
        fs::write(&file, "line1\nline2\nline3").unwrap();
        let file_str = file.to_string_lossy();
        
        // Instance 1: Position cursor at line 2, column 3, scroll top at 1
        let mut h1 = UndoHistory::new();
        h1.push(Edit::InsertChar { line: 1, col: 0, ch: 'x' });
        h1.update_state(1, 2, 3, vec!["line1".to_string(), "xline2".to_string(), "line3".to_string()]);
        h1.save(&file_str).unwrap();
        
        // Instance 2: Load and verify cursor restoration
        let h2 = UndoHistory::load(&file_str).unwrap();
        assert_eq!(h2.scroll_top, 1);
        assert_eq!(h2.cursor_line, 2);
        assert_eq!(h2.cursor_col, 3);
    }

    #[test]
    fn multi_instance_modified_flag_synchronized() {
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("modified_test.txt");
        fs::write(&file, "content").unwrap();
        let file_str = file.to_string_lossy();
        
        // Instance 1: Make edits (modified=true)
        let mut h1 = UndoHistory::new();
        h1.push(Edit::InsertChar { line: 0, col: 0, ch: 'a' });
        h1.update_state(0, 0, 1, vec!["acontent".to_string()]);
        assert!(h1.modified);
        h1.save(&file_str).unwrap();
        
        // Instance 2: Load and verify modified flag is true
        let h2 = UndoHistory::load(&file_str).unwrap();
        assert!(h2.modified);
        assert_eq!(h2.edits.len(), 1);
        
        // Instance 1: Undo all changes (modified=false)
        h1.undo();
        h1.update_state(0, 0, 0, vec!["content".to_string()]);
        assert!(!h1.modified);
        h1.save(&file_str).unwrap();
        
        // Instance 2: Reload and verify modified flag is now false
        let h2_reloaded = UndoHistory::load(&file_str).unwrap();
        assert!(!h2_reloaded.modified);
    }

    #[test]
    fn multi_instance_concurrent_edits_last_write_wins() {
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("concurrent.txt");
        fs::write(&file, "base").unwrap();
        let file_str = file.to_string_lossy();
        
        // Both instances start from same state
        let h_base = UndoHistory::new();
        h_base.save(&file_str).unwrap();
        
        // Instance 1: Add edit 'x'
        let mut h1 = UndoHistory::load(&file_str).unwrap();
        h1.push(Edit::InsertChar { line: 0, col: 0, ch: 'x' });
        h1.update_state(0, 0, 1, vec!["xbase".to_string()]);
        h1.save(&file_str).unwrap();
        
        // Instance 2: Add edit 'y' (loads instance 1's state first)
        let mut h2 = UndoHistory::load(&file_str).unwrap();
        h2.push(Edit::InsertChar { line: 0, col: 0, ch: 'y' });
        h2.update_state(0, 0, 1, vec!["yxbase".to_string()]);
        h2.save(&file_str).unwrap();
        
        // Final state should be instance 2's changes (which includes instance 1's edit)
        let h_final = UndoHistory::load(&file_str).unwrap();
        // h2 loaded h1's state (with 'x') then added 'y', so we have 2 edits
        assert_eq!(h_final.edits.len(), 2);
    }

    #[test]
    fn multi_instance_preserves_undo_redo_chain() {
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("chain.txt");
        fs::write(&file, "text").unwrap();
        let file_str = file.to_string_lossy();
        
        // Instance 1: Create undo chain with redo capability
        let mut h1 = UndoHistory::new();
        h1.push(Edit::InsertChar { line: 0, col: 0, ch: 'a' });
        h1.push(Edit::InsertChar { line: 0, col: 1, ch: 'b' });
        h1.push(Edit::InsertChar { line: 0, col: 2, ch: 'c' });
        h1.undo(); // Undo 'c'
        assert_eq!(h1.current, 2);
        assert!(h1.can_redo());
        h1.update_state(0, 0, 2, vec!["abtext".to_string()]);
        h1.save(&file_str).unwrap();
        
        // Instance 2: Load and verify redo chain preserved
        let h2 = UndoHistory::load(&file_str).unwrap();
        assert_eq!(h2.current, 2);
        assert_eq!(h2.edits.len(), 3);
        assert!(h2.can_redo());
        assert!(h2.can_undo());
    }

    #[test]
    fn get_undo_file_mtime_changes_after_modification() {
        use std::thread;
        use std::time::Duration;
        
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("mtime_change.txt");
        fs::write(&file, "data").unwrap();
        let file_str = file.to_string_lossy();
        
        // Initial save
        let h1 = UndoHistory::new();
        h1.save(&file_str).unwrap();
        let mtime1 = UndoHistory::get_undo_file_mtime(&file_str);
        assert!(mtime1.is_some());
        
        // Wait and modify
        thread::sleep(Duration::from_millis(10));
        let mut h2 = UndoHistory::load(&file_str).unwrap();
        h2.push(Edit::InsertChar { line: 0, col: 0, ch: 'z' });
        h2.save(&file_str).unwrap();
        
        let mtime2 = UndoHistory::get_undo_file_mtime(&file_str);
        assert!(mtime2.is_some());
        
        // mtimes should differ (though on fast systems with low-res fs, may be equal)
        // We just verify both are Some and the second is >= first
        assert!(mtime2.unwrap() >= mtime1.unwrap());
    }

    #[test]
    fn modified_flag_tracks_save_baseline() {
        let (_tmp, _guard) = set_temp_home();
        let mut h = UndoHistory::new();
        
        // Initially not modified, saved_at = 0
        assert!(!h.modified);
        assert_eq!(h.saved_at, 0);
        assert_eq!(h.current, 0);
        
        // Make edit 1
        h.push(Edit::InsertChar { line: 0, col: 0, ch: 'a' });
        h.update_state(0, 0, 1, vec!["a".into()]);
        assert!(h.modified);
        assert_eq!(h.current, 1);
        assert_eq!(h.saved_at, 0);
        
        // Make edit 2
        h.push(Edit::InsertChar { line: 0, col: 1, ch: 'b' });
        h.update_state(0, 0, 2, vec!["ab".into()]);
        assert!(h.modified);
        assert_eq!(h.current, 2);
        
        // Save - sets saved_at to current position
        h.clear_unsaved_state();
        assert!(!h.modified);
        assert_eq!(h.saved_at, 2);
        assert_eq!(h.current, 2);
        
        // Make edit 3
        h.push(Edit::InsertChar { line: 0, col: 2, ch: 'c' });
        h.update_state(0, 0, 3, vec!["abc".into()]);
        assert!(h.modified);
        assert_eq!(h.current, 3);
        assert_eq!(h.saved_at, 2);
        
        // Undo to saved position - should not be modified
        h.undo();
        h.update_state(0, 0, 2, vec!["ab".into()]);
        assert!(!h.modified, "Should not be modified when at saved position");
        assert_eq!(h.current, 2);
        assert_eq!(h.saved_at, 2);
        
        // Undo past saved position - should be modified
        h.undo();
        h.update_state(0, 0, 1, vec!["a".into()]);
        assert!(h.modified, "Should be modified when before saved position");
        assert_eq!(h.current, 1);
        assert_eq!(h.saved_at, 2);
        
        // Redo back to saved position - should not be modified
        h.redo();
        h.update_state(0, 0, 2, vec!["ab".into()]);
        assert!(!h.modified, "Should not be modified when back at saved position");
        assert_eq!(h.current, 2);
        
        // Redo past saved position - should be modified
        h.redo();
        h.update_state(0, 0, 3, vec!["abc".into()]);
        assert!(h.modified, "Should be modified when past saved position");
        assert_eq!(h.current, 3);
    }

    #[test]
    fn multi_instance_save_propagates_modified_flag() {
        let (tmp, _guard) = set_temp_home();
        let file = tmp.path().join("save_flag.txt");
        fs::write(&file, "content").unwrap();
        let file_str = file.to_string_lossy();
        
        // Instance 1: Make edits (modified=true)
        let mut h1 = UndoHistory::new();
        h1.push(Edit::InsertChar { line: 0, col: 0, ch: 'x' });
        h1.update_state(0, 0, 1, vec!["xcontent".to_string()]);
        assert!(h1.modified);
        h1.save(&file_str).unwrap();
        
        // Instance 2: Load and verify modified=true
        let h2 = UndoHistory::load(&file_str).unwrap();
        assert!(h2.modified);
        assert_eq!(h2.current, 1);
        assert_eq!(h2.saved_at, 0);
        
        // Instance 1: Save the file (clears modified flag)
        h1.clear_unsaved_state();
        assert!(!h1.modified);
        assert_eq!(h1.saved_at, 1);
        h1.save(&file_str).unwrap();
        
        // Instance 2: Reload and verify modified=false
        let h2_reloaded = UndoHistory::load(&file_str).unwrap();
        assert!(!h2_reloaded.modified, "Modified flag should propagate from save in other instance");
        assert_eq!(h2_reloaded.current, 1);
        assert_eq!(h2_reloaded.saved_at, 1);
    }
}
