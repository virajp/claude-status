//! A whole-machine lock, so two refresh children never fetch at once.
//!
//! `<cache>.lock`, created with `O_CREAT|O_EXCL`. A lock younger than two
//! minutes means another refresh is live and this one exits; older than that,
//! its holder died and this one takes over.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How long a lock may sit before its holder is presumed dead.
pub const STALE_AFTER: Duration = Duration::from_secs(120);

/// Unlinks the lock on drop — the JS `finally`, and on unwind too.
pub struct LockGuard {
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub enum Acquired {
    /// The lock is ours.
    Held(LockGuard),
    /// Another refresh is running; its lock is this old.
    Contended { holder_age: Duration },
    /// The lock vanished mid-check, or could not be read.
    ///
    /// Returns without refreshing rather than retrying — faithful to the
    /// original, and the safe direction: a missed refresh costs one interval,
    /// a double fetch costs a rate limit.
    Indeterminate,
}

pub fn acquire(cache_path: &Path) -> Acquired {
    let path = lock_path(cache_path);

    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(_) => return Acquired::Held(LockGuard { path }),
        Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => return Acquired::Indeterminate,
        Err(_) => {}
    }

    // It exists. Whether it is alive is a question about its age.
    let Ok(age) = std::fs::metadata(&path).and_then(|m| m.modified()).and_then(|t| {
        SystemTime::now().duration_since(t).map_err(|_| std::io::Error::other("lock is in the future"))
    }) else {
        return Acquired::Indeterminate;
    };

    if age < STALE_AFTER {
        return Acquired::Contended { holder_age: age };
    }

    // The holder died. Take it over non-exclusively.
    match File::create(&path) {
        Ok(_) => Acquired::Held(LockGuard { path }),
        Err(_) => Acquired::Indeterminate,
    }
}

pub fn lock_path(cache_path: &Path) -> PathBuf {
    let mut path = cache_path.as_os_str().to_os_string();
    path.push(".lock");
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache_in(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("spend.json")
    }

    #[test]
    fn an_uncontended_lock_is_acquired_and_released_on_drop() {
        let dir = tempfile::TempDir::new().unwrap();
        let cache = cache_in(&dir);
        let path = lock_path(&cache);

        {
            let Acquired::Held(_guard) = acquire(&cache) else { panic!("should have been acquired") };
            assert!(path.exists(), "the lock exists while held");
        }
        assert!(!path.exists(), "and is unlinked on drop");
    }

    #[test]
    fn a_fresh_lock_blocks_a_second_holder() {
        let dir = tempfile::TempDir::new().unwrap();
        let cache = cache_in(&dir);

        let _held = acquire(&cache);
        match acquire(&cache) {
            Acquired::Contended { holder_age } => assert!(holder_age < STALE_AFTER),
            _ => panic!("a live lock must block"),
        }
    }

    #[test]
    fn a_stale_lock_is_taken_over() {
        let dir = tempfile::TempDir::new().unwrap();
        let cache = cache_in(&dir);
        let path = lock_path(&cache);

        std::fs::write(&path, "").unwrap();
        // Backdate it past the staleness threshold.
        let old = SystemTime::now() - Duration::from_secs(180);
        filetime_set(&path, old);

        assert!(matches!(acquire(&cache), Acquired::Held(_)), "a dead holder's lock is taken over");
    }

    #[test]
    fn the_guard_releases_on_unwind() {
        let dir = tempfile::TempDir::new().unwrap();
        let cache = cache_in(&dir);
        let path = lock_path(&cache);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let Acquired::Held(_guard) = acquire(&cache) else { panic!("not acquired") };
            panic!("boom");
        }));

        assert!(result.is_err());
        assert!(!path.exists(), "a panicking refresh still releases the lock");
    }

    /// `std::fs` cannot set mtime without a dependency, so go through the
    /// platform call the same way `filetime` would.
    fn filetime_set(path: &Path, time: SystemTime) {
        let file = OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(time).unwrap();
    }
}
