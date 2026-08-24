//! `--configure`: wire Claude Code to this binary, and seed a user config.
//!
//! `brew install` then `claude-status --configure` is the whole setup. The npm
//! installer that used to do this is being deleted, so this is the only thing
//! left that knows how Claude Code is wired.
//!
//! This is the **only mode that writes under `$HOME`** outside the spend cache,
//! and the only one that can exit non-zero. Both are deliberate, and both are
//! carved out by name in `no_mode_writes_outside_the_cache_directory` rather
//! than left as a silent exception to an invariant that test argues holds
//! everywhere.
//!
//! # It refuses rather than guesses
//!
//! `~/.claude/settings.json` is Claude Code's file, not this tool's. So the
//! three ways this can go wrong are kept apart:
//!
//! - **absent** — normal. There is nothing to lose, so the file is created.
//! - **unreadable, malformed, or a shape this tool cannot merge into** —
//!   refuse. Name the file, change nothing, exit non-zero.
//! - **readable** — merge the three keys and leave every other one alone.
//!
//! The TypeScript did not draw that line. `readSettings`
//! (`installer/src/modules/settings.ts:66-71`) parsed inside a bare
//! `catch { return null }`, fell back to `{}`, and the write that followed
//! **replaced the user's entire Claude Code configuration with three keys**.
//! Silent data loss on a file we do not own, reachable from a single stray
//! comma. Nothing here may do that.
//!
//! # There is no receipt and no `--unconfigure`
//!
//! Decided, not omitted. A statusLine belonging to another tool is overwritten
//! without asking, which means the printing below is not cosmetic — it is the
//! entire mitigation, and it is why a *foreign* value is quoted back on stderr
//! rather than merely counted.
//!
//! # `--configure` means something else in the installer
//!
//! `installer/src/_runtime/configure.ts` used the same flag for the opposite
//! thing: giving the repo you are standing in a repo-level config layer, and it
//! advertises itself as "the only one that writes nothing under `~`". The name
//! is **deliberately repurposed** — this `--configure` writes only under `~`,
//! and the repo layer is now written by hand (see `--help`). A reader who knows
//! the old flag should find that said out loud rather than infer it.
//!
//! The two also now write the **same path**: `installer/src/_shared/paths.ts`
//! puts its `config` at `~/.config/claude-status/config.json`, which is what
//! step 4 below seeds. No data is at risk — this one writes only when the file
//! is absent, and the installer's uninstall is sha-guarded — but until
//! `distribution/01` deletes the installer, two tools write one file with
//! different contents: the installer seeds the whole defaults asset verbatim,
//! this seeds a `$schema` pointer alone. Recorded, not solved here.

use std::fmt::Write as _;
use std::path::Path;

use serde_json::{Map, Value};

use crate::_runtime::app::Outcome;
use crate::_shared::paths::home;
use crate::config::write::SCHEMA_KEY;
use crate::config::{Config, layers, write};
use crate::json::write_json_atomic_pretty_mode;
use crate::modules::settings::{self, Change, Ownership, Wiring};

/// The longest quoted-back value one warning may carry.
///
/// It is a whole JSON value out of a file this tool does not own, so it could
/// be a megabyte. The warning exists to let a user recognise what they are
/// losing, and a prefix does that.
const QUOTE_LIMIT: usize = 200;

/// The `--dry-run` marker.
///
/// Every line a real run would have printed as an action is prefixed instead,
/// because a dry run whose output is indistinguishable from a real one teaches
/// the user nothing about which it was. The TypeScript used the same two words
/// (`installer/src/_shared/io.ts:30-32`).
const WOULD: &str = "would ";

