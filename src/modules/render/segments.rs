//! The ten main-bar segment builders, and the styling that wraps them.
//!
//! A builder returning `None` means "no data" and **omits the segment
//! entirely** — it does not render an empty box.

use std::panic::{AssertUnwindSafe, catch_unwind};

use serde_json::Value;

use crate::config::{Config, FALLBACK_BG};
use crate::fmt::{gauge, human_duration, human_reset_in, human_tokens, to_fixed};
use crate::git::GitFacts;
use crate::payload::{MainFacts, RateLimit};
use crate::render::powerline::Segment;
use crate::time::to_epoch_ms;

/// Every segment id the bar knows.
pub const KNOWN: [&str; 11] =
    ["model", "context", "rl5h", "rl7d", "session", "cost", "spend", "duration", "project", "worktree", "branch"];

/// Builds one line's segments, dropping every one that omits.
pub fn build_line(
    entries: &[Value],
    facts: &MainFacts,
    git: &GitFacts,
    config: &Config,
    spend: Option<&str>,
) -> Vec<Segment> {
    entries.iter().filter_map(|entry| build(entry, facts, git, config, spend)).collect()
}

fn build(entry: &Value, facts: &MainFacts, git: &GitFacts, config: &Config, spend: Option<&str>) -> Option<Segment> {
    // An entry is a bare segment id, or an object keyed by `name` **or** `id`
    // carrying inline styling overrides.
    let id = match entry {
        Value::String(id) => id.as_str(),
        obj => obj.get("name").or_else(|| obj.get("id"))?.as_str()?,
    };

    if !KNOWN.contains(&id) {
        // Warn and omit. Never fail the render, and never touch stdout.
        crate::_shared::diag(&format!("statusline: unknown segment {id:?}"));
        return None;
    }

    // A panicking builder costs its own segment and nothing else.
    let text = catch_unwind(AssertUnwindSafe(|| text_for(id, facts, git, config, spend))).ok().flatten()?;
    // Every segment's text passes through here, which is why the filter lives
    // at this one point rather than in each builder.
    let text = super::sanitize(&text);

    // Inline override → `segments.<id>` default → hard fallback. An explicit
    // `null` at either level falls through, as `??` made it, while `false` and
    // `0` do not.
    let inline = entry.as_object();
    let style = |key: &str| {
        let present = |v: &&Value| !v.is_null();
        inline
            .and_then(|o| o.get(key))
            .filter(present)
            .or_else(|| config.get(&format!("segments.{id}.{key}")).filter(present))
    };

    let fallback_bg = Value::String(FALLBACK_BG.into());
    Some(Segment {
        text,
        bg: config.color(style("bg").or(Some(&fallback_bg))),
        // No hard fallback for `fg`: it resolves to `defaultFg` at render time.
        fg: config.color(style("fg").or_else(|| config.default_fg())),
        bold: style("bold").is_some_and(truthy),
    })
}

/// JavaScript truthiness, because `bold` was coerced with `!!` — a config
/// carrying `"bold": 1` meant bold.
pub(crate) fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn text_for(id: &str, facts: &MainFacts, git: &GitFacts, config: &Config, spend: Option<&str>) -> Option<String> {
    let sym = |key: &str| config.symbol(key);
    match id {
        "model" => Some(model(facts, config)),
        "context" => Some(context(facts, config)),
        "rl5h" => rate_limit(&facts.five_hour, facts.now_ms, sym("win5h"), config),
        "rl7d" => rate_limit(&facts.seven_day, facts.now_ms, sym("win7d"), config),
        "session" => facts.session_name.as_ref().map(|name| format!("{} {name}", sym("session"))),
        // Never omits: an absent cost renders zero.
        "cost" => Some(format!("{} ${}", sym("cost"), to_fixed(facts.cost_usd.unwrap_or(0.0), 2))),
        // Resolved before the render began, because gate 1 has to be answered
        // before the cache is opened. `None` means one of the four gates hid
        // it, and the segment omits like any other.
        "spend" => spend.map(str::to_string),
        // Omitted only when absent — `0` renders `0s`.
        "duration" => facts.duration_ms.map(|ms| format!("{} {}", sym("duration"), human_duration(Some(ms)))),
        "project" => config.project_name().map(|name| format!("{} {name}", sym("project"))),
        "worktree" => git
            .worktree_subpath
            .as_ref()
            .map(|sub| format!("{} {} {sub}", sym("worktree"), sym("folder"))),
        "branch" => branch(git, config),
        _ => None,
    }
}

