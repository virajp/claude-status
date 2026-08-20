//! The usage mirror — a hard contract with `ai-plugins` (contract §8).
//!
//! **This is not internal.** Context-window and rate-limit figures arrive only
//! on the statusline payload, never on hook stdin, so every main-bar render
//! mirrors them to a session-keyed file that vwf's `PostToolUse` context-cap
//! hook reads. The field names, the file layout and the env var name are all
//! part of the contract; changing any of them silently disables that hook.
//!
//! Best-effort throughout: a failure here must never affect the rendered line.

use std::path::PathBuf;

use serde_json::{Map, Value, json};

use crate::json::write_json_atomic;
use crate::payload::MainFacts;

/// The env var that enables the mirror. **This name does not change.**
pub const USAGE_DIR_ENV: &str = "AI_PLUGINS_USAGE_DIR";

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

    let path = expand_home(dir).join(format!("{session_id}.json"));
    let _ = write_json_atomic(&path, &document(facts, session_id));
}

/// The nine keys, in order. `serde_json`'s `preserve_order` is what keeps them
/// in it.
///
/// Two shapes are load-bearing:
/// - `resets_at` is mirrored **raw**, not normalised. The consumer does its own
///   seconds/millis/ISO discrimination, and an ISO string must stay one.
/// - `ctxSize` is **absent** when the payload carried no `context_window_size`,
///   while the four rate-limit fields are present as `null`. That asymmetry is
///   what the consumer was written against.
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

/// Expands a leading `~`, `$HOME`, `${HOME}` or `%USERPROFILE%`.
///
/// Deliberately loose, matching the old implementation: `${HOME` and `$HOME}`
/// expand too. Claude Code may or may not have expanded the value before
/// exporting it, so every spelling arrives in the wild — and on Windows the
/// unexpanded form is `%USERPROFILE%`, which no POSIX-shaped matcher would
/// catch.
fn expand_home(dir: &str) -> PathBuf {
    let Some(home) = crate::_shared::paths::home() else {
        return PathBuf::from(dir);
    };
    let home = home.to_string_lossy();

    for prefix in ["${HOME}", "${HOME", "$HOME}", "$HOME", "%USERPROFILE%", "~"] {
        if let Some(rest) = dir.strip_prefix(prefix) {
            return PathBuf::from(format!("{home}{rest}"));
        }
    }
    PathBuf::from(dir)
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
        // SAFETY: single-threaded test setup; no other thread reads HOME here.
        unsafe { std::env::set_var("HOME", "/tmp/fakehome") };
        for spelling in ["~/usage", "$HOME/usage", "${HOME}/usage", "${HOME/usage", "$HOME}/usage"] {
            assert_eq!(expand_home(spelling), PathBuf::from("/tmp/fakehome/usage"), "{spelling} should expand");
        }
        assert_eq!(expand_home("/absolute/usage"), PathBuf::from("/absolute/usage"));
    }
}
