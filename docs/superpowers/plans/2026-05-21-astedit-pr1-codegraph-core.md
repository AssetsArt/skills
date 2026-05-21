# astedit PR 1 — Extract `codegraph-core` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract `crates/codegraph` into a library crate `crates/codegraph-core` (shared between `codegraph` and the future `astedit`), add the additive surfaces astedit will need (`FileMeta`, `FileHash`, `compute_file_hash`, `AliasSite`, `WildcardSite`, `CoreError`), and invert the build/release iteration so lib-only crates ship without churn. Existing `codegraph` / `codemap` behaviour must not change; all 31 existing tests must remain green without edits.

**Architecture:** Pure mechanical move of four `.rs` files and a `queries/` tree from `crates/codegraph/src/` into `crates/codegraph-core/src/`, plus a thin set of additive types and one new helper module. `codegraph` keeps its CLI dispatcher, output envelope, and command handlers; it gains a `codegraph-core` path dep and swaps `crate::*` imports for `codegraph_core::*`. Build/release scripts switch from iterating `crates/*/` to `skills/ny-*/` so the new lib crate is structurally invisible; a separate audit step preserves the "binary crates must have a skill dir" guarantee.

**Tech Stack:** Rust 2021, Cargo workspace (`resolver = "2"`), tree-sitter 0.22 + per-language grammar crates, `sha2 = "0.10"`, `hex = "0.4"`, `thiserror = "1"`. CI: stable rustfmt + clippy `-D warnings` + cargo test.

**Branch:** `feat/ast-grep` (already checked out). All commits in this plan land on that branch; PR opens against `main` when Task 19 passes.

---

## File Structure

**Files moving (4 source files + 12 query files):**

| From | To |
| --- | --- |
| `crates/codegraph/src/index.rs` | `crates/codegraph-core/src/index.rs` |
| `crates/codegraph/src/resolve.rs` | `crates/codegraph-core/src/resolve.rs` |
| `crates/codegraph/src/lang.rs` | `crates/codegraph-core/src/lang.rs` |
| `crates/codegraph/src/walk.rs` | `crates/codegraph-core/src/walk.rs` |
| `crates/codegraph/src/queries/*.scm` (12 files) | `crates/codegraph-core/src/queries/*.scm` |

**Files being created:**

- `crates/codegraph-core/Cargo.toml`
- `crates/codegraph-core/src/lib.rs` — re-exports the public surface
- `crates/codegraph-core/src/hash.rs` — `FileHash` newtype + `compute_file_hash`
- `crates/codegraph-core/src/error.rs` — `CoreError` enum (`thiserror`)
- `crates/codegraph-core/tests/hash_test.rs`
- `crates/codegraph-core/tests/reexport_test.rs`
- Fixture files in `crates/codegraph-core/tests/fixtures/` (per-language re-export samples)

**Files being modified:**

- `Cargo.toml` (workspace root): add `sha2`, `hex`, `thiserror` to `[workspace.dependencies]`; add `rust-version = "1.74"` to `[workspace.package]`.
- `crates/codegraph/Cargo.toml`: drop the tree-sitter deps that move with the source; add `codegraph-core = { path = "../codegraph-core" }`.
- `crates/codegraph/src/main.rs`: remove `mod index; mod resolve; mod lang; mod walk;` declarations.
- `crates/codegraph/src/commands/{find_refs,callers,callees,impact}.rs`: swap `use crate::{index,resolve}::…` for `use codegraph_core::{…}`.
- `crates/codegraph/src/queries/*.scm`: stays in `codegraph-core` after Task 3; this row is here only because the parser code that reads these files moves with it.
- `crates/codegraph-core/src/queries/{rust,typescript,javascript,python}_imports.scm`: extend each with alias + wildcard re-export captures (Tasks 13–15).
- `crates/codegraph-core/src/index.rs`: add new public structs (`FileMeta`, `AliasSite`, `WildcardSite`), add fields to `Index`, populate during `build_index`, dispatch the new query captures (Tasks 8–9, 13–15).
- `scripts/build-skills.sh`: invert iteration from `crates/*/` to `skills/ny-*/` (Task 16).
- `.github/workflows/release.yml`: invert both crate-iterating loops, add new `[[bin]]`-without-skill audit step (Tasks 17–18).

---

## Task 1: Workspace dependencies + MSRV floor

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add MSRV + new workspace deps**

Open the workspace `Cargo.toml`. Inside `[workspace.package]` add a `rust-version` line directly after `edition`. Inside `[workspace.dependencies]` add three new lines after `ignore = "0.4"`.

Final state of the two blocks:

```toml
[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.74"
license = "MIT"
authors = ["AssetsArt"]
repository = "https://github.com/AssetsArt/skills"
homepage = "https://github.com/AssetsArt/skills"

[workspace.dependencies]
clap = { version = "4.5", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
ignore = "0.4"
sha2 = "0.10"
hex = "0.4"
thiserror = "1"
```

- [ ] **Step 2: Verify nothing breaks**

```bash
cargo check --workspace --locked
```

Expected: success, no warnings. Cargo.lock will gain entries for `sha2`, `hex`, `thiserror` but nothing else touches them yet.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore(workspace): add sha2/hex/thiserror deps and rust-version = 1.74"
```

---

## Task 2: Create empty `codegraph-core` crate

**Files:**
- Create: `crates/codegraph-core/Cargo.toml`
- Create: `crates/codegraph-core/src/lib.rs`

- [ ] **Step 1: Make the crate directory and Cargo.toml**

```bash
mkdir -p crates/codegraph-core/src
```

Write `crates/codegraph-core/Cargo.toml`:

```toml
[package]
name = "codegraph-core"
description = "Shared index, resolver, and language registry for codegraph and astedit."
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true
homepage.workspace = true

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
ignore = { workspace = true }
thiserror = { workspace = true }
sha2 = { workspace = true }
hex = { workspace = true }
tree-sitter = "0.22"
tree-sitter-rust = "0.21"
tree-sitter-typescript = "0.21"
tree-sitter-javascript = "0.21"
tree-sitter-python = "0.21"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Write empty lib.rs**

Write `crates/codegraph-core/src/lib.rs`:

```rust
//! Shared index, resolver, walker, and language registry for `codegraph`
//! (read-only cross-references) and `astedit` (write-side rename / structural
//! rewrite). Existing `codegraph` subcommands keep their behaviour; this crate
//! exists so `astedit` does not have to copy the parsing pipeline.
//!
//! Stability: every additive public type is marked `#[non_exhaustive]`. Add
//! fields freely; never delete or repurpose.
```

- [ ] **Step 3: Verify the workspace picks up the new member**

```bash
cargo check -p codegraph-core --locked
```

Expected: success. The `members = ["crates/*"]` glob automatically includes the new directory; no edit to the workspace root is needed.

- [ ] **Step 4: Verify the rest of the workspace still builds**

```bash
cargo check --workspace --locked
```

Expected: success.

- [ ] **Step 5: Commit**

```bash
git add crates/codegraph-core/
git commit -m "feat(codegraph-core): create empty lib crate"
```

---

## Task 3: Move `lang.rs` + queries to `codegraph-core`

**Files:**
- Move: `crates/codegraph/src/lang.rs` → `crates/codegraph-core/src/lang.rs`
- Move: `crates/codegraph/src/queries/` → `crates/codegraph-core/src/queries/` (12 `.scm` files)
- Modify: `crates/codegraph-core/src/lib.rs`
- Modify: `crates/codegraph/src/main.rs`

- [ ] **Step 1: Move the source file with git mv**

```bash
git mv crates/codegraph/src/lang.rs crates/codegraph-core/src/lang.rs
```

- [ ] **Step 2: Move the queries directory**

```bash
git mv crates/codegraph/src/queries crates/codegraph-core/src/queries
```

Verify with `ls crates/codegraph-core/src/queries/` — must show 12 `.scm` files: `javascript_defs.scm`, `javascript_imports.scm`, `javascript_refs.scm`, `python_defs.scm`, `python_imports.scm`, `python_refs.scm`, `rust_defs.scm`, `rust_imports.scm`, `rust_refs.scm`, `typescript_defs.scm`, `typescript_imports.scm`, `typescript_refs.scm`.

- [ ] **Step 3: Expose `lang` from the lib crate**

Append to `crates/codegraph-core/src/lib.rs`:

```rust

