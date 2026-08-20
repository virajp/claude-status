//! The subagent panel — the second rendering surface, selected by the
//! `--subagent` flag rather than by the payload's shape.
//!
//! The panel shares the powerline renderer, the config layers and the
//! formatting helpers with the main bar, and nothing else: it never reads the
//! spend cache, never spawns a refresh child, and never writes the usage
//! mirror.

use serde_json::Value;

use crate::config::Config;
use crate::config::color::Rgb;
use crate::config::matcher::Matcher;

/// What a matched status contributes to the head segment: its glyph, and the
/// background the **whole** head takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub symbol: String,
    pub bg: Rgb,
}

/// Resolves a task's `status` against `subagent.statuses`, **in config order** —
/// which is what `preserve_order` on `serde_json` is for. A `BTreeMap` would
/// reorder a user-authored config and silently change which entry wins.
///
/// Two details of the walk are load-bearing:
///
/// - an entry with an empty `match` is *recorded* as the fallback and the loop
///   **continues**, so with two such entries the **last** one wins;
/// - a pattern the engine rejects is skipped, not fatal — a hand-broken config
///   costs one status, never the render.
///
/// **Deviation:** an entry with no `symbol` contributes `""`. The JS
/// interpolated `undefined` and rendered the literal text, the same slip
/// `Config::symbol` already corrects for the main bar.
pub fn task_mark(status: &str, config: &Config) -> Mark {
    let lowered = status.to_lowercase();
    let statuses = config.get("subagent.statuses").and_then(Value::as_object);

    let mut fallback: Option<&Value> = None;
    for def in statuses.into_iter().flat_map(|defs| defs.values()) {
        // A non-object entry has no `match` either, so it lands here too.
        let Some(pattern) = def.get("match").and_then(Value::as_str).filter(|p| !p.is_empty()) else {
            fallback = Some(def);
            continue;
        };
        if Matcher::compile(pattern).is_ok_and(|m| m.is_match(&lowered)) {
            return mark(def, config);
        }
    }

    match fallback {
        Some(def) => mark(def, config),
        // Nothing matched and no entry declared itself the fallback.
        None => Mark { symbol: String::new(), bg: config.color(Some(&Value::String("bg3".into()))) },
    }
}

fn mark(def: &Value, config: &Config) -> Mark {
    Mark {
        symbol: def.get("symbol").and_then(Value::as_str).unwrap_or_default().to_string(),
        bg: config.color(def.get("bg")),
    }
}

/// The glyph for a task's `type`, falling back to `typeSymbols._default`.
///
/// **Never rendered as text.** Claude Code reports `type` as the generic
/// `"local_agent"` regardless of the real subagent type, so the string carries
/// no information the glyph does not.
///
/// Looked up on the object rather than through a dotted path, so a type
/// containing a `.` cannot silently read some other key.
pub fn type_glyph<'a>(kind: &str, config: &'a Config) -> &'a str {
    let symbols = config.get("typeSymbols");
    let lowered = kind.to_lowercase();
    // An empty entry falls back too, as `||` made it.
    let named = symbols.and_then(|s| s.get(&lowered)).and_then(Value::as_str).filter(|g| !g.is_empty());
    named.or_else(|| symbols.and_then(|s| s.get("_default")).and_then(Value::as_str)).unwrap_or_default()
}

/// The terminal width the description budget is computed from:
/// `payload.columns` → `$COLUMNS` → `80`.
///
/// The contract mentions neither the env var nor the default. Zero falls
/// through at each rung rather than winning, because the original chained them
/// with `||`.
pub fn columns(payload: &Value, env_columns: Option<&str>) -> f64 {
    let usable = |n: &f64| *n != 0.0 && !n.is_nan();
    let from_payload = payload.get("columns").and_then(js_number).filter(usable);
    let from_env = env_columns.and_then(|s| s.trim().parse::<f64>().ok()).filter(usable);
    from_payload.or(from_env).unwrap_or(80.0)
}

/// `Number(v)` for the shapes a config or payload can actually carry.
fn js_number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

/// `max(12, floor(cols × descBudgetFraction))`, in UTF-16 code units.
///
/// So 120 columns gives 54 and the absent-`columns` case gives 36. A fraction
/// of `0` is **kept** (the original used `??`, not `||`) and clamps to the
/// floor of 12.
///
/// **Divergence:** a non-numeric `descBudgetFraction` falls back to `0.45`
/// here. In the JS it produced a `NaN` budget, and `length > NaN` is false, so
/// the description was never truncated at all — a hand-broken config silently
/// disabling the budget is not worth reproducing.
pub fn desc_budget(cols: f64, config: &Config) -> usize {
    const DEFAULT_FRACTION: f64 = 0.45;
    let fraction = config.get("subagent.descBudgetFraction").and_then(Value::as_f64).unwrap_or(DEFAULT_FRACTION);
    let scaled = (cols * fraction).floor();
    if scaled.is_nan() { 12 } else { scaled.max(12.0) as usize }
}

