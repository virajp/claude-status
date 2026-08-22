//! Rendering: the powerline row, the segment builders, and the main bar.

pub mod main_bar;
pub mod powerline;
pub mod segments;
pub mod subagent;

// The escape filter itself is domain-free and lives in `_shared/text.rs`; it is
// re-exported here because every caller but the stderr chokepoint reaches it
// through the renderer, and contract §4a names it as a rendering rule.
pub use crate::_shared::text::{sanitize, sanitize_report};
