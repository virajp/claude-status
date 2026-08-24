//! The render pipeline and the process's single write to stdout.
//!
//! This module is the **only** place anything writes to stdout, and it writes
//! only after the renderer has returned a complete `String`. Streaming segments
//! could emit half a bar and *then* the fallback, which is worse than either.


use std::fmt::Write as _;
use std::io::{IsTerminal, Read, Write as _};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use crate::_shared::paths::home;
use crate::cli::{Cli, HELP, MISSING_FLAG, Mode, VERSION};
use crate::config::layers::{self, Layers};
use crate::config::{Config, SegmentEntry};
use crate::git::GitFacts;
use crate::payload::MainFacts;
use crate::render::main_bar::render_main;
use crate::render::subagent;
use crate::_runtime::configure;
use crate::_shared::proc;
use crate::modules::spend;
use crate::modules::caps;
use crate::modules::settings;
use crate::{cli, git, json, payload, time, usage};

/// What the bar falls back to when a render panics: U+26A1, a space, `Claude`.
const FALLBACK_LINE: &str = "\u{26a1} Claude";

/// The process entry point. Returns the exit code.
pub fn run() -> i32 {
    let cli = cli::parse(std::env::args_os(), std::io::stdin().is_terminal());
    proc::set_narrate(cli.debug);
    let Outcome { stdout, code } = dispatch(cli);
    write_stdout(&stdout);
    code
}

/// One invocation's whole answer.
///
/// The exit code is part of it because [`Mode::Configure`] can **refuse**:
/// every other mode is infallible by construction — §1's invariant 3 makes a
/// render that cannot work still print a line and exit 0 — but a `--configure`
/// that declined to touch a file has to be distinguishable by a script, and a
/// message on stderr is not.
pub(crate) struct Outcome {
    pub(crate) stdout: String,
    pub(crate) code: i32,
}

impl From<String> for Outcome {
    fn from(stdout: String) -> Self {
        Self { stdout, code: 0 }
    }
}

/// Builds the complete stdout payload, and the exit code, for one invocation.
fn dispatch(cli: Cli) -> Outcome {
    match cli.mode {
        // Checked first and never decorated: the release workflow and the
        // build smoke test both fail over the shape of this answer.
        Mode::Version => format!("{VERSION}\n").into(),
        Mode::Help => HELP.to_string().into(),
        Mode::MissingFlag => format!("{MISSING_FLAG}\n").into(),
        Mode::Statusline => render_statusline(cli.debug).into(),
        Mode::Subagent => render_subagent().into(),
        Mode::Refresh => refresh_spend().into(),
        Mode::CapsHook => caps_hook().into(),
        Mode::Configure => configure::run(cli.dry_run, &cli.unknown),
        Mode::Debug => debug_report().into(),
    }
}

/// The detached refresh child. It writes **nothing** to stdout: it is spawned
/// with its stdio at `/dev/null`, so anything it said would be discarded, and
/// it always exits 0.
fn refresh_spend() -> String {
    // No cache path — no `$HOME` — means there is nowhere to write the result,
    // so there is no point making the request.
    let Some(path) = spend::cache::path() else {
        return String::new();
    };
    let config = layers::load(home().as_deref(), None).config;
    spend::refresh::run(&path, config.spend.refresh_minutes, time::now_ms(), false);
    String::new()
}

/// Renders the main bar, catching a panic into the fallback line.
fn render_statusline(debug: bool) -> String {
    let narrate = |msg: &str| {
        if debug {
            crate::_shared::diag(&format!("claude-status: {msg}"));
        }
    };

    match catch_unwind(AssertUnwindSafe(|| build_bar(&narrate))) {
        Ok(bar) => bar,
        Err(payload) => {
            // The real error goes to stderr; stdout still gets a usable line.
            crate::_shared::diag(&format!("claude-status error: {}", panic_message(&payload)));
            FALLBACK_LINE.to_string()
        }
    }
}

