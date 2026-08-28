//! The argument surface. A hand-rolled scan, deliberately not `clap`.
//!
//! A dependency that writes to stdout when it dislikes an argument is a
//! liability against "stdout is the bar", and `--version` comes from
//! `CARGO_PKG_VERSION` so there is no second version constant to drift.

use std::ffi::OsString;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The flag [`crate::_shared::proc::spawn_detached`] re-invokes this binary
/// with, named once so the caller and the parser cannot drift apart.
///
/// A literal in both places is a **silent** failure: the child's stdio is
/// `/dev/null`, so a child that parsed no surface flag would write its
/// missing-flag line into nothing and exit 0, and the spend segment would just
/// stop updating with no error on any stream. Unrecognised flags are ignored by
/// design (see [`parse`]), so nothing downstream catches it either.
pub const REFRESH_FLAG: &str = "--refresh";

pub const HELP: &str = r#"claude-status — the Claude Code powerline status line

USAGE:
    claude-status --configure    wire Claude Code to this binary
    claude-status --doctor       report configuration, wiring and a sample render
    claude-status --version      print the version and exit
    claude-status --help         print this help

MODIFIERS:
    --doctor    (earlier flag was --debug) also usable on any surface: it
                narrates to stderr and never changes a byte of stdout.
    --dry-run   with --configure: print every change and write nothing.

    An unrecognised argument is named on stderr, with this help after it.
    Every surface but --configure then carries on; --configure refuses.

MORE:
    https://claude-status.virajp.dev
    Configuring the bar, the per-repo layer, every segment, and the
    surfaces Claude Code invokes for you.
"#;

/// The one-line answer when Claude Code invoked us with no surface flag.
///
/// One line, because it has to fit in a status bar. It names the fix, because
/// the alternative — a blank bar — gives the user nothing to go on.
pub const MISSING_FLAG: &str = "claude-status: missing --statusline or --subagent (run --help)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Version,
    Help,
    Statusline,
    Subagent,
    Refresh,
    /// `--caps-hook`: the vwf `PostToolUse` actuator. Writes nothing at all in
    /// the common case — silence is the normal outcome for this surface.
    CapsHook,
    /// `--configure`: wire Claude Code to this binary. The **only** mode that
    /// writes under `$HOME` outside the spend cache, and the only one that can
    /// exit non-zero.
    Configure,
    /// `--doctor` on its own: the diagnostic report is the output.
    Doctor,
    /// No surface flag and stdin is piped — a stale `settings.json`.
    MissingFlag,
}

/// **No longer `Copy`**, because [`Cli::unknown`] has to name the arguments it
/// did not recognise and a `Vec<String>` cannot be. The alternative — a bare
/// `has_unknown: bool` — keeps `Copy` and makes the resulting error useless: a
/// user who typed `--dry-runn` needs to be shown `--dry-runn`, not told that
/// something somewhere was wrong.
#[derive(Debug, Clone)]
pub struct Cli {
    pub mode: Mode,
    /// `--doctor` was passed. On a render mode this is a modifier, not the mode.
    pub doctor: bool,
    /// `--dry-run` was passed. Only [`Mode::Configure`] reads it — it is the
    /// one mode with anything to decline to do.
    pub dry_run: bool,
    /// Arguments [`parse`] did not recognise, in the order they were given.
    ///
    /// **Every mode names these on stderr; only [`Mode::Configure`] treats them
    /// as an error.** No other mode may: Claude Code invokes the render
    /// surfaces, and a
    /// stray argument there costing the user their status bar would break the invariants'
    /// invariant 3 over a typo. Naming is not costing — see
    /// `app::report_unknown`, which writes to stderr alone and leaves both
    /// stdout and the exit code exactly as they were. `--configure` is the one mode that *writes*,
    /// with no receipt and no undo, so the same silence there means
    /// `--configure --dry-runn` overwrites a `settings.json` the user believed
    /// they were only previewing. The asymmetry is the point.
    pub unknown: Vec<String>,
}

