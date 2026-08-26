//! Reading the OAuth token the usage endpoint needs.
//!
//! **The token is never logged, at any verbosity.** `--debug` reports only
//! *where* it was found. `Credentials` therefore has a hand-written `Debug`
//! that redacts it, so no future `{:?}` can leak it by accident.

use std::time::Duration;

use serde_json::Value;

use crate::_shared::proc::{Deadline, run_bounded};
use crate::json::read_json_file;

/// The keychain entry Claude Code writes on macOS.
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

/// The first keychain read may prompt the user, so this is generous — it is
/// **not** the 250 ms git budget, and it never runs on the render path.
const KEYCHAIN_TIMEOUT_MS: u64 = 5_000;

#[derive(Clone, PartialEq)]
pub struct Credentials {
    pub token: String,
    /// The seat's plan, which the cache records beside the figures.
    pub plan: Option<String>,
    /// Where this came from, for `--debug` to report.
    pub source: Source,
}

/// Redacted on purpose. The token must not be printable by accident.
impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("token", &"<redacted>")
            .field("plan", &self.plan)
            .field("source", &self.source)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    File,
    Keychain,
}

impl Source {
    pub fn describe(self) -> &'static str {
        match self {
            Self::File => "~/.claude/.credentials.json",
            Self::Keychain => "keychain \"Claude Code-credentials\"",
        }
    }
}

/// The credentials file, then — on macOS only — the keychain.
pub fn load() -> Option<Credentials> {
    from_file().or_else(from_keychain)
}

fn from_file() -> Option<Credentials> {
    let path = crate::_shared::paths::home()?.join(".claude").join(".credentials.json");
    parse(&read_json_file(&path)?, Source::File)
}

/// The `security` keychain lookup.
///
/// The `target_os` guard is kept even though the only published targets are
/// macOS: this is a *capability* check, not a platform-support statement, and
/// it is what keeps a source build on another OS from shelling out to a command
/// that is not there. Why the *shipped* set is macOS-only is in
/// `docs/decisions.md`.
fn from_keychain() -> Option<Credentials> {
    if !cfg!(target_os = "macos") {
        return None;
    }

    let out = run_bounded(
        "security",
        &["find-generic-password", "-s", KEYCHAIN_SERVICE, "-w"],
        std::path::Path::new("."),
        Deadline::in_ms(KEYCHAIN_TIMEOUT_MS),
    )?;

    parse(&serde_json::from_str(out.trim()).ok()?, Source::Keychain)
}

/// Both sources carry the same document.
fn parse(value: &Value, source: Source) -> Option<Credentials> {
    let oauth = value.get("claudeAiOauth")?;
    let token = oauth.get("accessToken")?.as_str().filter(|t| !t.is_empty())?;

    Some(Credentials {
        token: token.to_string(),
        plan: oauth.get("subscriptionType").and_then(Value::as_str).map(str::to_string),
        source,
    })
}

/// How long the keychain probe is allowed, exposed for the tests.
pub fn keychain_timeout() -> Duration {
    Duration::from_millis(KEYCHAIN_TIMEOUT_MS)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn it_reads_the_token_and_the_plan() {
        let doc = json!({ "claudeAiOauth": { "accessToken": "sk-secret", "subscriptionType": "team" } });
        let creds = parse(&doc, Source::File).unwrap();
        assert_eq!(creds.token, "sk-secret");
        assert_eq!(creds.plan.as_deref(), Some("team"));
        assert_eq!(creds.source, Source::File);
    }

    #[test]
    fn a_document_without_a_token_does_not_qualify() {
        // So the caller falls through to the next source rather than trying to
        // authenticate with nothing.
        assert_eq!(parse(&json!({ "claudeAiOauth": { "subscriptionType": "team" } }), Source::File), None);
        assert_eq!(parse(&json!({ "claudeAiOauth": { "accessToken": "" } }), Source::File), None);
        assert_eq!(parse(&json!({}), Source::File), None);
        assert_eq!(parse(&json!(null), Source::File), None);
    }

    #[test]
    fn a_missing_plan_is_absent_rather_than_fatal() {
        let creds = parse(&json!({ "claudeAiOauth": { "accessToken": "t" } }), Source::File).unwrap();
        assert_eq!(creds.plan, None);
    }

    #[test]
    fn debug_never_renders_the_token() {
        let creds = parse(&json!({ "claudeAiOauth": { "accessToken": "sk-verysecret" } }), Source::File).unwrap();
        let rendered = format!("{creds:?}");
        assert!(!rendered.contains("sk-verysecret"), "the token leaked into Debug: {rendered}");
        assert!(rendered.contains("<redacted>"));
    }

    #[test]
    fn the_keychain_probe_is_gated_on_macos() {
        // The point is the gate, not the result: on a non-macOS host this must
        // not spawn anything at all.
        if !cfg!(target_os = "macos") {
            assert_eq!(from_keychain(), None);
        }
        assert_eq!(keychain_timeout(), Duration::from_millis(5_000), "not the 250ms git budget");
    }
}
