//! The powerline row. Pure: `&[Segment] -> String`.
//!
//! The SGR emission *order* differs between the two seam branches and is
//! load-bearing — a same-background seam writes its background before its
//! foreground, a differing one writes foreground first. Reordering either
//! produces the same colours on most terminals and the wrong ones on some.
//!
//! `RESET` follows every fragment, so bold never leaks and a mid-line
//! truncation by Claude Code cannot bleed colour into the TUI.

use crate::config::Config;
use crate::config::color::{Rgb, bg, fg};

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";

/// One rendered segment, with its styling already resolved to RGB.
///
/// Backgrounds compare by resolved RGB, so `"blue"` and `"#458588"` are the
/// same background and take the thin seam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub bg: Rgb,
    pub fg: Rgb,
    pub bold: bool,
}

/// The row's own glyphs, read once from config.
pub struct Powerline {
    pub cap: String,
    pub sep: String,
    pub sep_thin: String,
    pub thin_fg: Rgb,
}

impl Powerline {
    /// The three separators are **sanitized here, once**, rather than at each
    /// of the ~10 places a row emits one.
    ///
    /// They come from `powerline.cap` / `sep` / `sepThin` in the config, and
    /// they are written **outside** any segment's SGR bracket — so an escape
    /// here is not even contained by the colour codes around it. That is why
    /// they are filtered at all, and it has not changed.
    ///
    /// **What has changed is the reach.** This used to say they were the widest
    /// attacker-controlled surface on the bar, because the repo-level layer
    /// could set them and cloning a hostile repo was the whole attack. Since
    /// the `config-relocation` cycle a repo layer may set `projectName` and
    /// nothing else, so a cloned repository cannot reach these keys at all —
    /// pinned by `a_cloned_repo_can_no_longer_reach_the_separators_at_all`
    /// below. They now come from the user layer, which is a file the user
    /// wrote or synced from a dotfiles repo.
    ///
    /// The filter stays exactly as it was. A narrower input is still an input,
    /// and the position outside the SGR bracket is what makes these the
    /// sharpest place to get it wrong.
    ///
    /// `thin_fg` needs no such treatment: it goes through `config.color`, which
    /// yields an `Rgb`, and a colour cannot carry an escape.
    pub fn from_config(config: &Config) -> Self {
        Self {
            cap: super::sanitize(&config.powerline.cap),
            sep: super::sanitize(&config.powerline.sep),
            sep_thin: super::sanitize(&config.powerline.sep_thin),
            thin_fg: config.color(config.powerline.thin_fg.as_ref()),
        }
    }
}

