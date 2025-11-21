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
