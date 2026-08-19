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
    pub fn from_config(config: &Config) -> Self {
        Self {
            cap: config.powerline("cap").to_string(),
            sep: config.powerline("sep").to_string(),
            sep_thin: config.powerline("sepThin").to_string(),
            thin_fg: config.color(config.get("powerline.thinFg")),
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
