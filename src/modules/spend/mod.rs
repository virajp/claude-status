//! The spend segment: the account's monthly budget, and everything behind it.
//!
//! **A render must never fetch.** The usage endpoint throttles on accumulated
//! account usage, and a tripped account stays 429 for half an hour or more —
//! while this bar can render every four seconds in every open session. So a
//! render reads a cache file and nothing else; when that cache is stale it
//! spawns a detached child and draws the cached value immediately, never
//! waiting for it.

pub mod cache;
pub mod creds;
pub mod extract;
pub mod http;
pub mod lock;
pub mod refresh;
pub mod schedule;

use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::config::SegmentEntry;
use crate::fmt::{money, to_fixed};

/// Which gate hid the segment, for `--debug` to name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// `spend` is not in the resolved layout. Nothing is read or spawned.
    NotInLayout,
    /// No cached data — nothing to show yet.
    NoData,
    /// The account reports no usable budget.
    Disabled,
    /// `show: "auto"` and this is not a team or enterprise seat.
    NotATeamPlan,
}

impl Gate {
    pub fn describe(self) -> &'static str {
        match self {
            Self::NotInLayout => "spend is not in the configured lines",
            Self::NoData => "no cached spend data",
            Self::Disabled => "the account reports no usable budget",
            Self::NotATeamPlan => "show=auto and the plan is not team/enterprise",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    WillRender { text: String },
    Hidden { gate: Gate },
}

impl Verdict {
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::WillRender { text } => Some(text),
            Self::Hidden { .. } => None,
        }
    }
}

/// The `spend` block of the merged config.
///
/// A field of [`crate::config::Config`] rather than something copied out of it
/// key by key: the block is closed in the schema, and the module that reads a
/// block owns its shape.
#[derive(Debug, PartialEq, Deserialize, serde::Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(inline, deny_unknown_fields, description = ""))]
#[serde(default, rename_all = "camelCase")]
pub struct SpendConfig {
    /// Genuinely zero when set to zero — this is the one numeric config value
    /// where 0 is a meaningful setting rather than "unset". A value that is
    /// not a number at all is the shipped interval, and costs only itself.
    #[cfg_attr(feature = "schema", schemars(range(min = 0)))]
    #[cfg_attr(feature = "schema", schemars(description = "Minimum minutes between fetches of the OAuth usage endpoint (the file-based timer). 0 disables the background refresh entirely; the segment then renders whatever the cache holds. Default 15."))]
    #[serde(deserialize_with = "refresh_minutes")]
    pub refresh_minutes: f64,
    /// A `String` and not an enum in the types, because [`verdict`] renders on
    /// **anything that is not `auto`** — so a third value is a working config
    /// rather than an error. The schema still enumerates the two that mean
    /// something, which is what an editor's completion list is for.
    #[cfg_attr(feature = "schema", schemars(with = "crate::config::schema::ShowMode"))]
    #[cfg_attr(feature = "schema", schemars(description = "`auto` (default) renders only for team/enterprise seats — the plans whose budget is a monthly spend limit. `always` renders whenever budget data exists (e.g. to watch a Pro/Max extra-usage credit cap)."))]
    #[serde(deserialize_with = "show")]
    pub show: String,
}

/// The shipped refresh interval, in minutes.
const DEFAULT_REFRESH_MINUTES: f64 = 15.0;

/// The shipped `show`. Any other string renders, so `"always"` needs no
/// special case — see [`verdict`].
const DEFAULT_SHOW: &str = "auto";

impl Default for SpendConfig {
    fn default() -> Self {
        Self { refresh_minutes: DEFAULT_REFRESH_MINUTES, show: DEFAULT_SHOW.into() }
    }
}

fn refresh_minutes<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    crate::json::leaf(d, Value::as_f64, DEFAULT_REFRESH_MINUTES)
}

fn show<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    crate::json::leaf(d, |v| Some(v.as_str()?.to_string()), DEFAULT_SHOW.to_string())
}

/// Is `spend` an entry in any row of the resolved layout?
///
/// Gate 1, and the reason it is first: a user without the segment pays
/// **nothing** for it — no file read, no fork, no keychain prompt.
pub fn in_layout(lines: &[Vec<SegmentEntry>]) -> bool {
    lines.iter().flatten().any(|entry| entry.id() == Some("spend"))
}

