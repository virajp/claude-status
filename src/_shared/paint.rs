//! Colour for the human-facing surfaces, applied **after** the filter.
//!
//! # Why after, and why that is not a hole in invariant 4
//!
//! Invariant 4 says only the renderer emits escapes, and [`text::sanitize`]
//! enforces it by stripping every control character — ESC included — from
//! whatever is written. Colour is escapes, so the two look opposed. They are
//! not, because of the **order**: a string is filtered first and painted
//! second, so every escape in the result was put there by this module and
//! nothing a config file, a branch name or an argv token contained can survive
//! into it. The filter stays exactly as strict as it was.
//!
//! `--doctor` already worked this way before there was any colour: its SAMPLE
//! RENDER is appended *after* the sweep, "because it is the one part whose
//! escapes are meant to be there". This module generalises that idiom rather
//! than adding a second rule.
//!
//! # What the colours mean
//!
//! Health, not severity-of-prose: green is a thing that is working, yellow is a
//! thing that is absent-but-fine or that the user wrote and which did nothing,
//! red is a thing actively failing. A reader should be able to scan a report
//! and find the red without reading the words.
//!
//! # What is deliberately never painted
//!
//! `--version`, `--caps-hook` and `--subagent`. All three are **parsed by a
//! machine**: the release workflow and the `build:statusline` smoke test read
//! the version, the caps hook's stdout is injected verbatim into an agent's
//! context, and the panel is NDJSON. A decoration on any of them is a broken
//! build or a corrupted payload rather than a nicer terminal.
//!
//! `--statusline` is not painted here either, for the opposite reason: it is
//! already entirely colour, drawn by the powerline renderer from the user's
//! own palette. Its escapes are the bar.

use std::io::IsTerminal;

/// What a line says about the health of the machine.
///
/// [`Health::Note`] exists because most lines are neither good nor bad —
/// headings, paths, narration. Without it every caller would have to pick a
/// colour for text that has no state to report, and a report where everything
/// is coloured says no more than one where nothing is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    /// No colour. Headings, plain rows, narration.
    Note,
    /// Working: a layer loaded, a key wired, a credential found.
    Ok,
    /// Absent but fine, or written by the user and ignored by the binary.
    /// Nothing is broken; something is worth a second look.
    Warn,
    /// Actively failing: a refusal, a panic, a live backoff, an unreadable file.
    Bad,
}

impl Health {
    /// The SGR parameter, or `None` for the one variant that is not painted.
    const fn sgr(self) -> Option<&'static str> {
        match self {
            Health::Note => None,
            Health::Ok => Some("32"),
            Health::Warn => Some("33"),
            Health::Bad => Some("31"),
        }
    }
}

/// The three-byte reset. Written after every painted span, never on its own.
const RESET: &str = "\u{1b}[0m";

/// Wraps `text` in `health`'s colour, when `enabled`.
///
/// `enabled` is a parameter rather than a call to [`stdout`] or [`stderr`]
/// because those read the process's real file descriptors and environment,
/// which a unit test cannot fake. The decision is taken once at the edge and
/// passed down; everything below is pure and testable.
///
/// **Empty text is never painted.** An empty span still costs the escapes
/// around it, and a line of pure colour codes is what a `grep` for a blank
/// value would then match.
pub fn paint(text: &str, health: Health, enabled: bool) -> String {
    match health.sgr() {
        Some(sgr) if enabled && !text.is_empty() => format!("\u{1b}[{sgr}m{text}{RESET}"),
        _ => text.to_string(),
    }
}

/// Paints the lines of `text` named in `marks`, leaving the rest alone.
///
/// `marks` is `(line index, health)`, built as the text was assembled. It is a
/// **side channel on purpose**: the alternative is embedding the colour during
/// assembly, which puts escapes into the string *before* the filter runs and
/// therefore gets them stripped. Recording where the colour goes and applying
/// it afterwards is what lets one sweep still cover the whole report.
///
/// Indices survive the filter because [`text::sanitize_report`] preserves
/// newlines and adds none, so it cannot renumber a line. `line_count_survives_
/// the_report_filter` pins that, and it is the assumption this whole function
/// rests on.
///
/// A mark past the end is ignored rather than a panic: this runs inside
/// `--doctor`, and invariant 3 says a diagnostic surface does not get to crash
/// over its own decoration.
pub fn lines(text: &str, marks: &[(usize, Health)], enabled: bool) -> String {
    if !enabled || marks.is_empty() {
        return text.to_string();
    }
    let trailing_newline = text.ends_with('\n');
    let mut out: Vec<String> = text.lines().map(str::to_string).collect();
    for (index, health) in marks {
        if let Some(line) = out.get_mut(*index) {
            *line = paint(line, *health, true);
        }
    }
    let mut joined = out.join("\n");
    if trailing_newline {
        joined.push('\n');
    }
    joined
}

