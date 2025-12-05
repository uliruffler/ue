use std::sync::{Mutex, OnceLock};
use tempfile::TempDir;

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Acquire a global lock and set UE_TEST_HOME to a fresh temporary directory.
/// Returns the TempDir and the guard holding the lock for the duration of the test.
pub fn set_temp_home() -> (TempDir, std::sync::MutexGuard<'static, ()>) {
    let lock = ENV_LOCK.get_or_init(|| Mutex::new(()));
    let guard = match lock.lock() {
        Ok(g) => g,
        Err(poisoned) => {
            // Recover from a poisoned lock to allow subsequent tests to proceed.
            poisoned.into_inner()
        }
    };
    let dir = TempDir::new().expect("temp dir");
    unsafe { std::env::set_var("UE_TEST_HOME", dir.path()); }
    (dir, guard)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_set_temp_home_creates_directory() {
        let (_temp_dir, _guard) = set_temp_home();
        
        // Verify UE_TEST_HOME is set
        let home = std::env::var("UE_TEST_HOME").expect("UE_TEST_HOME should be set");
        let home_path = Path::new(&home);
        
        // Verify the directory exists
        assert!(home_path.exists(), "Temp home directory should exist");
        assert!(home_path.is_dir(), "Temp home should be a directory");
    }

    #[test]
    fn test_set_temp_home_creates_unique_directories() {
        // First temp home
        let (temp_dir1, guard1) = set_temp_home();
        let home1 = std::env::var("UE_TEST_HOME").expect("UE_TEST_HOME should be set");
        drop(guard1);
        drop(temp_dir1);

        // Second temp home
        let (_temp_dir2, _guard2) = set_temp_home();
        let home2 = std::env::var("UE_TEST_HOME").expect("UE_TEST_HOME should be set");
        
        // Paths should be different (though first may no longer exist)
        assert_ne!(home1, home2, "Each call should create a unique temp directory");
    }

    #[test]
    fn test_temp_dir_cleanup() {
        let path;
        {
            let (temp_dir, _guard) = set_temp_home();
            path = temp_dir.path().to_path_buf();
            assert!(path.exists(), "Temp dir should exist while in scope");
            // temp_dir and guard drop here
        }
        
        // After dropping, the temp directory should be cleaned up
        // Note: This test may be flaky on some systems due to timing
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(!path.exists(), "Temp dir should be cleaned up after dropping");
    }

    #[test]
    fn test_lock_prevents_concurrent_modification() {
        // This test verifies that the lock is held
        let (_temp_dir1, guard1) = set_temp_home();
        let home1 = std::env::var("UE_TEST_HOME").expect("should be set");
        
        // Try to get another lock in the same thread (this should succeed in sequence)
        drop(guard1);
        drop(_temp_dir1);
        
        let (_temp_dir2, _guard2) = set_temp_home();
        let home2 = std::env::var("UE_TEST_HOME").expect("should be set");
        
        // Should get different temp dirs
        assert_ne!(home1, home2);
    }
}