pub mod lang;
```

- [ ] **Step 4: Remove the `mod lang;` declaration from the binary**

In `crates/codegraph/src/main.rs`, delete the line:

```rust
mod lang;
```

This is the first of four removals from `main.rs`; the others land in Tasks 4–6.

- [ ] **Step 5: Build codegraph-core in isolation to confirm queries resolve**

```bash
cargo check -p codegraph-core --locked
```

Expected: success. Tree-sitter `include_str!("queries/…scm")` paths inside `lang.rs` are relative to the source file, so the move keeps them valid.

- [ ] **Step 6: Build the workspace — codegraph will break**

```bash
cargo check --workspace --locked
```

Expected: codegraph fails to compile because `crate::lang::*` no longer resolves. That breakage is fixed in Task 7; we proceed file by file so each move is small.

- [ ] **Step 7: Commit the move**

```bash
git add crates/codegraph-core/src/lang.rs \
        crates/codegraph-core/src/queries \
        crates/codegraph-core/src/lib.rs \
        crates/codegraph/src/main.rs
git commit -m "refactor(codegraph-core): move lang + queries from codegraph"
```

---

## Task 4: Move `walk.rs` to `codegraph-core`

**Files:**
- Move: `crates/codegraph/src/walk.rs` → `crates/codegraph-core/src/walk.rs`
- Modify: `crates/codegraph-core/src/lib.rs`
- Modify: `crates/codegraph/src/main.rs`

- [ ] **Step 1: Move the file**

```bash
git mv crates/codegraph/src/walk.rs crates/codegraph-core/src/walk.rs
```

- [ ] **Step 2: Expose `walk` from the lib crate**

Append to `crates/codegraph-core/src/lib.rs`:

```rust
pub mod walk;
```

- [ ] **Step 3: Drop `mod walk;` from main.rs**

In `crates/codegraph/src/main.rs`, delete the line:

```rust
mod walk;
```

- [ ] **Step 4: Build codegraph-core**

```bash
cargo check -p codegraph-core --locked
```

Expected: success.

- [ ] **Step 5: Commit**

```bash
git add crates/codegraph-core/src/walk.rs \
        crates/codegraph-core/src/lib.rs \
        crates/codegraph/src/main.rs