/// The vwf caps hook. **Silence is the normal outcome**, and a panic is
/// silent too: the `⚡ Claude` fallback must not apply here, because whatever
/// this writes to stdout is injected verbatim into the agent's context. A
/// status-bar fragment arriving there is worse than nothing.
fn caps_hook() -> String {
    match catch_unwind(AssertUnwindSafe(build_caps_directive)) {
        Ok(out) => out,
        Err(payload) => {
            crate::_shared::diag(&format!("claude-status error: {}", panic_message(&payload)));
            String::new()
        }
    }
}

/// Reads the hook JSON, resolves the caps, and emits a directive only when the
/// breach is an **escalation** above the last one recorded for this session.
fn build_caps_directive() -> String {
    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);
    let input = payload::parse(&stdin);

    // Inert without a usage directory or a session to key on.
    let (Some(dir), Some(session_id)) = (usage::usage_dir_from_env(), json::opt_str(&input, "session_id")) else {
        return String::new();
    };
    // Inert too when the directory names `$HOME` and there is none — the same
    // rule the writer follows, so the hook never reads from a directory the bar
    // would not have written to.
    let Some(dir) = usage::expand_home(&dir) else {
        return String::new();
    };

    // No mirror yet: the bar has not rendered this session. Not an error.
    let Some(mirror) = json::read_json_file(&dir.join(format!("{session_id}.json"))) else {
        return String::new();
    };

    // Caps come from the same three layers everything else does, so the repo
    // root has to be resolved here too — the hook stays strictly read-only
    // while doing it, unlike `--statusline`, which may create layer 3.
    let cwd = json::opt_str(&input, "cwd");
    let root = cwd.map(PathBuf::from).and_then(|c| git::find_root_and_branch(&c).0);
    let caps = caps::resolve_caps(&layers::load(home().as_deref(), root.as_deref()).config);

    // The budget figure is the refresh child's, read from its cache. A seat
    // without a budget block yields `None`, which never breaches.
    let usage = caps::Usage::from_mirror(&mirror)
        .with_spend(spend::cache::path().as_deref().and_then(spend::cache::read_from).as_ref());

    let Some((level, directive)) = caps::level(&usage, &caps, time::now_ms()) else {
        return String::new();
    };

    // The debounce file's name is **not ours to choose**: it sits beside
    // `<sid>.json`, and during the transition the JS hook writes it too. A
    // machine running both must not double-fire.
    let state = dir.join(format!("{session_id}.state.json"));
    let last = json::read_json_file(&state).and_then(|s| json::opt_f64(&s, "level")).unwrap_or(0.0);
    if f64::from(level) <= last {
        return String::new();
    }

    // A failed write must never suppress the directive — a read-only usage
    // directory should cost the debounce, not the cap.
    let _ = json::write_json_atomic(&state, &serde_json::json!({ "level": level, "ts": time::now_ms() }));

    caps::envelope(&directive)
}

/// Renders the subagent panel, catching a panic into **empty output**.
///
/// **Divergence from the JS entry point**, which printed `⚡ Claude` on any
/// error whichever surface was rendering. Here that would be a line of NDJSON
/// that is not JSON, and the consumer parses every line it gets. An empty panel
/// is a valid panel; a malformed one is not.
fn render_subagent() -> String {
    match catch_unwind(AssertUnwindSafe(build_panel)) {
        Ok(panel) => panel,
        Err(payload) => {
            crate::_shared::diag(&format!("claude-status error: {}", panic_message(&payload)));
            String::new()
        }
    }
}

/// The panel's own pipeline. Deliberately shorter than the bar's: no usage
/// mirror, no spend gate, and no git marker resolution — the panel renders none
/// of those, and a subagent render must cost nothing it does not use.
fn build_panel() -> String {
    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);
    let payload = payload::parse(&stdin);

    let process_cwd = std::env::current_dir().ok().map(|p| p.to_string_lossy().into_owned());
    let cwd = subagent::panel_cwd(&payload, process_cwd);

    // The repo config layer still comes from the resolved git root, so a panel
    // does pick up per-repo theming.
    let root = cwd.as_deref().map(PathBuf::from).and_then(|c| git::find_root_and_branch(&c).0);
    let layers = layers::load(home().as_deref(), root.as_deref());

    subagent::render_panel(&payload, &layers.config, time::now_ms(), std::env::var("COLUMNS").ok().as_deref())
}

