//! Normalises the main-bar payload: `Value` in, plain owned facts out, so the
//! renderer below is pure and a golden test can build facts by hand.
//!
//! Every field is optional. Claude Code has changed this shape before and will
//! again, so a missing or unexpected field omits its segment — it never fails
//! the render. Malformed stdin parses to an empty object and renders a normal
//! bar of defaults; it does **not** trigger the panic fallback.

use serde_json::Value;

use crate::json::opt_str;

/// Everything the main bar needs from stdin, plus the injected clock.
///
/// `now_ms` is a field rather than a call so goldens can pin it: the reference
/// fixture's `resets_at` values are already in the past, and without an
/// injected clock every reset half would render `now` and the goldens would rot.
#[derive(Debug, Clone, Default)]
pub struct MainFacts {
    pub now_ms: i64,
    pub session_id: Option<String>,
    pub session_name: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub cwd: Option<String>,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<f64>,
    pub ctx_pct: Option<f64>,
    pub ctx_size: Option<f64>,
    pub ctx_used: Option<f64>,
    /// `resets_at` is kept **raw** — the usage mirror writes it unnormalised,
    /// and an ISO string must stay an ISO string for its consumer.
    pub five_hour: RateLimit,
    pub seven_day: RateLimit,
}

#[derive(Debug, Clone, Default)]
pub struct RateLimit {
    pub used_pct: Option<f64>,
    pub resets_at: Option<Value>,
}

/// Parses stdin. Anything that is not a JSON object becomes an empty object.
pub fn parse(input: &str) -> Value {
    match serde_json::from_str(input) {
        Ok(v @ Value::Object(_)) => v,
        _ => Value::Object(Default::default()),
    }
}

pub fn normalise(payload: &Value, now_ms: i64, process_cwd: Option<String>) -> MainFacts {
    MainFacts {
        now_ms,
        session_id: opt_str(payload, "session_id").map(str::to_string),
        session_name: opt_str(payload, "session_name").filter(|s| !s.is_empty()).map(str::to_string),
        model: model_label(payload.get("model")),
        effort: effort_label(payload.get("effort")),
        cwd: payload
            .get("workspace")
            .and_then(|w| w.get("current_dir"))
            .and_then(Value::as_str)
            .or_else(|| opt_str(payload, "cwd"))
            .map(str::to_string)
            .or(process_cwd),
        cost_usd: payload.get("cost").and_then(|c| c.get("total_cost_usd")).and_then(Value::as_f64),
        duration_ms: payload.get("cost").and_then(|c| c.get("total_duration_ms")).and_then(Value::as_f64),
        ctx_pct: context(payload, "used_percentage"),
        ctx_size: context(payload, "context_window_size"),
        ctx_used: context_used(payload),
        five_hour: rate_limit(payload, &["five_hour"]),
        seven_day: rate_limit(payload, &["seven_day", "weekly"]),
    }
}

/// The display label for a `model` field — an object with `display_name` or
/// `id`, or a bare string. Public because the subagent panel resolves the same
/// field per task and panel-wide.
pub fn model_label(v: Option<&Value>) -> Option<String> {
    label(v, &["display_name", "id"])
}

/// The label for an `effort` field — an object with `level`, or a bare string.
pub fn effort_label(v: Option<&Value>) -> Option<String> {
    label(v, &["level"])
}

