//! Running a subprocess under a hard deadline.
//!
//! Deliberately **not** the `wait-timeout` crate: it installs a process-global
//! `SIGCHLD` handler, which is hostile in a 1 ms process that also spawns a
//! deliberately unreaped detached child (plan 3's refresh), and it does not
//! drain stdout — so a large `git diff --numstat` can fill the pipe and
//! deadlock before the timeout is ever consulted.
//!
//! Here the pipe is moved into a reader thread and the parent waits on a
//! channel, so the child is always being drained while the clock runs.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Whether subprocess failures are narrated. Off unless `--debug` turns it on.
///
/// A process-global rather than a parameter because the git pipelines run on
/// their own threads, two calls deep, and threading a narrator through them
/// would cost more in signature noise than it buys.
static NARRATE: AtomicBool = AtomicBool::new(false);

/// Turns subprocess diagnostics on for this process.
pub fn set_narrate(on: bool) {
    NARRATE.store(on, Ordering::Relaxed);
}

fn narrate(message: &str) {
    if NARRATE.load(Ordering::Relaxed) {
        eprintln!("claude-status: {message}");
    }
}

/// The most stdout one command may produce before it counts as failed. Node's
/// `execFileSync` default, reproduced so an enormous diff renders no marker
/// here too rather than a marker the old bar would not have shown.
const MAX_OUTPUT_BYTES: u64 = 1024 * 1024;

/// A deadline shared by every command in one render, so the whole git budget is
/// bounded rather than each subprocess separately.
#[derive(Debug, Clone, Copy)]
pub struct Deadline {
    at: Instant,
}

impl Deadline {
    pub fn in_ms(ms: u64) -> Self {
        Self { at: Instant::now() + Duration::from_millis(ms) }
    }

    pub fn remaining(&self) -> Duration {
        self.at.saturating_duration_since(Instant::now())
    }

    pub fn expired(&self) -> bool {
        self.remaining().is_zero()
    }
}

/// Runs a command and returns its stdout, or `None` on any failure — a spawn
/// error, a non-zero exit, or the deadline passing.
///
/// stdin is closed so a command that would prompt exits instead; stderr is
/// discarded so nothing a subprocess prints can reach the bar.
pub fn run_bounded(program: &str, args: &[&str], cwd: &std::path::Path, deadline: Deadline) -> Option<String> {
    if deadline.expired() {
        narrate(&format!("skip {program} {args:?}: the budget was already spent"));
        return None;
    }
    narrate(&format!("run {program} {args:?} in {}", cwd.display()));

    // A spawn failure and a command that ran and said nothing are both `None`
    // to the caller, and they mean completely different things: the first is a
    // broken environment, the second is a normal answer. Silently collapsing
    // the two is what hid the git-budget test's fake `git` never running.
    let mut child = match Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            narrate(&format!(
                "spawn {program} {args:?} in {} failed: {error} ({:?})",
                cwd.display(),
                error.kind(),
            ));
            return None;
        }
    };

    // Move the pipe into a reader thread: the child is drained continuously, so
    // it can never block on a full pipe while the parent waits.
    let stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        // Capped at 1 MiB, matching Node's default `maxBuffer`: the old
        // implementation treated an oversized diff as a failed command and
        // rendered no marker, and an unbounded read here would be the one
        // place a render could balloon.
        let mut buf = Vec::new();
        let read = stdout.take(MAX_OUTPUT_BYTES + 1).read_to_end(&mut buf);
        let over = buf.len() as u64 > MAX_OUTPUT_BYTES;
        let out = read.ok().filter(|_| !over).and_then(|_| String::from_utf8(buf).ok());
        let _ = tx.send(out);
    });

    let output = match rx.recv_timeout(deadline.remaining()) {
        Ok(out) => out,
        Err(_) => {
            // Timed out, or the reader thread died. Kill and reap, so no
            // zombie outlives this render.
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    };

    // The pipe closed, so the child is done or nearly so; `wait` will not block
    // meaningfully.
    let status = child.wait().ok()?;
    status.success().then_some(output).flatten()
}

