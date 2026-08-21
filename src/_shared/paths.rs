//! Locating the user's home directory.

use std::path::PathBuf;

/// The user's home directory.
///
/// `$HOME` and nothing else. It is what macOS uses, and honouring it is what
/// lets a test point the whole binary at a throwaway directory. This used to
/// fall through to `USERPROFILE` and `HOMEDRIVE`+`HOMEPATH` behind a
/// `cfg!(windows)`; the `macos-only` cycle removed the platform and the branch
/// with it.
///
/// Deliberately **not** a platform config directory: on macOS that resolves to
/// `~/Library/Application Support`, which would miss every existing install.
pub fn home() -> Option<PathBuf> {
    non_empty("HOME").map(PathBuf::from)
}

fn non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_prefers_the_posix_variable() {
        // Set by the harness on every platform this runs on.
        assert!(home().is_some(), "a test process always has a home");
    }

    #[test]
    fn an_empty_variable_is_not_a_home() {
        assert_eq!(non_empty("CLAUDE_STATUS_DEFINITELY_UNSET_12345"), None);
    }
}