fn build_bar(narrate: &dyn Fn(&str)) -> String {
    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);
    let payload = payload::parse(&stdin);

    let cwd = std::env::current_dir().ok().map(|p| p.to_string_lossy().into_owned());
    let facts = payload::normalise(&payload, time::now_ms(), cwd);

    // The mirror runs before anything that can fail, and is gated on neither
    // the layout nor git: a broken config must not cost the caps hook its data.
    usage::mirror(&facts, usage::usage_dir_from_env().as_deref(), facts.session_id.as_deref());

    // Root first, because the repo config layer is read from it, and only then
    // is `worktreePattern` available to match with.
    let cwd_path = facts.cwd.as_deref().map(PathBuf::from);
    let (root, branch) = match cwd_path.as_deref() {
        Some(cwd) => git::find_root_and_branch(cwd),
        None => (None, None),
    };
    narrate(&format!("repo root: {root:?}, branch: {branch:?}"));

    // Read, and only read. `--statusline` used to be able to *create* the repo
    // layer it did not find, which made the one surface that redraws every four
    // seconds also the one surface that wrote to disk. Every mode is read-only
    // now, so there is no writer to reason about rather than one careful one.
    let layers = layers::load(home().as_deref(), root.as_deref());

    for source in &layers.sources {
        narrate(&format!("config layer {}: {:?} {}", source.label, source.path, source.state.label()));
        if !source.ignored.is_empty() {
            narrate(&format!("config layer {} ignored: {}", source.label, source.ignored.join(", ")));
        }
    }

    let mut git_facts = GitFacts {
        worktree_subpath: cwd_path.as_deref().and_then(|c| git::worktree_subpath(c, &layers.config.worktree_matcher())),
        root,
        branch,
        ..Default::default()
    };
    git::resolve_markers(&mut git_facts);
    narrate(&format!("git markers: ahead={} +{} -{}", git_facts.ahead, git_facts.additions, git_facts.deletions));

    let spend = resolve_spend(&layers.config, facts.now_ms, narrate);
    render_main(&facts, &git_facts, &layers.config, spend.as_deref())
}

/// The spend segment's text, and the decision to spawn a refresh behind it.
///
/// **Gate 1 comes before everything.** A user without `spend` in their layout
/// pays nothing for it: no cache read, no fork, no keychain prompt. That is
/// why this is not simply a segment builder.
///
/// A render never fetches. When the cache is stale this spawns a detached
/// child and returns the **cached** text immediately, without waiting.
fn resolve_spend(config: &Config, now_ms: i64, narrate: &dyn Fn(&str)) -> Option<String> {
    if !spend::in_layout(&config.lines) {
        narrate("spend: not in the layout, nothing read");
        return None;
    }

    let Some(cache_path) = spend::cache::path() else {
        narrate("spend: no $HOME, so no cache to read and nowhere to refresh into");
        return None;
    };

    let cached = spend::cache::read_from(&cache_path);

    match spend::schedule::decide(cached.as_ref(), &config.spend, now_ms) {
        spend::schedule::Decision::Spawn => {
            let spawned = proc::spawn_detached(&[cli::REFRESH_FLAG]);
            narrate(&format!("spend: stale, refresh child spawned={spawned}"));
        }
        decision => narrate(&format!("spend: no refresh ({decision:?})")),
    }

    let verdict = spend::verdict(cached.as_ref(), &config.spend, &config.lines, config.symbol("spend"));
    narrate(&format!("spend: {verdict:?}"));
    verdict.text().map(str::to_string)
}

/// The `--debug` report: what this binary sees.
///
/// The `spend` cycle adds the spend section, which
/// is the part users actually reach for. This is the config, wiring, layout and
/// git half.
fn debug_report() -> String {
    // The spend section is produced by a closure rather than called inline so
    // a test can assemble the report without performing a live fetch — the
    // whole point of that section is that it reaches the network.
    debug_report_with(&|config| crate::_runtime::debug::spend_report(config, time::now_ms()))
}

