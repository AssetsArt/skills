//! Internal library crate. The astedit binary at `src/main.rs` re-uses these
//! modules, and integration tests under `tests/` consume them directly for
//! cases that would otherwise need test-only back-doors in the CLI.

pub mod apply;
pub mod cli;
pub mod commands;
pub mod error;
pub mod output;
pub mod rewrite;
pub mod serialize;
