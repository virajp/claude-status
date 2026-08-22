//! The vwf context and rate-limit caps hook — a `PostToolUse` actuator.
//!
//! Context-window and rate-limit figures arrive **only** on the statusline
//! payload, never on hook stdin, so the bar mirrors them to
//! `<usage dir>/<session_id>.json` ([`crate::usage`]) and this reads that file
//! after each tool call. When a cap is breached it injects a directive telling
//! the agent to snapshot via `vwf:handoff` and halt.
//!
//! Ported from `context-caps.js` in `virajp/ai-plugins`, which paid Node's
//! 30–50 ms startup after **every tool call** — in the critical path of the
//! agent loop, for a program that reads one small file.
//!
//! Silence is the normal outcome. This surface writes nothing unless a cap is
//! breached *and* the breach is an escalation.

pub mod config;

use serde_json::{Value, json};

use crate::fmt::human_caps_in;
use crate::json::opt_f64;
pub use config::{Caps, DEFAULTS, resolve as resolve_caps};

/// The figures this hook compares against the caps, read from the mirror.
///
/// A missing percentage is `0`, which never breaches — the reference used
/// `?? 0` and an absent figure must not fire a cap.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub ctx_pct: f64,
    pub five_hour_pct: f64,
    pub seven_day_pct: f64,
    pub five_hour_resets_at: Option<Value>,
    pub seven_day_resets_at: Option<Value>,
    /// Percent of the monthly budget spent, for a seat that has one.
    ///
    /// **Not from the mirror.** The mirror is written by a render from the
    /// payload, which carries no spend figure; this comes from the spend cache
    /// the refresh child maintains. `None` on any seat without a budget block,
    /// which is most of them, and `None` never breaches.
    pub spend_pct: Option<f64>,
}

impl Usage {
    /// Reads the mirror document. Field names are [`crate::usage`]'s contract.
    pub fn from_mirror(doc: &Value) -> Self {
        Self {
            ctx_pct: opt_f64(doc, "ctxPct").unwrap_or(0.0),
            five_hour_pct: opt_f64(doc, "fiveHourPct").unwrap_or(0.0),
            seven_day_pct: opt_f64(doc, "sevenDayPct").unwrap_or(0.0),
            five_hour_resets_at: doc.get("fiveHourResetsAt").cloned(),
            seven_day_resets_at: doc.get("sevenDayResetsAt").cloned(),
            spend_pct: None,
        }
    }

    /// Attaches the budget percentage from the spend cache, if there is one.
    ///
    /// Separate from `from_mirror` because the two come from different files
    /// written by different things: the mirror is the render's, the cache is
    /// the refresh child's. Reading the cache is a local file open and no more
    /// — the hook never fetches, exactly as a render never does.
    #[must_use]
    pub fn with_spend(mut self, cache: Option<&crate::spend::cache::SpendCache>) -> Self {
        self.spend_pct = cache.and_then(|c| c.data.as_ref()).and_then(percent_of);
        self
    }
}

/// The budget percentage: the endpoint's own when it supplies one, else derived
/// from the amounts. A zero limit yields `None` rather than a division by zero.
fn percent_of(spend: &crate::spend::extract::Spend) -> Option<f64> {
    if let Some(pct) = spend.percent {
        return Some(pct);
    }
    (spend.limit_minor > 0.0).then(|| spend.used_minor / spend.limit_minor * 100.0)
}

/// The breach level and the directive it injects, or `None` for no breach.
///
/// Evaluated **in order** — 7-day, 5-hour, context — with the first breach
/// winning, so a session over both the 7-day and context caps reports the
/// 7-day one and says nothing about context. Comparison is strictly `>`, so a
/// figure exactly at its cap does not fire.
///
/// Levels are `4` spend, `3` 7-day, `2` 5-hour, `1` context; the debounce
/// compares them. Spend sits highest because a monthly budget is the one limit
/// that does not reset on its own — a 7-day window empties itself, an exhausted
/// budget needs somebody to act.
pub fn level(usage: &Usage, caps: &Caps, now_ms: i64) -> Option<(u8, String)> {
    if let Some(pct) = usage.spend_pct
        && pct > caps.spend as f64
    {
        return Some((4, spend_directive(pct, caps.spend)));
    }
    if usage.seven_day_pct > caps.seven_day as f64 {
        let resets = human_caps_in(usage.seven_day_resets_at.as_ref(), now_ms);
        return Some((3, seven_day_directive(usage.seven_day_pct, caps.seven_day, &resets)));
    }
    if usage.five_hour_pct > caps.five_hour as f64 {
        let resets = human_caps_in(usage.five_hour_resets_at.as_ref(), now_ms);
        return Some((2, five_hour_directive(usage.five_hour_pct, caps.five_hour, &resets)));
    }
    if usage.ctx_pct > caps.context as f64 {
        return Some((1, context_directive(usage.ctx_pct, caps.context)));
    }
    None
}

// The three directives. Their wording is injected verbatim into the agent's
// context and is what makes it stop rather than continue, so it is reproduced
// rather than improved — with one correction, noted at `HANDOFF_PATH`.

