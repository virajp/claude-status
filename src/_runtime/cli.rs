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
    claude-status --statusline   render the main bar from a payload on stdin
    claude-status --subagent     render the subagent panel from stdin (NDJSON)
    claude-status --configure    wire Claude Code to this binary
    claude-status --refresh      refresh the spend cache and exit
    claude-status --caps-hook    vwf PostToolUse cap actuator; silent unless breached
    claude-status --debug        report configuration, wiring and a sample render
    claude-status --version      print the version and exit
    claude-status --help         print this help

MODIFIERS:
    --debug     also usable on any of the above. It narrates to stderr and
                never changes a byte of stdout.
    --dry-run   with --configure: print every change and write nothing.

    Every surface but --configure ignores an argument it does not recognise, so
    a stray token can never cost you a status bar. --configure refuses instead:
    it writes, it cannot be undone, and a typo in --dry-run must not turn a
    preview into a real overwrite.

WHAT --configure WRITES:
    Three keys in ~/.claude/settings.json, each invoking `claude-status` by
    name from your PATH:

      "statusLine":         { "type": "command", "command": "claude-status --statusline",
                              "padding": 0, "refreshInterval": 4 }
      "subagentStatusLine": { "type": "command", "command": "claude-status --subagent" }
      "hooks": { "PostToolUse": [ { "hooks": [ { "type": "command",
                              "command": "claude-status --caps-hook" } ] } ] }

    Every other key in that file is left as it was, and another tool's
    PostToolUse hooks are kept alongside ours. A statusLine belonging to
    someone else IS replaced — --configure prints what it replaced, and there
    is no undo, so set yours again to get it back.

    It also creates ~/.config/claude-status/config.json when you have none,
    holding a "$schema" pointer and nothing else. An existing one is never
    touched.

    Run --debug to see what is currently wired.

CONFIGURATION:
    ~/.config/claude-status/config.json      your settings — only the keys that
                                             differ from the shipped defaults
    <repo-root>/.config/claude-status.json   the per-repo layer

    Nothing has to exist. With no config file anywhere the bar renders from the
    defaults compiled into this binary, and that is a supported state.

    THE PER-REPO LAYER sets exactly one key: "projectName", the name the
    `project` segment draws for that repository. You rarely need it: with no
    name set anywhere the segment already draws the git root's directory name,
    and this key only calls it something else. Every other key in it is
    ignored, and --debug names the ones it dropped. Write it by hand:

      {
        "$schema": "https://raw.githubusercontent.com/virajp/claude-status/main/schemas/claude-status.schema.json",
        "projectName": "my-repo"
      }