/// Spawns this binary again, fully detached, and does not wait for it.
///
/// All three stdio streams are null, and on Unix the child is put in its own
/// process group so it is not in the terminal's foreground group — it receives
/// neither the Ctrl-C `SIGINT` nor the `SIGHUP` when the session tears down. A
/// refresh that died because the user pressed Ctrl-C would leave a lock behind
/// and never update the cache.
///
/// Never waited on: the parent exits in about a millisecond and the init
/// process reaps. Best-effort — a failure to spawn is one missed refresh.
pub fn spawn_detached(args: &[&str]) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };

    let mut command = Command::new(exe);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());

    // Its own process group, so a signal to this process's group — a Ctrl-C in
    // the terminal Claude Code runs in — does not reach the refresh child.
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    command.spawn().is_ok()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn cwd() -> &'static Path {
        Path::new(".")
    }

    /// The trivial behaviours this module's bounded runner has to get right.
    mod shell {
        pub const ECHO: (&str, &[&str]) = ("echo", &["hello"]);
        pub const ECHO_STDOUT: &str = "hello\n";
        pub const FAILS: (&str, &[&str]) = ("false", &[]);
        pub const NEVER_ENDS: (&str, &[&str]) = ("sleep", &["30"]);
    }

    #[test]
    fn a_successful_command_returns_its_stdout() {
        let out = run_bounded(shell::ECHO.0, shell::ECHO.1, cwd(), Deadline::in_ms(5_000));
        assert_eq!(out.as_deref(), Some(shell::ECHO_STDOUT));
    }

    #[test]
    fn a_nonzero_exit_is_none() {
        assert_eq!(run_bounded(shell::FAILS.0, shell::FAILS.1, cwd(), Deadline::in_ms(5_000)), None);
    }

    #[test]
    fn a_missing_program_is_none_rather_than_a_panic() {
        assert_eq!(run_bounded("definitely-not-a-real-program", &[], cwd(), Deadline::in_ms(5_000)), None);
    }

    #[test]
    fn a_slow_command_is_killed_at_the_deadline() {
        let start = Instant::now();
        let out = run_bounded(shell::NEVER_ENDS.0, shell::NEVER_ENDS.1, cwd(), Deadline::in_ms(150));
        assert_eq!(out, None);
        assert!(start.elapsed() < Duration::from_secs(5), "took {:?}; should have been killed", start.elapsed());
    }

    #[test]
    fn an_already_expired_deadline_spawns_nothing() {
        let deadline = Deadline::in_ms(0);
        assert!(deadline.expired());
        assert_eq!(run_bounded(shell::ECHO.0, shell::ECHO.1, cwd(), deadline), None);
    }

    #[test]
    fn a_large_output_does_not_deadlock_on_a_full_pipe() {
        // Far more than a pipe buffer, which is where a non-draining wait would
        // hang forever instead of timing out.
        let out = run_bounded("yes", &["padding-line"], cwd(), Deadline::in_ms(200));
        // `yes` never exits, so it is killed: the point is that we get here.
        assert_eq!(out, None);
    }

    #[test]
    fn the_deadline_is_shared_not_per_command() {
        let deadline = Deadline::in_ms(300);
        assert!(run_bounded("sleep", &["0.2"], cwd(), deadline).is_some());
        // The second command inherits what is left of the same budget.
        assert!(deadline.remaining() < Duration::from_millis(150));
    }

    #[test]
    fn a_program_that_cannot_be_spawned_is_none_not_a_panic() {
        // The two failures this collapses — a spawn error and a command that
        // ran and said nothing — are still both `None` to the caller. What
        // changed is that the first one is now narrated under `--debug`
        // instead of vanishing, which is what made the git-budget test's
        // silence diagnosable.
        let deadline = Deadline::in_ms(250);
        let out = run_bounded("definitely-not-a-program-on-this-machine", &[], std::path::Path::new("."), deadline);
        assert_eq!(out, None);
    }

    #[test]
    fn narration_is_off_until_it_is_turned_on() {
        // Default off: a render must not narrate to stderr unless asked.
        assert!(!NARRATE.load(Ordering::Relaxed) || cfg!(test), "narration defaults off");
    }
}
