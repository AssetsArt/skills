# astedit PR 2 — Rename MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the `astedit rename <OLD> <NEW>` MVP — a new binary crate (`crates/astedit/`) and matching skill (`skills/ny-astedit/`) that performs cross-file symbol renames driven by the existing `codegraph-core` index, with dry-run-by-default safety, atomic per-file writes, length-based drift detection with SHA-256 fallback, a race-window guard, and a `{schema_version:1, data:…}` JSON envelope. `rewrite` is out of scope and lands in PR 3.

**Architecture:** astedit composes three things: (1) `codegraph_core::build_index` to find references, (2) `codegraph_core::resolve_refs` to rank them by confidence, and (3) byte-level splicing (no AST re-parse) at each reference's pre-computed `byte_offset`. The pipeline groups edits by file, pre-flights file length against `Index.file_meta`, applies via temp-file + `rename(2)`, and emits a structured envelope. Errors map 1:1 to a closed `AstEditError` enum that serializes to the six spec-locked `error_kind` strings via `thiserror` + `#[serde(rename_all = "kebab-case")]`. No new `codegraph-core` surface is added; PR 1 already landed everything astedit needs.

**Tech Stack:** Rust 2021, Cargo workspace member, `codegraph-core` (path dep), `ast-grep-core`/`-config`/`-language` (exact-pinned in workspace, declared in `astedit/Cargo.toml` per spec but unused at runtime in PR 2 — PR 3 consumes them), `clap` 4.5, `serde`, `serde_json`, `anyhow`, `thiserror`, `tempfile`.

**Branch:** `feat/astedit-rename` (already checked out from `main`, no commits yet). All commits in this plan land on that branch; the PR opens against `main` after Task 19 passes.

**Test baseline going in:** 48 tests pass on `main` (13 codemap + 18 codegraph + 17 codegraph-core). PR 2 adds 18 tests (10 rename integration tests covering the handoff matrix + 1 extra `--apply` write test, 4 `apply::*` unit tests, 3 serialization unit tests, 1 `AstEditError::kind` unit test) without editing any of the existing 48.

---

## File Structure

**Files being created:**

```
crates/astedit/
  Cargo.toml
  src/
    main.rs                    — thin dispatcher
    cli.rs                     — clap structs (Cli, Command, RenameArgs)
    output.rs                  — print_json envelope helper
    error.rs                   — AstEditError enum (six kebab-case variants)
    serialize.rs               — JSON payload structs (Envelope data shape)
    apply.rs                   — atomic write + drift detection helpers
    commands/
      mod.rs
      rename.rs                — the rename pipeline
  tests/
    common/
      mod.rs                   — copy_fixture() helper
    rename_test.rs             — 9 integration tests
    fixtures/
      same_file/               — Task 8 fixture (Rust, same-file def + use)
      cross_file_import/       — Task 9 fixture (Python, explicit import)
      glob_import/             — Task 10 fixture (Rust glob)
      name_only/               — Task 11 fixture (unrelated file)
      alias_reexport/          — Task 12 fixture
      wildcard_reexport/       — Task 13 fixture
      multi_def/               — Tasks 14 + 15 fixture
      apply_write/             — Tasks 17 + 18 fixture

skills/ny-astedit/
  SKILL.md                     — "PREFER THIS over manual rename" template
  scripts/astedit              — release binary, copied by build-skills.sh
```

**Files being modified:**

- `Cargo.toml` (workspace root): add `ast-grep-core`, `ast-grep-config`, `ast-grep-language` to `[workspace.dependencies]`, each pinned exact (`= "x.y.z"`).

That is the complete edit list for files outside `crates/astedit/` and `skills/ny-astedit/`. **No edits to `codegraph-core`, `codegraph`, or `codemap`.** If during implementation a missing helper looks like it belongs in `codegraph-core`, stop and flag — the handoff's out-of-scope notes apply.

---

## Task 1: Add `ast-grep-*` workspace deps (exact-pinned)

**Files:**
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Resolve the latest 0.38.x patch version**

`ast-grep-core` and its sibling crates ship breaking changes in minor versions. The spec requires `= "0.38.x"` exact pinning. Resolve the concrete patch:

```bash
cargo search ast-grep-core --limit 1
```

Expected: output line like `ast-grep-core = "0.38.6"   # …`. Capture the exact version string. Confirm `ast-grep-config` and `ast-grep-language` ship matching versions:

```bash
cargo search ast-grep-config --limit 1
cargo search ast-grep-language --limit 1
```

If the three crates do not all expose the same patch number, use the **lowest common patch** they all publish — they're tightly coupled and a version skew between them is a known footgun.

For the rest of this plan, the placeholder `0.38.6` is used wherever the exact version appears. Substitute the resolved version before committing.

- [ ] **Step 2: Add to `[workspace.dependencies]`**

Open the workspace `Cargo.toml`. Append three lines to `[workspace.dependencies]`, after the existing `thiserror = "1"` line:

```toml
ast-grep-core     = "=0.38.6"
ast-grep-config   = "=0.38.6"
ast-grep-language = "=0.38.6"
```

(The `=` prefix is significant — it pins exactly, rejecting any caret/tilde expansion.)

- [ ] **Step 3: Resolve the lockfile and audit for `tree-sitter` duplication**

```bash
cargo update -w --locked --dry-run
```

If `--dry-run` reports the lockfile would change, re-run without `--dry-run`:

```bash
cargo update -w
```

Then audit for duplicates that could cause tree-sitter symbol collisions with the grammars already present in `codegraph-core`:

```bash
cargo tree -d
```

Expected: no duplicate `tree-sitter` entries. If the output shows `tree-sitter` v0.X appearing twice (once from `codegraph-core` grammars, once from `ast-grep-language`), stop. The remediation is to align `tree-sitter` versions or pick a different `ast-grep-*` patch — do not proceed to Task 2 with a duplicated `tree-sitter`.

- [ ] **Step 4: Verify the workspace still compiles**

```bash
cargo check --workspace --locked
```

Expected: success. The new deps land in `Cargo.lock` but no crate consumes them yet.

- [ ] **Step 5: Verify the existing 48 tests still pass**

```bash
cargo test --workspace --locked
```

Expected: `48 passed`. If any pre-existing test broke from a transitive dep change, revert the new lines and investigate before continuing.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore(workspace): pin ast-grep-{core,config,language} for astedit"
```

---

## Task 2: Create empty `crates/astedit/` crate

**Files:**
- Create: `crates/astedit/Cargo.toml`
- Create: `crates/astedit/src/main.rs`

- [ ] **Step 1: Make the crate skeleton**

```bash
mkdir -p crates/astedit/src
```

Write `crates/astedit/Cargo.toml`:

```toml
[package]
name = "astedit"
description = "AST-validated rename and structural rewrite tool for AI coding agents."
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true
homepage.workspace = true

[[bin]]
name = "astedit"
path = "src/main.rs"

[dependencies]
codegraph-core    = { path = "../codegraph-core" }
clap              = { workspace = true }
serde             = { workspace = true }
serde_json        = { workspace = true }
anyhow            = { workspace = true }
thiserror         = { workspace = true }

[dev-dependencies]
tempfile = "3"
```

**Deviation from spec:** the spec § "astedit/Cargo.toml" lists `ast-grep-{core,config,language}` as deps of astedit. We dropped them because `ast-grep-core 0.38.7` requires `tree-sitter ^0.25.4` while `codegraph-core` uses `tree-sitter 0.22`, and Cargo's `links = "tree-sitter"` constraint disallows both versions in one workspace. The three lines stay in workspace.dependencies (Task 1) for PR 3 to consume after resolving the tree-sitter version mismatch.

- [ ] **Step 2: Write the minimal main.rs**

Write `crates/astedit/src/main.rs`:

```rust
fn main() -> anyhow::Result<()> {
    Ok(())
}
```

- [ ] **Step 3: Verify the new crate is picked up**

```bash
cargo check -p astedit --locked
```

Expected: success. The `members = ["crates/*"]` glob in the workspace root grabs the new directory automatically.

- [ ] **Step 4: Verify the binary builds and runs (does nothing)**

```bash
cargo build -p astedit --locked
./target/debug/astedit
echo "exit=$?"
```

Expected: `exit=0`, no output.

- [ ] **Step 5: Commit**

```bash
git add crates/astedit/
git commit -m "feat(astedit): scaffold empty binary crate"
```

---

## Task 3: Scaffold `skills/ny-astedit/` skill dir

**Files:**
- Create: `skills/ny-astedit/SKILL.md`

`build-skills.sh` iterates `skills/ny-*/` and copies `target/release/<name>` into `skills/ny-<name>/scripts/<name>`. The scripts directory is `.gitignore`d. We only need to commit `SKILL.md` here; Task 20 finalizes its description text. For now a stub is enough to keep `build-skills.sh` happy.

- [ ] **Step 1: Create the directory and a placeholder SKILL.md**

```bash
mkdir -p skills/ny-astedit
```

Write `skills/ny-astedit/SKILL.md` (placeholder — final content lands in Task 20):

```markdown
---
name: ny-astedit
description: AST-validated rename and structural rewrite for AI coding agents. Placeholder description — finalized in Task 20.
---

# astedit

Placeholder. Final SKILL.md content is written in Task 20 of `docs/superpowers/plans/2026-05-21-astedit-pr2-rename.md`.
```

- [ ] **Step 2: Build skills and confirm astedit is picked up**

```bash
./scripts/build-skills.sh
```

Expected output includes a line `built skills/ny-astedit/scripts/astedit` and the summary `done: 3 skill binary(ies)`. If the script complained about `target/release/astedit` missing, re-run `cargo build --release -p astedit --locked` then re-run the script.

- [ ] **Step 3: Verify the binary is in place**

```bash
ls skills/ny-astedit/scripts/astedit
./skills/ny-astedit/scripts/astedit
echo "exit=$?"
```

Expected: file exists and is executable; running it exits 0 with no output (it's still the empty stub).

- [ ] **Step 4: Commit**

```bash
git add skills/ny-astedit/SKILL.md
git commit -m "feat(skills): scaffold ny-astedit skill dir"
```

---

## Task 4: Define `AstEditError` enum

**Files:**
- Create: `crates/astedit/src/error.rs`

The enum is the single source of truth for the six `error_kind` strings in the JSON envelope. Variants serialize via `#[serde(rename_all = "kebab-case")]` on a companion `error_kind` accessor so the JSON contract is mechanical, not hand-typed in multiple places.

- [ ] **Step 1: Write the failing unit test**

Append a `#[cfg(test)]` block to `crates/astedit/src/error.rs` (the file is created below; the test gets compiled with the source). The test asserts every variant's `error_kind` string matches the spec:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_kind_strings_match_spec() {
        assert_eq!(AstEditError::ParseError { file: "x".into(), message: "y".into() }.kind(), "parse-error");
        assert_eq!(AstEditError::HashMismatch { file: "x".into() }.kind(), "hash-mismatch");
        assert_eq!(AstEditError::ConcurrentWrite { file: "x".into() }.kind(), "concurrent-write");
        assert_eq!(AstEditError::NodeKindMismatch { file: "x".into(), line: 1, col: 1 }.kind(), "node-kind-mismatch");
        assert_eq!(AstEditError::WriteFailed { file: "x".into(), os_code: None, message: "y".into() }.kind(), "write-failed");
        assert_eq!(AstEditError::PatternCompile { lang: "rust".into(), message: "y".into() }.kind(), "pattern-compile");
    }
}
```

- [ ] **Step 2: Run to confirm it fails to compile**

```bash
cargo test -p astedit --locked
```

Expected: compile error — `AstEditError` doesn't exist yet. (The whole `error.rs` file is missing; `cargo` will complain about an unknown module the moment we add the `mod` line in Step 4.)

- [ ] **Step 3: Implement `error.rs`**

Write `crates/astedit/src/error.rs`:

```rust
use thiserror::Error;

/// Closed enum of every condition astedit reports through the `errors[]`
/// lane of the JSON envelope. Variant names map 1:1 to the spec-locked
/// `error_kind` strings via `kind()` — used by `crate::serialize::ErrorEntry`
/// to emit the JSON. We deliberately do not derive `Serialize`: the public
/// JSON shape lives in `crate::serialize`, which builds it explicitly so
/// the wire schema cannot drift from variant renaming.
#[derive(Debug, Error)]
pub enum AstEditError {
    #[error("parse error in {file}: {message}")]
    ParseError { file: String, message: String },

    #[error("file changed between index and apply: {file}")]
    HashMismatch { file: String },

    #[error("concurrent write detected on {file}")]
    ConcurrentWrite { file: String },