git commit -m "refactor(codegraph-core): move walk from codegraph"
```

---

## Task 5: Move `resolve.rs` to `codegraph-core`

**Files:**
- Move: `crates/codegraph/src/resolve.rs` → `crates/codegraph-core/src/resolve.rs`
- Modify: `crates/codegraph-core/src/lib.rs`
- Modify: `crates/codegraph/src/main.rs`

- [ ] **Step 1: Move the file**

```bash
git mv crates/codegraph/src/resolve.rs crates/codegraph-core/src/resolve.rs
```

- [ ] **Step 2: Update intra-core imports**

`resolve.rs` currently has `use crate::index::*` and `use crate::lang::*` (or similar). Those are still correct — both modules now live in the same crate (`codegraph-core`). No edit needed unless the file uses `use crate::walk::*` or similar; verify with:

```bash
grep -n 'use crate::' crates/codegraph-core/src/resolve.rs
```

Every `use crate::X` should reference a module that already lives in `codegraph-core` (currently: `lang`, soon: `index`). If you see anything else, stop and re-check.

- [ ] **Step 3: Expose `resolve` from the lib crate**

Append to `crates/codegraph-core/src/lib.rs`:

```rust
pub mod resolve;
```

- [ ] **Step 4: Drop `mod resolve;` from main.rs**

In `crates/codegraph/src/main.rs`, delete:

```rust
mod resolve;
```

- [ ] **Step 5: Build codegraph-core**

```bash
cargo check -p codegraph-core --locked
```

Expected: failure. `resolve.rs` references `crate::index::*` but `index` hasn't moved yet. That's expected — Task 6 fixes it. Do NOT commit yet; this is a transient broken state.

- [ ] **Step 6: Do NOT commit until Task 6 finishes**

Leave the working tree dirty. The next task moves `index.rs` and resolves the break atomically.

---

## Task 6: Move `index.rs` to `codegraph-core`

**Files:**
- Move: `crates/codegraph/src/index.rs` → `crates/codegraph-core/src/index.rs`
- Modify: `crates/codegraph-core/src/lib.rs`
- Modify: `crates/codegraph/src/main.rs`

- [ ] **Step 1: Move the file**

```bash
git mv crates/codegraph/src/index.rs crates/codegraph-core/src/index.rs
```

- [ ] **Step 2: Verify intra-core imports**

```bash
grep -n 'use crate::' crates/codegraph-core/src/index.rs
```

Should only reference `lang`, `walk`, or `resolve` — all of which now live in `codegraph-core`. If anything else appears (e.g. `use crate::output`), stop and re-check the original file.

- [ ] **Step 3: Expose `index` from the lib crate**

Append to `crates/codegraph-core/src/lib.rs`:

```rust
pub mod index;
```

- [ ] **Step 4: Drop `mod index;` from main.rs**

In `crates/codegraph/src/main.rs`, delete:

```rust
mod index;
```

After this edit, `crates/codegraph/src/main.rs` should have only `mod cli;`, `mod commands;`, `mod output;` left.

- [ ] **Step 5: Build codegraph-core standalone**

```bash
cargo check -p codegraph-core --locked
```

Expected: success. The four moved files now compile inside their new crate.

- [ ] **Step 6: Build the whole workspace**

```bash
cargo check --workspace --locked
```

Expected: codegraph still fails — its `commands/*.rs` reference `crate::index`, `crate::resolve`, etc. Task 7 fixes that.

- [ ] **Step 7: Commit Tasks 5 + 6 together**

```bash
git add crates/codegraph-core/src/resolve.rs \
        crates/codegraph-core/src/index.rs \
        crates/codegraph-core/src/lib.rs \
        crates/codegraph/src/main.rs
git commit -m "refactor(codegraph-core): move resolve + index from codegraph"
```

(Tasks 5 and 6 share a commit because Task 5 left the tree in a broken state — `resolve` depended on `index` which had not moved yet.)

---

## Task 7: Switch `codegraph` to depend on `codegraph-core`

**Files:**
- Modify: `crates/codegraph/Cargo.toml`
- Modify: `crates/codegraph/src/commands/find_refs.rs`
- Modify: `crates/codegraph/src/commands/callers.rs`
- Modify: `crates/codegraph/src/commands/callees.rs`
- Modify: `crates/codegraph/src/commands/impact.rs`

- [ ] **Step 1: Add the path dep, drop the tree-sitter deps**

Rewrite the `[dependencies]` block of `crates/codegraph/Cargo.toml` to:

```toml
[dependencies]
codegraph-core = { path = "../codegraph-core" }
clap = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
```

Removed: `ignore`, `tree-sitter`, `tree-sitter-rust`, `tree-sitter-typescript`, `tree-sitter-javascript`, `tree-sitter-python`. These now belong only to `codegraph-core`. `[dev-dependencies]` stays as-is (`tempfile = "3"`).

- [ ] **Step 2: Swap imports in `find_refs.rs`**

Edit `crates/codegraph/src/commands/find_refs.rs`. Replace the three `use crate::*` lines that reference moved modules:

```rust
use crate::cli::FindRefsArgs;
use codegraph_core::index::{build_index, RefKind};
use crate::output::print_json;
use codegraph_core::resolve::resolve_refs;
```

(`crate::cli::*` and `crate::output::*` stay — those modules are still inside `codegraph`.)

- [ ] **Step 3: Swap imports in `callers.rs`**

Edit `crates/codegraph/src/commands/callers.rs`. The current import block:

```rust
use crate::cli::CallersArgs;
use crate::index::{build_index, DefKind, RefKind};
use crate::output::print_json;
use crate::resolve::resolve_refs;
```

becomes:

```rust
use crate::cli::CallersArgs;
use codegraph_core::index::{build_index, DefKind, RefKind};
use crate::output::print_json;
use codegraph_core::resolve::resolve_refs;
```

- [ ] **Step 4: Swap imports in `callees.rs`**

Same pattern. Final import block of `crates/codegraph/src/commands/callees.rs`:

```rust
use crate::cli::CalleesArgs;
use codegraph_core::index::{build_index, DefKind, RefKind};
use crate::output::print_json;
use codegraph_core::resolve::resolve_refs;
```

- [ ] **Step 5: Swap imports in `impact.rs`**

Final import block of `crates/codegraph/src/commands/impact.rs`:

```rust
use crate::cli::ImpactArgs;
use codegraph_core::index::{build_index, DefKind, RefKind};
use crate::output::print_json;
use codegraph_core::resolve::resolve_refs;
```

- [ ] **Step 6: Build the workspace**

```bash
cargo build --workspace --locked
```

Expected: success. Both crates compile, no warnings.

- [ ] **Step 7: Run every existing test**

```bash
cargo test --workspace --locked
```

Expected: all 31 tests pass (13 in `codemap` + 18 in `codegraph`). No test should require editing — that would mean the move changed behaviour. If any test fails, revert the last commit and investigate before continuing.

- [ ] **Step 8: Run clippy with the CI gate**

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: clean.

- [ ] **Step 9: Commit**

```bash
git add crates/codegraph/Cargo.toml crates/codegraph/src/commands/
git commit -m "refactor(codegraph): depend on codegraph-core, drop tree-sitter deps"
```

At this point: the move is complete. The next 5 tasks add the new types/fields/files astedit will need (still in `codegraph-core`).

---

## Task 8: Mark `Index` `#[non_exhaustive]`

**Files:**
- Modify: `crates/codegraph-core/src/index.rs`

- [ ] **Step 1: Add the attribute**

Find the `pub struct Index` declaration in `crates/codegraph-core/src/index.rs`. Add `#[non_exhaustive]` directly above it. Example before/after:

Before:
```rust
#[derive(Debug, Clone, Default)]
pub struct Index {
    pub definitions: Vec<Definition>,
    pub imports: Vec<Import>,
    pub references: Vec<Reference>,
}
```

After:
```rust
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Index {
    pub definitions: Vec<Definition>,
    pub imports: Vec<Import>,
    pub references: Vec<Reference>,
}
```

(If the existing struct has additional fields, keep them; the attribute is the only edit.)

- [ ] **Step 2: Verify the rest of the workspace still constructs `Index` correctly**

`Index::default()` is the only constructor used in this workspace (`crates/codegraph-core/src/index.rs` ~line 113 for the existing `build_index` and tests via the same path). Confirm with:

```bash
grep -rn 'Index {' crates/ --include='*.rs'
```

Expected: only matches inside `crates/codegraph-core/src/index.rs` itself (the struct definition). Test code uses `Index::default()` via `build_index`. If any consumer uses a struct literal, switch it to `Index::default()` or a builder before the attribute lands; failing to do so is a compile error.

- [ ] **Step 3: Build and test**

```bash
cargo build --workspace --locked && cargo test --workspace --locked
```

Expected: both succeed, 31 tests still pass.

- [ ] **Step 4: Commit**

```bash
git add crates/codegraph-core/src/index.rs
git commit -m "feat(codegraph-core): mark Index non_exhaustive for forward compat"
```

---

## Task 9: Add `FileMeta` + `Index.file_meta`

**Files:**
- Modify: `crates/codegraph-core/src/index.rs`
- Create: `crates/codegraph-core/tests/file_meta_test.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/codegraph-core/tests/file_meta_test.rs`:

```rust
use codegraph_core::index::build_index;
use std::fs;
use tempfile::TempDir;

#[test]
fn build_index_populates_file_meta_len() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("hello.rs");
    let body = "fn hello() {}\n";
    fs::write(&path, body).unwrap();

    let index = build_index(tmp.path()).unwrap();

    let key = "hello.rs"; // repo-relative, forward-slash-normalized
    let meta = index
        .file_meta
        .get(key)
        .expect("file_meta should contain hello.rs");
    assert_eq!(meta.len, body.len() as u64);
}
```

- [ ] **Step 2: Run the test to confirm it fails to compile**

```bash
cargo test -p codegraph-core --test file_meta_test --locked
```

Expected: compile error — `Index` has no field `file_meta`, and `FileMeta` is not defined.

- [ ] **Step 3: Define `FileMeta`**

Append to `crates/codegraph-core/src/index.rs` (place near the other top-level structs):

```rust
/// Snapshot of stat-time metadata for one source file. Populated during
/// `build_index`. Used by `astedit` for cheap drift detection between
/// index build and apply.
#[derive(Debug, Clone, Default, Serialize)]
#[non_exhaustive]
pub struct FileMeta {
    pub len: u64,
}
```

- [ ] **Step 4: Add `file_meta` to `Index`**

Edit the `Index` struct in the same file:

```rust
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Index {
    pub definitions: Vec<Definition>,
    pub imports: Vec<Import>,
    pub references: Vec<Reference>,
    pub file_meta: std::collections::HashMap<String, FileMeta>,
}
```

(If `HashMap` is already imported at the top of the file, drop the `std::collections::` prefix.)

- [ ] **Step 5: Populate `file_meta` during `build_index`**

Find the per-file loop inside `build_index` (currently around line 112 — the loop that reads file bytes and parses them). Immediately after the file bytes have been read into a `String`, insert:

```rust
index.file_meta.insert(
    f.path_str.clone(),               // already-normalized String key
    FileMeta { len: source.len() as u64 },
);
```

(Use whichever local variables are already in scope for the repo-relative path and the read bytes. The existing parser code names them — adopt those names; do NOT introduce new locals.)

- [ ] **Step 6: Run the test to confirm it passes**

```bash
cargo test -p codegraph-core --test file_meta_test --locked
```

Expected: PASS.

- [ ] **Step 7: Run the full suite**

```bash
cargo test --workspace --locked
```

Expected: all 32 tests pass (31 existing + 1 new).

- [ ] **Step 8: Commit**

```bash
git add crates/codegraph-core/src/index.rs \
        crates/codegraph-core/tests/file_meta_test.rs
git commit -m "feat(codegraph-core): add FileMeta + Index.file_meta drift snapshot"
```

---

## Task 10: Add `hash.rs` with `FileHash` newtype

**Files:**
- Create: `crates/codegraph-core/src/hash.rs`
- Modify: `crates/codegraph-core/src/lib.rs`
- Create: `crates/codegraph-core/tests/hash_test.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/codegraph-core/tests/hash_test.rs`:

```rust
use codegraph_core::hash::FileHash;

#[test]
fn file_hash_display_is_lowercase_hex() {
    let bytes = [0xab_u8; 32];
    let hash = FileHash::new(bytes);
    let s = format!("{hash}");
    assert_eq!(s.len(), 64);
    assert!(s.chars().all(|c| c.is_ascii_hexdigit() && (!c.is_alphabetic() || c.is_lowercase())));
    assert_eq!(s, "ab".repeat(32));
}

#[test]
fn file_hash_as_ref_returns_underlying_bytes() {
    let bytes = [0x42_u8; 32];
    let hash = FileHash::new(bytes);
    let slice: &[u8] = hash.as_ref();
    assert_eq!(slice, &bytes[..]);
}

#[test]
fn file_hash_equality() {
    assert_eq!(FileHash::new([0; 32]), FileHash::new([0; 32]));
    assert_ne!(FileHash::new([0; 32]), FileHash::new([1; 32]));
}
```

- [ ] **Step 2: Run the test to confirm it fails**

```bash
cargo test -p codegraph-core --test hash_test --locked
```

Expected: compile error — `codegraph_core::hash` does not exist.

- [ ] **Step 3: Create `hash.rs` with the newtype**

Write `crates/codegraph-core/src/hash.rs`:

```rust
use std::fmt;

/// SHA-256 of a file's contents. Wraps a raw 32-byte array so callers get
/// hex `Display`, structural equality, and a clear type name in `Debug`
/// output instead of an opaque byte array.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileHash([u8; 32]);

impl FileHash {
    /// Wrap a 32-byte digest.
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Return the underlying digest bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for FileHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FileHash({self})")
    }
}

impl fmt::Display for FileHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex::encode(self.0))
    }
}

impl AsRef<[u8]> for FileHash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
```

- [ ] **Step 4: Expose the module from `lib.rs`**

Append to `crates/codegraph-core/src/lib.rs`:

```rust
pub mod hash;
```

- [ ] **Step 5: Run the test to confirm it passes**

```bash
cargo test -p codegraph-core --test hash_test --locked
```

Expected: 3 tests pass.

- [ ] **Step 6: Run clippy to make sure the new module is clean**

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add crates/codegraph-core/src/hash.rs \
        crates/codegraph-core/src/lib.rs \
        crates/codegraph-core/tests/hash_test.rs
git commit -m "feat(codegraph-core): add FileHash newtype with hex Display"
```

---

## Task 11: Add `error.rs` with `CoreError`

**Files:**
- Create: `crates/codegraph-core/src/error.rs`
- Modify: `crates/codegraph-core/src/lib.rs`

- [ ] **Step 1: Create `error.rs`**

Write `crates/codegraph-core/src/error.rs`:

```rust
use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Errors returned by library-level helpers in `codegraph-core`. Binaries
/// (`codegraph`, `astedit`) wrap these in their own error enums to enrich
/// with command-specific context.
///
/// `#[non_exhaustive]` so variants can be added without a major-version bump.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CoreError {
    /// Filesystem read/stat failed for `path`.
    #[error("io error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}
```

- [ ] **Step 2: Expose from `lib.rs`**

Append to `crates/codegraph-core/src/lib.rs`:

```rust
pub mod error;
pub use error::CoreError;
```

- [ ] **Step 3: Build to confirm `thiserror` is wired correctly**

```bash
cargo build -p codegraph-core --locked
```

Expected: success.

- [ ] **Step 4: Commit**

```bash
git add crates/codegraph-core/src/error.rs \
        crates/codegraph-core/src/lib.rs
git commit -m "feat(codegraph-core): add CoreError enum via thiserror"
```

---

## Task 12: Add `compute_file_hash` helper

**Files:**
- Modify: `crates/codegraph-core/src/hash.rs`
- Modify: `crates/codegraph-core/tests/hash_test.rs`

- [ ] **Step 1: Extend the test file with a failing test**

Append to `crates/codegraph-core/tests/hash_test.rs`:

```rust
use codegraph_core::hash::compute_file_hash;
use std::fs;
use tempfile::TempDir;

#[test]
fn compute_file_hash_matches_known_sha256() {
    // SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("data.txt");
    fs::write(&path, b"abc").unwrap();

    let hash = compute_file_hash(&path).expect("hash");
    assert_eq!(
        format!("{hash}"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
}

#[test]
fn compute_file_hash_streams_large_input() {
    // Verify the streaming loop produces the same digest as a single-pass
    // call on input larger than the 64 KiB buffer.
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("big.bin");
    let body = vec![0x5a_u8; 200 * 1024]; // 200 KiB, well over the buffer
    fs::write(&path, &body).unwrap();

    let hash = compute_file_hash(&path).expect("hash");

    // Reference digest computed independently.
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(&body);
    let expected: [u8; 32] = h.finalize().into();
    assert_eq!(hash.as_bytes(), &expected);
}

#[test]
fn compute_file_hash_missing_file_returns_io_error() {
    use codegraph_core::CoreError;
    let tmp = TempDir::new().unwrap();
    let missing = tmp.path().join("not-there.bin");

    let err = compute_file_hash(&missing).expect_err("should error");
    match err {
        CoreError::Io { ref path, .. } => assert_eq!(path, &missing),
    }
}
```

- [ ] **Step 2: Run the test to confirm it fails**

```bash
cargo test -p codegraph-core --test hash_test --locked
```

Expected: compile error — `compute_file_hash` is undefined.

- [ ] **Step 3: Implement `compute_file_hash`**

Append to `crates/codegraph-core/src/hash.rs`:

```rust
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::CoreError;

/// Compute the SHA-256 of a file's contents. Streams through a 64 KiB
/// buffer so memory use stays bounded regardless of file size.
///
/// Returns `CoreError::Io` wrapping the path on any filesystem error.
pub fn compute_file_hash(path: &Path) -> Result<FileHash, CoreError> {
    let file = File::open(path).map_err(|source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|source| CoreError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let bytes: [u8; 32] = hasher.finalize().into();
    Ok(FileHash::new(bytes))
}
```

- [ ] **Step 4: Run the tests**

```bash
cargo test -p codegraph-core --test hash_test --locked
```

Expected: all 6 hash tests pass (3 from Task 10 + 3 new).

- [ ] **Step 5: Clippy**

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/codegraph-core/src/hash.rs \
        crates/codegraph-core/tests/hash_test.rs
git commit -m "feat(codegraph-core): add compute_file_hash streaming SHA-256 helper"
```

---

## Task 13: Detect Rust alias + wildcard re-exports

**Files:**
- Modify: `crates/codegraph-core/src/index.rs`
- Modify: `crates/codegraph-core/src/queries/rust_imports.scm`
- Create: `crates/codegraph-core/tests/fixtures/reexport_rust/`
- Create: `crates/codegraph-core/tests/reexport_test.rs`

- [ ] **Step 1: Define `AliasSite` and `WildcardSite`**

Append to `crates/codegraph-core/src/index.rs` (next to the other top-level structs):

```rust
/// One `pub use foo::Bar as Baz;` style alias re-export. Records both names
/// so the rename pipeline can surface the site under `skip_reason:
/// "re-export-alias"` with a `via_alias` field.
#[derive(Debug, Clone, Default, Serialize)]
#[non_exhaustive]
pub struct AliasSite {
    pub file: String,
    pub line: usize,
    pub alias: String,
    pub original: String,
    pub module_path: String,
}

/// One `pub use foo::*;` style wildcard re-export. The set of symbols that
/// cross the boundary cannot be known without parsing the target module's
/// exports, so the rename pipeline surfaces these under `skip_reason:
/// "wildcard-reexport"` with a `via_module` field.
#[derive(Debug, Clone, Default, Serialize)]
#[non_exhaustive]
pub struct WildcardSite {
    pub file: String,
    pub line: usize,
    pub module_path: String,
}
```

- [ ] **Step 2: Add the two new fields to `Index`**

Edit `Index`:

```rust
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Index {
    pub definitions: Vec<Definition>,
    pub imports: Vec<Import>,
    pub references: Vec<Reference>,
    pub file_meta: std::collections::HashMap<String, FileMeta>,
    pub alias_reexports: std::collections::HashMap<String, Vec<AliasSite>>,
    pub wildcard_reexports: std::collections::HashMap<String, Vec<WildcardSite>>,
}
```

The map key is the symbol name being re-exported (`alias` for alias maps, the module path's last segment for wildcards). This lets the resolver look up "is name X involved in any alias re-export?" in O(1).

- [ ] **Step 3: Extend the Rust imports query with re-export captures**

Append to `crates/codegraph-core/src/queries/rust_imports.scm`:

```scheme
; pub use foo::Bar as Baz;   -- alias re-export
(use_declaration
  (visibility_modifier) @vis
  argument: (use_as_clause
    path: (_) @path
    alias: (identifier) @alias)) @reexport_alias

; pub use foo::*;            -- wildcard re-export
(use_declaration
  (visibility_modifier) @vis
  argument: (use_wildcard
    (_) @path)) @reexport_wildcard
```

These coexist with the existing `@import` captures because tree-sitter queries match any pattern whose node shape fits; the parser dispatches on capture name.

- [ ] **Step 4: Add fixture files**

Create `crates/codegraph-core/tests/fixtures/reexport_rust/lib.rs`:

```rust
mod inner;

pub use inner::Bar as Baz;
pub use inner::widgets::*;

pub use inner::Untouched;
```

Create `crates/codegraph-core/tests/fixtures/reexport_rust/inner.rs`:

```rust
pub struct Bar;
pub struct Untouched;

pub mod widgets {
    pub struct Gadget;
}
```

- [ ] **Step 5: Write a failing parser test**

Create `crates/codegraph-core/tests/reexport_test.rs`:

```rust
use codegraph_core::index::build_index;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn rust_alias_reexport_recorded() {
    let index = build_index(&fixture("reexport_rust")).unwrap();
    let sites = index
        .alias_reexports
        .get("Baz")
        .expect("Baz alias should be recorded under its local name");
    assert_eq!(sites.len(), 1);
    let site = &sites[0];
    assert!(site.file.ends_with("lib.rs"), "got {}", site.file);
    assert_eq!(site.alias, "Baz");
    assert_eq!(site.original, "Bar");
    assert!(site.module_path.contains("inner"));
}

#[test]
fn rust_wildcard_reexport_recorded() {
    let index = build_index(&fixture("reexport_rust")).unwrap();
    let sites = index
        .wildcard_reexports
        .get("widgets")
        .expect("widgets wildcard re-export should be recorded");
    assert_eq!(sites.len(), 1);
    assert!(sites[0].file.ends_with("lib.rs"));
    assert!(sites[0].module_path.contains("inner::widgets"));
}

#[test]
fn rust_non_pub_use_is_not_a_reexport() {
    // `use inner::Untouched;` (no `pub`) is a plain import, not a re-export.
    // It must NOT appear in either re-export table.
    let index = build_index(&fixture("reexport_rust")).unwrap();
    assert!(
        index.alias_reexports.values().flat_map(|v| v.iter()).all(|s| s.alias != "Untouched"),
        "Untouched is a pub use without alias — should not be in alias_reexports",
    );
}
```

- [ ] **Step 6: Run to confirm it fails**

```bash
cargo test -p codegraph-core --test reexport_test --locked
```

Expected: failure — the parser does not yet dispatch on `@reexport_alias` / `@reexport_wildcard`, so both maps are empty.

- [ ] **Step 7: Wire the new captures into the parser**

In `crates/codegraph-core/src/index.rs`, locate the function that processes import-query matches (look for where `@import`, `@path`, `@name`, `@alias`, `@group` capture names are matched — typically a `match capture_name {…}` block inside the imports loop).

Extend the dispatch with two new arms. The exact code depends on the function's existing local-variable names; the canonical shape is:

```rust
// Inside the per-match loop, after collecting captures by name into locals
// (`path`, `name`, `alias`, etc.):
let pattern_index = mat.pattern_index; // tree_sitter::QueryMatch::pattern_index

// The query file lists patterns in source order. After the move in Task 3
// + the appends in Step 3, the order is:
//   0: scoped_identifier single path
//   1: use_as_clause aliased single path
//   2: scoped_use_list group
//   3: use_wildcard glob (existing import)
//   4: pub use_as_clause -- @reexport_alias
//   5: pub use_wildcard  -- @reexport_wildcard
// Re-confirm by reading the .scm file before relying on these indices.

match pattern_index {
    4 => {
        // alias re-export: pub use <path> as <alias>;
        if let (Some(path), Some(alias)) = (path.clone(), alias.clone()) {
            // `original` for Rust's `use_as_clause` is the last segment of `path`.
            let original = path
                .rsplit("::")
                .next()
                .unwrap_or(&path)
                .to_string();
            index
                .alias_reexports
                .entry(alias.clone())
                .or_default()
                .push(AliasSite {
                    file: rel_path.clone(),
                    line: line_number,
                    alias,
                    original,
                    module_path: path,
                });
        }
    }
    5 => {
        // wildcard re-export: pub use <path>::*;
        if let Some(path) = path.clone() {
            let key = path
                .rsplit("::")
                .next()
                .unwrap_or(&path)
                .to_string();
            index
                .wildcard_reexports
                .entry(key)
                .or_default()
                .push(WildcardSite {
                    file: rel_path.clone(),
                    line: line_number,
                    module_path: path,
                });
        }
    }
    _ => { /* existing import dispatch continues here */ }
}
```

(Use the variable names that already exist in the function for `rel_path` and `line_number`; do not introduce new names.)

- [ ] **Step 8: Run the test**

```bash
cargo test -p codegraph-core --test reexport_test --locked
```

Expected: 3 tests pass.

- [ ] **Step 9: Run the full suite to confirm no regression**

```bash
cargo test --workspace --locked
```

Expected: all 35 tests pass (31 existing + file_meta + 3 hash + 3 reexport).

- [ ] **Step 10: Commit**

```bash
git add crates/codegraph-core/src/index.rs \
        crates/codegraph-core/src/queries/rust_imports.scm \
        crates/codegraph-core/tests/fixtures/reexport_rust/ \
        crates/codegraph-core/tests/reexport_test.rs