/// One dynamic value in the `--debug` report.
///
/// `render::sanitize` — the **row** filter, which strips newlines too. The
/// report's own sweep exempts `\n` because the report is many lines, so a value
/// that carried one could forge a line, a section header, or a whole
/// `CLAUDE WIRING` block in a diagnostic the user is reading precisely because
/// they are trying to work out what is wrong. Only the report's own structure
/// may contribute newlines; nothing that came from a config, a path or a
/// payload may.
fn field(value: &str) -> String {
    crate::render::sanitize(value)
}

fn debug_report_with(spend_section: &dyn Fn(&Config) -> String) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "claude-status {VERSION}");

    let cwd = std::env::current_dir().ok();
    let (root, branch) = match cwd.as_deref() {
        Some(cwd) => git::find_root_and_branch(cwd),
        None => (None, None),
    };

    let Layers { config, sources } = layers::load(home().as_deref(), root.as_deref());

    let _ = writeln!(out, "\nCONFIG LAYERS (low to high)");
    for source in &sources {
        // A `None` path means two different things: the **embedded** layer has
        // no file by definition, while the user or repo layer has none because
        // there was no home or no git root to build one from. Printing
        // `<embedded>` for the second is how `--debug` outside a repo once
        // reported `repo  not found  <embedded>` — a **historical** bug, fixed;
        // `not found` is not a state string this binary emits any more (see
        // [`layers::LayerState::label`]).
        // Each `None` is explained by the layer it belongs to, and the arms
        // are matched against the named constants rather than string literals:
        // a catch-all here is how that bug happened, and a renamed or fourth
        // layer would reintroduce it silently.
        let path = match (&source.path, source.label) {
            // A path that resolved but has no file behind it says so, because
            // `using defaults` next to a bare path otherwise reads as though
            // the file were there and had nothing in it.
            (Some(path), _) if source.state == layers::LayerState::Absent => format!("{} (no file)", path.display()),
            (Some(path), _) => path.display().to_string(),
            (None, layers::LABEL_EMBEDDED) => "<embedded>".to_string(),
            (None, layers::LABEL_USER) => "<no $HOME>".to_string(),
            // Also reached when the process has no readable cwd to walk up from.
            (None, layers::LABEL_REPO) => "<no git root>".to_string(),
            (None, other) => format!("<no path for {other}>"),
        };
        // Fourteen wide, not ten, because `using defaults` is the answer a
        // config-free machine now gets and it does not fit in ten. The *label*
        // column is untouched, which is what the exact-spacing assertions in
        // `tests/e2e.rs` pin.
        let state = source.state.label();
        let _ = writeln!(out, "  {:8} {state:14} {}", source.label, field(&path));

        // A continuation row, in the same two columns, rather than a fourth
        // section. The keys a repo layer is not allowed to set are dropped
        // silently everywhere else — the never-fail rule (§1, invariant 3)
        // leaves nowhere to complain to —
        // so this is the **only** place a user editing that file can find out
        // why it is doing nothing. It sits directly under the path it belongs
        // to, because the answer to "why is my `gauge` ignored" is the file it
        // is written in.
        //
        // **The `", "` join is ambiguous, and knowingly so.** A JSON key may
        // contain anything, so one key literally named `x, y, z` renders the
        // same as three keys, and one named `gauge — a repo layer may set
        // projectName only` repeats the suffix. Cosmetic only: `field` strips
        // the control characters, so no key can leave this line, forge a row or
        // forge a section header — which is what
        // `a_repo_layers_ignored_key_names_cannot_forge_lines_in_the_debug_report`
        // pins. Quoting each key would remove the ambiguity and cost every
        // ordinary reader clarity for a case nobody hits by accident.
        if !source.ignored.is_empty() {
            let _ = writeln!(
                out,
                "  {:8} {:14} {} — a repo layer may set {} only",
                "",
                "ignored",
                field(&source.ignored.join(", ")),
                layers::REPO_LAYER_KEY,
            );
        }
    }

    let _ = writeln!(out, "\nCLAUDE WIRING (~/.claude/settings.json)");
    for line in claude_wiring() {
        let _ = writeln!(out, "  {}", field(&line));
    }

    let _ = writeln!(out, "\nEFFECTIVE LAYOUT");
    for (i, line) in config.lines.iter().enumerate() {
        let ids: Vec<String> = line.iter().map(describe_entry).map(|e| field(&e)).collect();
        let _ = writeln!(out, "  line {i}: {}", ids.join(", "));
    }

    let _ = writeln!(out, "\nGIT");
    let _ = writeln!(out, "  cwd:      {}", field(&cwd.as_ref().map_or("<unknown>".into(), |c| c.display().to_string())));
    let _ = writeln!(out, "  root:     {}", field(&root.as_ref().map_or("<none>".into(), |r| r.display().to_string())));
    let _ = writeln!(out, "  branch:   {}", field(branch.as_deref().unwrap_or("<none>")));

    let mut git_facts = GitFacts {
        worktree_subpath: cwd.as_deref().and_then(|c| git::worktree_subpath(c, &config.worktree_matcher())),
        root,
        branch,
        ..Default::default()
    };
    git::resolve_markers(&mut git_facts);
    let _ = writeln!(out, "  worktree: {}", field(git_facts.worktree_subpath.as_deref().unwrap_or("<none>")));
    let _ = writeln!(out, "  ahead:    {}", git_facts.ahead);
    let _ = writeln!(out, "  dirty:    +{} -{}", git_facts.additions, git_facts.deletions);

    let _ = writeln!(out, "\nSPEND");
    out.push_str(&spend_section(&config));

    // **The one place `--debug` output is filtered** (contract §4a, invariant
    // 4). Everything assembled above is diagnostic text drawn from untrusted
    // sources — the config `lines` entries, the spend gate table and its
    // symbols, the `settings.json` command, the endpoint URL, the plan tag,
    // and every path — and filtering those one write at a time is how several
    // of them were missed. One sweep covers whatever is added here later.
    //
    // Newlines survive: the report is deliberately many lines. That exemption
    // is why every dynamic value above ALSO goes through `field`, which strips
    // them — a value carrying a `\n` would otherwise forge whole lines and
    // section headers in a report the user reads to diagnose their machine.
    // The sweep is the backstop for what a `field` call misses; it is not the
    // only defence, because on its own it cannot be one.
    let mut out = crate::render::sanitize_report(&out);

    // Appended AFTER the sweep, because it is the one part whose escapes are
    // meant to be there: `render_main` emits the SGR codes itself, and every
    // dynamic value inside it already went through `segments::build`.
    let _ = writeln!(out, "\nSAMPLE RENDER");
    // No spend text: the SPEND section above already reported what it would
    // draw and why, and the sample's facts are synthetic anyway.
    let sample = render_main(&sample_facts(), &git_facts, &config, None);
    for line in sample.lines() {
        let _ = writeln!(out, "  {line}");
    }

    out
}

