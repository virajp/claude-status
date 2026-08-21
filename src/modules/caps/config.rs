//! Per-repo cap overrides, scraped from `<cwd>/.config/vwf.yaml`.
//!
//! A **narrow line scan, not a YAML parse**. That is what keeps this cycle
//! dependency-free: the alternative is a YAML crate on the `PostToolUse` path,
//! for three integers in one block, in a file this binary does not own.

use std::path::Path;

/// The shipped caps. A repo may lower these and may not raise them.
pub const DEFAULTS: Caps = Caps { context: 65, five_hour: 90, seven_day: 80 };

/// The thresholds a breach is measured against, as percentages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    pub context: u32,
    pub five_hour: u32,
    pub seven_day: u32,
}

/// The effective caps for a working directory: `min(shipped, repo)` per key.
///
/// **Tighten-only.** A repo that sets `context: 90` still gets 65. Getting this
/// backwards would let a project silently disable its own safety rail, which is
/// the one failure mode of a config-driven cap that nobody would notice.
pub fn resolve(cwd: Option<&str>) -> Caps {
    let repo = scrape(cwd);
    Caps {
        context: tighten(DEFAULTS.context, repo.context),
        five_hour: tighten(DEFAULTS.five_hour, repo.five_hour),
        seven_day: tighten(DEFAULTS.seven_day, repo.seven_day),
    }
}

fn tighten(shipped: u32, repo: Option<u32>) -> u32 {
    repo.map_or(shipped, |r| shipped.min(r))
}

/// What the file actually declared, before the tighten-only rule.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RepoCaps {
    pub context: Option<u32>,
    pub five_hour: Option<u32>,
    pub seven_day: Option<u32>,
}

/// Reads `<cwd>/.config/vwf.yaml` and pulls the caps block out of it.
///
/// Both block names are honoured: `pipeline.execute_caps` and the legacy
/// `pipeline.autopilot_caps`. Any read failure — no cwd, no file, unreadable,
/// not UTF-8 — yields no overrides rather than an error.
pub fn scrape(cwd: Option<&str>) -> RepoCaps {
    let Some(cwd) = cwd.filter(|c| !c.is_empty()) else {
        return RepoCaps::default();
    };
    let path = Path::new(cwd).join(".config").join("vwf.yaml");
    let Ok(text) = std::fs::read_to_string(path) else {
        return RepoCaps::default();
    };
    parse(&text)
}

/// The scan itself, split out so it can be tested without a filesystem.
pub fn parse(text: &str) -> RepoCaps {
    let lines: Vec<&str> = text.split('\n').collect();
    let Some(start) = lines.iter().position(|l| is_block_header(l)) else {
        return RepoCaps::default();
    };
    let indent = indent_of(lines[start]);

    let mut caps = RepoCaps::default();
    for line in &lines[start + 1..] {
        // Blanks and comments never end the block — they can sit at any
        // indent, including column zero, without meaning the block is over.
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        // The first line at or left of the header's indent ends the block.
        if indent_of(line) <= indent {
            break;
        }
        if let Some((key, value)) = cap_entry(line) {
            match key {
                "context" => caps.context = Some(value),
                "five_hour" => caps.five_hour = Some(value),
                "seven_day" => caps.seven_day = Some(value),
                _ => {}
            }
        }
    }
    caps
}

/// `^\s*(execute_caps|autopilot_caps):\s*(#.*)?$` — the header and nothing
/// else on the line, a trailing comment aside. A header with a value on it is
/// not a block.
fn is_block_header(line: &str) -> bool {
    let head = line.trim_start();
    let Some(rest) = head.strip_prefix("execute_caps:").or_else(|| head.strip_prefix("autopilot_caps:")) else {
        return false;
    };
    let rest = rest.trim_start();
    rest.is_empty() || rest.starts_with('#')
}