git commit -m "feat(codegraph-core): detect Rust pub use alias + wildcard re-exports"
```

---

## Task 14: Detect TypeScript + JavaScript re-exports

**Files:**
- Modify: `crates/codegraph-core/src/queries/typescript_imports.scm`
- Modify: `crates/codegraph-core/src/queries/javascript_imports.scm`
- Modify: `crates/codegraph-core/src/index.rs` (pattern_index arms if TS/JS dispatch is separate from Rust)
- Create: `crates/codegraph-core/tests/fixtures/reexport_ts/lib.ts`
- Create: `crates/codegraph-core/tests/fixtures/reexport_js/lib.js`
- Modify: `crates/codegraph-core/tests/reexport_test.rs`

- [ ] **Step 1: Write the failing TS test**

Append to `crates/codegraph-core/tests/reexport_test.rs`:

```rust
#[test]
fn ts_alias_reexport_recorded() {
    let index = build_index(&fixture("reexport_ts")).unwrap();
    let sites = index
        .alias_reexports
        .get("Baz")
        .expect("Baz alias should be recorded");
    assert_eq!(sites.len(), 1);
    assert!(sites[0].file.ends_with("lib.ts"));
    assert_eq!(sites[0].alias, "Baz");
    assert_eq!(sites[0].original, "Bar");
    assert_eq!(sites[0].module_path, "./inner");
}

