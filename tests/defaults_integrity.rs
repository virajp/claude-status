//! Guards `assets/claude-status.defaults.json`, which cannot be verified by
//! reading a diff: 28 of its symbols are Nerd Font private-use codepoints that
//! render as nothing or as a box, and are silently dropped by copy-paste and by
//! any tool that transcribes them.
//!
//! The *test* is safe to retype — it names every codepoint as a `\u{…}` escape,
//! from the table in contract §3. The *asset* is not. Never edit the asset
//! through an editor buffer, and never let a formatter touch it.

use std::collections::BTreeSet;

use claude_status::config::defaults::DEFAULTS_JSON;
use serde_json::Value;

fn defaults() -> Value {
    serde_json::from_str(DEFAULTS_JSON).expect("defaults asset is valid JSON")
}

/// Walks a dotted path of object keys.
fn at<'a>(v: &'a Value, path: &str) -> &'a Value {
    let mut cur = v;
    for key in path.split('.') {
        cur = cur.get(key).unwrap_or_else(|| panic!("defaults asset is missing `{path}` (at `{key}`)"));
    }
    cur
}

fn str_at<'a>(v: &'a Value, path: &str) -> &'a str {
    at(v, path).as_str().unwrap_or_else(|| panic!("`{path}` is not a string"))
}

/// Every glyph in the asset, keyed by its config path, with the value written
/// as escapes so this file stays legible in an editor that has no Nerd Font.
const GLYPHS: &[(&str, &str)] = &[
    ("gauge.empty", "\u{25b1}"),
    ("gauge.filled", "\u{25b0}"),
    ("powerline.cap", "\u{e0b6}"),
    ("powerline.sep", "\u{e0b0}"),
    ("powerline.sepThin", "\u{e0b1}"),
    ("powerline.thinFg", "\u{67}\u{72}\u{65}\u{79}"),
    ("subagent.statuses.done.symbol", "\u{f00c}"),
    ("subagent.statuses.error.symbol", "\u{f00d}"),
    ("subagent.statuses.pending.symbol", "\u{f017}"),
    ("subagent.statuses.running.symbol", "\u{f04b}"),
    ("symbols.agent", "\u{f007}"),
    ("symbols.ahead", "\u{2191}"),
    ("symbols.branch", "\u{e0a0}"),
    ("symbols.context", "\u{f1c0}"),
    ("symbols.cost", "\u{23f1}\u{fe0f}"),
    ("symbols.dirtyAdd", "\u{2b}"),
    ("symbols.dirtyDel", "\u{2d}"),
    ("symbols.dirtyMix", "\u{b1}"),
    ("symbols.duration", "\u{f017}"),
    ("symbols.folder", "\u{f07b}"),
    ("symbols.model", "\u{26a1}"),
    ("symbols.project", "\u{f401}"),
    ("symbols.repo", "\u{f401}"),
    ("symbols.reset", "\u{21bb}"),
    ("symbols.session", "\u{f02b}"),
    ("symbols.spend", "\u{f09d}"),
    ("symbols.tokens", "\u{f51e}"),
    ("symbols.win5h", "\u{f252}"),
    ("symbols.win7d", "\u{f073}"),
    ("symbols.worktree", "\u{1f332}"),
    ("typeSymbols._default", "\u{f1b2}"),
    ("typeSymbols.background", "\u{f013}"),
    ("typeSymbols.cloud_agent", "\u{f0c2}"),
    ("typeSymbols.local_agent", "\u{f109}"),
    ("typeSymbols.mcp", "\u{f1e6}"),
    ("typeSymbols.remote_agent", "\u{f0c2}"),
    ("typeSymbols.review", "\u{f06e}"),
    ("typeSymbols.task", "\u{f0ae}"),
    ("typeSymbols.test", "\u{f0c3}"),
];

/// The Gruvbox palette, as RGB triples.
const PALETTE: &[(&str, [u64; 3])] = &[
    ("aqua", [104, 157, 106]),
    ("bg3", [102, 92, 84]),
    ("blue", [69, 133, 136]),
    ("green", [152, 151, 26]),
    ("grey", [60, 56, 54]),
    ("orange", [214, 93, 14]),
    ("purple", [177, 98, 134]),
    ("red", [204, 36, 29]),
    ("white", [251, 241, 199]),
    ("yellow", [215, 153, 33]),
];

#[test]
fn every_glyph_matches_its_codepoint() {
    let d = defaults();
    for (path, expected) in GLYPHS {
        let actual = str_at(&d, path);
        assert_eq!(
            actual,
            *expected,
            "`{path}` drifted: asset has {:?}, contract §3 says {:?}",
            actual.escape_debug().to_string(),
            expected.escape_debug().to_string(),
        );
    }
    assert_eq!(GLYPHS.len(), 39, "the contract §3 codepoint table has 39 rows");
}

#[test]
fn the_asset_carries_no_symbol_the_table_does_not_cover() {
    let d = defaults();
    let covered = |path: &str| GLYPHS.iter().any(|(p, _)| *p == path);
    for group in ["symbols", "typeSymbols"] {
        let obj = at(&d, group).as_object().unwrap_or_else(|| panic!("`{group}` is not an object"));
        for key in obj.keys() {
            let path = format!("{group}.{key}");
            assert!(covered(&path), "`{path}` is in the asset but not in the contract §3 table");
        }
    }
}