/// Renders one row. An empty row renders `""` — the caller drops it rather than
/// printing a blank line.
pub fn render(segments: &[Segment], pl: &Powerline) -> String {
    let Some(first) = segments.first() else {
        return String::new();
    };

    let mut out = String::with_capacity(segments.len() * 64);

    // Cap: the first segment's background as a *foreground*, and no background,
    // so it dissolves into the terminal.
    out.push_str(&fg(first.bg));
    out.push_str(&pl.cap);
    out.push_str(RESET);

    for (i, seg) in segments.iter().enumerate() {
        out.push_str(&bg(seg.bg));
        out.push_str(&fg(seg.fg));
        if seg.bold {
            out.push_str(BOLD);
        }
        out.push(' ');
        out.push_str(&seg.text);
        out.push(' ');
        out.push_str(RESET);

        if let Some(next) = segments.get(i + 1) {
            if next.bg == seg.bg {
                // Same background: background first, then the thin foreground.
                out.push_str(&bg(next.bg));
                out.push_str(&fg(pl.thin_fg));
                out.push_str(&pl.sep_thin);
            } else {
                // Different backgrounds: foreground first, then the background.
                out.push_str(&fg(seg.bg));
                out.push_str(&bg(next.bg));
                out.push_str(&pl.sep);
            }
            out.push_str(RESET);
        }
    }

    // Closing separator, unconditional and with no background.
    let last = segments.last().expect("non-empty, checked above");
    out.push_str(&fg(last.bg));
    out.push_str(&pl.sep);
    out.push_str(RESET);

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLUE: Rgb = [69, 133, 136];
    const AQUA: Rgb = [104, 157, 106];
    const WHITE: Rgb = [251, 241, 199];
    const GREY: Rgb = [60, 56, 54];

    fn pl() -> Powerline {
        Powerline { cap: "C".into(), sep: "S".into(), sep_thin: "T".into(), thin_fg: GREY }
    }

    fn seg(text: &str, background: Rgb) -> Segment {
        Segment { text: text.into(), bg: background, fg: WHITE, bold: false }
    }

    #[test]
    fn an_empty_row_renders_nothing() {
        assert_eq!(render(&[], &pl()), "");
    }

    /// The separators land **outside** any segment's SGR bracket, so an escape
    /// in one is not merely ugly: it can close the renderer's own sequences
    /// early, repaint the TUI above the bar, or emit OSC 52 to the clipboard.
    ///
    /// **This used to be planted in the repo layer, and the premise has
    /// narrowed.** Cloning a hostile repo was the whole attack because a repo
    /// config could set anything; since `config-relocation` it may set
    /// `projectName` and nothing else, so `powerline` is unreachable from a
    /// cloned repository — see the sibling test below, which pins that.
    ///
    /// The payload therefore moves to the **user** layer, which still carries
    /// these keys and is still not trustworthy input: a `~/.config` directory
    /// is the thing people commit to a dotfiles repo and sync between machines,
    /// which is a supply chain of exactly one hop.
    #[test]
    fn a_hostile_user_config_cannot_put_escapes_in_the_separators() {
        let home = tempfile::TempDir::new().unwrap();
        let path = crate::config::layers::user_config_path(home.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            serde_json::json!({
                "powerline": {
                    // OSC 52 writes to the clipboard; CSI repaints the TUI above.
                    "cap": "\u{1b}]52;c;cGF5bG9hZA==\u{7}",
                    "sep": "\u{1b}[2J\u{1b}[H",
                    "sepThin": "\u{9b}31m",
                },
            })
            .to_string(),
        )
        .unwrap();

        let layers = crate::config::layers::load(Some(home.path()), None);
        // Not vacuous: the layer has to have been read for the assertions below
        // to mean anything. A fixture at the wrong path loads as "not found",
        // hands back the shipped glyphs, and passes having tested nothing.
        assert!(layers.sources.iter().any(|s| s.label == crate::config::layers::LABEL_USER && s.loaded));

        let pl = Powerline::from_config(&layers.config);
        assert!(!pl.cap.contains('\u{1b}'), "cap: {:?}", pl.cap);
        assert!(!pl.sep.contains('\u{1b}'), "sep: {:?}", pl.sep);
        assert!(!pl.sep_thin.contains('\u{9b}'), "sepThin: {:?}", pl.sep_thin);

        // And nothing leaks through the rendered row either.
        let row = render(&[seg("x", BLUE), seg("y", AQUA)], &pl);
        assert!(!row.contains("\u{1b}]"), "no OSC in {row:?}");
        assert!(!row.contains("\u{1b}[2J"), "no erase-display in {row:?}");
    }

    /// What the test above gave up, asserted rather than assumed.
    ///
    /// The narrowing removed a whole attack surface, and a removed surface is
    /// worth a test of its own: if the repo layer ever widens again, this goes
    /// red and the sibling above stops being the only place the separators are
    /// checked against a cloned repository.
    #[test]
    fn a_cloned_repo_can_no_longer_reach_the_separators_at_all() {
        let repo = tempfile::TempDir::new().unwrap();
        let path = crate::config::layers::repo_config_path(repo.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            serde_json::json!({ "powerline": { "cap": "\u{1b}]52;c;cGF5bG9hZA==\u{7}" } }).to_string(),
        )
        .unwrap();

        let layers = crate::config::layers::load(None, Some(repo.path()));
        assert_eq!(
            layers.sources.iter().find(|s| s.label == crate::config::layers::LABEL_REPO).unwrap().ignored,
            ["powerline"],
            "the repo layer reached `powerline` again",
        );

        let pl = Powerline::from_config(&layers.config);
        assert_eq!(pl.cap, "\u{e0b6}", "the shipped cap, not the repo's");
    }

    #[test]
    fn a_one_segment_row_still_gets_a_closing_separator() {
        let out = render(&[Segment { bold: true, ..seg("a", BLUE) }], &pl());
        assert_eq!(
            out,
            concat!(
                "\x1b[38;2;69;133;136mC\x1b[0m",
                "\x1b[48;2;69;133;136m\x1b[38;2;251;241;199m\x1b[1m a \x1b[0m",
                "\x1b[38;2;69;133;136mS\x1b[0m",
            ),
            "got: {}",
            out.escape_debug(),
        );
    }

    #[test]
    fn two_same_background_segments_take_the_thin_seam() {
        let out = render(&[seg("a", BLUE), seg("b", BLUE)], &pl());
        assert_eq!(
            out,
            concat!(
                "\x1b[38;2;69;133;136mC\x1b[0m",
                "\x1b[48;2;69;133;136m\x1b[38;2;251;241;199m a \x1b[0m",
                // Background before foreground on this branch.
                "\x1b[48;2;69;133;136m\x1b[38;2;60;56;54mT\x1b[0m",
                "\x1b[48;2;69;133;136m\x1b[38;2;251;241;199m b \x1b[0m",
                "\x1b[38;2;69;133;136mS\x1b[0m",
            ),
            "got: {}",
            out.escape_debug(),
        );
    }

    #[test]
    fn two_differing_background_segments_take_the_thick_seam() {
        let out = render(&[seg("a", BLUE), seg("b", AQUA)], &pl());
        assert_eq!(
            out,
            concat!(
                "\x1b[38;2;69;133;136mC\x1b[0m",
                "\x1b[48;2;69;133;136m\x1b[38;2;251;241;199m a \x1b[0m",
                // Foreground before background on this branch.
                "\x1b[38;2;69;133;136m\x1b[48;2;104;157;106mS\x1b[0m",
                "\x1b[48;2;104;157;106m\x1b[38;2;251;241;199m b \x1b[0m",
                "\x1b[38;2;104;157;106mS\x1b[0m",
            ),
            "got: {}",
            out.escape_debug(),
        );
    }

    #[test]
    fn backgrounds_compare_by_resolved_rgb() {
        // "blue" and "#458588" resolve to the same triple, so the seam between
        // them must be the thin one.
        let out = render(&[seg("a", BLUE), seg("b", [69, 133, 136])], &pl());
        assert!(out.contains('T'), "expected the thin seam, got: {}", out.escape_debug());
        assert!(!out.contains("mS\x1b[0m\x1b[48"), "no thick seam mid-row");
    }

    #[test]
    fn every_fragment_is_reset_so_bold_never_leaks() {
        let out = render(&[Segment { bold: true, ..seg("a", BLUE) }, seg("b", AQUA)], &pl());
        assert_eq!(out.matches(BOLD).count(), 1, "bold is set once and reset before the next segment");
        assert!(out.ends_with(RESET));
    }
}
