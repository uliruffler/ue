use std::path::PathBuf;
use std::{fs, io};

const MAX_RECENT: usize = 50;

fn recent_list_path() -> io::Result<PathBuf> {
    let home = std::env::var("UE_TEST_HOME")
        .or_else(|_| std::env::var("HOME"))
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
    Ok(PathBuf::from(home).join(".ue").join("files.ue"))
}

pub(crate) fn get_recent_files() -> io::Result<Vec<PathBuf>> {
    let path = recent_list_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path)?;
    let mut result = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        result.push(PathBuf::from(trimmed));
    }
    Ok(result)
}

pub(crate) fn update_recent_file(file_path: &str) -> io::Result<()> {
    let path_buf = PathBuf::from(file_path);
    // Try canonicalize but fall back to original if fails (may not exist yet)
    let canonical = path_buf.canonicalize().unwrap_or(path_buf);
    let canonical_str = canonical.to_string_lossy().to_string();

    let recent_path = recent_list_path()?;
    if let Some(parent) = recent_path.parent() { fs::create_dir_all(parent)?; }

    let mut current = if recent_path.exists() {
        let content = fs::read_to_string(&recent_path)?;
        content.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect::<Vec<_>>()
    } else { Vec::new() };

    // Remove existing occurrence
    current.retain(|p| p != &canonical_str);
    // Insert at front
    current.insert(0, canonical_str);
    // Truncate
    if current.len() > MAX_RECENT { current.truncate(MAX_RECENT); }

    let serialized = current.join("\n");
    fs::write(&recent_path, serialized)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::set_temp_home;
    use std::fs;

    #[test]
    fn recent_file_updates_order() {
        let (tmp, _guard) = set_temp_home();
        let base = tmp.path();
        let f1 = base.join("a.txt");
        let f2 = base.join("b.txt");
        let f3 = base.join("c.txt");
        fs::write(&f1, "a").unwrap();
        fs::write(&f2, "b").unwrap();
        fs::write(&f3, "c").unwrap();

        update_recent_file(f1.to_string_lossy().as_ref()).unwrap();
        update_recent_file(f2.to_string_lossy().as_ref()).unwrap();
        update_recent_file(f3.to_string_lossy().as_ref()).unwrap();

        let recent = get_recent_files().unwrap();
        assert_eq!(recent[0], f3.canonicalize().unwrap());
        assert_eq!(recent[1], f2.canonicalize().unwrap());
        assert_eq!(recent[2], f1.canonicalize().unwrap());
    }

    #[test]
    fn recent_file_deduplicates() {
        let (tmp, _guard) = set_temp_home();
        let f1 = tmp.path().join("a.txt");
        fs::write(&f1, "a").unwrap();
        update_recent_file(f1.to_string_lossy().as_ref()).unwrap();
        update_recent_file(f1.to_string_lossy().as_ref()).unwrap();
        update_recent_file(f1.to_string_lossy().as_ref()).unwrap();
        let recent = get_recent_files().unwrap();
        assert_eq!(recent.len(), 1);
    }

    #[test]
    fn recent_file_truncates() {
        let (tmp, _guard) = set_temp_home();
        for i in 0..(MAX_RECENT + 10) {
            let f = tmp.path().join(format!("f{}.txt", i));
            fs::write(&f, "x").unwrap();
            update_recent_file(f.to_string_lossy().as_ref()).unwrap();
        }
        let recent = get_recent_files().unwrap();
        assert_eq!(recent.len(), MAX_RECENT);
    }
}