#[test]
fn the_palette_is_the_gruvbox_ten() {
    let d = defaults();
    let palette = at(&d, "palette").as_object().expect("`palette` is an object");
    assert_eq!(palette.len(), 10, "the palette has exactly ten entries");
    for (name, rgb) in PALETTE {
        let actual = at(&d, &format!("palette.{name}"));
        let actual: Vec<u64> = actual
            .as_array()
            .unwrap_or_else(|| panic!("`palette.{name}` is not an array"))
            .iter()
            .map(|n| n.as_u64().unwrap_or_else(|| panic!("`palette.{name}` holds a non-integer")))
            .collect();
        assert_eq!(actual, rgb.to_vec(), "`palette.{name}` drifted");
    }
}

#[test]
fn the_non_glyph_scalars_hold() {
    let d = defaults();
    assert_eq!(at(&d, "gauge.width").as_u64(), Some(10));
    assert_eq!(at(&d, "subagent.descBudgetFraction").as_f64(), Some(0.45));
    assert_eq!(str_at(&d, "defaultFg"), "white");
    assert_eq!(str_at(&d, "worktreePattern"), "worktree");
    assert_eq!(at(&d, "spend.refreshMinutes").as_u64(), Some(15));
    assert_eq!(str_at(&d, "spend.show"), "auto");
}

/// The one key the defaults must **not** carry.
///
/// `projectName` is repo-level only — it is the only key
/// `<repo-root>/.config/claude-status.json` may set. Shipping it embedded is
/// what made every repo without its own config render the same placeholder
/// name.
#[test]
fn the_defaults_carry_no_project_name() {
    assert_eq!(defaults().as_object().unwrap().get("projectName"), None);
}

#[test]
fn the_default_layout_is_two_lines() {
    let d = defaults();
    let expected: Value = serde_json::json!([
        ["model", "context", "rl5h", "rl7d", "spend", "cost"],
        ["project", "worktree", "branch"],
    ]);
    assert_eq!(at(&d, "lines"), &expected);
}

#[test]
fn subagent_statuses_stay_in_config_order() {
    // `serde_json`'s `preserve_order` feature is what makes this true, and
    // plan 2's first-match-wins status ladder depends on it.
    //
    // Note the asset's order is not the precedence order contract §3 prints:
    // `pending` sits third and carries the *empty* match, which is the
    // designated fallback rather than a pattern that matches everything. A
    // reader that walked the ladder naively would resolve every unknown status
    // to `pending` before ever reaching `running`.
    let d = defaults();
    let statuses = at(&d, "subagent.statuses").as_object().expect("`subagent.statuses` is an object");
    let order: Vec<&str> = statuses.keys().map(String::as_str).collect();
    assert_eq!(order, ["done", "error", "pending", "running"]);
    assert_eq!(str_at(&d, "subagent.statuses.pending.match"), "", "`pending` is the empty-match fallback");

}

#[test]
fn the_schema_url_points_at_this_repo_not_the_one_it_came_from() {
    // The asset **is** the seeded config, so this URL ships to every install.
    // It pointed at `virajp/ai-plugins` until the schema was republished here,
    // and a revert would be invisible in any render.
    let defaults: Value = serde_json::from_str(DEFAULTS_JSON).expect("the asset parses");
    let url = defaults["$schema"].as_str().expect("the asset declares a $schema");
    assert_eq!(
        url,
        "https://raw.githubusercontent.com/virajp/claude-status/main/schemas/claude-status.schema.json",
    );
}

#[test]
fn the_shipped_schema_describes_exactly_the_keys_the_asset_carries() {
    // A schema that has drifted from the file it describes is worse than none:
    // an editor reports a valid key as an error, or misses a typo.
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("schemas").join("claude-status.schema.json"),
        )
        .expect("the schema ships in this repo"),
    )
    .expect("the schema parses");

    let described: BTreeSet<&str> = schema["properties"].as_object().unwrap().keys().map(String::as_str).collect();
    let defaults: Value = serde_json::from_str(DEFAULTS_JSON).unwrap();
    let shipped: BTreeSet<&str> = defaults.as_object().unwrap().keys().map(String::as_str).collect();

    // The asset may not carry a key the schema does not describe — that is the
    // direction that makes an editor report a valid key as an error.
    let undescribed: BTreeSet<&str> = shipped.difference(&described).copied().collect();
    assert!(undescribed.is_empty(), "the shipped defaults carry keys the schema does not describe: {undescribed:?}");

    // The other direction is **not** equality, because one key is deliberately
    // schema-only: the schema validates the repo config too, and `projectName`
    // lives there. Named rather than tolerated, so a second omission still
    // fails this test.
    let schema_only: BTreeSet<&str> = described.difference(&shipped).copied().collect();
    let expected: BTreeSet<&str> = ["projectName"].into_iter().collect();
    assert_eq!(schema_only, expected, "the schema and the shipped defaults have drifted apart");
}
