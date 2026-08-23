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
    fn home_reads_the_environment() {
        // Takes the lock even though it only reads: the tests that *unset*
        // `HOME` would otherwise make this one flake.
        let _guard = crate::_shared::env_lock();

        // Asserted against `$HOME` itself, not merely `is_some()`. The doc
        // comment above warns against substituting a platform config directory,
        // and on macOS `dirs::config_dir()` is `~/Library/Application Support`
        // — which is `Some`, so an `is_some()` assertion would wave through
        // exactly the substitution the warning exists to prevent.
        let expected = std::env::var("HOME").expect("a test process always has $HOME");
        assert_eq!(home(), Some(PathBuf::from(&expected)));
        assert!(!expected.contains("Application Support"), "$HOME is not a platform config directory");
    }

    #[test]
    fn an_unset_variable_is_not_a_home() {
        assert_eq!(non_empty("CLAUDE_STATUS_DEFINITELY_UNSET_12345"), None);
    }

    #[test]
    fn an_empty_variable_is_not_a_home() {
        // The `.filter(!is_empty)` is the only branch left in this file, and an
        // *unset* variable short-circuits on `.ok()` without ever reaching it —
        // so that test alone would stay green if the filter were deleted, while
        // `HOME=""` resolved to `Some("")` and every caller joined its paths
        // onto nothing.
        //
        // The guard serialises against every other env-touching test and puts
        // the variable back on drop — including when an assertion below fails.
        let mut env = crate::_shared::env_lock();
        const KEY: &str = "CLAUDE_STATUS_EMPTY_HOME_PROBE";
        env.set(KEY, "");
        assert_eq!(non_empty(KEY), None, "an empty value is not a value");
        env.set(KEY, "/somewhere");
        assert_eq!(non_empty(KEY).as_deref(), Some("/somewhere"));
    }
}