#[test]
fn ts_wildcard_reexport_recorded() {
    let index = build_index(&fixture("reexport_ts")).unwrap();
    // Wildcard re-exports key on the trailing module-name segment.
    let sites = index
        .wildcard_reexports
        .get("widgets")
        .expect("widgets wildcard re-export should be recorded");
    assert_eq!(sites.len(), 1);
    assert!(sites[0].file.ends_with("lib.ts"));
    assert_eq!(sites[0].module_path, "./widgets");
}
```

Create the fixture `crates/codegraph-core/tests/fixtures/reexport_ts/lib.ts`:

```typescript
export { Bar as Baz } from "./inner";
export * from "./widgets";
```

Create `crates/codegraph-core/tests/fixtures/reexport_ts/inner.ts`:

```typescript
export class Bar {}
```

Create `crates/codegraph-core/tests/fixtures/reexport_ts/widgets.ts`:

```typescript
export class Gadget {}
```

- [ ] **Step 2: Run to confirm both fail**

```bash
cargo test -p codegraph-core --test reexport_test --locked ts_alias_reexport_recorded ts_wildcard_reexport_recorded
```

Expected: both fail — TS imports query does not yet emit re-export captures.

- [ ] **Step 3: Extend the TS query**

Append to `crates/codegraph-core/src/queries/typescript_imports.scm`:

```scheme
; export { Bar as Baz } from "./foo";    -- alias re-export
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @original
      alias: (identifier) @alias))
  source: (string) @path) @reexport_alias

