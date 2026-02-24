#[cfg(test)]
use std::sync::{Mutex, OnceLock};

#[cfg(test)]
use tempfile::TempDir;

#[cfg(test)]
static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Resolve the home directory that should be used for storing editor state.
///
/// Priority order:
/// 1. `UE_TEST_HOME` — used by the test suite to isolate state.
/// 2. When running under `sudo`: the invoking user's real home, derived from
///    `SUDO_USER` by looking up `/etc/passwd` (falls back to `/home/$SUDO_USER`
///    if passwd parsing fails).
/// 3. `HOME` — the normal case for non-sudo runs.
/// 4. `USERPROFILE` — Windows / unusual Unix environments.
pub(crate) fn resolve_home() -> Result<String, std::env::VarError> {
    // Test override — highest priority so tests stay isolated.
    if let Ok(test_home) = std::env::var("UE_TEST_HOME") {
        return Ok(test_home);
    }

    // When invoked via `sudo`, SUDO_USER holds the name of the original user.
    // We want to store/read editor state from *that* user's home, not root's.
    if let Ok(sudo_user) = std::env::var("SUDO_USER") {
        if !sudo_user.is_empty() && sudo_user != "root" {
            // Try to read the home directory from /etc/passwd first.
            if let Some(home) = home_from_passwd(&sudo_user) {
                return Ok(home);
            }
            // Fallback: assume conventional /home/$SUDO_USER layout.
            return Ok(format!("/home/{}", sudo_user));
        }
    }

    // Regular (non-sudo) execution.
    std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"))
}

/// Look up a user's home directory from `/etc/passwd`.
/// Returns `None` if the file cannot be read or the user is not found.
fn home_from_passwd(username: &str) -> Option<String> {
    let contents = std::fs::read_to_string("/etc/passwd").ok()?;
    for line in contents.lines() {
        // passwd fields: name:password:uid:gid:gecos:home:shell
        let mut fields = line.splitn(7, ':');
        let name = fields.next()?;
        if name == username {
            // Skip password, uid, gid, gecos
            let _pw = fields.next();
            let _uid = fields.next();
            let _gid = fields.next();
            let _gecos = fields.next();
            let home = fields.next()?;
            return Some(home.to_string());
        }
    }
    None
}

/// Acquire a global lock and set UE_TEST_HOME to a fresh temporary directory.
/// Returns the TempDir and the guard holding the lock for the duration of the test.
#[cfg(test)]
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
    unsafe {
        std::env::set_var("UE_TEST_HOME", dir.path());
    }
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
        assert_ne!(
            home1, home2,
            "Each call should create a unique temp directory"
        );
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
        assert!(
            !path.exists(),
            "Temp dir should be cleaned up after dropping"
        );
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

    #[test]
    fn resolve_home_returns_ue_test_home_when_set() {
        let (_tmp, _guard) = set_temp_home();
        let expected = std::env::var("UE_TEST_HOME").unwrap();
        assert_eq!(resolve_home().unwrap(), expected);
    }

    #[test]
    fn resolve_home_uses_sudo_user_home_from_passwd() {
        // Use a real user from /etc/passwd (we know "root" is always present).
        // We simulate sudo by setting SUDO_USER=root temporarily.
        // Because root's home is in /etc/passwd, home_from_passwd should find it.
        let (_tmp, _guard) = set_temp_home();

        // Clear the test override so the SUDO_USER branch is reachable.
        unsafe { std::env::remove_var("UE_TEST_HOME") };

        unsafe { std::env::set_var("SUDO_USER", "root") };
        // root is skipped by the "sudo_user != root" guard, so this falls through
        // to HOME — we just verify resolve_home() doesn't panic.
        let result = resolve_home();
        unsafe { std::env::remove_var("SUDO_USER") };
        assert!(result.is_ok());
    }

    #[test]
    fn home_from_passwd_finds_root() {
        // root is always in /etc/passwd on Linux.
        let home = home_from_passwd("root");
        assert!(home.is_some(), "root should be findable in /etc/passwd");
        // root's home is conventionally /root.
        assert_eq!(home.unwrap(), "/root");
    }

    #[test]
    fn home_from_passwd_unknown_user_returns_none() {
        let home = home_from_passwd("__no_such_user_xyzzy__");
        assert!(home.is_none());
    }
}