/// Parses the argument vector.
///
/// `--version` is checked **first**, before anything else can print, and prints
/// nothing but the bare number. The reason used to be that the npm installer
/// told an installed binary from a bundled one by the *shape* of that answer;
/// that installer is gone. The guarantee stays because it now has **two live
/// consumers that fail a release over it**: the release workflow refuses to
/// publish a binary whose `--version` differs from the crate version, and the
/// `build:statusline` smoke test asserts the same thing locally. It is the one
/// output of this binary a script may parse, so a decoration is a broken build.
pub fn parse<I: IntoIterator<Item = OsString>>(args: I, stdin_is_tty: bool) -> Cli {
    let mut doctor = false;
    let mut dry_run = false;
    let mut surface = None;
    let mut help = false;
    let mut version = false;
    let mut unknown = Vec::new();

    for arg in args.into_iter().skip(1) {
        match arg.to_string_lossy().as_ref() {
            "--version" | "-V" => version = true,
            "--help" | "-h" => help = true,
            "--doctor" => doctor = true,
            "--dry-run" => dry_run = true,
            "--statusline" => surface = surface.or(Some(Mode::Statusline)),
            "--subagent" => surface = surface.or(Some(Mode::Subagent)),
            REFRESH_FLAG => surface = surface.or(Some(Mode::Refresh)),
            "--caps-hook" => surface = surface.or(Some(Mode::CapsHook)),
            "--configure" => surface = surface.or(Some(Mode::Configure)),
            // Anything unrecognised is non-fatal here rather than silent; with
            // no surface flag the no-flag case below still explains itself.
            //
            // **Collected for two callers now.** `app::report_unknown` names
            // every one of them on stderr, whatever the mode — the rename made
            // that necessary, since `--debug` is now exactly this arm and a
            // user who kept it deserves to be told rather than left with
            // narration quietly off. Not costing a bar is right for a render —
            // Claude Code invokes those, a stray argument
            // must never cost a bar, and the worst case is a bar drawn without
            // a modifier. It is the opposite of right for the one flag that
            // *writes*: `--configure --dry-runn` would silently perform a real,
            // unundoable overwrite of the user's `settings.json` while the user
            // believed they had asked for a preview. See [`Cli::unknown`].
            other => unknown.push(other.to_string()),
        }
    }

    let mode = if version {
        Mode::Version
    } else if help {
        Mode::Help
    } else if let Some(surface) = surface {
        surface
    } else if doctor {
        Mode::Doctor
    } else if stdin_is_tty {
        // Someone typed the command. Show them how to use it.
        Mode::Help
    } else {
        // Claude Code invoked us. One line that fits the bar.
        Mode::MissingFlag
    };

    Cli { mode, doctor, dry_run, unknown }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(args: &[&str], tty: bool) -> Cli {
        parse(std::iter::once("claude-status").chain(args.iter().copied()).map(OsString::from), tty)
    }

    #[test]
    fn version_wins_over_everything() {
        for args in [&["--version"][..], &["--version", "--doctor"], &["--statusline", "--version"], &["--version", "--help"]]
        {
            assert_eq!(parse_args(args, false).mode, Mode::Version, "{args:?}");
        }
    }

    #[test]
    fn help_wins_over_a_surface_flag() {
        assert_eq!(parse_args(&["--help"], false).mode, Mode::Help);
        assert_eq!(parse_args(&["--statusline", "--help"], false).mode, Mode::Help);
        assert_eq!(parse_args(&["-h"], false).mode, Mode::Help);
    }

    #[test]
    fn surface_flags_select_their_mode() {
        assert_eq!(parse_args(&["--statusline"], false).mode, Mode::Statusline);
        assert_eq!(parse_args(&["--subagent"], false).mode, Mode::Subagent);
        assert_eq!(parse_args(&["--refresh"], false).mode, Mode::Refresh);
        assert_eq!(parse_args(&["--configure"], false).mode, Mode::Configure);
    }

    /// The rename's silent failure mode, pinned.
    ///
    /// `resolve_spend` re-invokes this binary with [`REFRESH_FLAG`] and the
    /// child's stdio is `/dev/null`. If the literal the caller passes and the
    /// arm the parser matches ever drift apart, the child parses no surface
    /// flag, falls to [`Mode::MissingFlag`] on a non-TTY stdin, writes its one
    /// line into a null stdout and exits 0 — so **the spend segment simply
    /// stops updating**, with nothing on any stream to say why. Unrecognised
    /// flags are ignored by design, so nothing else catches it either.
    #[test]
    fn the_flag_the_refresh_child_is_spawned_with_parses_back_to_that_mode() {
        assert_eq!(parse_args(&[REFRESH_FLAG], false).mode, Mode::Refresh);
    }

    #[test]
    fn doctor_is_a_modifier_on_a_surface_flag_and_a_mode_alone() {
        let modifier = parse_args(&["--statusline", "--doctor"], false);
        assert_eq!(modifier.mode, Mode::Statusline);
        assert!(modifier.doctor);

        let mode = parse_args(&["--doctor"], false);
        assert_eq!(mode.mode, Mode::Doctor);
        assert!(mode.doctor);
    }

    /// The rename from the parser's side: `--debug` is not a second spelling of
    /// `--doctor`, it is an argument this binary does not know.
    ///
    /// Both of its old jobs are checked, because they failed differently. As a
    /// **mode** it selected the report; as a **modifier** it turned narration
    /// on. A rename that missed the second would leave `--statusline --debug`
    /// drawing a bar with the diagnostics silently off — which looks exactly
    /// like a working install.
    #[test]
    fn the_old_debug_spelling_is_no_longer_recognised() {
        let alone = parse_args(&["--debug"], false);
        assert!(!alone.doctor, "--debug must not still select the report");
        assert_eq!(alone.mode, Mode::MissingFlag, "it selects no mode of its own");
        assert_eq!(alone.unknown, ["--debug"], "and it is collected, so `app::report_unknown` can name it");

        let modifier = parse_args(&["--statusline", "--debug"], false);
        assert_eq!(modifier.mode, Mode::Statusline, "invariant 3: a stray token still may not cost a bar");
        assert!(!modifier.doctor, "--debug must not still turn narration on");
        assert_eq!(modifier.unknown, ["--debug"]);
    }

    /// `--help` is the only documentation that ships **inside** the binary, so
    /// it is the only place a user whose `settings.json` still says `--debug`
    /// can find out what happened to it. The old name has to appear here, and
    /// this is the one test that wants it to.
    #[test]
    fn help_records_that_doctor_was_previously_called_debug() {
        assert!(HELP.contains("--debug"), "the old name is not written down anywhere in the binary");
        // One parenthetical beside the new name, not a section. The reader who
        // needs this is scanning for `--doctor` and recognising the old word
        // next to it; anyone wanting the reasoning has the website.
        assert!(HELP.contains("--doctor    (earlier flag was --debug)"), "the note is not beside the flag it explains");
    }

    #[test]
    fn dry_run_is_a_modifier_and_never_a_mode_of_its_own() {
        let configure = parse_args(&["--configure", "--dry-run"], false);
        assert_eq!(configure.mode, Mode::Configure);
        assert!(configure.dry_run);

        // On its own it selects nothing — the no-flag rules still decide.
        assert_eq!(parse_args(&["--dry-run"], false).mode, Mode::MissingFlag);
        assert!(!parse_args(&["--configure"], false).dry_run);
    }

    /// **This test is criterion 7 inverted, and the inversion is the point.**
    ///
    /// Criterion 7 said `--help` was the only documentation shipping in the
    /// binary, so "a vague `--help` is the feature being gone in practice" —
    /// and the test that held it asserted a **floor**: five sections, forty
    /// lines, the repo-config path, `projectName`, `refreshInterval`, the
    /// `settings.json` shapes. That premise expired when the website shipped.
    /// Help is now the index and the site is the documentation, so the same
    /// concern needs the **opposite** assertion: a ceiling.
    ///
    /// It is inverted rather than deleted for the reason the repo-config pair
    /// was. Dropping it would leave the one surface a user meets first with no
    /// guard at all, and length is precisely what regresses here — every
    /// future flag will want three explaining lines.
    #[test]
    fn help_stays_short_and_sends_the_reader_to_the_website() {
        assert!(
            HELP.lines().count() < 30,
            "HELP grew back to {} lines — details belong on the website",
            HELP.lines().count(),
        );

        // The flags a person types. Structure still matters: a keyword soup
        // under one heading would satisfy the ceiling and help nobody.
        for section in ["USAGE:", "MODIFIERS:", "MORE:"] {
            assert!(HELP.contains(section), "the {section} section is gone");
        }
        for flag in ["--configure", "--doctor", "--version", "--help", "--dry-run"] {
            assert!(HELP.contains(flag), "{flag} is undocumented");
        }

        // **The surfaces Claude Code invokes are deliberately absent.** They
        // are wired by `--configure` and never typed, so listing them spends
        // three lines of the first thing a user reads on three flags they will
        // never use. `MISSING_FLAG` still names the two of them that matter to
        // someone whose `settings.json` went stale.
        // `--refresh` sits with them for the same reason but a different
        // caller: `resolve_spend` spawns THIS binary with it, detached, when
        // the cache goes stale. Typing it is the identical call — same
        // `bypass_dedupe: false`, so a no-op on a fresh cache — and it prints
        // nothing, because it was built for a child whose stdio is /dev/null.
        // `--doctor` is the one that forces a fetch and shows the answer.
        for wired in ["--statusline", "--subagent", "--caps-hook", "--refresh"] {
            assert!(!HELP.contains(wired), "{wired} is back in HELP — the tool calls it, the user does not");
        }
        assert!(MISSING_FLAG.contains("--statusline"), "and the line that DOES need to name one no longer does");

        // Everything cut has to have somewhere to have gone.
        assert!(HELP.contains("https://claude-status.virajp.dev"), "the website URL");
    }

    /// **Deleted, not weakened.** `the_help_examples_schema_url_is_the_one_the_writer_emits`
    /// guarded a hand-written `$schema` example inside `HELP` against drifting
    /// from `config::write::SCHEMA_URL`. `HELP` carries no example now — the
    /// website does — so there is no second copy left to drift, and a test
    /// asserting that would be asserting nothing.
    ///
    /// The URL itself is still pinned where it is still duplicated: the
    /// writer emits it, and `site/` is checked against the committed schema by
    /// `tests/site.rs`.
    ///
    /// This note is the deletion's receipt. Without it the next reader finds a
    /// constant with no test and adds one back.
    #[allow(dead_code)]
    const SCHEMA_EXAMPLE_LEFT_HELP: () = ();

    #[test]
    fn the_no_flag_case_turns_on_whether_stdin_is_a_tty() {
        // Someone typed it.
        assert_eq!(parse_args(&[], true).mode, Mode::Help);
        // Claude Code invoked it with a stale settings.json.
        assert_eq!(parse_args(&[], false).mode, Mode::MissingFlag);
    }

    #[test]
    fn an_unrecognised_flag_does_not_derail_a_surface_flag() {
        assert_eq!(parse_args(&["--statusline", "--nonsense"], false).mode, Mode::Statusline);
        assert_eq!(parse_args(&["--nonsense"], false).mode, Mode::MissingFlag);
    }

    /// The parser **collects** what it does not recognise without acting on it.
    /// Only `--configure` treats the list as fatal; see [`Cli::unknown`]. The
    /// motivating case is one mistyped character in `--dry-runn` turning a
    /// preview into an unundoable overwrite of the user's `settings.json`.
    #[test]
    fn unrecognised_arguments_are_collected_in_order_and_named() {
        let cli = parse_args(&["--configure", "--dry-runn", "-n"], false);
        assert_eq!(cli.mode, Mode::Configure, "the mode still resolves — the caller decides what to do");
        assert!(!cli.dry_run, "and `--dry-runn` did NOT turn the dry run on");
        assert_eq!(cli.unknown, ["--dry-runn", "-n"]);

        // Every recognised token, in every combination, leaves it empty.
        for args in [&["--statusline", "--doctor"][..], &["--configure", "--dry-run"], &["--version"], &["-h"]] {
            assert!(parse_args(args, false).unknown.is_empty(), "{args:?}");
        }
    }

    #[test]
    fn the_first_surface_flag_wins() {
        assert_eq!(parse_args(&["--statusline", "--subagent"], false).mode, Mode::Statusline);
        assert_eq!(parse_args(&["--subagent", "--statusline"], false).mode, Mode::Subagent);
    }

    #[test]
    fn the_version_is_the_crate_version() {
        // The shape, not a literal. `VERSION` *is* `CARGO_PKG_VERSION`, so
        // asserting a number here only pins a copy that has to be edited on
        // every bump — and the release workflow and the `build:statusline`
        // smoke test both key on this output's shape rather than on any
        // particular value.
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
        let parts: Vec<&str> = VERSION.split('.').collect();
        assert_eq!(parts.len(), 3, "the version must be bare semver: {VERSION}");
        assert!(
            parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())),
            "the version must carry no prefix or suffix: {VERSION}"
        );
    }
}