; export * from "./foo";                  -- wildcard re-export
(export_statement
  "*"
  source: (string) @path) @reexport_wildcard
```

(Tree-sitter-typescript node names — verify against `node_types.json` of the pinned `tree-sitter-typescript 0.21` if a capture fails to match.)

- [ ] **Step 4: Update the parser dispatch for the new pattern indices**

In `crates/codegraph-core/src/index.rs`, the TS/JS imports parser is structurally identical to the Rust one but lives in a separate per-language function. Mirror the two arms added in Task 13. Strip TypeScript string-literal quotes when populating `module_path` (the `(string)` node captures include the surrounding quotes — trim them with `.trim_matches('"').trim_matches('\'')`).

- [ ] **Step 5: Run the TS tests**

```bash
cargo test -p codegraph-core --test reexport_test --locked ts_alias_reexport_recorded ts_wildcard_reexport_recorded
```

Expected: PASS.

- [ ] **Step 6: Write the failing JS test**

Append to `crates/codegraph-core/tests/reexport_test.rs`:

```rust
#[test]
fn js_alias_reexport_recorded() {
    let index = build_index(&fixture("reexport_js")).unwrap();
    let sites = index
        .alias_reexports
        .get("Baz")
        .expect("Baz alias should be recorded");
    assert_eq!(sites.len(), 1);
    assert!(sites[0].file.ends_with("lib.js"));
    assert_eq!(sites[0].alias, "Baz");
    assert_eq!(sites[0].original, "Bar");
    assert_eq!(sites[0].module_path, "./inner");
}

#[test]
fn js_wildcard_reexport_recorded() {
    let index = build_index(&fixture("reexport_js")).unwrap();
    let sites = index
        .wildcard_reexports
        .get("widgets")
        .expect("widgets wildcard re-export should be recorded");
    assert_eq!(sites.len(), 1);
    assert!(sites[0].file.ends_with("lib.js"));
    assert_eq!(sites[0].module_path, "./widgets");
}
```

Create `crates/codegraph-core/tests/fixtures/reexport_js/lib.js`:

```javascript
export { Bar as Baz } from "./inner";
export * from "./widgets";
```

Create `crates/codegraph-core/tests/fixtures/reexport_js/inner.js`:

```javascript
export class Bar {}
```

Create `crates/codegraph-core/tests/fixtures/reexport_js/widgets.js`:

```javascript
export class Gadget {}
```

- [ ] **Step 7: Run to confirm fail**

```bash
cargo test -p codegraph-core --test reexport_test --locked js_alias_reexport_recorded js_wildcard_reexport_recorded
```

Expected: both fail.

- [ ] **Step 8: Extend the JS query**

Append to `crates/codegraph-core/src/queries/javascript_imports.scm` (tree-sitter-javascript uses the same `export_statement` shape as tree-sitter-typescript for ES modules):

```scheme
; export { Bar as Baz } from "./foo";
(export_statement
  (export_clause
    (export_specifier
      name: (identifier) @original
      alias: (identifier) @alias))
  source: (string) @path) @reexport_alias

; export * from "./foo";
(export_statement
  "*"
  source: (string) @path) @reexport_wildcard
```

The JS parser dispatch already mirrors the TS one (they share helper functions in `index.rs`). If they share a function, no additional Rust code is needed. If they don't, mirror the Rust arms.

- [ ] **Step 9: Run the JS tests**

```bash
cargo test -p codegraph-core --test reexport_test --locked js_alias_reexport_recorded js_wildcard_reexport_recorded
```

Expected: PASS.

- [ ] **Step 10: Full suite + clippy**

```bash
cargo test --workspace --locked && cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: 39 tests pass (35 from Task 13 + 4 new), clippy clean.

- [ ] **Step 11: Commit**

```bash
git add crates/codegraph-core/src/index.rs \
        crates/codegraph-core/src/queries/typescript_imports.scm \
        crates/codegraph-core/src/queries/javascript_imports.scm \
        crates/codegraph-core/tests/fixtures/reexport_ts/ \
        crates/codegraph-core/tests/fixtures/reexport_js/ \
        crates/codegraph-core/tests/reexport_test.rs
git commit -m "feat(codegraph-core): detect TS + JS alias + wildcard re-exports"
```

---

## Task 15: Detect Python alias + wildcard import re-exports

**Files:**
- Modify: `crates/codegraph-core/src/queries/python_imports.scm`
- Modify: `crates/codegraph-core/src/index.rs`
- Create: `crates/codegraph-core/tests/fixtures/reexport_py/lib.py`
- Create: `crates/codegraph-core/tests/fixtures/reexport_py/inner.py`
- Modify: `crates/codegraph-core/tests/reexport_test.rs`