pub(crate) fn run(dry_run: bool, unknown: &[String]) -> Outcome {
    // **Before anything else, and before `$HOME` is even looked at.** The
    // parser ignores unrecognised arguments by design — right for a render
    // surface Claude Code invokes, where a stray token must never cost a bar.
    // Here it is the opposite: a single mistyped character in
    // `--configure --dry-runn` would silently perform a **real** overwrite of
    // the user's `settings.json`, unundoable, while they believed they had
    // asked for a preview. A destructive flag does not get to guess.
    if !unknown.is_empty() {
        let named: Vec<String> = unknown.iter().map(|a| quote(a)).collect();
        return refuse(&format!("unrecognised argument{}: {}", plural(unknown.len()), named.join(", ")));
    }

    let Some(home) = home() else {
        // Nothing to do and nowhere to do it. Guessing a relative `.claude/`
        // would wire Claude Code to a file under whatever directory this
        // happened to be run from.
        return refuse("$HOME is unset, so there is no ~/.claude/settings.json to wire");
    };

    let settings_path = home.join(".claude").join("settings.json");
    let existing = match read_settings(&settings_path) {
        Ok(value) => value,
        Err(reason) => return refuse(&format!("{} {reason}", tilde(&settings_path, &home))),
    };

    let wiring = match settings::wire(&existing) {
        Ok(wiring) => wiring,
        Err(refusal) => return refuse(&format!("{} — {}", tilde(&settings_path, &home), refusal.reason())),
    };

    // Loud, and **before** anything is written, because there is no undo. Only
    // a genuinely foreign value gets here: a stale value of our own is rewritten
    // quietly, since shouting about it would be shouting at a user about their
    // own previous install.
    for change in &wiring.changes {
        if let Some(replaced) = &change.replaced {
            crate::_shared::diag(&format!(
                "claude-status: replacing the {} you had set: {}",
                change.key,
                quote(replaced),
            ));
        }
    }

    let mut out = String::new();
    let _ = writeln!(out, "CLAUDE CODE ({})", tilde(&settings_path, &home));
    for change in &wiring.changes {
        let _ = writeln!(out, "  {:20} {}", change.key, verdict(change));
    }
    let wrote = report_write(&mut out, &settings_path, &home, &wiring, dry_run);

    let config_path = layers::user_config_path(&home);
    let _ = writeln!(out, "\nYOUR CONFIG ({})", tilde(&config_path, &home));
    let seeded = report_seed(&mut out, &config_path, &home, dry_run);

    // **A write this mode could not perform is a failure**, even though the
    // report says so in words. A read-only `$HOME`, a full disk or a
    // permissions problem all end up here, and a setup script that saw exit 0
    // would carry on and tell the user their bar was wired. Either half
    // failing is enough: the seed is the smaller of the two, but "it mostly
    // worked" reported as success is what makes a script wrong later.
    let code = i32::from(!(wrote && seeded));

    // The same rule §4a applies to `--debug`: everything above is drawn from a
    // file this tool does not own — key names, and the `settings.json` path
    // itself — so the assembled report is swept once rather than at each write.
    // The report-keeping variant, because it is deliberately many lines.
    Outcome { stdout: crate::render::sanitize_report(&out), code }
}

