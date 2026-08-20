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
}