/// `^\s*(context|five_hour|seven_day):\s*(\d+)`, with the digits taken greedily
/// and whatever follows them ignored — a trailing comment, a unit, anything.
fn cap_entry(line: &str) -> Option<(&str, u32)> {
    let rest = line.trim_start();
    let (key, rest) = rest.split_once(':')?;
    if !matches!(key, "context" | "five_hour" | "seven_day") {
        return None;
    }
    let digits: String = rest.trim_start().chars().take_while(char::is_ascii_digit).collect();
    // A non-integer value — `65.5`, `"65"`, `null` — matches nothing and falls
    // back to the default, exactly as the reference scan does.
    digits.parse().ok().map(|v| (key, v))
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: &str = "\
pipeline:
  execute_caps:
    context: 50
    five_hour: 85
    seven_day: 70
  review_round_cap: 4
";

    #[test]
    fn both_block_names_are_honoured() {
        assert_eq!(parse(BLOCK).context, Some(50));
        assert_eq!(parse(&BLOCK.replace("execute_caps", "autopilot_caps")).five_hour, Some(85));
    }

    #[test]
    fn a_repo_may_tighten_a_cap_but_never_loosen_one() {
        let tighter = resolve_with("  execute_caps:\n    context: 50\n");
        assert_eq!(tighter.context, 50, "below the shipped 65, so it wins");

        let looser = resolve_with("  execute_caps:\n    context: 90\n");
        assert_eq!(looser.context, 65, "above the shipped 65, so it is ignored");
    }

    /// `resolve` without touching a filesystem.
    fn resolve_with(text: &str) -> Caps {
        let repo = parse(text);
        Caps {
            context: tighten(DEFAULTS.context, repo.context),
            five_hour: tighten(DEFAULTS.five_hour, repo.five_hour),
            seven_day: tighten(DEFAULTS.seven_day, repo.seven_day),
        }
    }

    #[test]
    fn the_whole_block_is_read_not_just_the_first_key() {
        assert_eq!(parse(BLOCK), RepoCaps { context: Some(50), five_hour: Some(85), seven_day: Some(70) });
    }

    #[test]
    fn the_block_ends_at_the_first_line_back_at_its_own_indent() {
        // `review_round_cap` sits at the header's indent, so `context` below it
        // is a different block's key and must not be picked up.
        let text = "\
pipeline:
  execute_caps:
    seven_day: 70
  other:
    context: 10
";
        assert_eq!(parse(text), RepoCaps { seven_day: Some(70), ..Default::default() });
    }

    #[test]
    fn a_key_at_the_wrong_indent_is_not_picked_up() {
        let text = "  execute_caps:\n  context: 50\n";
        assert_eq!(parse(text), RepoCaps::default(), "same indent as the header ends the block");
    }

    #[test]
    fn blanks_and_comments_inside_the_block_do_not_end_it() {
        let text = "\
  execute_caps:
    context: 50

# a comment at column zero
    seven_day: 70
";
        assert_eq!(parse(text), RepoCaps { context: Some(50), seven_day: Some(70), ..Default::default() });
    }

    #[test]
    fn a_commented_out_key_is_not_read() {
        let text = "  execute_caps:\n    # context: 50\n    seven_day: 70\n";
        assert_eq!(parse(text).context, None);
    }

    #[test]
    fn a_non_integer_value_falls_back_to_the_default() {
        for value in ["\"50\"", "null", "", "abc", "-5"] {
            let text = format!("  execute_caps:\n    context: {value}\n");
            assert_eq!(parse(&text).context, None, "value {value:?}");
        }
    }

    #[test]
    fn a_decimal_value_is_truncated_at_the_point_rather_than_rejected() {
        // The reference pattern is `\s*(\d+)` with nothing anchoring the end,
        // so `65.5` matches `65` and `Number("65")` is 65. Faithful, and it
        // only matters for a config nobody would write on purpose.
        assert_eq!(parse("  execute_caps:\n    context: 65.5\n").context, Some(65));
    }

    #[test]
    fn a_header_carrying_a_value_is_not_a_block() {
        assert_eq!(parse("  execute_caps: true\n    context: 50\n"), RepoCaps::default());
    }

    #[test]
    fn a_trailing_comment_on_the_header_or_a_value_is_tolerated() {
        let text = "  execute_caps: # the caps\n    context: 50 # tightened\n";
        assert_eq!(parse(text).context, Some(50));
    }

    #[test]
    fn a_missing_file_or_block_yields_the_shipped_defaults() {
        assert_eq!(scrape(None), RepoCaps::default());
        assert_eq!(scrape(Some("")), RepoCaps::default());
        assert_eq!(scrape(Some("/nonexistent/path/for/a/test")), RepoCaps::default());
        assert_eq!(parse("pipeline:\n  review_round_cap: 4\n"), RepoCaps::default());
        assert_eq!(resolve(None), DEFAULTS);
    }
}