/// Reads `settings.json`, distinguishing **absent** from **broken**.
///
/// [`crate::json::read_json_file`] cannot be used here: it collapses missing,
/// unreadable and malformed into one `None`, which is exactly the distinction
/// this mode turns on. A layer that does not exist is a layer to skip; a
/// settings file that does not parse is a file to leave alone.
///
/// # Three inputs this refuses that Claude Code itself accepts
///
/// `serde_json` is stricter than the parser Claude Code uses, so a **UTF-8
/// BOM**, a float that overflows to infinity (`1e400`), and nesting deeper than
/// 128 levels are all refused here while loading fine there. The BOM is the
/// plausible one — a file written by a Windows editor. Refusing is the safe
/// direction, since the alternative is writing over a file we did not
/// understand, but the message a user gets names the parser's complaint rather
/// than the real cause, so a BOM reads as an unhelpful "expected value at line
/// 1 column 1". Recorded rather than special-cased: stripping a BOM here would
/// leave this reader and every other JSON reader in the crate disagreeing about
/// what valid input is.
///
/// # Numbers are re-encoded, not preserved
///
/// Without `arbitrary_precision`, `serde_json` normalises integers beyond
/// `u64::MAX` and floats past 17 significant digits — `12345678901234567890123`
/// comes back as `1.2345678901234568e+22`. So "every other key is left as it
/// was" is true of every *value shape* but not, strictly, of extreme numeric
/// literals. `u64::MAX`, `1.0` and `-0.0` all round-trip exactly, so nothing a
/// real `settings.json` contains is affected. Not fixed on purpose: enabling
/// `arbitrary_precision` would change number handling across the whole binary,
/// including the config path and the goldens, which is far past this cycle.
fn read_settings(path: &Path) -> Result<Value, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("is not valid JSON — {e}")),
        // **A dangling symlink is not an absent file.** `read_to_string` reports
        // both as `NotFound`, and treating this one as absent would take the
        // write path — where `canonicalize` also fails, the fallback writes to
        // the link's own path, and the rename **destroys the link**. That is the
        // exact damage the symlink handling exists to prevent, hit in the one
        // state it cannot detect any other way: a dotfiles repo not cloned yet,
        // `stow` not run yet, an external volume unmounted.
        //
        // Refusing rather than creating the target: the user's setup is
        // transiently broken, and materialising a whole missing directory tree
        // would leave a phantom to collide with the real clone later. This is
        // the same absent-versus-broken line the rest of this function draws.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => match std::fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                Err("is a symlink whose target is missing — restore it, or remove the link".to_string())
            }
            _ => Ok(Value::Object(Map::new())),
        },
        Err(e) => Err(format!("could not be read — {e}")),
    }
}

/// A refusal: nothing on stdout, the reason on stderr, and a non-zero exit so a
/// script can tell.
fn refuse(reason: &str) -> Outcome {
    crate::_shared::diag(&format!("claude-status: refusing to configure — {reason} (run --help)"));
    Outcome { stdout: String::new(), code: 1 }
}

fn plural(n: usize) -> &'static str {
    match n {
        1 => "",
        _ => "s",
    }
}

/// One key's answer, in three words or fewer.
fn verdict(change: &Change) -> &'static str {
    if !change.changed {
        return "already wired";
    }
    match change.ownership {
        Ownership::Absent => "set",
        // The one destructive case, and the one that also warned on stderr.
        Ownership::Foreign => "REPLACED",
        // A previous install of ours, current or stale — the same word for
        // both, on purpose. See the module note on quiet rewriting.
        Ownership::Ours | Ownership::OursStale => "updated",
    }
}

