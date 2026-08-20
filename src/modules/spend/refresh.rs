//! The refresh child: the only code that fetches.
//!
//! Lock → 60-second dedupe → credentials → fetch → extract → atomic write,
//! releasing the lock on every path. It prints nothing, renders nothing, and
//! **always exits 0** — it is spawned detached with its stdio at `/dev/null`,
//! so anything it said would be discarded anyway.
//!
//! The failure handling is the subtle part: a failed fetch must **never clear a
//! good value**. A stale figure is worth far more than an empty segment.

use std::path::Path;

use serde_json::Value;

use crate::modules::spend::cache::{self, SpendCache};
use crate::modules::spend::creds::Credentials;
use crate::modules::spend::lock::{self, Acquired};
use crate::modules::spend::{extract, http};

/// A sibling that ran within this window means there is nothing to do.
pub const DEDUPE_MS: i64 = 60_000;

/// The ceiling on exponential backoff.
pub const MAX_BACKOFF_MS: i64 = 6 * 60 * 60 * 1000;

/// What one refresh attempt did, for `--debug` to narrate.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Updated,
    /// No budget block on this account — a valid answer, cached as `null`.
    NoBudget,
    Unauthorized,
    RateLimited { backoff_until: i64 },
    NoCredentials,
    Failed { reason: String },
    /// Another refresh holds the lock.
    Locked { holder_age_secs: u64 },
    /// A sibling wrote the cache moments ago.
    Deduped,
    LockUnavailable,
}

/// Runs a refresh, start to finish.
///
/// `bypass_dedupe` is what `--debug` passes: a user typing it twice wants two
/// answers, where the background child wants to stay off the endpoint.
pub fn run(cache_path: &Path, refresh_minutes: f64, now_ms: i64, bypass_dedupe: bool) -> Outcome {
    // `mkdir -p` before the lock, or the lock itself has nowhere to live.
    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let _guard = match lock::acquire(cache_path) {
        Acquired::Held(guard) => guard,
        Acquired::Contended { holder_age } => return Outcome::Locked { holder_age_secs: holder_age.as_secs() },
        Acquired::Indeterminate => return Outcome::LockUnavailable,
    };

    let previous = cache::read_from(cache_path);

    // A sibling that just wrote means this fetch would buy nothing. Note the
    // asymmetry the original had and this keeps: the two contended returns
    // above leave the lock alone, while this one releases it via the guard.
    if !bypass_dedupe
        && let Some(prev) = previous.as_ref()
        && now_ms - prev.ts < DEDUPE_MS
    {
        return Outcome::Deduped;
    }

    let credentials = crate::modules::spend::creds::load();
    let Some(credentials) = credentials else {
        write_failure(cache_path, previous.as_ref(), None, now_ms, None);
        return Outcome::NoCredentials;
    };

    match http::fetch(&http::url(), &credentials.token) {
        http::Response::Ok(body) => {
            let parsed: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            let data = extract::extract(&parsed);
            let had_budget = data.is_some();

            // Only the 200 branch writes a zero backoff and clears failures.
            let _ = cache::write_to(cache_path, &SpendCache {
                ts: now_ms,
                plan: credentials.plan.clone().or_else(|| previous.and_then(|p| p.plan)),
                failures: 0,
                backoff_until: 0,
                data,
            });

            if had_budget { Outcome::Updated } else { Outcome::NoBudget }
        }

        http::Response::RateLimited => {
            // The backoff uses the **already-incremented** failure count, so
            // the first 429 gives 30 minutes rather than 15.
            let failures = previous.as_ref().map_or(0, |p| p.failures) + 1;
            let backoff = backoff_for(refresh_minutes, failures);
            let backoff_until = now_ms + backoff;

            write_failure(cache_path, previous.as_ref(), Some(&credentials), now_ms, Some(backoff_until));
            Outcome::RateLimited { backoff_until }
        }

        // Every other failure leaves `backoffUntil` **absent**, which reads
        // back as 0 and erases any prior backoff. Faithful, and load-bearing
        // for anyone comparing behaviour against the original.
        http::Response::Unauthorized => {
            write_failure(cache_path, previous.as_ref(), Some(&credentials), now_ms, None);
            Outcome::Unauthorized
        }
        http::Response::Transport(reason) => {
            write_failure(cache_path, previous.as_ref(), Some(&credentials), now_ms, None);
            Outcome::Failed { reason }
        }
        http::Response::Unexpected(status) => {
            write_failure(cache_path, previous.as_ref(), Some(&credentials), now_ms, None);
            Outcome::Failed { reason: format!("HTTP {status}") }
        }
    }
}