/// `description` else `label` else nothing, with every whitespace run collapsed
/// to a single space and the result trimmed. An empty result omits the segment.
///
/// None of this is in the contract, and all of it is observable: a description
/// carrying a newline would otherwise break the row in half.
pub fn description(task: &Value) -> Option<String> {
    let raw = task
        .get("description")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .or_else(|| task.get("label").and_then(Value::as_str))?;

    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    (!collapsed.is_empty()).then_some(collapsed)
}

/// Truncates to `budget - 1` **UTF-16 code units** plus U+2026, so a truncated
/// description is exactly `budget` units long.
///
/// UTF-16 because that is what JS `String.length` and `slice` count, and the
/// panel is meant to be byte-identical to the bar it replaces. Two ways to get
/// this wrong: `&desc[..budget - 1]` **panics** on a non-char boundary, and a
/// cut through a surrogate pair needs the lossy decode — one replacement
/// character, which is itself one UTF-16 unit, so the width still holds.
pub fn truncate(desc: &str, budget: usize) -> String {
    let units: Vec<u16> = desc.encode_utf16().collect();
    if units.len() <= budget {
        return desc.to_string();
    }
    let mut out = String::from_utf16_lossy(&units[..budget - 1]);
    out.push('\u{2026}');
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::config::layers;

    const GREEN: Rgb = [152, 151, 26];
    const RED: Rgb = [204, 36, 29];
    const BLUE: Rgb = [69, 133, 136];
    const BG3: Rgb = [102, 92, 84];

    fn shipped() -> Config {
        layers::load(None, None).config
    }

    fn cfg(statuses: Value) -> Config {
        Config::new(json!({
            "palette": { "bg3": [102, 92, 84], "white": [251, 241, 199], "red": [204, 36, 29] },
            "subagent": { "statuses": statuses },
        }))
    }

    #[test]
    fn the_four_shipped_statuses_resolve_to_their_symbol_and_colour() {
        let config = shipped();
        for (status, symbol, bg) in [
            ("done", '\u{f00c}', GREEN),
            ("error", '\u{f00d}', RED),
            ("running", '\u{f04b}', BLUE),
            ("pending", '\u{f017}', BG3),
        ] {
            assert_eq!(task_mark(status, &config), Mark { symbol: symbol.to_string(), bg }, "status {status:?}");
        }
    }

    #[test]
    fn matching_is_an_unanchored_substring_so_not_ok_reads_as_done() {
        // `ok` is inside `not_ok`. Faithful to the original, and not a bug to
        // fix here: the patterns are the user's, and anchoring them would
        // change every existing config's behaviour.
        assert_eq!(task_mark("not_ok", &shipped()).bg, GREEN);
    }

    #[test]
    fn an_unknown_status_takes_the_empty_match_fallback() {
        let mark = task_mark("queued", &shipped());
        assert_eq!(mark.bg, BG3, "the shipped fallback is `pending`");
        assert_eq!(mark.symbol, '\u{f017}'.to_string());
    }

    #[test]
    fn status_matching_follows_config_order_not_alphabetical() {
        // `zebra` is declared first and matches, so it wins over `alpha`,
        // which also matches and would sort earlier.
        let config = cfg(json!({
            "zebra": { "match": "run", "symbol": "Z", "bg": [1, 1, 1] },
            "alpha": { "match": "running", "symbol": "A", "bg": [2, 2, 2] },
        }));
        assert_eq!(task_mark("running", &config), Mark { symbol: "Z".into(), bg: [1, 1, 1] });
    }

    #[test]
    fn with_two_empty_match_entries_the_last_one_wins() {
        // The loop records a fallback and keeps going rather than breaking.
        let config = cfg(json!({
            "first": { "match": "", "symbol": "1", "bg": [1, 1, 1] },
            "second": { "match": "", "symbol": "2", "bg": [2, 2, 2] },
        }));
        assert_eq!(task_mark("anything", &config), Mark { symbol: "2".into(), bg: [2, 2, 2] });
    }

    #[test]
    fn an_uncompilable_pattern_is_skipped_rather_than_fatal() {
        let config = cfg(json!({
            "broken": { "match": "(unclosed", "symbol": "B", "bg": [1, 1, 1] },
            "ok": { "match": "run", "symbol": "O", "bg": [2, 2, 2] },
        }));
        assert_eq!(task_mark("running", &config), Mark { symbol: "O".into(), bg: [2, 2, 2] });
    }

    #[test]
    fn no_match_and_no_fallback_lands_on_bg3() {
        let config = cfg(json!({ "only": { "match": "nothingalike", "symbol": "X", "bg": [1, 1, 1] } }));
        assert_eq!(task_mark("running", &config), Mark { symbol: String::new(), bg: BG3 });
    }

    #[test]
    fn an_absent_statuses_block_still_marks() {
        let config = Config::new(json!({ "palette": { "bg3": [102, 92, 84] } }));
        assert_eq!(task_mark("running", &config), Mark { symbol: String::new(), bg: BG3 });
    }

    #[test]
    fn a_missing_symbol_renders_empty_not_undefined() {
        let config = cfg(json!({ "done": { "match": "done", "bg": [1, 1, 1] } }));
        assert_eq!(task_mark("done", &config).symbol, "");
    }

    #[test]
    fn a_known_type_renders_its_glyph_and_an_unknown_one_the_default() {
        let config = shipped();
        assert_eq!(type_glyph("local_agent", &config), "\u{f109}");
        assert_eq!(type_glyph("review", &config), "\u{f06e}");
        assert_eq!(type_glyph("no_such_type", &config), "\u{f544}", "the _default glyph");
        assert_eq!(type_glyph("", &config), "\u{f544}");
    }

    #[test]
    fn type_lookup_is_case_insensitive_and_never_reads_a_dotted_path() {
        let config = Config::new(json!({
            "typeSymbols": { "_default": "D", "local_agent": "L", "a.b": "AB" },
            "a": { "b": "nested" },
        }));
        assert_eq!(type_glyph("LOCAL_AGENT", &config), "L");
        assert_eq!(type_glyph("a.b", &config), "AB", "the literal key, not typeSymbols → a → b");
    }

    #[test]
    fn an_empty_type_glyph_falls_back_to_the_default() {
        let config = Config::new(json!({ "typeSymbols": { "_default": "D", "task": "" } }));
        assert_eq!(type_glyph("task", &config), "D");
    }

    #[test]
    fn columns_resolve_payload_then_env_then_eighty() {
        assert_eq!(columns(&json!({ "columns": 120 }), None), 120.0);
        assert_eq!(columns(&json!({}), Some("100")), 100.0);
        assert_eq!(columns(&json!({}), None), 80.0);
        // Zero is not a width; it falls through at each rung, as `||` made it.
        assert_eq!(columns(&json!({ "columns": 0 }), Some("100")), 100.0);
        assert_eq!(columns(&json!({ "columns": 0 }), Some("0")), 80.0);
        assert_eq!(columns(&json!({ "columns": "nonsense" }), None), 80.0);
    }

    #[test]
    fn the_budget_is_a_fraction_of_the_columns_with_a_floor_of_twelve() {
        let config = shipped();
        assert_eq!(desc_budget(120.0, &config), 54);
        assert_eq!(desc_budget(80.0, &config), 36, "the absent-columns case");
        assert_eq!(desc_budget(20.0, &config), 12, "clamped");
    }

    #[test]
    fn a_zero_fraction_is_kept_and_clamps_to_twelve() {
        // `??`, not `||` — an explicit 0 is a real value.
        let config = Config::new(json!({ "subagent": { "descBudgetFraction": 0 } }));
        assert_eq!(desc_budget(1000.0, &config), 12);
    }

    #[test]
    fn a_negative_width_still_yields_the_floor_rather_than_panicking() {
        assert_eq!(desc_budget(-5.0, &shipped()), 12);
    }

    #[test]
    fn a_description_collapses_whitespace_and_falls_back_to_label() {
        assert_eq!(description(&json!({ "description": "  a\n\tb   c " })).as_deref(), Some("a b c"));
        assert_eq!(description(&json!({ "label": "from label" })).as_deref(), Some("from label"));
        assert_eq!(description(&json!({ "description": "", "label": "L" })).as_deref(), Some("L"));
        assert_eq!(description(&json!({})), None);
        assert_eq!(description(&json!({ "description": "   " })), None, "whitespace-only omits");
    }

    #[test]
    fn a_truncated_description_is_exactly_the_budget_in_utf16_units() {
        let long = "x".repeat(100);
        let out = truncate(&long, 54);
        assert_eq!(out.encode_utf16().count(), 54);
        assert!(out.ends_with('\u{2026}'), "one ellipsis character, not three dots");
        assert_eq!(truncate("short", 54), "short", "under budget is untouched");
        assert_eq!(truncate(&"y".repeat(54), 54), "y".repeat(54), "exactly at budget is untouched");
    }

    #[test]
    fn a_cut_through_a_surrogate_pair_does_not_panic_and_keeps_the_width() {
        // Each emoji is two UTF-16 units, so a budget of 13 cuts the seventh
        // one in half. A naive byte slice would panic here.
        let emojis = "\u{1f600}".repeat(20);
        let out = truncate(&emojis, 13);
        assert_eq!(out.encode_utf16().count(), 13, "the replacement char is one unit, like the half it replaced");
        assert!(out.ends_with('\u{2026}'));
    }

    #[test]
    fn a_multibyte_description_is_measured_in_units_not_bytes() {
        // Three-byte characters that are one UTF-16 unit each: 20 of them is
        // 60 bytes and 20 units, so a budget of 30 leaves it alone.
        let cjk = "\u{4e2d}".repeat(20);
        assert_eq!(truncate(&cjk, 30), cjk);
    }
}
