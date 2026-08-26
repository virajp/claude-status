//! The spend cache: the only thing a render is allowed to read.
//!
//! Machine-global on purpose. One fetch per interval per machine, however many
//! sessions are open — which is the whole reason the endpoint's per-account
//! throttle is survivable.

use std::path::PathBuf;

use serde_json::{Value, json};

use crate::json::{read_json_file, write_json_atomic};
use crate::modules::spend::extract::Spend;

/// Overrides the cache location. Renamed from the `ai-plugins` spelling; the
/// old cache is left in place and ignored, and a first run re-fetches once.
pub const CACHE_ENV: &str = "CLAUDE_STATUS_SPEND_CACHE";

#[derive(Debug, Clone, PartialEq)]
pub struct SpendCache {
    /// When this was written, epoch millis.
    pub ts: i64,
    /// The seat's plan tag, beside the figures rather than inside them.
    pub plan: Option<String>,
    pub failures: u32,
    /// Epoch millis before which no refresh may run. `0` means none.
    pub backoff_until: i64,
    /// `None` when the account has no budget block — a valid outcome.
    pub data: Option<Spend>,
}

impl SpendCache {
    pub fn to_json(&self) -> Value {
        let data = match &self.data {
            None => Value::Null,
            Some(d) => json!({
                "usedMinor": d.used_minor,
                "limitMinor": d.limit_minor,
                "exponent": d.exponent,
                "percent": d.percent,
                "enabled": d.enabled,
            }),
        };
        json!({
            "ts": self.ts,
            "plan": self.plan,
            "failures": self.failures,
            "backoffUntil": self.backoff_until,
            "data": data,
        })
    }

    pub fn from_json(v: &Value) -> Option<Self> {
        if !v.is_object() {
            return None;
        }
        let data = v.get("data").filter(|d| d.is_object()).map(|d| Spend {
            used_minor: d.get("usedMinor").and_then(Value::as_f64).unwrap_or(0.0),
            limit_minor: d.get("limitMinor").and_then(Value::as_f64).unwrap_or(0.0),
            exponent: d.get("exponent").and_then(Value::as_i64).unwrap_or(2) as i32,
            percent: d.get("percent").and_then(Value::as_f64),
            enabled: d.get("enabled").and_then(Value::as_bool),
        });

        Some(Self {
            ts: v.get("ts").and_then(Value::as_i64).unwrap_or(0),
            plan: v.get("plan").and_then(Value::as_str).map(str::to_string),
            failures: v.get("failures").and_then(Value::as_u64).unwrap_or(0) as u32,
            // An absent key reads back as 0, which is how a network error after
            // a 429 erases the backoff. Faithful, and load-bearing.
            backoff_until: v.get("backoffUntil").and_then(Value::as_i64).unwrap_or(0),
            data,
        })
    }
}

/// Where the cache lives.
///
/// `$CLAUDE_STATUS_SPEND_CACHE` wins, expanding a **leading `~` only** — unlike
/// `$AI_PLUGINS_USAGE_DIR`, which also expands `$HOME`. The two are different
/// contracts, deliberately; the retired behaviour contract conflated them.
/// See `docs/usage-mirror-contract.md` for the other one.
///
/// `None` when there is no `$HOME` to resolve against. **Absent, never
/// relative:** falling back to a bare `spend.json` wrote the cache into
/// whatever directory Claude Code happened to be launched from — a stray file
/// in the user's working tree, and a cache that never hit, because the next
/// session started somewhere else.
pub fn path() -> Option<PathBuf> {
    if let Ok(override_path) = std::env::var(CACHE_ENV)
        && !override_path.is_empty()
    {
        return expand_tilde(&override_path);
    }

    Some(crate::_shared::paths::home()?.join(".cache").join("claude-status").join("spend.json"))
}

fn expand_tilde(path: &str) -> Option<PathBuf> {
    match path.strip_prefix('~') {
        // Same rule as `path()`: a value that asks for the home directory and
        // cannot get one is absent, not a literal `~/…` relative to nowhere.
        Some(rest) => Some(PathBuf::from(format!("{}{rest}", crate::_shared::paths::home()?.display()))),
        None => Some(PathBuf::from(path)),
    }
}

