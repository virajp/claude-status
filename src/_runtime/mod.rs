//! The process boundary.
//!
//! Everything that touches the outside world — argv, stdin, stdout, stderr —
//! lives here, so the domain modules stay pure and testable.

pub mod app;
pub mod cli;
pub mod configure;
pub mod debug;