/// Returns whether the file is now in the state this run promised. A dry run
/// and a no-op both count: neither left anything undone.
fn report_write(out: &mut String, path: &Path, home: &Path, wiring: &Wiring, dry_run: bool) -> bool {
    if !wiring.changed() {
        // Not "wrote it anyway": leaving the file completely alone is what
        // makes a second run byte-identical rather than merely value-identical,
        // and this tool has no business normalising someone else's indentation.
        let _ = writeln!(out, "  nothing to change — left untouched");
        return true;
    }

    // **Follow the symlink before writing.** `write_json_atomic_pretty` is
    // temp-then-rename, and a rename over a symlink *replaces the symlink with
    // a regular file* — so a `~/.claude/settings.json` symlinked into a
    // dotfiles repo would be silently unlinked and the real file orphaned, with
    // the user's settings apparently reverting on their next sync. Resolving
    // first puts the atomic replace at the real location and leaves the link
    // intact. A symlink inside the user's own `$HOME` pointing somewhere
    // unexpected is not a threat this can defend against: anyone able to plant
    // one already owns the account.
    //
    // Only when the **file itself** is a link. Canonicalizing unconditionally
    // would also resolve every parent directory, so a `$HOME` reached through a
    // symlinked ancestor — `/tmp` → `/private/tmp` on macOS, and every test
    // temp directory with it — would report a symlink that is not there and
    // print a path the user does not recognise.
    //
    // # Known limitation: a HARDLINKED settings.json goes stale
    //
    // **The root fact, true of every write here:** temp-then-rename replaces
    // the directory entry, so the file at this path gets a **new inode** on
    // every single run. That is what the symlink resolution above exists to
    // work around, and it is what no amount of resolution can fix for a
    // hardlink — because a hardlink is not a pointer to a path, it is a second
    // name for an inode, and there is nothing to follow.
    //
    // Measured: both names start on one inode with `nlink=2`; afterwards
    // `~/.claude/settings.json` is a new inode carrying the correct wiring, and
    // the other name still holds the pre-run bytes with `nlink=1`.
    //
    // **Accepted, not overlooked**, and it is a much milder failure than the
    // symlink one above. Claude Code reads `~/.claude/settings.json`, which is
    // correct afterwards; nothing is lost, one other path merely stops tracking
    // it. The only fix is to write in place — truncate and rewrite — and that
    // trades away atomicity on a file that **breaks Claude Code outright if it
    // is ever seen half-written**. A stale second name is worth far less than
    // that risk. Every atomic writer on the system behaves this way, editors
    // included, so a user keeping hardlinked dotfiles already lives with it.
    //
    // It is not left *silent*, though — see [`warn_if_hardlinked`], which is
    // step 3's "the destructive case must be visible" applied to the one
    // consequence here that a user cannot otherwise find out about.
    // `a_hardlinked_settings_file_goes_stale_because_the_write_is_atomic` pins
    // the limitation itself, so the trade cannot be quietly reversed.
    let is_link = path.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink());
    let target = match is_link {
        true => std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
        false => path.to_path_buf(),
    };
    // One `stat` of the file about to be replaced, read for two things: the
    // mode to carry over, and the link count to warn about.
    let existing = std::fs::metadata(&target).ok();
    // Keep whatever mode it had, and **hand it to the writer** rather than
    // chmod-ing afterwards. A `fs::write` creates at 0644 minus the umask, so a
    // re-tighten after the fact leaves the user's `env` block world-readable for
    // the length of the write — and a signal in that window strands a
    // world-readable copy on disk permanently. See `write_json_atomic_pretty_mode`.
    let mode = existing.as_ref().map(|m| std::os::unix::fs::PermissionsExt::mode(&m.permissions()));
    warn_if_hardlinked(existing.as_ref(), &target, home, dry_run);

    if dry_run {
        let _ = writeln!(out, "  {WOULD}write {}", named(&target, path, home, existing.as_ref()));
        return true;
    }
    match write_json_atomic_pretty_mode(&target, &wiring.settings, mode) {
        Ok(()) => {
            let _ = writeln!(out, "  wrote {}", named(&target, path, home, existing.as_ref()));
            true
        }
        Err(e) => {
            crate::_shared::diag(&format!("claude-status: could not write {} — {e}", tilde(path, home)));
            let _ = writeln!(out, "  FAILED to write {}", tilde(path, home));
            false
        }
    }
}

