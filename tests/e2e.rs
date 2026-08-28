//! End-to-end: the built binary as a subprocess with a fake `$HOME`, which is
//! how Claude Code invokes it.
//!
//! Every case captures the two streams **separately** and checks that stdout
//! holds only the bar. That is the invariant most likely to regress and least
//! likely to be noticed.
//!
//! # The spend hazard
//!
//! The macOS keychain is **not** scoped by `$HOME`, so a fake home does not
//! stop a real credential read or a real network call — and a stray one is both
//! a privacy leak and a 429 the user wears for half an hour. Every invocation
//! here neutralises it four ways at once: `spend.refreshMinutes = 0` in the
//! seeded config, `CLAUDE_STATUS_SPEND_URL` pointed at a closed port, a
//! pre-seeded fresh cache, and a seeded `.claude/.credentials.json` so the
//! keychain fallback is never reached.
//!
//! The fourth was added when `--refresh` and `--doctor` stopped being
//! inert: the first three were written while nothing in this binary could
//! fetch, and a fake home alone was never enough once something could.
//!
//! The **fifth** is `CLAUDE_STATUS_SPEND_CACHE`, pinned into the fake home
//! rather than left to `$HOME` redirection alone. `cache::path()` falls back to
//! `paths::home()`, which reads the process environment — so a future in-process
//! caller would land on the developer's real cache with nothing to stop it. The
//! endpoint has had a two-deep guard since the spend cycle; the cache had one,
//! and that asymmetry is the shape of the unexplained write that cycle
//! recorded and could not attribute.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_claude-status");

/// Port 1 is reserved and nothing listens on it.
const CLOSED_PORT_URL: &str = "http://127.0.0.1:1/never";

/// What the binary prints when it recognised no surface flag — the tell that a
/// test thought it was exercising a mode and was not.
const MISSING_FLAG_LINE: &str = "missing --statusline or --subagent";

/// The reference main-bar payload, read from the file rather than transcribed.
///
/// It used to be a `const` here **and** a shell example in the retired
/// behaviour contract, and the two drifted: that copy documented piping to a
/// bare `claude-status`,
/// which prints the missing-flag line instead of a bar. One file, read by the
/// suite that proves it works — see `tests/fixtures/README.md`.
const FIXTURE: &str = include_str!("fixtures/main-bar.json");

struct Home {
    dir: TempDir,
}

