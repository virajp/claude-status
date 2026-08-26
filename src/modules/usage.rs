//! The usage mirror — a hard contract with `ai-plugins`.
//!
//! **This is not internal.** Context-window and rate-limit figures arrive only
//! on the statusline payload, never on hook stdin, so every main-bar render
//! mirrors them to a session-keyed file that vwf's `PostToolUse` context-cap
//! hook reads. The field names, the file layout and the env var name are all
//! part of the contract; changing any of them silently disables that hook.
//!
//! **The contract is `docs/usage-mirror-contract.md`.** Read it before changing
//! anything in this file: the consumer lives in another repository, so the
//! tests below prove only that *this* side still writes what it promised — they
//! cannot fail when the promise itself is broken.
//!
//! Best-effort throughout: a failure here must never affect the rendered line.

use std::path::PathBuf;

use serde_json::{Map, Value, json};

use crate::json::write_json_atomic;
use crate::payload::MainFacts;

/// The env var that enables the mirror, under this repo's own name.
pub const USAGE_DIR_ENV: &str = "CLAUDE_STATUS_USAGE_DIR";

/// The name `ai-plugins` exports and `context-caps.js` reads. Still honoured on
/// **both** sides so the variable can migrate without breaking a machine that
/// is still running the JS hook, which only knows this one. Phase 5 drops it.
pub const LEGACY_USAGE_DIR_ENV: &str = "AI_PLUGINS_USAGE_DIR";

/// The usage directory, new name first. Both readers — the mirror writer and
/// the caps hook — resolve it through here, so the two can never disagree
/// about which variable won.
pub fn usage_dir_from_env() -> Option<String> {
    // Emptiness is filtered per-arm, not once at the end: `var()` yields
    // `Ok("")` for an empty variable, so a trailing filter would see `Some("")`
    // from the new name, never try the legacy one, and only then discard it —
    // letting an emptied new variable mask a valid legacy one.
    let non_empty = |key: &str| std::env::var(key).ok().filter(|dir| !dir.is_empty());

    non_empty(USAGE_DIR_ENV).or_else(|| non_empty(LEGACY_USAGE_DIR_ENV))
}

/// Writes `<dir>/<session_id>.json`, or does nothing.
///
/// Runs before anything that can fail, and is gated on neither the layout nor
/// git — a broken config or a slow repo must not cost the caps hook its data.
pub fn mirror(facts: &MainFacts, usage_dir: Option<&str>, session_id: Option<&str>) {
    let (Some(dir), Some(session_id)) = (usage_dir.filter(|d| !d.is_empty()), session_id.filter(|s| !s.is_empty()))
    else {
        return;
    };

    // `<session>.state.json` is `context-caps.js`'s own file in the same
    // directory. A session id would have to contain a literal ".state" to
    // collide, but the two names are a contract, so assert the shape here.
    debug_assert!(!format!("{session_id}.json").ends_with(".state.json"));

    // No `$HOME` to expand against means no directory, which means no mirror.
    // Writing to a relative path instead would put it wherever Claude Code was
    // launched from, which is nobody's idea of a cache.
    let Some(dir) = expand_home(dir) else {
        return;
    };
    let _ = write_json_atomic(&dir.join(format!("{session_id}.json")), &document(facts, session_id));
}

/// The nine keys, in order. `serde_json`'s `preserve_order` is what keeps them
/// in it.
///
/// Two shapes are load-bearing:
/// - `resets_at` is mirrored **raw**, not normalised. The consumer does its own
///   seconds/millis/ISO discrimination, and an ISO string must stay one.
/// - `ctxSize` is **absent** when the payload carried no `context_window_size`,
///   while the other six value fields — `ctxPct`, `ctxUsed` and the four
///   rate-limit ones — are present as `null`. That asymmetry is what the
///   consumer was written against.
fn document(facts: &MainFacts, session_id: &str) -> Value {
    let mut doc = Map::new();
    doc.insert("sessionId".into(), json!(session_id));
    doc.insert("ts".into(), json!(facts.now_ms));
    doc.insert("ctxPct".into(), number(facts.ctx_pct));
    doc.insert("ctxUsed".into(), number(facts.ctx_used));
    if let Some(size) = facts.ctx_size {
        doc.insert("ctxSize".into(), number(Some(size)));
    }
    doc.insert("fiveHourPct".into(), number(facts.five_hour.used_pct));
    doc.insert("fiveHourResetsAt".into(), facts.five_hour.resets_at.clone().unwrap_or(Value::Null));
    doc.insert("sevenDayPct".into(), number(facts.seven_day.used_pct));
    doc.insert("sevenDayResetsAt".into(), facts.seven_day.resets_at.clone().unwrap_or(Value::Null));
    Value::Object(doc)
}

