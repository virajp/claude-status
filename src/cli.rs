//! The argument surface. A hand-rolled scan, deliberately not `clap`.
//!
//! A dependency that writes to stdout when it dislikes an argument is a
//! liability against "stdout is the bar", and `--version` comes from
//! `CARGO_PKG_VERSION` so there is no second version constant to drift.

use std::ffi::OsString;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const HELP: &str = "\
claude-status — the Claude Code powerline status line

USAGE:
    claude-status --statusline      render the main bar from a payload on stdin
    claude-status --subagent        render the subagent panel from stdin (NDJSON)
    claude-status --refresh-spend   refresh the spend cache and exit
    claude-status --debug           report configuration, wiring and a sample render
    claude-status --version         print the version and exit
    claude-status --help            print this help

FLAGS:
    --debug     also usable as a modifier on any of the above. It narrates to
                stderr and never changes a byte of stdout.

WIRING:
    Claude Code invokes this binary through two keys in ~/.claude/settings.json.
    Both must carry a surface flag:

      \"statusLine\":         { \"type\": \"command\", \"command\": \"…/claude-status --statusline\" }
      \"subagentStatusLine\": { \"type\": \"command\", \"command\": \"…/claude-status --subagent\" }

    Run --debug to see what is currently wired.
";

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
    RefreshSpend,
    /// `--debug` on its own: the diagnostic report is the output.
    Debug,
    /// No surface flag and stdin is piped — a stale `settings.json`.
    MissingFlag,
}

#[derive(Debug, Clone, Copy)]
pub struct Cli {
    pub mode: Mode,
    /// `--debug` was passed. On a render mode this is a modifier, not the mode.
    pub debug: bool,
}

/// Parses the argument vector.
///
/// `--version` is checked **first**, before anything else can print: the
/// installer tells an installed binary from a bundled one by the *shape* of
/// that answer, so it must never be decorated.
pub fn parse<I: IntoIterator<Item = OsString>>(args: I, stdin_is_tty: bool) -> Cli {
    let mut debug = false;
    let mut surface = None;
    let mut help = false;
    let mut version = false;

    for arg in args.into_iter().skip(1) {
        match arg.to_string_lossy().as_ref() {
            "--version" | "-V" => version = true,
            "--help" | "-h" => help = true,
            "--debug" => debug = true,
            "--statusline" => surface = surface.or(Some(Mode::Statusline)),
            "--subagent" => surface = surface.or(Some(Mode::Subagent)),
            "--refresh-spend" => surface = surface.or(Some(Mode::RefreshSpend)),
            // Anything unrecognised is ignored rather than fatal; with no
            // surface flag the no-flag case below still explains itself.
            _ => {}
        }
    }

    let mode = if version {
        Mode::Version
    } else if help {
        Mode::Help
    } else if let Some(surface) = surface {
        surface
    } else if debug {
        Mode::Debug
    } else if stdin_is_tty {
        // Someone typed the command. Show them how to use it.
        Mode::Help
    } else {
        // Claude Code invoked us. One line that fits the bar.
        Mode::MissingFlag
    };

    Cli { mode, debug }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(args: &[&str], tty: bool) -> Cli {
        parse(std::iter::once("claude-status").chain(args.iter().copied()).map(OsString::from), tty)
    }

    #[test]
    fn version_wins_over_everything() {
        for args in [&["--version"][..], &["--version", "--debug"], &["--statusline", "--version"], &["--version", "--help"]]
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
        assert_eq!(parse_args(&["--refresh-spend"], false).mode, Mode::RefreshSpend);
    }

    #[test]
    fn debug_is_a_modifier_on_a_surface_flag_and_a_mode_alone() {
        let modifier = parse_args(&["--statusline", "--debug"], false);
        assert_eq!(modifier.mode, Mode::Statusline);
        assert!(modifier.debug);

        let mode = parse_args(&["--debug"], false);
        assert_eq!(mode.mode, Mode::Debug);
        assert!(mode.debug);
    }

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

    #[test]
    fn the_first_surface_flag_wins() {
        assert_eq!(parse_args(&["--statusline", "--subagent"], false).mode, Mode::Statusline);
        assert_eq!(parse_args(&["--subagent", "--statusline"], false).mode, Mode::Subagent);
    }

    #[test]
    fn the_version_is_the_crate_version() {
        assert_eq!(VERSION, "6.0.0");
    }
}