/// Records a failure without discarding the last good figures.
fn write_failure(
    cache_path: &Path,
    previous: Option<&SpendCache>,
    credentials: Option<&Credentials>,
    now_ms: i64,
    backoff_until: Option<i64>,
) {
    let _ = cache::write_to(cache_path, &SpendCache {
        ts: now_ms,
        // The plan is refreshed even on failure: from the credentials when
        // they were found, else whatever the cache already knew.
        plan: credentials.and_then(|c| c.plan.clone()).or_else(|| previous.and_then(|p| p.plan.clone())),
        failures: previous.map_or(0, |p| p.failures) + 1,
        backoff_until: backoff_until.unwrap_or(0),
        data: previous.and_then(|p| p.data.clone()),
    });
}

/// `refreshMinutes × 2^failures`, capped at six hours.
pub fn backoff_for(refresh_minutes: f64, failures: u32) -> i64 {
    let base = (refresh_minutes * 60_000.0).max(0.0);
    let scaled = base * 2f64.powi(failures.min(24) as i32);
    (scaled as i64).min(MAX_BACKOFF_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_rate_limit_uses_the_incremented_count() {
        // 15 minutes with one failure is 30 minutes, not 15.
        assert_eq!(backoff_for(15.0, 1), 30 * 60_000);
        assert_eq!(backoff_for(15.0, 2), 60 * 60_000);
        assert_eq!(backoff_for(15.0, 3), 120 * 60_000);
    }

    #[test]
    fn backoff_is_capped_at_six_hours() {
        assert_eq!(backoff_for(15.0, 20), MAX_BACKOFF_MS);
        assert_eq!(backoff_for(15.0, 99), MAX_BACKOFF_MS, "and does not overflow");
    }

    #[test]
    fn a_zero_refresh_interval_yields_no_backoff() {
        assert_eq!(backoff_for(0.0, 3), 0);
    }

    #[test]
    fn a_failure_preserves_the_last_good_data() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("spend.json");

        let good = SpendCache {
            ts: 1000,
            plan: Some("team".into()),
            failures: 0,
            backoff_until: 0,
            data: Some(extract::Spend {
                used_minor: 7593.0,
                limit_minor: 15000.0,
                exponent: 2,
                percent: None,
                enabled: Some(true),
            }),
        };
        cache::write_to(&path, &good).unwrap();

        write_failure(&path, Some(&good), None, 2000, None);

        let after = cache::read_from(&path).unwrap();
        assert_eq!(after.data, good.data, "a failed fetch never clears a good value");
        assert_eq!(after.failures, 1);
        assert_eq!(after.plan.as_deref(), Some("team"));
        assert_eq!(after.ts, 2000);
    }

    #[test]
    fn a_non_rate_limit_failure_erases_a_prior_backoff() {
        // Faithful to the original: only the 429 branch writes a backoff, so a
        // network error after one leaves the key absent, which reads as 0.
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("spend.json");

        let backed_off =
            SpendCache { ts: 1000, plan: None, failures: 1, backoff_until: 9_999_999, data: None };
        cache::write_to(&path, &backed_off).unwrap();

        write_failure(&path, Some(&backed_off), None, 2000, None);

        assert_eq!(cache::read_from(&path).unwrap().backoff_until, 0);
    }

    #[test]
    fn a_held_lock_stops_the_refresh_without_fetching() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("spend.json");

        let _held = lock::acquire(&path);
        match run(&path, 15.0, 1000, false) {
            Outcome::Locked { .. } => {}
            other => panic!("expected Locked, got {other:?}"),
        }
        assert!(!path.exists(), "nothing was written");
    }

    #[test]
    fn a_recent_sibling_write_dedupes() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("spend.json");
        cache::write_to(&path, &SpendCache { ts: 100_000, plan: None, failures: 0, backoff_until: 0, data: None })
            .unwrap();

        // 30 seconds later: inside the window.
        assert_eq!(run(&path, 15.0, 130_000, false), Outcome::Deduped);
    }

    #[test]
    fn debug_bypasses_the_dedupe() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("spend.json");
        cache::write_to(&path, &SpendCache { ts: 100_000, plan: None, failures: 0, backoff_until: 0, data: None })
            .unwrap();

        // Same instant, but bypassing: it gets as far as looking for
        // credentials rather than returning Deduped.
        assert_ne!(run(&path, 15.0, 130_000, true), Outcome::Deduped);
    }
}