/// Writes a whole number without a `.0`, so `26` mirrors as `26` rather than
/// `26.0` — the consumer compares these against thresholds.
fn number(v: Option<f64>) -> Value {
    match v {
        None => Value::Null,
        Some(n) if !n.is_finite() => Value::Null,
        Some(n) if n.fract() == 0.0 && n.abs() < 9e15 => json!(n as i64),
        Some(n) => json!(n),
    }
}

/// Expands a leading `~`, `$HOME` or `${HOME}`.
///
/// Deliberately loose, matching the old implementation: `${HOME` and `$HOME}`
/// expand too. Claude Code may or may not have expanded the value before
/// exporting it, so every spelling arrives in the wild.
pub(crate) fn expand_home(dir: &str) -> Option<PathBuf> {
    const PREFIXES: [&str; 5] = ["${HOME}", "${HOME", "$HOME}", "$HOME", "~"];

    let Some(prefix) = PREFIXES.into_iter().find(|p| dir.starts_with(p)) else {
        // Nothing to expand — the value stands as given.
        return Some(PathBuf::from(dir));
    };

    // A value that *asks* for the home directory and cannot get one resolves to
    // **nothing**, never to the unexpanded text: `~/x` taken literally is a
    // relative path, and the caller would then write into whatever directory it
    // happened to be started in, believing it had written into the home one.
    let home = crate::_shared::paths::home()?;
    Some(PathBuf::from(format!("{}{}", home.to_string_lossy(), &dir[prefix.len()..])))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::payload::RateLimit;

    fn facts() -> MainFacts {
        MainFacts {
            now_ms: 1_787_037_452_146,
            ctx_pct: Some(26.0),
            ctx_used: Some(259_000.0),
            ctx_size: Some(1_000_000.0),
            five_hour: RateLimit { used_pct: Some(7.0), resets_at: Some(json!(1_774_200_000i64)) },
            seven_day: RateLimit { used_pct: Some(1.0), resets_at: Some(json!(1_774_600_000i64)) },
            ..Default::default()
        }
    }

    #[test]
    fn the_document_matches_the_contract_layout() {
        let doc = document(&facts(), "abc123");
        assert_eq!(
            doc,
            json!({
                "sessionId": "abc123",
                "ts": 1_787_037_452_146i64,
                "ctxPct": 26,
                "ctxUsed": 259_000,
                "ctxSize": 1_000_000,
                "fiveHourPct": 7,
                "fiveHourResetsAt": 1_774_200_000i64,
                "sevenDayPct": 1,
                "sevenDayResetsAt": 1_774_600_000i64,
            }),
        );
    }

    #[test]
    fn the_nine_keys_are_in_contract_order() {
        let doc = document(&facts(), "abc123");
        let keys: Vec<&str> = doc.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, [
            "sessionId",
            "ts",
            "ctxPct",
            "ctxUsed",
            "ctxSize",
            "fiveHourPct",
            "fiveHourResetsAt",
            "sevenDayPct",
            "sevenDayResetsAt",
        ]);
    }

    #[test]
    fn ctx_size_is_absent_but_the_rate_limit_fields_are_null() {
        let doc = document(&MainFacts::default(), "s");
        assert!(!doc.as_object().unwrap().contains_key("ctxSize"), "ctxSize is omitted, not null");
        for key in ["ctxPct", "ctxUsed", "fiveHourPct", "fiveHourResetsAt", "sevenDayPct", "sevenDayResetsAt"] {
            assert_eq!(doc.get(key), Some(&Value::Null), "{key} must be present as null");
        }
    }

    #[test]
    fn resets_at_is_mirrored_raw() {
        let mut f = facts();
        f.five_hour.resets_at = Some(json!("2026-08-19T12:00:00Z"));
        let doc = document(&f, "s");
        assert_eq!(doc.get("fiveHourResetsAt"), Some(&json!("2026-08-19T12:00:00Z")), "an ISO string stays an ISO string");
    }

    #[test]
    fn a_fractional_percentage_survives() {
        let mut f = facts();
        f.seven_day.used_pct = Some(1.5);
        assert_eq!(document(&f, "s").get("sevenDayPct"), Some(&json!(1.5)));
    }

    #[test]
    fn nothing_is_written_without_the_env_var_or_a_session_id() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().to_str().unwrap();

        mirror(&facts(), None, Some("abc"));
        mirror(&facts(), Some(path), None);
        mirror(&facts(), Some(""), Some("abc"));
        mirror(&facts(), Some(path), Some(""));

        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0, "no file should exist");
    }

    #[test]
    fn the_file_is_named_for_the_session_and_does_not_collide_with_the_hooks_state_file() {
        let dir = tempfile::TempDir::new().unwrap();
        mirror(&facts(), dir.path().to_str(), Some("abc123"));

        let written = dir.path().join("abc123.json");
        assert!(written.exists(), "the mirror writes <session_id>.json");
        // `context-caps.js` owns `<session>.state.json` in the same directory.
        assert!(!dir.path().join("abc123.state.json").exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "and leaves no temp file");
    }

    #[test]
    fn an_unwritable_directory_is_silently_survived() {
        // Best-effort: a failure here must never affect the rendered line.
        mirror(&facts(), Some("/proc/nonexistent/nope"), Some("abc"));
    }

    #[test]
    fn home_prefixes_expand_loosely() {
        let mut env = crate::_shared::env_lock();
        env.set("HOME", "/tmp/fakehome");
        for spelling in ["~/usage", "$HOME/usage", "${HOME}/usage", "${HOME/usage", "$HOME}/usage"] {
            assert_eq!(expand_home(spelling), Some(PathBuf::from("/tmp/fakehome/usage")), "{spelling} should expand");
        }
        assert_eq!(expand_home("/absolute/usage"), Some(PathBuf::from("/absolute/usage")));
    }

    #[test]
    fn a_home_prefix_with_no_home_is_absent_rather_than_literal() {
        // Restored on drop, so a failing assertion below cannot strand `HOME`.
        let mut env = crate::_shared::env_lock();
        env.unset("HOME");

        for spelling in ["~/usage", "$HOME/usage", "${HOME}/usage"] {
            // The old behaviour returned the text unexpanded, so `~/usage`
            // became a *relative* path and the mirror landed in the cwd.
            assert_eq!(expand_home(spelling), None, "{spelling} must not degrade to a relative path");
        }
        // A path that never asked for the home directory is unaffected.
        assert_eq!(expand_home("/absolute/usage"), Some(PathBuf::from("/absolute/usage")));
    }

    #[test]
    fn the_new_variable_wins_and_the_legacy_one_is_the_fallback() {
        let mut env = crate::_shared::env_lock();

        env.set(USAGE_DIR_ENV, "/new");
        env.set(LEGACY_USAGE_DIR_ENV, "/legacy");
        assert_eq!(usage_dir_from_env().as_deref(), Some("/new"), "the new name wins outright");

        env.unset(USAGE_DIR_ENV);
        assert_eq!(usage_dir_from_env().as_deref(), Some("/legacy"), "unset falls back");
    }

    #[test]
    fn an_empty_new_variable_falls_back_rather_than_masking_the_legacy_one() {
        // `var()` returns `Ok("")` for an empty variable, so an `or_else` chain
        // that filters emptiness only at the end never reaches the fallback —
        // it sees `Some("")`, skips the legacy arm, and then filters to `None`.
        // Emptying the new name is how someone disables it, and doing so must
        // not silently take the mirror (and the caps hook's only data) with it.
        let mut env = crate::_shared::env_lock();

        env.set(USAGE_DIR_ENV, "");
        env.set(LEGACY_USAGE_DIR_ENV, "/legacy");
        assert_eq!(usage_dir_from_env().as_deref(), Some("/legacy"));
    }

    #[test]
    fn the_mirror_is_off_when_neither_variable_carries_a_path() {
        let mut env = crate::_shared::env_lock();

        env.unset(USAGE_DIR_ENV);
        env.unset(LEGACY_USAGE_DIR_ENV);
        assert_eq!(usage_dir_from_env(), None, "neither set");

        env.set(USAGE_DIR_ENV, "");
        env.set(LEGACY_USAGE_DIR_ENV, "");
        assert_eq!(usage_dir_from_env(), None, "both empty");
    }
}