/// Text under assembly, plus the health of the lines that have one.
///
/// # Why a side channel rather than colour in the string
///
/// Painting during assembly puts escapes into the text *before*
/// [`text::sanitize_report`] runs over it, so the filter strips them straight
/// back out. Recording *where* the colour goes and applying it afterwards is
/// what lets a single sweep still cover the whole report — which is the
/// property `--doctor` was built around and which per-write filtering lost
/// three times before it was made a chokepoint.
///
/// # Why it implements `fmt::Write`
///
/// So that `writeln!(out, ...)` keeps working unchanged. The report is
/// assembled by five helper functions that each take `&mut String`; making the
/// accumulator a new type that writes like a `String` changes their signature
/// and nothing else, where threading a second `&mut Vec` parameter through all
/// of them would have touched every line.
#[derive(Debug, Default)]
pub struct Marked {
    text: String,
    marks: Vec<(usize, Health)>,
}

impl std::fmt::Write for Marked {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.text.push_str(s);
        Ok(())
    }
}

impl Marked {
    pub fn new() -> Self {
        Self::default()
    }

    /// Tags the line most recently completed — the one the `writeln!` above
    /// this call just wrote.
    ///
    /// [`Health::Note`] is dropped rather than recorded. It paints nothing, so
    /// storing it would only make `marks` longer and the disabled-colour fast
    /// path in [`lines`] harder to hit.
    pub fn mark(&mut self, health: Health) {
        if health == Health::Note {
            return;
        }
        if let Some(index) = self.text.lines().count().checked_sub(1) {
            self.marks.push((index, health));
        }
    }

    /// Appends another accumulator, shifting its marks to their new rows.
    ///
    /// The offset is the line the appended text starts on. Getting this wrong
    /// is silent — the report would still render, with the colour on the wrong
    /// rows — so `appending_shifts_the_marks_to_their_new_rows` pins it.
    pub fn append(&mut self, other: Marked) {
        let offset = if self.text.is_empty() {
            0
        } else {
            // A trailing newline means the next write starts a fresh line;
            // without one it continues the last, which is the same index.
            self.text.lines().count() - usize::from(!self.text.ends_with('\n'))
        };
        self.marks.extend(other.marks.into_iter().map(|(i, h)| (i + offset, h)));
        self.text.push_str(&other.text);
    }

    /// The text so far, unpainted.
    ///
    /// Test-only: the production path goes through [`Marked::into_parts`],
    /// which hands over the marks as well, and a reader that took only the text
    /// would silently drop every colour.
    #[cfg(test)]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The assembled text and its marks, for filtering and then painting.
    pub fn into_parts(self) -> (String, Vec<(usize, Health)>) {
        (self.text, self.marks)
    }
}

impl From<&str> for Marked {
    /// Unmarked text. For a test stub standing in for a section that would
    /// otherwise fetch, and for anything with no health to report.
    fn from(text: &str) -> Self {
        Self { text: text.to_string(), marks: Vec::new() }
    }
}

/// Whether stdout should carry colour.
pub fn stdout() -> bool {
    allowed(std::io::stdout().is_terminal())
}

/// Whether stderr should carry colour. Asked separately from [`stdout`],
/// because the two are redirected independently — `--doctor > report.txt` in a
/// terminal leaves stderr a tty and stdout a file, and each has to answer for
/// itself.
pub fn stderr() -> bool {
    allowed(std::io::stderr().is_terminal())
}

