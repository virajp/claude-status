//! Cross-cutting helpers, with no knowledge of any domain.

pub mod fmt;
pub mod json;
pub mod paths;
pub mod proc;
pub mod text;
pub mod time;

/// **The stderr chokepoint** — the fifth of the five filter surfaces.
///
/// Every **dynamic** line this binary writes to stderr goes through here,
/// sanitized. stderr
/// is a terminal like stdout, and the diagnostics carry exactly the things most
/// likely to be hostile: a cwd, a branch, a config value, an argv token, a regex out of a
/// config file, a segment id, or a panic message quoting any of them.
///
/// [`diag_report`] is the other half of the chokepoint, for static multi-line
/// text. Between them they are still the only two writers.
///
/// `narrate` in [`proc`] is the `--doctor`-gated caller; this is the one that
/// always writes. Callers pass the whole line, prefix included — the prefixes
/// are static, so filtering them costs nothing and leaves one rule instead of
/// two. The chokepoint rule exists because the per-write alternative was tried and missed
/// writers three times.
///
/// **One line out per call, deliberately.** This uses the *row* filter, which
/// strips newlines, not the report one that keeps them — so a multi-line panic
/// payload is collapsed onto a single line rather than keeping its shape. That
/// is a real readability cost and it is the right trade: a panic message quotes
/// whatever it panicked on, so letting it carry a newline would let a branch or
/// a config value forge a second `claude-status:` line on stderr. A stack shape
/// is worth less than a diagnostic a reader can trust the boundaries of.
pub fn diag(line: &str) {
    eprintln!("{}", text::sanitize(line));
}

/// [`diag`] for text that is deliberately many lines — today, `HELP` after an
/// unrecognised argument.
///
/// The same chokepoint with the *report* filter instead of the row one. The
/// pair mirrors [`text::sanitize`] and [`text::sanitize_report`] and exists for
/// the same reason those two do: `HELP` is fifty lines, and the row filter
/// would collapse it onto one.
///
/// **Static text only.** Keeping newlines is exactly what [`diag`]'s row filter
/// refuses to do, and it refuses for a reason — a value carrying one could
/// forge a second `claude-status:` line on stderr. So the argv tokens that
/// *provoke* this help go through [`diag`] one line at a time; only the
/// constant that follows them comes through here.
pub fn diag_report(text: &str) {
    eprint!("{}", text::sanitize_report(text));
}

/// Serialises the tests that mutate the process environment, and puts back
/// whatever they changed.
///
/// `std::env::set_var` is process-global and `cargo test` runs a binary's tests
/// on a thread pool, so two tests touching the same variable race — and one
/// that *unsets* `HOME` makes every other test that reads a home directory
/// flake. Every test that writes an environment variable takes this guard, and
/// so does every test that depends on one.
///
/// **Restoration is RAII, not a trailing statement.** A failed assertion
/// unwinds, and a `set_var` at the bottom of the test body does not run on that
/// path — so one failure would leave `HOME` unset and cascade into every test
/// that ran afterwards, which is the exact failure this type exists to prevent.
/// The original value is captured on first touch and put back on drop, whether
/// the test passed, failed or panicked.
///
/// Poisoning is ignored on purpose: a panicking test has already failed, and
/// propagating its poison would fail every later test for the wrong reason.
#[cfg(test)]
pub(crate) struct EnvGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    /// `(key, value before this guard first touched it)`. `None` means it was
    /// unset, and restoring means removing it again.
    saved: Vec<(String, Option<String>)>,
}

#[cfg(test)]
impl EnvGuard {
    fn remember(&mut self, key: &str) {
        if self.saved.iter().any(|(k, _)| k == key) {
            return; // Already captured — keep the ORIGINAL, not an interim value.
        }
        self.saved.push((key.to_string(), std::env::var(key).ok()));
    }

    pub(crate) fn set(&mut self, key: &str, value: &str) {
        self.remember(key);
        // SAFETY: the guard holds the lock for as long as it lives, so no other
        // test is reading or writing the environment concurrently.
        unsafe { std::env::set_var(key, value) };
    }

    pub(crate) fn unset(&mut self, key: &str) {
        self.remember(key);
        // SAFETY: as above.
        unsafe { std::env::remove_var(key) };
    }
}

#[cfg(test)]
impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.saved.drain(..) {
            // SAFETY: still holding the lock — it is dropped after this.
            match value {
                Some(value) => unsafe { std::env::set_var(&key, value) },
                None => unsafe { std::env::remove_var(&key) },
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn env_lock() -> EnvGuard {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    EnvGuard {
        _lock: LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
        saved: Vec::new(),
    }
}