/// Reads the cache. Missing, unreadable or corrupt all read as `None` — a
/// render must never fail because of this file.
///
/// There is no `read()`/`write()` pair taking the path implicitly. Both existed
/// and neither had a caller: every real site resolves `path()` once and threads
/// it, which is also what lets the tests point at a temp directory. Making the
/// path an argument is the reason there is nothing here to get wrong when
/// `$HOME` is unresolvable.
pub fn read_from(path: &std::path::Path) -> Option<SpendCache> {
    SpendCache::from_json(&read_json_file(path)?)
}

pub fn write_to(path: &std::path::Path, cache: &SpendCache) -> std::io::Result<()> {
    write_json_atomic(path, &cache.to_json())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SpendCache {
        SpendCache {
            ts: 1_787_000_000_000,
            plan: Some("team".into()),
            failures: 2,
            backoff_until: 1_787_000_600_000,
            data: Some(Spend {
                used_minor: 7593.0,
                limit_minor: 15000.0,
                exponent: 2,
                percent: Some(50.62),
                enabled: Some(true),
            }),
        }
    }

    #[test]
    fn it_round_trips() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("spend.json");
        write_to(&path, &sample()).unwrap();
        assert_eq!(read_from(&path), Some(sample()));
    }

    #[test]
    fn a_cache_with_no_budget_block_round_trips_as_none() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("spend.json");
        let empty = SpendCache { data: None, ..sample() };
        write_to(&path, &empty).unwrap();

        let back = read_from(&path).unwrap();
        assert_eq!(back.data, None);
        assert_eq!(back.plan.as_deref(), Some("team"), "the plan survives without data");
    }

    #[test]
    fn missing_and_corrupt_files_read_as_none() {
        let dir = tempfile::TempDir::new().unwrap();
        assert_eq!(read_from(&dir.path().join("absent.json")), None);

        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, "{ not json").unwrap();
        assert_eq!(read_from(&bad), None);

        let wrong_shape = dir.path().join("array.json");
        std::fs::write(&wrong_shape, "[1,2,3]").unwrap();
        assert_eq!(read_from(&wrong_shape), None);
    }

    #[test]
    fn an_absent_backoff_reads_as_zero() {
        let cache = SpendCache::from_json(&serde_json::json!({ "ts": 1, "data": null })).unwrap();
        assert_eq!(cache.backoff_until, 0);
        assert_eq!(cache.failures, 0);
        assert_eq!(cache.plan, None);
    }

    #[test]
    fn the_env_override_wins_and_expands_a_leading_tilde() {
        let mut env = crate::_shared::env_lock();
        env.set("HOME", "/tmp/fakehome");
        env.set(CACHE_ENV, "~/custom/spend.json");
        assert_eq!(path(), Some(PathBuf::from("/tmp/fakehome/custom/spend.json")));

        env.set(CACHE_ENV, "/absolute/spend.json");
        assert_eq!(path(), Some(PathBuf::from("/absolute/spend.json")));

        // Unlike the usage mirror, `$HOME` is NOT expanded here.
        env.set(CACHE_ENV, "$HOME/spend.json");
        assert_eq!(path(), Some(PathBuf::from("$HOME/spend.json")));

        env.unset(CACHE_ENV);
        assert_eq!(path(), Some(PathBuf::from("/tmp/fakehome/.cache/claude-status/spend.json")));
    }

    #[test]
    fn without_a_home_the_path_is_absent_rather_than_relative() {
        // Both variables are restored on drop, including if an assertion here
        // fails — a trailing restore would be skipped by the unwind and every
        // later test would then run with no `HOME`.
        let mut env = crate::_shared::env_lock();
        env.unset(CACHE_ENV);
        env.unset("HOME");
        assert_eq!(path(), None, "a bare spend.json would land in the user's cwd");

        // A `~` override cannot resolve either — and must not degrade to the
        // literal `~/…`, which is just as relative.
        env.set(CACHE_ENV, "~/custom/spend.json");
        assert_eq!(path(), None);

        // An absolute override needs no home and still works.
        env.set(CACHE_ENV, "/absolute/spend.json");
        assert_eq!(path(), Some(PathBuf::from("/absolute/spend.json")));
    }
}