/// **Corrected from the reference**, which points at `docs/handoffs/next.md`.
/// The vwf handoff skill writes `docs/memory/handoff/next.md`; the JS text is
/// stale, and telling an agent to look in the wrong place is worse than saying
/// nothing.
const HANDOFF_PATH: &str = "docs/memory/handoff/next.md";

fn seven_day_directive(pct: f64, cap: u32, resets: &str) -> String {
    format!(
        "⛔ 7-DAY LIMIT CAP — weekly usage at {pct}% (cap {cap}%), resets in {resets}. \
         Finish ONLY the current step, then: (1) invoke the vwf:handoff skill with NO argument \
         (it writes the reserved `next` handoff to mempalace and {HANDOFF_PATH}); \
         (2) STOP and tell the user the 7-day limit is nearly exhausted and work is halted until \
         it resets — resume with /vwf:recall next. Do NOT start a new vwf stage or keep coding."
    )
}

fn five_hour_directive(pct: f64, cap: u32, resets: &str) -> String {
    format!(
        "⚠ 5-HOUR LIMIT CAP — 5h usage at {pct}% (cap {cap}%), resets in {resets}. \
         Finish ONLY the current step, then: (1) invoke the vwf:handoff skill with NO argument \
         (it writes the reserved `next` handoff); (2) STOP and tell the user work is paused until \
         the 5-hour window resets (~{resets}) — resume with /vwf:recall next after reset. \
         Do NOT continue now."
    )
}

fn spend_directive(pct: f64, cap: u32) -> String {
    let pct = crate::fmt::js_round(pct);
    format!(
        "⛔ SPEND CAP — monthly budget at {pct}% (cap {cap}%). This one does not reset on a timer. \
         Finish ONLY the current step, then: (1) invoke the vwf:handoff skill with NO argument \
         (it writes the reserved `next` handoff to mempalace and {HANDOFF_PATH}); \
         (2) STOP and tell the user the account's monthly budget is nearly exhausted, so somebody \
         has to raise it or wait for the billing period — resume with /vwf:recall next. \
         Do NOT start a new vwf stage or keep coding."
    )
}

fn context_directive(pct: f64, cap: u32) -> String {
    // The context figure is rounded where the two rate-limit ones are not —
    // it arrives as a fraction and the others as whole percentages.
    let pct = crate::fmt::js_round(pct);
    format!(
        "⚠ CONTEXT CAP — context window at {pct}% (cap {cap}%). \
         Finish ONLY the current step, then: (1) invoke the vwf:handoff skill with NO argument \
         (it writes the reserved `next` handoff to mempalace and {HANDOFF_PATH}); \
         (2) STOP and tell the user to run /clear (or /compact) then /vwf:recall next in a fresh \
         session to continue — the context cannot be cleared from inside this session. \
         Do NOT start a new vwf stage."
    )
}

