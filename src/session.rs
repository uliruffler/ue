use std::{fs, io, path::PathBuf};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum SessionMode {
    Editor,
    Selector,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LastSession {
    pub mode: SessionMode,
    pub file: Option<PathBuf>,
}

fn session_file_path() -> io::Result<PathBuf> {
    let home = std::env::var("UE_TEST_HOME")
        .or_else(|_| std::env::var("HOME"))
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
    Ok(PathBuf::from(home).join(".ue").join("last_session"))
}

pub fn load_last_session() -> io::Result<Option<LastSession>> {
    let path = session_file_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)?;
    let mut mode: Option<SessionMode> = None;
    let mut file: Option<PathBuf> = None;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("mode=") {
            mode = match rest.trim() {
                "editor" => Some(SessionMode::Editor),
                "selector" => Some(SessionMode::Selector),
                _ => None,
            };
        } else if let Some(rest) = line.strip_prefix("file=") {
            let p = PathBuf::from(rest.trim());
            file = Some(p);
        }
    }
    if let Some(m) = mode {
        Ok(Some(LastSession { mode: m, file }))
    } else {
        Ok(None)
    }
}

pub fn save_editor_session(file: &str) -> io::Result<()> {
    let path = session_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = format!("mode=editor\nfile={}\n", file);
    fs::write(path, data)?;
    Ok(())
}

pub fn save_selector_session() -> io::Result<()> {
    let path = session_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, "mode=selector\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::set_temp_home;
    use std::fs;

    #[test]
    fn save_and_load_editor_session() {
        let (_tmp, _guard) = set_temp_home();
        save_editor_session("/tmp/test.txt").unwrap();
        let loaded = load_last_session().unwrap();
        assert!(loaded.is_some());
        let ls = loaded.unwrap();
        assert_eq!(ls.mode, SessionMode::Editor);
        assert_eq!(ls.file.unwrap(), PathBuf::from("/tmp/test.txt"));
    }

    #[test]
    fn save_and_load_selector_session() {
        let (_tmp, _guard) = set_temp_home();
        save_selector_session().unwrap();
        let loaded = load_last_session().unwrap();
        assert!(loaded.is_some());
        let ls = loaded.unwrap();
        assert_eq!(ls.mode, SessionMode::Selector);
        assert!(ls.file.is_none());
    }

    #[test]
    fn load_missing_returns_none() {
        let (_tmp, _guard) = set_temp_home();
        let loaded = load_last_session().unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn corrupt_file_returns_none() {
        let (_tmp, _guard) = set_temp_home();
        let path = session_file_path().unwrap();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, "bad=stuff\nfile=/x\n").unwrap();
        let loaded = load_last_session().unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn session_file_with_special_characters_in_path() {
        let (tmp, _guard) = set_temp_home();
        let special_path = tmp.path().join("file with spaces & special.txt");
        fs::write(&special_path, "content").unwrap();

        save_editor_session(special_path.to_str().unwrap()).unwrap();
        let loaded = load_last_session().unwrap().unwrap();

        assert_eq!(loaded.mode, SessionMode::Editor);
        assert_eq!(loaded.file, Some(special_path));
    }

    #[test]
    fn session_overwrite_previous() {
        let (tmp, _guard) = set_temp_home();

        // Save editor session
        let file1 = tmp.path().join("first.txt");
        fs::write(&file1, "1").unwrap();
        save_editor_session(file1.to_str().unwrap()).unwrap();

        // Save selector session (should overwrite)
        save_selector_session().unwrap();

        let loaded = load_last_session().unwrap().unwrap();
        assert_eq!(loaded.mode, SessionMode::Selector);
        assert!(loaded.file.is_none());
    }

    #[test]
    fn session_with_unicode_path() {
        let (tmp, _guard) = set_temp_home();
        let unicode_path = tmp.path().join("文件.txt");
        fs::write(&unicode_path, "content").unwrap();

        save_editor_session(unicode_path.to_str().unwrap()).unwrap();
        let loaded = load_last_session().unwrap().unwrap();

        assert_eq!(loaded.file, Some(unicode_path));
    }

    #[test]
    fn session_file_path_creation() {
        let (_tmp, _guard) = set_temp_home();
        let path = session_file_path().unwrap();

        // Verify path is in expected location
        assert!(path.to_string_lossy().contains("last_session"));
    }

    #[test]
    fn session_empty_file_contents() {
        let (_tmp, _guard) = set_temp_home();
        let path = session_file_path().unwrap();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }

        // Write empty file
        fs::write(path, "").unwrap();
        let loaded = load_last_session().unwrap();
        assert!(loaded.is_none(), "Empty session file should return None");
    }

    #[test]
    fn session_malformed_mode() {
        let (_tmp, _guard) = set_temp_home();
        let path = session_file_path().unwrap();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }

        // Write invalid mode
        fs::write(path, "mode=invalid\nfile=/test.txt\n").unwrap();
        let loaded = load_last_session().unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn session_persists_nonexistent_file() {
        let (tmp, _guard) = set_temp_home();
        let nonexistent = tmp.path().join("does_not_exist.txt");
        let nonexistent_str = nonexistent.to_string_lossy().to_string();

        // Save session with non-existent file
        save_editor_session(&nonexistent_str).unwrap();

        // Load session - should work even though file doesn't exist
        let session = load_last_session().unwrap().unwrap();
        assert_eq!(session.mode, SessionMode::Editor);
        assert_eq!(session.file.unwrap().to_string_lossy(), nonexistent_str);
    }

    #[test]
    fn session_editor_to_selector_transition() {
        let (_tmp, _guard) = set_temp_home();

        // Start in editor mode
        save_editor_session("/tmp/file1.txt").unwrap();
        let session1 = load_last_session().unwrap().unwrap();
        assert_eq!(session1.mode, SessionMode::Editor);

        // Switch to selector mode
        save_selector_session().unwrap();
        let session2 = load_last_session().unwrap().unwrap();
        assert_eq!(session2.mode, SessionMode::Selector);
    }

    #[test]
    fn session_selector_to_editor_transition() {
        let (_tmp, _guard) = set_temp_home();

        // Start in selector mode
        save_selector_session().unwrap();
        let session1 = load_last_session().unwrap().unwrap();
        assert_eq!(session1.mode, SessionMode::Selector);

        // Switch to editor mode
        save_editor_session("/tmp/file2.txt").unwrap();
        let session2 = load_last_session().unwrap().unwrap();
        assert_eq!(session2.mode, SessionMode::Editor);
        assert_eq!(
            session2.file.unwrap().to_string_lossy(),
            "/tmp/file2.txt"
        );
    }
}