/// Says that this write is about to break a hard link, when it is.
///
/// **This is step 3's rule, not an extra.** *"Because there is no receipt and
/// no undo, the destructive case must be visible."* A stale hard link is a
/// destructive case with no undo — and it is worse served than the one step 3
/// was written for, because an overwritten `statusLine` at least gets quoted
/// back. This one is otherwise **completely undiscoverable**: the user's other
/// name silently stops tracking their live settings and nothing anywhere says
/// so. So the rule applies, to a case the plan did not know existed.
///
/// It says the link count and stops there. **Naming the other path is not
/// possible** — a hard link is a second name for an inode, and an inode does
/// not know its own names; finding them means walking the filesystem. That
/// impossibility is the same fact that makes the limitation unfixable, so the
/// warning is deliberately shaped by it rather than apologising for it.
///
/// **`--dry-run` gets it too, in the `would` form, rather than suppressed.** A
/// dry run exists to show what a real run would do, and this is the one
/// consequence a user cannot see any other way — suppressing it would leave the
/// preview silent about the only thing the preview is uniquely good for.
///
/// Never blocks and never changes the exit code: the write is correct and the
/// user asked for it. This is information, not a refusal.
fn warn_if_hardlinked(existing: Option<&std::fs::Metadata>, target: &Path, home: &Path, dry_run: bool) {
    use std::os::unix::fs::MetadataExt as _;
    use std::os::unix::fs::PermissionsExt as _;

    let tense = if dry_run { "would" } else { "will" };

    let links = existing.map_or(1, std::fs::Metadata::nlink);
    if links >= 2 {
        // A symlink is *not* this case and must not warn: it was resolved
        // above, so `target` is the real file and its own link count is read.
        crate::_shared::diag(&format!(
            "claude-status: {} has {links} hard links — the write replaces the file, so the other name{} {tense} \
             stop tracking it",
            tilde(target, home),
            if links > 2 { "s" } else { "" },
        ));
    }

    // **A read-only file is replaced anyway**, and the same rule applies to
    // saying so. `rename` needs write permission on the *directory*, not on the
    // file, so a `settings.json` the user set to 0400 is swapped out and then
    // restored to 0400 — the mode is honoured, the intent behind it is not, and
    // the output is otherwise indistinguishable from an ordinary run.
    //
    // Not a refusal: the user typed `--configure`, which is a clearer statement
    // of intent than a mode bit set at some point in the past. Not a change to
    // the write either — refusing to use `rename` here would cost atomicity,
    // which is the one thing that must not be traded (see the note above). So
    // it is reported, for the same reason the hard-link case is: it is a
    // consequence the output would otherwise hide completely.
    let writable = existing.is_none_or(|m| m.permissions().mode() & 0o200 != 0);
    if !writable {
        crate::_shared::diag(&format!(
            "claude-status: {} is read-only — an atomic replace needs no write permission on the file, so it {tense} \
             be rewritten anyway (its mode is preserved)",
            tilde(target, home),
        ));
    }
}

/// The path to print. When the file the user named and the file actually
/// written are different — a symlink — **say both**, because "which file did
/// this change" is the only question a dotfiles user will have.
///
/// The hard-link count rides along for the same reason. The stderr warning is
/// the loud half, but a report that says a bare `wrote …` for the one case that
/// quietly desynchronises a second path is the asymmetry a reader notices.
fn named(target: &Path, path: &Path, home: &Path, existing: Option<&std::fs::Metadata>) -> String {
    use std::os::unix::fs::MetadataExt as _;

    let links = existing.map_or(1, std::fs::Metadata::nlink);
    let note = match links {
        0 | 1 => String::new(),
        n => format!(" [{n} hard links — see the warning on stderr]"),
    };
    if target == path {
        return format!("{}{note}", tilde(path, home));
    }
    format!("{} (a symlink to {}){note}", tilde(path, home), tilde(target, home))
}

