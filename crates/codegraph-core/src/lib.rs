//! Shared index, resolver, walker, and language registry for `codegraph`
//! (read-only cross-references) and `astedit` (write-side rename / structural
//! rewrite). Existing `codegraph` subcommands keep their behaviour; this crate
//! exists so `astedit` does not have to copy the parsing pipeline.
//!
//! Stability: every additive public type is marked `#[non_exhaustive]`. Add
//! fields freely; never delete or repurpose.

pub mod lang;