MORE:
    https://claude-status.virajp.dev
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
    /// `--debug` on its own: the diagnostic report is the output.
    Debug,
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
    /// `--debug` was passed. On a render mode this is a modifier, not the mode.
    pub debug: bool,
    /// `--dry-run` was passed. Only [`Mode::Configure`] reads it — it is the
    /// one mode with anything to decline to do.
    pub dry_run: bool,
    /// Arguments [`parse`] did not recognise, in the order they were given.
    ///
    /// **Only [`Mode::Configure`] treats these as an error.** Every other mode
    /// ignores them, and must: Claude Code invokes the render surfaces, and a
    /// stray argument there costing the user their status bar would break §1's
    /// invariant 3 over a typo. `--configure` is the one mode that *writes*,
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
    let mut debug = false;
    let mut dry_run = false;
    let mut surface = None;
    let mut help = false;
    let mut version = false;
    let mut unknown = Vec::new();

    for arg in args.into_iter().skip(1) {
        match arg.to_string_lossy().as_ref() {
            "--version" | "-V" => version = true,
            "--help" | "-h" => help = true,
            "--debug" => debug = true,
            "--dry-run" => dry_run = true,
            "--statusline" => surface = surface.or(Some(Mode::Statusline)),
            "--subagent" => surface = surface.or(Some(Mode::Subagent)),
            REFRESH_FLAG => surface = surface.or(Some(Mode::Refresh)),
            "--caps-hook" => surface = surface.or(Some(Mode::CapsHook)),
            "--configure" => surface = surface.or(Some(Mode::Configure)),
            // Anything unrecognised is ignored rather than fatal; with no
            // surface flag the no-flag case below still explains itself.
            //
            // **Collected anyway, for `--configure` alone.** Ignoring a typo is
            // right for a render — Claude Code invokes those, a stray argument
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
    } else if debug {
        Mode::Debug
    } else if stdin_is_tty {
        // Someone typed the command. Show them how to use it.
        Mode::Help
    } else {
        // Claude Code invoked us. One line that fits the bar.
        Mode::MissingFlag
    };

    Cli { mode, debug, dry_run, unknown }
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
    fn debug_is_a_modifier_on_a_surface_flag_and_a_mode_alone() {
        let modifier = parse_args(&["--statusline", "--debug"], false);
        assert_eq!(modifier.mode, Mode::Statusline);
        assert!(modifier.debug);

        let mode = parse_args(&["--debug"], false);
        assert_eq!(mode.mode, Mode::Debug);
        assert!(mode.debug);
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

    /// **Criterion 7.** `--help` is the only documentation that ships in the
    /// binary now that the npm installer is gone, and after `config-relocation`
    /// deleted the autoseed the repo layer has **no other discovery route** —
    /// a vague `--help` is the feature being gone in practice.
    #[test]
    fn help_documents_the_repo_layer_the_website_and_what_configure_writes() {
        // **Structure first, because the substrings below do not imply it.**
        // A nine-line blob carrying nothing but the asserted keywords satisfies
        // every `contains` in this test, and satisfies nothing a user needs —
        // which is the whole point of a criterion whose own reasoning is that
        // "a vague `--help` is the feature being gone in practice". The section
        // headers and the length are what make it documentation rather than a
        // keyword soup.
        for section in ["USAGE:", "MODIFIERS:", "WHAT --configure WRITES:", "CONFIGURATION:", "MORE:"] {
            assert!(HELP.contains(section), "the {section} section is gone");
        }
        assert!(HELP.lines().count() > 40, "HELP collapsed to {} lines", HELP.lines().count());

        assert!(HELP.contains(".config/claude-status.json"), "the repo config path");
        assert!(HELP.contains("projectName"), "the one key it may set");
        assert!(HELP.contains("Every other key in it is\n    ignored"), "and that the rest are not");
        // **This URL does not resolve yet.** `website/01` ships the site, and
        // the plan index already records that it should land before
        // `distribution/02` so the formula's caveats do not print a dead link.
        // Naming it here is the recorded decision, not a guess — dropping the
        // clause because the site is not up would leave the repo layer with no
        // documented home at all.
        assert!(HELP.contains("https://claude-status.virajp.dev"), "the website URL");

        // The paths and the shapes `--configure` writes, so a reader can check
        // their own file against this without running anything.
        assert!(HELP.contains("~/.claude/settings.json"));
        assert!(HELP.contains("~/.config/claude-status/config.json"));
        for flag in ["--statusline", "--subagent", "--caps-hook", "--refresh", "--configure", "--dry-run"] {
            assert!(HELP.contains(flag), "{flag} is undocumented");
        }
        assert!(HELP.contains("refreshInterval"), "the bar's refresh cadence is part of the shape");

        // Two comments in `app.rs` rest on this sentence; `--debug` is where a
        // user is sent to see what is actually wired.
        assert!(HELP.contains("Run --debug to see what is currently wired."));
    }

    /// The example in `--help` has to be an example of the real thing.
    #[test]
    fn the_help_examples_schema_url_is_the_one_the_writer_emits() {
        assert!(HELP.contains(crate::config::write::SCHEMA_URL), "the `$schema` pointer in HELP has drifted");
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
        for args in [&["--statusline", "--debug"][..], &["--configure", "--dry-run"], &["--version"], &["-h"]] {
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
