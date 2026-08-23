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
use crate::config::Config;
use crate::config::autoseed;
use crate::config::layers::{self, Layers};
use crate::git::GitFacts;
use crate::payload::MainFacts;
use crate::render::main_bar::render_main;
use crate::render::subagent;
use crate::_shared::proc;
use crate::modules::spend;
use crate::modules::caps;
use crate::{cli, git, json, payload, time, usage};

/// What the bar falls back to when a render panics: U+26A1, a space, `Claude`.
const FALLBACK_LINE: &str = "\u{26a1} Claude";

/// The process entry point. Returns the exit code.
pub fn run() -> i32 {
    let cli = cli::parse(std::env::args_os(), std::io::stdin().is_terminal());
    proc::set_narrate(cli.debug);
    let output = dispatch(cli);
    write_stdout(&output);
    0
}

/// Builds the complete stdout payload for one invocation.
fn dispatch(cli: Cli) -> String {
    match cli.mode {
        // Checked first and never decorated: the installer distinguishes an
        // installed binary from a bundled one by the shape of this answer.
        Mode::Version => format!("{VERSION}\n"),
        Mode::Help => HELP.to_string(),
        Mode::MissingFlag => format!("{MISSING_FLAG}\n"),
        Mode::Statusline => render_statusline(cli.debug),
        Mode::Subagent => render_subagent(),
        Mode::RefreshSpend => refresh_spend(),
        Mode::CapsHook => caps_hook(),
        Mode::Debug => debug_report(),
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
    let spend_config = spend::SpendConfig::from_config(&config);
    spend::refresh::run(&path, spend_config.refresh_minutes, time::now_ms(), false);
    String::new()
}

/// Creates the repo config layer, when every condition to do so is met.
///
/// Split out so the three gates read as one list rather than as nested `if`s in
/// the middle of the render: opt-in, in a repo, and the layer genuinely absent.
/// The flag is read from the merged config, which at this point is the embedded
/// and user layers — the repo layer cannot enable its own creation.
fn maybe_seed_repo_config(layers: &Layers, root: Option<&std::path::Path>) -> Option<PathBuf> {
    if !layers.config.auto_configure_repo() {
        return None;
    }
    let repo_layer_loaded =
        layers.sources.iter().any(|s| s.label == layers::LABEL_REPO && s.loaded);
    if repo_layer_loaded {
        return None;
    }
    autoseed::ensure(root?)
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

    let mut layers = layers::load(home().as_deref(), root.as_deref());

    // The repo layer, created rather than merely read — opt-in, and only here.
    // `--subagent` and the caps hook resolve a root too and stay read-only, so
    // there is exactly one writer. Re-reading afterwards costs one file read on
    // the single render that created it, and buys the project name appearing on
    // that render rather than the next.
    if let Some(created) = maybe_seed_repo_config(&layers, root.as_deref()) {
        narrate(&format!("seeded repo config layer: {created:?}"));
        layers = layers::load(home().as_deref(), root.as_deref());
    }

    for source in &layers.sources {
        narrate(&format!("config layer {}: {:?} loaded={}", source.label, source.path, source.loaded));
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
    let lines = config.lines();
    if !spend::in_layout(&lines) {
        narrate("spend: not in the layout, nothing read");
        return None;
    }

    let Some(cache_path) = spend::cache::path() else {
        narrate("spend: no $HOME, so no cache to read and nowhere to refresh into");
        return None;
    };

    let spend_config = spend::SpendConfig::from_config(config);
    let cached = spend::cache::read_from(&cache_path);

    match spend::schedule::decide(cached.as_ref(), &spend_config, now_ms) {
        spend::schedule::Decision::Spawn => {
            let spawned = proc::spawn_detached(&["--refresh-spend"]);
            narrate(&format!("spend: stale, refresh child spawned={spawned}"));
        }
        decision => narrate(&format!("spend: no refresh ({decision:?})")),
    }

    let verdict = spend::verdict(cached.as_ref(), &spend_config, &lines, config.symbol("spend"));
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
        // `<embedded>` for the second is how `--debug` outside a repo came to
        // report `repo  not found  <embedded>`.
        // Each `None` is explained by the layer it belongs to, and the arms
        // are matched against the named constants rather than string literals:
        // a catch-all here is how `repo  not found  <embedded>` happened, and a
        // renamed or fourth layer would reintroduce it silently.
        let path = match (&source.path, source.label) {
            (Some(path), _) => path.display().to_string(),
            (None, layers::LABEL_EMBEDDED) => "<embedded>".to_string(),
            (None, layers::LABEL_USER) => "<no $HOME>".to_string(),
            // Also reached when the process has no readable cwd to walk up from.
            (None, layers::LABEL_REPO) => "<no git root>".to_string(),
            (None, other) => format!("<no path for {other}>"),
        };
        let state = if source.loaded { "loaded" } else { "not found" };
        let _ = writeln!(out, "  {:8} {state:10} {}", source.label, field(&path));
    }

    let _ = writeln!(out, "\nCLAUDE WIRING (~/.claude/settings.json)");
    for line in claude_wiring() {
        let _ = writeln!(out, "  {}", field(&line));
    }

    let _ = writeln!(out, "\nEFFECTIVE LAYOUT");
    for (i, line) in config.lines().iter().enumerate() {
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

fn describe_entry(entry: &serde_json::Value) -> String {
    match entry {
        serde_json::Value::String(id) => id.clone(),
        obj => {
            let id = obj.get("name").or_else(|| obj.get("id")).and_then(|v| v.as_str()).unwrap_or("<unnamed>");
            format!("{id} (styled)")
        }
    }
}

/// Reads the two keys Claude Code invokes this binary through, so a stale
/// `settings.json` after an upgrade is visible rather than merely puzzling.
fn claude_wiring() -> Vec<String> {
    let Some(home) = home() else {
        return vec!["$HOME is unset".to_string()];
    };
    let path = home.join(".claude").join("settings.json");
    let Some(settings) = json::read_json_file(&path) else {
        return vec![format!("{} is missing or unreadable", path.display())];
    };

    ["statusLine", "subagentStatusLine"]
        .iter()
        .map(|key| match settings.get(key).and_then(|k| k.get("command")).and_then(|c| c.as_str()) {
            Some(command) => format!("{key:20} {command}"),
            None => format!("{key:20} <not set>"),
        })
        .collect()
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

    #[test]
    fn version_output_is_bare() {
        let expected = format!("{}\n", env!("CARGO_PKG_VERSION"));
        let out = dispatch(Cli { mode: Mode::Version, debug: false });
        assert_eq!(out, expected);

        let with_debug = dispatch(Cli { mode: Mode::Version, debug: true });
        assert_eq!(with_debug, expected, "--debug must not decorate the version");
    }

    #[test]
    fn the_missing_flag_answer_is_exactly_one_line() {
        let out = dispatch(Cli { mode: Mode::MissingFlag, debug: false });
        assert_eq!(out.lines().count(), 1);
        assert!(out.ends_with('\n'));
        assert!(out.contains("--statusline"), "it names the fix");
    }

    #[test]
    fn help_is_multi_line_and_names_both_surfaces() {
        let out = dispatch(Cli { mode: Mode::Help, debug: false });
        assert!(out.lines().count() > 5);
        assert!(out.contains("--statusline") && out.contains("--subagent"));
    }

    // `--subagent` and `--refresh-spend` are both covered in `tests/e2e.rs`
    // rather than here. Neither is inert any more: the panel reads stdin, the
    // filesystem and the git root, and the refresh child fetches. A unit test
    // calling `dispatch` for either would inherit the real process's stdin and
    // `$HOME` — the hazard the spend cycle recorded when two of these tests
    // quietly became live-fetch tests.

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