/// `model` and `effort` each accept an object or a bare string.
///
/// A trailing parenthetical is stripped — `"Opus 5 (1M context)"` → `"Opus 5"` —
/// on the `id` branch too.
fn label(v: Option<&Value>, keys: &[&str]) -> Option<String> {
    let raw = match v? {
        Value::String(s) => s.as_str(),
        obj => keys.iter().find_map(|k| obj.get(k).and_then(Value::as_str))?,
    };
    let trimmed = strip_parenthetical(raw);
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn strip_parenthetical(s: &str) -> &str {
    match s.rfind(" (") {
        Some(i) if s.ends_with(')') => s[..i].trim_end(),
        _ => s.trim(),
    }
}

fn context(payload: &Value, key: &str) -> Option<f64> {
    payload.get("context_window")?.get(key)?.as_f64()
}

/// Context tokens: prefer `total_input_tokens` — it is what `used_percentage`
/// is computed from — else the sum of the three `current_usage` fields, else
/// derive from `used_percentage × context_window_size`.
///
/// The two rungs differ in how they treat zero, and it is not a slip:
/// `total_input_tokens: 0` is **kept** (the old code used `??`), but a
/// `current_usage` sum of exactly `0` **falls through** to the derived branch
/// (that rung used `||`).
fn context_used(payload: &Value) -> Option<f64> {
    let cw = payload.get("context_window")?;

    if let Some(total) = cw.get("total_input_tokens").and_then(Value::as_f64) {
        return Some(total);
    }

    if let Some(usage) = cw.get("current_usage") {
        let sum: f64 = ["input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"]
            .iter()
            .filter_map(|k| usage.get(k).and_then(Value::as_f64))
            .sum();
        if sum != 0.0 {
            return Some(sum);
        }
    }

    match (cw.get("used_percentage").and_then(Value::as_f64), cw.get("context_window_size").and_then(Value::as_f64)) {
        (Some(pct), Some(size)) => Some(pct / 100.0 * size),
        _ => None,
    }
}

/// `rate_limits.seven_day` **or** `weekly` — both spellings appear. `five_hour`
/// has no alias.
fn rate_limit(payload: &Value, keys: &[&str]) -> RateLimit {
    let Some(limits) = payload.get("rate_limits") else {
        return RateLimit::default();
    };
    let Some(win) = keys.iter().find_map(|k| limits.get(k)) else {
        return RateLimit::default();
    };
    RateLimit {
        used_pct: win.get("used_percentage").and_then(Value::as_f64),
        resets_at: win.get("resets_at").filter(|v| !v.is_null()).cloned(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn facts(payload: Value) -> MainFacts {
        normalise(&payload, 0, None)
    }

    #[test]
    fn malformed_stdin_becomes_an_empty_object() {
        for input in ["", "not json", "[1,2,3]", "\"a string\"", "null"] {
            assert_eq!(parse(input), json!({}), "{input:?} should parse to an empty object");
        }
        // Which then renders a normal bar of defaults, not the panic fallback.
        assert_eq!(facts(parse("garbage")).model, None);
    }

    #[test]
    fn model_accepts_an_object_or_a_bare_string() {
        assert_eq!(facts(json!({ "model": { "display_name": "Opus 5" } })).model.as_deref(), Some("Opus 5"));
        assert_eq!(facts(json!({ "model": "Opus 5" })).model.as_deref(), Some("Opus 5"));
        assert_eq!(facts(json!({ "model": { "id": "claude-opus-5" } })).model.as_deref(), Some("claude-opus-5"));
        assert_eq!(
            facts(json!({ "model": { "display_name": "Opus 5", "id": "x" } })).model.as_deref(),
            Some("Opus 5"),
            "display_name wins",
        );
        assert_eq!(facts(json!({ "model": {} })).model, None);
        assert_eq!(facts(json!({})).model, None);
    }

    #[test]
    fn a_trailing_parenthetical_is_stripped_on_both_branches() {
        assert_eq!(facts(json!({ "model": { "display_name": "Opus 5 (1M context)" } })).model.as_deref(), Some("Opus 5"));
        assert_eq!(facts(json!({ "model": { "id": "claude-opus-5 (1M)" } })).model.as_deref(), Some("claude-opus-5"));
        assert_eq!(facts(json!({ "model": "Opus 5 (1M context)" })).model.as_deref(), Some("Opus 5"));
        assert_eq!(
            facts(json!({ "model": "Sonnet (beta) 5" })).model.as_deref(),
            Some("Sonnet (beta) 5"),
            "only a *trailing* parenthetical is stripped",
        );
    }

    #[test]
    fn effort_accepts_an_object_or_a_bare_string() {
        assert_eq!(facts(json!({ "effort": { "level": "high" } })).effort.as_deref(), Some("high"));
        assert_eq!(facts(json!({ "effort": "high" })).effort.as_deref(), Some("high"));
        assert_eq!(facts(json!({ "effort": {} })).effort, None);
    }

    #[test]
    fn cwd_falls_through_workspace_then_cwd_then_the_process() {
        let p = json!({ "workspace": { "current_dir": "/a" }, "cwd": "/b" });
        assert_eq!(normalise(&p, 0, Some("/c".into())).cwd.as_deref(), Some("/a"));
        assert_eq!(normalise(&json!({ "cwd": "/b" }), 0, Some("/c".into())).cwd.as_deref(), Some("/b"));
        assert_eq!(normalise(&json!({}), 0, Some("/c".into())).cwd.as_deref(), Some("/c"));
        assert_eq!(normalise(&json!({}), 0, None).cwd, None);
    }

    #[test]
    fn context_tokens_prefer_total_input_tokens_and_keep_a_zero() {
        let p = json!({ "context_window": {
            "total_input_tokens": 0,
            "current_usage": { "input_tokens": 5 },
            "used_percentage": 26, "context_window_size": 1_000_000,
        }});
        assert_eq!(facts(p).ctx_used, Some(0.0), "an explicit 0 is kept, not fallen through");
    }

    #[test]
    fn a_current_usage_sum_of_zero_falls_through_to_the_derived_branch() {
        let p = json!({ "context_window": {
            "current_usage": { "input_tokens": 0, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0 },
            "used_percentage": 26, "context_window_size": 1_000_000,
        }});
        assert_eq!(facts(p).ctx_used, Some(260_000.0), "the asymmetry with total_input_tokens is deliberate");
    }

    #[test]
    fn a_nonzero_current_usage_sum_is_used() {
        let p = json!({ "context_window": {
            "current_usage": { "input_tokens": 1000, "cache_creation_input_tokens": 200, "cache_read_input_tokens": 34 },
            "used_percentage": 26, "context_window_size": 1_000_000,
        }});
        assert_eq!(facts(p).ctx_used, Some(1234.0));
    }

    #[test]
    fn with_no_context_window_there_is_nothing_to_derive_from() {
        assert_eq!(facts(json!({})).ctx_used, None);
        assert_eq!(facts(json!({ "context_window": { "used_percentage": 26 } })).ctx_used, None);
    }

    #[test]
    fn seven_day_accepts_the_weekly_spelling_and_five_hour_does_not() {
        let p = json!({ "rate_limits": { "weekly": { "used_percentage": 1.0, "resets_at": 1_774_600_000i64 } } });
        assert_eq!(facts(p).seven_day.used_pct, Some(1.0));

        let p = json!({ "rate_limits": { "seven_day": { "used_percentage": 2.0 } } });
        assert_eq!(facts(p).seven_day.used_pct, Some(2.0));

        // No alias on this side.
        let p = json!({ "rate_limits": { "5h": { "used_percentage": 7.0 } } });
        assert_eq!(facts(p).five_hour.used_pct, None);
    }

    #[test]
    fn resets_at_is_carried_raw() {
        let p = json!({ "rate_limits": { "five_hour": { "resets_at": "2026-08-19T12:00:00Z" } } });
        assert_eq!(facts(p).five_hour.resets_at, Some(json!("2026-08-19T12:00:00Z")), "an ISO string stays an ISO string");

        let p = json!({ "rate_limits": { "five_hour": { "resets_at": null } } });
        assert_eq!(facts(p).five_hour.resets_at, None);
    }

    #[test]
    fn an_empty_session_name_is_absent() {
        assert_eq!(facts(json!({ "session_name": "" })).session_name, None);
        assert_eq!(facts(json!({ "session_name": "x" })).session_name.as_deref(), Some("x"));
    }

    #[test]
    fn cost_and_duration_keep_an_explicit_zero() {
        let f = facts(json!({ "cost": { "total_cost_usd": 0, "total_duration_ms": 0 } }));
        assert_eq!(f.cost_usd, Some(0.0));
        assert_eq!(f.duration_ms, Some(0.0), "0 renders `0s`; only absence omits the segment");
    }
}
