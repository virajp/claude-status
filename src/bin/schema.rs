//! Writes `schemas/claude-status.schema.json`, or checks it for drift.
//!
//! Behind `required-features = ["schema"]`, so `cargo build` and every release
//! build skip this target entirely. `mise run code:schema` is the front door;
//! `--check` is what the pre-commit hook and `tests/schema.rs` call for.
//!
//! Two exit codes and nothing else: `0` when the committed file is what the
//! types produce, `1` when it is not — and on `1` it names the command that
//! fixes it, because a drift failure is only useful if the reader learns what
//! to run.

use std::path::PathBuf;
use std::process::ExitCode;

/// The committed schema, relative to the crate root.
///
/// `CARGO_MANIFEST_DIR` and not the current directory: `mise run` sets its own
/// cwd and a developer may run this from anywhere in the tree.
fn schema_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schemas").join("claude-status.schema.json")
}

fn main() -> ExitCode {
    let check = std::env::args().any(|arg| arg == "--check");
    let path = schema_path();
    let generated = claude_status::config::schema::render();

    if check {
        let committed = std::fs::read_to_string(&path).unwrap_or_default();
        if committed == generated {
            println!("schema up to date: {}", path.display());
            return ExitCode::SUCCESS;
        }
        eprintln!("{} has drifted from the config types.", path.display());
        eprintln!("Run `mise run code:schema` to regenerate it.");
        return ExitCode::FAILURE;
    }

    match std::fs::write(&path, &generated) {
        Ok(()) => {
            println!("wrote {}", path.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("could not write {}: {e}", path.display());
            ExitCode::FAILURE
        }
    }
}