/// The four gates, in order.
///
/// Deliberately pure, and deliberately returning *which* gate hid the segment:
/// every one of these was a silent `return` in the old implementation, which is
/// why a working account with a perfect token was indistinguishable from a
/// broken one.
pub fn verdict(cached: Option<&cache::SpendCache>, config: &SpendConfig, lines: &[Vec<SegmentEntry>], symbol: &str) -> Verdict {
    if !in_layout(lines) {
        return Verdict::Hidden { gate: Gate::NotInLayout };
    }

    let Some(cache) = cached else {
        return Verdict::Hidden { gate: Gate::NoData };
    };
    let Some(data) = cache.data.as_ref() else {
        return Verdict::Hidden { gate: Gate::NoData };
    };

    // `enabled` is tested **strictly** against false — an `enabled: 0` does not
    // hide — while the limit is tested loosely, so a limit of 0 does.
    if data.enabled == Some(false) || data.limit_minor == 0.0 {
        return Verdict::Hidden { gate: Gate::Disabled };
    }

    // Any value of `show` other than "auto" renders, so "always" needs no
    // special case and "yes" renders too.
    if config.show == "auto" && !is_team_plan(cache.plan.as_deref()) {
        return Verdict::Hidden { gate: Gate::NotATeamPlan };
    }

    Verdict::WillRender { text: render_text(data, symbol) }
}

/// Case-sensitive, and read from the **cache root** rather than from the data
/// block — the plan tag comes from the same fetch but sits beside the figures.
pub fn is_team_plan(plan: Option<&str>) -> bool {
    matches!(plan, Some("team") | Some("enterprise"))
}

/// `{spend} $75.93/$150 (51%)`.
pub fn render_text(data: &extract::Spend, symbol: &str) -> String {
    let pct = percent_of(data);
    format!(
        "{symbol} {}/{} ({}%)",
        money(data.used_minor, data.exponent),
        money(data.limit_minor, data.exponent),
        to_fixed(pct, 0),
    )
}

