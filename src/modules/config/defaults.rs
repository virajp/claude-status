//! The shipped defaults, embedded in the binary.
//!
//! This is the lowest of the three config layers, and since the
//! `config-relocation` cycle it is also the **only one that has to exist**: a
//! machine with no config file anywhere renders a full bar, and that is a
//! supported, tested state rather than a degraded one. (The JS implementation
//! had only two file layers and rendered blank without them — see the plan's
//! deviation table.)
//!
//! The asset began as a byte-faithful copy of the JS bar's config and has
//! since diverged — it gained `caps`, and it dropped `autoConfigureRepo` when
//! the render-path write was removed. It is nonetheless **irreplaceable**: 28
//! of its symbols are Nerd Font private-use
//! codepoints that render as nothing or a box in most editors and are silently
//! dropped by copy-paste. It is `-text -diff` in `.gitattributes` and excluded
//! from dprint. Never edit it through an editor buffer; verify it by rendering,
//! or through `tests/defaults_integrity.rs`, never by reading a diff.

/// The shipped defaults as raw JSON text.
pub const DEFAULTS_JSON: &str = include_str!("../../../assets/claude-status.defaults.json");
