use std::{fs, io, path::PathBuf};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum SessionMode { Editor, Selector }

#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct LastSession {
    pub(crate) mode: SessionMode,
    pub(crate) file: Option<PathBuf>,
}

fn session_file_path() -> io::Result<PathBuf> {
    let home = std::env::var("UE_TEST_HOME")
        .or_else(|_| std::env::var("HOME"))
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
    Ok(PathBuf::from(home).join(".ue").join("last_session"))
}

pub(crate) fn load_last_session() -> io::Result<Option<LastSession>> {
    let path = session_file_path()?;
    if !path.exists() { return Ok(None); }
    let content = fs::read_to_string(&path)?;
    let mut mode: Option<SessionMode> = None;
    let mut file: Option<PathBuf> = None;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
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
    if let Some(m) = mode { Ok(Some(LastSession { mode: m, file })) } else { Ok(None) }
}

pub(crate) fn save_editor_session(file: &str) -> io::Result<()> {
    let path = session_file_path()?;
    if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
    let data = format!("mode=editor\nfile={}\n", file);
    fs::write(path, data)?;
    Ok(())
}

pub(crate) fn save_selector_session() -> io::Result<()> {
    let path = session_file_path()?;
    if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
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
        if let Some(parent) = path.parent() { fs::create_dir_all(parent).unwrap(); }
        fs::write(path, "bad=stuff\nfile=/x\n").unwrap();
        let loaded = load_last_session().unwrap();
        assert!(loaded.is_none());
    }
}

