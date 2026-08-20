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
//! here neutralises it three ways at once: `spend.refreshMinutes = 0` in the
//! seeded config, `CLAUDE_STATUS_SPEND_URL` pointed at a closed port, and a
//! pre-seeded fresh cache. The spend subsystem does not exist yet (plan 3), and
//! the point is that the harness is already safe for when it does.

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

        // A fourth neutralisation, needed now that `--refresh-spend` is real:
        // the keychain is not scoped by `$HOME`, so a home with no credentials
        // file falls through to the user's actual token.
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
        // Three-way neutralisation; see the module docs.
        .env("CLAUDE_STATUS_SPEND_URL", CLOSED_PORT_URL)
        .env("AI_PLUGINS_SPEND_URL", CLOSED_PORT_URL)
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
fn the_non_rendering_surfaces_are_recognised_and_silent() {
    // `--refresh-spend` now does real work, but it is spawned with its stdio
    // at /dev/null, so writing anything would be pointless — it stays silent.
    let home = Home::new(&safe_config());
    for flag in ["--subagent", "--refresh-spend"] {
        let out = run(&home, &[flag], FIXTURE, &[]);
        assert_eq!(stdout(&out), "", "{flag} writes nothing to stdout");
        assert!(out.status.success(), "{flag} exits 0");
    }
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