/// `{model} Opus 5 [high]`. Falls back to `Claude`, including when the
/// display name resolved to the empty string.
fn model(facts: &MainFacts, config: &Config) -> String {
    let name = facts.model.as_deref().filter(|n| !n.is_empty()).unwrap_or("Claude");
    let mut out = format!("{} {name}", config.symbol("model"));
    if let Some(effort) = facts.effort.as_deref().filter(|e| !e.is_empty()) {
        out.push_str(&format!(" [{effort}]"));
    }
    out
}

/// `{context} ▰▰▰▱▱▱▱▱▱▱ 259k/1M (26%)`.
///
/// Never omits: with no data at all it renders an empty gauge and `?/? (0%)`.
fn context(facts: &MainFacts, config: &Config) -> String {
    let bar = gauge(facts.ctx_pct, config.gauge_width(), config.gauge_glyph("filled"), config.gauge_glyph("empty"));
    let pct = facts.ctx_pct.unwrap_or(0.0).round();
    format!(
        "{} {bar} {}/{} ({pct}%)",
        config.symbol("context"),
        human_tokens(facts.ctx_used),
        human_tokens(facts.ctx_size),
    )
}

/// `{win5h} 7.0% {reset} 4h36m` — note the space *before* the reset glyph.
///
/// Omitted entirely when `used_percentage` is absent; the reset half alone is
/// dropped when the timestamp does not parse.
fn rate_limit(limit: &RateLimit, now_ms: i64, symbol: &str, config: &Config) -> Option<String> {
    let pct = limit.used_pct?;
    let mut out = format!("{symbol} {}%", to_fixed(pct, 1));

    let resets_in = limit.resets_at.as_ref().and_then(to_epoch_ms).and_then(|ms| human_reset_in(Some(ms), now_ms));
    if let Some(resets_in) = resets_in {
        out.push_str(&format!(" {} {resets_in}", config.symbol("reset")));
    }
    out
        .into()
}

/// `{worktree} {branch} main ↑ ±` — every part after the branch glyph is
/// conditional, and each is preceded by exactly one space.
fn branch(git: &GitFacts, config: &Config) -> Option<String> {
    let name = git.branch.as_deref().filter(|b| !b.is_empty())?;

    let mut parts: Vec<&str> = Vec::with_capacity(5);
    if git.worktree_subpath.is_some() {
        parts.push(config.symbol("worktree"));
    }
    parts.push(config.symbol("branch"));
    parts.push(name);
    if git.ahead {
        parts.push(config.symbol("ahead"));
    }
    if let Some(mark) = dirty_symbol(git, config) {
        parts.push(mark);
    }
    Some(parts.join(" "))
}