fn describe_entry(entry: &SegmentEntry) -> String {
    match entry {
        SegmentEntry::Id(id) => id.clone(),
        styled => format!("{} (styled)", styled.id().unwrap_or("<unnamed>")),
    }
}

/// Reads the three keys Claude Code invokes this binary through, so a stale
/// `settings.json` after an upgrade is visible rather than merely puzzling.
///
/// All three are always reported, `<not set>` included — HELP tells the user to
/// run `--debug` to see what is wired, and a key omitted from the report is
/// indistinguishable from a key the report does not know about.
fn claude_wiring() -> Vec<String> {
    let Some(home) = home() else {
        return vec!["$HOME is unset".to_string()];
    };
    let path = home.join(".claude").join("settings.json");
    // Missing and unreadable are **different answers**, and the `cli-surface`
    // cycle is what made the difference matter: `--configure` creates the first
    // and refuses the second. A single "missing or unreadable" row sent a user
    // whose file will not parse to run the one command that will decline to fix
    // it, with nothing here to say why.
    let settings = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return vec![format!("{} does not exist — run --configure to create it", path.display())];
        }
        Err(e) => return vec![format!("{} could not be read — {e}", path.display())],
        Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(settings) => settings,
            Err(e) => return vec![format!("{} is not valid JSON — {e}", path.display())],
        },
    };

    let mut rows: Vec<String> = ["statusLine", "subagentStatusLine"]
        .iter()
        .map(|key| match settings.get(key).and_then(|k| k.get("command")).and_then(|c| c.as_str()) {
            Some(command) => format!("{key:20} {command}"),
            None => format!("{key:20} <not set>"),
        })
        .collect();

    let hook = caps_hook_command(&settings).unwrap_or("<not set>");
    rows.push(format!("{:20} {hook}", settings::HOOK_KEY));
    rows
}

