//! When a render should spawn a refresh — and when it must not.
//!
//! Kept pure and separate from the spawning itself, because "should we fetch"
//! is the decision with all the conditions in it and none of the I/O.

use crate::modules::spend::cache::SpendCache;
use crate::modules::spend::{SpendConfig, is_team_plan};

/// The stretched interval for a seat that gate 4 hides anyway.
pub const STRETCHED_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Spawn,
    /// `refreshMinutes: 0` disables the spawn — but the cache is still read
    /// and still rendered.
    Disabled,
    Fresh,
    InBackoff { until: i64 },
}

/// Should a render spawn a refresh child?
///
/// The **24-hour stretch**, stated with its conditions rather than as the
/// conclusion alone: the interval stretches to a day when `show` is `auto`
/// **and** a cache exists **and** it records a plan **and** that plan is not
/// team/enterprise. A cache with *no plan recorded* uses the normal interval —
/// the plan tag comes from the very fetch being scheduled, so a machine that
/// has never learnt its plan must keep asking at full rate.
///
/// All four conditions are pinned by `the_stretch_needs_all_four_conditions`
/// below; the day boundary by `a_stretched_cache_still_refreshes_after_a_day`.
/// This used to cite a contract that gave only the conclusion, which is how the
/// conditions came to live in one place and be applied in another.
///
/// It exists because gate 4 hides the segment for Pro/Max seats, but a seat can
/// become a team seat, so the machine still re-checks daily.
pub fn decide(cached: Option<&SpendCache>, config: &SpendConfig, now_ms: i64) -> Decision {
    if config.refresh_minutes == 0.0 {
        return Decision::Disabled;
    }

    let Some(cache) = cached else {
        return Decision::Spawn;
    };

    // Strict `>`, on both clocks.
    if cache.backoff_until > now_ms {
        return Decision::InBackoff { until: cache.backoff_until };
    }

    let interval = interval_for(cache, config);
    if now_ms - cache.ts > interval { Decision::Spawn } else { Decision::Fresh }
}

fn interval_for(cache: &SpendCache, config: &SpendConfig) -> i64 {
    let stretched = config.show == "auto"
        && cache.plan.as_deref().is_some_and(|plan| !plan.is_empty())
        && !is_team_plan(cache.plan.as_deref());

    if stretched { STRETCHED_MS } else { (config.refresh_minutes * 60_000.0) as i64 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(show: &str, minutes: f64) -> SpendConfig {
        SpendConfig { refresh_minutes: minutes, show: show.to_string() }
    }

    fn cache(plan: Option<&str>, ts: i64, backoff: i64) -> SpendCache {
        SpendCache { ts, plan: plan.map(str::to_string), failures: 0, backoff_until: backoff, data: None }
    }

    const NOW: i64 = 1_000_000_000;

    #[test]
    fn no_cache_means_spawn() {
        assert_eq!(decide(None, &cfg("auto", 15.0), NOW), Decision::Spawn);
    }

    #[test]
    fn zero_minutes_disables_the_spawn_entirely() {
        // Including when there is no cache at all — the render still draws
        // whatever it has, it just never fetches.
        assert_eq!(decide(None, &cfg("auto", 0.0), NOW), Decision::Disabled);
        let stale = cache(Some("team"), 0, 0);
        assert_eq!(decide(Some(&stale), &cfg("auto", 0.0), NOW), Decision::Disabled);
    }

    #[test]
    fn a_fresh_cache_does_not_spawn() {
        let fresh = cache(Some("team"), NOW - 60_000, 0);
        assert_eq!(decide(Some(&fresh), &cfg("auto", 15.0), NOW), Decision::Fresh);
    }

    #[test]
    fn a_stale_cache_spawns() {
        let stale = cache(Some("team"), NOW - 16 * 60_000, 0);
        assert_eq!(decide(Some(&stale), &cfg("auto", 15.0), NOW), Decision::Spawn);
    }

    #[test]
    fn staleness_is_a_strict_comparison() {
        let exactly = cache(Some("team"), NOW - 15 * 60_000, 0);
        assert_eq!(decide(Some(&exactly), &cfg("auto", 15.0), NOW), Decision::Fresh, "exactly at the TTL is fresh");
    }

    #[test]
    fn a_future_backoff_blocks_the_spawn() {
        let backed_off = cache(Some("team"), 0, NOW + 1000);
        assert_eq!(decide(Some(&backed_off), &cfg("auto", 15.0), NOW), Decision::InBackoff { until: NOW + 1000 });

        let expired = cache(Some("team"), 0, NOW);
        assert_eq!(decide(Some(&expired), &cfg("auto", 15.0), NOW), Decision::Spawn, "strictly greater");
    }

    #[test]
    fn the_stretch_needs_all_four_conditions() {
        let seventeen_hours_ago = NOW - 17 * 60 * 60 * 1000;

        // All four hold: auto, a cache, a recorded plan, not team → stretched,
        // so seventeen hours is still fresh.
        let max_plan = cache(Some("max"), seventeen_hours_ago, 0);
        assert_eq!(decide(Some(&max_plan), &cfg("auto", 15.0), NOW), Decision::Fresh);

        // show is not auto → normal interval.
        assert_eq!(decide(Some(&max_plan), &cfg("always", 15.0), NOW), Decision::Spawn);

        // The plan IS team → normal interval.
        let team = cache(Some("team"), seventeen_hours_ago, 0);
        assert_eq!(decide(Some(&team), &cfg("auto", 15.0), NOW), Decision::Spawn);

        // No plan recorded → normal interval, because the plan tag comes from
        // the very fetch being scheduled.
        let unknown = cache(None, seventeen_hours_ago, 0);
        assert_eq!(decide(Some(&unknown), &cfg("auto", 15.0), NOW), Decision::Spawn);

        let empty_plan = cache(Some(""), seventeen_hours_ago, 0);
        assert_eq!(decide(Some(&empty_plan), &cfg("auto", 15.0), NOW), Decision::Spawn);
    }

    #[test]
    fn a_stretched_cache_still_refreshes_after_a_day() {
        let two_days = NOW - 2 * STRETCHED_MS;
        let max_plan = cache(Some("max"), two_days, 0);
        assert_eq!(decide(Some(&max_plan), &cfg("auto", 15.0), NOW), Decision::Spawn, "a seat can become a team seat");
    }
}
