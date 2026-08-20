//! Locating the user's home directory, on every platform Claude Code runs on.

use std::path::PathBuf;

/// The user's home directory.
///
/// `HOME` first, because it is what Unix uses and it is also what Git Bash and
/// most POSIX-flavoured Windows shells set — and because honouring it lets a
/// test point the whole binary at a throwaway directory. Then the native
/// Windows variables.
///
/// Deliberately **not** a platform config directory: on macOS that resolves to
/// `~/Library/Application Support` and would miss every existing install, and
/// on Windows it would resolve to `%APPDATA%`, which is not where the installer
/// writes.
pub fn home() -> Option<PathBuf> {
    if let Some(home) = non_empty("HOME") {
        return Some(PathBuf::from(home));
    }

    if cfg!(windows) {
        if let Some(profile) = non_empty("USERPROFILE") {
            return Some(PathBuf::from(profile));
        }
        // The last resort on a domain-joined machine with no USERPROFILE.
        if let (Some(drive), Some(path)) = (non_empty("HOMEDRIVE"), non_empty("HOMEPATH")) {
            return Some(PathBuf::from(format!("{drive}{path}")));
        }
    }

    None
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
