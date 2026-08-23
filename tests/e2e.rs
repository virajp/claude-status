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
//! The fourth was added when `--refresh-spend` and `--debug` stopped being
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

/// The reference payload from contract §12.
const FIXTURE: &str = r#"{"model":{"display_name":"Opus 4.8"},"effort":{"level":"high"},
"session_id":"abc123","session_name":"users-and-groups","workspace":{"current_dir":"/tmp/demo"},
"cost":{"total_cost_usd":46.51,"total_duration_ms":33540000},
"context_window":{"used_percentage":26,"context_window_size":1000000,"total_input_tokens":259000},
"rate_limits":{"five_hour":{"used_percentage":7,"resets_at":1774200000},
"seven_day":{"used_percentage":1.0,"resets_at":1774600000}}}"#;

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

/// The reference subagent payload from contract §12.
const SUBAGENT_FIXTURE: &str = r#"{"columns":120,"tasks":[{"id":"t1","name":"reviewer",
"type":"review","status":"running","description":"Auditing auth flow","tokenCount":18234}]}"#;

#[test]
fn the_subagent_fixture_renders_ndjson_that_survives_a_json_parser() {
    let home = Home::new(&safe_config());
    let out = run(&home, &["--subagent"], SUBAGENT_FIXTURE, &[]);

    assert!(out.status.success(), "exit code {:?}", out.status.code());
    let panel = stdout(&out);
    assert_eq!(panel.lines().count(), 1);
    assert!(!panel.ends_with('\n'), "no trailing newline");
    assert_eq!(stderr(&out), "", "a clean panel says nothing");

    // What `jq -r .content` does, which is how the contract says to read it.
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

/// Criterion 5, whole: the name applies, the other key does not, and `--debug`
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

    // And the user is told, because `--debug` is the only place they can find
    // out why the file they wrote is doing nothing.
    let report = stdout(&run_in(&["--debug"], "", Some(home.path()), Some(repo.path()), &[]));
    let line = report
        .lines()
        .find(|l| l.trim_start().starts_with("ignored"))
        .unwrap_or_else(|| panic!("--debug never named the ignored key:\n{report}"));
    assert!(line.contains("gauge"), "{line:?}");
    assert!(line.contains("projectName"), "it says what the file IS allowed to set: {line:?}");
    assert!(!line.contains("$schema"), "a `$schema` pointer is not an ignored setting: {line:?}");
}

/// A repo config carrying only what it may set says nothing extra — otherwise
/// every correctly written file would put a line in `--debug`.
#[test]
fn a_well_formed_repo_config_is_reported_as_ignoring_nothing() {
    let home = Home::new(&safe_config());
    let repo = fake_repo(r#"{ "$schema": "https://example.invalid/s.json", "projectName": "tidy" }"#);

    let report = stdout(&run_in(&["--debug"], "", Some(home.path()), Some(repo.path()), &[]));
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
    for args in [&["--version"][..], &["--version", "--debug"]] {
        let out = run(&home, args, "", &[]);
        assert_eq!(stdout(&out), format!("{}\n", env!("CARGO_PKG_VERSION")), "{args:?}");
        assert_eq!(stderr(&out), "", "{args:?} must not narrate");
    }
}

#[test]
fn debug_is_a_modifier_that_never_changes_stdout() {
    let home = Home::new(&safe_config());
    let plain = run(&home, &["--statusline"], FIXTURE, &[]);
    let debug = run(&home, &["--statusline", "--debug"], FIXTURE, &[]);

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

#[test]
fn help_lists_both_surfaces() {
    let home = Home::new(&safe_config());
    let out = run(&home, &["--help"], "", &[]);
    let text = stdout(&out);
    assert!(text.lines().count() > 5);
    assert!(text.contains("--statusline") && text.contains("--subagent") && text.contains("--refresh-spend"));
}

#[test]
fn debug_alone_reports_layers_wiring_layout_and_git() {
    let home = Home::new(&safe_config());
    let out = run(&home, &["--debug"], "", &[]);
    let text = stdout(&out);

    for section in ["CONFIG LAYERS", "CLAUDE WIRING", "EFFECTIVE LAYOUT", "GIT", "SAMPLE RENDER"] {
        assert!(text.contains(section), "missing {section}");
    }
    assert!(
        text.contains(".config/claude-status/config.json"),
        "it names the user config path it looked at: {text}",
    );
    assert!(out.status.success());

    // The `SAMPLE RENDER` carve-out contract §4a calls load-bearing: that
    // section is appended **after** the report-wide sweep precisely so its SGR
    // codes survive. Asserting the header alone would pass with an empty body,
    // and would still pass if the sweep were moved to cover it — which would
    // silently strip the colours the section exists to show.
    let sample = text.split("SAMPLE RENDER").nth(1).expect("the section is present");
    assert!(sample.contains('\u{1b}'), "the sample render lost its colour: {}", sample.escape_debug());
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
    let home = Home::new(&safe_config());
    let out = run(&home, &["--refresh-spend"], FIXTURE, &[]);
    assert_eq!(stdout(&out), "", "--refresh-spend writes nothing to stdout");
    assert!(out.status.success(), "--refresh-spend exits 0");
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
    let out = run(&home, &["--statusline", "--debug"], "{\"garbage\":", &[]);

    let bar = stdout(&out);
    for noise in ["claude-status:", "unknown segment", "config layer", "error"] {
        assert!(!bar.contains(noise), "{noise:?} leaked onto stdout: {}", bar.escape_debug());
    }
    assert!(!stderr(&out).is_empty(), "the diagnostics did happen — just not on stdout");
}

/// Runs the binary with **no `$HOME` at all**, which is the case the contract's
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
/// wording forbids and neither of which this cycle may touch — §8 is a live
/// contract with another repository and the plan puts it explicitly out of
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
        (&["--refresh-spend"][..], "", Child::NotExpected),
        (&["--debug"][..], "", Child::NotExpected),
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

        // The §8 carve-out, deliberately outside `$HOME` so the two are
        // separable: whatever lands here is the usage mirror, and whatever
        // lands under `$HOME` outside the cache is a violation.
        let usage = TempDir::new().unwrap();
        let env = [("AI_PLUGINS_USAGE_DIR", usage.path().to_str().unwrap())];

        let out = run_in(args, stdin, Some(home.path()), Some(repo.path()), &env);
        assert!(out.status.success(), "{args:?} exited {:?}", out.status.code());

        // After the child, not before it. A mode that wrote nothing itself but
        // spawned something that did would otherwise pass.
        settle(home.path(), spawns);
        assert_eq!(outside_cache(home.path()), before, "{args:?} wrote under $HOME outside the cache");
        assert_eq!(snapshot(repo.path()), repo_before, "{args:?} wrote inside the repo");
    }

    // Not vacuous: §8's writer has to be live, or every assertion above holds
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
    run_without_home(&["--refresh-spend"], "", dir.path(), &[]);

    let after: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(after, vec![marker.file_name().unwrap()], "something was written: {after:?}");
    assert!(!dir.path().join("spend.json").exists(), "the spend cache went relative");
    assert!(!dir.path().join("~").exists(), "the usage mirror wrote into a directory named `~`");
}

