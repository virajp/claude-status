//! Assembling the main bar from the configured layout.

use crate::config::Config;
use crate::git::GitFacts;
use crate::payload::MainFacts;
use crate::render::powerline::{Powerline, render};
use crate::render::segments::build_line;

/// Renders the whole bar.
///
/// A line whose segments all omit is **dropped**, not printed blank, and the
/// survivors join with `\n`. There is **no trailing newline** — Claude Code
/// renders what arrives, and a trailing newline would cost a row.
/// `spend` is the already-resolved spend text, or `None` when any of the four
/// gates hid it. It arrives pre-resolved rather than being read here because
/// gate 1 must be answered **before** the cache is opened — a user without the
/// segment pays nothing for it.
pub fn render_main(facts: &MainFacts, git: &GitFacts, config: &Config, spend: Option<&str>) -> String {
    let powerline = Powerline::from_config(config);

    config
        .lines
        .iter()
        .filter_map(|entries| {
            let segments = build_line(entries, facts, git, config, spend);
            (!segments.is_empty()).then(|| render(&segments, &powerline))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::config::layers;

    fn config() -> Config {
        layers::load(None, None).config
    }

    #[test]
    fn the_default_layout_renders_two_lines() {
        // The second line survives on `project`, which the shipped config sets.
        let git = GitFacts { branch: Some("main".into()), ..Default::default() };
        let out = render_main(&MainFacts::default(), &git, &config(), None);
        assert_eq!(out.lines().count(), 2, "got: {}", out.escape_debug());
        assert!(!out.ends_with('\n'), "no trailing newline");
    }

    #[test]
    fn a_line_whose_segments_all_omit_is_dropped_not_blank() {
        // `session` and `branch` both omit with empty facts, so line two goes.
        let config = Config::new(json!({
            "palette": { "blue": [69, 133, 136], "white": [251, 241, 199] },
            "powerline": { "cap": "C", "sep": "S", "sepThin": "T", "thinFg": "white" },
            "lines": [["cost"], ["session"], ["branch"]],
        }));
        let out = render_main(&MainFacts::default(), &GitFacts::default(), &config, None);
        assert_eq!(out.lines().count(), 1, "got: {}", out.escape_debug());
        assert!(!out.contains("\n\n"), "no blank line where a row was dropped");
    }

    #[test]
    fn a_layout_with_no_usable_lines_renders_nothing() {
        let config = Config::new(json!({ "lines": [["session"]] }));
        assert_eq!(render_main(&MainFacts::default(), &GitFacts::default(), &config, None), "");
    }

    #[test]
    fn an_empty_layout_renders_nothing_rather_than_failing() {
        // Written out as `[]`, not left absent: an absent `lines` is the
        // shipped layout now, which is the whole point of the typed defaults.
        let config = Config::new(json!({ "lines": [] }));
        assert_eq!(render_main(&MainFacts::default(), &GitFacts::default(), &config, None), "");
    }

    #[test]
    fn an_unknown_segment_does_not_stop_the_line() {
        let config = Config::new(json!({
            "palette": { "blue": [69, 133, 136], "white": [251, 241, 199] },
            "powerline": { "cap": "C", "sep": "S", "sepThin": "T", "thinFg": "white" },
            "symbols": { "cost": "$" },
            "lines": [["nosuchsegment", "cost"]],
        }));
        let out = render_main(&MainFacts::default(), &GitFacts::default(), &config, None);
        assert!(out.contains("$ $0.00"), "the sibling still rendered: {}", out.escape_debug());
    }
}
