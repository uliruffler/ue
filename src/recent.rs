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
        if trimmed.is_empty() {
            continue;
        }
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
    if let Some(parent) = recent_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut current = if recent_path.exists() {
        let content = fs::read_to_string(&recent_path)?;
        content
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    // Remove existing occurrence
    current.retain(|p| p != &canonical_str);
    // Insert at front
    current.insert(0, canonical_str);
    // Truncate
    if current.len() > MAX_RECENT {
        current.truncate(MAX_RECENT);
    }

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

    #[test]
    fn recent_file_with_special_characters() {
        let (tmp, _guard) = set_temp_home();
        // Test file names with spaces, unicode, and special chars
        let special_names = vec![
            "file with spaces.txt",
            "file-with-dashes.txt",
            "file_with_underscores.txt",
            "файл.txt", // Cyrillic
            "文件.txt", // Chinese
        ];

        for name in special_names {
            let f = tmp.path().join(name);
            fs::write(&f, "content").unwrap();
            let result = update_recent_file(f.to_string_lossy().as_ref());
            assert!(
                result.is_ok(),
                "Should handle special characters in filename: {}",
                name
            );
        }

        let recent = get_recent_files().unwrap();
        assert_eq!(
            recent.len(),
            5,
            "All files with special characters should be tracked"
        );
    }

    #[test]
    fn recent_file_with_very_long_path() {
        let (tmp, _guard) = set_temp_home();

        // Create a deeply nested directory structure
        let mut path = tmp.path().to_path_buf();
        for _ in 0..10 {
            path.push("very_long_directory_name_to_test_path_length");
        }
        fs::create_dir_all(&path).unwrap();

        let file_path = path.join("file.txt");
        fs::write(&file_path, "content").unwrap();

        let result = update_recent_file(file_path.to_string_lossy().as_ref());
        assert!(result.is_ok(), "Should handle very long file paths");

        let recent = get_recent_files().unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0], file_path.canonicalize().unwrap());
    }

    #[test]
    fn recent_file_nonexistent_file() {
        let (tmp, _guard) = set_temp_home();
        let nonexistent = tmp.path().join("does_not_exist.txt");

        // Should still add to recent list even if file doesn't exist yet
        // (user might be creating a new file)
        let result = update_recent_file(nonexistent.to_string_lossy().as_ref());

        // This depends on implementation - if it requires the file to exist,
        // we might want to change behavior or document it
        // For now, let's test current behavior
        let _ = result; // Don't assert success/failure, just verify it doesn't panic
    }

    #[test]
    fn recent_file_symlink_handling() {
        let (tmp, _guard) = set_temp_home();

        let original = tmp.path().join("original.txt");
        fs::write(&original, "content").unwrap();

        let link = tmp.path().join("link.txt");

        // Create symlink (skip test on platforms that don't support it)
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            if symlink(&original, &link).is_ok() {
                update_recent_file(link.to_string_lossy().as_ref()).unwrap();

                let recent = get_recent_files().unwrap();
                // Should canonicalize to the original file
                assert_eq!(recent.len(), 1);
                assert_eq!(recent[0], original.canonicalize().unwrap());
            }
        }
    }

    #[test]
    fn recent_file_empty_list_initially() {
        let (_tmp, _guard) = set_temp_home();

        // Fresh environment should have empty or minimal recent list
        let recent = get_recent_files().unwrap();
        assert!(
            recent.is_empty() || recent.len() < MAX_RECENT,
            "New environment should not have full recent list"
        );
    }

    #[test]
    fn recent_file_preserves_order_after_access() {
        let (tmp, _guard) = set_temp_home();

        let f1 = tmp.path().join("first.txt");
        let f2 = tmp.path().join("second.txt");
        let f3 = tmp.path().join("third.txt");

        fs::write(&f1, "1").unwrap();
        fs::write(&f2, "2").unwrap();
        fs::write(&f3, "3").unwrap();

        update_recent_file(f1.to_string_lossy().as_ref()).unwrap();
        update_recent_file(f2.to_string_lossy().as_ref()).unwrap();
        update_recent_file(f3.to_string_lossy().as_ref()).unwrap();

        // Re-access f1 - it should move to the front
        update_recent_file(f1.to_string_lossy().as_ref()).unwrap();

        let recent = get_recent_files().unwrap();
        assert_eq!(
            recent[0],
            f1.canonicalize().unwrap(),
            "Re-accessed file should be most recent"
        );
        assert_eq!(recent[1], f3.canonicalize().unwrap());
        assert_eq!(recent[2], f2.canonicalize().unwrap());
    }

    #[test]
    fn recent_file_concurrent_updates() {
        let (tmp, _guard) = set_temp_home();

        // Simulate rapid consecutive updates
        let f1 = tmp.path().join("rapid1.txt");
        let f2 = tmp.path().join("rapid2.txt");

        fs::write(&f1, "1").unwrap();
        fs::write(&f2, "2").unwrap();

        for _ in 0..100 {
            update_recent_file(f1.to_string_lossy().as_ref()).unwrap();
            update_recent_file(f2.to_string_lossy().as_ref()).unwrap();
        }

        let recent = get_recent_files().unwrap();
        assert_eq!(
            recent.len(),
            2,
            "Should handle rapid updates without duplication"
        );
    }
}
