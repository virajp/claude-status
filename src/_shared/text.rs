//! Filtering the characters that let a value repaint a terminal instead of
//! appearing in it.
//!
//! A pure `&str -> String` filter with no knowledge of any domain, which is why
//! it lives here: the stderr chokepoint in this module's parent needs it, and
//! `_shared/` is declared to have no domain knowledge. It was briefly reached
//! as `crate::render::sanitize` from there, which made the layering breach
//! central rather than fixing it.

/// Strips the characters that let a value repaint the terminal instead of
/// appearing in it.
///
/// **Everything on the bar is attacker-nameable.** A branch, a directory under
/// a worktree, a session name, a model string from the payload, a task
/// description written by a model, and — the widest of them —
/// `<repo-root>/.config/claude-status.json`, which is read from whatever
/// repository you happen to `cd` into. Cloning a hostile repo is enough; there
/// is no further interaction.
///
/// The renderer emits its own SGR escapes and powerline separators around and
/// between segments, so a value carrying escapes of its own is not merely
/// ugly — it can close them early, repaint the TUI above the bar to forge
/// output, or emit OSC 52 to put text on the clipboard.
///
/// What goes, and why each is not paranoia:
///
/// - **`Cc`** — C0, DEL **and C1** (`U+0080`–`U+009F`); Rust's `is_control` is
///   the Unicode `Cc` category, which spans all three. `\x1b` starts every
///   escape sequence, `\n` and `\r` break or overwrite a row that is supposed
///   to be one line, and a terminal in 8-bit mode reads `U+009B` as CSI
///   directly with no `\x1b` in front of it.
/// - **Bidi overrides and isolates** (`U+202A`–`U+202E`, `U+2066`–`U+2069`) —
///   reorder rendered text without changing it, so `main` can be made to read
///   as something else.
/// - **Zero-width space and BOM** (`U+200B`, `U+FEFF`) — invisible, and enough
///   to make two different branches render identically.
///
/// What stays, deliberately: **ZWJ** (`U+200D`) and **variation selectors**,
/// without which compound emoji fall apart, and the **private-use** codepoints
/// the Nerd Font glyphs live in — the entire bar is built from those, so
/// filtering them would erase it. A dynamic value containing a separator glyph
/// can therefore still *look* like a separator. That is a display-spoofing
/// residual, accepted: the row's colours are already themeable by the same
/// config layer by design, and the line this function draws is between
/// *theming* the bar and *escaping* out of it.
pub fn sanitize(text: &str) -> String {
    text.chars().filter(|c| safe(*c)).collect()
}

/// `sanitize`, but a newline survives.
///
/// For `--debug`, which is a **terminal write like any other** and so obeys the
/// same rule — but is deliberately many lines, so the row version would collapse
/// the whole report onto one.
///
/// It exists because `--debug` is the fourth surface [§4a] names, and it earned
/// its own entry point rather than a scatter of per-write calls: the report
/// prints the config `lines` entries, the spend gate table, the `settings.json`
/// command, the endpoint URL and the plan tag, and filtering those one at a
/// time is how two of them were missed twice. Sanitize the assembled report
/// once, and anything added to it later is covered without being remembered.
///
/// [§4a]: ../../../docs/spec/statusline-behaviour.md
pub fn sanitize_report(text: &str) -> String {
    text.chars().filter(|c| *c == '\n' || safe(*c)).collect()
}

fn safe(c: char) -> bool {
    // `is_control` already covers C1 — there is deliberately no second range
    // check for it here.
    !c.is_control()
        && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        && !matches!(c, '\u{200b}' | '\u{feff}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_strips_what_could_repaint_the_terminal() {
        assert_eq!(sanitize("\u{1b}[31mred"), "[31mred", "ESC cannot start an SGR");
        assert_eq!(sanitize("a\nb"), "ab", "a newline cannot split the row");
        assert_eq!(sanitize("a\rb"), "ab", "a carriage return cannot overwrite it");
        assert_eq!(sanitize("a\tb"), "ab");
        assert_eq!(sanitize("a\u{7f}b"), "ab", "DEL");
        assert_eq!(sanitize("a\u{9b}31mb"), "a31mb", "C1 CSI, honoured in 8-bit mode");
        assert_eq!(sanitize("a\u{85}b"), "ab", "NEL");
        // OSC 52 is the clipboard one: without the ESC it is inert text.
        assert!(!sanitize("\u{1b}]52;c;cGF5bG9hZA==\u{7}").contains('\u{1b}'));
    }

    #[test]
    fn it_strips_the_invisible_reorderings() {
        assert_eq!(sanitize("main\u{202e}dlrow"), "maindlrow", "RTL override");
        assert_eq!(sanitize("a\u{2066}b\u{2069}c"), "abc", "bidi isolates");
        assert_eq!(sanitize("ma\u{200b}in"), "main", "zero-width space");
        assert_eq!(sanitize("\u{feff}main"), "main", "BOM");
    }

    #[test]
    fn it_leaves_the_bar_intact() {
        // The separators, the glyphs, and a compound emoji that needs its ZWJ.
        assert_eq!(sanitize("\u{e0b0}\u{e0b1}"), "\u{e0b0}\u{e0b1}", "powerline private-use glyphs");
        assert_eq!(sanitize("\u{23f1}\u{fe0f}"), "\u{23f1}\u{fe0f}", "variation selector");
        assert_eq!(sanitize("\u{1f468}\u{200d}\u{1f4bb}"), "\u{1f468}\u{200d}\u{1f4bb}", "ZWJ sequence");
        assert_eq!(sanitize("main ↑ ± é 1M (26%)"), "main ↑ ± é 1M (26%)");
    }
}
