//! Cross-cutting helpers, with no knowledge of any domain.

pub mod fmt;
pub mod json;
pub mod paths;
pub mod proc;
pub mod time;

/// Serialises the tests that mutate the process environment.
///
/// `std::env::set_var` is process-global and `cargo test` runs a binary's tests
/// on a thread pool, so two tests touching `HOME` race — and one that *unsets*
/// it makes every other test that reads a home directory flake. Every test that
/// writes an environment variable takes this lock, and so does every test that
/// depends on one; the lock is what makes "restore it afterwards" mean
/// anything.
///
/// Poisoning is ignored on purpose: a panicking test has already failed, and
/// propagating its poison would fail every later test for the wrong reason.
#[cfg(test)]
pub(crate) fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