/// Seeds `~/.config/claude-status/config.json` when there is none.
///
/// `$schema` and nothing else — a starting point an editor can complete
/// against, and the opposite of what the npm installer did, which seeded the
/// whole defaults asset and so froze every shipped value at the version that
/// happened to be installed.
///
/// **An existing config is never touched.** Not merged, not topped up, not
/// reordered — the file is the user's, and a writer that "helpfully" rewrote it
/// would have to round-trip a degraded config, which is lossy by construction
/// (`config/write.rs`).
fn report_seed(out: &mut String, path: &Path, home: &Path, dry_run: bool) -> bool {
    // `symlink_metadata`, not `exists()`: the latter follows the link, so a
    // config symlinked into a dotfiles repo whose target is temporarily missing
    // would read as absent — and the write below is a rename, which would
    // replace the link with a regular file. Anything at all at that path is
    // something this tool did not put there.
    if path.symlink_metadata().is_ok() {
        let _ = writeln!(out, "  already there — left untouched");
        return true;
    }
    if dry_run {
        let _ = writeln!(out, "  {WOULD}create it, holding a \"{SCHEMA_KEY}\" pointer and nothing else");
        return true;
    }
    // **`Config::default()`, never the loaded config**, and the difference is
    // not cosmetic. `layers::load` merges the *repo* layer's `projectName` into
    // what it returns, so writing the loaded config would take the name of
    // whatever repository the user happened to be standing in and pin it,
    // permanently, into their **global** `~/.config/claude-status/config.json`
    // — where it would then override every other repo's name. That is why this
    // mode never calls `load` at all.
    //
    // It is also what keeps `write.rs`'s two pinned exemptions out of reach.
    // Against `Config::default()` the diff is empty by construction, so
    // `non_defaults` is `$schema` alone. Against a *loaded* config both
    // exemptions bite at once: a user whose `subagent` block had degraded would
    // find `statuses: {}` silently normalised back to the shipped values on
    // disk, because degradation maps several inputs onto one state and a writer
    // that round-tripped it would persist the damage rather than the
    // configuration. `write.rs`'s "inherits a known boundary" is this link.
    match write::write(path, &Config::default()) {
        Ok(()) => {
            let _ = writeln!(out, "  created, holding a \"{SCHEMA_KEY}\" pointer and nothing else");
            true
        }
        Err(e) => {
            // `tilde`, like every other path this mode prints. Its own doc says
            // the report "is the thing people paste into an issue, and the
            // username is the one part of these paths that is nobody's
            // business" — which a bare `display()` here quietly undid.
            crate::_shared::diag(&format!("claude-status: could not write {} — {e}", tilde(path, home)));
            let _ = writeln!(out, "  FAILED to create it");
            false
        }
    }
}

/// `~/…` rather than `/Users/someone/…`.
///
/// A `--configure` report is the thing people paste into an issue, and the
/// username is the one part of these paths that is nobody's business. The
/// TypeScript had the same helper for the same reason.
fn tilde(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
        Err(_) => path.display().to_string(),
    }
}

