//! The subagent panel — the second rendering surface, selected by the
//! `--subagent` flag rather than by the payload's shape.
//!
//! The panel shares the powerline renderer, the config layers and the
//! formatting helpers with the main bar, and nothing else: it never reads the
//! spend cache, never spawns a refresh child, and never writes the usage
//! mirror.

use serde_json::{Value, json};

use crate::config::Config;
use crate::config::color::Rgb;
use crate::config::matcher::Matcher;
use crate::fmt::{human_duration, human_tokens};
use crate::payload;
use crate::render::powerline::{Powerline, Segment, render};
use crate::render::segments::truthy;
use crate::time::to_epoch_ms;

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

/// The panel-wide facts every row shares, resolved **once** rather than per
/// task — including the head's foreground and weight, which the original also
/// hoisted out of the loop.
pub struct Panel {
    model: Option<String>,
    effort: Option<String>,
    budget: usize,
    now_ms: i64,
    head_fg: Rgb,
    head_bold: bool,
}

impl Panel {
    pub fn new(payload: &Value, config: &Config, now_ms: i64, env_columns: Option<&str>) -> Self {
        let head = config.get("subagent.segments.head");
        let field = |name: &str| head.and_then(|h| h.get(name)).filter(present);
        Self {
            model: payload::model_label(payload.get("model")),
            effort: payload::effort_label(payload.get("effort")),
            budget: desc_budget(columns(payload, env_columns), config),
            now_ms,
            // Only `fg` and `bold` are read here: the head's background always
            // comes from the matched status, so `segments.head.bg` is ignored.
            head_fg: config.color(field("fg").or_else(|| config.default_fg())),
            head_bold: field("bold").is_some_and(truthy),
        }
    }
}

/// A styling value counts as set only when it is neither `null` nor `""` — the
/// original reached for each with `||`, so an empty string falls back.
fn present(v: &&Value) -> bool {
    !v.is_null() && v.as_str() != Some("")
}

/// One subagent segment's styling: `subagent.segments.<key>` over the hard
/// per-segment fallback. `fg` resolves to `defaultFg` when unset, as it does
/// for the main bar.
fn style(key: &str, fallback_bg: &str, config: &Config) -> (Rgb, Rgb, bool) {
    let seg = config.get(&format!("subagent.segments.{key}"));
    let field = |name: &str| seg.and_then(|s| s.get(name)).filter(present);
    let fallback = Value::String(fallback_bg.to_string());
    (
        config.color(field("bg").or(Some(&fallback))),
        config.color(field("fg").or_else(|| config.default_fg())),
        field("bold").is_some_and(truthy),
    )
}

/// One task's row: `head`, `name`, `model`, `desc`, `tokens`, `duration`, each
/// conditional.
pub fn task_row(task: &Value, panel: &Panel, config: &Config) -> Vec<Segment> {
    let str_field = |key: &str| task.get(key).and_then(Value::as_str).unwrap_or_default();
    let sym = |key: &str| config.symbol(key);
    let mut segments = Vec::with_capacity(6);

    // head — the space between the two glyphs is unconditional, even when the
    // status symbol is empty, so the type glyph never shifts left.
    let mark = task_mark(str_field("status"), config);
    let glyph = type_glyph(str_field("type"), config);
    segments.push(Segment {
        text: format!("{} {glyph}", mark.symbol),
        bg: mark.bg,
        fg: panel.head_fg,
        bold: panel.head_bold,
    });

    // name — **no fallback to `type`**. The contract's §3 table says there is
    // one; the reference implementation has none, and the type glyph already
    // carries what the fallback would have shown.
    if let Some(name) = task.get("name").and_then(Value::as_str).filter(|n| !n.is_empty()) {
        let (bg, fg, bold) = style("name", "orange", config);
        segments.push(Segment { text: format!("{} {name}", sym("agent")), bg, fg, bold });
    }

    // model — a per-task value wins over the panel-wide one, and effort with no
    // model renders just `[high]`.
    let model = payload::model_label(task.get("model")).or_else(|| panel.model.clone());
    let effort = payload::effort_label(task.get("effort")).or_else(|| panel.effort.clone());
    let parts: Vec<String> = [model, effort.map(|e| format!("[{e}]"))].into_iter().flatten().collect();
    if !parts.is_empty() {
        let (bg, fg, bold) = style("model", "blue", config);
        segments.push(Segment { text: format!("{} {}", sym("model"), parts.join(" ")), bg, fg, bold });
    }

    if let Some(desc) = description(task) {
        let (bg, fg, bold) = style("desc", "bg3", config);
        segments.push(Segment { text: truncate(&desc, panel.budget), bg, fg, bold });
    }

    // tokens — present-but-not-null, so an honest `0` renders `0`.
    if let Some(count) = task.get("tokenCount").filter(present) {
        let (bg, fg, bold) = style("tokens", "aqua", config);
        segments.push(Segment { text: format!("{} {}", sym("tokens"), human_tokens(js_number(count))), bg, fg, bold });
    }

    // duration — skipped when `startTime` does not parse **or is falsy**, and
    // allowed to go negative for a future start rather than being clamped.
    if let Some(start) = task.get("startTime").and_then(to_epoch_ms).filter(|ms| *ms != 0) {
        let (bg, fg, bold) = style("duration", "purple", config);
        let elapsed = panel.now_ms.saturating_sub(start) as f64;
        segments.push(Segment { text: format!("{} {}", sym("duration"), human_duration(Some(elapsed))), bg, fg, bold });
    }

    segments
}