/// A terminal, and no `NO_COLOR`.
///
/// Per <https://no-color.org>, the variable disables colour when it is present
/// **and non-empty**; an empty value is treated as unset, which is how a user
/// clears it in a shell that cannot unset one.
fn allowed(is_terminal: bool) -> bool {
    is_terminal && !std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_disabled_paint_is_the_text_unchanged() {
        assert_eq!(paint("loaded", Health::Ok, false), "loaded");
        assert_eq!(paint("loaded", Health::Bad, false), "loaded");
    }

    #[test]
    fn note_is_never_painted_even_when_enabled() {
        assert_eq!(paint("a path", Health::Note, true), "a path");
    }

    #[test]
    fn each_health_gets_its_own_colour_and_resets() {
        for (health, sgr) in [(Health::Ok, "32"), (Health::Warn, "33"), (Health::Bad, "31")] {
            let out = paint("x", health, true);
            assert_eq!(out, format!("\u{1b}[{sgr}mx\u{1b}[0m"), "{health:?}");
            assert!(out.ends_with(RESET), "{health:?} left the colour on for whatever prints next");
        }
    }

    #[test]
    fn an_empty_span_is_not_painted() {
        // Otherwise a blank value renders as escapes around nothing, which
        // looks like a value to anything reading the output and is invisible
        // to anyone reading the terminal.
        assert_eq!(paint("", Health::Bad, true), "");
    }

    #[test]
    fn only_the_marked_lines_are_painted() {
        let text = "one\ntwo\nthree\n";
        let out = lines(text, &[(1, Health::Bad)], true);
        assert_eq!(out, "one\n\u{1b}[31mtwo\u{1b}[0m\nthree\n");
    }

    #[test]
    fn the_trailing_newline_survives_a_paint() {
        // The report is concatenated onto after this runs — SAMPLE RENDER is
        // appended last — so losing the final newline would weld a section
        // heading onto the end of the spend block.
        assert!(lines("one\ntwo\n", &[(0, Health::Ok)], true).ends_with('\n'));
        assert!(!lines("one\ntwo", &[(0, Health::Ok)], true).ends_with('\n'));
    }

    #[test]
    fn a_mark_past_the_end_is_ignored_rather_than_a_panic() {
        assert_eq!(lines("one\n", &[(9, Health::Bad)], true), "one\n");
    }

    #[test]
    fn disabled_returns_the_text_untouched_however_many_marks() {
        let text = "one\ntwo\n";
        assert_eq!(lines(text, &[(0, Health::Ok), (1, Health::Bad)], false), text);
    }

    /// **The assumption `lines` rests on.** Marks are recorded as line indices
    /// while the report is assembled, and applied after the filter has run over
    /// it. If the filter could add or drop a line, every mark after that point
    /// would paint the wrong row — silently, and in the one surface a user
    /// reads to find out what is wrong with their machine.
    #[test]
    fn line_count_survives_the_report_filter() {
        let hostile = "a\nb\u{1b}[31m\nc\u{7f}\u{202e}\nd\r\ne\u{200b}\n";
        let filtered = crate::render::sanitize_report(hostile);
        assert_eq!(filtered.lines().count(), hostile.lines().count(), "the filter renumbered the report");
        assert!(!filtered.contains('\u{1b}'), "and it is still stripping what it was written to strip");
    }

    #[test]
    fn mark_tags_the_line_just_written() {
        use std::fmt::Write as _;
        let mut m = Marked::new();
        let _ = writeln!(m, "first");
        let _ = writeln!(m, "second");
        m.mark(Health::Bad);
        let (text, marks) = m.into_parts();
        assert_eq!(marks, [(1, Health::Bad)], "the mark landed on the wrong row");
        assert_eq!(lines(&text, &marks, true), "first\n\u{1b}[31msecond\u{1b}[0m\n");
    }

    #[test]
    fn a_note_mark_is_not_recorded() {
        use std::fmt::Write as _;
        let mut m = Marked::new();
        let _ = writeln!(m, "plain");
        m.mark(Health::Note);
        assert!(m.into_parts().1.is_empty(), "Note paints nothing, so storing it only lengthens the list");
    }

    /// Named in [`Marked::append`]'s docs as what holds it up. The failure is
    /// silent — the report still renders, with the colour on the wrong rows —
    /// so it is worth its own case.
    #[test]
    fn appending_shifts_the_marks_to_their_new_rows() {
        use std::fmt::Write as _;
        let mut head = Marked::new();
        let _ = writeln!(head, "one");
        let _ = writeln!(head, "two");

        let mut tail = Marked::new();
        let _ = writeln!(tail, "three");
        tail.mark(Health::Ok);

        head.append(tail);
        let (text, marks) = head.into_parts();
        assert_eq!(text, "one\ntwo\nthree\n");
        assert_eq!(marks, [(2, Health::Ok)], "the appended mark still points at its old row");
    }

    #[test]
    fn appending_onto_an_empty_accumulator_shifts_nothing() {
        let mut head = Marked::new();
        let mut tail = Marked::from("only\n");
        tail.mark(Health::Ok);
        head.append(tail);
        assert_eq!(head.into_parts().1, [(0, Health::Ok)]);
    }

    #[test]
    fn no_color_disables_colour_on_a_terminal_but_an_empty_value_does_not() {
        let mut env = crate::_shared::env_lock();

        env.unset("NO_COLOR");
        assert!(allowed(true), "a terminal with no NO_COLOR is colour");

        env.set("NO_COLOR", "1");
        assert!(!allowed(true), "NO_COLOR is honoured");

        // no-color.org: present **and non-empty**. Empty is how a user clears
        // it in a shell that cannot unset a variable.
        env.set("NO_COLOR", "");
        assert!(allowed(true), "an empty NO_COLOR is not a request for monochrome");
    }

    #[test]
    fn a_pipe_is_never_coloured_whatever_no_color_says() {
        let mut env = crate::_shared::env_lock();
        env.unset("NO_COLOR");
        assert!(!allowed(false), "redirecting to a file must not embed escapes");
    }
}
