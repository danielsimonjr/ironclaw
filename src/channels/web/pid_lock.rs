//! PID-based lock for the web gateway.
//!
//! Ensures only one gateway instance runs at a time by maintaining a PID file
//! at `~/.ironclaw/gateway.pid`. The lock checks whether the recorded process
//! is still alive before claiming ownership.

use std::fs;
use std::path::PathBuf;

/// A PID-based file lock for the gateway process.
///
/// On acquisition, writes the current process ID to the lock file. On drop,
/// the lock file is removed (best-effort). Callers can also check whether
/// an existing lock is held by a live process without acquiring it.
#[derive(Debug)]
pub struct PidLock {
    /// Path to the PID lock file.
    lock_path: PathBuf,
    /// PID that owns this lock (the current process when acquired).
    pid: u32,
}

/// Errors that can occur during PID lock operations.
#[derive(Debug, thiserror::Error)]
pub enum PidLockError {
    #[error("Lock already held by running process (PID: {pid})")]
    AlreadyLocked { pid: u32 },

    #[error("Failed to create lock directory {path}: {reason}")]
    DirectoryCreationFailed { path: String, reason: String },

    #[error("Failed to write lock file {path}: {reason}")]
    WriteFailed { path: String, reason: String },

    #[error("Failed to read lock file {path}: {reason}")]
    ReadFailed { path: String, reason: String },

    #[error("Failed to remove lock file {path}: {reason}")]
    RemoveFailed { path: String, reason: String },
}

impl PidLock {
    /// Acquire the PID lock at the given path.
    ///
    /// If a lock file already exists, checks whether the recorded process is
    /// still running. If the process is alive, returns [`PidLockError::AlreadyLocked`].
    /// If the process is dead (stale lock), removes the old file and proceeds.
    ///
    /// On success, writes the current process ID to the lock file and returns
    /// the lock handle. Dropping the handle removes the lock file.
    pub fn acquire(path: impl Into<PathBuf>) -> Result<Self, PidLockError> {
        let lock_path = path.into();

        // Check for an existing lock
        if let Some(existing_pid) = Self::read_pid_from(&lock_path) {
            if is_process_alive(existing_pid) {
                return Err(PidLockError::AlreadyLocked { pid: existing_pid });
            }
            // Stale lock file -- remove it
            let _ = fs::remove_file(&lock_path);
        }

        // Ensure parent directory exists
        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent).map_err(|e| PidLockError::DirectoryCreationFailed {
                path: parent.display().to_string(),
                reason: e.to_string(),
            })?;
        }

        let pid = std::process::id();
        fs::write(&lock_path, pid.to_string()).map_err(|e| PidLockError::WriteFailed {
            path: lock_path.display().to_string(),
            reason: e.to_string(),
        })?;

        Ok(Self { lock_path, pid })
    }

    /// Release the lock by removing the PID file.
    ///
    /// This is also called automatically on [`Drop`], but callers may invoke it
    /// explicitly for deterministic cleanup and error handling.
    pub fn release(&self) -> Result<(), PidLockError> {
        if self.lock_path.exists() {
            fs::remove_file(&self.lock_path).map_err(|e| PidLockError::RemoveFailed {
                path: self.lock_path.display().to_string(),
                reason: e.to_string(),
            })?;
        }
        Ok(())
    }

    /// Check whether the lock file exists and the recorded process is still alive.
    pub fn is_locked(&self) -> bool {
        Self::read_pid_from(&self.lock_path).is_some_and(is_process_alive)
    }

    /// Read the PID from the lock file, if it exists and is parseable.
    pub fn read_pid(&self) -> Option<u32> {
        Self::read_pid_from(&self.lock_path)
    }

    /// Return the path to the lock file.
    pub fn lock_path(&self) -> &PathBuf {
        &self.lock_path
    }

    /// Return the PID that owns this lock.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Return the default gateway PID lock path (`~/.ironclaw/gateway.pid`).
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".ironclaw")
            .join("gateway.pid")
    }

    // -- internal helpers --

    /// Read and parse a PID from the given file path.
    fn read_pid_from(path: &PathBuf) -> Option<u32> {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
    }
}

impl Drop for PidLock {
    fn drop(&mut self) {
        // Best-effort cleanup; ignore errors.
        let _ = fs::remove_file(&self.lock_path);
    }
}