Python has no explicit re-export syntax; the spec treats `from foo import Bar as Baz` and `from foo import *` as the alias/wildcard re-export markers. Detection without `__all__` awareness is a deliberate MVP simplification (documented in the spec under "Limited Support").

- [ ] **Step 1: Write the failing test**

Append to `crates/codegraph-core/tests/reexport_test.rs`:

```rust
#[test]
fn python_alias_import_treated_as_reexport_site() {
    let index = build_index(&fixture("reexport_py")).unwrap();
    let sites = index
        .alias_reexports
        .get("Baz")
        .expect("Baz alias should be recorded");
    assert_eq!(sites.len(), 1);
    assert!(sites[0].file.ends_with("lib.py"));
    assert_eq!(sites[0].alias, "Baz");
    assert_eq!(sites[0].original, "Bar");
    assert_eq!(sites[0].module_path, "inner");
}

#[test]
fn python_wildcard_import_treated_as_wildcard_reexport_site() {
    let index = build_index(&fixture("reexport_py")).unwrap();
    let sites = index
        .wildcard_reexports
        .get("widgets")
        .expect("widgets wildcard import should be recorded");
    assert_eq!(sites.len(), 1);
    assert!(sites[0].file.ends_with("lib.py"));
    assert_eq!(sites[0].module_path, "widgets");
}
```

Create `crates/codegraph-core/tests/fixtures/reexport_py/lib.py`:

```python
from inner import Bar as Baz
from widgets import *
```

Create `crates/codegraph-core/tests/fixtures/reexport_py/inner.py`:

```python
class Bar:
    pass
```

Create `crates/codegraph-core/tests/fixtures/reexport_py/widgets.py`:

```python
class Gadget:
    pass
```

- [ ] **Step 2: Run to confirm fail**

```bash
cargo test -p codegraph-core --test reexport_test --locked python_alias_import_treated_as_reexport_site python_wildcard_import_treated_as_wildcard_reexport_site
```

Expected: both fail.

- [ ] **Step 3: Extend the Python query**

Append to `crates/codegraph-core/src/queries/python_imports.scm`:

```scheme
; from foo import Bar as Baz    -- alias "re-export" (MVP: treat all aliased
; imports as candidate re-export sites; agent verifies via __all__ manually)
(import_from_statement
  module_name: (dotted_name) @path
  name: (aliased_import
    name: (dotted_name) @original
    alias: (identifier) @alias)) @reexport_alias

; from foo import *             -- wildcard "re-export"
(import_from_statement
  module_name: (dotted_name) @path
  (wildcard_import)) @reexport_wildcard
```

- [ ] **Step 4: Update the Python parser dispatch in `index.rs`**

Mirror the Rust/TS arms. The Python module-name comes from a `(dotted_name)` node — its text is already in dotted form (e.g. `pkg.sub`) and needs no quote-stripping.

- [ ] **Step 5: Run the tests**

```bash
cargo test -p codegraph-core --test reexport_test --locked python_alias_import_treated_as_reexport_site python_wildcard_import_treated_as_wildcard_reexport_site
```

Expected: PASS.

- [ ] **Step 6: Full suite + clippy**

```bash
cargo test --workspace --locked && cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: 41 tests pass, clippy clean.

- [ ] **Step 7: Commit**

```bash
git add crates/codegraph-core/src/index.rs \
        crates/codegraph-core/src/queries/python_imports.scm \
        crates/codegraph-core/tests/fixtures/reexport_py/ \
        crates/codegraph-core/tests/reexport_test.rs
git commit -m "feat(codegraph-core): detect Python alias + wildcard import re-export sites"
```

---

## Task 16: Invert iteration in `scripts/build-skills.sh`

**Files:**
- Modify: `scripts/build-skills.sh`

- [ ] **Step 1: Rewrite the build loop**

Replace the entire body of `scripts/build-skills.sh` with:

```bash
#!/usr/bin/env bash
# Build every skill (skills/ny-<name>/) by compiling the crate of the same
# name and copying the release binary into skills/ny-<name>/scripts/<name>.
#
# Iteration walks `skills/ny-*/`, NOT `crates/*/`. Lib-only crates (e.g.
# codegraph-core) have no matching skill dir and are not iterated. The
# CI release.yml runs a separate audit that fails if any crate with a
# [[bin]] section lacks a skill dir.
#
# Run from the repo root:
#   ./scripts/build-skills.sh

set -euo pipefail

command -v cargo >/dev/null || {
  echo "error: cargo not found on PATH" >&2
  echo "hint: install Rust via https://rustup.rs/" >&2
  exit 1
}

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

cargo build --workspace --release --locked

built=0
shopt -s nullglob
for skill_dir in skills/ny-*/; do
  skill_dir="${skill_dir%/}"               # strip trailing /
  name="${skill_dir#skills/ny-}"           # ny-codemap -> codemap
  binary="target/release/$name"
  if [ ! -f "$binary" ]; then
    echo "error: $binary missing; the crate may not declare [[bin]]" >&2
    exit 1
  fi
  mkdir -p "$skill_dir/scripts"
  cp "$binary" "$skill_dir/scripts/$name"
  chmod +x "$skill_dir/scripts/$name"
  echo "built $skill_dir/scripts/$name"
  built=$((built + 1))
done
echo "done: $built skill binary(ies)"
```

- [ ] **Step 2: Run the script and verify the existing skills still build**

```bash
./scripts/build-skills.sh
```

Expected output (order may vary):
```
built skills/ny-codegraph/scripts/codegraph
built skills/ny-codemap/scripts/codemap
done: 2 skill binary(ies)
```

The new `codegraph-core` crate must NOT appear in the loop output.

- [ ] **Step 3: Confirm the produced binaries still work**

```bash
./skills/ny-codemap/scripts/codemap stats --json --path .
./skills/ny-codegraph/scripts/codegraph find-refs build_index --json --path crates/codegraph-core
```

Expected: each prints a `{"schema_version": 1, "data": …}` envelope. Any other output is a regression.

- [ ] **Step 4: Commit**

```bash
git add scripts/build-skills.sh
git commit -m "build(scripts): invert build-skills.sh iteration to walk skills/ny-*"
```

---

## Task 17: Invert iteration in `.github/workflows/release.yml`

**Files:**
- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: Rewrite the "Audit crate/skill pairs" step**

In `.github/workflows/release.yml`, replace the `- name: Audit crate/skill pairs` step with:

```yaml
      - name: Audit skills exist
        shell: bash
        run: |
          set -euo pipefail
          shopt -s nullglob
          skills=(skills/ny-*/)
          if [ "${#skills[@]}" -eq 0 ]; then
            echo "::error::no skill dirs found under skills/ -- nothing to package"
            exit 1
          fi
          for s in "${skills[@]}"; do
            s="${s%/}"
            name="${s#skills/ny-}"
            if [ ! -f "crates/$name/Cargo.toml" ]; then
              echo "::error::skill $s has no matching crates/$name"
              exit 1
            fi
            if [ ! -f "$s/SKILL.md" ]; then
              echo "::error::skill $s missing SKILL.md"
              exit 1
            fi
          done
          echo "packaging ${#skills[@]} skill(s) for ${{ matrix.slug }}"