    #[error("node kind mismatch at {file}:{line}:{col}")]
    NodeKindMismatch { file: String, line: usize, col: usize },

    #[error("write failed on {file}: {message}")]
    WriteFailed { file: String, os_code: Option<i32>, message: String },

    #[error("pattern failed to compile for {lang}: {message}")]
    PatternCompile { lang: String, message: String },
}

impl AstEditError {
    /// The kebab-case string emitted as `error_kind` in the JSON envelope.
    pub fn kind(&self) -> &'static str {
        match self {
            AstEditError::ParseError { .. }       => "parse-error",
            AstEditError::HashMismatch { .. }     => "hash-mismatch",
            AstEditError::ConcurrentWrite { .. }  => "concurrent-write",
            AstEditError::NodeKindMismatch { .. } => "node-kind-mismatch",
            AstEditError::WriteFailed { .. }      => "write-failed",
            AstEditError::PatternCompile { .. }   => "pattern-compile",
        }
    }

    /// The repo-relative file path the error is attributed to, if any.
    /// `PatternCompile` is not file-scoped (it errors before any file is
    /// touched), so returns `None` for that variant.
    pub fn file(&self) -> Option<&str> {
        match self {
            AstEditError::ParseError { file, .. }
            | AstEditError::HashMismatch { file }
            | AstEditError::ConcurrentWrite { file }
            | AstEditError::NodeKindMismatch { file, .. }
            | AstEditError::WriteFailed { file, .. } => Some(file),
            AstEditError::PatternCompile { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_kind_strings_match_spec() {
        assert_eq!(AstEditError::ParseError { file: "x".into(), message: "y".into() }.kind(), "parse-error");
        assert_eq!(AstEditError::HashMismatch { file: "x".into() }.kind(), "hash-mismatch");
        assert_eq!(AstEditError::ConcurrentWrite { file: "x".into() }.kind(), "concurrent-write");
        assert_eq!(AstEditError::NodeKindMismatch { file: "x".into(), line: 1, col: 1 }.kind(), "node-kind-mismatch");
        assert_eq!(AstEditError::WriteFailed { file: "x".into(), os_code: None, message: "y".into() }.kind(), "write-failed");
        assert_eq!(AstEditError::PatternCompile { lang: "rust".into(), message: "y".into() }.kind(), "pattern-compile");
    }
}
```

- [ ] **Step 4: Add `mod error;` to main.rs**

Edit `crates/astedit/src/main.rs` to:

```rust
mod error;

fn main() -> anyhow::Result<()> {
    Ok(())
}
```

- [ ] **Step 5: Run the test**

```bash
cargo test -p astedit --locked
```

Expected: 1 test passes (`error_kind_strings_match_spec`).

- [ ] **Step 6: Clippy**

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add crates/astedit/src/error.rs crates/astedit/src/main.rs
git commit -m "feat(astedit): add AstEditError enum mapped to spec error_kind strings"
```

---

## Task 5: Define JSON envelope payload structs

**Files:**
- Create: `crates/astedit/src/serialize.rs`
- Create: `crates/astedit/src/output.rs`

The serializable types are the contract with the agent. They live in a dedicated module so the JSON shape is auditable in one place.

- [ ] **Step 1: Write the failing test**

Create `crates/astedit/src/serialize.rs`:

```rust
use serde::Serialize;

/// The wire-format payload for `astedit rename`. Wrapped by `output::print_json`
/// in `{schema_version: 1, data: <RenameData>}`.
#[derive(Debug, Serialize)]
pub struct RenameData {
    pub subcommand: &'static str, // always "rename"
    pub dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needs_anchor: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<Candidate>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied: Option<Vec<AppliedFile>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<Vec<SkippedSite>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<ErrorEntry>>,
}

#[derive(Debug, Serialize)]
pub struct Candidate {
    pub file: String,
    pub line: usize,
    pub kind: String, // serialized DefKind ("fn", "struct", ...)
}

#[derive(Debug, Serialize)]
pub struct AppliedFile {
    pub file: String,
    pub bytes_changed: i64,
    pub edits: Vec<AppliedEdit>,
}

#[derive(Debug, Serialize)]
pub struct AppliedEdit {
    pub line: usize,
    pub col: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub old: String,
    pub new: String,
    pub confidence: &'static str, // "high" | "medium"
    pub reason: &'static str,     // ResolveReason::as_str()
}

#[derive(Debug, Serialize)]
pub struct SkippedSite {
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub name: String,
    pub confidence: &'static str,
    pub reason: &'static str,
    pub skip_reason: &'static str, // "low-confidence" | "re-export-alias" | "wildcard-reexport"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via_alias: Option<String>,  // only for skip_reason == "re-export-alias"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via_module: Option<String>, // only for skip_reason == "wildcard-reexport"
}

#[derive(Debug, Serialize)]
pub struct ErrorEntry {
    pub error_kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub col: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

impl From<&crate::error::AstEditError> for ErrorEntry {
    fn from(e: &crate::error::AstEditError) -> Self {
        use crate::error::AstEditError as E;
        let mut entry = ErrorEntry {
            error_kind: e.kind(),
            file: e.file().map(|s| s.to_string()),
            message: None,
            os_code: None,
            line: None,
            col: None,
            lang: None,
        };
        match e {
            E::ParseError { message, .. } => entry.message = Some(message.clone()),
            E::HashMismatch { .. } => {}
            E::ConcurrentWrite { .. } => {}
            E::NodeKindMismatch { line, col, .. } => {
                entry.line = Some(*line);
                entry.col = Some(*col);
            }
            E::WriteFailed { os_code, message, .. } => {
                entry.os_code = *os_code;
                entry.message = Some(message.clone());
            }
            E::PatternCompile { lang, message } => {
                entry.lang = Some(lang.clone());
                entry.message = Some(message.clone());
            }
        }
        entry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn rename_data_serializes_omitting_none_fields() {
        let data = RenameData {
            subcommand: "rename",
            dry_run: true,
            needs_anchor: None,
            candidates: None,
            applied: Some(vec![]),
            skipped: Some(vec![]),
            errors: Some(vec![]),
        };
        let v: Value = serde_json::to_value(&data).unwrap();
        assert_eq!(v["subcommand"], "rename");
        assert_eq!(v["dry_run"], true);
        assert!(v.get("needs_anchor").is_none());
        assert!(v.get("candidates").is_none());
        assert!(v["applied"].is_array());
        assert!(v["skipped"].is_array());
        assert!(v["errors"].is_array());
    }

    #[test]
    fn error_entry_from_write_failed_carries_os_code() {
        let err = crate::error::AstEditError::WriteFailed {
            file: "src/x.rs".into(),
            os_code: Some(13),
            message: "permission denied".into(),
        };
        let entry: ErrorEntry = (&err).into();
        let v = serde_json::to_value(&entry).unwrap();
        assert_eq!(v["error_kind"], "write-failed");
        assert_eq!(v["file"], "src/x.rs");
        assert_eq!(v["os_code"], 13);
        assert_eq!(v["message"], "permission denied");
    }

    #[test]
    fn skipped_site_alias_carries_via_alias_only() {
        let s = SkippedSite {
            file: "src/lib.rs".into(),
            line: 4, col: 0,
            start_byte: 10, end_byte: 14,
            name: "User".into(),
            confidence: "high",
            reason: "same-file-scope",
            skip_reason: "re-export-alias",
            via_alias: Some("Account".into()),
            via_module: None,
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["skip_reason"], "re-export-alias");
        assert_eq!(v["via_alias"], "Account");
        assert!(v.get("via_module").is_none());
    }
}
```

- [ ] **Step 2: Write `output.rs` (envelope helper)**

Create `crates/astedit/src/output.rs` mirroring codegraph's pattern:

```rust
use serde::Serialize;

#[derive(Serialize)]
struct Envelope<T: Serialize> {
    schema_version: u32,
    data: T,
}

/// Print `data` wrapped in the v1 envelope `{schema_version:1, data:...}`.
/// Every subcommand that produces JSON routes through this helper.
pub fn print_json<T: Serialize>(data: T) -> anyhow::Result<()> {
    let env = Envelope {
        schema_version: 1,
        data,
    };
    println!("{}", serde_json::to_string(&env)?);
    Ok(())
}
```

- [ ] **Step 3: Wire modules into main.rs**

Edit `crates/astedit/src/main.rs`:

```rust
mod error;
mod output;
mod serialize;

fn main() -> anyhow::Result<()> {
    Ok(())
}
```

- [ ] **Step 4: Run the tests**

```bash
cargo test -p astedit --locked
```

Expected: 4 tests pass (1 from Task 4 + 3 new in `serialize::tests`).

- [ ] **Step 5: Clippy**

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/astedit/src/serialize.rs crates/astedit/src/output.rs crates/astedit/src/main.rs
git commit -m "feat(astedit): add envelope helper + serializable JSON payload structs"
```

---

## Task 6: CLI scaffolding (`Cli`, `Command`, `RenameArgs`)

**Files:**
- Create: `crates/astedit/src/cli.rs`
- Create: `crates/astedit/src/commands/mod.rs`
- Create: `crates/astedit/src/commands/rename.rs`
- Modify: `crates/astedit/src/main.rs`

The CLI shape matches the spec's CLI surface. `rewrite` is intentionally omitted — PR 3 adds it.

- [ ] **Step 1: Write `cli.rs`**

Create `crates/astedit/src/cli.rs`:

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "astedit",
    version,
    about = "AST-validated rename and structural rewrite for AI coding agents."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Rename a symbol across the project (dry-run by default; pass --apply to write).
    Rename(RenameArgs),
}

#[derive(clap::Args, Debug)]
pub struct RenameArgs {
    /// The symbol's current name.
    pub old: String,
    /// The new name.
    pub new: String,
    /// Project root to scan (default: current directory).
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    /// Actually write edits to disk. Without this flag, astedit reports
    /// what it would do and exits.
    #[arg(long)]
    pub apply: bool,
    /// Emit `{schema_version:1, data:...}` JSON instead of human output.
    #[arg(long)]
    pub json: bool,
    /// Optional language hint (e.g. `rust`, `python`). Without this, every
    /// supported extension is scanned and dispatched per file.
    #[arg(long)]
    pub lang: Option<String>,
    /// Disambiguator for multi-def symbols. Format: `FILE:LINE`
    /// (the `file` value must match a definition's `file` field exactly —
    /// repo-relative, forward-slash-normalized).
    #[arg(long)]
    pub anchor: Option<String>,
}
```

- [ ] **Step 2: Write the empty rename module**

Create `crates/astedit/src/commands/mod.rs`:

```rust
pub mod rename;
```

Create `crates/astedit/src/commands/rename.rs`:

```rust
use crate::cli::RenameArgs;
use crate::output::print_json;
use crate::serialize::RenameData;

pub fn run(args: RenameArgs) -> anyhow::Result<i32> {
    // PR 2 implementation will fill this out in Tasks 8 onward. For now,
    // emit an empty dry-run envelope so the CLI is wireable.
    let data = RenameData {
        subcommand: "rename",
        dry_run: !args.apply,
        needs_anchor: None,
        candidates: None,
        applied: Some(vec![]),
        skipped: Some(vec![]),
        errors: Some(vec![]),
    };
    if args.json {
        print_json(data)?;
    }
    Ok(0)
}
```

- [ ] **Step 3: Wire `main.rs`**

Replace `crates/astedit/src/main.rs` entirely with:

```rust
mod cli;
mod commands;
mod error;
mod output;
mod serialize;

use clap::Parser;
use cli::{Cli, Command};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Rename(a) => commands::rename::run(a)?,
    };
    std::process::exit(code);
}
```

- [ ] **Step 4: Verify the CLI parses and runs**

```bash
cargo build -p astedit --locked
./target/debug/astedit --help
./target/debug/astedit rename Foo Bar --json --path /tmp
```

Expected: `--help` prints the subcommand list with `rename`. The `rename` invocation emits `{"schema_version":1,"data":{"subcommand":"rename","dry_run":true,"applied":[],"skipped":[],"errors":[]}}` and exits 0.

- [ ] **Step 5: Clippy + tests**

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Expected: clean. 52 tests pass (48 baseline + 4 from Tasks 4 + 5).

- [ ] **Step 6: Commit**

```bash
git add crates/astedit/src/cli.rs \
        crates/astedit/src/commands/ \
        crates/astedit/src/main.rs
git commit -m "feat(astedit): add CLI scaffolding with rename subcommand"
```

---

## Task 7: Add common test helper `copy_fixture`

**Files:**
- Create: `crates/astedit/tests/common/mod.rs`

Every integration test starts by copying a fixture tree into a `tempfile::TempDir` so `--apply` writes don't pollute the source tree. The helper lives in `tests/common/` per cargo convention.

- [ ] **Step 1: Create the helper**

```bash
mkdir -p crates/astedit/tests/common
```

Write `crates/astedit/tests/common/mod.rs`:

```rust
#![allow(dead_code)] // not every test consumes every helper

use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Copy `crates/astedit/tests/fixtures/<name>/` into a fresh TempDir
/// and return the TempDir (so the caller controls its lifetime — drop
/// removes the copy).
pub fn copy_fixture(name: &str) -> TempDir {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    assert!(
        src.is_dir(),
        "fixture {:?} not found — did you forget to add it under tests/fixtures/?",
        src,
    );
    let dst = TempDir::new().expect("create tempdir");
    copy_recursive(&src, dst.path());
    dst
}

fn copy_recursive(from: &Path, to: &Path) {
    if !to.exists() {
        fs::create_dir_all(to).expect("mkdir -p tempdir target");
    }
    for entry in fs::read_dir(from).expect("read_dir fixture") {
        let entry = entry.expect("dir entry");
        let kind = entry.file_type().expect("file_type");
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if kind.is_dir() {
            copy_recursive(&src, &dst);
        } else if kind.is_file() {
            fs::copy(&src, &dst).expect("copy fixture file");
        }
        // Symlinks in fixtures are not supported (yagni).
    }
}

/// Invoke the astedit binary built by `cargo test` and capture stdout +
/// exit code. Returns the parsed JSON `data` payload (the helper assumes
/// `--json` and asserts `schema_version == 1`).
pub fn run_astedit_json(args: &[&str]) -> (i32, serde_json::Value) {
    let exe = env!("CARGO_BIN_EXE_astedit");
    let out = std::process::Command::new(exe)
        .args(args)
        .output()
        .expect("spawn astedit");
    let code = out.status.code().unwrap_or(-1);
    let stdout = std::str::from_utf8(&out.stdout).expect("stdout utf8");
    let env: serde_json::Value = serde_json::from_str(stdout)
        .unwrap_or_else(|e| panic!("parse JSON: {e}\nstdout was: {stdout}\nstderr was: {}",
            std::str::from_utf8(&out.stderr).unwrap_or("(non-utf8)")));
    assert_eq!(env["schema_version"], 1, "schema_version must be 1");
    (code, env["data"].clone())
}
```

- [ ] **Step 2: Create a stub `rename_test.rs` declaring the helper module**

Create `crates/astedit/tests/rename_test.rs` with only the `mod common;` declaration. Task 8 onward appends actual test functions; isolating the helper now keeps later diffs additive.

```rust
mod common;
```

- [ ] **Step 3: Verify the helper compiles**

```bash
cargo test -p astedit --test rename_test --locked
```

Expected: `0 tests passed` (no tests yet — confirms the file and its `mod common;` link compile cleanly). The whole-workspace test count stays at 52 (48 baseline + 1 error + 3 serialize unit tests from Tasks 4–5).

- [ ] **Step 4: Commit**

```bash
git add crates/astedit/tests/common/ crates/astedit/tests/rename_test.rs
git commit -m "test(astedit): add copy_fixture + run_astedit_json helpers"
```

---

## Task 8: TDD — same-file high-confidence rename (dry-run default)

**Files:**
- Create: `crates/astedit/tests/fixtures/same_file/main.rs`
- Modify: `crates/astedit/tests/rename_test.rs`
- Modify: `crates/astedit/src/commands/rename.rs`

This is the load-bearing task — it drives the index → resolve → byte-splice pipeline into existence. Subsequent TDD tasks layer on edge cases.

- [ ] **Step 1: Create the fixture**

```bash
mkdir -p crates/astedit/tests/fixtures/same_file
```

Write `crates/astedit/tests/fixtures/same_file/main.rs`:

```rust
struct User {
    id: u64,
}

fn make() -> User {
    User { id: 0 }
}

fn count(u: &User) -> u64 {
    u.id
}
```

The fixture has a Rust `struct User` def and three same-file uses (`User` in return type, `User { … }` literal, and `&User` arg type — three references that the same-file-scope rule should resolve as `high` confidence).

- [ ] **Step 2: Write the failing test**

Append the import lines and test function to `crates/astedit/tests/rename_test.rs` (the file from Task 7 currently contains only `mod common;`). The final shape after this step:

```rust
mod common;

use common::{copy_fixture, run_astedit_json};

#[test]
fn rename_same_file_high_confidence_dry_run_default() {
    let tmp = copy_fixture("same_file");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rename", "User", "Account",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0, "dry-run with matches should exit 0");
    assert_eq!(data["subcommand"], "rename");
    assert_eq!(data["dry_run"], true);
    assert!(data["errors"].as_array().unwrap().is_empty(), "no errors expected: {:?}", data["errors"]);

    let applied = data["applied"].as_array().expect("applied array");
    assert_eq!(applied.len(), 1, "single fixture file expected; got: {applied:?}");
    let file_entry = &applied[0];
    assert!(file_entry["file"].as_str().unwrap().ends_with("main.rs"));

    let edits = file_entry["edits"].as_array().unwrap();
    // The fixture has the struct DEFINITION (1) + 3 use sites = 4 identifier
    // sites total. The resolver returns references (not definitions); whether
    // the definition's identifier is itself a Reference depends on the index.
    // The test asserts at least 3 (the use sites) and at most 4.
    assert!(edits.len() >= 3 && edits.len() <= 4, "expected 3–4 edits, got {}", edits.len());

    for e in edits {
        assert_eq!(e["old"], "User");
        assert_eq!(e["new"], "Account");
        assert_eq!(e["confidence"], "high");
        assert_eq!(e["reason"], "same-file-scope");
        assert!(e["start_byte"].as_u64().unwrap() < e["end_byte"].as_u64().unwrap());
        assert_eq!(
            e["end_byte"].as_u64().unwrap() - e["start_byte"].as_u64().unwrap(),
            "User".len() as u64,
        );
    }

    // Dry-run must NOT mutate the fixture copy.
    let fixture_file = tmp.path().join("main.rs");
    let after = std::fs::read_to_string(&fixture_file).unwrap();
    assert!(after.contains("struct User"), "dry-run modified the file: {after}");
    assert!(!after.contains("Account"), "dry-run wrote Account into the file");
}
```

- [ ] **Step 3: Run to confirm it fails**

```bash
cargo test -p astedit --test rename_test --locked rename_same_file_high_confidence_dry_run_default
```

Expected: failure. The current `commands::rename::run` returns an empty `applied` list.

- [ ] **Step 4: Implement the rename pipeline (high/medium → applied)**

Replace `crates/astedit/src/commands/rename.rs` with the full pipeline (re-export skipping and Low-confidence skipping land in later tasks; this implementation stubs them as empty branches so the test passes):

```rust
use std::collections::BTreeMap;

use codegraph_core::index::build_index;
use codegraph_core::resolve::{resolve_refs, Confidence, Resolved};

use crate::cli::RenameArgs;
use crate::output::print_json;
use crate::serialize::{AppliedEdit, AppliedFile, RenameData, SkippedSite};

pub fn run(args: RenameArgs) -> anyhow::Result<i32> {
    let path = args.path.as_path();
    let index = build_index(path)?;
    let resolved = resolve_refs(&index, &args.old);

    let mut applied: Vec<AppliedFile> = Vec::new();
    let mut skipped: Vec<SkippedSite> = Vec::new();
    let errors: Vec<crate::serialize::ErrorEntry> = Vec::new();

    // Group queue-for-edit refs by file. Low confidence and re-export traversal
    // become Skipped — wired in later tasks of PR 2.
    let mut by_file: BTreeMap<String, Vec<&Resolved>> = BTreeMap::new();
    for r in &resolved {
        match r.confidence {
            Confidence::High | Confidence::Medium => {
                by_file.entry(r.reference.file.clone()).or_default().push(r);
            }
            Confidence::Low => {
                // Wired in Task 11.
            }
        }
    }

    for (file, refs) in by_file {
        let edits = build_edits(&file, &refs, &args, path)?;
        applied.push(edits);
    }

    let data = RenameData {
        subcommand: "rename",
        dry_run: !args.apply,
        needs_anchor: None,
        candidates: None,
        applied: Some(applied),
        skipped: Some(skipped),
        errors: Some(errors),
    };

    if args.json {
        print_json(data)?;
    }
    // Exit-status rules from Task 14 onward override this.
    Ok(0)
}

/// Construct an `AppliedFile` for one file. In dry-run, builds the edit list
/// without touching disk. `--apply` write logic lands in Task 17 / 18.
fn build_edits(
    file: &str,
    refs: &[&Resolved<'_>],
    args: &RenameArgs,
    root: &std::path::Path,
) -> anyhow::Result<AppliedFile> {
    // For now we use the reference's pre-computed `byte_offset` + len(OLD) for
    // the replacement range. Trust comes from the index: build_index recorded
    // the offset for an identifier whose name matched OLD at parse time.
    // Task 18 adds drift checking; Task 17 adds writes; this task only needs
    // the dry-run shape.
    let _ = root; // used in Task 17 (file IO)

    let old_len = args.old.len();
    let new_len = args.new.len();
    let mut edits: Vec<AppliedEdit> = Vec::new();
    for r in refs {
        edits.push(AppliedEdit {
            line: r.reference.line,
            col: r.reference.column,
            start_byte: r.reference.byte_offset,
            end_byte: r.reference.byte_offset + old_len,
            old: args.old.clone(),
            new: args.new.clone(),
            confidence: r.confidence.as_str(),
            reason: r.reason.as_str(),
        });
    }
    // Sort by byte position descending so later steps apply in reverse.
    edits.sort_by(|a, b| b.start_byte.cmp(&a.start_byte));

    let bytes_changed = (new_len as i64 - old_len as i64) * edits.len() as i64;

    Ok(AppliedFile {
        file: file.to_string(),
        bytes_changed,
        edits,
    })
}
```

- [ ] **Step 5: Run the test**

```bash
cargo test -p astedit --test rename_test --locked rename_same_file_high_confidence_dry_run_default
```

Expected: PASS.

- [ ] **Step 6: Run the full suite + clippy**

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: 53 tests pass (48 baseline + 4 unit tests from Tasks 4–5 + 1 new integration test). Clippy clean.

- [ ] **Step 7: Commit**

```bash
git add crates/astedit/tests/fixtures/same_file/ \
        crates/astedit/tests/rename_test.rs \
        crates/astedit/src/commands/rename.rs
git commit -m "feat(astedit): rename pipeline emits high-confidence edits in dry-run"
```

---

## Task 9: TDD — cross-file import-resolved rename

**Files:**
- Create: `crates/astedit/tests/fixtures/cross_file_import/foo.py`
- Create: `crates/astedit/tests/fixtures/cross_file_import/bar.py`
- Modify: `crates/astedit/tests/rename_test.rs`

Confirms multi-file aggregation works. Python is used because its `from foo import User` produces clean `import-resolved` references.

- [ ] **Step 1: Create the fixture**

```bash
mkdir -p crates/astedit/tests/fixtures/cross_file_import
```

Write `crates/astedit/tests/fixtures/cross_file_import/foo.py`:

```python
class User:
    def __init__(self, name):
        self.name = name
```

Write `crates/astedit/tests/fixtures/cross_file_import/bar.py`:

```python
from foo import User

def make(name):
    return User(name)
```

- [ ] **Step 2: Append the failing test**

Append to `crates/astedit/tests/rename_test.rs`:

```rust
#[test]
fn rename_cross_file_import_resolved() {
    let tmp = copy_fixture("cross_file_import");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rename", "User", "Account",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0);
    assert!(data["errors"].as_array().unwrap().is_empty(), "errors: {:?}", data["errors"]);

    let applied = data["applied"].as_array().expect("applied array");
    // Expect at least bar.py to be touched (the import-resolved use). foo.py
    // contains the definition itself; whether its self-references count
    // depends on the indexer — accept 1 or 2 files.
    assert!(applied.len() >= 1 && applied.len() <= 2, "got {} files: {applied:?}", applied.len());

    let files: Vec<&str> = applied.iter()
        .map(|f| f["file"].as_str().unwrap())
        .collect();
    assert!(files.iter().any(|f| f.ends_with("bar.py")), "bar.py not in applied: {files:?}");

    let bar = applied.iter().find(|f| f["file"].as_str().unwrap().ends_with("bar.py")).unwrap();
    let bar_edits = bar["edits"].as_array().unwrap();
    assert!(!bar_edits.is_empty());
    for e in bar_edits {
        assert_eq!(e["old"], "User");
        assert_eq!(e["new"], "Account");
        // import-resolved across files is "high" via ImportResolved reason.
        assert_eq!(e["reason"], "import-resolved");
        assert_eq!(e["confidence"], "high");
    }
}
```

- [ ] **Step 3: Run**

```bash
cargo test -p astedit --test rename_test --locked rename_cross_file_import_resolved
```

Expected: PASS — the Task 8 implementation already handles cross-file (the resolver returns import-resolved refs with `Confidence::High`).

If it fails because the Python `Reference.byte_offset` doesn't point at the identifier start, dig into `codegraph-core::index` Python query handling — but **flag before touching core** per the handoff.

- [ ] **Step 4: Commit**

```bash
git add crates/astedit/tests/fixtures/cross_file_import/ \
        crates/astedit/tests/rename_test.rs
git commit -m "test(astedit): cross-file import-resolved rename"
```

---

## Task 10: TDD — glob/wildcard import yields medium confidence

**Files:**
- Create: `crates/astedit/tests/fixtures/glob_import/lib.rs`
- Create: `crates/astedit/tests/fixtures/glob_import/inner.rs`
- Modify: `crates/astedit/tests/rename_test.rs`

The resolver returns `Confidence::Medium` (reason: `import-resolved`) when the only import covering a reference's file is a glob/wildcard. The rename pipeline should still include medium-confidence sites in `applied`.

- [ ] **Step 1: Create the fixture**

```bash
mkdir -p crates/astedit/tests/fixtures/glob_import
```

Write `crates/astedit/tests/fixtures/glob_import/inner.rs`:

```rust
pub struct User;
```

Write `crates/astedit/tests/fixtures/glob_import/lib.rs`:

```rust
use inner::*;

fn make() -> User {
    User
}
```

- [ ] **Step 2: Append the failing test**

Append to `crates/astedit/tests/rename_test.rs`:

```rust
#[test]
fn rename_glob_import_medium_confidence_applied() {
    let tmp = copy_fixture("glob_import");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rename", "User", "Account",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0);
    let applied = data["applied"].as_array().expect("applied array");

    let lib = applied.iter().find(|f| f["file"].as_str().unwrap().ends_with("lib.rs"))
        .expect("lib.rs should be in applied");
    let lib_edits = lib["edits"].as_array().unwrap();
    assert!(!lib_edits.is_empty(), "expected at least one edit in lib.rs");
    for e in lib_edits {
        // Glob-only import → resolver assigns Medium.
        assert_eq!(e["confidence"], "medium", "expected medium for glob import: {e:?}");
    }
}
```

- [ ] **Step 3: Run**

```bash
cargo test -p astedit --test rename_test --locked rename_glob_import_medium_confidence_applied
```

Expected: PASS — Task 8 already routes `Medium` into `applied`.

- [ ] **Step 4: Commit**

```bash
git add crates/astedit/tests/fixtures/glob_import/ \
        crates/astedit/tests/rename_test.rs
git commit -m "test(astedit): glob/wildcard import medium-confidence sites apply"
```

---

## Task 11: TDD — name-only refs go to `skipped{low-confidence}`

**Files:**
- Create: `crates/astedit/tests/fixtures/name_only/foo.py`
- Create: `crates/astedit/tests/fixtures/name_only/unrelated.py`
- Modify: `crates/astedit/tests/rename_test.rs`
- Modify: `crates/astedit/src/commands/rename.rs`

`Confidence::Low` references have no resolving import nor same-file definition. They must NOT end up in `applied` — the rename would be guesswork. Instead they're surfaced under `skipped[].skip_reason: "low-confidence"`.

- [ ] **Step 1: Create the fixture**

```bash
mkdir -p crates/astedit/tests/fixtures/name_only
```

Write `crates/astedit/tests/fixtures/name_only/foo.py`:

```python
class User:
    pass
```

Write `crates/astedit/tests/fixtures/name_only/unrelated.py`:

```python
# This file does NOT import from foo. The bare `User` reference below
# has no resolving import — the resolver returns Confidence::Low.
def make():
    return User()
```

- [ ] **Step 2: Append the failing test**

Append to `crates/astedit/tests/rename_test.rs`:

```rust
#[test]
fn rename_name_only_goes_to_skipped_low_confidence() {
    let tmp = copy_fixture("name_only");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rename", "User", "Account",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0);

    let skipped = data["skipped"].as_array().expect("skipped array");
    let lows: Vec<&serde_json::Value> = skipped.iter()
        .filter(|s| s["skip_reason"] == "low-confidence")
        .collect();
    assert!(!lows.is_empty(), "expected at least one low-confidence skip; skipped: {skipped:?}");

    for s in &lows {
        assert_eq!(s["confidence"], "low");
        assert_eq!(s["reason"], "name-only");
        assert_eq!(s["name"], "User");
        assert!(s["file"].as_str().unwrap().ends_with("unrelated.py"));
    }

    // unrelated.py must NOT appear in applied.
    let applied = data["applied"].as_array().unwrap();
    let bad = applied.iter()
        .find(|f| f["file"].as_str().unwrap().ends_with("unrelated.py"));
    assert!(bad.is_none(), "unrelated.py should not be in applied: {bad:?}");
}
```

- [ ] **Step 3: Run to confirm it fails**

```bash
cargo test -p astedit --test rename_test --locked rename_name_only_goes_to_skipped_low_confidence
```

Expected: failure — the `Confidence::Low` arm in `rename.rs` is currently empty.

- [ ] **Step 4: Wire `Low` → `skipped`**

In `crates/astedit/src/commands/rename.rs`, replace the `Confidence::Low` arm of the partition loop:

```rust
            Confidence::Low => {
                // Wired in Task 11.
            }
```

with:

```rust
            Confidence::Low => {
                skipped.push(SkippedSite {
                    file: r.reference.file.clone(),
                    line: r.reference.line,
                    col: r.reference.column,
                    start_byte: r.reference.byte_offset,
                    end_byte: r.reference.byte_offset + args.old.len(),
                    name: r.reference.name.clone(),
                    confidence: r.confidence.as_str(),
                    reason: r.reason.as_str(),
                    skip_reason: "low-confidence",
                    via_alias: None,
                    via_module: None,
                });
            }
```

- [ ] **Step 5: Run the test**

```bash
cargo test -p astedit --test rename_test --locked rename_name_only_goes_to_skipped_low_confidence
```

Expected: PASS.

- [ ] **Step 6: Run the suite + clippy**

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: 56 tests (52 + 4 rename integration tests from Tasks 8–11). Clippy clean.

- [ ] **Step 7: Commit**

```bash
git add crates/astedit/tests/fixtures/name_only/ \
        crates/astedit/tests/rename_test.rs \
        crates/astedit/src/commands/rename.rs
git commit -m "feat(astedit): route low-confidence refs into skipped lane"
```

---

## Task 12: TDD — alias re-export site goes to `skipped{re-export-alias}`

**Files:**
- Create: `crates/astedit/tests/fixtures/alias_reexport/lib.rs`
- Create: `crates/astedit/tests/fixtures/alias_reexport/inner.rs`
- Modify: `crates/astedit/tests/rename_test.rs`
- Modify: `crates/astedit/src/commands/rename.rs`

`index.alias_reexports[OLD]` returns sites where someone wrote `pub use ::Y as OLD;`. Renaming the local `OLD` at such a site would break the alias contract — surface it as `skipped` with `skip_reason: "re-export-alias"` and `via_alias: <original>`.

- [ ] **Step 1: Create the fixture**

```bash
mkdir -p crates/astedit/tests/fixtures/alias_reexport
```

Write `crates/astedit/tests/fixtures/alias_reexport/inner.rs`:

```rust
pub struct Bar;
```

Write `crates/astedit/tests/fixtures/alias_reexport/lib.rs`:

```rust
mod inner;

// pub use inner::Bar as User;  -- "User" here is an alias for inner::Bar.
// Renaming User → Account would change the public API surface of this crate.
pub use inner::Bar as User;

fn make() -> User {
    User
}
```

The fixture has two `User` mentions in `lib.rs`: the alias declaration (line 5) and the `make` body's same-file reference (lines 7–8). The alias site should be `skipped{re-export-alias}`; the same-file uses should still be `applied{high}`. (How the index handles the alias site as a definition or a re-export entry is up to `codegraph-core` — the test asserts the desired astedit behaviour.)

- [ ] **Step 2: Append the failing test**

Append to `crates/astedit/tests/rename_test.rs`:

```rust
#[test]
fn rename_alias_reexport_skipped_with_via_alias() {
    let tmp = copy_fixture("alias_reexport");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rename", "User", "Account",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0);

    let skipped = data["skipped"].as_array().expect("skipped array");
    let aliases: Vec<&serde_json::Value> = skipped.iter()
        .filter(|s| s["skip_reason"] == "re-export-alias")
        .collect();
    assert_eq!(aliases.len(), 1, "expected one alias skip; got {skipped:?}");

    let alias = aliases[0];
    assert!(alias["file"].as_str().unwrap().ends_with("lib.rs"));
    assert_eq!(alias["name"], "User");
    assert_eq!(alias["via_alias"], "Bar", "via_alias should be the original symbol");
    assert!(alias.get("via_module").is_none_or(|v| v.is_null()),
        "via_module must not appear on re-export-alias entries: {alias:?}");
}
```

(`is_none_or` requires Rust 1.82 — well below the 1.74 MSRV ceiling? Actually 1.74 < 1.82, so `is_none_or` is NOT in MSRV. Replace with the safer manual check.)

Use this version of the final assertion instead:

```rust
    match alias.get("via_module") {
        None => {},
        Some(v) if v.is_null() => {},
        Some(other) => panic!("via_module must not appear on re-export-alias entries: {other:?}"),
    }
```

- [ ] **Step 3: Run to confirm fail**

```bash
cargo test -p astedit --test rename_test --locked rename_alias_reexport_skipped_with_via_alias
```

Expected: failure — astedit currently knows nothing about `index.alias_reexports`.

- [ ] **Step 4: Wire alias-reexport detection into the pipeline**

The alias declaration site (`pub use inner::Bar as User;`) may or may not appear in `resolved` depending on how `codegraph-core` indexes `use_as_clause` nodes — that's an implementation detail we shouldn't depend on. Instead, drive the skip purely from `index.alias_reexports[OLD]`: every site in that map is, by construction, the local end of an alias that would break if we renamed it.

In `crates/astedit/src/commands/rename.rs`, immediately after the existing `for r in &resolved { … }` partition loop (the one Task 8 wrote), add a separate pass over `index.alias_reexports`:

```rust
    // Alias re-export sites: `index.alias_reexports[OLD]` lists every place
    // someone wrote `pub use ::Y as OLD;`. Renaming OLD at those sites
    // would change the public API surface, so they go to skipped[] with
    // `via_alias` = the original symbol being aliased.
    if let Some(sites) = index.alias_reexports.get(&args.old) {
        for site in sites {
            skipped.push(SkippedSite {
                file: site.file.clone(),
                line: site.line,
                col: 0,
                start_byte: 0,
                end_byte: 0,
                name: args.old.clone(),
                confidence: "high",
                reason: "same-file-scope",
                skip_reason: "re-export-alias",
                via_alias: Some(site.original.clone()),
                via_module: None,
            });
        }
    }
```

`col`, `start_byte`, and `end_byte` are zero-filled because `AliasSite` doesn't carry a column or byte offset — only `line`. The schema's `col`/`start_byte`/`end_byte` are required fields on `SkippedSite`, so we emit zeros; agents already know `re-export-alias` entries lack precise byte coordinates because the alias declaration is the whole `use_declaration` node, not a single identifier.

Also: if a same-file partition entry (added by the `for r in &resolved` loop) happens to point at the same `(file, line)` as an alias re-export, it must NOT appear in `applied` — the alias takes precedence. Just before the `let by_file` line, filter out any queued ref whose `(file, line)` matches an alias-reexport site:

```rust
    // Strip out any queued ref that collides with an alias-reexport site —
    // the re-export-alias skip wins.
    let by_file: std::collections::BTreeMap<String, Vec<&Resolved>> = {
        let alias_keys: std::collections::HashSet<(String, usize)> = index
            .alias_reexports
            .get(&args.old)
            .map(|sites| {
                sites.iter()
                    .map(|s| (s.file.clone(), s.line))
                    .collect()
            })
            .unwrap_or_default();

        let mut m: std::collections::BTreeMap<String, Vec<&Resolved>> = Default::default();
        for r in &resolved {
            if !matches!(r.confidence, Confidence::High | Confidence::Medium) { continue; }
            if alias_keys.contains(&(r.reference.file.clone(), r.reference.line)) { continue; }
            m.entry(r.reference.file.clone()).or_default().push(r);
        }
        m
    };
```

Then replace the existing `for r in &resolved { … }` partition loop and the `let mut by_file: BTreeMap…` line from Task 8 with the block above, **keeping the `Confidence::Low → skipped` arm** Task 11 added. The final shape of the partition section is:

```rust
    // Low-confidence refs go to skipped[low-confidence] (Task 11).
    for r in &resolved {
        if matches!(r.confidence, Confidence::Low) {
            skipped.push(SkippedSite {
                file: r.reference.file.clone(),
                line: r.reference.line,
                col: r.reference.column,
                start_byte: r.reference.byte_offset,
                end_byte: r.reference.byte_offset + args.old.len(),
                name: r.reference.name.clone(),
                confidence: r.confidence.as_str(),
                reason: r.reason.as_str(),
                skip_reason: "low-confidence",
                via_alias: None,
                via_module: None,
            });
        }
    }

    // Alias re-export sites → skipped[re-export-alias].
    if let Some(sites) = index.alias_reexports.get(&args.old) {
        for site in sites {
            skipped.push(SkippedSite {
                file: site.file.clone(),
                line: site.line,
                col: 0,
                start_byte: 0,
                end_byte: 0,
                name: args.old.clone(),
                confidence: "high",
                reason: "same-file-scope",
                skip_reason: "re-export-alias",
                via_alias: Some(site.original.clone()),
                via_module: None,
            });
        }
    }

    // High/Medium → queued for edit, minus any collision with an alias site.
    let alias_keys: std::collections::HashSet<(String, usize)> = index
        .alias_reexports
        .get(&args.old)
        .map(|sites| sites.iter().map(|s| (s.file.clone(), s.line)).collect())
        .unwrap_or_default();

    let mut by_file: std::collections::BTreeMap<String, Vec<&Resolved>> = Default::default();
    for r in &resolved {
        if !matches!(r.confidence, Confidence::High | Confidence::Medium) { continue; }
        if alias_keys.contains(&(r.reference.file.clone(), r.reference.line)) { continue; }
        by_file.entry(r.reference.file.clone()).or_default().push(r);
    }
```

This replaces the original Task 8 partition loop end-to-end. The control flow is now: surface Low refs → surface alias re-exports → queue High/Medium that don't collide.

- [ ] **Step 5: Run the test**

```bash
cargo test -p astedit --test rename_test --locked rename_alias_reexport_skipped_with_via_alias
```

Expected: PASS.

- [ ] **Step 6: Run the suite + clippy**

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: 57 tests (52 + 5 rename integration tests). Clippy clean.

- [ ] **Step 7: Commit**

```bash
git add crates/astedit/tests/fixtures/alias_reexport/ \
        crates/astedit/tests/rename_test.rs \
        crates/astedit/src/commands/rename.rs
git commit -m "feat(astedit): surface alias re-export sites as skipped"
```

---

## Task 13: TDD — wildcard re-export goes to `skipped{wildcard-reexport}`

**Files:**
- Create: `crates/astedit/tests/fixtures/wildcard_reexport/lib.rs`
- Create: `crates/astedit/tests/fixtures/wildcard_reexport/inner.rs`
- Modify: `crates/astedit/tests/rename_test.rs`
- Modify: `crates/astedit/src/commands/rename.rs`

`pub use foo::*;` is a wildcard re-export. The set of names that cross the alias is unknown without resolving the target module. astedit's MVP surfaces every wildcard re-export entry whose target module contains a definition of `OLD` as `skipped{wildcard-reexport}` with `via_module: <module_path>`.

- [ ] **Step 1: Create the fixture**

```bash
mkdir -p crates/astedit/tests/fixtures/wildcard_reexport
```

Write `crates/astedit/tests/fixtures/wildcard_reexport/inner.rs`:

```rust
pub struct User;
```

Write `crates/astedit/tests/fixtures/wildcard_reexport/lib.rs`:

```rust
mod inner;

// `User` crosses this re-export boundary; renaming it would silently
// change the wildcard's export surface.
pub use inner::*;
```

- [ ] **Step 2: Append the failing test**

Append to `crates/astedit/tests/rename_test.rs`:

```rust
#[test]
fn rename_wildcard_reexport_skipped_with_via_module() {
    let tmp = copy_fixture("wildcard_reexport");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rename", "User", "Account",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0);

    let skipped = data["skipped"].as_array().expect("skipped array");
    let wilds: Vec<&serde_json::Value> = skipped.iter()
        .filter(|s| s["skip_reason"] == "wildcard-reexport")
        .collect();
    assert_eq!(wilds.len(), 1, "expected one wildcard skip; got {skipped:?}");

    let w = wilds[0];
    assert!(w["file"].as_str().unwrap().ends_with("lib.rs"));
    assert!(w["via_module"].as_str().unwrap().contains("inner"),
        "via_module should reference inner module; got {:?}", w["via_module"]);
    match w.get("via_alias") {
        None => {},
        Some(v) if v.is_null() => {},
        Some(other) => panic!("via_alias must not appear on wildcard-reexport entries: {other:?}"),
    }
}
```

- [ ] **Step 3: Run to confirm fail**

```bash
cargo test -p astedit --test rename_test --locked rename_wildcard_reexport_skipped_with_via_module
```

Expected: failure.

- [ ] **Step 4: Implement wildcard-reexport surfacing**

In `crates/astedit/src/commands/rename.rs`, immediately after the alias-reexport backstop loop, add a wildcard surfacing pass. The key insight: `index.wildcard_reexports` is keyed by the *module-path-tail* (e.g. `"inner"`), not by the renamed symbol. We iterate every entry and surface those whose module path resolves to a file that defines `OLD`:

```rust
    // Wildcard re-exports: surface every wildcard whose target module defines
    // a symbol named OLD. We use a lossy match (file path contains the module
    // path's last segment) — it's the same heuristic the resolver uses for
    // wildcard import matching.
    let defines_old: std::collections::HashSet<String> = index
        .definitions
        .iter()
        .filter(|d| d.name == args.old)
        .map(|d| d.file.clone())
        .collect();

    for sites in index.wildcard_reexports.values() {
        for site in sites {
            // Cheap match: the module path's tail segment appears in some
            // definition-file's path. e.g. module_path "inner" matches
            // "inner.rs" in `defines_old`.
            let tail = site.module_path.rsplit("::").next().unwrap_or(&site.module_path);
            let related = defines_old.iter().any(|f| {
                let stem = std::path::Path::new(f)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                stem == tail
            });
            if !related { continue; }

            let already = skipped.iter().any(|s|
                s.file == site.file && s.line == site.line && s.skip_reason == "wildcard-reexport"
            );
            if already { continue; }

            skipped.push(SkippedSite {
                file: site.file.clone(),
                line: site.line,
                col: 0,
                start_byte: 0,
                end_byte: 0,
                name: args.old.clone(),
                confidence: "medium",
                reason: "import-resolved",
                skip_reason: "wildcard-reexport",
                via_alias: None,
                via_module: Some(site.module_path.clone()),
            });
        }
    }
```

- [ ] **Step 5: Run the test**

```bash
cargo test -p astedit --test rename_test --locked rename_wildcard_reexport_skipped_with_via_module
```

Expected: PASS.

- [ ] **Step 6: Confirm no regression in earlier rename tests**

```bash
cargo test -p astedit --test rename_test --locked
```

Expected: every test from Tasks 8–13 passes. If `same_file` or `cross_file_import` now fail because the wildcard backstop is over-surfacing skips on a non-wildcard fixture, narrow the predicate (the fixtures for those tasks contain no `pub use … *`, so `index.wildcard_reexports` should be empty for them).

- [ ] **Step 7: Suite + clippy**

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: 58 tests (52 + 6 rename integration tests). Clippy clean.

- [ ] **Step 8: Commit**

```bash
git add crates/astedit/tests/fixtures/wildcard_reexport/ \
        crates/astedit/tests/rename_test.rs \
        crates/astedit/src/commands/rename.rs
git commit -m "feat(astedit): surface wildcard re-export sites as skipped"
```

---

## Task 14: TDD — multi-def without `--anchor` emits `needs_anchor` + non-zero exit

**Files:**
- Create: `crates/astedit/tests/fixtures/multi_def/a.rs`
- Create: `crates/astedit/tests/fixtures/multi_def/b.rs`
- Modify: `crates/astedit/tests/rename_test.rs`
- Modify: `crates/astedit/src/commands/rename.rs`

When `<OLD>` has more than one definition and `--anchor` is absent, the envelope wraps `needs_anchor: true` + a `candidates` array and the process exits non-zero so the agent's wrapper notices.

- [ ] **Step 1: Create the fixture**

```bash
mkdir -p crates/astedit/tests/fixtures/multi_def
```

Write `crates/astedit/tests/fixtures/multi_def/a.rs`:

```rust
pub struct User {
    pub id: u64,
}

fn use_struct() {
    let _: User;
}
```

Write `crates/astedit/tests/fixtures/multi_def/b.rs`:

```rust
pub fn User() -> u64 { 0 }

fn use_fn() -> u64 {
    User()
}
```

Two definitions named `User` (struct in `a.rs`, fn in `b.rs`), each with one same-file use site. Without `--anchor` the pipeline short-circuits on multi-def detection (Task 14). With `--anchor a.rs:1` the resolver yields two refs but the anchor filter keeps only the one in `a.rs` (Task 15).

- [ ] **Step 2: Append the failing test**

Append to `crates/astedit/tests/rename_test.rs`:

```rust
#[test]
fn rename_multi_def_without_anchor_needs_anchor_envelope() {
    let tmp = copy_fixture("multi_def");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rename", "User", "Account",
        "--path", path,
        "--json",
    ]);

    assert_ne!(code, 0, "multi-def without --anchor must exit non-zero");
    assert_eq!(data["subcommand"], "rename");
    assert_eq!(data["needs_anchor"], true);

    let candidates = data["candidates"].as_array().expect("candidates array");
    assert_eq!(candidates.len(), 2, "expected exactly two candidates: {candidates:?}");

    let kinds: Vec<&str> = candidates.iter().map(|c| c["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"struct"));
    assert!(kinds.contains(&"fn"));

    for c in candidates {
        assert!(c["line"].as_u64().unwrap() >= 1);
        let f = c["file"].as_str().unwrap();
        assert!(f.ends_with("a.rs") || f.ends_with("b.rs"));
    }

    // No applied/skipped/errors on the needs_anchor path.
    assert!(data.get("applied").is_none_or_null());
    assert!(data.get("skipped").is_none_or_null());
    assert!(data.get("errors").is_none_or_null());
}

trait IsNoneOrNull {
    fn is_none_or_null(&self) -> bool;
}
impl IsNoneOrNull for Option<&serde_json::Value> {
    fn is_none_or_null(&self) -> bool {
        match self {
            None => true,
            Some(v) => v.is_null(),
        }
    }
}
```

- [ ] **Step 3: Run to confirm fail**

```bash
cargo test -p astedit --test rename_test --locked rename_multi_def_without_anchor_needs_anchor_envelope
```

Expected: failure.

- [ ] **Step 4: Wire multi-def detection**

In `crates/astedit/src/commands/rename.rs`, at the top of `run`, immediately after `let resolved = resolve_refs(...);`, insert:

```rust
    // Step 2: find defs by name. Resolver doesn't expose this — query the
    // index directly.
    let defs: Vec<&codegraph_core::index::Definition> = index
        .definitions
        .iter()
        .filter(|d| d.name == args.old)
        .collect();

    if defs.len() > 1 && args.anchor.is_none() {
        // Multi-def disambiguation: emit needs_anchor + candidates and exit non-zero.
        let candidates: Vec<crate::serialize::Candidate> = defs
            .iter()
            .map(|d| crate::serialize::Candidate {
                file: d.file.clone(),
                line: d.line,
                kind: def_kind_str(d.kind).to_string(),
            })
            .collect();
        let data = RenameData {
            subcommand: "rename",
            dry_run: !args.apply,
            needs_anchor: Some(true),
            candidates: Some(candidates),
            applied: None,
            skipped: None,
            errors: None,
        };
        if args.json {
            print_json(data)?;
        }
        return Ok(2);
    }
```

Add this helper at the bottom of the file (the `DefKind` Serialize derive emits lowercase, but it's behind a `serde_json::to_value` round-trip — a direct match is simpler):

```rust
fn def_kind_str(k: codegraph_core::index::DefKind) -> &'static str {
    use codegraph_core::index::DefKind::*;
    match k {
        Fn => "fn",
        Struct => "struct",
        Enum => "enum",
        Trait => "trait",
        Class => "class",
        Interface => "interface",
        Type => "type",
        Const => "const",
        Method => "method",
    }
}
```

Add `use codegraph_core::index::DefKind;` to the top of the file (so the `def_kind_str` arms resolve).

- [ ] **Step 5: Run the test**

```bash
cargo test -p astedit --test rename_test --locked rename_multi_def_without_anchor_needs_anchor_envelope
```

Expected: PASS.

- [ ] **Step 6: Suite + clippy**

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: 59 tests (52 + 7 rename integration tests). Clippy clean.

- [ ] **Step 7: Commit**

```bash
git add crates/astedit/tests/fixtures/multi_def/ \
        crates/astedit/tests/rename_test.rs \
        crates/astedit/src/commands/rename.rs
git commit -m "feat(astedit): emit needs_anchor envelope on multi-def w/o --anchor"
```

---

## Task 15: TDD — `--anchor FILE:LINE` disambiguates

**Files:**
- Modify: `crates/astedit/tests/rename_test.rs`
- Modify: `crates/astedit/src/commands/rename.rs`

Re-uses the `multi_def` fixture. When `--anchor a.rs:1` is passed, the rename picks the struct (the one in `a.rs:1`) and only renames its references; the `fn User` in `b.rs` is untouched. The path comparison uses the same forward-slash-normalized form the index uses.

- [ ] **Step 1: Append the failing test**

Append to `crates/astedit/tests/rename_test.rs`:

```rust
#[test]
fn rename_with_anchor_picks_matching_definition() {
    let tmp = copy_fixture("multi_def");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rename", "User", "Account",
        "--path", path,
        "--anchor", "a.rs:1",
        "--json",
    ]);

    assert_eq!(code, 0, "anchor present → exit 0: data was {data:?}");
    assert!(data.get("needs_anchor").is_none_or_null());

    // The `fn User` in b.rs must not be touched — its references (none, in
    // this fixture) should not appear in applied.
    let applied = data["applied"].as_array().expect("applied array");
    for f in applied {
        let file = f["file"].as_str().unwrap();
        assert!(file.ends_with("a.rs"), "anchor picked struct in a.rs; b.rs should not appear: {file}");
    }
}
```

(Re-uses the `IsNoneOrNull` trait from Task 14.)

- [ ] **Step 2: Run to confirm fail**

```bash
cargo test -p astedit --test rename_test --locked rename_with_anchor_picks_matching_definition
```

Expected: failure — astedit currently ignores `--anchor` and renames every reference to `User`, including via the struct.

(Actually for this fixture, both defs are `pub`, but neither file references the other; the resolver yields only the same-file references for each. Without an anchor we'd partition by confidence and emit both. With the anchor, we filter to the chosen def's references.)

- [ ] **Step 3: Implement anchor handling**

In `crates/astedit/src/commands/rename.rs`, replace the existing `if defs.len() > 1 && args.anchor.is_none()` block with a richer version that also picks the right def when an anchor IS present:

```rust
    // Step 2: find defs by name. Multi-def disambiguation:
    let defs: Vec<&codegraph_core::index::Definition> = index
        .definitions
        .iter()
        .filter(|d| d.name == args.old)
        .collect();

    let chosen_def: Option<&codegraph_core::index::Definition> = match defs.len() {
        0 => None,
        1 => Some(defs[0]),
        _ => {
            // Multi-def. Require --anchor.
            match args.anchor.as_deref() {
                Some(anchor) => {
                    let (file, line) = parse_anchor(anchor)?;
                    defs.iter()
                        .find(|d| d.file == file && d.line == line)
                        .copied()
                        .ok_or_else(|| anyhow::anyhow!(
                            "anchor {file}:{line} did not match any definition of {}",
                            args.old,
                        ))?
                        .into()
                }
                None => {
                    let candidates: Vec<crate::serialize::Candidate> = defs
                        .iter()
                        .map(|d| crate::serialize::Candidate {
                            file: d.file.clone(),
                            line: d.line,
                            kind: def_kind_str(d.kind).to_string(),
                        })
                        .collect();
                    let data = RenameData {
                        subcommand: "rename",
                        dry_run: !args.apply,
                        needs_anchor: Some(true),
                        candidates: Some(candidates),
                        applied: None,
                        skipped: None,
                        errors: None,
                    };
                    if args.json {
                        print_json(data)?;
                    }
                    return Ok(2);
                }
            }
        }
    };
```

Add the anchor parser helper at the bottom of the file:

```rust
/// Parse a `--anchor FILE:LINE` value into `(file, line)`. The file is the
/// repo-relative, forward-slash-normalized form the index uses; the line is
/// 1-based.
fn parse_anchor(s: &str) -> anyhow::Result<(String, usize)> {
    let (file, line) = s.rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("--anchor expected FILE:LINE, got {s:?}"))?;
    let line: usize = line.parse()
        .map_err(|_| anyhow::anyhow!("--anchor line must be a positive integer, got {line:?}"))?;
    if line == 0 {
        anyhow::bail!("--anchor line must be 1-based, got 0");
    }
    Ok((file.to_string(), line))
}
```

Then filter the resolved set to only references that would resolve via `chosen_def`. For the MVP, the cheapest correct rule is:

- If a `chosen_def` exists, only resolved refs whose **`r.definition` (when present)** equals it OR whose `r.reference.file` matches the chosen def's file (for same-file references) are kept.
- If `chosen_def` is `None` (zero defs), pass through all resolved refs (the rename can still surface name-only as low-confidence skipped sites).

Insert this filtering immediately after the chosen_def block, before the `for r in &resolved` loop:

```rust
    let resolved: Vec<Resolved<'_>> = if let Some(def) = chosen_def {
        resolved
            .into_iter()
            .filter(|r| match r.confidence {
                // Low-confidence (name-only) refs aren't tied to any specific
                // definition — preserve them so the skipped[low-confidence]
                // lane still surfaces them regardless of which def --anchor
                // picked.
                Confidence::Low => true,
                _ => match r.definition {
                    Some(d) => std::ptr::eq(d, def),
                    None => r.reference.file == def.file,
                },
            })
            .collect()
    } else {
        resolved
    };
```

The pointer-equality fast path works because `Resolved.definition` carries the same `&Definition` reference the resolver picked from `index.definitions`; `chosen_def` is also a reference into that same vector, so `std::ptr::eq` is sound. The `Confidence::Low` carve-out keeps Task 11's name-only behaviour intact when multi-def + anchor is in play.

- [ ] **Step 4: Run the test**

```bash
cargo test -p astedit --test rename_test --locked rename_with_anchor_picks_matching_definition
```

Expected: PASS.

- [ ] **Step 5: Suite + clippy**

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: 60 tests (52 + 8 rename integration tests). Clippy clean.

- [ ] **Step 6: Commit**

```bash
git add crates/astedit/tests/rename_test.rs crates/astedit/src/commands/rename.rs
git commit -m "feat(astedit): --anchor FILE:LINE disambiguates multi-def renames"
```

---

## Task 16: `apply.rs` — atomic write + drift detection helpers

**Files:**
- Create: `crates/astedit/src/apply.rs`
- Modify: `crates/astedit/src/main.rs`

Two free functions, each unit-tested. The rename pipeline (Tasks 17–18) calls them when `--apply` is set; in dry-run they're unreached.

- [ ] **Step 1: Write the failing unit tests**

Create `crates/astedit/src/apply.rs`:

```rust
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use codegraph_core::hash::compute_file_hash;
use codegraph_core::index::FileMeta;

use crate::error::AstEditError;

/// Atomic per-file write. Writes `bytes` to a temp file in the same directory
/// as `target`, then `rename(2)`s it over `target`. Same-directory placement
/// keeps `rename` atomic on every supported filesystem.
///
/// Returns `WriteFailed` on any IO error, with `os_code` populated from
/// `io::Error::raw_os_error()`.
pub fn write_atomic(target: &Path, bytes: &[u8]) -> Result<(), AstEditError> {
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    let file_name = target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("astedit.tmp");
    let tmp_path = dir.join(format!(".{file_name}.astedit.tmp"));
    let map_err = |e: std::io::Error| AstEditError::WriteFailed {
        file: target.to_string_lossy().into_owned(),
        os_code: e.raw_os_error(),
        message: e.to_string(),
    };

    {
        let mut f = File::create(&tmp_path).map_err(map_err)?;
        f.write_all(bytes).map_err(map_err)?;
        f.sync_all().map_err(map_err)?;
    }
    fs::rename(&tmp_path, target).map_err(|e| {
        // Best-effort cleanup; ignore failure since the original is still intact.
        let _ = fs::remove_file(&tmp_path);
        AstEditError::WriteFailed {
            file: target.to_string_lossy().into_owned(),
            os_code: e.raw_os_error(),
            message: e.to_string(),
        }
    })?;
    Ok(())
}

/// Detect whether `target` has drifted from the snapshot recorded in
/// `Index.file_meta` at index time.
///
/// Returns `Ok(())` if the file is unchanged (length match, OR length differs
/// but the on-demand SHA matches the supplied `index_hash`).
/// Returns `Err(HashMismatch)` if drift is detected (length mismatch AND
/// either no `index_hash` was supplied or the recomputed hash differs).
/// Returns `Err(WriteFailed)` on IO failure during the stat call.
///
/// `index_hash` is `None` when the index didn't hash the file eagerly
/// (it never does in PR 1's `build_index`). When `None` AND lengths
/// differ, drift is assumed — there's no cheap way to confirm.
pub fn check_drift(
    target: &Path,
    rel_path: &str,
    meta: &FileMeta,
    index_hash: Option<&codegraph_core::hash::FileHash>,
) -> Result<(), AstEditError> {
    let stat = fs::metadata(target).map_err(|e| AstEditError::WriteFailed {
        file: rel_path.to_string(),
        os_code: e.raw_os_error(),
        message: e.to_string(),
    })?;
    if stat.len() == meta.len {
        return Ok(());
    }

    // Length mismatch — fall back to SHA-256.
    let on_disk = compute_file_hash(target).map_err(|e| match e {
        codegraph_core::CoreError::Io { source, .. } => AstEditError::WriteFailed {
            file: rel_path.to_string(),
            os_code: source.raw_os_error(),
            message: source.to_string(),
        },
    })?;
    match index_hash {
        Some(h) if h == &on_disk => Ok(()),
        _ => Err(AstEditError::HashMismatch { file: rel_path.to_string() }),
    }
}

/// Stat `target` and return its current length. Used by the race-window
/// guard right before a write — compared against the length read at step 5a.
pub fn current_len(target: &Path, rel_path: &str) -> Result<u64, AstEditError> {
    fs::metadata(target)
        .map(|m| m.len())
        .map_err(|e| AstEditError::WriteFailed {
            file: rel_path.to_string(),
            os_code: e.raw_os_error(),
            message: e.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::TempDir;

    #[test]
    fn write_atomic_replaces_existing_content() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("x.txt");
        fs::write(&target, b"old").unwrap();

        write_atomic(&target, b"new content").unwrap();

        let mut s = String::new();
        File::open(&target).unwrap().read_to_string(&mut s).unwrap();
        assert_eq!(s, "new content");
    }

    #[test]
    fn write_atomic_leaves_no_temp_files() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("y.txt");
        fs::write(&target, b"old").unwrap();
        write_atomic(&target, b"new").unwrap();

        let leftovers: Vec<PathBuf> = fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| {
                let p = e.unwrap().path();
                let n = p.file_name().unwrap().to_string_lossy().to_string();
                if n.ends_with(".astedit.tmp") { Some(p) } else { None }
            })
            .collect();
        assert!(leftovers.is_empty(), "leftover temp files: {leftovers:?}");
    }

    #[test]
    fn check_drift_ok_when_length_matches() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("z.txt");
        fs::write(&target, b"hello").unwrap();

        let meta = FileMeta { len: 5 };
        check_drift(&target, "z.txt", &meta, None).unwrap();
    }

    #[test]
    fn check_drift_errors_when_length_differs_and_no_hash() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("z.txt");
        fs::write(&target, b"hello world").unwrap();

        let meta = FileMeta { len: 5 };
        let err = check_drift(&target, "z.txt", &meta, None).unwrap_err();
        assert_eq!(err.kind(), "hash-mismatch");
    }
}
```

- [ ] **Step 2: Add `mod apply;` to main.rs**

Edit `crates/astedit/src/main.rs`:

```rust
mod apply;
mod cli;
mod commands;
mod error;
mod output;
mod serialize;

use clap::Parser;
use cli::{Cli, Command};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Rename(a) => commands::rename::run(a)?,
    };
    std::process::exit(code);
}
```

- [ ] **Step 3: Run the tests**

```bash
cargo test -p astedit --locked apply::tests
```

Expected: 4 tests pass.

- [ ] **Step 4: Suite + clippy**

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: 64 tests (60 + 4 apply unit tests). Clippy clean.

- [ ] **Step 5: Commit**

```bash
git add crates/astedit/src/apply.rs crates/astedit/src/main.rs
git commit -m "feat(astedit): add atomic write + drift detection helpers"
```

---

## Task 17: TDD — `--apply` integration writes correctly

**Files:**
- Create: `crates/astedit/tests/fixtures/apply_write/main.rs`
- Modify: `crates/astedit/tests/rename_test.rs`
- Modify: `crates/astedit/src/commands/rename.rs`

Confirms `--apply` actually writes edits to disk, dry-run produces no writes, and `bytes_changed` is the signed difference between new and old bytes summed across edits.

- [ ] **Step 1: Create the fixture**

```bash
mkdir -p crates/astedit/tests/fixtures/apply_write
```

Write `crates/astedit/tests/fixtures/apply_write/main.rs`:

```rust
struct User {
    id: u64,
}

fn make() -> User {
    User { id: 42 }
}
```

- [ ] **Step 2: Append the failing test**

Append to `crates/astedit/tests/rename_test.rs`:

```rust
#[test]
fn rename_apply_writes_changes_to_disk() {
    let tmp = copy_fixture("apply_write");
    let path = tmp.path().to_str().unwrap();
    let target = tmp.path().join("main.rs");
    let before = std::fs::read_to_string(&target).unwrap();
    assert!(before.contains("struct User"));

    let (code, data) = run_astedit_json(&[
        "rename", "User", "Account",
        "--path", path,
        "--apply",
        "--json",
    ]);

    assert_eq!(code, 0);
    assert_eq!(data["dry_run"], false);
    assert!(data["errors"].as_array().unwrap().is_empty());

    let after = std::fs::read_to_string(&target).unwrap();
    assert!(!after.contains("struct User"), "User should be renamed");
    assert!(after.contains("struct Account"), "Account should appear: {after}");
    // The body literal: `User { id: 42 }` → `Account { id: 42 }`.
    assert!(after.contains("Account { id: 42 }"), "body literal not renamed: {after}");
    // Function return type: `-> User` → `-> Account`.
    assert!(after.contains("-> Account"), "return type not renamed: {after}");

    // `bytes_changed` should be (new - old) * #edits, where new=7 (Account),
    // old=4 (User), delta=+3 per edit.
    let applied = data["applied"].as_array().unwrap();
    let entry = applied.iter().find(|f| f["file"].as_str().unwrap().ends_with("main.rs")).unwrap();
    let bytes_changed = entry["bytes_changed"].as_i64().unwrap();
    let edit_count = entry["edits"].as_array().unwrap().len() as i64;
    assert_eq!(bytes_changed, 3 * edit_count, "bytes_changed should be 3 per edit");
}
```

- [ ] **Step 3: Run to confirm fail**

```bash
cargo test -p astedit --test rename_test --locked rename_apply_writes_changes_to_disk
```

Expected: failure — the current pipeline never writes (`build_edits` is dry-run only).

- [ ] **Step 4: Wire `--apply` into `build_edits`**

In `crates/astedit/src/commands/rename.rs`, replace `build_edits` with a version that performs the full apply pipeline (drift check → read → splice → race-window guard → atomic write) when `args.apply` is true. The function returns a typed `AstEditError` on apply-time failures so the caller can route them into `errors[]` per the spec's "errors entries are non-fatal" rule:

```rust
fn build_edits(
    file: &str,
    refs: &[&Resolved<'_>],
    args: &RenameArgs,
    root: &std::path::Path,
    index: &codegraph_core::index::Index,
) -> Result<AppliedFile, AstEditError> {
    let old_len = args.old.len();
    let new_len = args.new.len();
    let mut edits: Vec<AppliedEdit> = Vec::new();
    for r in refs {
        edits.push(AppliedEdit {
            line: r.reference.line,
            col: r.reference.column,
            start_byte: r.reference.byte_offset,
            end_byte: r.reference.byte_offset + old_len,
            old: args.old.clone(),
            new: args.new.clone(),
            confidence: r.confidence.as_str(),
            reason: r.reason.as_str(),
        });
    }
    edits.sort_by(|a, b| b.start_byte.cmp(&a.start_byte));
    let bytes_changed = (new_len as i64 - old_len as i64) * edits.len() as i64;

    if args.apply {
        let abs = root.join(file);

        // Step 5a: drift check. Skip the file if it changed since indexing.
        let meta = index.file_meta.get(file).ok_or_else(|| AstEditError::HashMismatch {
            file: file.to_string(),
        })?;
        crate::apply::check_drift(&abs, file, meta, None)?;

        // Read after drift passes.
        let source = std::fs::read(&abs).map_err(|e| AstEditError::WriteFailed {
            file: file.to_string(),
            os_code: e.raw_os_error(),
            message: e.to_string(),
        })?;
        let original_len = source.len() as u64;
        let mut bytes = source;

        // Defensive node-kind check + splice in descending byte order.
        for e in &edits {
            if e.end_byte > bytes.len() || &bytes[e.start_byte..e.end_byte] != args.old.as_bytes() {
                return Err(AstEditError::NodeKindMismatch {
                    file: file.to_string(),
                    line: e.line,
                    col: e.col,
                });
            }
            bytes.splice(e.start_byte..e.end_byte, args.new.bytes());
        }

        // Step 5e: race-window guard. Re-stat just before the write and
        // compare against the length we read into memory. Same-length
        // concurrent writes slip through — accepted trade-off (spec).
        let current = crate::apply::current_len(&abs, file)?;
        if current != original_len {
            return Err(AstEditError::ConcurrentWrite { file: file.to_string() });
        }

        // Step 5f: atomic write.
        crate::apply::write_atomic(&abs, &bytes)?;
    }

    Ok(AppliedFile {
        file: file.to_string(),
        bytes_changed,
        edits,
    })
}
```

In the caller (the `for (file, refs) in by_file { … }` loop in `run`), route apply-time errors into `errors[]` instead of bubbling them. Replace the loop with:

```rust
    for (file, refs) in by_file {
        match build_edits(&file, &refs, &args, path, &index) {
            Ok(entry) => applied.push(entry),
            Err(e) => errors.push(crate::serialize::ErrorEntry::from(&e)),
        }
    }
```

Add the missing imports to the top of `rename.rs` (only the lines that aren't already present from earlier tasks):

```rust
use crate::error::AstEditError;
```

(`crate::serialize::{AppliedEdit, AppliedFile, RenameData, SkippedSite}` and the `ErrorEntry::from(&AstEditError)` `From` impl are already in scope from Tasks 5–8.)

- [ ] **Step 5: Run the test**

```bash
cargo test -p astedit --test rename_test --locked rename_apply_writes_changes_to_disk
```

Expected: PASS.

- [ ] **Step 6: Re-run the dry-run tests**

```bash
cargo test -p astedit --test rename_test --locked
```

Expected: every prior test still passes (dry-run still skips the `if args.apply` block).

- [ ] **Step 7: Suite + clippy**

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: 65 tests (64 + 1 apply integration test). Clippy clean.

- [ ] **Step 8: Commit**

```bash
git add crates/astedit/tests/fixtures/apply_write/ \
        crates/astedit/tests/rename_test.rs \
        crates/astedit/src/commands/rename.rs
git commit -m "feat(astedit): --apply performs atomic byte splicing with node-kind guard"
```

---

## Task 18: TDD — drift between index and apply emits `hash-mismatch`

**Files:**
- Modify: `crates/astedit/tests/rename_test.rs`
- Modify: `crates/astedit/src/commands/rename.rs`

The whole `apply_write` flow is gated on `apply::check_drift`. If the file changed between index build and apply (longer or shorter), the file is skipped with `error_kind: "hash-mismatch"` — other files in the same invocation still apply normally. This is the load-bearing safety property.

This test deliberately races the index against a mid-flight file edit. Because `astedit` builds the index, then iterates files, then writes, the race is: build_index runs, the test modifies the file from outside, then `--apply` checks drift and bails on that file.

We achieve this by writing two files into the fixture, then **before** invoking astedit, modifying one of them so its on-disk length differs from what `build_index` would now see. Since the test calls `build_index` indirectly via astedit, we need a different trick: run astedit twice. First run with `--apply` rewrites file A. Second run uses the stale length from build_index — but build_index re-runs each invocation. So this approach doesn't reproduce drift.

A cleaner repro: spawn astedit with the `_ASTEDIT_DRIFT_INJECT_FILE` env var set, which makes the pipeline pretend `index.file_meta[<file>].len` is some bogus value. That's a debug back-door we don't want in production.

**Simpler repro**: structure the pipeline so it stats the file once at step 5a (pre-flight) and the test trims the file *between* `build_index` and step 5a. The window is small but doable because we shell out:

Actually the cleanest design: the rename pipeline reads `index.file_meta[file].len`, then immediately stats the file. Both happen inside the pipeline, so racing from outside is hard. Instead, we make `check_drift` an injection point: the test populates a fixture, runs `astedit --apply`, observes a successful write — and then for the drift test, we modify the file AFTER `build_index` but BEFORE the per-file pre-flight by running astedit with a `--delay-before-apply` flag we add purely for testing.

That's noisy. **Pivot:** test `apply::check_drift` directly as a unit test (already done in Task 16) and skip the integration test for drift. The handoff lists "file-changed-between-index-and-apply (hash-mismatch)" as a required test case, so we need *something* end-to-end.

**Final approach:** the test uses two passes. Pass 1: agent runs `astedit --apply rename User Account` on the fixture. Pass 2: the test mutates `main.rs` externally (appends a comment), then runs `astedit --apply rename Account UserAccount`. In pass 2, build_index will see the freshly-appended comment, so `file_meta.len` will match the on-disk length — no drift. So this trick doesn't reproduce drift either.

**Real solution:** drift is hard to integration-test deterministically without a back-door. We accept this and replace the end-to-end drift test with a *targeted* test that:

1. Builds an `Index` directly from the fixture (calling `codegraph_core::build_index`).
2. Mutates the fixture file (changing its length).
3. Calls `apply::check_drift` with the now-stale `index.file_meta[file]`.
4. Asserts it returns `HashMismatch`.

This is an integration test of the drift path even though it doesn't go through the binary's CLI. It pins the contract `check_drift` enforces and exercises real index data. End-to-end CLI drift testing can be added in a follow-up PR with a debug-only `--inject-drift` flag if it ever blocks an agent.

- [ ] **Step 1: Append the integration test**

Append to `crates/astedit/tests/rename_test.rs`:

```rust
#[test]
fn drift_between_index_and_apply_emits_hash_mismatch() {
    // Use the same fixture as the apply test.
    let tmp = copy_fixture("apply_write");

    // Build an index against the fresh fixture.
    let index = codegraph_core::index::build_index(tmp.path()).unwrap();
    // file_meta keys are repo-relative, forward-slash-normalized.
    let (rel, meta) = index
        .file_meta
        .iter()
        .find(|(k, _)| k.ends_with("main.rs"))
        .expect("main.rs in file_meta");
    let original_len = meta.len;

    // Mutate the file so its length differs from the snapshot.
    let target = tmp.path().join("main.rs");
    let mut content = std::fs::read_to_string(&target).unwrap();
    content.push_str("\n// drift bait\n");
    std::fs::write(&target, &content).unwrap();
    assert_ne!(content.len() as u64, original_len);

    // The drift checker now sees length mismatch + no recorded hash → error.
    let err = astedit::apply::check_drift(&target, rel, meta, None)
        .expect_err("expected hash-mismatch");
    assert_eq!(err.kind(), "hash-mismatch");
}
```

This calls `astedit::apply::check_drift` directly — which means `apply` must be reachable from the integration test. Since `astedit` is a binary crate, the integration test can't `use` its modules unless we expose them via a lib target.

Two options:
- **A.** Add `[lib]` to `crates/astedit/Cargo.toml` with `name = "astedit"`, then `pub use` the relevant modules from `src/lib.rs`. The `[[bin]]` keeps using `main.rs` as before.
- **B.** Skip this integration test; rely on the unit tests in Task 16 to pin `check_drift`'s behaviour.

Option A is cleaner for testability. Go with A.

- [ ] **Step 2: Convert astedit to a bin + lib crate**

Edit `crates/astedit/Cargo.toml` to add a `[lib]` section after `[package]`:

```toml
[lib]
name = "astedit"
path = "src/lib.rs"

[[bin]]
name = "astedit"
path = "src/main.rs"
```

Create `crates/astedit/src/lib.rs`:

```rust
//! Internal library crate. The astedit binary at `src/main.rs` re-uses these
//! modules, and integration tests under `tests/` consume them directly for
//! cases that would otherwise need test-only back-doors in the CLI.

pub mod apply;
pub mod cli;
pub mod commands;
pub mod error;
pub mod output;
pub mod serialize;
```

Edit `crates/astedit/src/main.rs` to use the lib crate instead of declaring the modules itself:

```rust
use astedit::cli::{Cli, Command};
use astedit::commands;
use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Rename(a) => commands::rename::run(a)?,
    };
    std::process::exit(code);
}
```

Update inter-module `crate::*` paths in `apply.rs`, `commands/rename.rs`, `serialize.rs` if any reference `crate::error` or similar — they remain valid because `crate::` inside the lib now refers to the lib root.

- [ ] **Step 3: Build and re-run all existing tests**

```bash
cargo build --workspace --locked
cargo test --workspace --locked
```

Expected: the lib/bin restructure compiles and all 60 tests still pass.

- [ ] **Step 4: Run the new drift test**

```bash
cargo test -p astedit --test rename_test --locked drift_between_index_and_apply_emits_hash_mismatch
```

Expected: PASS.

- [ ] **Step 5: Suite + clippy**

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: 66 tests (65 + 1 drift integration test). Clippy clean.

- [ ] **Step 6: Commit**

```bash
git add crates/astedit/Cargo.toml \
        crates/astedit/src/lib.rs \
        crates/astedit/src/main.rs \
        crates/astedit/tests/rename_test.rs
git commit -m "feat(astedit): expose lib + integration-test drift detection"
```

---

## Task 19: SKILL.md final content + verify discoverability

**Files:**
- Modify: `skills/ny-astedit/SKILL.md`

Replace the placeholder with the agent-discovery hook. Follow the `codegraph` template: a `description:` field packed with trigger phrases (English + Thai), and a body that explains the safety model, JSON envelope, and dry-run-default.

- [ ] **Step 1: Write the final SKILL.md**

Replace `skills/ny-astedit/SKILL.md` entirely with:

```markdown
---
name: ny-astedit
description: PREFER THIS over manual `sed`/`grep -rl … | xargs sed`/multi-file Edit batches whenever you need to rename a symbol across files. `astedit rename <OLD> <NEW>` parses the project with tree-sitter via codegraph, resolves cross-file imports with confidence scores, and rewrites only the references the resolver vouches for. Dry-run by default; pass `--apply` to write. Atomic per-file writes, length-based drift detection with SHA-256 fallback. Trigger BEFORE running `sed -i s/X/Y/g`, `rg -l X | xargs sed`, or chains of `Edit` calls renaming the same identifier. Also use when the user asks "rename X to Y", "เปลี่ยนชื่อ symbol", "rename this struct/fn/class across the project", "เปลี่ยน X เป็น Y ทั้งโปรเจกต์". Returns `{schema_version:1, data:{applied,skipped,errors}}` JSON. Supports Rust, TypeScript, TSX, JavaScript, Python.
---

# astedit

`astedit` is the write-side companion to `codegraph` in the `skills` monorepo. `codegraph` answers "where is X used?" without writing anything; `astedit` answers "rewrite all those sites to Y" with a safety model designed for agents that chain tool calls without inspecting diffs between them.

## When to use

- The user says "rename X to Y", "rename this symbol across the project", "เปลี่ยนชื่อ X เป็น Y".
- You are about to run `sed -i s/Old/New/g` across multiple files. **Stop.** Run `astedit rename Old New` instead — it disambiguates definitions, respects import boundaries, and reports the per-file changes structurally.
- You are about to issue a series of `Edit` calls renaming the same identifier in N files. Use `astedit rename` and inspect the dry-run envelope first.

`astedit` is not rust-analyzer / tsserver. It does not resolve types, expand macros, or chase re-exports. References that traverse alias or wildcard re-exports show up under `skipped[]` so you can review them manually.

## Run

The skill ships a pre-built binary:

```bash
./scripts/astedit rename <OLD> <NEW> [flags]
```

If missing, run `./scripts/install.sh` (downloads from Releases) or `./scripts/build-skills.sh` (local cargo build) from the `skills` repo root.

## Subcommand: `rename`

```
astedit rename <OLD> <NEW>  [--path DIR]  [--apply]  [--json]
                            [--lang LANG] [--anchor FILE:LINE]
```

- `--path DIR` — project root to scan; default current directory.
- `--apply` — actually write edits. Without it, astedit reports what it *would* do and exits without writing.
- `--json` — emit `{schema_version:1, data:…}` instead of the human-readable preview.
- `--lang LANG` — restrict to one language (`rust`, `typescript`, `javascript`, `python`).
- `--anchor FILE:LINE` — required when `<OLD>` has more than one definition. Pass `--anchor src/user.rs:12` to pick the definition at that location.

## Safety model

- **Dry-run by default.** No writes unless `--apply` is passed.
- **Atomic per-file writes.** Temp file in the same directory + `rename(2)`. No partial writes on crash.
- **Length-based drift detection.** Pre-flight stats each file and compares against the index snapshot. Mismatch ⇒ SHA-256 fallback; persistent mismatch ⇒ `error_kind: "hash-mismatch"`, skip the file.
- **Race-window guard.** Just before the atomic write, re-stat length. Same-length concurrent writes slip through (accepted trade-off — git is the final accountability layer).
- **No built-in change-count cap.** A 200-file rename of a project-wide util is legitimate. Trust the dry-run preview.

## JSON envelope

`--json` emits exactly this shape (every field documented in the spec at `docs/superpowers/specs/2026-05-21-astedit-design.md`):

```json
{
  "schema_version": 1,
  "data": {
    "subcommand": "rename",
    "dry_run": true,
    "applied": [{"file": "src/lib.rs", "bytes_changed": 12, "edits": [...]}],
    "skipped": [{"file": "...", "skip_reason": "low-confidence", ...}],
    "errors":  [{"error_kind": "hash-mismatch", "file": "..."}]
  }
}
```

Multi-def disambiguation: when `<OLD>` has multiple definitions and `--anchor` is absent, `data` wraps `needs_anchor: true` + a `candidates` array, and the process exits non-zero. Use one of the candidates' `file:line` as `--anchor` and retry.

Exit status:
- `0` — invocation valid; `applied` may be empty.
- non-zero — multi-def without `--anchor`, or every targeted file ended up in `errors[]` with no successful applies.

## Out of scope (today)

- `astedit rewrite --pattern P --rewrite R` — coming in PR 3.
- Recipe files (`astedit apply recipe.yaml`) — future work.
- Type-aware rename (would need rust-analyzer / tsserver embeddings) — future work.
```

- [ ] **Step 2: Rebuild the skill binary**

```bash
./scripts/build-skills.sh
```

Expected: `done: 3 skill binary(ies)` with a line for `skills/ny-astedit/scripts/astedit`.

- [ ] **Step 3: Smoke-test the installed binary**

```bash
./skills/ny-astedit/scripts/astedit --help
./skills/ny-astedit/scripts/astedit rename Foo Bar --json --path /tmp
```

Expected: `--help` shows the `rename` subcommand. The `rename` invocation on `/tmp` (empty of source code) returns `{"schema_version":1,"data":{"subcommand":"rename","dry_run":true,"applied":[],"skipped":[],"errors":[]}}` and exits 0.

- [ ] **Step 4: Commit**

```bash
git add skills/ny-astedit/SKILL.md
git commit -m "docs(skills): finalize ny-astedit SKILL.md with discovery hook"
```

---

## Task 20: Final regression — fmt + clippy + test + cargo tree -d + open PR

**Files:** none modified (verification gate + PR).

- [ ] **Step 1: Format check**

```bash
cargo fmt --all --check
```

If anything is unformatted, run `cargo fmt --all` and either amend the most recent commit or land a separate `style: cargo fmt --all` commit.

- [ ] **Step 2: Clippy with the CI gate**

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: clean. Zero new `#[allow(...)]` annotations unless justified inline.

- [ ] **Step 3: Full test suite**

```bash
cargo test --workspace --locked
```

Expected count: **66 tests pass** (the 48 baseline from PR 1 + 18 new astedit tests = 1 error_kind unit + 3 serialize unit + 4 apply unit + 10 rename integration tests across Tasks 8–15 and 17–18). If the codemap/codegraph/codegraph-core counts changed without a matching test edit, behaviour drifted somewhere — revert and investigate.

(The handoff lists 9 integration scenarios; this plan implements all of them — same-file, cross-file, glob, name-only, alias re-export, wildcard re-export, multi-def w/o anchor, --anchor disambiguation, drift/hash-mismatch — plus 1 extra integration test for `--apply` writes (Task 17). The 10th rename test (Task 17's `rename_apply_writes_changes_to_disk`) wasn't on the handoff matrix but is required to exercise the --apply path end-to-end. Race-window guard and parse-error are not exercised by integration tests in PR 2 — both fire in code paths that need PR 3's machinery or production-time race conditions to trigger deterministically.)

- [ ] **Step 4: Audit for `tree-sitter` duplication**

```bash
cargo tree -d
```

Expected: no `tree-sitter` line. If `ast-grep-language` pulls a different `tree-sitter` version than `codegraph-core`'s grammar crates, this command will print both. That's a merge-blocker; resolve by aligning versions before opening the PR.

- [ ] **Step 5: Build the release artifacts**

```bash
./scripts/build-skills.sh
```

Expected: `done: 3 skill binary(ies)` with `skills/ny-astedit/scripts/astedit` present and executable.

- [ ] **Step 6: Open the PR**

```bash
git push -u origin feat/astedit-rename
gh pr create --base main --head feat/astedit-rename \
  --title "feat(astedit): rename MVP (astedit PR 2/3)" \
  --body "$(cat <<'EOF'
## Summary

Ships the `astedit rename <OLD> <NEW>` MVP — the second of three PRs rolling out astedit on top of the `codegraph-core` foundation landed in #2.

- New binary crate `crates/astedit/` and skill `skills/ny-astedit/`. Uses the existing `codegraph-core::{build_index, resolve_refs}` to find references, then byte-level splicing (no AST re-parse) driven by each `Reference.byte_offset`.
- Full safety model: **dry-run by default**, `--apply` opts in. Atomic per-file writes (temp + `rename(2)`). Length-based drift detection with SHA-256 fallback via `codegraph_core::compute_file_hash`. Race-window guard. Defensive `node-kind-mismatch` check before each splice.
- `AstEditError` enum maps 1:1 to the six spec-locked `error_kind` strings (`parse-error`, `hash-mismatch`, `concurrent-write`, `node-kind-mismatch`, `write-failed`, `pattern-compile`).
- JSON envelope `{schema_version:1, data:{subcommand, dry_run, applied, skipped, errors}}`. Multi-def without `--anchor` ⇒ `needs_anchor: true` + `candidates[]` + non-zero exit.
- Alias re-export sites surfaced as `skipped{re-export-alias}` with `via_alias`; wildcard re-export sites as `skipped{wildcard-reexport}` with `via_module`.
- Low-confidence (name-only) refs surfaced as `skipped{low-confidence}` — never auto-renamed.
- Workspace gains exact-pinned `ast-grep-{core,config,language} = "=0.38.x"` dependencies. They're declared in `astedit/Cargo.toml` per spec but **unused at runtime in PR 2** — PR 3 consumes them. `cargo tree -d` confirmed no `tree-sitter` collision with `codegraph-core`'s grammar deps.
- `astedit` is shaped as bin + lib so integration tests can drive `apply::check_drift` directly without a back-door CLI flag.

No `codegraph-core` changes. No `codegraph` / `codemap` changes.

## Test plan
- [x] `cargo fmt --all --check`
- [x] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [x] `cargo test --workspace --locked` — **66 tests pass** (48 baseline + 18 new)
- [x] `cargo tree -d` — no `tree-sitter` duplication
- [x] `./scripts/build-skills.sh` produces 3 skill binaries (codemap, codegraph, astedit)
- [x] `./skills/ny-astedit/scripts/astedit rename Foo Bar --json --path /tmp` returns a v1 envelope and exits 0

Spec: `docs/superpowers/specs/2026-05-21-astedit-design.md`
Plan: `docs/superpowers/plans/2026-05-21-astedit-pr2-rename.md`
EOF
)"
```

Expected: PR URL printed. CI runs the same fmt + clippy + test gates; should be green on the first run unless `cargo tree -d` reports a duplication CI's `--locked` lockfile resolves differently than the local one.

- [ ] **Step 7: Mark this plan complete**

After merge, leave the plan file in place. PR 3 (`astedit rewrite`) will reference both this plan and `docs/superpowers/specs/2026-05-21-astedit-design.md` § Structural rewrite pipeline.