/// Check whether a process with the given PID is currently running.
fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // `kill -0` checks for process existence without sending a signal.
        std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .is_ok_and(|s| s.success())
    }

    #[cfg(not(unix))]
    {
        let _ = pid;
        // On non-Unix platforms we cannot reliably check; assume not alive
        // so that stale locks are cleaned up.
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Helper to create a temporary lock file path that does not clash with
    /// other tests running in parallel.
    fn temp_lock_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("ironclaw_pid_lock_tests");
        let _ = fs::create_dir_all(&dir);
        dir.join(format!("{}.pid", name))
    }

    /// Clean up after a test.
    fn cleanup(path: &PathBuf) {
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_acquire_and_release() {
        let path = temp_lock_path("acquire_release");
        cleanup(&path);

        let lock = PidLock::acquire(path.clone()).expect("should acquire lock");
        assert!(path.exists(), "lock file should exist after acquire");
        assert_eq!(lock.pid(), std::process::id());

        lock.release().expect("should release lock");
        assert!(!path.exists(), "lock file should be removed after release");
    }

    #[test]
    fn test_drop_removes_lock_file() {
        let path = temp_lock_path("drop_cleanup");
        cleanup(&path);

        {
            let _lock = PidLock::acquire(path.clone()).expect("should acquire lock");
            assert!(path.exists());
        }
        // After drop, file should be gone.
        assert!(!path.exists(), "lock file should be removed on drop");
    }

    #[test]
    fn test_acquire_fails_when_current_process_holds_lock() {
        let path = temp_lock_path("already_locked");
        cleanup(&path);

        // Manually write our own PID to simulate a running holder.
        let _ = fs::create_dir_all(path.parent().unwrap());
        fs::write(&path, std::process::id().to_string()).unwrap();

        let result = PidLock::acquire(path.clone());
        assert!(result.is_err(), "should fail when live process holds lock");

        match result.unwrap_err() {
            PidLockError::AlreadyLocked { pid } => {
                assert_eq!(pid, std::process::id());
            }
            other => panic!("expected AlreadyLocked, got: {}", other),
        }

        cleanup(&path);
    }

    #[test]
    fn test_acquire_succeeds_with_stale_lock() {
        let path = temp_lock_path("stale_lock");
        cleanup(&path);

        // Write a PID that (almost certainly) does not correspond to a running process.
        let stale_pid: u32 = 4_294_967; // unlikely to be a live PID
        let _ = fs::create_dir_all(path.parent().unwrap());
        fs::write(&path, stale_pid.to_string()).unwrap();

        let lock = PidLock::acquire(path.clone()).expect("should acquire after stale lock");
        assert_eq!(lock.pid(), std::process::id());

        // Cleanup via drop
        drop(lock);
        assert!(!path.exists());
    }

    #[test]
    fn test_read_pid() {
        let path = temp_lock_path("read_pid");
        cleanup(&path);

        let lock = PidLock::acquire(path.clone()).expect("should acquire lock");
        let read = lock.read_pid();
        assert_eq!(read, Some(std::process::id()));

        drop(lock);
    }

    #[test]
    fn test_read_pid_no_file() {
        let path = temp_lock_path("read_pid_no_file");
        cleanup(&path);

        // Construct a lock struct directly to test read_pid on a missing file.
        // We cannot use acquire because the file won't exist, so build manually.
        let lock = PidLock {
            lock_path: path.clone(),
            pid: 0,
        };
        assert_eq!(lock.read_pid(), None);

        // Prevent drop from failing
        std::mem::forget(lock);
    }

    #[test]
    fn test_is_locked_with_live_process() {
        let path = temp_lock_path("is_locked_live");
        cleanup(&path);

        let lock = PidLock::acquire(path.clone()).expect("should acquire lock");
        assert!(lock.is_locked(), "should report locked for current process");

        drop(lock);
    }

    #[test]
    fn test_is_locked_with_no_file() {
        let path = temp_lock_path("is_locked_none");
        cleanup(&path);

        let lock = PidLock {
            lock_path: path.clone(),
            pid: 0,
        };
        assert!(!lock.is_locked(), "should report not locked when no file");

        std::mem::forget(lock);
    }

    #[test]
    fn test_default_path() {
        let path = PidLock::default_path();
        assert!(
            path.ends_with(".ironclaw/gateway.pid"),
            "default path should end with .ironclaw/gateway.pid, got: {}",
            path.display()
        );
    }

    #[test]
    fn test_lock_path_accessor() {
        let path = temp_lock_path("lock_path_accessor");
        cleanup(&path);

        let lock = PidLock::acquire(path.clone()).expect("should acquire lock");
        assert_eq!(lock.lock_path(), &path);

        drop(lock);
    }
}
