//! Golden renders: facts in, an exact ANSI string out.
//!
//! This is what catches a separator regression or an off-by-one in the gauge,
//! and nothing else will. The goldens are **generated**, never hand-written —
//! they contain Nerd Font private-use codepoints that do not survive being
//! retyped. Regenerate with `UPDATE_GOLDEN=1 cargo test --test golden`, then
//! check the rendered bar by eye in a terminal, not the diff.
//!
//! The clock is pinned, because the reference fixture's `resets_at` values are
//! already in the past: without an injected clock every reset half would render
//! `now` and these goldens would rot into meaninglessness.

use std::path::PathBuf;

use claude_status::config::{Config, layers};
use claude_status::git::GitFacts;
use claude_status::payload::{MainFacts, RateLimit};
use claude_status::render::subagent::render_panel;
use claude_status::render_bar;
use serde_json::json;

/// 4h36m before the fixture's five-hour reset.
const PINNED_NOW: i64 = 1_774_183_440_000;

fn config() -> Config {
    layers::load(None, None).config
}

/// The shipped defaults with the layout replaced.
fn with_layout(lines: serde_json::Value) -> serde_json::Value {
    let mut root: serde_json::Value = serde_json::from_str(claude_status::config::defaults::DEFAULTS_JSON).unwrap();
    claude_status::json::deep_merge(&mut root, &json!({ "lines": lines }));
    root
}

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("golden").join(format!("{name}.txt"))
}

/// Compares a render against its golden, or rewrites it under `UPDATE_GOLDEN=1`.
fn assert_golden(name: &str, actual: &str) {
    let path = golden_path(name);

    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }

    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing golden {}: {e}\nrun UPDATE_GOLDEN=1 cargo test --test golden", path.display()));

    // Escaped on both sides, so a failure shows `\u{1b}[48;2;69;133;136m`
    // rather than invisible bytes.
    assert_eq!(
        actual,
        expected,
        "\n golden {name} drifted\n  actual: {}\nexpected: {}\n",
        actual.escape_debug(),
        expected.escape_debug(),
    );
}

/// The reference fixture, as facts. The payload it corresponds to is
/// `tests/fixtures/main-bar.json`; this is the parsed form, because a golden
/// pins the render and not the parse.
fn fixture_facts() -> MainFacts {
    MainFacts {
        now_ms: PINNED_NOW,
        model: Some("Opus 4.8".into()),
        effort: Some("high".into()),
        session_name: Some("users-and-groups".into()),
        cwd: Some("/tmp/demo".into()),
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
fn the_reference_fixture_renders() {
    let git = GitFacts { branch: Some("main".into()), ..Default::default() };
    assert_golden("fixture", &render_bar(&fixture_facts(), &git, &config(), None));
}

#[test]
fn a_cold_start_with_no_payload_still_draws_a_full_bar() {
    // Deviation from the JS, which rendered blank without a config file: the
    // embedded defaults layer means a cold machine gets a real bar.
    let facts = MainFacts { now_ms: PINNED_NOW, ..Default::default() };
    assert_golden("cold_start", &render_bar(&facts, &GitFacts::default(), &config(), None));
}

#[test]
fn a_worktree_with_every_git_marker_renders() {
    let git = GitFacts {
        branch: Some("worktree-main-bar".into()),
        ahead: true,
        additions: 12,
        deletions: 3,
        worktree_subpath: Some("main-bar".into()),
        ..Default::default()
    };
    assert_golden("worktree", &render_bar(&fixture_facts(), &git, &config(), None));
}

#[test]
fn adjacent_same_background_segments_take_the_thin_seam() {
    // `model` and `rl5h` are both blue, but the shipped layout never puts them
    // side by side, so the default bar never exercises this branch.
    let config = Config::new(with_layout(json!([["model", "rl5h", "context"]])));
    assert_golden("thin_seam", &render_bar(&fixture_facts(), &GitFacts::default(), &config, None));
}

#[test]
fn the_spend_segment_renders_when_every_gate_passes() {
    // The four goldens above all pass `None`, which is spend gated off. This
    // is the other half: the text arrives pre-resolved and draws like any
    // other segment.
    let config = Config::new(with_layout(json!([["model", "spend", "cost"]])));
    let spend = "\u{f155} $75.93/$150 (51%)";
    assert_golden("spend", &render_bar(&fixture_facts(), &GitFacts::default(), &config, Some(spend)));
}

/// The reference subagent payload — `tests/fixtures/subagent.json` — with the panel-wide model
/// and effort the schema documents.
fn subagent_fixture() -> serde_json::Value {
    json!({
        "columns": 120,
        "model": { "display_name": "Opus 4.8" },
        "effort": { "level": "high" },
        "tasks": [{
            "id": "t1",
            "name": "reviewer",
            "type": "review",
            "status": "running",
            "description": "Auditing auth flow",
            "tokenCount": 18234,
        }],
    })
}

#[test]
fn the_subagent_fixture_renders_one_ndjson_row() {
    assert_golden("subagent", &render_panel(&subagent_fixture(), &config(), PINNED_NOW, None));
}

#[test]
fn a_subagent_row_with_every_optional_segment_omitted_is_the_head_alone() {
    // The other end of the range the row builder covers: no name, no model, no
    // description, no tokens, no start time.
    let payload = json!({ "tasks": [{ "id": 1, "status": "queued", "type": "local_agent" }] });
    assert_golden("subagent_bare", &render_panel(&payload, &config(), PINNED_NOW, None));
}

#[test]
fn a_subagent_panel_renders_one_row_per_task() {
    let payload = json!({ "columns": 120, "tasks": [
        { "id": "a", "name": "builder", "type": "task", "status": "done", "tokenCount": 1_200_000 },
        { "id": "b", "name": "tester", "type": "test", "status": "error", "startTime": PINNED_NOW - 3_725_000i64 },
    ] });
    assert_golden("subagent_multi", &render_panel(&payload, &config(), PINNED_NOW, None));
}

#[test]
fn every_golden_is_free_of_stray_control_characters() {
    // A golden should hold ANSI SGR sequences and text, and nothing else — no
    // stray carriage returns, no NULs from a bad regeneration.
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        return; // the goldens are being written by sibling tests right now
    }
    // **Read the directory, do not list the names.** An earlier version spelled
    // out seven of them and `spend.txt` was not among the seven, so the one
    // golden added after the list was written was never checked. A hand-kept
    // list of files that already exist on disk is a list that goes stale in one
    // direction only — silently, and towards less coverage.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("golden");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("the golden directory exists")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".txt"))
        .map(|n| n.trim_end_matches(".txt").to_owned())
        .collect();
    names.sort();

    // A scan of nothing passes. This repo has shipped that twice.
    assert!(names.len() >= 8, "found {} goldens — the scan below would be near-vacuous: {names:?}", names.len());

    for name in &names {
        let body = std::fs::read_to_string(golden_path(name)).expect("golden exists");
        for ch in body.chars() {
            let ok = ch == '\u{1b}' || ch == '\n' || !ch.is_control();
            assert!(ok, "golden {name} holds a stray control character {:?}", ch.escape_debug().to_string());
        }
        assert!(!body.ends_with('\n'), "golden {name} has a trailing newline the renderer does not emit");
    }
}