/// The `PostToolUse` envelope Claude Code reads.
pub fn envelope(directive: &str) -> String {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": directive,
        }
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage_at(spend_pct: Option<f64>) -> Usage {
        Usage { spend_pct, ..Default::default() }
    }

    #[test]
    fn a_seat_with_no_budget_never_breaches_the_spend_cap() {
        // Most seats have no budget block at all. `None` must not read as 0%,
        // and must not read as breaching either.
        let caps = Caps { spend: 0, ..DEFAULTS };
        assert!(level(&usage_at(None), &caps, 0).is_none(), "a cap of 0 with no figure still cannot fire");
    }

    #[test]
    fn the_spend_cap_fires_above_its_threshold_and_not_at_it() {
        let caps = Caps { spend: 90, ..DEFAULTS };
        assert!(level(&usage_at(Some(90.0)), &caps, 0).is_none(), "exactly at the cap does not fire");

        let (lvl, directive) = level(&usage_at(Some(91.0)), &caps, 0).expect("91% is over 90%");
        assert_eq!(lvl, 4, "spend is the highest level");
        assert!(directive.contains("SPEND CAP"), "{directive}");
        assert!(directive.contains("cap 90%"), "{directive}");
    }

    #[test]
    fn spend_outranks_the_windows_that_reset_themselves() {
        // Both breached; spend wins, because a budget does not empty on a timer.
        let caps = Caps { context: 65, five_hour: 90, seven_day: 80, spend: 90 };
        let usage = Usage { seven_day_pct: 99.0, ctx_pct: 99.0, spend_pct: Some(99.0), ..Default::default() };
        let (lvl, directive) = level(&usage, &caps, 0).expect("something breached");
        assert_eq!(lvl, 4);
        assert!(directive.contains("SPEND CAP"), "{directive}");
    }

    #[test]
    fn the_budget_percentage_is_derived_when_the_endpoint_omits_one() {
        use crate::spend::extract::Spend;
        let derived = Spend { used_minor: 4500.0, limit_minor: 5000.0, exponent: 2, percent: None, enabled: None };
        assert_eq!(percent_of(&derived), Some(90.0));

        let given = Spend { percent: Some(12.5), ..derived.clone() };
        assert_eq!(percent_of(&given), Some(12.5), "the endpoint's own figure wins");

        let zero_limit = Spend { limit_minor: 0.0, percent: None, ..derived };
        assert_eq!(percent_of(&zero_limit), None, "no division by zero");
    }

    const NOW: i64 = 1_774_183_440_000;

    fn caps() -> Caps {
        DEFAULTS
    }

    fn usage(ctx: f64, five: f64, seven: f64) -> Usage {
        Usage { ctx_pct: ctx, five_hour_pct: five, seven_day_pct: seven, ..Default::default() }
    }

    #[test]
    fn each_cap_fires_on_its_own() {
        assert_eq!(level(&usage(66.0, 0.0, 0.0), &caps(), NOW).unwrap().0, 1);
        assert_eq!(level(&usage(0.0, 91.0, 0.0), &caps(), NOW).unwrap().0, 2);
        assert_eq!(level(&usage(0.0, 0.0, 81.0), &caps(), NOW).unwrap().0, 3);
    }

    #[test]
    fn the_most_severe_of_two_simultaneous_breaches_wins() {
        let (level_, directive) = level(&usage(99.0, 0.0, 85.0), &caps(), NOW).unwrap();
        assert_eq!(level_, 3);
        assert!(directive.contains("7-DAY"));
        assert!(!directive.contains("CONTEXT"), "the lesser breach is not mentioned");
    }

    #[test]
    fn exactly_at_the_cap_does_not_fire() {
        // Strictly greater. 65% context with a 65% cap is not a breach.
        assert!(level(&usage(65.0, 90.0, 80.0), &caps(), NOW).is_none());
    }

    #[test]
    fn absent_figures_do_not_fire() {
        assert!(level(&Usage::default(), &caps(), NOW).is_none());
        assert!(level(&Usage::from_mirror(&json!({})), &caps(), NOW).is_none());
    }

    #[test]
    fn a_tightened_cap_fires_where_the_shipped_one_would_not() {
        let tight = Caps { context: 50, ..DEFAULTS };
        assert!(level(&usage(55.0, 0.0, 0.0), &caps(), NOW).is_none());
        assert_eq!(level(&usage(55.0, 0.0, 0.0), &tight, NOW).unwrap().0, 1);
    }

    #[test]
    fn every_directive_names_its_cap_the_reset_and_the_handoff() {
        let mirror = json!({
            "ctxPct": 70,
            "fiveHourPct": 95,
            "sevenDayPct": 85,
            "fiveHourResetsAt": (NOW + 3_600_000) / 1000,
            "sevenDayResetsAt": (NOW + 2 * 86_400_000) / 1000,
        });
        let usage = Usage::from_mirror(&mirror);

        let (_, seven) = level(&usage, &caps(), NOW).unwrap();
        assert!(seven.contains("85%") && seven.contains("cap 80%"), "{seven}");
        assert!(seven.contains("resets in 2d0h"), "{seven}");
        assert!(seven.contains("vwf:handoff") && seven.contains("/vwf:recall next"));

        let five_only = Usage { seven_day_pct: 0.0, ..usage.clone() };
        let (_, five) = level(&five_only, &caps(), NOW).unwrap();
        assert!(five.contains("95%") && five.contains("cap 90%"), "{five}");
        assert!(five.contains("resets in 1h0m"), "{five}");

        let ctx_only = Usage { seven_day_pct: 0.0, five_hour_pct: 0.0, ..usage };
        let (_, ctx) = level(&ctx_only, &caps(), NOW).unwrap();
        assert!(ctx.contains("70%") && ctx.contains("cap 65%"), "{ctx}");
        assert!(ctx.contains("/clear") && ctx.contains("/vwf:recall next"), "{ctx}");
    }

    #[test]
    fn no_directive_points_at_the_stale_handoff_path() {
        // The reference says `docs/handoffs/next.md`; the skill writes
        // `docs/memory/handoff/next.md`.
        for level_ in [level(&usage(0.0, 0.0, 85.0), &caps(), NOW), level(&usage(70.0, 0.0, 0.0), &caps(), NOW)] {
            let directive = level_.unwrap().1;
            assert!(!directive.contains("docs/handoffs/"), "stale path: {directive}");
            assert!(directive.contains(HANDOFF_PATH) || !directive.contains("handoff to mempalace"));
        }
    }

    #[test]
    fn a_fractional_context_percentage_is_rounded_in_the_directive() {
        let (_, directive) = level(&usage(70.4, 0.0, 0.0), &caps(), NOW).unwrap();
        assert!(directive.contains("at 70%"), "{directive}");
    }

    #[test]
    fn the_envelope_is_the_shape_claude_code_reads() {
        let out = envelope("hello");
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["hookSpecificOutput"]["hookEventName"], "PostToolUse");
        assert_eq!(parsed["hookSpecificOutput"]["additionalContext"], "hello");
        assert!(!out.ends_with('\n'), "no trailing newline");
    }
}
