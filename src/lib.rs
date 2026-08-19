//! claude-status — the Claude Code powerline status line.
//!
//! Invariants that outrank everything else (contract §1):
//!
//! 1. **stdout is the bar.** Every diagnostic goes to stderr.
//! 2. **A render never blocks.** No network call on the render path; git is
//!    hard-bounded.
//! 3. **A render never fails visibly.** A panic still produces a usable line.

pub mod config;
pub mod json;