#[test]
fn with_no_home_debug_names_the_missing_variable() {
    // `--debug` exists to say what is wrong; an empty SPEND section would be
    // the useless answer the user already had.
    let dir = TempDir::new().unwrap();
    let out = run_without_home(&["--debug"], "", dir.path(), &[]);

    // Scoped to the SPEND section. A bare `contains("$HOME")` over the whole
    // report is satisfied by the `user  not found  <no $HOME>` row in CONFIG
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
fn debug_reports_a_hostile_config_without_obeying_it() {
    // `--debug` is the fourth surface contract §4a names. Two of the values
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
    // fails to parse, the layer loads as "not found", and the assertions below
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

    let out = run_in(&["--debug"], "", Some(home.path()), Some(repo.path()), &[]);

    let report = stdout(&out);
    // SAMPLE RENDER is renderer output and legitimately carries SGR codes, so
    // assert against everything before it.
    let diagnostics = report.split("SAMPLE RENDER").next().unwrap();
    // Proof the fixtures actually landed. Without this the layer can fail to
    // parse, load as "not found", and the escape assertion below passes having
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
fn a_config_cannot_forge_lines_in_the_debug_report() {
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

    let out = run_in(&["--debug"], "", Some(home.path()), None, &[]);
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
fn a_repo_layers_ignored_key_names_cannot_forge_lines_in_the_debug_report() {
    let home = Home::new(&safe_config());
    let repo = fake_repo(
        &serde_json::json!({
            "projectName": "victim",
            "gauge\n  VERDICT  everything is fine, nothing to see": 1,
            "caps\nCLAUDE WIRING (~/.claude/settings.json)\n  statusLine: FORGED": 1,
        })
        .to_string(),
    );

    let report = stdout(&run_in(&["--debug"], "", Some(home.path()), Some(repo.path()), &[]));
    let diagnostics = report.split("SAMPLE RENDER").next().unwrap();

    let forged = diagnostics.lines().find(|l| l.contains("FORGED")).expect("the key is still reported");
    assert!(forged.trim_start().starts_with("ignored"), "it broke out onto its own line: {forged:?}");

    let starts_with = |needle: &str| diagnostics.lines().filter(|l| l.trim_start().starts_with(needle)).count();
    assert_eq!(starts_with("CLAUDE WIRING"), 1, "a section header was forged: {diagnostics}");
    assert_eq!(starts_with("VERDICT"), 1, "a VERDICT line was forged: {diagnostics}");
}