/// One untrusted value, made safe to print and short enough to read.
fn quote(value: &str) -> String {
    let safe = crate::render::sanitize(value);
    match safe.char_indices().nth(QUOTE_LIMIT) {
        Some((at, _)) => format!("{}…", &safe[..at]),
        None => safe,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// The one path this mode derives from `$HOME`, so the assertions below
    /// check the file it actually claims to write.
    fn settings_path(home: &Path) -> PathBuf {
        home.join(".claude").join("settings.json")
    }

    /// A throwaway `$HOME` with `--configure` run against it.
    ///
    /// **The `HOME` guard is not optional.** `paths::home()` reads the live
    /// process environment, so a test that exercises this write path without it
    /// rewrites the developer's own `~/.claude/settings.json`. It is held for
    /// the whole call and restored on drop, including on a failed assertion.
    fn configure_in(settings: Option<&str>, dry_run: bool) -> (tempfile::TempDir, Outcome) {
        let home = tempfile::TempDir::new().unwrap();
        if let Some(body) = settings {
            let dir = home.path().join(".claude");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("settings.json"), body).unwrap();
        }

        let mut env = crate::_shared::env_lock();
        env.set("HOME", home.path().to_str().unwrap());
        let outcome = run(dry_run, &[]);
        (home, outcome)
    }

    fn read(path: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(path).expect("the file is there")).expect("it is JSON")
    }

    #[test]
    fn an_absent_settings_file_is_created_rather_than_refused() {
        let (home, outcome) = configure_in(None, false);
        assert_eq!(outcome.code, 0, "an absent file is the normal case, not a broken one");

        let settings = read(&settings_path(home.path()));
        assert_eq!(settings["statusLine"]["command"], "claude-status --statusline");
        assert_eq!(settings["subagentStatusLine"]["command"], "claude-status --subagent");
        assert_eq!(settings["hooks"]["PostToolUse"][0]["hooks"][0]["command"], "claude-status --caps-hook");
    }

    /// The single most dangerous behaviour in the code being ported. The
    /// TypeScript would have replaced this entire file with three keys.
    #[test]
    fn a_malformed_settings_file_is_refused_and_left_exactly_as_it_was() {
        let broken = r#"{ "model": "opus", "permissions": { "allow": [] },, }"#;
        let (home, outcome) = configure_in(Some(broken), false);

        assert_eq!(outcome.code, 1, "a file this tool cannot read must not be a file it writes");
        assert_eq!(outcome.stdout, "", "and it must not claim to have done anything");
        assert_eq!(
            std::fs::read_to_string(settings_path(home.path())).unwrap(),
            broken,
            "the user's settings.json was rewritten",
        );
        assert!(!layers::user_config_path(home.path()).exists(), "a refusal writes nothing at all, not even the seed");
    }

    #[test]
    fn a_shape_the_merge_cannot_read_is_refused_with_the_file_named() {
        for body in [r#"[1, 2, 3]"#, r#"{ "hooks": "all" }"#, r#"{ "hooks": { "PostToolUse": {} } }"#] {
            let (home, outcome) = configure_in(Some(body), false);
            assert_eq!(outcome.code, 1, "{body} was wired rather than refused");
            assert_eq!(std::fs::read_to_string(settings_path(home.path())).unwrap(), body, "{body} was rewritten");
        }
    }

    #[test]
    fn with_no_home_it_refuses_rather_than_writing_somewhere_relative() {
        let mut env = crate::_shared::env_lock();
        env.unset("HOME");
        let outcome = run(false, &[]);
        assert_eq!(outcome.code, 1);
        assert_eq!(outcome.stdout, "");
    }

    /// **Criterion 5.** A dry run prints its plan and touches nothing.
    #[test]
    fn a_dry_run_prints_what_it_would_do_and_writes_nothing() {
        let (home, outcome) = configure_in(Some(r#"{ "model": "opus" }"#), true);

        assert_eq!(outcome.code, 0);
        assert!(outcome.stdout.contains("would write"), "a dry run must be distinguishable: {}", outcome.stdout);
        assert!(outcome.stdout.contains("would create"), "{}", outcome.stdout);
        assert_eq!(
            std::fs::read_to_string(settings_path(home.path())).unwrap(),
            r#"{ "model": "opus" }"#,
            "a dry run wrote to settings.json",
        );
        assert!(!layers::user_config_path(home.path()).exists(), "a dry run seeded a config");
    }

    /// **Criterion 6.** Seeded only when absent, and never touched afterwards.
    #[test]
    fn the_user_config_is_seeded_with_a_schema_pointer_alone() {
        let (home, _) = configure_in(None, false);
        let path = layers::user_config_path(home.path());
        assert_eq!(read(&path), serde_json::json!({ SCHEMA_KEY: write::SCHEMA_URL }));
    }

    #[test]
    fn an_existing_user_config_is_left_byte_identical() {
        let home = tempfile::TempDir::new().unwrap();
        let path = layers::user_config_path(home.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Deliberately not what this tool would have written: odd indentation,
        // no `$schema`, a key order of the user's own.
        let body = "{\n\t\"projectName\": \"mine\",\n\t\"defaultFg\": \"aqua\"\n}";
        std::fs::write(&path, body).unwrap();

        let mut env = crate::_shared::env_lock();
        env.set("HOME", home.path().to_str().unwrap());
        let outcome = run(false, &[]);

        assert_eq!(outcome.code, 0);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), body, "an existing config was touched");
        assert!(outcome.stdout.contains("already there"), "{}", outcome.stdout);
    }

    /// **Criteria 1 and 2**, at the surface this mode actually writes.
    #[test]
    fn unrelated_keys_and_another_tools_hook_survive_the_write() {
        let (home, outcome) = configure_in(
            Some(
                &serde_json::json!({
                    "model": "opus",
                    "permissions": { "allow": ["Bash(ls:*)"], "deny": [] },
                    "hooks": {
                        "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "/usr/bin/audit" }] }],
                        "PostToolUse": [{ "matcher": "Edit|Write", "hooks": [{ "type": "command", "command": "/usr/bin/fmt" }] }],
                    },
                })
                .to_string(),
            ),
            false,
        );
        assert_eq!(outcome.code, 0);

        let after = read(&settings_path(home.path()));
        assert_eq!(after["model"], "opus");
        assert_eq!(after["permissions"], serde_json::json!({ "allow": ["Bash(ls:*)"], "deny": [] }));
        assert_eq!(after["hooks"]["PreToolUse"][0]["hooks"][0]["command"], "/usr/bin/audit");

        let groups = after["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(groups[0]["matcher"], "Edit|Write", "their matcher was rewritten");
        assert_eq!(groups[0]["hooks"][0]["command"], "/usr/bin/fmt");
        assert_eq!(groups[1]["hooks"][0]["command"], "claude-status --caps-hook");
    }

    /// **Criterion 4.** Replaced *and* said so — the printing is the whole
    /// mitigation for having no undo.
    #[test]
    fn a_foreign_status_line_is_replaced_and_reported() {
        let (home, outcome) = configure_in(
            Some(r#"{ "statusLine": { "type": "command", "command": "starship prompt" } }"#),
            false,
        );

        assert!(outcome.stdout.contains("REPLACED"), "{}", outcome.stdout);
        assert_eq!(read(&settings_path(home.path()))["statusLine"]["command"], "claude-status --statusline");
    }

    /// **Criterion 3**, end to end and over **three** runs, because a hook list
    /// that grows by one entry per run takes three to notice.
    #[test]
    fn three_runs_leave_the_file_byte_identical_after_the_first() {
        let home = tempfile::TempDir::new().unwrap();
        let dir = home.path().join(".claude");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), r#"{"model":"opus"}"#).unwrap();

        let mut env = crate::_shared::env_lock();
        env.set("HOME", home.path().to_str().unwrap());

        let path = settings_path(home.path());
        assert_eq!(run(false, &[]).code, 0);
        let first = std::fs::read(&path).unwrap();
        assert_eq!(run(false, &[]).code, 0);
        let second = std::fs::read(&path).unwrap();
        assert_eq!(run(false, &[]).code, 0);
        let third = std::fs::read(&path).unwrap();

        assert_eq!(first, second, "the second run changed the bytes");
        assert_eq!(second, third, "the third run changed the bytes");
        let groups = read(&path)["hooks"]["PostToolUse"].as_array().unwrap().len();
        assert_eq!(groups, 1, "the PostToolUse list grew by a group per run");
    }

    /// The value quoted back on stderr came out of the user's file, so it goes
    /// through the same filter every other terminal write does (§4a). It is
    /// bounded too: a `command` key could hold a megabyte.
    #[test]
    fn a_quoted_back_value_is_sanitized_and_bounded() {
        let hostile = format!("\u{1b}[2J{}", "x".repeat(QUOTE_LIMIT * 3));
        let quoted = quote(&hostile);
        assert!(!quoted.contains('\u{1b}'), "an escape survived: {}", quoted.escape_debug());
        assert!(quoted.chars().count() <= QUOTE_LIMIT + 1, "unbounded: {} chars", quoted.chars().count());
        assert!(quoted.ends_with('…'), "a truncation must say so: {quoted}");
    }

    #[test]
    fn a_quoted_back_value_shorter_than_the_limit_is_not_marked_truncated() {
        assert_eq!(quote("starship prompt"), "starship prompt");
    }

    #[test]
    fn tilde_hides_the_username_and_leaves_paths_outside_home_alone() {
        let home = Path::new("/Users/someone");
        assert_eq!(tilde(&home.join(".claude").join("settings.json"), home), "~/.claude/settings.json");
        assert_eq!(tilde(Path::new("/etc/hosts"), home), "/etc/hosts");
    }
}