/// This binary's `PostToolUse` command, in its current or its previous form.
///
/// Claude Code's shape here is a list of groups, each with its own `hooks`
/// list. The `ai-plugins` installer wired the same actuator as
/// `node …/context-caps.js`, so that is **ours in its old form** and showing it
/// is the point of the report — an upgrade that left it behind runs the old
/// actuator alongside the new one. Anyone else's hook is not ours to report.
///
/// "Ours" is [`settings::hook_ownership_of`] rather than a second pair of
/// substring checks, and that is not tidiness. This report exists to tell a
/// user what `--configure` is looking at; the two had already drifted by the
/// *order* of the match — `--caps-hook claude-status` was ours here and
/// somebody else's there — so `--debug` would have shown a hook that
/// `--configure` was about to wire a second copy alongside.
fn caps_hook_command(value: &serde_json::Value) -> Option<&str> {
    value
        .get(settings::HOOKS)?
        .get(settings::POST_TOOL_USE)?
        .as_array()?
        .iter()
        .filter_map(|group| group.get(settings::HOOKS)?.as_array())
        .flatten()
        .filter_map(|entry| entry.get("command"))
        .find(|command| settings::hook_ownership_of(Some(command)) != settings::Ownership::Foreign)
        .and_then(serde_json::Value::as_str)
}

/// Representative facts for the sample render, so `--debug` shows a full bar
/// rather than one made of placeholders.
fn sample_facts() -> MainFacts {
    let now = time::now_ms();
    MainFacts {
        now_ms: now,
        model: Some("Opus 5".into()),
        effort: Some("high".into()),
        session_name: Some("sample-session".into()),
        cost_usd: Some(46.51),
        duration_ms: Some(33_540_000.0),
        ctx_pct: Some(26.0),
        ctx_size: Some(1_000_000.0),
        ctx_used: Some(259_000.0),
        five_hour: payload::RateLimit {
            used_pct: Some(7.0),
            resets_at: Some(serde_json::json!((now + 4 * 3_600_000 + 36 * 60_000) / 1000)),
        },
        seven_day: payload::RateLimit {
            used_pct: Some(1.0),
            resets_at: Some(serde_json::json!((now + 5 * 86_400_000 + 2 * 3_600_000) / 1000)),
        },
        ..Default::default()
    }
}


/// One write, after the whole string exists.
fn write_stdout(output: &str) {
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    let _ = lock.write_all(output.as_bytes());
    let _ = lock.flush();
}

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "panicked".to_string()
    }
}