/// The panel's cwd chain, which is **not** the main bar's: `payload.cwd` → the
/// first task's `cwd` → the process cwd. `workspace` is never consulted.
///
/// It still decides which repo config layer loads, so a panel does pick up
/// per-repo theming.
pub fn panel_cwd(payload: &Value, process_cwd: Option<String>) -> Option<String> {
    let nonempty = |v: &&Value| v.as_str().is_some_and(|s| !s.is_empty());
    payload
        .get("cwd")
        .filter(nonempty)
        .or_else(|| payload.get("tasks")?.as_array()?.first()?.get("cwd").filter(nonempty))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or(process_cwd)
}

/// The whole panel: one `{"id": …, "content": …}` object per task, joined with
/// `\n` and with **no trailing newline**.
///
/// A payload carrying no `tasks` array renders **empty** and never falls back
/// to the main bar — the flag chose this surface, and a silent surface swap is
/// the failure the flags exist to prevent.
///
/// A task that is not an object, or whose `id` is absent or `null`, is skipped;
/// `id: 0` and `id: ""` are kept, and an `id` keeps its original JSON type.
pub fn render_panel(payload: &Value, config: &Config, now_ms: i64, env_columns: Option<&str>) -> String {
    let Some(tasks) = payload.get("tasks").and_then(Value::as_array) else {
        return String::new();
    };

    let panel = Panel::new(payload, config, now_ms, env_columns);
    let powerline = Powerline::from_config(config);

    tasks
        .iter()
        .filter_map(|task| {
            let id = task.get("id").filter(|v| !v.is_null())?;
            let content = render(&task_row(task, &panel, config), &powerline);
            Some(json!({ "id": id, "content": content }).to_string())
        })
        .collect::<Vec<_>>()
        .join("\n")
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

    /// The clock the row tests pin, so `duration` never renders "now".
    const NOW: i64 = 1_774_200_000_000;

    fn panel(payload: &Value, config: &Config) -> Panel {
        Panel::new(payload, config, NOW, None)
    }

    /// A row's segment texts, which is what the assertions below are about —
    /// the powerline seams are `powerline.rs`'s tests, not these.
    fn texts(task: &Value, payload: &Value, config: &Config) -> Vec<String> {
        task_row(task, &panel(payload, config), config).iter().map(|s| s.text.clone()).collect()
    }

    #[test]
    fn the_full_row_is_six_segments_in_order() {
        let config = shipped();
        let task = json!({
            "id": "t1",
            "name": "reviewer",
            "type": "review",
            "status": "running",
            "description": "Auditing auth flow",
            "tokenCount": 18234,
            "startTime": NOW - 125_000,
        });
        let payload = json!({ "columns": 120, "model": { "display_name": "Opus 5" }, "effort": { "level": "high" } });
        assert_eq!(texts(&task, &payload, &config), vec![
            "\u{f04b} \u{f06e}".to_string(),
            "\u{f007} reviewer".to_string(),
            "\u{26a1} Opus 5 [high]".to_string(),
            "Auditing auth flow".to_string(),
            "\u{f51e} 18k".to_string(),
            "\u{f017} 2m 5s".to_string(),
        ]);
    }

    #[test]
    fn every_optional_segment_omits_on_its_own() {
        let config = shipped();
        // A task with a status and nothing else is the head alone.
        assert_eq!(texts(&json!({ "id": 1, "status": "done" }), &json!({}), &config), vec![
            "\u{f00c} \u{f544}".to_string()
        ]);
    }

    #[test]
    fn the_head_keeps_its_space_even_when_the_status_symbol_is_empty() {
        let config = cfg(json!({ "unmatched": { "match": "nothingalike", "symbol": "X" } }));
        let row = texts(&json!({ "id": 1, "type": "task" }), &json!({}), &config);
        assert_eq!(row[0], " ", "an empty symbol, the unconditional space, and no glyph configured");
    }

    #[test]
    fn the_head_background_comes_from_the_status_and_ignores_its_own_bg() {
        let config = Config::new(json!({
            "palette": { "green": [152, 151, 26], "white": [251, 241, 199] },
            "subagent": {
                "segments": { "head": { "bg": "white", "bold": true } },
                "statuses": { "done": { "match": "done", "symbol": "D", "bg": "green" } },
            },
        }));
        let row = task_row(&json!({ "id": 1, "status": "done" }), &panel(&json!({}), &config), &config);
        assert_eq!(row[0].bg, GREEN, "segments.head.bg is deliberately not read");
        assert!(row[0].bold, "but bold is");
    }

    #[test]
    fn the_name_never_falls_back_to_the_type() {
        // The contract's §3 table says it does. The reference implementation
        // does not, and the type glyph already carries the same information.
        let config = shipped();
        let row = texts(&json!({ "id": 1, "type": "local_agent" }), &json!({}), &config);
        assert_eq!(row.len(), 1, "head only: {row:?}");
        assert!(!row.iter().any(|t| t.contains("local_agent")), "the type is never text");
    }

    #[test]
    fn a_per_task_model_wins_over_the_panel_wide_one() {
        let config = shipped();
        let payload = json!({ "model": { "display_name": "Opus 5" }, "effort": { "level": "high" } });
        let row = texts(&json!({ "id": 1, "model": "Haiku 4.5" }), &payload, &config);
        assert_eq!(row[1], "\u{26a1} Haiku 4.5 [high]", "per-task model, panel-wide effort");
    }

    #[test]
    fn effort_with_no_model_renders_the_bracket_alone() {
        let config = shipped();
        let row = texts(&json!({ "id": 1 }), &json!({ "effort": { "level": "high" } }), &config);
        assert_eq!(row[1], "\u{26a1} [high]");
    }

    #[test]
    fn with_neither_model_nor_effort_the_segment_omits() {
        let config = shipped();
        assert_eq!(texts(&json!({ "id": 1 }), &json!({}), &config).len(), 1, "head only");
    }

    #[test]
    fn a_token_count_of_zero_renders_rather_than_omitting() {
        let config = shipped();
        let row = texts(&json!({ "id": 1, "tokenCount": 0 }), &json!({}), &config);
        assert_eq!(row[1], "\u{f51e} 0");
        assert_eq!(texts(&json!({ "id": 1, "tokenCount": null }), &json!({}), &config).len(), 1, "null omits");
    }

    #[test]
    fn a_duration_is_skipped_when_start_time_is_falsy_or_unparseable() {
        let config = shipped();
        for start in [json!(0), json!("not a date"), json!(null), json!(true)] {
            let row = texts(&json!({ "id": 1, "startTime": start }), &json!({}), &config);
            assert_eq!(row.len(), 1, "startTime {start:?} should omit the segment: {row:?}");
        }
    }

    #[test]
    fn a_future_start_time_goes_negative_rather_than_clamping() {
        let config = shipped();
        let row = texts(&json!({ "id": 1, "startTime": NOW + 5_000 }), &json!({}), &config);
        // Reproduced from the original's arithmetic rather than clamped to
        // zero: floor(-5s) lands on the minutes branch.
        assert_eq!(row[1], "\u{f017} 59m 55s");
    }

    #[test]
    fn the_description_is_truncated_to_the_payloads_budget() {
        let config = shipped();
        let long = "word ".repeat(40);
        let row = texts(&json!({ "id": 1, "description": long }), &json!({ "columns": 120 }), &config);
        assert_eq!(row[1].encode_utf16().count(), 54);
    }

    #[test]
    fn the_panel_renders_one_ndjson_object_per_task_with_no_trailing_newline() {
        let config = shipped();
        let payload = json!({ "columns": 120, "tasks": [
            { "id": "t1", "status": "running" },
            { "id": "t2", "status": "done" },
        ] });
        let out = render_panel(&payload, &config, NOW, None);
        assert_eq!(out.lines().count(), 2);
        assert!(!out.ends_with('\n'), "no trailing newline");
        for line in out.lines() {
            let parsed: Value = serde_json::from_str(line).expect("each line is its own object");
            assert!(parsed.get("id").is_some() && parsed.get("content").is_some());
        }
    }

    #[test]
    fn a_payload_with_no_tasks_array_renders_empty_and_never_the_main_bar() {
        let config = shipped();
        assert_eq!(render_panel(&json!({}), &config, NOW, None), "");
        assert_eq!(render_panel(&json!({ "tasks": [] }), &config, NOW, None), "");
        assert_eq!(render_panel(&json!({ "tasks": "not an array" }), &config, NOW, None), "");
    }

    #[test]
    fn a_task_without_an_id_is_skipped_and_the_rest_still_render() {
        let config = shipped();
        let payload = json!({ "tasks": [
            { "status": "running" },
            { "id": null, "status": "running" },
            "not an object",
            { "id": "kept", "status": "running" },
        ] });
        let out = render_panel(&payload, &config, NOW, None);
        assert_eq!(out.lines().count(), 1);
        assert!(out.contains("\"kept\""));
    }

    #[test]
    fn an_id_keeps_its_original_json_type() {
        let config = shipped();
        let payload = json!({ "tasks": [{ "id": 0 }, { "id": "" }] });
        let out = render_panel(&payload, &config, NOW, None);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2, "0 and \"\" are both kept");
        assert!(lines[0].starts_with(r#"{"id":0,"#), "a number stays a number: {}", lines[0]);
        assert!(lines[1].starts_with(r#"{"id":"","#), "an empty string stays a string: {}", lines[1]);
    }

    #[test]
    fn esc_is_escaped_but_the_nerd_font_glyphs_stay_raw() {
        let config = shipped();
        let out = render_panel(&json!({ "tasks": [{ "id": 1, "status": "done" }] }), &config, NOW, None);
        assert!(out.contains("\\u001b"), "ESC serialises as the six-character escape");
        assert!(!out.contains('\u{1b}'), "and never raw");
        assert!(out.contains('\u{f00c}'), "the status glyph stays raw UTF-8: {out}");
    }

    #[test]
    fn the_panel_cwd_chain_is_payload_then_first_task_then_process() {
        let task_cwd = json!({ "tasks": [{ "cwd": "/from/task" }] });
        assert_eq!(panel_cwd(&json!({ "cwd": "/from/payload" }), None).as_deref(), Some("/from/payload"));
        assert_eq!(panel_cwd(&task_cwd, None).as_deref(), Some("/from/task"));
        assert_eq!(panel_cwd(&json!({}), Some("/process".into())).as_deref(), Some("/process"));
        assert_eq!(panel_cwd(&json!({ "cwd": "" }), Some("/process".into())).as_deref(), Some("/process"));
    }

    #[test]
    fn the_panel_never_consults_workspace() {
        // `workspace.current_dir` is a main-bar field. Reading it here would
        // silently give the panel a different repo's config.
        let payload = json!({ "workspace": { "current_dir": "/from/workspace" } });
        assert_eq!(panel_cwd(&payload, Some("/process".into())).as_deref(), Some("/process"));
    }
}
