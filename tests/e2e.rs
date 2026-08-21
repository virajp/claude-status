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
        let config_dir = dir.path().join(".config");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(config_dir.join("claude-status.json"), config).unwrap();

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
    let mut cmd = Command::new(BINARY);
    cmd.args(args)
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        // See the module docs on the spend hazard.
        .env("CLAUDE_STATUS_SPEND_URL", CLOSED_PORT_URL)
        .env("CLAUDE_STATUS_SPEND_CACHE", home.path().join(".cache").join("claude-status").join("spend.json"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }

    let mut child = cmd.spawn().expect("the binary runs");
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
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
fn a_repo_config_may_tighten_a_cap_but_never_loosen_one() {
    let home = Home::new(&safe_config());
    let usage = home.path().join("usage");
    seed_mirror(&usage, "s1", r#"{"ctxPct":55}"#);

    // 55% is under the shipped 65% cap, so nothing fires...
    let repo = home.path().join("repo");
    std::fs::create_dir_all(repo.join(".config")).unwrap();
    let stdin = format!(r#"{{"session_id":"s1","cwd":"{}"}}"#, repo.display());
    assert_eq!(stdout(&caps_run(&home, &usage, &stdin, "CLAUDE_STATUS_USAGE_DIR")), "");

    // ...until the repo tightens it to 50.
    std::fs::write(repo.join(".config").join("vwf.yaml"), "pipeline:\n  execute_caps:\n    context: 50\n").unwrap();
    let tightened = stdout(&caps_run(&home, &usage, &stdin, "CLAUDE_STATUS_USAGE_DIR"));
    assert!(tightened.contains("cap 50%"), "the repo value applied: {tightened}");

    // A value above the shipped default is ignored: 70% still breaches at 65.
    std::fs::remove_file(usage.join("s1.state.json")).unwrap();
    seed_mirror(&usage, "s1", r#"{"ctxPct":70}"#);
    std::fs::write(repo.join(".config").join("vwf.yaml"), "pipeline:\n  execute_caps:\n    context: 90\n").unwrap();
    let ignored = stdout(&caps_run(&home, &usage, &stdin, "CLAUDE_STATUS_USAGE_DIR"));
    assert!(ignored.contains("cap 65%"), "config may only tighten: {ignored}");
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
        assert_eq!(stdout(&out), "6.0.0\n", "{args:?}");
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
    assert!(text.contains("claude-status.json"), "it names the config path it looked at");
    assert!(out.status.success());
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
    assert!(bar.contains("Project-Name"), "the embedded default came through");
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
    // Every value here is the wrong type or an absurd size. The bar must not
    // go blank and must not hang.
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
}

#[test]
fn a_hanging_git_costs_one_shared_budget_not_one_per_subprocess() {
    // The whole git budget is 250 ms *shared*. Run against a `git` that never
    // returns: sequentially at 250 ms each, the four subprocesses the dirty and
    // ahead pipelines can issue would cost about a second. This asserts the
    // shared deadline, and that the render still completes.
    let home = Home::new(&safe_config());

    // A `git` shim that hangs, ahead of the real one on PATH.
    let shim_dir = TempDir::new().unwrap();
    let shim = shim_dir.path().join("git");
    std::fs::write(&shim, "#!/bin/sh\nsleep 30\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

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
    assert!(
        elapsed < std::time::Duration::from_millis(900),
        "took {elapsed:?}; a shared 250 ms budget should not approach the ~1 s a per-subprocess timeout would cost",
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