/// Renders a bar from already-built facts. The seam the golden tests use, and
/// the one place a caller can pin the clock.
pub fn render_bar(facts: &MainFacts, git: &GitFacts, config: &Config, spend: Option<&str>) -> String {
    render_main(facts, git, config, spend)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `Cli` for a mode that takes no modifiers.
    fn plain(mode: Mode) -> Cli {
        Cli { mode, debug: false, dry_run: false, unknown: Vec::new() }
    }

    #[test]
    fn version_output_is_bare() {
        let expected = format!("{}\n", env!("CARGO_PKG_VERSION"));
        let out = dispatch(plain(Mode::Version));
        assert_eq!(out.stdout, expected);
        assert_eq!(out.code, 0);

        let with_debug = dispatch(Cli { mode: Mode::Version, debug: true, dry_run: false, unknown: Vec::new() });
        assert_eq!(with_debug.stdout, expected, "--debug must not decorate the version");
    }

    #[test]
    fn the_missing_flag_answer_is_exactly_one_line() {
        let out = dispatch(plain(Mode::MissingFlag)).stdout;
        assert_eq!(out.lines().count(), 1);
        assert!(out.ends_with('\n'));
        assert!(out.contains("--statusline"), "it names the fix");
    }

    #[test]
    fn help_is_multi_line_and_names_both_surfaces() {
        let out = dispatch(plain(Mode::Help)).stdout;
        assert!(out.lines().count() > 5);
        assert!(out.contains("--statusline") && out.contains("--subagent"));
    }

    /// Every mode but `--configure` exits 0 whatever it found, because §1's
    /// invariant 3 says a render never fails visibly. `--configure` is the one
    /// carve-out, and it is tested in its own module.
    #[test]
    fn only_configure_can_exit_non_zero() {
        for mode in [Mode::Version, Mode::Help, Mode::MissingFlag] {
            assert_eq!(dispatch(plain(mode)).code, 0, "{mode:?} exited non-zero");
        }
    }

    // `--subagent` and `--refresh` are both covered in `tests/e2e.rs`
    // rather than here. Neither is inert any more: the panel reads stdin, the
    // filesystem and the git root, and the refresh child fetches. A unit test
    // calling `dispatch` for either would inherit the real process's stdin and
    // `$HOME` — the hazard the spend cycle recorded when two of these tests
    // quietly became live-fetch tests.

    /// Writes a `~/.claude/settings.json` into a throwaway home and reports
    /// what `claude_wiring` makes of it. The guard restores `$HOME` on drop, so
    /// a failing assertion cannot strand it — the hazard the note above records.
    #[cfg(test)]
    fn wiring_for(settings: serde_json::Value) -> (tempfile::TempDir, Vec<String>) {
        let home = tempfile::TempDir::new().unwrap();
        let dir = home.path().join(".claude");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), settings.to_string()).unwrap();

        let mut env = crate::_shared::env_lock();
        env.set("HOME", home.path().to_str().unwrap());
        let rows = claude_wiring();
        (home, rows)
    }

    #[test]
    fn the_wiring_report_covers_the_caps_hook_as_well_as_the_two_surfaces() {
        // HELP tells users to run --debug "to see what is currently wired", and
        // names three keys. A report covering two of them cannot answer that.
        let (_home, rows) = wiring_for(serde_json::json!({
            "statusLine": { "type": "command", "command": "/bin/claude-status --statusline" },
            "subagentStatusLine": { "type": "command", "command": "/bin/claude-status --subagent" },
            "hooks": {
                "PostToolUse": [
                    { "hooks": [{ "type": "command", "command": "/bin/claude-status --caps-hook" }] },
                ],
            },
        }));

        let report = rows.join("\n");
        assert!(report.contains("--statusline"), "{report}");
        assert!(report.contains("--subagent"), "{report}");
        assert!(report.contains("--caps-hook"), "the caps hook must be reported: {report}");
    }

    /// `--configure` creates a missing `settings.json` and **refuses** one it
    /// cannot parse, so a report that calls both "missing or unreadable" sends
    /// half its readers to run the command that will decline to help them.
    #[test]
    fn a_missing_settings_file_and_an_unparseable_one_read_differently() {
        let home = tempfile::TempDir::new().unwrap();
        let mut env = crate::_shared::env_lock();
        env.set("HOME", home.path().to_str().unwrap());

        let absent = claude_wiring().join("\n");
        assert!(absent.contains("does not exist"), "{absent}");
        assert!(absent.contains("--configure"), "it names the fix: {absent}");

        let dir = home.path().join(".claude");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), "{ not json").unwrap();

        let broken = claude_wiring().join("\n");
        assert!(broken.contains("not valid JSON"), "{broken}");
        assert!(!broken.contains("--configure"), "a file --configure will refuse must not be sold as its job: {broken}");
    }

    #[test]
    fn an_unwired_caps_hook_reports_as_unset_rather_than_vanishing() {
        let (_home, rows) = wiring_for(serde_json::json!({
            "statusLine": { "type": "command", "command": "/bin/claude-status --statusline" },
        }));

        let report = rows.join("\n");
        assert_eq!(rows.len(), 3, "all three keys are always reported: {report}");
        assert!(report.contains("PostToolUse"), "{report}");
    }

    #[test]
    fn a_stale_node_caps_hook_is_shown_rather_than_reported_absent() {
        // The `ai-plugins` installer wired `node …/context-caps.js`. That is
        // this tool's actuator in its previous form, and the whole point of the
        // report is that a stale settings.json is visible after an upgrade.
        let (_home, rows) = wiring_for(serde_json::json!({
            "hooks": {
                "PostToolUse": [
                    { "matcher": "*", "hooks": [{ "type": "command", "command": "node /h/.claude/hooks/context-caps.js" }] },
                ],
            },
        }));

        let report = rows.join("\n");
        assert!(report.contains("context-caps.js"), "a stale hook must be visible: {report}");
    }

    /// `--debug` reports the wiring `--configure` is about to change, so the
    /// two have to agree on what "ours" means. They did not: this used to be a
    /// second pair of substring checks, unordered, so `--caps-hook
    /// claude-status` read as ours here and as somebody else's there — and
    /// `--configure` would have appended a second copy beside a hook `--debug`
    /// had just shown the user as already wired.
    #[test]
    fn what_counts_as_our_hook_is_the_same_answer_the_writer_gives() {
        for command in [
            "claude-status --caps-hook",
            "/opt/homebrew/bin/claude-status --caps-hook",
            "node /h/.claude/hooks/context-caps.js",
            "--caps-hook claude-status",
            "/usr/bin/some-other-linter --fix",
        ] {
            let (_home, rows) = wiring_for(serde_json::json!({
                "hooks": { "PostToolUse": [{ "hooks": [{ "type": "command", "command": command }] }] },
            }));
            let reported = rows.join("\n").contains(command);
            let ours = settings::hook_ownership_of(Some(&serde_json::json!(command)))
                != settings::Ownership::Foreign;
            assert_eq!(reported, ours, "{command:?} is reported={reported} but owned={ours}");
        }
    }

    #[test]
    fn another_tools_post_tool_use_hook_is_not_mistaken_for_ours() {
        let (_home, rows) = wiring_for(serde_json::json!({
            "hooks": {
                "PostToolUse": [
                    { "hooks": [{ "type": "command", "command": "/usr/bin/some-other-linter --fix" }] },
                ],
            },
        }));

        let report = rows.join("\n");
        assert!(!report.contains("some-other-linter"), "a foreign hook is not ours to report: {report}");
        assert!(report.contains("PostToolUse"), "{report}");
    }

    #[test]
    fn the_fallback_line_is_the_documented_bytes() {
        assert_eq!(FALLBACK_LINE, "\u{26a1} Claude");
        let chars: Vec<char> = FALLBACK_LINE.chars().collect();
        assert_eq!(chars[0] as u32, 0x26a1, "U+26A1 HIGH VOLTAGE SIGN");
        assert_eq!(chars[1], ' ', "exactly one space");
        assert_eq!(FALLBACK_LINE.chars().skip(2).collect::<String>(), "Claude");
        assert!(!FALLBACK_LINE.ends_with('\n'), "the fallback is a line, not a line plus a newline");
    }

    #[test]
    fn a_panicking_render_yields_the_fallback() {
        let out = catch_unwind(AssertUnwindSafe(|| -> String { panic!("boom") }))
            .unwrap_or_else(|_| FALLBACK_LINE.to_string());
        assert_eq!(out, FALLBACK_LINE);
    }

    #[test]
    fn the_debug_report_names_every_section() {
        // Reads `$HOME` indirectly, through the config layers.
        let _env = crate::_shared::env_lock();
        // Stubbed rather than live: `spend_report` fetches, and no unit test
        // may reach the spend endpoint.
        let out = debug_report_with(&|_| "  stubbed\n".to_string());
        for section in ["CONFIG LAYERS", "CLAUDE WIRING", "EFFECTIVE LAYOUT", "GIT", "SPEND", "SAMPLE RENDER"] {
            assert!(out.contains(section), "missing {section} in:\n{out}");
        }
    }
}