impl Home {
    /// A throwaway `$HOME` carrying a config layer and a fresh spend cache.
    fn new(config: &str) -> Self {
        let dir = TempDir::new().unwrap();
        // `~/.config/claude-status/config.json` — a directory, not the bare
        // `~/.config/claude-status.json` this used to be. There is no fallback
        // to the old path, so a fixture left at it would silently test the
        // embedded defaults instead of the config it seeded.
        let config_dir = dir.path().join(".config").join("claude-status");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(config_dir.join("config.json"), config).unwrap();

        // A cache stamped now, so nothing could decide it needs refreshing.
        let cache_dir = dir.path().join(".cache").join("claude-status");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        std::fs::write(cache_dir.join("spend.json"), format!(r#"{{"ts":{now},"spend":null}}"#)).unwrap();

        // The fourth neutralisation: the keychain is not scoped by `$HOME`, so
        // a home with no credentials file falls through to the real token.
        let claude = dir.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(
            claude.join(".credentials.json"),
            r#"{"claudeAiOauth":{"accessToken":"e2e-stub-token","subscriptionType":"team"}}"#,
        )
        .unwrap();

        Self { dir }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }
}

/// Runs the binary with stdin piped, returning both streams separately.
fn run(home: &Home, args: &[&str], stdin: &str, extra_env: &[(&str, &str)]) -> Output {
    run_in(args, stdin, Some(home.path()), None, extra_env)
}

/// A throwaway directory git will accept as a repo root, carrying `layer` as
/// its `<root>/.config/claude-status.json`.
///
/// A bare `.git/HEAD` rather than `git init`: the root walk looks for the
/// directory, and a real init is a subprocess per test for nothing.
fn fake_repo(layer: &str) -> TempDir {
    let repo = TempDir::new().unwrap();
    std::fs::create_dir_all(repo.path().join(".config")).unwrap();
    std::fs::write(repo.path().join(".config").join("claude-status.json"), layer).unwrap();
    std::fs::create_dir_all(repo.path().join(".git")).unwrap();
    std::fs::write(repo.path().join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
    repo
}

/// The shipped defaults, with spend refresh disabled.
fn safe_config() -> String {
    r#"{ "projectName": "e2e-fixture", "spend": { "refreshMinutes": 0, "show": "never" } }"#.to_string()
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("stdout is UTF-8")
}

fn stderr(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).expect("stderr is UTF-8")
}

#[test]
fn the_fixture_renders_a_bar_on_stdout_and_nothing_on_stderr() {
    let home = Home::new(&safe_config());
    let out = run(&home, &["--statusline"], FIXTURE, &[]);

    assert!(out.status.success(), "exit code {:?}", out.status.code());
    let bar = stdout(&out);
    assert!(bar.contains("Opus 4.8"), "got: {}", bar.escape_debug());
    assert!(bar.contains("e2e-fixture"), "the seeded config layer applied");
    assert!(bar.contains('\u{1b}'), "the bar carries ANSI colour");
    assert!(!bar.ends_with('\n'), "no trailing newline");
    assert_eq!(stderr(&out), "", "a clean render says nothing");
}

/// **The invocation §12 got wrong, pinned as a control.**
///
/// §12's main-bar example piped this payload to a bare `claude-status`, which
/// resolves to the missing-flag mode and prints an error line rather than a
/// bar. The example was wrong for as long as it existed and nothing could tell
/// you: the document was the only place the invocation was written down.
///
/// This is the control for the assertion above. Without it,
/// `the_fixture_renders_a_bar_on_stdout_and_nothing_on_stderr` proves the
/// payload renders but not that the **flag** is what makes it render — and the
/// flag is the half that was documented wrong.
#[test]
fn the_reference_payload_without_its_flag_is_the_missing_flag_error_and_not_a_bar() {
    let home = Home::new(&safe_config());
    let out = run(&home, &[], FIXTURE, &[]);

    let stdout = stdout(&out);
    assert!(stdout.contains(MISSING_FLAG_LINE), "got: {}", stdout.escape_debug());
    assert!(!stdout.contains("Opus 4.8"), "a bar was rendered without the flag: {}", stdout.escape_debug());
    assert_eq!(stdout.lines().count(), 1, "one line fits the bar; twenty lines of usage do not");
}

/// Every reference payload has a documented invocation.
///
/// The failure this closes is the one that produced the retired contract's
/// broken example: a
/// payload and the command that runs it lived in different places, so one could
/// be changed without the other. A file added to `tests/fixtures/` and never
/// written into its README is a payload nobody knows how to run.
#[test]
fn every_reference_payload_is_named_in_the_fixture_readme() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures");
    let readme = std::fs::read_to_string(dir.join("README.md")).expect("tests/fixtures/README.md exists");

    let mut payloads: Vec<String> = std::fs::read_dir(&dir)
        .expect("the fixture directory exists")
        .filter_map(|entry| entry.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .filter(|name| name.ends_with(".json"))
        .collect();
    payloads.sort();

    // A scan of nothing passes, so say what the floor is. Two payloads today:
    // the main bar and the subagent panel.
    assert!(payloads.len() >= 2, "only {} payload(s) found — the scan would be vacuous", payloads.len());

    let undocumented: Vec<&String> = payloads.iter().filter(|name| !readme.contains(name.as_str())).collect();
    assert!(undocumented.is_empty(), "payloads with no row in tests/fixtures/README.md: {undocumented:?}");
}

/// The reference subagent payload, read from the file for the same reason.
const SUBAGENT_FIXTURE: &str = include_str!("fixtures/subagent.json");

#[test]
fn the_subagent_fixture_renders_ndjson_that_survives_a_json_parser() {
    let home = Home::new(&safe_config());
    let out = run(&home, &["--subagent"], SUBAGENT_FIXTURE, &[]);

    assert!(out.status.success(), "exit code {:?}", out.status.code());
    let panel = stdout(&out);
    assert_eq!(panel.lines().count(), 1);
    assert!(!panel.ends_with('\n'), "no trailing newline");
    assert_eq!(stderr(&out), "", "a clean panel says nothing");

    // What `jq -r .content` does, which is how `tests/fixtures/README.md`
    // says to read it.
    let row: serde_json::Value = serde_json::from_str(&panel).expect("each line is a JSON object");
    let content = row["content"].as_str().expect("content is a string");
    assert_eq!(row["id"], "t1");
    assert!(content.contains("reviewer"), "got: {}", content.escape_debug());
    assert!(content.contains("Auditing auth flow"));
    assert!(content.contains("18k"));
    assert!(content.contains('\u{1b}'), "the row carries ANSI colour");
}

#[test]
fn a_subagent_payload_with_no_tasks_renders_nothing_and_never_the_main_bar() {
    let home = Home::new(&safe_config());
    // The same fixture the main bar renders — under `--subagent` it must not.
    for payload in ["{}", FIXTURE, r#"{"tasks":[]}"#] {
        let out = run(&home, &["--subagent"], payload, &[]);
        assert!(out.status.success());
        assert_eq!(stdout(&out), "", "payload {payload:.20} produced output");
    }
}

#[test]
fn a_subagent_task_without_an_id_is_skipped_and_its_siblings_still_render() {
    let home = Home::new(&safe_config());
    let payload = r#"{"tasks":[{"name":"orphan","status":"done"},{"id":"kept","name":"sibling","status":"done"}]}"#;
    let panel = stdout(&run(&home, &["--subagent"], payload, &[]));

    assert_eq!(panel.lines().count(), 1, "got: {}", panel.escape_debug());
    assert!(panel.contains("sibling"));
    assert!(!panel.contains("orphan"));
}

#[test]
fn a_generic_type_appears_as_a_glyph_and_never_as_text() {
    let home = Home::new(&safe_config());
    let payload = r#"{"tasks":[{"id":1,"type":"local_agent","status":"running"}]}"#;
    let panel = stdout(&run(&home, &["--subagent"], payload, &[]));

    assert!(!panel.contains("local_agent"), "the raw type leaked: {}", panel.escape_debug());
    assert!(panel.contains('\u{f109}'), "the local_agent glyph is there: {}", panel.escape_debug());
}

#[test]
fn a_subagent_render_writes_no_usage_mirror_and_leaves_the_spend_cache_alone() {
    let home = Home::new(&safe_config());
    let usage_dir = home.path().join("usage-mirror");
    std::fs::create_dir_all(&usage_dir).unwrap();
    let cache = home.path().join(".cache").join("claude-status").join("spend.json");
    let before = std::fs::read(&cache).unwrap();

    let payload = r#"{"session_id":"abc123","tasks":[{"id":1,"status":"running"}]}"#;
    let out = run(&home, &["--subagent"], payload, &[("AI_PLUGINS_USAGE_DIR", usage_dir.to_str().unwrap())]);

    assert!(out.status.success());
    assert_eq!(std::fs::read_dir(&usage_dir).unwrap().count(), 0, "the mirror is the main bar's job");
    assert_eq!(std::fs::read(&cache).unwrap(), before, "the panel never writes the spend cache");
    // The structural guarantee is stronger than this assertion can be: the
    // panel's pipeline never calls the spend resolver at all, so there is no
    // gate to pass and no child to spawn.
    assert!(!panel_mentions_spend(&stdout(&out)), "the panel renders no spend segment");
}

fn panel_mentions_spend(panel: &str) -> bool {
    panel.contains('\u{f09d}')
}

/// A usage mirror the caps hook will read, written where the hook looks.
fn seed_mirror(dir: &Path, session_id: &str, body: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join(format!("{session_id}.json")), body).unwrap();
}

fn caps_run(home: &Home, usage_dir: &Path, stdin: &str, var: &str) -> Output {
    run(home, &["--caps-hook"], stdin, &[(var, usage_dir.to_str().unwrap())])
}

#[test]
fn the_caps_hook_emits_one_directive_when_the_seven_day_cap_is_breached() {
    let home = Home::new(&safe_config());
    let usage = home.path().join("usage");
    seed_mirror(&usage, "s1", r#"{"sevenDayPct":85,"sevenDayResetsAt":1774600000}"#);

    let out = caps_run(&home, &usage, r#"{"session_id":"s1"}"#, "CLAUDE_STATUS_USAGE_DIR");
    assert!(out.status.success());

    let emitted: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("one JSON object");
    let ctx = emitted["hookSpecificOutput"]["additionalContext"].as_str().unwrap();
    assert_eq!(emitted["hookSpecificOutput"]["hookEventName"], "PostToolUse");
    assert!(ctx.contains("7-DAY LIMIT CAP") && ctx.contains("85%"), "{ctx}");
    assert!(ctx.contains("vwf:handoff") && ctx.contains("/vwf:recall next"), "{ctx}");
    assert!(!ctx.contains("docs/handoffs/"), "the stale path from the JS: {ctx}");
}

#[test]
fn the_more_severe_of_two_breaches_is_the_one_reported() {
    let home = Home::new(&safe_config());
    let usage = home.path().join("usage");
    seed_mirror(&usage, "s1", r#"{"ctxPct":99,"sevenDayPct":85}"#);

    let panel = stdout(&caps_run(&home, &usage, r#"{"session_id":"s1"}"#, "CLAUDE_STATUS_USAGE_DIR"));
    assert!(panel.contains("7-DAY"), "{panel}");
    assert!(!panel.contains("CONTEXT CAP"), "the lesser breach is not mentioned");
}

#[test]
fn the_directive_is_debounced_until_the_breach_escalates() {
    let home = Home::new(&safe_config());
    let usage = home.path().join("usage");
    seed_mirror(&usage, "s1", r#"{"ctxPct":70}"#);
    let stdin = r#"{"session_id":"s1"}"#;

    let first = stdout(&caps_run(&home, &usage, stdin, "CLAUDE_STATUS_USAGE_DIR"));
    assert!(first.contains("CONTEXT CAP"), "the first breach fires");
    let second = stdout(&caps_run(&home, &usage, stdin, "CLAUDE_STATUS_USAGE_DIR"));
    assert_eq!(second, "", "the same level does not fire again");

    // Escalating to the 7-day cap fires a second time.
    seed_mirror(&usage, "s1", r#"{"ctxPct":70,"sevenDayPct":85}"#);
    let third = stdout(&caps_run(&home, &usage, stdin, "CLAUDE_STATUS_USAGE_DIR"));
    assert!(third.contains("7-DAY"), "an escalation fires: {third}");

    // De-escalating back to context does not re-fire.
    seed_mirror(&usage, "s1", r#"{"ctxPct":70}"#);
    assert_eq!(stdout(&caps_run(&home, &usage, stdin, "CLAUDE_STATUS_USAGE_DIR")), "");
}

#[test]
fn the_debounce_state_file_is_the_name_the_js_hook_also_writes() {
    // Both hooks are installed during the transition. A different name here
    // would let a machine running both double-fire.
    let home = Home::new(&safe_config());
    let usage = home.path().join("usage");
    seed_mirror(&usage, "s1", r#"{"ctxPct":70}"#);
    caps_run(&home, &usage, r#"{"session_id":"s1"}"#, "CLAUDE_STATUS_USAGE_DIR");

    let state = usage.join("s1.state.json");
    assert!(state.exists(), "the state file is <sid>.state.json beside <sid>.json");
    let recorded: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&state).unwrap()).unwrap();
    assert_eq!(recorded["level"], 1);
    assert!(recorded["ts"].is_number());
}

#[test]
fn a_corrupt_state_file_is_treated_as_no_previous_breach() {
    let home = Home::new(&safe_config());
    let usage = home.path().join("usage");
    seed_mirror(&usage, "s1", r#"{"ctxPct":70}"#);
    std::fs::write(usage.join("s1.state.json"), "{not json").unwrap();

    let out = stdout(&caps_run(&home, &usage, r#"{"session_id":"s1"}"#, "CLAUDE_STATUS_USAGE_DIR"));
    assert!(out.contains("CONTEXT CAP"), "a corrupt state file must not suppress the cap");
}

#[test]
fn a_user_config_may_raise_a_cap_as_well_as_lower_it() {
    // The tighten-only rule is gone: caps are ordinary config now, so the user
    // layer may move a cap in either direction.
    let raised = Home::new(r#"{ "projectName": "e2e", "spend": { "refreshMinutes": 0, "show": "never" }, "caps": { "context": 90 } }"#);
    let usage = raised.path().join("usage");
    seed_mirror(&usage, "s1", r#"{"ctxPct":70}"#);
    assert_eq!(
        stdout(&caps_run(&raised, &usage, r#"{"session_id":"s1"}"#, "CLAUDE_STATUS_USAGE_DIR")),
        "",
        "70% is under a raised cap of 90, so nothing fires",
    );

    let lowered = Home::new(r#"{ "projectName": "e2e", "spend": { "refreshMinutes": 0, "show": "never" }, "caps": { "context": 50 } }"#);
    let usage = lowered.path().join("usage");
    seed_mirror(&usage, "s1", r#"{"ctxPct":55}"#);
    let out = stdout(&caps_run(&lowered, &usage, r#"{"session_id":"s1"}"#, "CLAUDE_STATUS_USAGE_DIR"));
    assert!(out.contains("cap 50%"), "the user cap applied: {out}");
}

/// Replaces `a_repo_config_overrides_the_user_one_outright`, whose capability
/// the `config-relocation` cycle removed. It is **inverted rather than
/// deleted**: the two tests together were the whole guard on the repo layer's
/// width, and dropping one would leave the narrowing untested at the surface
/// where it matters most.
///
/// The **caps hook** is the sharpest case, and the reason this is an e2e test
/// rather than a unit one. A repo raising its own cap does not draw a bar
/// oddly — it suppresses the halt directive that stops an agent running past
/// its context budget, which is exactly the thing a file inside the repo should
/// not decide on the user's behalf.
#[test]
fn a_repo_config_can_no_longer_override_a_user_cap() {
    let home = Home::new(r#"{ "projectName": "e2e", "spend": { "refreshMinutes": 0, "show": "never" }, "caps": { "context": 50 } }"#);
    let usage = home.path().join("usage");
    seed_mirror(&usage, "s1", r#"{"ctxPct":70}"#);

    // A repo, which git has to recognise for layer 3 to be read at all.
    let repo = home.path().join("repo");
    std::fs::create_dir_all(repo.join(".config")).unwrap();
    std::process::Command::new("git").args(["init", "-q"]).current_dir(&repo).status().unwrap();
    let stdin = format!(r#"{{"session_id":"s1","cwd":"{}"}}"#, repo.display());

    // The user cap of 50 is what fires before the repo says anything.
    let user = stdout(&caps_run(&home, &usage, &stdin, "CLAUDE_STATUS_USAGE_DIR"));
    assert!(user.contains("cap 50%"), "the user cap applied: {user}");

    // The repo tries to raise it to 90. It used to win outright; it is not
    // merged at all now, so the user's 50 still fires on the same 70%.
    std::fs::remove_file(usage.join("s1.state.json")).unwrap();
    std::fs::write(repo.join(".config").join("claude-status.json"), r#"{"caps":{"context":90}}"#).unwrap();
    let after = stdout(&caps_run(&home, &usage, &stdin, "CLAUDE_STATUS_USAGE_DIR"));
    assert!(after.contains("cap 50%"), "the repo raised a cap it may no longer touch: {after}");
}

/// Criterion 5, whole: the name applies, the other key does not, and `--doctor`
/// names the one it dropped.
#[test]
fn a_repo_config_sets_the_project_name_and_nothing_else() {
    let home = Home::new(&safe_config());
    let repo = fake_repo(r#"{ "projectName": "from-the-repo", "gauge": { "width": 3 } }"#);

    // `--statusline` resolves the repo root from the **payload's** cwd, not the
    // process's, so `FIXTURE`'s `/tmp/demo` would find no repo at all.
    let payload = serde_json::json!({
        "model": { "display_name": "Opus 4.8" },
        "workspace": { "current_dir": repo.path() },
        "context_window": { "used_percentage": 26, "context_window_size": 1_000_000 },
    })
    .to_string();

    let bar = stdout(&run_in(&["--statusline"], &payload, Some(home.path()), Some(repo.path()), &[]));
    assert!(bar.contains("from-the-repo"), "the one key it may set did not apply: {}", bar.escape_debug());
    let cells = bar.chars().filter(|c| *c == '\u{25b0}' || *c == '\u{25b1}').count();
    assert_eq!(cells, 10, "the repo widened the gauge it may not touch: {}", bar.escape_debug());

    // And the user is told, because `--doctor` is the only place they can find
    // out why the file they wrote is doing nothing.
    let report = stdout(&run_in(&["--doctor"], "", Some(home.path()), Some(repo.path()), &[]));
    let line = report
        .lines()
        .find(|l| l.trim_start().starts_with("ignored"))
        .unwrap_or_else(|| panic!("--doctor never named the ignored key:\n{report}"));
    assert!(line.contains("gauge"), "{line:?}");
    assert!(line.contains("projectName"), "it says what the file IS allowed to set: {line:?}");
    assert!(!line.contains("$schema"), "a `$schema` pointer is not an ignored setting: {line:?}");
}

/// A repo config carrying only what it may set says nothing extra — otherwise
/// every correctly written file would put a line in `--doctor`.
#[test]
fn a_well_formed_repo_config_is_reported_as_ignoring_nothing() {
    let home = Home::new(&safe_config());
    let repo = fake_repo(r#"{ "$schema": "https://example.invalid/s.json", "projectName": "tidy" }"#);

    let report = stdout(&run_in(&["--doctor"], "", Some(home.path()), Some(repo.path()), &[]));
    assert!(report.contains("repo     loaded"), "the repo layer never loaded:\n{report}");
    assert!(!report.contains("ignored"), "a tidy repo config was reported as dropping something:\n{report}");
}

#[test]
fn a_vwf_yaml_cap_block_is_no_longer_read() {
    // vwf.yaml was the only source of repo caps and is not consulted any more.
    let home = Home::new(&safe_config());
    let usage = home.path().join("usage");
    seed_mirror(&usage, "s1", r#"{"ctxPct":55}"#);

    let repo = home.path().join("repo");
    std::fs::create_dir_all(repo.join(".config")).unwrap();
    std::process::Command::new("git").args(["init", "-q"]).current_dir(&repo).status().unwrap();
    std::fs::write(repo.join(".config").join("vwf.yaml"), "pipeline:\n  execute_caps:\n    context: 50\n").unwrap();

    let stdin = format!(r#"{{"session_id":"s1","cwd":"{}"}}"#, repo.display());
    assert_eq!(
        stdout(&caps_run(&home, &usage, &stdin, "CLAUDE_STATUS_USAGE_DIR")),
        "",
        "55% is under the shipped 65 — the vwf.yaml cap of 50 must be ignored",
    );
}

#[test]
fn a_bad_key_elsewhere_in_the_config_cannot_move_a_cap() {
    // The hook is an **actuator**, not a render, so a config that degrades
    // wrongly here does not draw a bar oddly — it injects a halt directive into
    // the agent's loop at the wrong moment. A configured 80 that silently fell
    // back to the shipped 65 would fire fifteen points early, and nothing on
    // either stream would say why.
    let home = Home::new(r#"{ "caps": { "context": 80 }, "symbols": { "model": 5 }, "gauge": null }"#);
    let usage = home.path().join("usage");
    seed_mirror(&usage, "s1", r#"{"ctxPct":70}"#);

    let out = caps_run(&home, &usage, r#"{"session_id":"s1"}"#, "CLAUDE_STATUS_USAGE_DIR");
    assert!(out.status.success());
    assert_eq!(stdout(&out), "", "70% is under the configured cap of 80, so the hook stays silent");

    // And the cap it is honouring really is the configured one, not the shipped
    // 65 that a discarded config would have handed back.
    seed_mirror(&usage, "s2", r#"{"ctxPct":82}"#);
    let breached = caps_run(&home, &usage, r#"{"session_id":"s2"}"#, "CLAUDE_STATUS_USAGE_DIR");
    let emitted: serde_json::Value = serde_json::from_str(&stdout(&breached)).expect("one JSON object");
    let ctx = emitted["hookSpecificOutput"]["additionalContext"].as_str().unwrap();
    assert!(ctx.contains("82%"), "{ctx}");
}

#[test]
fn the_caps_hook_is_completely_silent_when_it_has_nothing_to_read() {
    let home = Home::new(&safe_config());
    let usage = home.path().join("usage");
    seed_mirror(&usage, "s1", r#"{"ctxPct":99}"#);

    // No usage dir at all.
    let bare = run(&home, &["--caps-hook"], r#"{"session_id":"s1"}"#, &[]);
    assert_eq!(stdout(&bare), "", "no usage dir");
    assert!(bare.status.success());

    // A usage dir but no session id.
    assert_eq!(stdout(&caps_run(&home, &usage, "{}", "CLAUDE_STATUS_USAGE_DIR")), "", "no session_id");

    // A session with no mirror written yet — the bar has not rendered.
    assert_eq!(stdout(&caps_run(&home, &usage, r#"{"session_id":"unseen"}"#, "CLAUDE_STATUS_USAGE_DIR")), "");

    // Unparseable stdin.
    assert_eq!(stdout(&caps_run(&home, &usage, "not json", "CLAUDE_STATUS_USAGE_DIR")), "");
}

#[test]
fn an_iso_reset_timestamp_degrades_to_soon_rather_than_parsing() {
    let home = Home::new(&safe_config());
    let usage = home.path().join("usage");
    seed_mirror(&usage, "s1", r#"{"sevenDayPct":85,"sevenDayResetsAt":"2026-08-21T00:00:00Z"}"#);

    let out = stdout(&caps_run(&home, &usage, r#"{"session_id":"s1"}"#, "CLAUDE_STATUS_USAGE_DIR"));
    assert!(out.contains("resets in soon"), "matches the JS numeric-only coercion: {out}");
}

#[test]
fn the_usage_dir_variable_migrates_without_breaking_the_old_name() {
    let home = Home::new(&safe_config());
    let usage = home.path().join("usage");
    seed_mirror(&usage, "s1", r#"{"ctxPct":70}"#);

    // The legacy name alone still works — a machine still running the JS hook
    // exports only this one.
    let legacy = stdout(&caps_run(&home, &usage, r#"{"session_id":"s1"}"#, "AI_PLUGINS_USAGE_DIR"));
    assert!(legacy.contains("CONTEXT CAP"), "the old name is still honoured: {legacy}");

    // With both set, the new name wins.
    let other = home.path().join("elsewhere");
    seed_mirror(&other, "s2", r#"{"sevenDayPct":85}"#);
    let out = run(&home, &["--caps-hook"], r#"{"session_id":"s2"}"#, &[
        ("CLAUDE_STATUS_USAGE_DIR", other.to_str().unwrap()),
        ("AI_PLUGINS_USAGE_DIR", usage.to_str().unwrap()),
    ]);
    assert!(stdout(&out).contains("7-DAY"), "the new name won: {}", stdout(&out));
}

#[test]
fn version_is_exactly_the_version_with_or_without_debug() {
    let home = Home::new(&safe_config());
    for args in [&["--version"][..], &["--version", "--doctor"]] {
        let out = run(&home, args, "", &[]);
        assert_eq!(stdout(&out), format!("{}\n", env!("CARGO_PKG_VERSION")), "{args:?}");
        assert_eq!(stderr(&out), "", "{args:?} must not narrate");
    }
}

/// A throwaway `$HOME` for `--configure`, optionally carrying a settings file.
///
/// Deliberately **not** [`Home`]: that seeds a config, a spend cache and a
/// credentials file, and every `--configure` case below turns on what is and is
/// not already there.
fn configure_home(settings: Option<&str>) -> TempDir {
    let home = TempDir::new().unwrap();
    if let Some(body) = settings {
        let dir = home.path().join(".claude");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), body).unwrap();
    }
    home
}

fn settings_of(home: &TempDir) -> serde_json::Value {
    let text = std::fs::read_to_string(home.path().join(".claude").join("settings.json")).expect("it is there");
    serde_json::from_str(&text).expect("it is JSON")
}

/// **Criteria 1 and 2**, through the binary rather than through the merge.
///
/// The three keys are set, every other key keeps its value, and another tool's
/// `PostToolUse` group keeps its matcher and its hook.
///
/// "Byte-identical" (criterion 1's word) is **not** what is asserted, and
/// cannot be: this writes 2-space pretty JSON with a trailing newline, so a
/// file indented any other way is reformatted even where no value moved. The
/// TypeScript this replaces had the same property. What is deliverable, and
/// what a user actually cares about, is that no *value* changed.
#[test]
fn configure_sets_the_three_keys_and_preserves_everything_else() {
    let home = configure_home(Some(
        &serde_json::json!({
            "model": "opus",
            "permissions": { "allow": ["Bash(ls:*)"] },
            "hooks": { "PostToolUse": [
                { "matcher": "Edit|Write", "hooks": [{ "type": "command", "command": "/usr/bin/fmt" }] },
            ] },
        })
        .to_string(),
    ));

    let out = run_in(&["--configure"], "", Some(home.path()), None, &[]);
    assert!(out.status.success(), "exit code {:?}, stderr: {}", out.status.code(), stderr(&out));

    let after = settings_of(&home);
    assert_eq!(after["statusLine"]["command"], "claude-status --statusline");
    assert_eq!(after["statusLine"]["refreshInterval"], 4, "the bar's redraw cadence");
    assert_eq!(after["statusLine"]["padding"], 0);
    assert_eq!(after["subagentStatusLine"]["command"], "claude-status --subagent");
    assert_eq!(after["model"], "opus", "an unrelated key was altered");
    assert_eq!(after["permissions"], serde_json::json!({ "allow": ["Bash(ls:*)"] }));

    let groups = after["hooks"]["PostToolUse"].as_array().unwrap();
    assert_eq!(groups[0]["matcher"], "Edit|Write", "their matcher was rewritten");
    assert_eq!(groups[0]["hooks"][0]["command"], "/usr/bin/fmt", "their hook was replaced");
    assert_eq!(groups[1]["hooks"][0]["command"], "claude-status --caps-hook");
}

/// **Criterion 3**, over three runs, because a hook list that grows by one
/// entry per run takes three runs to notice.
#[test]
fn configure_is_idempotent_over_three_runs() {
    let home = configure_home(Some(r#"{"model":"opus"}"#));
    let path = home.path().join(".claude").join("settings.json");

    let mut bytes = Vec::new();
    for run_number in 1..=3 {
        let out = run_in(&["--configure"], "", Some(home.path()), None, &[]);
        assert!(out.status.success(), "run {run_number} exited {:?}", out.status.code());
        bytes.push(std::fs::read(&path).unwrap());
    }

    assert_eq!(bytes[0], bytes[1], "the second run changed the file");
    assert_eq!(bytes[1], bytes[2], "the third run changed the file");
    assert_eq!(settings_of(&home)["hooks"]["PostToolUse"].as_array().unwrap().len(), 1, "the hook list grew");
}

/// **An already-wired file is not rewritten — it is not opened for writing at
/// all.**
///
/// `Wiring::changed()` calls this "what makes idempotence structural rather
/// than incidental", and it was untested at this level: forcing it to `true`
/// makes `--configure` rewrite byte-identically on every run, so **both
/// three-run tests stay green** — identical values serialise to identical
/// bytes. Only a file whose formatting is *not* ours can tell the difference.
///
/// So the fixture is deliberately hostile to a rewrite: four-space indent, keys
/// in the user's own order, our three keys already correct. A rewrite would
/// reflow all of it — normalising indentation in a file this tool does not own,
/// on a run that had nothing to do.
#[test]
fn an_already_wired_file_is_left_byte_identical_and_says_so() {
    let body = concat!(
        "{\n",
        "    \"statusLine\": {\n",
        "        \"type\": \"command\",\n",
        "        \"command\": \"claude-status --statusline\",\n",
        "        \"padding\": 0,\n",
        "        \"refreshInterval\": 4\n",
        "    },\n",
        "    \"subagentStatusLine\": { \"type\": \"command\", \"command\": \"claude-status --subagent\" },\n",
        "    \"hooks\": { \"PostToolUse\": [ { \"hooks\": [ { \"type\": \"command\", ",
        "\"command\": \"claude-status --caps-hook\" } ] } ] },\n",
        "    \"model\": \"opus\"\n",
        "}\n",
    );
    let home = configure_home(Some(body));
    // The user config too, so the whole of `$HOME` is expected to be untouched
    // rather than "untouched apart from the seed" — which would leave the
    // strongest assertion below unavailable.
    let config = home.path().join(".config").join("claude-status").join("config.json");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(&config, "{}\n").unwrap();
    let before = snapshot(home.path());

    let out = run_in(&["--configure"], "", Some(home.path()), None, &[]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));

    assert_eq!(snapshot(home.path()), before, "a run with nothing to do still wrote something");
    assert_eq!(
        std::fs::read_to_string(home.path().join(".claude").join("settings.json")).unwrap(),
        body,
        "the user's own formatting was normalised on a run with nothing to do",
    );
    // The line that says so is asserted nowhere else in the suite.
    assert!(stdout(&out).contains("nothing to change"), "{}", stdout(&out));
    assert!(stdout(&out).contains("already wired"), "{}", stdout(&out));
}

/// **Criterion 4.** Replaced, and said so — the printing is the entire
/// mitigation for a flag that overwrites with no undo.
#[test]
fn configure_replaces_a_foreign_status_line_and_says_what_it_replaced() {
    let home = configure_home(Some(
        r#"{ "statusLine": { "type": "command", "command": "starship prompt --right" } }"#,
    ));

    let out = run_in(&["--configure"], "", Some(home.path()), None, &[]);
    assert!(out.status.success());
    assert_eq!(settings_of(&home)["statusLine"]["command"], "claude-status --statusline");

    // The value came out of the user's file, so it is quoted back on stderr —
    // the one stream this binary sends untrusted content to, and every write
    // to it goes through the `_shared::diag` chokepoint.
    let said = stderr(&out);
    assert!(said.contains("starship prompt --right"), "the user was not told what they lost: {said}");
    assert!(stdout(&out).contains("REPLACED"), "and the report marks the key: {}", stdout(&out));
}

/// A previous install of ours is rewritten **quietly**. The tempting rule —
/// "warn whenever the value differs" — shouts at every upgrading user about
/// their own last install.
#[test]
fn configure_rewrites_a_stale_command_of_ours_without_a_warning() {
    let home = configure_home(Some(
        r#"{ "statusLine": { "type": "command", "command": "/h/.claude/bin/claude-status" } }"#,
    ));

    let out = run_in(&["--configure"], "", Some(home.path()), None, &[]);
    assert!(out.status.success());
    assert_eq!(settings_of(&home)["statusLine"]["command"], "claude-status --statusline");
    assert_eq!(stderr(&out), "", "a previous install of ours is not a warning: {}", stderr(&out));
}

/// **Criterion 5.** A dry run prints a plan marked as a plan, and touches
/// nothing on disk.
#[test]
fn configure_dry_run_prints_and_writes_nothing() {
    let body = r#"{"model":"opus"}"#;
    let home = configure_home(Some(body));
    let before = snapshot(home.path());

    let out = run_in(&["--configure", "--dry-run"], "", Some(home.path()), None, &[]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("would write"), "a dry run must be distinguishable: {}", stdout(&out));
    assert_eq!(snapshot(home.path()), before, "a dry run wrote to disk");
}

/// **`--doctor` is a modifier on `--configure` too**, and a modifier must not
/// change stdout by a single byte.
///
/// `--configure` is the first mode that could break that claim: it is the only
/// one that writes, and the only one that can exit non-zero. Two *separate*
/// throwaway homes seeded identically, because running `--configure` twice
/// against one home is not a control — the first run changes the state the
/// second one reports, so the outputs would differ for a reason that has
/// nothing to do with `--doctor`. Both paths are checked: the ordinary one and a
/// refusal.
#[test]
fn debug_is_a_modifier_on_configure_and_never_changes_its_stdout() {
    for settings in [
        Some(r#"{"model":"opus","statusLine":{"type":"command","command":"starship prompt"}}"#),
        // A refusal: nothing on stdout either way, and the same exit code.
        Some(r#"{ "model": "opus",, }"#),
        None,
    ] {
        let plain_home = configure_home(settings);
        let debug_home = configure_home(settings);

        let plain = run_in(&["--configure"], "", Some(plain_home.path()), None, &[]);
        let debug = run_in(&["--configure", "--doctor"], "", Some(debug_home.path()), None, &[]);

        // Byte-identical, not merely equivalent — the paths in the report are
        // tilde-rendered, so two different homes produce the same bytes and any
        // difference is `--doctor`'s doing.
        assert_eq!(plain.stdout, debug.stdout, "--doctor changed stdout for {settings:?}");
        assert_eq!(plain.status.code(), debug.status.code(), "--doctor changed the exit code for {settings:?}");
        // And the file each one produced is the same too: a modifier that
        // changed what was *written* while leaving stdout alone would satisfy
        // the assertion above and still be a bug.
        assert_eq!(snapshot(plain_home.path()), snapshot(debug_home.path()), "--doctor changed what was written");
    }
}

/// **A typo in `--dry-run` must not perform a real write.**
///
/// The parser ignores unrecognised arguments, which is right for a render — a
/// stray token must never cost a bar. On the one flag that *writes*, with no
/// receipt and no undo, that same silence turns one mistyped character into an
/// unrecoverable overwrite of a file this tool does not own, while the user
/// believed they had asked for a preview.
#[test]
fn configure_refuses_an_unrecognised_argument_rather_than_writing_anyway() {
    for typo in ["--dry-runn", "--dryrun", "-n", "--force"] {
        let body = r#"{"model":"opus"}"#;
        let home = configure_home(Some(body));
        let before = snapshot(home.path());

        let out = run_in(&["--configure", typo], "", Some(home.path()), None, &[]);
        assert_eq!(out.status.code(), Some(1), "{typo} was accepted");
        assert_eq!(snapshot(home.path()), before, "{typo} performed a real write");
        assert!(stderr(&out).contains(typo), "the refusal must name what it did not understand: {}", stderr(&out));
        assert!(stderr(&out).contains("--help"), "and where to look: {}", stderr(&out));
    }
}

/// The asymmetry is deliberate: the render surfaces keep ignoring what they do
/// not recognise, because Claude Code invokes those and invariant 3 (see
/// `src/lib.rs`) says a render never fails visibly. Only the writing flag is
/// strict.
#[test]
fn a_render_surface_still_ignores_an_unrecognised_argument() {
    let home = Home::new(&safe_config());
    let out = run(&home, &["--statusline", "--dry-runn"], FIXTURE, &[]);

    assert!(out.status.success(), "a typo cost the user their bar");
    assert!(stdout(&out).contains("Opus 4.8"), "got: {}", stdout(&out).escape_debug());
}

/// **The `ours-stale` path, end to end** — the case the four-state ownership
/// model exists for.
///
/// `ai-plugins` wired this actuator as `node …/context-caps.js`. Collapse the
/// four states to two and that command reads as *foreign*: it stays where it
/// is, ours is appended beside it, and the caps actuator **fires twice on every
/// tool call**. Nothing about that is visible in the output of either run.
#[test]
fn a_legacy_node_caps_hook_is_replaced_rather_than_joined() {
    let home = configure_home(Some(
        &serde_json::json!({
            "hooks": { "PostToolUse": [
                { "matcher": "*", "hooks": [{ "type": "command", "command": "node /h/.claude/hooks/context-caps.js" }] },
            ] },
        })
        .to_string(),
    ));

    let out = run_in(&["--configure"], "", Some(home.path()), None, &[]);
    assert!(out.status.success());

    let groups = settings_of(&home)["hooks"]["PostToolUse"].as_array().unwrap().clone();
    assert_eq!(groups.len(), 1, "the actuator would now fire twice per tool call: {groups:?}");
    assert_eq!(groups[0]["hooks"][0]["command"], "claude-status --caps-hook");
    assert_eq!(groups[0]["matcher"], "*", "their matcher was rewritten");
    assert_eq!(stderr(&out), "", "a previous install of ours is rewritten quietly: {}", stderr(&out));
}

/// **The most dangerous behaviour in the TypeScript this replaces.**
///
/// Its `readSettings` — in the installer `distribution/01` deleted, so there is
/// no file left to cite — parsed inside a
/// bare `catch { return null }` and fell back to `{}`, so a single stray comma
/// in a user's `settings.json` cost them their **entire** Claude Code
/// configuration on the next install. Absent and corrupt are different things
/// and only one of them is safe to write over.
#[test]
fn a_malformed_settings_file_is_refused_with_a_non_zero_exit_and_nothing_written() {
    let body = r#"{ "model": "opus", "permissions": { "allow": [] },, }"#;
    let home = configure_home(Some(body));
    let before = snapshot(home.path());

    let out = run_in(&["--configure"], "", Some(home.path()), None, &[]);
    assert_eq!(out.status.code(), Some(1), "a file this tool cannot read must not be one it writes");
    assert_eq!(stdout(&out), "", "and it must not report work it did not do");
    assert!(stderr(&out).contains("settings.json"), "the refusal names the file: {}", stderr(&out));
    assert_eq!(snapshot(home.path()), before, "the refusal still changed something on disk");
}

#[test]
fn a_settings_shape_the_merge_cannot_read_is_refused_rather_than_overwritten() {
    for body in [r#"[1,2,3]"#, r#"{ "hooks": "all" }"#, r#"{ "hooks": { "PostToolUse": {} } }"#] {
        let home = configure_home(Some(body));
        let before = snapshot(home.path());

        let out = run_in(&["--configure"], "", Some(home.path()), None, &[]);
        assert_eq!(out.status.code(), Some(1), "{body} was wired rather than refused");
        assert_eq!(snapshot(home.path()), before, "{body} was overwritten");
    }
}

/// The seeded **global** config must not pick up the name of whatever
/// repository `--configure` happened to be run in.
///
/// `layers::load` merges the repo layer's `projectName` into the `Config` it
/// returns, so seeding from the *loaded* config rather than `Config::default()`
/// would pin one repo's name into `~/.config/claude-status/config.json`
/// permanently — where it would then override the name of every other repo the
/// user ever opens. The two implementations differ by one identifier and only
/// this test can tell them apart.
#[test]
fn configuring_inside_a_repo_does_not_pin_that_repos_name_into_the_global_config() {
    let home = configure_home(None);
    let repo = fake_repo(r#"{ "projectName": "the-repo-i-happened-to-be-in" }"#);

    let out = run_in(&["--configure"], "", Some(home.path()), Some(repo.path()), &[]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));

    let seeded = std::fs::read_to_string(home.path().join(".config").join("claude-status").join("config.json"))
        .expect("the config was seeded");
    assert!(
        !seeded.contains("the-repo-i-happened-to-be-in"),
        "a repo's name was written into the user's global config: {seeded}",
    );
    assert!(!seeded.contains("projectName"), "the global config must carry no project name at all: {seeded}");
}

/// A `~/.claude/settings.json` symlinked into a dotfiles repo is **followed**,
/// not replaced.
///
/// The write is temp-then-rename, and a rename over a symlink swaps the link
/// for a regular file — so without resolving first, a dotfiles user's real
/// settings file would be orphaned and their settings would appear to revert on
/// their next sync. Nothing about that failure is visible at the time.
#[test]
fn a_symlinked_settings_file_is_written_through_rather_than_replaced() {
    let home = configure_home(None);
    let store = TempDir::new().unwrap();
    let real = store.path().join("settings.json");
    std::fs::write(&real, r#"{"model":"opus"}"#).unwrap();

    let link = home.path().join(".claude").join("settings.json");
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let out = run_in(&["--configure"], "", Some(home.path()), None, &[]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));

    assert!(link.symlink_metadata().unwrap().is_symlink(), "the symlink was replaced by a regular file");
    let written: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&real).unwrap()).unwrap();
    assert_eq!(written["statusLine"]["command"], "claude-status --statusline", "the real file was not written");
    assert_eq!(written["model"], "opus");
    assert!(stdout(&out).contains("symlink"), "the user is told which file changed: {}", stdout(&out));
}

/// **A known limitation, pinned so it cannot be traded away by accident.**
///
/// A hardlink is a second *name* for an inode, not a pointer to a path, so
/// there is nothing to follow the way the symlink case above follows one. The
/// atomic replace gives `~/.claude/settings.json` a new inode and the other
/// name keeps the old contents.
///
/// This test exists to pin the **atomicity**, of which the stale link is the
/// accepted cost: the only way to keep a hardlink in step is to truncate and
/// rewrite in place, and a `settings.json` seen half-written breaks Claude Code
/// outright. A stale second name is worth much less than that risk — and unlike
/// the symlink case, the file Claude Code actually reads is correct afterwards.
/// If this test ever goes red, check what was given up to make it.
#[test]
fn a_hardlinked_settings_file_goes_stale_because_the_write_is_atomic() {
    let home = configure_home(None);
    let store = TempDir::new().unwrap();
    let other_name = store.path().join("settings.json");
    std::fs::write(&other_name, r#"{"model":"opus"}"#).unwrap();

    let wired = home.path().join(".claude").join("settings.json");
    std::fs::create_dir_all(wired.parent().unwrap()).unwrap();
    std::fs::hard_link(&other_name, &wired).unwrap();

    let out = run_in(&["--configure"], "", Some(home.path()), None, &[]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));

    // The file Claude Code reads is correct — this is not data loss.
    let after: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&wired).unwrap()).unwrap();
    assert_eq!(after["statusLine"]["command"], "claude-status --statusline");
    assert_eq!(after["model"], "opus", "the user's own keys survived");

    // The other name kept the old inode, and so the old bytes.
    assert_eq!(
        std::fs::read_to_string(&other_name).unwrap(),
        r#"{"model":"opus"}"#,
        "the write was performed in place — atomicity was given up to keep the hardlink in step",
    );
}

/// **F1, at the surface.** A tool whose name merely *contains* ours is not
/// ours, and destroying one silently is the worst outcome this flag has.
///
/// The module doc calls the stderr warning "the entire mitigation" for having
/// no undo — so a value that is overwritten without it has bypassed the whole
/// design, not just a nicety. All four rows must warn.
#[test]
fn a_similarly_named_tools_status_line_is_replaced_only_with_a_warning() {
    for command in [
        "starship prompt",
        "claude-statusline",
        "/opt/claude-statusbar --statusline",
        "claude-status-pro --statusline --theme dark",
    ] {
        let home = configure_home(Some(
            &serde_json::json!({ "statusLine": { "type": "command", "command": command } }).to_string(),
        ));
        let out = run_in(&["--configure"], "", Some(home.path()), None, &[]);
        assert!(out.status.success(), "stderr: {}", stderr(&out));

        assert!(stderr(&out).contains(command), "{command:?} was destroyed silently: {:?}", stderr(&out));
        assert!(stdout(&out).contains("REPLACED"), "{command:?} was not reported as a replacement");
        assert_eq!(settings_of(&home)["statusLine"]["command"], "claude-status --statusline");
    }
}

/// The hook side of F1, where the consequence is **deletion** rather than
/// replacement. `ai-plugins` only ever wrote its `context-caps.js` under
/// `.claude/hooks/`, so a script of that name anywhere else belongs to someone
/// else and must survive untouched.
#[test]
fn another_projects_context_caps_hook_is_not_deleted() {
    let theirs = "node /work/vendor/context-caps.js --lint";
    let home = configure_home(Some(
        &serde_json::json!({
            "hooks": { "PostToolUse": [{ "matcher": "Edit", "hooks": [{ "type": "command", "command": theirs }] }] },
        })
        .to_string(),
    ));

    let out = run_in(&["--configure"], "", Some(home.path()), None, &[]);
    assert!(out.status.success());

    let groups = settings_of(&home)["hooks"]["PostToolUse"].as_array().unwrap().clone();
    let commands: Vec<String> = groups
        .iter()
        .filter_map(|g| g["hooks"].as_array())
        .flatten()
        .filter_map(|e| e["command"].as_str().map(str::to_string))
        .collect();
    assert!(commands.iter().any(|c| c == theirs), "another project's hook was deleted: {commands:?}");
    assert!(commands.iter().any(|c| c == "claude-status --caps-hook"), "and ours was not added: {commands:?}");
}

/// **F3, by the reviewer's own probe.** `PostToolUse` is iterated, so two
/// entries of ours means two invocations per tool call — and `--caps-hook`
/// output goes verbatim into the agent's context.
#[test]
fn only_one_caps_hook_entry_survives_however_many_were_there() {
    let home = configure_home(Some(
        &serde_json::json!({
            "hooks": { "PostToolUse": [
                { "hooks": [
                    { "type": "command", "command": "claude-status --caps-hook" },
                    { "type": "command", "command": "node /h/.claude/hooks/context-caps.js" },
                ] },
                { "hooks": [{ "type": "command", "command": "/opt/homebrew/bin/claude-status --caps-hook" }] },
            ] },
        })
        .to_string(),
    ));

    assert!(run_in(&["--configure"], "", Some(home.path()), None, &[]).status.success());

    let groups = settings_of(&home)["hooks"]["PostToolUse"].as_array().unwrap().clone();
    let ours: Vec<&str> = groups
        .iter()
        .filter_map(|g| g["hooks"].as_array())
        .flatten()
        .filter_map(|e| e["command"].as_str())
        .filter(|c| c.contains("caps-hook") || c.contains("context-caps"))
        .collect();
    assert_eq!(ours.len(), 1, "the actuator would fire {} times per tool call: {ours:?}", ours.len());
    assert_eq!(ours[0], "claude-status --caps-hook");
}

/// **F6.** A symlink whose target is temporarily missing — dotfiles not cloned,
/// `stow` not run, volume unmounted — is the exact state the symlink handling
/// exists for, and the one it could not see: `canonicalize` fails, the fallback
/// writes to the link's own path, and the rename destroys the link.
#[test]
fn a_symlink_with_a_missing_target_is_refused_rather_than_destroyed() {
    let home = configure_home(None);
    let link = home.path().join(".claude").join("settings.json");
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink("/nonexistent/dotfiles/settings.json", &link).unwrap();

    let out = run_in(&["--configure"], "", Some(home.path()), None, &[]);
    assert_eq!(out.status.code(), Some(1), "a dangling symlink was treated as an absent file");
    assert!(stderr(&out).contains("target is missing"), "the reason must be nameable: {}", stderr(&out));

    let after = link.symlink_metadata().unwrap();
    assert!(after.file_type().is_symlink(), "the symlink was replaced by a regular file");
    assert_eq!(std::fs::read_link(&link).unwrap(), Path::new("/nonexistent/dotfiles/settings.json"));
}

/// **Step 3's rule applied to the one consequence a user cannot discover.**
///
/// *"Because there is no receipt and no undo, the destructive case must be
/// visible."* A stale hard link qualifies twice over: no undo, and — unlike an
/// overwritten `statusLine`, which gets quoted back — nothing anywhere would
/// otherwise say it happened.
///
/// The negative half is what keeps it worth reading. A warning that fired on
/// ordinary files, or on the symlinks this tool handles correctly, would be
/// noise on every run and would be tuned out long before it mattered.
#[test]
fn a_write_that_breaks_a_hard_link_says_so_and_only_then() {
    let store = TempDir::new().unwrap();
    let marker = "hard link";

    // Fires: a second name for the inode about to be replaced.
    let linked = configure_home(None);
    let wired = linked.path().join(".claude").join("settings.json");
    std::fs::create_dir_all(wired.parent().unwrap()).unwrap();
    let other_name = store.path().join("hardlinked.json");
    std::fs::write(&other_name, r#"{"model":"opus"}"#).unwrap();
    std::fs::hard_link(&other_name, &wired).unwrap();

    let out = run_in(&["--configure"], "", Some(linked.path()), None, &[]);
    let said = stderr(&out);
    assert!(said.contains(marker), "a broken hard link went unmentioned: {said:?}");
    assert!(said.contains("stop tracking"), "it must say what actually happens: {said:?}");
    // Information, not a refusal: the write is correct and the user asked for it.
    assert!(out.status.success(), "the warning blocked the write: {:?}", out.status.code());
    assert_eq!(settings_of(&linked)["statusLine"]["command"], "claude-status --statusline");

    // Silent: an ordinary file. The seeded settings carry no foreign statusLine,
    // so stderr has nothing else to say and can be asserted empty outright.
    let plain = configure_home(Some(r#"{"model":"opus"}"#));
    let out = run_in(&["--configure"], "", Some(plain.path()), None, &[]);
    assert_eq!(stderr(&out), "", "an ordinary file warned about hard links");

    // Silent: a symlink. This tool resolves and writes *through* one, so the
    // link survives — warning here would report a break that did not happen.
    let symlinked = configure_home(None);
    let link = symlinked.path().join(".claude").join("settings.json");
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    let real = store.path().join("symlinked.json");
    std::fs::write(&real, r#"{"model":"opus"}"#).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let out = run_in(&["--configure"], "", Some(symlinked.path()), None, &[]);
    assert!(!stderr(&out).contains(marker), "a symlink was reported as a broken hard link: {:?}", stderr(&out));

    // `--dry-run` gets it too, in the `would` form: a preview that stayed quiet
    // about this would be silent on the only thing it is uniquely good for.
    let dry = configure_home(None);
    let dry_wired = dry.path().join(".claude").join("settings.json");
    std::fs::create_dir_all(dry_wired.parent().unwrap()).unwrap();
    let dry_other = store.path().join("dry.json");
    std::fs::write(&dry_other, r#"{"model":"opus"}"#).unwrap();
    std::fs::hard_link(&dry_other, &dry_wired).unwrap();

    let out = run_in(&["--configure", "--dry-run"], "", Some(dry.path()), None, &[]);
    let said = stderr(&out);
    assert!(said.contains(marker) && said.contains("would stop"), "a dry run hid the warning: {said:?}");
    assert_eq!(std::fs::read_to_string(&dry_other).unwrap(), r#"{"model":"opus"}"#, "the dry run wrote anyway");
}

/// A read-only `settings.json` is replaced anyway — `rename` needs write
/// permission on the *directory*, not on the file — and the mode is then
/// restored, so the run is otherwise indistinguishable from an ordinary one.
///
/// The mode is honoured; the intent behind it is not. Same treatment as the
/// hard-link case, for the same reason: it is a consequence the output would
/// otherwise hide completely. Not a refusal — the user typed `--configure`,
/// which is a clearer statement of intent than a mode bit set some time ago.
#[test]
fn a_read_only_settings_file_is_rewritten_and_says_so() {
    use std::os::unix::fs::PermissionsExt;

    let home = configure_home(Some(r#"{"model":"opus"}"#));
    let path = home.path().join(".claude").join("settings.json");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o400)).unwrap();

    let out = run_in(&["--configure"], "", Some(home.path()), None, &[]);
    assert!(out.status.success(), "a read-only file is not a refusal: {:?}", out.status.code());
    assert!(stderr(&out).contains("read-only"), "the user was not told: {:?}", stderr(&out));

    assert_eq!(settings_of(&home)["statusLine"]["command"], "claude-status --statusline");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o400, "the mode itself was not preserved: {mode:o}");

    // A writable file must stay silent, or the warning becomes noise on every run.
    let ordinary = configure_home(Some(r#"{"model":"opus"}"#));
    let out = run_in(&["--configure"], "", Some(ordinary.path()), None, &[]);
    assert!(!stderr(&out).contains("read-only"), "an ordinary file warned: {:?}", stderr(&out));
}

/// `settings.json` can carry an `env` block with credentials in it, and a fresh
/// `fs::write` is 0644 minus the umask — so a file the user had tightened would
/// come back world-readable.
#[test]
fn writing_settings_keeps_the_permissions_it_had() {
    use std::os::unix::fs::PermissionsExt;

    let home = configure_home(Some(r#"{"model":"opus"}"#));
    let path = home.path().join(".claude").join("settings.json");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

    assert!(run_in(&["--configure"], "", Some(home.path()), None, &[]).status.success());
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "the file was widened to {mode:o}");
}

/// **F4 and F5.** Preserving the mode *afterwards* is not the same as never
/// having a wider one.
///
/// `fs::write` creates at 0644 on a default account, so a chmod after the
/// rename leaves the user's `env` block — credentials included — readable by
/// every account on the machine for the length of the write, and the real path
/// briefly readable after it. A signal in either window strands a
/// world-readable copy forever, because nothing sweeps a `*.tmp`.
///
/// The temp file is the observable: it is a sibling of the target, so scanning
/// the directory during the write is what proves no wider mode ever existed.
/// Here the run has finished, so the assertion is the stronger one — **no temp
/// survives at all, and nothing in that directory is group- or world-readable.**
#[test]
fn no_intermediate_file_is_left_wider_than_the_settings_it_holds() {
    use std::os::unix::fs::PermissionsExt;

    let home = configure_home(Some(
        &serde_json::json!({ "model": "opus", "env": { "SECRET_API_KEY": "sk-must-not-leak" } }).to_string(),
    ));
    let dir = home.path().join(".claude");
    std::fs::set_permissions(dir.join("settings.json"), std::fs::Permissions::from_mode(0o600)).unwrap();

    assert!(run_in(&["--configure"], "", Some(home.path()), None, &[]).status.success());

    for entry in std::fs::read_dir(&dir).unwrap().filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        assert!(!name.ends_with(".tmp"), "a temp file survived the write: {name}");
        if entry.path().is_file() {
            let mode = entry.metadata().unwrap().permissions().mode() & 0o077;
            assert_eq!(mode, 0, "{name} is readable beyond its owner: {:o}", mode);
        }
    }
    // And the secret really was in play — otherwise the above proves nothing.
    assert_eq!(settings_of(&home)["env"]["SECRET_API_KEY"], "sk-must-not-leak");
}

/// **Criterion 6.** Seeded only when there is none, and never touched again.
#[test]
fn configure_seeds_a_schema_only_user_config_and_leaves_an_existing_one_alone() {
    let fresh = configure_home(None);
    assert!(run_in(&["--configure"], "", Some(fresh.path()), None, &[]).status.success());

    let path = fresh.path().join(".config").join("claude-status").join("config.json");
    let seeded = std::fs::read_to_string(&path).expect("the config was seeded");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&seeded).unwrap(),
        serde_json::json!({ "$schema": "https://raw.githubusercontent.com/virajp/claude-status/main/schemas/claude-status.schema.json" }),
        "the seed must be a pointer and nothing else — the npm installer seeded the whole asset and froze it",
    );

    // Byte-identical, and deliberately not what this tool would have written.
    let kept = configure_home(None);
    let existing = kept.path().join(".config").join("claude-status").join("config.json");
    std::fs::create_dir_all(existing.parent().unwrap()).unwrap();
    let body = "{\n\t\"projectName\": \"mine\"\n}";
    std::fs::write(&existing, body).unwrap();

    assert!(run_in(&["--configure"], "", Some(kept.path()), None, &[]).status.success());
    assert_eq!(std::fs::read_to_string(&existing).unwrap(), body, "an existing config was touched");
}

/// A write that could not happen is a **failure**, not a note in the report.
///
/// A read-only `$HOME`, a full disk or a permissions problem all land here, and
/// a setup script that saw exit 0 would carry on and tell the user their bar
/// was wired. The report says so in words either way; the exit code is the part
/// a script can act on.
#[test]
fn a_write_that_fails_reports_it_and_exits_non_zero() {
    use std::os::unix::fs::PermissionsExt;

    let home = configure_home(Some(r#"{"model":"opus"}"#));
    let dir = home.path().join(".claude");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();

    let out = run_in(&["--configure"], "", Some(home.path()), None, &[]);
    // Restore before asserting, so a failure cannot leave the TempDir
    // undeletable and turn one red test into a stranded directory.
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();

    assert_eq!(out.status.code(), Some(1), "a failed write reported success");
    assert!(stdout(&out).contains("FAILED"), "and the report is silent about it: {}", stdout(&out));
    assert!(!stderr(&out).is_empty(), "nothing on stderr said why");
}

/// With no `$HOME` there is no `~/.claude/settings.json` to wire, and a
/// relative `.claude/` would wire Claude Code to whatever directory this
/// happened to be run from. It refuses instead — the same rule as a corrupt
/// file, for the same reason.
#[test]
fn with_no_home_configure_refuses_rather_than_writing_somewhere_relative() {
    let dir = TempDir::new().unwrap();
    let marker = dir.path().join("only-this-should-be-here");
    std::fs::write(&marker, "").unwrap();

    let out = run_without_home(&["--configure"], "", dir.path(), &[]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), "");
    assert!(stderr(&out).contains("$HOME"), "it names what is missing: {}", stderr(&out));

    let after: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(after, vec![marker.file_name().unwrap()], "something was written: {after:?}");
}

/// The wiring `--configure` writes is the wiring `--doctor` reads back — the two
/// halves of the same contract, and the only place either is checked against
/// the other.
#[test]
fn what_configure_writes_is_what_doctor_reports_as_wired() {
    let home = configure_home(None);
    assert!(run_in(&["--configure"], "", Some(home.path()), None, &[]).status.success());

    let report = stdout(&run_in(&["--doctor"], "", Some(home.path()), None, &[]));
    let wiring = report.split("CLAUDE WIRING").nth(1).expect("the section is present");
    let wiring = wiring.split("\nEFFECTIVE LAYOUT").next().expect("split always yields one");
    assert!(wiring.contains("claude-status --statusline"), "{wiring}");
    assert!(wiring.contains("claude-status --subagent"), "{wiring}");
    assert!(wiring.contains("claude-status --caps-hook"), "{wiring}");
    assert!(!wiring.contains("<not set>"), "a key --configure just wrote reads as unset: {wiring}");
}

#[test]
fn debug_is_a_modifier_that_never_changes_stdout() {
    let home = Home::new(&safe_config());
    let plain = run(&home, &["--statusline"], FIXTURE, &[]);
    let debug = run(&home, &["--statusline", "--doctor"], FIXTURE, &[]);

    assert_eq!(plain.stdout, debug.stdout, "stdout must be byte-identical");
    assert!(!stderr(&debug).is_empty(), "the narration went to stderr");
    assert!(stderr(&plain).is_empty());
}

#[test]
fn no_flag_with_piped_stdin_prints_exactly_one_diagnostic_line() {
    // What a stale settings.json produces after an upgrade. One line fits the
    // bar and names the fix; a blank bar would leave no clue.
    let home = Home::new(&safe_config());
    let out = run(&home, &[], FIXTURE, &[]);

    let text = stdout(&out);
    assert_eq!(text.lines().count(), 1, "got: {}", text.escape_debug());
    assert!(text.contains("--statusline") && text.contains("--subagent"));
    assert!(text.contains("--help"), "it points at the fix");
    assert!(out.status.success());
}

/// **Criterion 7, inverted**, at the surface. `cli.rs` pins `HELP`'s contents;
/// this pins that the binary actually prints them, which is the claim a user
/// makes when they run it.
///
/// The criterion said `--help` was the binary's only documentation, so a vague
/// one was the feature being gone. The website is that documentation now, and
/// `--help` is the index that points at it — so where this test once asserted
/// a floor of forty lines and five sections, it asserts a **ceiling**. The
/// concern did not change; what satisfies it did.
#[test]
fn help_is_a_short_index_that_points_at_the_website() {
    let home = Home::new(&safe_config());
    let out = run(&home, &["--help"], "", &[]);
    let text = stdout(&out);

    assert!(text.lines().count() < 30, "help grew back to {} lines:\n{text}", text.lines().count());
    // Structure, not just keywords: a blob containing every substring below and
    // nothing else passes a `contains`-only test while being useless as help.
    for section in ["USAGE:", "MODIFIERS:", "MORE:"] {
        assert!(text.contains(section), "the {section} section is gone:\n{text}");
    }
    for flag in ["--configure", "--doctor", "--refresh", "--version", "--dry-run"] {
        assert!(text.contains(flag), "{flag} is undocumented:\n{text}");
    }
    // The wired surfaces are absent on purpose — Claude Code calls them.
    for wired in ["--statusline", "--subagent", "--caps-hook"] {
        assert!(!text.contains(wired), "{wired} is back in the help a user reads:\n{text}");
    }
    assert!(text.contains("https://claude-status.virajp.dev"), "everything cut needs somewhere to have gone:\n{text}");
    assert!(out.status.success());
}

#[test]
fn debug_alone_reports_layers_wiring_layout_and_git() {
    let home = Home::new(&safe_config());
    let out = run(&home, &["--doctor"], "", &[]);
    let text = stdout(&out);

    for section in ["CONFIG LAYERS", "CLAUDE WIRING", "EFFECTIVE LAYOUT", "GIT", "SAMPLE RENDER"] {
        assert!(text.contains(section), "missing {section}");
    }
    assert!(
        text.contains(".config/claude-status/config.json"),
        "it names the user config path it looked at: {text}",
    );
    assert!(out.status.success());

    // The `SAMPLE RENDER` carve-out, which is load-bearing: that
    // section is appended **after** the report-wide sweep precisely so its SGR
    // codes survive. Asserting the header alone would pass with an empty body,
    // and would still pass if the sweep were moved to cover it — which would
    // silently strip the colours the section exists to show.
    let sample = text.split("SAMPLE RENDER").nth(1).expect("the section is present");
    assert!(sample.contains('\u{1b}'), "the sample render lost its colour: {}", sample.escape_debug());
}

/// **Criterion 8.** A machine with no config file anywhere is described as
/// working, because after `config-relocation` it *is* — the defaults are
/// embedded and no file has to exist.
///
/// The report used to say `not found` for it, which reads as a half-installed
/// machine. It said the same thing for a config that would not parse, which is
/// the case the word was actually needed for, so the two were indistinguishable
/// in the one place a user goes to tell them apart. Both halves are asserted
/// here: the absent case must read as normal **and** the broken case must not.
#[test]
fn debug_calls_a_config_free_machine_normal_and_a_broken_config_not() {
    let bare = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let clean = stdout(&run_in(&["--doctor"], "", Some(bare.path()), Some(cwd.path()), &[]));
    let layers = clean.split("\nCLAUDE WIRING").next().expect("split always yields one");

    assert!(layers.contains("user     using defaults"), "an absent user config reads as broken:\n{layers}");
    assert!(layers.contains("repo     using defaults"), "an absent repo config reads as broken:\n{layers}");
    assert!(layers.contains("(no file)"), "it says the path it looked at had nothing behind it:\n{layers}");
    assert!(!layers.contains("not found"), "the config-free state is still called an absence:\n{layers}");
    assert!(!layers.contains("UNREADABLE"), "nothing here is unreadable:\n{layers}");
    assert!(clean.contains("embedded loaded"), "and the layer actually in use is named:\n{clean}");

    // The distinction the word exists for. `Home::new` seeds a valid config, so
    // this one is planted by hand.
    let broken = TempDir::new().unwrap();
    let path = broken.path().join(".config").join("claude-status").join("config.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{ this is not json").unwrap();

    let report = stdout(&run_in(&["--doctor"], "", Some(broken.path()), Some(cwd.path()), &[]));
    let layers = report.split("\nCLAUDE WIRING").next().expect("split always yields one");
    assert!(layers.contains("user     UNREADABLE"), "a config that will not parse reads as fine:\n{layers}");
    assert!(!layers.contains("user     using defaults"), "{layers}");
}

#[test]
fn a_malformed_payload_renders_a_normal_bar_not_the_fallback() {
    let home = Home::new(&safe_config());
    for payload in ["", "not json at all", "[1,2,3]", "null", "{\"model\":12345}"] {
        let out = run(&home, &["--statusline"], payload, &[]);
        let bar = stdout(&out);
        assert!(bar.contains("Claude"), "{payload:?} should fall back to the model name, got: {}", bar.escape_debug());
        assert!(!bar.starts_with('\u{26a1}'), "{payload:?} must not trigger the panic fallback");
        assert!(out.status.success());
    }
}

#[test]
fn a_syntactically_invalid_config_layer_is_ignored_and_the_render_succeeds() {
    let home = Home::new("{ this is not json");
    let out = run(&home, &["--statusline"], FIXTURE, &[]);

    let bar = stdout(&out);
    assert!(bar.contains("Opus 4.8"), "the render succeeded on the embedded layer");
    // The gauge is the probe here, not `projectName`: that key is repo-level
    // only and ships in no layer this test has.
    assert!(bar.contains('\u{25b0}'), "the embedded gauge glyph came through");
    assert!(out.status.success());
}

#[test]
fn an_unknown_segment_warns_on_stderr_omits_and_still_exits_zero() {
    let home = Home::new(r#"{ "lines": [["model", "nosuchsegment", "cost"]] }"#);
    let out = run(&home, &["--statusline"], FIXTURE, &[]);

    assert!(stderr(&out).contains("unknown segment"), "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("nosuchsegment"));
    let bar = stdout(&out);
    assert!(bar.contains("Opus 4.8") && bar.contains("$46.51"), "the siblings still rendered");
    assert!(!bar.contains("nosuchsegment"), "stdout carries no diagnostic");
    assert!(out.status.success(), "an unknown segment is not fatal");
}

#[test]
fn the_usage_mirror_is_written_when_the_env_var_is_set() {
    let home = Home::new(&safe_config());
    let usage = TempDir::new().unwrap();
    let out = run(&home, &["--statusline"], FIXTURE, &[("AI_PLUGINS_USAGE_DIR", usage.path().to_str().unwrap())]);
    assert!(out.status.success());

    let mirrored = std::fs::read_to_string(usage.path().join("abc123.json")).expect("<session_id>.json exists");
    let parsed: serde_json::Value = serde_json::from_str(&mirrored).unwrap();

    assert_eq!(parsed["sessionId"], "abc123");
    assert_eq!(parsed["ctxPct"], 26);
    assert_eq!(parsed["ctxUsed"], 259_000);
    assert_eq!(parsed["ctxSize"], 1_000_000);
    assert_eq!(parsed["fiveHourPct"], 7);
    assert_eq!(parsed["sevenDayPct"], 1);
    // Mirrored raw, so the consumer can do its own discrimination.
    assert_eq!(parsed["fiveHourResetsAt"], 1_774_200_000i64);
    assert_eq!(parsed["sevenDayResetsAt"], 1_774_600_000i64);
}

#[test]
fn nothing_is_written_when_the_usage_env_var_is_unset() {
    let home = Home::new(&safe_config());
    let usage = TempDir::new().unwrap();
    let dir = usage.path().to_str().unwrap();

    // The `TempDir` is proved to be the directory the mirror *would* use before
    // it is used as evidence that nothing arrived in it. Without this half the
    // test creates a directory it never tells the child about and then asserts
    // it is empty — which it would be however the mirror behaved.
    let with = run(&home, &["--statusline"], FIXTURE, &[("AI_PLUGINS_USAGE_DIR", dir)]);
    assert!(with.status.success());
    assert!(usage.path().join("abc123.json").exists(), "the fixture does not name the directory the mirror uses");
    std::fs::remove_file(usage.path().join("abc123.json")).unwrap();

    let out = run(&home, &["--statusline"], FIXTURE, &[]);
    assert!(out.status.success());
    assert_eq!(std::fs::read_dir(usage.path()).unwrap().count(), 0, "the mirror is inert without the env var");
}

#[test]
fn the_refresh_child_is_recognised_and_silent() {
    // It does real work, but it is spawned with its stdio at /dev/null, so
    // writing anything would be pointless — it stays silent.
    //
    // `--subagent` used to be tested here too, back when it was a recognised
    // no-op. It renders now, and its own cases above cover it.
    //
    // **The empty stdout is the load-bearing half of this test**, and it is
    // what makes the flag's *name* checkable from outside. An unrecognised flag
    // is ignored by design, so a binary that no longer knew `--refresh` would
    // reach the missing-flag case and print one line here instead of nothing.
    // The other half of the rename — that the caller passes the same string —
    // is pinned in `cli.rs` by `REFRESH_FLAG` and, end to end, by
    // `spend_render::a_stale_cache_draws_immediately_and_spawns_a_child`.
    let home = Home::new(&safe_config());
    let out = run(&home, &["--refresh"], FIXTURE, &[]);
    assert_eq!(stdout(&out), "", "--refresh writes nothing to stdout");
    assert!(out.status.success(), "--refresh exits 0");

    // The control: the same run under the *old* name must now be unrecognised.
    // Without this the assertions above hold for a binary that recognises
    // neither, since the fixture's stdin is piped either way.
    let stale = run(&home, &[OLD_REFRESH_FLAG], FIXTURE, &[]);
    assert!(stdout(&stale).contains("missing --statusline"), "the old name is still a surface flag");
}

/// The retired flag name, spelled in two halves.
///
/// Not decoration: the sibling test below asserts the literal appears nowhere,
/// and this file is inside the tree it scans. Writing it whole here would make
/// the guard fail on its own control, and excluding this file from the scan
/// would blind the guard to every other line in it.
const OLD_REFRESH_FLAG: &str = concat!("--refresh", "-spend");

/// **Criterion 10.** The old name is gone from everything a reader could take
/// as a current instruction.
///
/// **The criterion says "appears nowhere", and that is not achievable as
/// written.** A contract amendment recording a rename has to name what it
/// renamed, or it records nothing; so does a cycle plan whose step 1 *is* the
/// rename. Read literally, the criterion deletes its own explanation.
///
/// The line drawn instead is between a **reference** and a **record**: an
/// occurrence is allowed only on a line that also says `rename`. That is one
/// rule, it is checkable, and it fails on any genuine reintroduction — a
/// `spawn_detached`, a `--help` row or a table cell has nowhere to put the
/// word. `docs/plans/` is out of scope entirely; a plan is a proposal, not a
/// description of the tree.
#[test]
fn the_old_refresh_flag_name_survives_only_where_it_records_the_rename() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut found = Vec::new();

    // **Tracked files, from git — not a filesystem walk with an exclusion
    // list.** The walk was the bug. It reached whatever happened to be sitting
    // in the working copy, so a tree that is gitignored — and therefore exists
    // in a normal checkout but not in a fresh worktree — made this test pass
    // where it was written and fail on `main`, with nothing in `git status` to
    // explain it. It happened four times, and each fix added one more path:
    // `docs/scratchpad/`, `graphify-out/`, `docs/memory/handoff/`, and a
    // `node_modules/` that no longer has anything to regenerate it. The fifth
    // would have been another path.
    //
    // Asking git instead closes the class: an untracked file is one no reader
    // is handed and no commit can change, which is exactly the scope this test
    // wanted all along. `target/`, `.claude/`, `node_modules/` and every
    // gitignored doc tree fall out for free, and nothing has to be maintained.
    let listing = std::process::Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
        .expect("git ls-files runs in a checkout");
    assert!(listing.status.success(), "git ls-files failed: {}", String::from_utf8_lossy(&listing.stderr));

    let files: Vec<&str> = std::str::from_utf8(&listing.stdout)
        .expect("git paths are utf-8 here")
        .split('\0')
        .filter(|p| !p.is_empty())
        .collect();

    // A scan of nothing passes. Cycle 03 shipped a guard that never walked the
    // repo root and cycle 04 one that could not fail; this is the assertion
    // that keeps *this* one from joining them.
    assert!(files.len() > 100, "git ls-files returned {} files — the scan would be vacuous", files.len());

    let mut scanned = 0usize;
    for rel in files {
        // `docs/plans/` is out of scope: a cycle plan whose step 1 *is* this
        // rename has to be allowed to name what it renamed. Excluded by path
        // rather than by directory name, so it cannot silently widen.
        if rel.starts_with("docs/plans/") {
            continue;
        }
        let path = root.join(rel);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue; // a binary or deleted-but-staged path is not prose
        };
        scanned += 1;
        for (n, line) in text.lines().enumerate() {
            if line.contains(OLD_REFRESH_FLAG) && !line.contains("rename") {
                found.push(format!("{rel}:{}", n + 1));
            }
        }
    }
    assert!(scanned > 50, "only {scanned} readable files scanned — the scan would be vacuous");

    found.sort();
    assert_eq!(found, Vec::<String>::new(), "the old flag name is still being used, not merely recorded");
}

#[test]
fn a_hostile_config_still_puts_a_usable_line_on_stdout() {
    // Every value here is an absurd size, an empty string where a glyph
    // belongs, or a spec that cannot resolve — all of them *deserializable*,
    // so each one is coerced on its own rather than costing the layer. The bar
    // must not go blank and must not hang.
    let hostile = r#"{
        "gauge": { "width": 1000000000000, "filled": "", "empty": "" },
        "worktreePattern": "(unclosed",
        "palette": { "blue": "not-a-triple" },
        "segments": { "model": { "bg": [300, -5, 1.5], "bold": 1 } },
        "symbols": { "model": null },
        "lines": [["model", "context", "cost"]]
    }"#;
    let home = Home::new(hostile);
    let out = run(&home, &["--statusline"], FIXTURE, &[]);

    assert!(out.status.success(), "exit code {:?}", out.status.code());
    assert!(!stdout(&out).is_empty(), "the bar must never go blank");
    // The uncompilable pattern is reported where a report belongs, and the
    // layer is otherwise honoured — the three-segment layout still applies.
    assert!(stderr(&out).contains("worktreePattern"), "stderr: {}", stderr(&out));
    assert!(!stdout(&out).contains("worktreePattern"), "and never on the bar");
    assert!(stdout(&out).contains("Opus 4.8"));
}

#[test]
fn a_scalar_where_a_block_belongs_costs_that_block_and_nothing_else() {
    // A scalar where the `gauge` block belongs. Every dotted read into it was
    // `None` before this config was typed, so it cost the gauge's keys and left
    // the layer standing — and it still must. Typing a block plainly would
    // instead make it a hard error, and the one `from_value` in the program
    // turns a hard error into a discarded config: this layer's name, its
    // layout and a user's whole theme, gone over one bad key.
    let home = Home::new(r#"{ "projectName": "e2e-fixture", "gauge": 5, "lines": [["project", "context"]] }"#);
    let out = run(&home, &["--statusline"], FIXTURE, &[]);

    assert!(out.status.success(), "exit code {:?}", out.status.code());
    let bar = stdout(&out);

    assert!(bar.contains("e2e-fixture"), "the layer survived its bad block: {}", bar.escape_debug());
    assert!(!bar.contains("Opus 4.8"), "and its two-segment layout applied, so `model` is absent");

    // The gauge itself falls back to the shipped ten-wide meter.
    let cells = bar.chars().filter(|c| *c == '\u{25b0}' || *c == '\u{25b1}').count();
    assert_eq!(cells, 10, "the shipped width, not a gauge built from `5`: {}", bar.escape_debug());

    assert!(
        !stderr(&out).contains("could not be read"),
        "a bad block is coerced, not reported — the whole-tree guard is not on this path: {}",
        stderr(&out)
    );
}

/// An unrecognised argument is **named on stderr with the help after it**, and
/// costs the surface nothing.
///
/// The two halves are one test on purpose. Naming the token is only safe
/// because it lands on the other stream — invariant 3 says a stray argument may
/// not cost a user their bar, so a version of this that wrote to stdout, or
/// that exited non-zero, would be a regression dressed as a feature. Comparing
/// against a clean run rather than asserting "stdout is non-empty" is what
/// makes that check real: the bar has to be **byte-identical**, not merely
/// present.
#[test]
fn an_unrecognised_argument_is_named_on_stderr_and_leaves_stdout_byte_identical() {
    let home = Home::new(&safe_config());

    let clean = run(&home, &["--statusline"], FIXTURE, &[]);
    let strayed = run(&home, &["--statusline", "--nonsense"], FIXTURE, &[]);

    assert_eq!(stdout(&strayed), stdout(&clean), "a stray token changed the bar");
    assert!(strayed.status.success(), "and it must not change the exit code either");
    assert_eq!(stderr(&clean), "", "the control run says nothing, so the stderr below is the stray token's");

    let err = stderr(&strayed);
    assert!(err.contains(r#"unrecognised argument "--nonsense""#), "the token is not named: {}", err.escape_debug());
    for section in ["USAGE:", "MODIFIERS:", "MORE:"] {
        assert!(err.contains(section), "the {section} section of the help did not follow it");
    }
}

/// The rename, end to end: `--debug` is not a synonym for `--doctor`, and the
/// binary says so rather than ignoring it.
///
/// **The `--doctor` control is the load-bearing half.** Asserting only that
/// `--debug` narrates nothing would pass just as well on a binary whose
/// narration had broken outright, which is the opposite of what this pins.
#[test]
fn the_old_debug_flag_is_named_as_unrecognised_and_narrates_nothing() {
    let home = Home::new(&safe_config());

    let old = run(&home, &["--statusline", "--debug"], FIXTURE, &[]);
    let plain = run(&home, &["--statusline"], FIXTURE, &[]);
    assert_eq!(stdout(&old), stdout(&plain), "the bar is untouched by the dead flag");

    let err = stderr(&old);
    assert!(err.contains(r#"unrecognised argument "--debug""#), "the old name is not named: {}", err.escape_debug());
    assert!(!err.contains("repo root:"), "--debug still turned narration on: {}", err.escape_debug());
    // And the help it printed carries the rename, so the user is not merely
    // told the flag is wrong — they are told what replaced it.
    assert!(err.contains("(earlier flag was --debug)"), "the help on stderr does not name what replaced it");

    let new = run(&home, &["--statusline", "--doctor"], FIXTURE, &[]);
    assert!(stderr(&new).contains("repo root:"), "the control failed: --doctor stopped narrating");
    assert!(!stderr(&new).contains("unrecognised"), "and it is a recognised flag");
}

/// `--help` already puts the help on stdout, so the unknown-argument path must
/// not put a second copy on stderr — both streams land in the same terminal,
/// and the motivating case (someone typing `claude-status --debug` at a prompt)
/// is exactly this mode.
#[test]
fn the_help_mode_names_the_stray_token_without_printing_the_help_twice() {
    let home = Home::new(&safe_config());
    let out = run(&home, &["--help", "--nonsense"], "", &[]);

    assert!(stdout(&out).contains("USAGE:"), "the help belongs on stdout when --help asked for it");
    let err = stderr(&out);
    assert!(err.contains(r#"unrecognised argument "--nonsense""#), "the token is still named: {}", err.escape_debug());
    assert!(!err.contains("USAGE:"), "the help was repeated on the second stream: {}", err.escape_debug());
}

/// A linked worktree's dirty state is read **in the worktree**, not in the
/// checkout its `.git` file points at.
///
/// Nothing in `resolve_markers` is worktree-aware — it runs `git diff` and
/// `git ls-files` with the cwd `find_root_and_branch` resolved, which for a
/// linked worktree is the worktree itself. That is exactly why this is pinned
/// here: the wiring is *incidental*, so nothing else in the suite would notice
/// it breaking, and the failure is silent. A dirty worktree would simply
/// render clean, which reads as "no changes" rather than as a fault.
///
/// This is the only case in the suite that runs a **real** `git worktree add`.
/// Everywhere else a repo is a hand-written `.git/HEAD`, which is cheaper and
/// enough — but a `.git` *file* written by hand is a fixture agreeing with the
/// parser, not with git. The `gitdir:` pointer, the `commondir` beside it and
/// the per-worktree index are git's to lay out, and the dirty pipeline reads
/// all three.
///
/// **The main checkout is left clean and rendered as the control.** Without it
/// a `+` on the worktree's bar is equally consistent with the markers having
/// been computed in the common checkout — the precise bug this exists to
/// catch — and the test would pass while proving nothing.
#[test]
fn a_dirty_linked_worktree_renders_its_own_dirty_marker() {
    /// Identity passed per-invocation: `run_in` clears the environment for the
    /// binary, but this helper is the *test* calling git, and a machine with no
    /// `user.email` cannot commit.
    fn git(cwd: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(["-c", "user.name=e2e", "-c", "user.email=e2e@example.invalid"])
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("git runs in the test environment");
        assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    }

    let home = Home::new(&safe_config());
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    // `-b main` explicitly: `init.defaultBranch` belongs to whoever is running
    // the suite, and the control below asserts on the branch name.
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("tracked.txt"), "one\n").unwrap();
    // Ignored *and committed*, so the checkout is still clean once the worktree
    // exists. An untracked `.worktrees/` would make the control dirty for a
    // reason that has nothing to do with what is being measured.
    std::fs::write(repo.join(".gitignore"), ".worktrees/\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "one"]);

    let worktree = repo.join(".worktrees").join("feature");
    git(&repo, &["worktree", "add", "-q", "-b", "feature", worktree.to_str().unwrap()]);

    // One tracked line added, in the worktree only. Tracked rather than
    // untracked on purpose: an untracked file contributes a flat `+1` however
    // many there are, so it would pass without `git diff` ever being read.
    std::fs::write(worktree.join("tracked.txt"), "one\ntwo\n").unwrap();

    let payload = |dir: &Path| {
        serde_json::json!({
            "model": { "display_name": "Opus 4.8" },
            "workspace": { "current_dir": dir },
            "context_window": { "used_percentage": 26, "context_window_size": 1_000_000 },
        })
        .to_string()
    };

    let in_worktree = stdout(&run(&home, &["--statusline"], &payload(&worktree), &[]));
    assert!(
        in_worktree.contains("feature +"),
        "a dirty worktree rendered clean: {}",
        in_worktree.escape_debug()
    );

    let in_checkout = stdout(&run(&home, &["--statusline"], &payload(&repo), &[]));
    assert!(
        in_checkout.contains("main"),
        "the control never resolved the checkout's branch, so it proves nothing: {}",
        in_checkout.escape_debug()
    );
    assert!(
        !in_checkout.contains("main +"),
        "the clean checkout picked up the worktree's change: {}",
        in_checkout.escape_debug()
    );
}

#[test]
fn a_hanging_git_costs_one_shared_budget_not_one_per_subprocess() {
    // The whole git budget is 250 ms *shared*. Run against a `git` that never
    // returns: sequentially at 250 ms each, the four subprocesses the dirty and
    // ahead pipelines can issue would cost about a second.
    let home = Home::new(&safe_config());

    // A `git` shim that records each invocation, then hangs until the deadline
    // kills it. `--warm` is the escape hatch the warm-up below needs.
    let shim_dir = TempDir::new().unwrap();
    let alive = shim_dir.path().join("alive");
    std::fs::create_dir_all(&alive).unwrap();
    let shim = shim_dir.path().join("git");
    std::fs::write(
        &shim,
        format!(
            "#!/bin/sh\n\
             [ \"$1\" = --warm ] && exit 0\n\
             touch '{alive}'/$$\n\
             sleep 30\n",
            alive = alive.display(),
        ),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // **Warm the shim before timing anything.** macOS assesses a newly written
    // executable on its first run — measured at 466 ms against ~4 ms warm — and
    // that assessment happens *before* the interpreter reads a line. Under a
    // 250 ms budget the deadline fires during it, so a cold shim is killed
    // having done nothing at all: no output, no `alive` marker, no `sleep`. The
    // test then measured one Gatekeeper stall instead of the git budget, which
    // is both the wrong thing and an unbounded, load-sensitive one — the whole
    // reason this was the only test in the suite that ever flaked.
    let _ = std::process::Command::new(&shim).arg("--warm").output();

    // A repo the filesystem walk will resolve a branch from, so the markers run.
    let repo = TempDir::new().unwrap();
    std::fs::create_dir_all(repo.path().join(".git")).unwrap();
    std::fs::write(repo.path().join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();

    let payload = format!(r#"{{"workspace":{{"current_dir":"{}"}}}}"#, repo.path().display());
    let path = format!("{}:{}", shim_dir.path().display(), std::env::var("PATH").unwrap_or_default());

    let started = std::time::Instant::now();
    let out = run(&home, &["--statusline"], &payload, &[("PATH", &path)]);
    let elapsed = started.elapsed();

    assert!(out.status.success());
    assert!(!stdout(&out).is_empty(), "the bar renders even when git hangs");

    // The shim actually ran — without this the timing below proves nothing,
    // which is exactly the state this test was in before the warm-up.
    let invocations = std::fs::read_dir(&alive).unwrap().count();
    assert!(invocations >= 2, "the hanging git was never reached; saw {invocations} invocations");

    assert!(
        elapsed < std::time::Duration::from_millis(900),
        "took {elapsed:?} across {invocations} hanging git calls; a shared 250 ms budget \
         should not approach the ~1 s a per-subprocess timeout would cost",
    );
}

#[test]
fn stdout_never_carries_a_diagnostic_whatever_the_input() {
    let home = Home::new(r#"{ "lines": [["bogus1", "bogus2", "model"]] }"#);
    let out = run(&home, &["--statusline", "--doctor"], "{\"garbage\":", &[]);

    let bar = stdout(&out);
    for noise in ["claude-status:", "unknown segment", "config layer", "error"] {
        assert!(!bar.contains(noise), "{noise:?} leaked onto stdout: {}", bar.escape_debug());
    }
    assert!(!stderr(&out).is_empty(), "the diagnostics did happen — just not on stdout");
}

/// Runs the binary with **no `$HOME` at all**, which is the case the
/// absent-never-relative clause governs.
///
/// A separate process, so this needs none of the in-process env locking the
/// unit tests do — `env_clear()` simply never sets it.
fn run_without_home(args: &[&str], stdin: &str, cwd: &Path, extra_env: &[(&str, &str)]) -> Output {
    // A `PATH` with no `security` on it, per invariant 5's second rule: with no
    // `$HOME` there is no credentials **file**, and the keychain arm is not
    // `$HOME`-scoped — so without this the "no credentials" half is true only
    // because the code happens to bail on the cache path first. Relying on that
    // is relying on an ordering, not on the test's own setup.
    let no_security = [("PATH", "/nonexistent/claude-status-test-path")];
    let env: Vec<(&str, &str)> = no_security.iter().chain(extra_env.iter()).copied().collect();
    run_in(args, stdin, None, Some(cwd), &env)
}

/// The one place this file builds a command.
///
/// `run` and `run_without_home` are the two shapes tests actually want; both go
/// through here so the spend hazard the module docs describe is neutralised in
/// exactly **one** place. Three near-identical hand-rolled builders is how a
/// `CLAUDE_STATUS_SPEND_URL` goes missing from the fourth.
fn run_in(args: &[&str], stdin: &str, home: Option<&Path>, cwd: Option<&Path>, extra_env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(BINARY);
    cmd.args(args)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        // Never optional. See the module docs on the spend hazard.
        .env("CLAUDE_STATUS_SPEND_URL", CLOSED_PORT_URL)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(home) = home {
        cmd.env("HOME", home)
            .env("CLAUDE_STATUS_SPEND_CACHE", home.join(".cache").join("claude-status").join("spend.json"));
    }
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().expect("the binary runs");
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

/// `~/.cache/claude-status/`, relative to `$HOME` — the one directory this tool
/// may write to.
fn cache_prefix() -> String {
    format!(".cache{}claude-status", std::path::MAIN_SEPARATOR)
}

/// Whether a run is expected to fork the detached spend-refresh child.
///
/// Passed explicitly rather than detected, because the two cases need opposite
/// bounds and a helper that guessed would be free to guess "no" and wait for
/// nothing — which is exactly the bug this enum replaced.
#[derive(Clone, Copy, PartialEq)]
enum Child {
    /// `refreshMinutes` is non-zero and the cache is missing or stale.
    Expected,
    /// `refreshMinutes` is `0`, so `schedule::decide` returns `Disabled`
    /// before it even looks at the cache.
    NotExpected,
}

/// Waits for the detached spend-refresh child, so a snapshot taken afterwards
/// includes everything it wrote.
///
/// A render that decides to refresh forks a detached child and returns
/// **without waiting for it**, so the parent exits — and the test snapshots —
/// while the child is still opening files.
///
/// # Why this does not watch the lock
///
/// The obvious handle is `spend.json.lock`, and it is the wrong one. It is
/// created `O_EXCL` by the child and unlinked by `LockGuard::drop`, so
/// **absence is the state both before and after** the child runs. A helper that
/// returned on `!lock.exists()` returns on its first poll, inside the
/// fork→create window, having waited for nothing — and reads as if it waited.
/// That is not a race being lost; it is a guard that never looked. The previous
/// version of this function did exactly that, and neutering it entirely turned
/// no test red.
///
/// # What it watches instead
///
/// The **cache file's bytes**. Every terminal path in `refresh::run_reported`
/// writes it — including the failure paths, which is what a closed-port fixture
/// takes — and unlike the lock it is never removed, so a change to it is
/// monotonic and cannot be missed between two polls.
///
/// [`Child::Expected`] waits for that change **and** for the lock to be gone,
/// then panics if neither arrives inside the deadline: a run that stops
/// spawning must fail loudly rather than make every caller vacuous.
/// [`Child::NotExpected`] watches briefly for the opposite and panics if a
/// child appears. Its window is a sanity check and not a proof — the proof that
/// `refreshMinutes: 0` cannot spawn is `schedule::decide`'s own unit test — so
/// its bound is short on purpose.
fn settle(home: &Path, expect: Child) {
    let cache = home.join(".cache").join("claude-status").join("spend.json");
    let lock = home.join(".cache").join("claude-status").join("spend.json.lock");
    let before = std::fs::read(&cache).ok();
    let touched = |b: &Option<Vec<u8>>| std::fs::read(&cache).ok() != *b || lock.exists();

    match expect {
        Child::Expected => {
            for _ in 0..500 {
                if std::fs::read(&cache).ok() != before && !lock.exists() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            panic!("no refresh child wrote {} within 5s — did this run stop spawning one?", cache.display());
        }
        Child::NotExpected => {
            for _ in 0..25 {
                assert!(!touched(&before), "a refresh child spawned where none was expected");
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }
}

/// Every path under `root`, relative and sorted, with each file's bytes.
///
/// Content and not merely names: "nothing was created" is the easy half, and
/// an in-place rewrite of a file that already existed would pass a name-only
/// check while being exactly the kind of write criterion 6 is about.
fn snapshot(root: &Path) -> Vec<(String, Option<Vec<u8>>)> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, Option<Vec<u8>>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let rel = path.strip_prefix(base).unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                out.push((format!("{rel}/"), None));
                walk(&path, base, out);
            } else {
                out.push((rel, Some(std::fs::read(&path).unwrap_or_default())));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

/// **Criterion 1.** The zero-config state, made a supported and tested one
/// rather than a degraded accident.
///
/// `$HOME` is a genuinely empty directory — no config, no cache, no
/// credentials — and the cwd is outside any git repo. A full bar renders, and
/// `$HOME` gains nothing outside `~/.cache/claude-status/`.
///
/// **The criterion says "nothing is created anywhere under that `$HOME`", and
/// that is not quite achievable**: an empty `$HOME` means an empty spend cache,
/// which is exactly the condition that makes a render spawn the detached
/// refresh child, and that child creates `~/.cache/claude-status/`. Criterion 6
/// permits precisely that directory, so the two criteria disagree by the cache
/// and this test resolves it criterion 6's way. The `.cache` exemption is
/// therefore named rather than assumed, and `settle` makes the child's writes
/// land *before* the snapshot rather than after it.
#[test]
fn with_an_empty_home_and_no_repo_a_full_bar_renders_and_nothing_is_created() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    assert!(snapshot(home.path()).is_empty(), "the fixture is not empty to begin with");

    // **A sixth neutralisation, and this test is the reason it is needed.**
    // The module note above lists five, and the fourth — a seeded
    // `.claude/.credentials.json` — is unavailable here by construction: an
    // empty `$HOME` is the whole point. `creds::load` falls through to
    // `from_keychain`, which shells out to `security`, and the macOS keychain
    // is not scoped by `$HOME`. Emptying `PATH` makes `security` unresolvable
    // so the probe cannot run at all, which is a harder guarantee than seeding
    // a file. `git` goes with it, which costs nothing: the cwd is not a repo.
    let empty_path = TempDir::new().unwrap();
    let env = [("PATH", empty_path.path().to_str().unwrap())];

    let out = run_in(&["--statusline"], FIXTURE, Some(home.path()), Some(cwd.path()), &env);

    assert!(out.status.success(), "exit code {:?}", out.status.code());
    let bar = stdout(&out);
    // A *full* bar, not a fallback line and not one lonely segment. The
    // embedded layout is two rows; the second is git-only and this cwd is not a
    // repo, so the first row is what must be complete.
    assert!(!bar.starts_with("\u{26a1} Claude"), "the panic fallback rendered: {}", bar.escape_debug());
    assert!(bar.contains("Opus 4.8"), "{}", bar.escape_debug());
    assert!(bar.contains("259k/1M (26%)"), "the gauge segment is missing: {}", bar.escape_debug());
    assert!(bar.contains("$46.51"), "the cost segment is missing: {}", bar.escape_debug());
    assert!(bar.contains('\u{25b0}'), "the embedded gauge glyph came through");

    // `Expected`, and it is load-bearing: an empty `$HOME` means no spend
    // cache, which is precisely the condition that makes a render fork the
    // refresh child. Snapshotting before it lands is how this test passed for
    // the wrong reason twice — first by racing it, then by watching a lock
    // whose absence meant nothing.
    settle(home.path(), Child::Expected);
    let created: Vec<String> = snapshot(home.path())
        .into_iter()
        .map(|(p, _)| p)
        .filter(|p| !p.starts_with(&cache_prefix()) && p != ".cache/")
        .collect();
    assert_eq!(created, Vec::<String>::new(), "the render created something outside the cache");
    assert_eq!(snapshot(cwd.path()), Vec::new(), "the render created something in the cwd");
}

/// **Criterion 6**, run rather than reasoned about.
///
/// A render is traced by comparing the whole of `$HOME` before and after, byte
/// for byte, with `~/.cache/claude-status/` excused — that is the one place a
/// render may write, and it is excused *by name* so a write anywhere else
/// cannot hide behind the exemption.
///
/// **The criterion names one carve-out and needs two.** `$AI_PLUGINS_USAGE_DIR`
/// is the second: `--statusline` writes the usage mirror there and the caps
/// hook writes `<sid>.state.json` beside it, both of which the criterion's
/// wording forbids and neither of which this cycle may touch — the usage
/// mirror (`docs/usage-mirror-contract.md`) is a live contract with another
/// repository and the plan puts it explicitly out of
/// scope. So the usage directory is pointed **outside `$HOME`** and asserted
/// separately: the mirror must land there, and `$HOME` must still gain nothing.
///
/// Setting it rather than leaving it unset is the point. Unset, the mirror is
/// inert and the test would be claiming "no mode writes outside the cache"
/// while never exercising the one code path that does.
///
/// This is the filesystem-level check rather than a syscall trace because
/// `dtrace` needs SIP disabled on macOS, which no CI machine and few laptops
/// have. It is weaker in one specific way and the gap is worth naming: a write
/// that opens a file, changes nothing and closes it is invisible here. It
/// catches everything that leaves a mark.
///
/// Every mode is covered, not just `--statusline`. `--statusline` is the one
/// that used to write, but a rule that holds for one surface and not the
/// others is not the invariant this cycle bought.
///
/// **`--configure` is the one deliberate exception, and it is named here rather
/// than quietly left out of the matrix.** Writing `~/.claude/settings.json` and
/// seeding `~/.config/claude-status/config.json` is the whole of what that flag
/// does, so including it would assert the opposite of its contract — and the
/// paragraph above argues this invariant holds for *every* surface, which makes
/// a silent omission read as a bug rather than as a decision. What it writes,
/// and that it writes nothing else, is covered by its own cases above;
/// `configure_dry_run_prints_and_writes_nothing` is the row that would have
/// belonged here, and it snapshots the whole of `$HOME` the same way.
#[test]
fn no_mode_writes_outside_the_cache_directory() {
    let prefix = cache_prefix();
    let outside_cache = |root: &Path| {
        snapshot(root).into_iter().filter(|(p, _)| !p.starts_with(&prefix) && p != ".cache/").collect::<Vec<_>>()
    };

    // `spawns` is the third column and the one that took two attempts to get
    // right. `safe_config()` sets `refreshMinutes: 0`, so every row using it
    // returns `Decision::Disabled` before the cache is even read and **no child
    // is ever forked** — which meant this test's `settle` call was inert in
    // every iteration, and its claim to cover "a mode that wrote nothing itself
    // but spawned something that did" was false.
    //
    // The `Child::Expected` rows fix that: a non-zero interval, and the seeded
    // cache deleted so the schedule has nothing fresh to be satisfied by. The
    // `NotExpected` rows are kept rather than converted — a mode that started
    // forking a child it never used to would be a regression too, and only
    // those rows can catch it.
    for (args, stdin, spawns) in [
        (&["--statusline"][..], FIXTURE, Child::NotExpected),
        (&["--statusline"][..], FIXTURE, Child::Expected),
        (&["--subagent"][..], SUBAGENT_FIXTURE, Child::NotExpected),
        (&["--refresh"][..], "", Child::NotExpected),
        (&["--doctor"][..], "", Child::NotExpected),
        (&["--caps-hook"][..], r#"{"session_id":"s1"}"#, Child::NotExpected),
        (&["--help"][..], "", Child::NotExpected),
        (&["--version"][..], "", Child::NotExpected),
    ] {
        let config = match spawns {
            Child::Expected => r#"{ "projectName": "traced", "spend": { "refreshMinutes": 15, "show": "never" } }"#,
            Child::NotExpected => &safe_config(),
        };
        let home = Home::new(config);
        if spawns == Child::Expected {
            // The seeded cache is stamped `now`, so the schedule would call it
            // fresh and decline to refresh. Removing it is what makes the fork
            // happen. The keychain stays neutralised — `Home::new` seeds the
            // credentials file, so `creds::load` never reaches `security`.
            std::fs::remove_file(home.path().join(".cache").join("claude-status").join("spend.json")).unwrap();
        }
        // Inside a repo, because that is where a render used to create a file.
        let repo = fake_repo(r#"{ "projectName": "traced" }"#);
        let repo_before = snapshot(repo.path());
        let before = outside_cache(home.path());

        // The usage-mirror carve-out, deliberately outside `$HOME` so the two are
        // separable: whatever lands here is the usage mirror, and whatever
        // lands under `$HOME` outside the cache is a violation.
        let usage = TempDir::new().unwrap();
        let env = [("AI_PLUGINS_USAGE_DIR", usage.path().to_str().unwrap())];

        let out = run_in(args, stdin, Some(home.path()), Some(repo.path()), &env);
        assert!(out.status.success(), "{args:?} exited {:?}", out.status.code());
        // **Every row must have run the mode it names.** An unrecognised flag
        // is ignored by design and writes no files, so a row whose flag had
        // been renamed out from under it would satisfy every assertion below
        // having exercised the missing-flag branch instead — cycle 02's C7
        // shape. The missing-flag line is the one thing that branch always
        // emits, and only `--help` legitimately mentions `--statusline`.
        if args != ["--help"] {
            assert!(
                !stdout(&out).contains(MISSING_FLAG_LINE),
                "{args:?} was not recognised, so this row exercised nothing",
            );
        }

        // After the child, not before it. A mode that wrote nothing itself but
        // spawned something that did would otherwise pass.
        settle(home.path(), spawns);
        assert_eq!(outside_cache(home.path()), before, "{args:?} wrote under $HOME outside the cache");
        assert_eq!(snapshot(repo.path()), repo_before, "{args:?} wrote inside the repo");
    }

    // Not vacuous: the mirror's writer has to be live, or every assertion holds
    // for a run in which nothing could have written to the usage directory in
    // the first place. `--statusline` with a `session_id` is what mirrors.
    let home = Home::new(&safe_config());
    let usage = TempDir::new().unwrap();
    let env = [("AI_PLUGINS_USAGE_DIR", usage.path().to_str().unwrap())];
    run_in(&["--statusline"], FIXTURE, Some(home.path()), None, &env);
    assert!(
        usage.path().join("abc123.json").exists(),
        "the usage mirror never fired, so the carve-out above was never exercised",
    );
}

/// **Criterion 4**, at the surface. The unit test in `layers.rs` proves the
/// path is not read; this proves the binary agrees, which is the claim a user
/// who moved their file is actually making.
#[test]
fn a_user_config_at_the_old_bare_path_is_ignored_by_the_binary() {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    let old = home.path().join(".config").join("claude-status.json");
    std::fs::create_dir_all(old.parent().unwrap()).unwrap();
    std::fs::write(&old, r#"{ "projectName": "from-the-old-path", "lines": [["project"]] }"#).unwrap();

    let bar = stdout(&run_in(&["--statusline"], FIXTURE, Some(home.path()), Some(cwd.path()), &[]));
    assert!(!bar.contains("from-the-old-path"), "the old path was still read: {}", bar.escape_debug());
    assert!(bar.contains("Opus 4.8"), "and the embedded layout rendered instead: {}", bar.escape_debug());

    // Not merely unread — untouched. A future "helpfully migrate it" would
    // land here, and this cycle deliberately has no migration.
    assert!(old.exists(), "the old file was moved or removed");
}

#[test]
fn with_no_home_the_bar_still_renders() {
    // Invariant 3 outranks the new clause: a render never fails visibly, even
    // when every path derived from `$HOME` is absent.
    let dir = TempDir::new().unwrap();
    let out = run_without_home(&["--statusline"], FIXTURE, dir.path(), &[]);

    assert!(out.status.success(), "exit code {:?}", out.status.code());
    let bar = stdout(&out);
    assert!(bar.contains("Opus 4.8"), "got: {}", bar.escape_debug());
}

#[test]
fn with_no_home_nothing_is_written_relative_to_the_cwd() {
    // The point of the clause, and the exact paths the old code produced:
    // `cache::path()` fell back to a bare `spend.json`, and `expand_home`
    // returned the literal `~/usage` — so the mirror landed in a directory
    // *named* `~`, not in one called `usage`. Naming both precisely is what
    // stops this passing for the wrong reason.
    let dir = TempDir::new().unwrap();
    let marker = dir.path().join("only-this-should-be-here");
    std::fs::write(&marker, "").unwrap();

    run_without_home(&["--statusline"], FIXTURE, dir.path(), &[("CLAUDE_STATUS_USAGE_DIR", "~/usage")]);
    // The empty stdout is not decoration. Every assertion below is about a file
    // that was *not* written, and an unrecognised flag writes no files either —
    // so with a stale flag name this whole test would pass having exercised the
    // missing-flag branch instead of the refresh path. That is the shape of
    // vacuity cycle 02 recorded as C7.
    let refresh = run_without_home(&["--refresh"], "", dir.path(), &[]);
    assert_eq!(stdout(&refresh), "", "--refresh was not recognised, so nothing below was exercised");

    let after: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(after, vec![marker.file_name().unwrap()], "something was written: {after:?}");
    assert!(!dir.path().join("spend.json").exists(), "the spend cache went relative");
    assert!(!dir.path().join("~").exists(), "the usage mirror wrote into a directory named `~`");
}

#[test]
fn with_no_home_debug_names_the_missing_variable() {
    // `--doctor` exists to say what is wrong; an empty SPEND section would be
    // the useless answer the user already had.
    let dir = TempDir::new().unwrap();
    let out = run_without_home(&["--doctor"], "", dir.path(), &[]);

    // Scoped to the SPEND section. A bare `contains("$HOME")` over the whole
    // report is satisfied by the `user  using defaults  <no $HOME>` row in CONFIG
    // LAYERS — which this same cycle added — so it would pass even if the spend
    // section said nothing at all.
    let report = stdout(&out);
    // Bounded at both ends. `SAMPLE RENDER` is appended after SPEND, so a slice
    // that only cut at the start would run to the end of the report and pick up
    // whatever came after — holding by luck rather than by construction.
    let spend = report
        .split("\nSPEND")
        .nth(1)
        .expect("the SPEND section is present")
        .split("\nSAMPLE RENDER")
        .next()
        .expect("split always yields one");
    assert!(spend.contains("$HOME"), "the spend section never mentions it: {spend}");
    assert!(spend.contains("UNAVAILABLE"), "no cache verdict: {spend}");
}

#[test]
fn with_no_home_the_caps_hook_stays_silent() {
    // Its usage directory names `$HOME` and cannot resolve one, so it is inert
    // — the same rule the writer follows, rather than reading from a directory
    // relative to wherever the hook happened to run.
    let dir = TempDir::new().unwrap();

    // **Seeded at the path the OLD code would have read.** `expand_home` used
    // to return the literal `~/usage`, which resolves relative to the cwd — so
    // a breach-level mirror here is exactly what it would have found and acted
    // on. Without this the test proves nothing: an absent file produces silence
    // either way, which is how it passed before this seeding was added.
    seed_mirror(&dir.path().join("~").join("usage"), "s1", r#"{"sevenDayPct":85,"sevenDayResetsAt":1774600000}"#);

    let out = run_without_home(
        &["--caps-hook"],
        r#"{"session_id":"s1"}"#,
        dir.path(),
        &[("CLAUDE_STATUS_USAGE_DIR", "~/usage")],
    );

    assert!(out.status.success());
    assert_eq!(stdout(&out), "", "a directive was injected into the agent's context");
}

#[test]
fn doctor_reports_a_hostile_config_without_obeying_it() {
    // `--doctor` is the fourth of the five filter surfaces. Two of the values
    // below land in two *different* sections of the report (EFFECTIVE LAYOUT
    // and the spend gate table), which is exactly why the filter is one sweep
    // over the assembled report rather than a call at each write: both were
    // missed that way.
    //
    // The third, `symbols.spend`, reaches the VERDICT line only through
    // `hidden_verdict`, which needs `Outcome::Updated` — a successful fetch,
    // which a closed port never produces. It is planted anyway, so that if this
    // test is ever pointed at a stub server the value is already in place; it
    // is **not** load-bearing here, and this comment says so rather than
    // letting a future reader assume it is covered.
    //
    // **The payload is in the user layer, not the repo layer.** It was the repo
    // layer, on the grounds that a repo config was the widest input to the
    // report — true until `config-relocation` narrowed it to `projectName`.
    // None of these three keys reaches the report from a repo file any more, so
    // left there this test would assert "no escape in the report" about a
    // report that had never been shown one.
    let esc = '\u{1b}';
    // Built with `serde_json` rather than written as raw text: a literal ESC
    // byte inside a JSON string is **invalid JSON**, so a hand-written fixture
    // fails to parse, the layer reads as UNREADABLE, and the assertions below
    // pass having exercised nothing. It has to be an escape sequence on disk.
    let home = Home::new(
        &serde_json::json!({
            // `describe_entry` prints a segment's id or name, so that is where
            // a layout entry can carry one into EFFECTIVE LAYOUT.
            //
            // `project` rides along because `lines` **replaces** the layout
            // wholesale: without it the row is two unknown segments, both
            // omitted, and the SAMPLE RENDER is empty — so the repo's
            // `projectName` would have nowhere to be drawn and the assertion
            // about it below would be checking an empty string.
            "lines": [[format!("{esc}]52;c;cGF5bG9hZA=="), { "name": format!("{esc}[41mcost") }, "project"]],
            // Straight into gate 4's row of the spend table.
            "spend": { "refreshMinutes": 0, "show": format!("{esc}[2J{esc}[H") },
            "symbols": { "spend": format!("{esc}[41mFAKE") },
        })
        .to_string(),
    );

    // The repo layer still gets a payload, in the one key it may still set.
    // A cloned repository has not stopped being attacker-controlled; it has
    // stopped being *wide*. `projectName` reaches the SAMPLE RENDER through the
    // `project` segment, which is the one part of the report deliberately not
    // covered by the report-wide sweep.
    let repo = fake_repo(&serde_json::json!({ "projectName": format!("{esc}[2Jowned") }).to_string());

    let out = run_in(&["--doctor"], "", Some(home.path()), Some(repo.path()), &[]);

    let report = stdout(&out);
    // SAMPLE RENDER is renderer output and legitimately carries SGR codes, so
    // assert against everything before it.
    let diagnostics = report.split("SAMPLE RENDER").next().unwrap();
    // Proof the fixtures actually landed. Without this the layer can fail to
    // parse, read as UNREADABLE, and the escape assertion below passes having
    // exercised nothing — which is what this test did at first, and what it
    // silently went back to doing when the repo layer was narrowed.
    assert!(diagnostics.contains("user     loaded"), "the user layer never loaded: {diagnostics}");
    assert!(diagnostics.contains("repo     loaded"), "the repo layer never loaded: {diagnostics}");
    assert!(!diagnostics.contains(esc), "an escape reached the report: {}", diagnostics.escape_debug());
    assert!(diagnostics.contains("EFFECTIVE LAYOUT"), "the report was not produced at all: {report}");
    assert!(report.contains("SAMPLE RENDER"), "the sample render must still be appended");

    // The repo's name is *rendered*, not merely dropped — the sanitiser is what
    // makes it safe, so it has to have arrived to have been sanitised.
    let sample = report.split("SAMPLE RENDER").nth(1).expect("the section is present");
    assert!(sample.contains("owned"), "the repo's projectName never reached the sample: {}", sample.escape_debug());
    // The **ESC-prefixed** sequence, not the bare text. `\x1b[2J` erases the
    // display; a literal `[2J` with the ESC stripped is inert characters on a
    // segment, which is precisely what the sanitiser is supposed to leave
    // behind. Asserting on the bare text would call a working filter a failure.
    // The renderer emits its own `\x1b[` SGR codes here, so this section cannot
    // be checked for escapes wholesale the way the diagnostics above are.
    assert!(
        !sample.contains("\u{1b}[2J"),
        "an erase-display survived into the sample: {}",
        sample.escape_debug(),
    );
}

#[test]
fn a_config_cannot_forge_lines_in_the_doctor_report() {
    // The **newline** attack, which needs no escape at all and which the
    // report-wide sweep cannot stop: that sweep exempts `\n` because the report
    // is many lines, so a dynamic value carrying one forges a line — or a whole
    // section header — in the diagnostic a user reads when trying to work out
    // what is wrong with their machine. It is why every value in the report
    // also goes through the row filter.
    //
    // The payload moved from the repo layer to the **user** layer when the repo
    // layer was narrowed to `projectName`: neither `lines` nor `spend.show`
    // reaches the report from a repo config any more, so mounting it there
    // would have quietly stopped testing anything. The repo layer's own new
    // vector is the sibling test below.
    let home = Home::new(
        &serde_json::json!({
            "spend": { "refreshMinutes": 0, "show": "auto\n\n  VERDICT  everything is fine, nothing to see" },
            "lines": [["model\nCLAUDE WIRING (~/.claude/settings.json)\n  statusLine: FORGED"]],
        })
        .to_string(),
    );

    let out = run_in(&["--doctor"], "", Some(home.path()), None, &[]);
    let report = stdout(&out);
    let diagnostics = report.split("SAMPLE RENDER").next().unwrap();

    assert!(diagnostics.contains("user     loaded"), "the user layer never loaded: {diagnostics}");

    // The text itself surviving is correct and expected — the report is
    // *reporting* what the config says. What must not survive is the
    // **structure**: the value may not become a line of its own.
    let forged = diagnostics.lines().find(|l| l.contains("FORGED")).expect("the value is still reported");
    assert!(forged.trim_start().starts_with("line 0:"), "it broke out onto its own line: {forged:?}");

    let verdict = diagnostics.lines().find(|l| l.contains("everything is fine")).expect("still reported");
    assert!(verdict.trim_start().starts_with("gate 4"), "it broke out of its gate row: {verdict:?}");

    // Exactly one line *begins* each real section header. Counting substrings
    // would count the inert copy inside the `line 0:` row above, which is the
    // report faithfully quoting the config and is not a forgery.
    let starts_with = |needle: &str| diagnostics.lines().filter(|l| l.trim_start().starts_with(needle)).count();
    assert_eq!(starts_with("CLAUDE WIRING"), 1, "a section header was forged: {diagnostics}");
    assert_eq!(starts_with("VERDICT"), 1, "a VERDICT line was forged: {diagnostics}");
}

/// The vector the narrowing **created**, which had no equivalent before it.
///
/// A repo layer's ignored keys are now reported by name, and a JSON key may
/// contain a newline. So the repo layer went from being able to forge a report
/// line through its *values* to being able to forge one through its *key
/// names* — a smaller surface, but a new one, and the report's row filter is
/// what has to cover it.
#[test]
fn a_repo_layers_ignored_key_names_cannot_forge_lines_in_the_doctor_report() {
    let home = Home::new(&safe_config());
    let repo = fake_repo(
        &serde_json::json!({
            "projectName": "victim",
            "gauge\n  VERDICT  everything is fine, nothing to see": 1,
            "caps\nCLAUDE WIRING (~/.claude/settings.json)\n  statusLine: FORGED": 1,
        })
        .to_string(),
    );

    let report = stdout(&run_in(&["--doctor"], "", Some(home.path()), Some(repo.path()), &[]));
    let diagnostics = report.split("SAMPLE RENDER").next().unwrap();

    let forged = diagnostics.lines().find(|l| l.contains("FORGED")).expect("the key is still reported");
    assert!(forged.trim_start().starts_with("ignored"), "it broke out onto its own line: {forged:?}");

    let starts_with = |needle: &str| diagnostics.lines().filter(|l| l.trim_start().starts_with(needle)).count();
    assert_eq!(starts_with("CLAUDE WIRING"), 1, "a section header was forged: {diagnostics}");
    assert_eq!(starts_with("VERDICT"), 1, "a VERDICT line was forged: {diagnostics}");
}

/// **Criterion 4.** A typo is reported under the layer that contains it — and
/// the bar renders exactly as it did before the typo was introduced.
///
/// Both halves matter, and the second is the one the plan's step 4 was a
/// hazard to. A strict deserialize on the render path would make `powerlin`
/// blank the whole `powerline` block, drawing a bar with no separators: the
/// user would trade a silent no-op for a visibly broken status line, which is
/// worse than the problem. So the two renders are compared byte for byte.
#[test]
fn a_typo_is_reported_under_its_layer_and_changes_nothing_about_the_bar() {
    let clean = Home::new(&safe_config());
    let typo = Home::new(
        r#"{ "projectName": "e2e-fixture", "spend": { "refreshMinutes": 0, "show": "never" },
             "powerlin": { "sep": ">" } }"#,
    );

    assert_eq!(
        stdout(&run(&typo, &["--statusline"], FIXTURE, &[])),
        stdout(&run(&clean, &["--statusline"], FIXTURE, &[])),
        "a key the binary does not know changed the bar",
    );

    let report = stdout(&run(&typo, &["--doctor"], "", &[]));
    let layers = report.split("\nCLAUDE WIRING").next().expect("split always yields one");
    let line = layers
        .lines()
        .find(|l| l.contains("powerlin`"))
        .unwrap_or_else(|| panic!("--doctor never named the typo:\n{layers}"));
    assert!(line.contains('\u{26a0}'), "a typo in a closed block is a warning: {line:?}");
    assert!(line.contains("did you mean `powerline`?"), "and it says what was probably meant: {line:?}");

    // Under the **user** layer, not floating at the end of the section: the
    // answer to "why is this doing nothing" is which of three files it is in.
    let user_at = layers.lines().position(|l| l.starts_with("  user")).expect("a user row");
    let repo_at = layers.lines().position(|l| l.starts_with("  repo")).expect("a repo row");
    let finding_at = layers.lines().position(|l| l.contains("powerlin`")).expect("the finding");
    assert!(user_at < finding_at && finding_at < repo_at, "the finding is not under its layer:\n{layers}");
}

/// **Criterion 5.**
#[test]
fn doctor_reports_what_a_zero_gauge_width_became() {
    let home = Home::new(r#"{ "spend": { "refreshMinutes": 0, "show": "never" }, "gauge": { "width": 0 } }"#);

    let report = stdout(&run(&home, &["--doctor"], "", &[]));
    let layers = report.split("\nCLAUDE WIRING").next().expect("split always yields one");
    let line = layers
        .lines()
        .find(|l| l.contains("gauge.width"))
        .unwrap_or_else(|| panic!("--doctor never reported the coercion:\n{layers}"));
    assert!(line.contains("0 \u{2192} 10"), "it says what the value became: {line:?}");
    assert!(!line.contains('\u{26a0}'), "a coercion is a note, not a warning: {line:?}");

    // And the bar really is ten wide, which is what the note claims.
    let bar = stdout(&run(&home, &["--statusline"], FIXTURE, &[]));
    let cells = bar.chars().filter(|c| *c == '\u{25b0}' || *c == '\u{25b1}').count();
    assert_eq!(cells, 10, "the note described a coercion that did not happen: {}", bar.escape_debug());
}

/// **Criterion 6, asserted positively.**
///
/// "An unknown key in `palette` is not reported as an error" passes with
/// nothing written at all — `palette` is an open map, so no key in it *can* be
/// unknown. The claim worth testing is the one a user would notice: the key
/// appears, under its layer, as a note rather than a warning.
#[test]
fn an_unused_palette_entry_is_a_note_under_its_layer_and_never_an_error() {
    let home =
        Home::new(r#"{ "spend": { "refreshMinutes": 0, "show": "never" }, "palette": { "nobodys": [1, 2, 3] } }"#);

    let out = run(&home, &["--doctor"], "", &[]);
    assert!(out.status.success(), "a palette key made --doctor fail: {:?}", out.status.code());
    let report = stdout(&out);
    let layers = report.split("\nCLAUDE WIRING").next().expect("split always yields one");

    let line = layers
        .lines()
        .find(|l| l.contains("palette.nobodys"))
        .unwrap_or_else(|| panic!("--doctor never mentioned the unused palette key:\n{layers}"));
    assert!(line.contains('\u{b7}'), "an open-map key is a note: {line:?}");
    assert!(!line.contains('\u{26a0}'), "an open-map key must never warn — it is legal: {line:?}");
    assert!(line.contains("is not a key this binary reads"), "{line:?}");

    // A palette entry something names is not reported at all.
    let used = Home::new(
        r#"{ "spend": { "refreshMinutes": 0, "show": "never" },
             "palette": { "mine": [1, 2, 3] }, "defaultFg": "mine" }"#,
    );
    let report = stdout(&run(&used, &["--doctor"], "", &[]));
    assert!(!report.contains("palette.mine"), "a colour something uses was reported as unused:\n{report}");
}

/// **Criterion 7.** Findings change nothing on stdout and nothing about the
/// exit code, in every mode.
#[test]
fn findings_change_neither_the_render_nor_the_exit_code() {
    let clean = Home::new(&safe_config());
    // One config carrying all three kinds at once.
    let messy = Home::new(
        r#"{ "projectName": "e2e-fixture", "spend": { "refreshMinutes": 0, "show": "never" },
             "powerlin": { "sep": ">" },
             "symbols": { "contxt": "x" },
             "palette": { "nobodys": [1, 2, 3] },
             "segments": { "model": { "bge": "red" } },
             "worktreePattern": "[" }"#,
    );

    for (args, stdin) in [(["--statusline"], FIXTURE), (["--subagent"], SUBAGENT_FIXTURE)] {
        let dirty = run(&messy, &args, stdin, &[]);
        let tidy = run(&clean, &args, stdin, &[]);
        assert!(dirty.status.success(), "{args:?} exited {:?}", dirty.status.code());
        assert_eq!(stdout(&dirty), stdout(&tidy), "{args:?} rendered differently");
    }

    let report = run(&messy, &["--doctor"], "", &[]);
    assert!(report.status.success(), "--doctor exited {:?} over advisory findings", report.status.code());

    // Every finding is present — a validator that stopped at the first would
    // send the user round the loop once per typo.
    let out = stdout(&report);
    let layers = out.split("\nCLAUDE WIRING").next().expect("split always yields one");
    for needle in ["powerlin`", "symbols.contxt", "palette.nobodys", "segments.model.bge", "worktreePattern"] {
        assert!(layers.contains(needle), "{needle} is missing from:\n{layers}");
    }
}

/// A finding's key names cannot forge a row or a section header.
///
/// The sibling of `a_repo_layers_ignored_key_names_cannot_forge_lines_in_the_doctor_report`,
/// for the second thing that now prints user-controlled text into this section.
/// A JSON key may contain a newline, and `--doctor` is read precisely by someone
/// trying to work out what is wrong.
#[test]
fn a_findings_key_name_cannot_forge_lines_in_the_doctor_report() {
    let home = Home::new(
        &serde_json::json!({
            "spend": { "refreshMinutes": 0, "show": "never" },
            "powerlin\n  VERDICT  everything is fine": 1,
            "symbols": { "contxt\nCLAUDE WIRING (~/.claude/settings.json)\n  statusLine: FORGED": "x" },
        })
        .to_string(),
    );

    let report = stdout(&run(&home, &["--doctor"], "", &[]));
    let diagnostics = report.split("SAMPLE RENDER").next().unwrap();

    let starts_with = |needle: &str| diagnostics.lines().filter(|l| l.trim_start().starts_with(needle)).count();
    assert_eq!(starts_with("CLAUDE WIRING"), 1, "a section header was forged:\n{diagnostics}");
    assert_eq!(starts_with("VERDICT"), 1, "a VERDICT line was forged:\n{diagnostics}");
    assert!(diagnostics.contains("FORGED"), "the key is still reported:\n{diagnostics}");
}

/// A correctly written config puts nothing in the section.
///
/// The counterpart to every test above. A validator that said something about
/// every file would be noise, and noise in a diagnostic is indistinguishable
/// from no diagnostic at all.
#[test]
fn a_config_the_binary_understands_adds_no_findings_to_the_report() {
    for config in [
        safe_config(),
        // Including the `$schema` pointer `--configure` writes into every file.
        r#"{ "$schema": "https://raw.githubusercontent.com/virajp/claude-status/main/schemas/claude-status.schema.json",
             "spend": { "refreshMinutes": 0, "show": "never" } }"#
            .to_string(),
    ] {
        let home = Home::new(&config);
        let report = stdout(&run(&home, &["--doctor"], "", &[]));
        let layers = report.split("\nCLAUDE WIRING").next().expect("split always yields one");
        assert!(!layers.contains('\u{26a0}'), "a clean config produced a warning:\n{layers}");
        assert!(!layers.contains('\u{b7}'), "a clean config produced a note:\n{layers}");
    }
}