/// `±` for both, `+` for additions, `-` for deletions, nothing for clean.
fn dirty_symbol<'a>(git: &GitFacts, config: &'a Config) -> Option<&'a str> {
    let key = match (git.additions > 0, git.deletions > 0) {
        (true, true) => "dirtyMix",
        (true, false) => "dirtyAdd",
        (false, true) => "dirtyDel",
        (false, false) => return None,
    };
    Some(config.symbol(key)).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::config::layers;

    fn config() -> Config {
        layers::load(None, None).config
    }

    /// Renders one segment's text, or `None` if it omits.
    fn text(id: &str, facts: &MainFacts, git: &GitFacts) -> Option<String> {
        text_for(id, facts, git, &config(), None)
    }

    fn facts() -> MainFacts {
        MainFacts {
            now_ms: 1_774_183_440_000, // 4h36m before the five-hour reset below
            model: Some("Opus 5".into()),
            effort: Some("high".into()),
            session_name: Some("users-and-groups".into()),
            cost_usd: Some(46.51),
            duration_ms: Some(33_540_000.0),
            ctx_pct: Some(26.0),
            ctx_size: Some(1_000_000.0),
            ctx_used: Some(259_000.0),
            five_hour: RateLimit { used_pct: Some(7.0), resets_at: Some(json!(1_774_200_000i64)) },
            seven_day: RateLimit { used_pct: Some(1.0), resets_at: Some(json!(1_774_600_000i64)) },
            ..Default::default()
        }
    }

    #[test]
    fn a_malicious_branch_name_reaches_the_row_defanged() {
        // Through `build`, not `text_for` — the filter is only worth anything
        // if it sits on the path the renderer actually takes.
        let git = GitFacts {
            branch: Some("main\u{1b}[0m\u{1b}[41mPWNED".into()),
            ..Default::default()
        };
        let segment = build(&json!("branch"), &facts(), &git, &config(), None).unwrap();
        assert!(!segment.text.contains('\u{1b}'), "got {:?}", segment.text);
        assert!(segment.text.contains("PWNED"), "the text survives, only the escapes go");
    }

    #[test]
    fn model_renders_the_name_and_effort() {
        let c = config();
        assert_eq!(model(&facts(), &c), format!("{} Opus 5 [high]", c.symbol("model")));
    }

    #[test]
    fn model_falls_back_to_claude() {
        let c = config();
        let expected = format!("{} Claude", c.symbol("model"));
        assert_eq!(model(&MainFacts::default(), &c), expected);
        // Including when the display name resolved to the empty string.
        assert_eq!(model(&MainFacts { model: Some(String::new()), ..Default::default() }, &c), expected);
    }

    #[test]
    fn model_drops_an_empty_effort() {
        let c = config();
        let f = MainFacts { effort: Some(String::new()), ..facts() };
        assert_eq!(model(&f, &c), format!("{} Opus 5", c.symbol("model")));
    }

    #[test]
    fn context_renders_the_gauge_and_both_token_counts() {
        let c = config();
        assert_eq!(context(&facts(), &c), format!("{} ▰▰▰▱▱▱▱▱▱▱ 259k/1M (26%)", c.symbol("context")));
    }

    #[test]
    fn context_with_no_data_still_renders() {
        let c = config();
        assert_eq!(context(&MainFacts::default(), &c), format!("{} ▱▱▱▱▱▱▱▱▱▱ ?/? (0%)", c.symbol("context")));
    }

    #[test]
    fn a_rate_limit_carries_a_space_before_the_reset_glyph() {
        let c = config();
        let out = text("rl5h", &facts(), &GitFacts::default()).unwrap();
        assert_eq!(out, format!("{} 7.0% {} 4h36m", c.symbol("win5h"), c.symbol("reset")));
    }

    #[test]
    fn a_rate_limit_drops_only_the_reset_half_when_the_timestamp_is_unparseable() {
        let c = config();
        let mut f = facts();
        f.five_hour.resets_at = Some(json!("not a date"));
        assert_eq!(text("rl5h", &f, &GitFacts::default()).unwrap(), format!("{} 7.0%", c.symbol("win5h")));

        f.five_hour.resets_at = None;
        assert_eq!(text("rl5h", &f, &GitFacts::default()).unwrap(), format!("{} 7.0%", c.symbol("win5h")));
    }

    #[test]
    fn a_rate_limit_omits_without_a_percentage() {
        let f = MainFacts { five_hour: RateLimit { used_pct: None, resets_at: Some(json!(1)) }, ..facts() };
        assert_eq!(text("rl5h", &f, &GitFacts::default()), None);
    }

    #[test]
    fn cost_renders_zero_rather_than_omitting() {
        let c = config();
        assert_eq!(text("cost", &facts(), &GitFacts::default()).unwrap(), format!("{} $46.51", c.symbol("cost")));
        let out = text("cost", &MainFacts::default(), &GitFacts::default()).unwrap();
        assert_eq!(out, format!("{} $0.00", c.symbol("cost")), "never omits");
    }

    #[test]
    fn duration_omits_only_when_absent() {
        let c = config();
        let out = text("duration", &facts(), &GitFacts::default()).unwrap();
        assert_eq!(out, format!("{} 9hr 19m", c.symbol("duration")));

        let zero = MainFacts { duration_ms: Some(0.0), ..Default::default() };
        assert_eq!(text("duration", &zero, &GitFacts::default()).unwrap(), format!("{} 0s", c.symbol("duration")));

        assert_eq!(text("duration", &MainFacts::default(), &GitFacts::default()), None);
    }

    #[test]
    fn session_omits_without_a_name() {
        assert!(text("session", &facts(), &GitFacts::default()).is_some());
        assert_eq!(text("session", &MainFacts::default(), &GitFacts::default()), None);
    }

    #[test]
    fn project_reads_the_config_not_the_payload() {
        let c = config();
        let out = text("project", &MainFacts::default(), &GitFacts::default()).unwrap();
        assert_eq!(out, format!("{} Project-Name", c.symbol("project")));

        let bare = Config::new(json!({ "symbols": { "project": "P" } }));
        assert_eq!(text_for("project", &MainFacts::default(), &GitFacts::default(), &bare, None), None);
    }

    #[test]
    fn worktree_carries_both_glyphs() {
        let c = config();
        let git = GitFacts { worktree_subpath: Some("main-bar".into()), ..Default::default() };
        let out = text("worktree", &MainFacts::default(), &git).unwrap();
        assert_eq!(out, format!("{} {} main-bar", c.symbol("worktree"), c.symbol("folder")));

        assert_eq!(text("worktree", &MainFacts::default(), &GitFacts::default()), None);
    }

    #[test]
    fn branch_assembles_its_conditional_parts() {
        let c = config();
        let base = GitFacts { branch: Some("main".into()), ..Default::default() };

        assert_eq!(branch(&base, &c).unwrap(), format!("{} main", c.symbol("branch")));

        let ahead = GitFacts { ahead: true, ..base.clone() };
        assert_eq!(branch(&ahead, &c).unwrap(), format!("{} main {}", c.symbol("branch"), c.symbol("ahead")));

        let dirty = GitFacts { additions: 3, deletions: 1, ..base.clone() };
        assert_eq!(branch(&dirty, &c).unwrap(), format!("{} main {}", c.symbol("branch"), c.symbol("dirtyMix")));

        let all = GitFacts { ahead: true, additions: 3, deletions: 1, worktree_subpath: Some("wt".into()), ..base };
        assert_eq!(
            branch(&all, &c).unwrap(),
            format!(
                "{} {} main {} {}",
                c.symbol("worktree"),
                c.symbol("branch"),
                c.symbol("ahead"),
                c.symbol("dirtyMix"),
            ),
        );
    }

    #[test]
    fn branch_picks_the_right_dirty_marker() {
        let c = config();
        let at = |a, d| {
            let g = GitFacts { branch: Some("m".into()), additions: a, deletions: d, ..Default::default() };
            branch(&g, &c).unwrap()
        };
        assert!(at(3, 1).ends_with(c.symbol("dirtyMix")));
        assert!(at(3, 0).ends_with(c.symbol("dirtyAdd")));
        assert!(at(0, 1).ends_with(c.symbol("dirtyDel")));
        assert_eq!(at(0, 0), format!("{} m", c.symbol("branch")), "clean carries no marker");
    }

    #[test]
    fn branch_omits_without_one() {
        assert_eq!(text("branch", &facts(), &GitFacts::default()), None);
    }

    #[test]
    fn spend_draws_what_it_was_given_and_omits_without_it() {
        assert_eq!(text("spend", &facts(), &GitFacts::default()), None, "a gated-off spend omits");
        assert_eq!(
            text_for("spend", &facts(), &GitFacts::default(), &config(), Some("$ 1/2 (50%)")),
            Some("$ 1/2 (50%)".to_string()),
            "and a resolved one is passed through verbatim",
        );
    }

    #[test]
    fn an_unknown_segment_warns_and_omits() {
        let built = build(&json!("nosuchsegment"), &facts(), &GitFacts::default(), &config(), None);
        assert!(built.is_none());
    }

    #[test]
    fn styling_resolves_inline_then_config_then_the_hard_fallback() {
        let c = config();
        let f = facts();
        let g = GitFacts::default();

        // The shipped default for `model` is blue/bold/white.
        let default = build(&json!("model"), &f, &g, &c, None).unwrap();
        assert_eq!(default.bg, [69, 133, 136]);
        assert!(default.bold);

        // Inline wins.
        let inline = build(&json!({ "name": "model", "bg": "red", "bold": false }), &f, &g, &c, None).unwrap();
        assert_eq!(inline.bg, [204, 36, 29]);
        assert!(!inline.bold);

        // An entry may be keyed by `id` instead of `name`.
        assert_eq!(build(&json!({ "id": "model", "bg": "red" }), &f, &g, &c, None).unwrap().bg, [204, 36, 29]);

        // With no `segments.<id>` entry at all, the hard fallback is blue.
        let bare = Config::new(json!({ "palette": { "blue": [69, 133, 136] } }));
        assert_eq!(build(&json!("cost"), &f, &g, &bare, None).unwrap().bg, [69, 133, 136]);
    }

    #[test]
    fn an_explicit_null_falls_through_but_false_does_not() {
        let c = config();
        let (f, g) = (facts(), GitFacts::default());

        // `model` defaults to bold; an inline null must not disable it.
        let nulled = build(&json!({ "name": "model", "bold": null }), &f, &g, &c, None).unwrap();
        assert!(nulled.bold, "a null override falls through to the config default");

        let explicit = build(&json!({ "name": "model", "bold": false }), &f, &g, &c, None).unwrap();
        assert!(!explicit.bold, "false is a value, not an absence");
    }

    #[test]
    fn bold_is_coerced_by_truthiness() {
        let c = config();
        let (f, g) = (facts(), GitFacts::default());
        assert!(build(&json!({ "name": "cost", "bold": 1 }), &f, &g, &c, None).unwrap().bold);
        assert!(!build(&json!({ "name": "cost", "bold": 0 }), &f, &g, &c, None).unwrap().bold);
        assert!(!build(&json!({ "name": "cost", "bold": "" }), &f, &g, &c, None).unwrap().bold);
    }

    #[test]
    fn a_line_drops_every_segment_that_omits() {
        let entries = vec![json!("model"), json!("session"), json!("branch"), json!("spend")];
        let built = build_line(&entries, &MainFacts::default(), &GitFacts::default(), &config(), None);
        assert_eq!(built.len(), 1, "only `model`, which never omits");
    }
}