```

- [ ] **Step 2: Rewrite the "Package + checksum every crate/skill pair" step**

Replace its body with a `skills/ny-*/` loop:

```yaml
      - name: Package + checksum every skill
        shell: bash
        run: |
          set -euo pipefail
          tag="${GITHUB_REF_NAME}"
          slug="${{ matrix.slug }}"
          triple="${{ matrix.triple }}"
          out="release-assets"
          mkdir -p "$out"
          if command -v sha256sum >/dev/null; then SHA=(sha256sum); else SHA=(shasum -a 256); fi
          shopt -s nullglob
          for skill_dir in skills/ny-*/; do
            skill_dir="${skill_dir%/}"
            name="${skill_dir#skills/ny-}"
            tar -C "target/$triple/release" -czf "$out/$name-$tag-$slug.tar.gz" "$name"
            ( cd "$out" && "${SHA[@]}" "$name-$tag-$slug.tar.gz" > "$name-$tag-$slug.sha256" )
            echo "packaged $out/$name-$tag-$slug.tar.gz"
          done
```

- [ ] **Step 3: Lint the workflow locally if `actionlint` is available**

```bash
command -v actionlint >/dev/null && actionlint .github/workflows/release.yml || echo "(actionlint not installed; CI will lint)"
```

Expected: no errors. (If `actionlint` isn't installed, CI will lint at PR time — that's fine for this plan.)

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci(release): invert audit + package loops to walk skills/ny-*"
```

---

## Task 18: Add new audit step — `[[bin]]` crates must have a skill dir

**Files:**
- Modify: `.github/workflows/release.yml`

The inverted loops in Task 17 won't catch "you added a binary crate but forgot to create the skill dir." This task restores that guarantee with an explicit reverse audit.

- [ ] **Step 1: Insert the new step immediately after "Audit skills exist"**

In `.github/workflows/release.yml`, add this step:

```yaml
      - name: Audit binary crates have skill dirs
        shell: bash
        run: |
          set -euo pipefail
          shopt -s nullglob
          failed=0
          for c in crates/*/; do
            c="${c%/}"
            name="${c#crates/}"
            # Skip lib-only crates (no [[bin]] section in Cargo.toml).
            if ! grep -q '^\[\[bin\]\]' "$c/Cargo.toml"; then
              continue
            fi
            if [ ! -f "skills/ny-$name/SKILL.md" ]; then
              echo "::error::binary crate $name has no matching skills/ny-$name/SKILL.md"
              failed=1
            fi
          done
          if [ "$failed" -ne 0 ]; then
            exit 1
          fi
```

- [ ] **Step 2: Smoke-test locally**

```bash
bash -euo pipefail -c '
  shopt -s nullglob
  failed=0
  for c in crates/*/; do
    c="${c%/}"
    name="${c#crates/}"
    if ! grep -q "^\[\[bin\]\]" "$c/Cargo.toml"; then continue; fi
    if [ ! -f "skills/ny-$name/SKILL.md" ]; then
      echo "would fail: $name"
      failed=1
    fi
  done
  echo "result: failed=$failed"
'
```

Expected: `result: failed=0`. `codegraph-core` has no `[[bin]]` so it's skipped; `codegraph` and `codemap` both have `[[bin]]` and their skill dirs exist.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci(release): audit binary crates have matching skill dir"
```

---

## Task 19: Final regression — fmt + clippy + test on the full workspace

**Files:** none modified; this task is a verification gate.

- [ ] **Step 1: Format check**

```bash
cargo fmt --all --check
```

Expected: exit 0. If anything is unformatted, run `cargo fmt --all` and add the diff to the next task's commit.

- [ ] **Step 2: Clippy with the CI gate**

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: clean. If warnings appear, fix them in their source file. Allowed: zero `#[allow(...)]` annotations added by this PR unless explicitly justified inline with a comment.

- [ ] **Step 3: Full test suite**

```bash
cargo test --workspace --locked
```

Expected count: **41 tests pass** = 13 (codemap, unchanged) + 18 (codegraph, unchanged) + 1 (file_meta) + 6 (hash, 3 + 3) + 9 (reexport: 3 Rust + 2 TS + 2 JS + 2 Python). If the codemap/codegraph counts changed without a matching test edit, behaviour changed somewhere — revert and investigate.

- [ ] **Step 4: Build the release artifacts to confirm the inverted scripts work**

```bash
./scripts/build-skills.sh
```

Expected: `done: 2 skill binary(ies)`. The new `codegraph-core` crate must not appear.

- [ ] **Step 5: Verify the new audit script catches the failure mode it's meant to**

Smoke-test by temporarily renaming a skill dir, running the audit logic from Task 18 step 2, and confirming it errors. Restore immediately.

```bash
mv skills/ny-codemap skills/ny-codemap-disabled
bash -euo pipefail -c '
  shopt -s nullglob
  for c in crates/*/; do
    c="${c%/}"; name="${c#crates/}"
    grep -q "^\[\[bin\]\]" "$c/Cargo.toml" || continue
    if [ ! -f "skills/ny-$name/SKILL.md" ]; then
      echo "::error::binary crate $name has no matching skills/ny-$name/SKILL.md"
      exit 1
    fi
  done
' && echo "AUDIT MISSED THE FAILURE" || echo "audit correctly errored"
mv skills/ny-codemap-disabled skills/ny-codemap
```

Expected: `audit correctly errored`. If you see `AUDIT MISSED THE FAILURE`, the Task 18 logic has a bug — fix it before opening the PR.

- [ ] **Step 6: Open the PR**

```bash
git push -u origin feat/ast-grep
gh pr create --base main --head feat/ast-grep \
  --title "feat(codegraph-core): extract shared library crate (astedit PR 1/3)" \
  --body "$(cat <<'EOF'
## Summary
- Extracts `crates/codegraph` into a shared library crate `crates/codegraph-core` (index, resolver, walker, language registry, queries).
- Adds the additive surfaces that `astedit` (PR 2) will consume: `FileMeta`, `FileHash`, `compute_file_hash`, `AliasSite`, `WildcardSite`, `CoreError`. Every new struct is `#[non_exhaustive]`.
- Detects alias and wildcard re-exports for Rust (`pub use`), TypeScript / JavaScript (`export … from`), and Python (`from … import … as / *`). Stored in `Index.alias_reexports` and `Index.wildcard_reexports` so the upcoming rename pipeline can route those sites into its `skipped` lane.
- Inverts `scripts/build-skills.sh` and the two crate-iterating loops in `.github/workflows/release.yml` to walk `skills/ny-*/` instead of `crates/*/`, so lib-only crates are structurally invisible. A new audit step preserves the "binary crates must have a skill dir" guarantee.

No observable behaviour change for existing `codegraph` / `codemap` invocations.

## Test plan
- [x] `cargo fmt --all --check`
- [x] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [x] `cargo test --workspace --locked` — 41 tests pass (31 pre-existing + 10 new)
- [x] `./scripts/build-skills.sh` produces only `skills/ny-codemap` and `skills/ny-codegraph` binaries
- [x] Reverse audit (temporarily renamed `skills/ny-codemap`) correctly errors

Spec: `docs/superpowers/specs/2026-05-21-astedit-design.md`
Plan: `docs/superpowers/plans/2026-05-21-astedit-pr1-codegraph-core.md`
EOF
)"
```

Expected: PR URL printed. CI should be green on the first run; if the new audit step is the only failure, the bug is in Task 18 step 2 — read the failed step's stdout for which `crate/skill` pair tripped it.

- [ ] **Step 7: Mark this plan complete**

After merge, leave the plan file in place. PR 2 (`docs/superpowers/plans/2026-05-21-astedit-pr2-rename.md`) will be drafted next, referencing the additive surfaces this PR landed.
