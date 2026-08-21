//! claude-status — the Claude Code powerline status line.
//!
//! Invariants that outrank everything else (contract §1):
//!
//! 1. **stdout is the bar.** Every diagnostic goes to stderr.
//! 2. **A render never blocks.** No network call on the render path; git is
//!    hard-bounded.
//! 3. **A render never fails visibly.** A panic still produces a usable line.
//!
//! # Layout
//!
//! This file is the crate root and holds no logic. Rust hangs the module tree
//! off the root's location, so it has to sit here for `_shared/` and `modules/`
//! to be reachable without a `#[path]` attribute on every declaration.
//!
//! - `_runtime/` — the process boundary: argument parsing, the render
//!   pipeline, the diagnostic report, and the single write to stdout.
//! - `_shared/` — cross-cutting helpers with no domain knowledge.
//! - `modules/` — one folder or file per domain.
//!
//! The folder layout is an implementation detail, so those modules are private
//! and re-exported here. Consumers get one canonical path per module
//! (`claude_status::config`), not two.

mod _runtime;
mod _shared;
mod modules;

pub use _runtime::app::{render_bar, run};
pub use _runtime::cli;
pub use _shared::{fmt, json, proc, time};
pub use modules::{caps, config, git, payload, render, spend, usage};