/// The endpoint's own percentage when it gave one — including `0` — else
/// derived. Dividing by a zero limit is unreachable here: gate 3 rejects it.
pub fn percent_of(data: &extract::Spend) -> f64 {
    match data.percent {
        Some(pct) => pct,
        None if data.limit_minor != 0.0 => data.used_minor / data.limit_minor * 100.0,
        None => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::modules::spend::cache::SpendCache;

    fn data() -> extract::Spend {
        extract::Spend { used_minor: 7593.0, limit_minor: 15000.0, exponent: 2, percent: None, enabled: Some(true) }
    }

    fn cache_with(plan: Option<&str>, data: Option<extract::Spend>) -> SpendCache {
        SpendCache { ts: 0, plan: plan.map(str::to_string), failures: 0, backoff_until: 0, data }
    }

    fn cfg(show: &str) -> SpendConfig {
        SpendConfig { refresh_minutes: 15.0, show: show.to_string() }
    }

    /// One row, as `Config` would have deserialized it.
    fn row(entries: [Value; 3]) -> Vec<Vec<SegmentEntry>> {
        vec![entries.into_iter().map(SegmentEntry::from).collect()]
    }

    fn lines_with_spend() -> Vec<Vec<SegmentEntry>> {
        row([json!("model"), json!("spend"), json!("cost")])
    }

    #[test]
    fn gate_1_hides_when_spend_is_not_in_the_layout() {
        let lines = row([json!("model"), json!("cost"), json!("duration")]);
        let cache = cache_with(Some("team"), Some(data()));
        assert_eq!(verdict(Some(&cache), &cfg("always"), &lines, "$"), Verdict::Hidden {
            gate: Gate::NotInLayout
        });
    }

    #[test]
    fn gate_1_finds_spend_however_the_entry_is_spelled() {
        let one = |entry: Value| vec![vec![SegmentEntry::from(entry)]];
        assert!(in_layout(&one(json!("spend"))));
        assert!(in_layout(&one(json!({ "name": "spend", "bg": "red" }))));
        assert!(in_layout(&one(json!({ "id": "spend" }))));
        assert!(in_layout(&[vec![json!("model").into()], vec![json!("spend").into()]]), "any row counts");
        assert!(!in_layout(&one(json!("model"))));
        assert!(!in_layout(&one(json!({ "bg": "red" }))), "an entry with no id is not spend");
        assert!(!in_layout(&[]));
    }

    #[test]
    fn gate_2_hides_without_a_cache_or_without_data() {
        let lines = lines_with_spend();
        assert_eq!(verdict(None, &cfg("always"), &lines, "$"), Verdict::Hidden { gate: Gate::NoData });

        let empty = cache_with(Some("team"), None);
        assert_eq!(verdict(Some(&empty), &cfg("always"), &lines, "$"), Verdict::Hidden { gate: Gate::NoData });
    }

    #[test]
    fn gate_3_hides_on_a_strict_false_or_a_zero_limit() {
        let lines = lines_with_spend();

        let disabled = cache_with(Some("team"), Some(extract::Spend { enabled: Some(false), ..data() }));
        assert_eq!(verdict(Some(&disabled), &cfg("always"), &lines, "$"), Verdict::Hidden {
            gate: Gate::Disabled
        });

        let no_limit = cache_with(Some("team"), Some(extract::Spend { limit_minor: 0.0, ..data() }));
        assert_eq!(verdict(Some(&no_limit), &cfg("always"), &lines, "$"), Verdict::Hidden {
            gate: Gate::Disabled
        });
    }

    #[test]
    fn gate_3_does_not_hide_when_enabled_is_merely_absent() {
        let lines = lines_with_spend();
        let unknown = cache_with(Some("team"), Some(extract::Spend { enabled: None, ..data() }));
        assert!(matches!(verdict(Some(&unknown), &cfg("always"), &lines, "$"), Verdict::WillRender { .. }));
    }

    #[test]
    fn gate_4_hides_a_non_team_plan_only_under_auto() {
        let lines = lines_with_spend();
        let max = cache_with(Some("max"), Some(data()));

        assert_eq!(verdict(Some(&max), &cfg("auto"), &lines, "$"), Verdict::Hidden {
            gate: Gate::NotATeamPlan
        });
        // Anything other than "auto" renders — "always" is not special-cased.
        assert!(matches!(verdict(Some(&max), &cfg("always"), &lines, "$"), Verdict::WillRender { .. }));
        assert!(matches!(verdict(Some(&max), &cfg("yes"), &lines, "$"), Verdict::WillRender { .. }));
    }

    #[test]
    fn gate_4_lets_team_and_enterprise_through_under_auto() {
        let lines = lines_with_spend();
        for plan in ["team", "enterprise"] {
            let cache = cache_with(Some(plan), Some(data()));
            assert!(matches!(verdict(Some(&cache), &cfg("auto"), &lines, "$"), Verdict::WillRender { .. }), "{plan}");
        }
        // Case-sensitive, matching the original.
        let shouty = cache_with(Some("TEAM"), Some(data()));
        assert_eq!(verdict(Some(&shouty), &cfg("auto"), &lines, "$"), Verdict::Hidden {
            gate: Gate::NotATeamPlan
        });
        // And an unrecorded plan is not a team plan.
        let unknown = cache_with(None, Some(data()));
        assert_eq!(verdict(Some(&unknown), &cfg("auto"), &lines, "$"), Verdict::Hidden {
            gate: Gate::NotATeamPlan
        });
    }

    #[test]
    fn the_gates_are_checked_in_order() {
        // Everything is wrong at once; gate 1 is the one reported, because it
        // is the one that costs nothing to check.
        let lines = vec![vec![SegmentEntry::from(json!("model"))]];
        let broken = cache_with(None, None);
        assert_eq!(verdict(Some(&broken), &cfg("auto"), &lines, "$"), Verdict::Hidden {
            gate: Gate::NotInLayout
        });
    }

    #[test]
    fn the_text_is_the_contract_format() {
        let cache = cache_with(Some("team"), Some(data()));
        let out = verdict(Some(&cache), &cfg("always"), &lines_with_spend(), "\u{f09d}").text().unwrap().to_string();
        assert_eq!(out, "\u{f09d} $75.93/$150 (51%)");
    }

    #[test]
    fn a_supplied_percentage_wins_over_dividing() {
        let supplied = extract::Spend { percent: Some(0.0), ..data() };
        assert_eq!(percent_of(&supplied), 0.0, "an explicit 0 is honoured, not recomputed");

        let derived = extract::Spend { percent: None, ..data() };
        assert!((percent_of(&derived) - 50.62).abs() < 0.01);
    }
}
