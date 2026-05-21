# codegraph MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `codegraph`, a new Cargo workspace crate + `ny-codegraph` skill that answers semantic cross-reference questions agents cannot ask `codemap`: *find references to a symbol*, *who calls this function*, *what does this function call*, and *what would break if I change this*.

**Architecture:** Tree-sitter + scope resolution implemented in-process. At every invocation `codegraph` walks the project (`ignore` crate, same gitignore-aware traversal as `codemap`), parses each supported file once, and builds an in-memory `Index` of three tables: `definitions`, `imports`, `references`. A small `Resolver` then answers each subcommand by joining those tables. Every reported hit carries a `confidence` (`high`/`medium`/`low`) and a machine-readable `reason` (`same-file-scope` / `import-resolved` / `name-only`) so agents can decide what to trust.

**Tech Stack:** Rust 2021, `clap` derive, `tree-sitter` 0.22, `tree-sitter-{rust,typescript,javascript,python}` 0.21, `ignore` 0.4, `serde`/`serde_json`, `anyhow`. JSON envelope schema_version 1 (same contract as `codemap`).

**Accuracy bar (agreed):** Best-effort + confidence tagging. We do NOT promise rust-analyzer-grade precision. We DO promise: (a) same-file scope hits are reliable, (b) cross-file import-resolved hits are reliable, (c) name-only hits are emitted but tagged low so agents can downrank them. False positives are acceptable when explicit. False positives that look like high-confidence hits are NOT acceptable.

**Non-goals (MVP):**
- Method resolution against receiver types (e.g. distinguishing `user.save()` on `User` vs `Session`). Method calls are reported as references to the method *name* only.
- Re-export chains (`pub use foo::bar` in Rust, `export * from "./mod"` in TS).
- Macro expansion (Rust). Macros are treated as opaque calls — `println!` is a reference to `println`, the macro body is not walked.
- Dynamic dispatch through traits (Rust), `**kwargs`/`getattr` (Python), `obj[key]` (JS).
- Persistent on-disk cache. Every invocation rebuilds the index. Future work; out of scope here.

---

## File Structure

New files (created by this plan):

```
crates/codegraph/
├── Cargo.toml                                 # workspace inheritance; tree-sitter deps
├── src/
│   ├── main.rs                                # thin dispatcher (mirror codemap/main.rs)
│   ├── cli.rs                                 # all clap structs
│   ├── output.rs                              # print_json envelope (copy from codemap)
│   ├── walk.rs                                # gitignore-aware walker (copy from codemap)
│   ├── lang.rs                                # Language enum (copy from codemap, query_source split)
│   ├── index.rs                               # Definition/Import/Reference structs + Index builder
│   ├── resolve.rs                             # confidence-tagged resolution
│   ├── queries/
│   │   ├── rust_defs.scm                      # function_item, struct_item, ...
│   │   ├── rust_imports.scm                   # use declarations
│   │   ├── rust_refs.scm                      # call_expression, macro_invocation, path refs
│   │   ├── typescript_defs.scm                # function/class/interface declarations
│   │   ├── typescript_imports.scm             # import statements
│   │   ├── typescript_refs.scm                # call_expression, identifier refs
│   │   ├── javascript_defs.scm                # function/class declarations
│   │   ├── javascript_imports.scm             # import statements (also `require` left out)
│   │   ├── javascript_refs.scm                # call_expression refs
│   │   ├── python_defs.scm                    # function/class definitions
│   │   ├── python_imports.scm                 # import + from-import
│   │   └── python_refs.scm                    # call refs
│   └── commands/
│       ├── mod.rs
│       ├── find_refs.rs                       # `codegraph find-refs <name>`
│       ├── callers.rs                         # `codegraph callers <fn>`
│       ├── callees.rs                         # `codegraph callees <fn>`
│       └── impact.rs                          # `codegraph impact <symbol>`
└── tests/
    ├── fixtures/
    │   └── multi_lang/                        # cross-file fixture; see Task 3 contents
    │       ├── rust_app/
    │       ├── ts_app/
    │       ├── js_app/
    │       └── py_app/
    ├── find_refs_test.rs
    ├── callers_test.rs
    ├── callees_test.rs
    └── impact_test.rs

skills/ny-codegraph/
└── SKILL.md                                   # agent-discoverable manifest
                                               # scripts/ is gitignored at repo root
```

Modified files:
- `README.md` — add row to the Skill Index table.

The `walk.rs` / `lang.rs` / `output.rs` modules are deliberately **copied verbatim** from `codemap` rather than extracted into a shared workspace library crate. Rationale: every existing helper script and the release workflow assume `crates/*` are independent binaries. Introducing a `crates/_common` lib forces touching `build-skills.sh`, the release matrix audit step, and the release tarball convention. YAGNI — revisit once the third skill needs the same code.

---

## Output Schema

Every subcommand returns JSON `{schema_version: 1, data: <payload>}` where the payload is a JSON array. Each element:

```json
{
  "file": "src/lib.rs",
  "line": 42,
  "column": 12,
  "kind": "call" | "definition" | "import" | "reference",
  "name": "User",
  "context": "    let u = User::new();",
  "confidence": "high" | "medium" | "low",
  "reason": "same-file-scope" | "import-resolved" | "name-only"
}
```

- `kind=call`: a call expression / macro invocation referencing the symbol.
- `kind=reference`: an identifier reference that is not a call (type position, path expression).
- `kind=import`: the symbol appears in an import / use statement.
- `kind=definition`: the symbol's defining site (only emitted by `find-refs` so agents see where it lives).

Confidence rules:
- `high` + `same-file-scope`: definition and reference are in the same file, and no inner scope shadows the name between them.
- `high` + `import-resolved`: reference is in a file that imports the symbol from the file containing the definition.
- `medium` + `import-resolved`: ambiguous import (e.g. `use foo::*` in Rust, `from foo import *` in Python) where we can identify the module but not pinpoint the symbol.
- `low` + `name-only`: the name matches but we could not resolve which definition it points to.

---

## Subagent Briefing (read before any task)

Each task is independent enough to run in a fresh subagent. The mapping `crate name == "codegraph"` and `skill dir == "ny-codegraph"` is enforced by `scripts/build-skills.sh` and the release workflow's audit step (`.github/workflows/release.yml`). Do not rename either side. The pre-built binary lives at `skills/ny-codegraph/scripts/codegraph` and is `.gitignored`.

Verification commands you will use repeatedly (run from repo root unless noted):

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p codegraph
cargo test -p codegraph --test find_refs_test
./scripts/build-skills.sh
```

CI sets `RUSTFLAGS=-Dwarnings`; locally it is not set, so clippy is the gate.

---

## Phase 1 — Scaffold

### Task 1: Create the empty `codegraph` crate and skill dir

**Files:**
- Create: `crates/codegraph/Cargo.toml`
- Create: `crates/codegraph/src/main.rs`
- Create: `skills/ny-codegraph/SKILL.md`
- Modify: `README.md` (Skill Index table)

- [ ] **Step 1: Write the failing test (workspace member compiles)**

```bash
# A test in name only — we are exercising `cargo check` to validate the new member.
# No file edit yet; this step just confirms the starting state is red.
cargo check -p codegraph
```

Expected: FAIL with `error: package ID specification \`codegraph\` did not match any packages`.

- [ ] **Step 2: Create `crates/codegraph/Cargo.toml`**

```toml
[package]
name = "codegraph"
description = "Semantic cross-references for a codebase: find-refs, callers, callees, impact. Built for AI coding agents."
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true
homepage.workspace = true

[[bin]]
name = "codegraph"
path = "src/main.rs"

[dependencies]
clap = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
ignore = { workspace = true }
tree-sitter = "0.22"
tree-sitter-rust = "0.21"
tree-sitter-typescript = "0.21"
tree-sitter-javascript = "0.21"
tree-sitter-python = "0.21"
```

- [ ] **Step 3: Create `crates/codegraph/src/main.rs` (smallest compilable stub)**

```rust
fn main() {
    eprintln!("codegraph: not yet implemented");
    std::process::exit(2);
}
```

- [ ] **Step 4: Verify it builds**

Run: `cargo check -p codegraph`
Expected: PASS (warnings ok at this point; we tighten in Task 2).

- [ ] **Step 5: Create `skills/ny-codegraph/SKILL.md` (placeholder)**

```markdown
---
name: ny-codegraph
description: Use when you need to know what calls a function, where a symbol is used, or what would break if you change it. `codegraph` answers find-references, callers/callees, and impact-analysis questions across Rust, TypeScript, TSX, JavaScript, and Python. Trigger on phrases like "find references to X", "who calls Y", "what does Z call", "what breaks if I change W", "find all usages", "หาที่ใช้", "ใครเรียกฟังก์ชันนี้", "ถ้าแก้แล้วอะไรพัง".
---

# codegraph

Placeholder. Filled in by Task 21.
```

- [ ] **Step 6: Add row to `README.md` Skill Index table**

Modify `README.md` lines 13-15 — the existing table block:

```markdown
| Skill | What it does | Crate |
| --- | --- | --- |
| [ny-codemap](./skills/ny-codemap) | Survey a codebase: list files, show symbols, find definitions | `crates/codemap` |
| [ny-codegraph](./skills/ny-codegraph) | Semantic cross-references: find-refs, callers, callees, impact | `crates/codegraph` |
```

- [ ] **Step 7: Verify the release-audit invariant holds**

```bash
test -d crates/codegraph && test -d skills/ny-codegraph && echo "audit-ok"
```

Expected: prints `audit-ok` (this is the exact pairing the `.github/workflows/release.yml` "Audit crate/skill pairs" step enforces).

- [ ] **Step 8: Run the workspace gate**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Expected: PASS on all three. `codegraph` has no tests yet, so `cargo test` just compiles the empty binary.

- [ ] **Step 9: Commit**

```bash
git add crates/codegraph/Cargo.toml crates/codegraph/src/main.rs skills/ny-codegraph/SKILL.md README.md
git commit -m "feat(codegraph): scaffold empty crate and skill dir"
```

---

### Task 2: Copy `walk` / `lang` / `output` modules from `codemap` and wire `cli.rs` dispatcher

**Files:**
- Create: `crates/codegraph/src/walk.rs`
- Create: `crates/codegraph/src/lang.rs`
- Create: `crates/codegraph/src/output.rs`
- Create: `crates/codegraph/src/cli.rs`
- Create: `crates/codegraph/src/commands/mod.rs`
- Create: `crates/codegraph/src/commands/find_refs.rs` (stub)
- Create: `crates/codegraph/src/commands/callers.rs` (stub)
- Create: `crates/codegraph/src/commands/callees.rs` (stub)
- Create: `crates/codegraph/src/commands/impact.rs` (stub)
- Modify: `crates/codegraph/src/main.rs`

- [ ] **Step 1: Write a failing CLI smoke test**

Create `crates/codegraph/tests/cli_test.rs`:

```rust
use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_codegraph"))
}

#[test]
fn cli_help_lists_all_four_subcommands() {
    let out = Command::new(bin()).args(["--help"]).output().expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    for sub in ["find-refs", "callers", "callees", "impact"] {
        assert!(stdout.contains(sub), "missing subcommand {sub} in --help:\n{stdout}");
    }
}
```

Run: `cargo test -p codegraph --test cli_test`
Expected: FAIL (binary still prints "not yet implemented").

- [ ] **Step 2: Create `crates/codegraph/src/walk.rs`**

Copy `crates/codemap/src/walk.rs` byte-for-byte. The doc comments referencing `codemap` are fine — we are deliberately duplicating, not abstracting (see "File Structure" rationale above).

```rust
use crate::lang::Language;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

pub const IGNORED_DIRS: &[&str] = &["target", "node_modules", ".git", "dist", "build"];

pub struct SourceFile {
    pub path: PathBuf,
    pub language: Language,
}

pub fn default_walker(root: &Path) -> WalkBuilder {
    let mut b = WalkBuilder::new(root);
    b.hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true)
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !IGNORED_DIRS.iter().any(|d| *d == name.as_ref())
        });
    b
}

pub fn walk_sources(root: &Path) -> anyhow::Result<Vec<SourceFile>> {
    let mut out = Vec::new();
    for entry in default_walker(root).build() {
        let entry = entry?;
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.into_path();
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if let Some(language) = Language::from_extension(ext) {
            out.push(SourceFile { path, language });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}
```

- [ ] **Step 3: Create `crates/codegraph/src/lang.rs`**

Almost identical to `codemap`'s, but the `query_source` returns three separate slots (defs / imports / refs) instead of one. All slots return `None` until later tasks wire them up.

```rust
use tree_sitter::Language as TsLanguage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    TypeScript,
    Tsx,
    JavaScript,
    Python,
}

#[derive(Debug, Clone, Copy)]
pub enum QueryKind {
    Defs,
    Imports,
    Refs,
}

impl Language {
    pub fn name(self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::TypeScript => "typescript",
            Language::Tsx => "tsx",
            Language::JavaScript => "javascript",
            Language::Python => "python",
        }
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "rs" => Some(Language::Rust),
            "ts" => Some(Language::TypeScript),
            "tsx" => Some(Language::Tsx),
            "js" | "mjs" | "cjs" => Some(Language::JavaScript),
            "py" => Some(Language::Python),
            _ => None,
        }
    }

    pub fn ts_language(self) -> TsLanguage {
        match self {
            Language::Rust => tree_sitter_rust::language(),
            Language::TypeScript => tree_sitter_typescript::language_typescript(),
            Language::Tsx => tree_sitter_typescript::language_tsx(),
            Language::JavaScript => tree_sitter_javascript::language(),
            Language::Python => tree_sitter_python::language(),
        }
    }

    /// Returns `None` when the query for `(language, kind)` is not wired up yet.
    /// Tasks 3, 6, 8, 16, 17, 18 fill these in.
    pub fn query_source(self, _kind: QueryKind) -> Option<&'static str> {
        None
    }
}
```

- [ ] **Step 4: Create `crates/codegraph/src/output.rs`**

Identical to `codemap`'s.

```rust
use serde::Serialize;

#[derive(Serialize)]
struct Envelope<T: Serialize> {
    schema_version: u32,
    data: T,
}

pub fn print_json<T: Serialize>(data: T) -> anyhow::Result<()> {
    let env = Envelope { schema_version: 1, data };
    println!("{}", serde_json::to_string(&env)?);
    Ok(())
}
```

- [ ] **Step 5: Create `crates/codegraph/src/cli.rs`**

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "codegraph",
    version,
    about = "Semantic cross-references: find-refs, callers, callees, impact"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Find references to <name> across the project
    FindRefs(FindRefsArgs),
    /// List functions that call <fn>
    Callers(CallersArgs),
    /// List functions called by <fn>
    Callees(CalleesArgs),
    /// Transitive callers + type users of <symbol>
    Impact(ImpactArgs),
}

#[derive(clap::Args, Debug)]
pub struct FindRefsArgs {
    pub name: String,
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct CallersArgs {
    pub name: String,
    /// Recursive depth (1 = direct callers only)
    #[arg(long, default_value_t = 1)]
    pub depth: usize,
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct CalleesArgs {
    pub name: String,
    #[arg(long, default_value_t = 1)]
    pub depth: usize,
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct ImpactArgs {
    pub name: String,
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub json: bool,
}
```

- [ ] **Step 6: Create `crates/codegraph/src/commands/mod.rs`**

```rust
pub mod callees;
pub mod callers;
pub mod find_refs;
pub mod impact;
```

- [ ] **Step 7: Create each `commands/<sub>.rs` stub**

`commands/find_refs.rs`:

```rust
use crate::cli::FindRefsArgs;

pub fn run(_args: FindRefsArgs) -> anyhow::Result<()> {
    anyhow::bail!("find-refs: not yet implemented")
}
```

`commands/callers.rs`:

```rust
use crate::cli::CallersArgs;

pub fn run(_args: CallersArgs) -> anyhow::Result<()> {
    anyhow::bail!("callers: not yet implemented")
}
```

`commands/callees.rs`:

```rust
use crate::cli::CalleesArgs;

pub fn run(_args: CalleesArgs) -> anyhow::Result<()> {
    anyhow::bail!("callees: not yet implemented")
}
```

`commands/impact.rs`:

```rust
use crate::cli::ImpactArgs;

pub fn run(_args: ImpactArgs) -> anyhow::Result<()> {
    anyhow::bail!("impact: not yet implemented")
}
```

- [ ] **Step 8: Rewrite `crates/codegraph/src/main.rs`**

```rust
mod cli;
mod commands;
mod lang;
mod output;
mod walk;

use clap::Parser;
use cli::{Cli, Command};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::FindRefs(a) => commands::find_refs::run(a),
        Command::Callers(a) => commands::callers::run(a),
        Command::Callees(a) => commands::callees::run(a),
        Command::Impact(a) => commands::impact::run(a),
    }
}
```

- [ ] **Step 9: Verify the smoke test passes**

Run: `cargo test -p codegraph --test cli_test`
Expected: PASS — `--help` now lists `find-refs`, `callers`, `callees`, `impact`.

- [ ] **Step 10: Workspace gate**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Expected: PASS.

- [ ] **Step 11: Commit**

```bash
git add crates/codegraph/src crates/codegraph/tests
git commit -m "feat(codegraph): wire CLI dispatcher and shared infra"
```

---

## Phase 2 — Rust support (vertical slice)

The strategy: get all four subcommands working end-to-end for Rust first. Treat that as the spec for the other languages. The order inside Phase 2 follows data dependencies (you need defs before imports, imports before resolution, resolution before commands).

### Task 3: Multi-language fixture project (Rust files only this task)

**Files:**
- Create: `crates/codegraph/tests/fixtures/multi_lang/rust_app/src/lib.rs`
- Create: `crates/codegraph/tests/fixtures/multi_lang/rust_app/src/auth.rs`
- Create: `crates/codegraph/tests/fixtures/multi_lang/rust_app/src/handlers.rs`
- Create: `crates/codegraph/tests/fixtures/multi_lang/rust_app/Cargo.toml`
- Test: `crates/codegraph/tests/index_test.rs` (creates a new test file)

The fixture intentionally exercises: same-file references, cross-file `use` imports, glob imports, method calls, type references, and a function that is unused.

- [ ] **Step 1: Create `rust_app/Cargo.toml`**

```toml
[package]
name = "rust_app_fixture"
version = "0.0.0"
edition = "2021"
publish = false

[lib]
path = "src/lib.rs"
```

(The fixture is never compiled — it lives under `tests/fixtures/` and the parent `crates/codegraph` only reads its files. The `[package]` block makes humans happier.)

- [ ] **Step 2: Create `rust_app/src/lib.rs`**

```rust
pub mod auth;
pub mod handlers;

pub struct User {
    pub id: u32,
    pub name: String,
}

pub fn new_user(id: u32, name: &str) -> User {
    User { id, name: name.to_string() }
}

pub fn unused_helper() -> u32 {
    42
}
```

- [ ] **Step 3: Create `rust_app/src/auth.rs`**

```rust
use crate::User;

pub fn authenticate(u: &User) -> bool {
    !u.name.is_empty()
}

pub fn revoke(u: &User) -> bool {
    authenticate(u)
}
```

- [ ] **Step 4: Create `rust_app/src/handlers.rs`**

```rust
use crate::auth::authenticate;
use crate::{new_user, User};

pub fn login(name: &str) -> Option<User> {
    let u = new_user(1, name);
    if authenticate(&u) {
        Some(u)
    } else {
        None
    }
}

pub fn whoami(u: &User) -> String {
    u.name.clone()
}
```

- [ ] **Step 5: Sanity-check that the fixture is well-formed Rust**

```bash
cargo check --manifest-path crates/codegraph/tests/fixtures/multi_lang/rust_app/Cargo.toml
```

Expected: PASS with a warning about `unused_helper` being unused — that's intentional.

- [ ] **Step 6: Commit**

```bash
git add crates/codegraph/tests/fixtures/multi_lang/rust_app
git commit -m "test(codegraph): add rust fixture for cross-file indexing"
```

---

### Task 4: `index.rs` core data structures

**Files:**
- Create: `crates/codegraph/src/index.rs`
- Modify: `crates/codegraph/src/main.rs` (add `mod index;`)
- Test: `crates/codegraph/tests/index_test.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/codegraph/tests/index_test.rs`:

```rust
use codegraph_test_helper as _; // placeholder so the file compiles before helper exists

#[test]
fn empty_index_has_no_definitions() {
    // We exercise the public API once it exists. For now the test compiles
    // against the structs we are about to write.
    let idx = codegraph::index::Index::default();
    assert_eq!(idx.definitions.len(), 0);
    assert_eq!(idx.imports.len(), 0);
    assert_eq!(idx.references.len(), 0);
}
```

Wait — `codegraph` is a binary crate; tests cannot import its modules directly. Instead, expose them by adding a `[lib]` shim. Update this test to invoke the binary on an empty dir.

Replace the test file with:

```rust
use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_codegraph")) }

#[test]
fn find_refs_on_empty_dir_returns_empty_data() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = Command::new(bin())
        .args(["find-refs", "Nonexistent", "--json", "--path"])
        .arg(tmp.path())
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["schema_version"].as_u64().unwrap(), 1);
    assert_eq!(v["data"].as_array().unwrap().len(), 0);
}
```

Add `tempfile = "3"` to `crates/codegraph/Cargo.toml` under `[dev-dependencies]`:

```toml
[dev-dependencies]
tempfile = "3"
```

Run: `cargo test -p codegraph --test index_test`
Expected: FAIL (binary still bails with "not yet implemented").

- [ ] **Step 2: Create `crates/codegraph/src/index.rs`**

```rust
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DefKind {
    Fn,
    Struct,
    Enum,
    Trait,
    Class,
    Interface,
    Type,
    Const,
    Method,
}

#[derive(Debug, Clone, Serialize)]
pub struct Definition {
    pub file: String,
    pub name: String,
    pub kind: DefKind,
    pub line: usize,
    pub column: usize,
    /// The byte range of the *body* of the definition. Used by `callers`/`callees`
    /// to decide whether a reference site sits inside this definition.
    pub body_start_byte: usize,
    pub body_end_byte: usize,
    /// True if the definition is module-public (Rust `pub`, TS `export`, Python module-level).
    /// Cross-file resolution only matches against exported definitions.
    pub exported: bool,
}

/// One concrete "this name in this file refers to that other module's symbol".
/// Glob imports leave `imported_name = "*"` and the resolver treats them as wildcards.
#[derive(Debug, Clone, Serialize)]
pub struct Import {
    pub file: String,
    pub line: usize,
    /// The local binding the rest of the file uses.
    pub local_name: String,
    /// The name as it exists in the source module (often equal to local_name).
    pub imported_name: String,
    /// The module path string as written in the source (e.g. `crate::auth`, `./util`).
    /// The resolver normalizes this against `file`'s location.
    pub module_path: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RefKind {
    Call,
    Reference,
}

#[derive(Debug, Clone, Serialize)]
pub struct Reference {
    pub file: String,
    pub name: String,
    pub kind: RefKind,
    pub line: usize,
    pub column: usize,
    /// Byte offset into the source — used to test "is this ref inside fn X's body?".
    pub byte_offset: usize,
    /// Line text trimmed to <= 200 chars — included in the output as `context`.
    pub context: String,
}

#[derive(Debug, Default)]
pub struct Index {
    pub definitions: Vec<Definition>,
    pub imports: Vec<Import>,
    pub references: Vec<Reference>,
}

impl Index {
    /// Look up the definition that *contains* `byte_offset` in `file` — i.e. the function
    /// that the byte_offset's reference sits inside. Returns the innermost match.
    pub fn enclosing_definition(&self, file: &str, byte_offset: usize) -> Option<&Definition> {
        self.definitions
            .iter()
            .filter(|d| d.file == file && d.body_start_byte <= byte_offset && byte_offset < d.body_end_byte)
            .min_by_key(|d| d.body_end_byte - d.body_start_byte)
    }
}
```

- [ ] **Step 3: Wire it into `main.rs`**

Modify `crates/codegraph/src/main.rs` — add `mod index;` between `mod cli;` and `mod commands;`. The full file:

```rust
mod cli;
mod commands;
mod index;
mod lang;
mod output;
mod walk;

use clap::Parser;
use cli::{Cli, Command};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::FindRefs(a) => commands::find_refs::run(a),
        Command::Callers(a) => commands::callers::run(a),
        Command::Callees(a) => commands::callees::run(a),
        Command::Impact(a) => commands::impact::run(a),
    }
}
```

- [ ] **Step 4: Make `find-refs` return an empty result for an empty dir (minimal implementation)**

Replace `crates/codegraph/src/commands/find_refs.rs` with:

```rust
use crate::cli::FindRefsArgs;
use crate::output::print_json;
use serde::Serialize;

#[derive(Serialize, Default)]
struct Empty(Vec<()>);

pub fn run(args: FindRefsArgs) -> anyhow::Result<()> {
    if args.json {
        print_json(Empty::default())?;
    } else {
        // Non-JSON path will be filled in once we have real hits to print.
    }
    Ok(())
}
```

- [ ] **Step 5: Verify the test passes**

Run: `cargo test -p codegraph --test index_test`
Expected: PASS.

- [ ] **Step 6: Workspace gate + commit**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

git add crates/codegraph
git commit -m "feat(codegraph): Index data model and empty find-refs path"
```

---

### Task 5: Rust definitions query + indexer

**Files:**
- Create: `crates/codegraph/src/queries/rust_defs.scm`
- Modify: `crates/codegraph/src/lang.rs` (wire `QueryKind::Defs` for Rust)
- Modify: `crates/codegraph/src/index.rs` (add `build_index` function)
- Test: `crates/codegraph/tests/index_test.rs` (add a definition test)

- [ ] **Step 1: Write the failing test**

Append to `crates/codegraph/tests/index_test.rs`:

```rust
#[test]
fn rust_index_finds_lib_and_module_definitions() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // Compose a small project in-place rather than copying the fixture so this test
    // does not depend on file paths under tests/fixtures.
    let src = tmp.path().join("src");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("lib.rs"), "pub mod m;\npub fn alpha() {}\nstruct Beta;\n").unwrap();
    std::fs::write(src.join("m.rs"), "pub fn gamma() {}\n").unwrap();

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_codegraph"))
        .args(["find-refs", "alpha", "--json", "--path"])
        .arg(tmp.path())
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    // We do not assert refs yet — only that the definition is reported.
    let kinds: Vec<&str> = v["data"].as_array().unwrap().iter()
        .map(|e| e["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"definition"), "expected a definition entry, got: {:?}", kinds);
}
```

Run: `cargo test -p codegraph --test index_test rust_index_finds`
Expected: FAIL (find-refs still returns empty array).

- [ ] **Step 2: Create `crates/codegraph/src/queries/rust_defs.scm`**

```scheme
(function_item name: (identifier) @name) @def.fn
(struct_item   name: (type_identifier) @name) @def.struct
(enum_item     name: (type_identifier) @name) @def.enum
(trait_item    name: (type_identifier) @name) @def.trait
(type_item     name: (type_identifier) @name) @def.type
(const_item    name: (identifier) @name)      @def.const
(static_item   name: (identifier) @name)      @def.const

; Methods sit inside `impl` blocks. We capture them separately so they get DefKind::Method.
(impl_item
  body: (declaration_list
    (function_item name: (identifier) @name) @def.method))
```

- [ ] **Step 3: Wire `QueryKind::Defs` in `lang.rs`**

Replace the `query_source` body in `crates/codegraph/src/lang.rs`:

```rust
    pub fn query_source(self, kind: QueryKind) -> Option<&'static str> {
        match (self, kind) {
            (Language::Rust, QueryKind::Defs) => Some(include_str!("queries/rust_defs.scm")),
            _ => None,
        }
    }
```

- [ ] **Step 4: Add `build_index` to `crates/codegraph/src/index.rs`**

Append to the file:

```rust
use crate::lang::{Language, QueryKind};
use crate::walk::walk_sources;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use tree_sitter::{Parser, Query, QueryCursor};

impl DefKind {
    fn from_capture_suffix(s: &str) -> Option<Self> {
        match s.strip_prefix("def.")? {
            "fn" => Some(DefKind::Fn),
            "struct" => Some(DefKind::Struct),
            "enum" => Some(DefKind::Enum),
            "trait" => Some(DefKind::Trait),
            "class" => Some(DefKind::Class),
            "interface" => Some(DefKind::Interface),
            "type" => Some(DefKind::Type),
            "const" => Some(DefKind::Const),
            "method" => Some(DefKind::Method),
            _ => None,
        }
    }
}

pub fn build_index(root: &Path) -> Result<Index> {
    let mut idx = Index::default();
    for f in walk_sources(root)? {
        let source = match fs::read_to_string(&f.path) {
            Ok(s) => s,
            Err(_) => continue, // unreadable file — skip silently
        };
        let rel = f.path.strip_prefix(root).unwrap_or(&f.path).to_string_lossy().into_owned();
        if let Some(q) = f.language.query_source(QueryKind::Defs) {
            index_defs(&mut idx, &source, &rel, f.language, q)?;
        }
    }
    Ok(idx)
}

fn index_defs(
    idx: &mut Index,
    source: &str,
    rel: &str,
    lang: Language,
    query_src: &str,
) -> Result<()> {
    let mut parser = Parser::new();
    parser.set_language(&lang.ts_language()).with_context(|| format!("set language {}", lang.name()))?;
    let tree = parser.parse(source, None).with_context(|| format!("parse {rel}"))?;
    let query = Query::new(&lang.ts_language(), query_src).with_context(|| format!("compile defs query for {}", lang.name()))?;
    let names = query.capture_names();
    let bytes = source.as_bytes();
    let mut cursor = QueryCursor::new();
    for m in cursor.matches(&query, tree.root_node(), bytes) {
        let mut def_node = None;
        let mut def_kind = None;
        let mut name = None;
        for cap in m.captures {
            let cname = names[cap.index as usize];
            if cname == "name" {
                name = Some(cap.node.utf8_text(bytes).unwrap_or("").to_string());
            } else if let Some(k) = DefKind::from_capture_suffix(cname) {
                def_node = Some(cap.node);
                def_kind = Some(k);
            }
        }
        let (Some(n), Some(node), Some(kind)) = (name, def_node, def_kind) else { continue };
        let exported = is_exported(node, bytes, lang);
        idx.definitions.push(Definition {
            file: rel.to_string(),
            name: n,
            kind,
            line: node.start_position().row + 1,
            column: node.start_position().column + 1,
            body_start_byte: node.start_byte(),
            body_end_byte: node.end_byte(),
            exported,
        });
    }
    Ok(())
}

fn is_exported(node: tree_sitter::Node<'_>, bytes: &[u8], lang: Language) -> bool {
    match lang {
        Language::Rust => {
            // Look for a `visibility_modifier` child whose text starts with `pub`.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "visibility_modifier" {
                    let text = child.utf8_text(bytes).unwrap_or("");
                    if text.starts_with("pub") { return true; }
                }
            }
            false
        }
        // TS/JS/Python wired in later tasks; default to exported=true for now so
        // cross-file resolution does not silently miss things in those languages.
        _ => true,
    }
}
```

- [ ] **Step 5: Make `find-refs` return definitions matching the name**

Replace `crates/codegraph/src/commands/find_refs.rs`:

```rust
use crate::cli::FindRefsArgs;
use crate::index::{build_index, DefKind};
use crate::output::print_json;
use serde::Serialize;

#[derive(Serialize)]
struct Hit {
    file: String,
    line: usize,
    column: usize,
    kind: &'static str,
    name: String,
    context: String,
    confidence: &'static str,
    reason: &'static str,
}

pub fn run(args: FindRefsArgs) -> anyhow::Result<()> {
    let idx = build_index(&args.path)?;
    let mut hits = Vec::new();
    for d in &idx.definitions {
        if d.name == args.name {
            hits.push(Hit {
                file: d.file.clone(),
                line: d.line,
                column: d.column,
                kind: "definition",
                name: d.name.clone(),
                context: format!("{:?} {}", d.kind, d.name).to_lowercase(),
                confidence: "high",
                reason: "same-file-scope",
            });
        }
    }
    if args.json {
        print_json(&hits)?;
    } else {
        for h in &hits {
            println!("{}:{}:{}  {}  {}  ({} {})", h.file, h.line, h.column, h.kind, h.name, h.confidence, h.reason);
        }
    }
    Ok(())
}

// Silence "unused" until we start reading non-`fn`/`struct` defs.
#[allow(dead_code)]
fn _kind_label(_: DefKind) {}
```

- [ ] **Step 6: Run the test**

```bash
cargo test -p codegraph --test index_test rust_index_finds
```

Expected: PASS — find-refs returns a `definition` hit for `alpha`.

- [ ] **Step 7: Workspace gate + commit**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

git add crates/codegraph
git commit -m "feat(codegraph): index rust definitions via tree-sitter"
```

---

### Task 6: Rust imports query + indexer

**Files:**
- Create: `crates/codegraph/src/queries/rust_imports.scm`
- Modify: `crates/codegraph/src/lang.rs` (wire `QueryKind::Imports` for Rust)
- Modify: `crates/codegraph/src/index.rs` (`index_imports`)
- Test: `crates/codegraph/tests/index_test.rs` (verify imports populated)

Rust `use` is the trickiest of the three because of grouped imports (`use foo::{a, b}`), aliases (`use foo as bar`), and globs (`use foo::*`). The query captures the path tokens; the indexer flattens them into `Import` records.

- [ ] **Step 1: Write the failing test**

Append to `tests/index_test.rs`:

```rust
#[test]
fn rust_imports_are_flattened() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src = tmp.path().join("src");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("lib.rs"), "pub mod m;\npub fn alpha() {}\n").unwrap();
    std::fs::write(
        src.join("m.rs"),
        "use crate::alpha;\nuse crate::{alpha as a2, beta};\nuse crate::*;\npub fn use_alpha() { alpha(); }\n",
    )
    .unwrap();
    // The probe we use here is `find-refs alpha` — we have not built the resolver yet,
    // so this test only asserts the definition still appears (= the imports query did not crash).
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_codegraph"))
        .args(["find-refs", "alpha", "--json", "--path"])
        .arg(tmp.path())
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert!(v["data"].as_array().unwrap().iter().any(|h| h["kind"] == "definition"));
}
```

Run: `cargo test -p codegraph --test index_test rust_imports_are_flattened`
Expected: PASS already (we have not wired imports yet, but the definition path still works). The point of this test is to lock in the contract for the next step — it stays green throughout.

- [ ] **Step 2: Create `crates/codegraph/src/queries/rust_imports.scm`**

```scheme
; Single path: use crate::auth;     use std::collections::HashMap;
(use_declaration
  argument: (scoped_identifier
    path: (_) @path
    name: (identifier) @name)) @import

; Aliased single path: use crate::auth as a;
(use_declaration
  argument: (use_as_clause
    path: (_) @path
    alias: (identifier) @alias)) @import

; Grouped: use crate::{a, b as bb};
(use_declaration
  argument: (scoped_use_list
    path: (_) @path
    list: (use_list) @group)) @import

; Glob: use crate::*;
(use_declaration
  argument: (scoped_use_list
    path: (_) @path
    list: (use_wildcard))) @import
```

(Note: `tree-sitter-rust` 0.21 uses node kinds `scoped_use_list`, `use_as_clause`, `use_wildcard`. If a captured run fails at runtime, fall back to capturing the entire `use_declaration` and parsing its text — see Step 4 fallback.)

- [ ] **Step 3: Wire `QueryKind::Imports` for Rust in `lang.rs`**

Update the match arm in `query_source`:

```rust
    pub fn query_source(self, kind: QueryKind) -> Option<&'static str> {
        match (self, kind) {
            (Language::Rust, QueryKind::Defs) => Some(include_str!("queries/rust_defs.scm")),
            (Language::Rust, QueryKind::Imports) => Some(include_str!("queries/rust_imports.scm")),
            _ => None,
        }
    }
```

- [ ] **Step 4: Extend `build_index` in `index.rs` to populate imports**

Add after the `index_defs` call inside `build_index`:

```rust
        if let Some(q) = f.language.query_source(QueryKind::Imports) {
            // Best-effort: if the query fails to compile against the live grammar
            // (node kinds drift across tree-sitter releases), skip imports for this
            // file rather than abort the whole index.
            if let Err(e) = index_imports(&mut idx, &source, &rel, f.language, q) {
                eprintln!("codegraph: imports query skipped for {rel}: {e}");
            }
        }
```

And add the function at the bottom of the file:

```rust
fn index_imports(
    idx: &mut Index,
    source: &str,
    rel: &str,
    lang: Language,
    query_src: &str,
) -> Result<()> {
    let mut parser = Parser::new();
    parser.set_language(&lang.ts_language())?;
    let tree = parser.parse(source, None).context("parse")?;
    let query = Query::new(&lang.ts_language(), query_src).context("compile imports query")?;
    let names = query.capture_names();
    let bytes = source.as_bytes();
    let mut cursor = QueryCursor::new();
    for m in cursor.matches(&query, tree.root_node(), bytes) {
        let mut path_text: Option<String> = None;
        let mut single_name: Option<String> = None;
        let mut alias: Option<String> = None;
        let mut group_node: Option<tree_sitter::Node<'_>> = None;
        let mut import_node: Option<tree_sitter::Node<'_>> = None;
        for cap in m.captures {
            let cname = names[cap.index as usize];
            let text = cap.node.utf8_text(bytes).unwrap_or("").to_string();
            match cname {
                "path"   => path_text = Some(text),
                "name"   => single_name = Some(text),
                "alias"  => alias = Some(text),
                "group"  => group_node = Some(cap.node),
                "import" => import_node = Some(cap.node),
                _ => {}
            }
        }
        let line = import_node.map(|n| n.start_position().row + 1).unwrap_or(0);
        let module_path = path_text.unwrap_or_default();

        match (single_name, alias, group_node) {
            (Some(n), _, _) => {
                idx.imports.push(Import {
                    file: rel.to_string(),
                    line,
                    local_name: n.clone(),
                    imported_name: n,
                    module_path: module_path.clone(),
                });
            }
            (_, Some(a), _) => {
                // `use foo::bar as a;` — alias is the local name, the last segment of path is imported_name.
                let imported = module_path.rsplit("::").next().unwrap_or(&module_path).to_string();
                idx.imports.push(Import {
                    file: rel.to_string(),
                    line,
                    local_name: a,
                    imported_name: imported,
                    module_path,
                });
            }
            (_, _, Some(group)) => {
                // Walk the group node's `(identifier)` and `(use_as_clause)` children.
                let mut cur = group.walk();
                for child in group.children(&mut cur) {
                    match child.kind() {
                        "identifier" => {
                            let nm = child.utf8_text(bytes).unwrap_or("").to_string();
                            idx.imports.push(Import {
                                file: rel.to_string(),
                                line,
                                local_name: nm.clone(),
                                imported_name: nm,
                                module_path: module_path.clone(),
                            });
                        }
                        "use_as_clause" => {
                            // First identifier child = imported, alias child = local.
                            let mut sub = child.walk();
                            let mut kids = child.children(&mut sub).filter(|n| n.kind() == "identifier");
                            let imp = kids.next().map(|n| n.utf8_text(bytes).unwrap_or("").to_string()).unwrap_or_default();
                            let als = kids.next().map(|n| n.utf8_text(bytes).unwrap_or("").to_string()).unwrap_or_else(|| imp.clone());
                            idx.imports.push(Import {
                                file: rel.to_string(),
                                line,
                                local_name: als,
                                imported_name: imp,
                                module_path: module_path.clone(),
                            });
                        }
                        _ => {}
                    }
                }
            }
            _ => {
                // Glob `use foo::*;` — record one wildcard entry.
                idx.imports.push(Import {
                    file: rel.to_string(),
                    line,
                    local_name: "*".to_string(),
                    imported_name: "*".to_string(),
                    module_path,
                });
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Verify the test still passes (no regression)**

```bash
cargo test -p codegraph --test index_test
```

Expected: all tests PASS. If the live `tree-sitter-rust` grammar uses different node names, the `eprintln!` from Step 4 will fire — go fix the query in Step 2.

- [ ] **Step 6: Workspace gate + commit**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

git add crates/codegraph
git commit -m "feat(codegraph): index rust use declarations"
```

---

### Task 7: Rust references query + indexer

**Files:**
- Create: `crates/codegraph/src/queries/rust_refs.scm`
- Modify: `crates/codegraph/src/lang.rs`
- Modify: `crates/codegraph/src/index.rs`
- Test: `crates/codegraph/tests/index_test.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn rust_call_expressions_become_references() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src = tmp.path().join("src");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("lib.rs"), "pub fn alpha() {}\npub fn beta() { alpha(); }\n").unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_codegraph"))
        .args(["find-refs", "alpha", "--json", "--path"])
        .arg(tmp.path())
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let kinds: Vec<&str> = v["data"].as_array().unwrap().iter()
        .map(|e| e["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"definition"), "kinds: {:?}", kinds);
    assert!(kinds.contains(&"call"), "kinds: {:?}", kinds);
}
```

Run: `cargo test -p codegraph --test index_test rust_call_expressions`
Expected: FAIL — call hits not produced yet.

- [ ] **Step 2: Create `crates/codegraph/src/queries/rust_refs.scm`**

```scheme
; Simple call: foo(...)
(call_expression
  function: (identifier) @name) @ref.call

; Path call: foo::bar(...)
(call_expression
  function: (scoped_identifier
    name: (identifier) @name)) @ref.call

; Method call: x.foo(...)
(call_expression
  function: (field_expression
    field: (field_identifier) @name)) @ref.call

; Macro invocation: println!(...)
(macro_invocation
  macro: (identifier) @name) @ref.call

; Type reference (handles fn signatures, struct fields, generics): Foo
(type_identifier) @name @ref.reference

; Path expression (e.g. `Foo::CONST` reads `Foo`): used as a non-call identifier reference.
(scoped_identifier
  name: (identifier) @name) @ref.reference
```

The two `@ref.reference` captures will overlap with the `@ref.call` captures on call sites. The indexer dedupes by `(file, byte_offset, name)`.

- [ ] **Step 3: Wire `QueryKind::Refs` for Rust**

```rust
    pub fn query_source(self, kind: QueryKind) -> Option<&'static str> {
        match (self, kind) {
            (Language::Rust, QueryKind::Defs) => Some(include_str!("queries/rust_defs.scm")),
            (Language::Rust, QueryKind::Imports) => Some(include_str!("queries/rust_imports.scm")),
            (Language::Rust, QueryKind::Refs) => Some(include_str!("queries/rust_refs.scm")),
            _ => None,
        }
    }
```

- [ ] **Step 4: Add `index_refs` to `index.rs` and call it from `build_index`**

In `build_index`, add after the imports call:

```rust
        if let Some(q) = f.language.query_source(QueryKind::Refs) {
            if let Err(e) = index_refs(&mut idx, &source, &rel, f.language, q) {
                eprintln!("codegraph: refs query skipped for {rel}: {e}");
            }
        }
```

And add the function:

```rust
impl RefKind {
    fn from_capture_suffix(s: &str) -> Option<Self> {
        match s.strip_prefix("ref.")? {
            "call" => Some(RefKind::Call),
            "reference" => Some(RefKind::Reference),
            _ => None,
        }
    }
}

fn index_refs(
    idx: &mut Index,
    source: &str,
    rel: &str,
    lang: Language,
    query_src: &str,
) -> Result<()> {
    let mut parser = Parser::new();
    parser.set_language(&lang.ts_language())?;
    let tree = parser.parse(source, None).context("parse")?;
    let query = Query::new(&lang.ts_language(), query_src).context("compile refs query")?;
    let names = query.capture_names();
    let bytes = source.as_bytes();
    let mut cursor = QueryCursor::new();

    // Dedupe key: (byte_offset, name). Calls and Reference captures often overlap at the same byte.
    let mut seen: std::collections::HashSet<(usize, String)> = std::collections::HashSet::new();

    for m in cursor.matches(&query, tree.root_node(), bytes) {
        let mut name_node: Option<tree_sitter::Node<'_>> = None;
        let mut ref_kind: Option<RefKind> = None;
        for cap in m.captures {
            let cname = names[cap.index as usize];
            if cname == "name" {
                name_node = Some(cap.node);
            } else if let Some(k) = RefKind::from_capture_suffix(cname) {
                ref_kind = Some(k);
            }
        }
        let (Some(node), Some(kind)) = (name_node, ref_kind) else { continue };
        let name = node.utf8_text(bytes).unwrap_or("").to_string();
        let byte_offset = node.start_byte();
        if !seen.insert((byte_offset, name.clone())) {
            // Same site already recorded — keep the first (Call wins over Reference because the query lists calls first).
            continue;
        }
        let line = node.start_position().row + 1;
        let column = node.start_position().column + 1;
        let context = line_at(source, line);
        idx.references.push(Reference { file: rel.to_string(), name, kind, line, column, byte_offset, context });
    }
    Ok(())
}

fn line_at(source: &str, line: usize) -> String {
    let raw = source.lines().nth(line.saturating_sub(1)).unwrap_or("");
    let trimmed = raw.trim();
    if trimmed.chars().count() > 200 {
        trimmed.chars().take(200).collect::<String>() + "…"
    } else {
        trimmed.to_string()
    }
}
```

- [ ] **Step 5: Emit references from `find-refs` (still name-only, resolver comes next task)**

Update `crates/codegraph/src/commands/find_refs.rs` to also emit reference hits, but tag every non-definition hit as `low` / `name-only` for now:

```rust
use crate::cli::FindRefsArgs;
use crate::index::{build_index, RefKind};
use crate::output::print_json;
use serde::Serialize;

#[derive(Serialize)]
struct Hit {
    file: String,
    line: usize,
    column: usize,
    kind: &'static str,
    name: String,
    context: String,
    confidence: &'static str,
    reason: &'static str,
}

pub fn run(args: FindRefsArgs) -> anyhow::Result<()> {
    let idx = build_index(&args.path)?;
    let mut hits = Vec::new();
    for d in &idx.definitions {
        if d.name == args.name {
            hits.push(Hit {
                file: d.file.clone(),
                line: d.line,
                column: d.column,
                kind: "definition",
                name: d.name.clone(),
                context: format!("{:?} {}", d.kind, d.name).to_lowercase(),
                confidence: "high",
                reason: "same-file-scope",
            });
        }
    }
    for r in &idx.references {
        if r.name == args.name {
            let kind = match r.kind { RefKind::Call => "call", RefKind::Reference => "reference" };
            hits.push(Hit {
                file: r.file.clone(),
                line: r.line,
                column: r.column,
                kind,
                name: r.name.clone(),
                context: r.context.clone(),
                confidence: "low",
                reason: "name-only",
            });
        }
    }
    hits.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    if args.json {
        print_json(&hits)?;
    } else {
        for h in &hits {
            println!("{}:{}:{}  {:<10} {:<6} {}", h.file, h.line, h.column, h.kind, h.confidence, h.context);
        }
    }
    Ok(())
}
```

- [ ] **Step 6: Run the test**

```bash
cargo test -p codegraph --test index_test rust_call_expressions
```

Expected: PASS — `kinds` now contains both `definition` and `call`.

- [ ] **Step 7: Workspace gate + commit**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

git add crates/codegraph
git commit -m "feat(codegraph): index rust references and emit them from find-refs"
```

---

### Task 8: Resolver — confidence tagging

**Files:**
- Create: `crates/codegraph/src/resolve.rs`
- Modify: `crates/codegraph/src/main.rs` (add `mod resolve;`)
- Modify: `crates/codegraph/src/commands/find_refs.rs` (consume resolver output)
- Test: `crates/codegraph/tests/find_refs_test.rs` (new dedicated test file using the rust_app fixture)

The resolver runs after `build_index` and walks `Index.references`, producing `ResolvedRef { reference, definition, confidence, reason }`. Rules:

1. If the reference and a definition with the same name are in the same file → `high` / `same-file-scope`.
2. Else, look at the file's imports. If an import's `local_name` matches the reference's name and the import points (heuristically) at a file that contains a matching definition → `high` / `import-resolved`.
3. Else, if a glob import `*` from a module that contains a matching definition exists → `medium` / `import-resolved`.
4. Else, if exactly one matching exported definition exists anywhere → `medium` / `name-only` (rare case worth surfacing).
5. Else → `low` / `name-only`.

Heuristic module-path resolution (Rust only this task):
- `crate::auth::foo` in `src/handlers.rs` matches a definition in `src/auth.rs` if the def's name == final path segment.
- `crate::foo` matches a definition in `src/lib.rs` or `src/main.rs`.
- `super::foo`, `self::foo`, and absolute crate paths from other workspace members are out of scope — they fall through to rule 4 or 5.

- [ ] **Step 1: Write the failing test**

Create `crates/codegraph/tests/find_refs_test.rs`:

```rust
use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_codegraph")) }
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/multi_lang/rust_app")
}

fn run_find_refs(name: &str) -> serde_json::Value {
    let out = Command::new(bin())
        .args(["find-refs", name, "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).expect("json")
}

#[test]
fn cross_file_call_to_imported_fn_is_high_confidence() {
    let v = run_find_refs("authenticate");
    let hits = v["data"].as_array().unwrap();
    let cross_file_call = hits.iter().find(|h|
        h["file"].as_str().unwrap().ends_with("handlers.rs")
        && h["kind"].as_str().unwrap() == "call"
    ).expect("handlers.rs should contain a call to authenticate");
    assert_eq!(cross_file_call["confidence"].as_str().unwrap(), "high");
    assert_eq!(cross_file_call["reason"].as_str().unwrap(), "import-resolved");
}

#[test]
fn same_file_call_is_high_confidence() {
    let v = run_find_refs("authenticate");
    let hits = v["data"].as_array().unwrap();
    let same_file_call = hits.iter().find(|h|
        h["file"].as_str().unwrap().ends_with("auth.rs")
        && h["kind"].as_str().unwrap() == "call"
    ).expect("auth.rs should call authenticate from revoke");
    assert_eq!(same_file_call["confidence"].as_str().unwrap(), "high");
    assert_eq!(same_file_call["reason"].as_str().unwrap(), "same-file-scope");
}

#[test]
fn unused_helper_has_definition_but_no_calls() {
    let v = run_find_refs("unused_helper");
    let hits = v["data"].as_array().unwrap();
    assert!(hits.iter().any(|h| h["kind"] == "definition"));
    assert!(!hits.iter().any(|h| h["kind"] == "call"),
        "unused_helper should not have any call sites, got: {hits:?}");
}
```

Run: `cargo test -p codegraph --test find_refs_test`
Expected: FAIL — every cross-file call is currently `low` / `name-only`.

- [ ] **Step 2: Create `crates/codegraph/src/resolve.rs`**

```rust
use crate::index::{Definition, Import, Index, Reference};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Confidence::High => "high",
            Confidence::Medium => "medium",
            Confidence::Low => "low",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveReason {
    SameFileScope,
    ImportResolved,
    NameOnly,
}

impl ResolveReason {
    pub fn as_str(self) -> &'static str {
        match self {
            ResolveReason::SameFileScope => "same-file-scope",
            ResolveReason::ImportResolved => "import-resolved",
            ResolveReason::NameOnly => "name-only",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Resolved<'a> {
    pub reference: &'a Reference,
    pub definition: Option<&'a Definition>,
    pub confidence: Confidence,
    pub reason: ResolveReason,
}

/// Resolve every reference whose `name == target` against the index.
/// Definitions are matched on `name` only (no signature equality, no generics).
pub fn resolve_refs<'a>(idx: &'a Index, target: &str) -> Vec<Resolved<'a>> {
    let defs_by_name: Vec<&Definition> = idx.definitions.iter().filter(|d| d.name == target).collect();
    let mut out = Vec::new();
    for r in &idx.references {
        if r.name != target { continue; }

        // Rule 1: same-file definition?
        if let Some(d) = defs_by_name.iter().find(|d| d.file == r.file) {
            out.push(Resolved { reference: r, definition: Some(d), confidence: Confidence::High, reason: ResolveReason::SameFileScope });
            continue;
        }
        // Rule 2: import in r.file pointing to a defining file?
        let imports_in_file: Vec<&Import> = idx.imports.iter().filter(|i| i.file == r.file).collect();
        let named_import = imports_in_file.iter().find(|i| i.local_name == target && i.imported_name != "*");
        if let Some(imp) = named_import {
            if let Some(d) = defs_by_name.iter().find(|d| module_matches(d, imp)) {
                out.push(Resolved { reference: r, definition: Some(d), confidence: Confidence::High, reason: ResolveReason::ImportResolved });
                continue;
            }
        }
        // Rule 3: glob import from a defining file?
        let glob_resolution = imports_in_file.iter().filter(|i| i.imported_name == "*").find_map(|imp| {
            defs_by_name.iter().find(|d| module_matches(d, imp)).copied()
        });
        if let Some(d) = glob_resolution {
            out.push(Resolved { reference: r, definition: Some(d), confidence: Confidence::Medium, reason: ResolveReason::ImportResolved });
            continue;
        }
        // Rule 4/5: name-only.
        let conf = if defs_by_name.len() == 1 { Confidence::Medium } else { Confidence::Low };
        out.push(Resolved {
            reference: r,
            definition: defs_by_name.first().copied(),
            confidence: conf,
            reason: ResolveReason::NameOnly,
        });
    }
    out
}

/// Heuristic: does `imp.module_path` (e.g. "crate::auth") plausibly point at the file
/// containing `def` (e.g. "src/auth.rs")?
fn module_matches(def: &Definition, imp: &Import) -> bool {
    if imp.module_path.is_empty() { return false; }
    // Strip the crate root prefix.
    let path = imp.module_path
        .trim_start_matches("crate::")
        .trim_start_matches("self::")
        .replace("::", "/");
    // Candidate file forms for `crate::auth`:
    //   src/auth.rs
    //   src/auth/mod.rs
    //   auth.rs (for crates without src/ layout)
    //   auth/mod.rs
    let def_file = def.file.replace('\\', "/");
    if path.is_empty() {
        // `use crate::*` — module_path was just "crate", we treat it as "lib root".
        return def_file == "src/lib.rs" || def_file == "src/main.rs" || def_file == "lib.rs";
    }
    let stem_variants = [
        format!("src/{path}.rs"),
        format!("src/{path}/mod.rs"),
        format!("{path}.rs"),
        format!("{path}/mod.rs"),
    ];
    stem_variants.iter().any(|s| s == &def_file)
}
```

- [ ] **Step 3: Wire `mod resolve;` into `main.rs`**

Add `mod resolve;` to the module list (alphabetical placement: after `mod output;`, before `mod walk;`).

- [ ] **Step 4: Update `commands/find_refs.rs` to use the resolver**

Replace the function body:

```rust
use crate::cli::FindRefsArgs;
use crate::index::{build_index, RefKind};
use crate::output::print_json;
use crate::resolve::{resolve_refs};
use serde::Serialize;

#[derive(Serialize)]
struct Hit {
    file: String,
    line: usize,
    column: usize,
    kind: &'static str,
    name: String,
    context: String,
    confidence: &'static str,
    reason: &'static str,
}

pub fn run(args: FindRefsArgs) -> anyhow::Result<()> {
    let idx = build_index(&args.path)?;
    let mut hits = Vec::new();
    for d in &idx.definitions {
        if d.name == args.name {
            hits.push(Hit {
                file: d.file.clone(),
                line: d.line,
                column: d.column,
                kind: "definition",
                name: d.name.clone(),
                context: format!("{:?} {}", d.kind, d.name).to_lowercase(),
                confidence: "high",
                reason: "same-file-scope",
            });
        }
    }
    for r in resolve_refs(&idx, &args.name) {
        let kind = match r.reference.kind { RefKind::Call => "call", RefKind::Reference => "reference" };
        hits.push(Hit {
            file: r.reference.file.clone(),
            line: r.reference.line,
            column: r.reference.column,
            kind,
            name: r.reference.name.clone(),
            context: r.reference.context.clone(),
            confidence: r.confidence.as_str(),
            reason: r.reason.as_str(),
        });
    }
    hits.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    if args.json {
        print_json(&hits)?;
    } else {
        for h in &hits {
            println!("{}:{}:{}  {:<10} {:<6} {}", h.file, h.line, h.column, h.kind, h.confidence, h.context);
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Run the tests**

```bash
cargo test -p codegraph --test find_refs_test
```

Expected: all three tests PASS. If `cross_file_call_to_imported_fn_is_high_confidence` fails, inspect `module_matches` against the actual `module_path` stored — print the index for the fixture by running `cargo run -p codegraph -- find-refs authenticate --json --path crates/codegraph/tests/fixtures/multi_lang/rust_app | jq` and check the `module_path` you got.

- [ ] **Step 6: Workspace gate + commit**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

git add crates/codegraph
git commit -m "feat(codegraph): resolver tags references with confidence + reason"
```

---

### Task 9: `callers` subcommand (Rust, depth=1)

**Files:**
- Modify: `crates/codegraph/src/commands/callers.rs`
- Test: `crates/codegraph/tests/callers_test.rs`

`callers <fn>` returns every function (Definition with `kind=Fn` or `kind=Method`) that contains at least one resolved Call reference to `<fn>`. Confidence is the *minimum* of the resolution confidence and the enclosing-fn's existence (always high — the enclosing fn is in `Index.definitions`).

- [ ] **Step 1: Write the failing test**

Create `crates/codegraph/tests/callers_test.rs`:

```rust
use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_codegraph")) }
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/multi_lang/rust_app")
}

#[test]
fn callers_of_authenticate_includes_revoke_and_login() {
    let out = Command::new(bin())
        .args(["callers", "authenticate", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let callers: Vec<&str> = v["data"].as_array().unwrap().iter()
        .map(|e| e["name"].as_str().unwrap()).collect();
    assert!(callers.contains(&"revoke"), "callers: {callers:?}");
    assert!(callers.contains(&"login"), "callers: {callers:?}");
    // `whoami` does not call authenticate.
    assert!(!callers.contains(&"whoami"), "callers: {callers:?}");
}

#[test]
fn callers_of_unused_helper_is_empty() {
    let out = Command::new(bin())
        .args(["callers", "unused_helper", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["data"].as_array().unwrap().len(), 0);
}
```

Run: `cargo test -p codegraph --test callers_test`
Expected: FAIL (callers still bails).

- [ ] **Step 2: Implement `commands/callers.rs`**

```rust
use crate::cli::CallersArgs;
use crate::index::{build_index, DefKind, RefKind};
use crate::output::print_json;
use crate::resolve::resolve_refs;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize, Clone)]
struct CallerEntry {
    file: String,
    line: usize,
    column: usize,
    name: String,
    kind: &'static str, // "fn" or "method"
    confidence: &'static str,
    reason: &'static str,
    /// Call sites inside this caller that point at the target.
    sites: Vec<CallSite>,
}

#[derive(Serialize, Clone)]
struct CallSite {
    file: String,
    line: usize,
    column: usize,
    context: String,
}

pub fn run(args: CallersArgs) -> anyhow::Result<()> {
    if args.depth != 1 {
        // Depth-N is implemented in Task 13 — surface the limitation here rather than silently ignore.
        anyhow::bail!("callers: --depth > 1 is not implemented yet (tracked in Task 13)");
    }
    let idx = build_index(&args.path)?;

    let mut by_caller: BTreeMap<(String, usize, usize), CallerEntry> = BTreeMap::new();

    for resolved in resolve_refs(&idx, &args.name) {
        if resolved.reference.kind != RefKind::Call { continue; }
        let Some(enclosing) = idx.enclosing_definition(&resolved.reference.file, resolved.reference.byte_offset) else {
            // Reference at module scope (e.g. `const X: () = foo();`). Skip — not a "caller fn".
            continue;
        };
        if !matches!(enclosing.kind, DefKind::Fn | DefKind::Method) { continue; }
        let key = (enclosing.file.clone(), enclosing.line, enclosing.column);
        let entry = by_caller.entry(key).or_insert_with(|| CallerEntry {
            file: enclosing.file.clone(),
            line: enclosing.line,
            column: enclosing.column,
            name: enclosing.name.clone(),
            kind: if enclosing.kind == DefKind::Method { "method" } else { "fn" },
            confidence: resolved.confidence.as_str(),
            reason: resolved.reason.as_str(),
            sites: Vec::new(),
        });
        // Downgrade: if any site is low, keep the worst.
        if confidence_rank(entry.confidence) > confidence_rank(resolved.confidence.as_str()) {
            entry.confidence = resolved.confidence.as_str();
            entry.reason = resolved.reason.as_str();
        }
        entry.sites.push(CallSite {
            file: resolved.reference.file.clone(),
            line: resolved.reference.line,
            column: resolved.reference.column,
            context: resolved.reference.context.clone(),
        });
    }

    let mut entries: Vec<CallerEntry> = by_caller.into_values().collect();
    entries.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

    if args.json {
        print_json(&entries)?;
    } else {
        for e in &entries {
            println!("{}:{}:{}  {}  {}  ({} call site(s), {} {})",
                e.file, e.line, e.column, e.kind, e.name, e.sites.len(), e.confidence, e.reason);
            for s in &e.sites {
                println!("    {}:{}  {}", s.file, s.line, s.context);
            }
        }
    }
    Ok(())
}

fn confidence_rank(c: &str) -> u8 {
    match c { "high" => 3, "medium" => 2, _ => 1 }
}
```

- [ ] **Step 3: Run the tests**

```bash
cargo test -p codegraph --test callers_test
```

Expected: PASS.

- [ ] **Step 4: Workspace gate + commit**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

git add crates/codegraph
git commit -m "feat(codegraph): callers subcommand (depth=1) for rust"
```

---

### Task 10: `callees` subcommand (Rust, depth=1)

**Files:**
- Modify: `crates/codegraph/src/commands/callees.rs`
- Test: `crates/codegraph/tests/callees_test.rs`

`callees <fn>` finds the definition of `<fn>` (must be `Fn` or `Method`), then returns every distinct call-kind reference whose `byte_offset` sits inside that definition's body.

- [ ] **Step 1: Write the failing test**

Create `crates/codegraph/tests/callees_test.rs`:

```rust
use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_codegraph")) }
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/multi_lang/rust_app")
}

#[test]
fn callees_of_login_includes_new_user_and_authenticate() {
    let out = Command::new(bin())
        .args(["callees", "login", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let names: Vec<&str> = v["data"].as_array().unwrap().iter()
        .map(|e| e["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"new_user"), "names: {names:?}");
    assert!(names.contains(&"authenticate"), "names: {names:?}");
}

#[test]
fn callees_of_missing_fn_errors_cleanly() {
    let out = Command::new(bin())
        .args(["callees", "definitely_not_a_function", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    // We treat "no such fn" as success-with-empty rather than non-zero exit,
    // because pipelines that ask "what does X call?" want a clean empty list.
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["data"].as_array().unwrap().len(), 0);
}
```

Run: `cargo test -p codegraph --test callees_test`
Expected: FAIL.

- [ ] **Step 2: Implement `commands/callees.rs`**

```rust
use crate::cli::CalleesArgs;
use crate::index::{build_index, DefKind, RefKind};
use crate::output::print_json;
use crate::resolve::resolve_refs;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
struct CalleeEntry {
    name: String,
    kind: &'static str,
    /// Where the callee is defined (empty when we couldn't resolve a definition).
    def_file: Option<String>,
    def_line: Option<usize>,
    confidence: &'static str,
    reason: &'static str,
    sites: Vec<CallSite>,
}

#[derive(Serialize)]
struct CallSite {
    file: String,
    line: usize,
    column: usize,
    context: String,
}

pub fn run(args: CalleesArgs) -> anyhow::Result<()> {
    if args.depth != 1 {
        anyhow::bail!("callees: --depth > 1 is not implemented yet (tracked in Task 13)");
    }
    let idx = build_index(&args.path)?;

    let outer: Vec<_> = idx.definitions.iter()
        .filter(|d| d.name == args.name && matches!(d.kind, DefKind::Fn | DefKind::Method))
        .collect();
    if outer.is_empty() {
        return emit(&[], args.json);
    }

    // Collect calls that live inside any matching outer's body.
    let mut by_callee: BTreeMap<String, CalleeEntry> = BTreeMap::new();
    for r in &idx.references {
        if r.kind != RefKind::Call { continue; }
        if !outer.iter().any(|o|
            o.file == r.file && o.body_start_byte <= r.byte_offset && r.byte_offset < o.body_end_byte
        ) { continue; }
        // Resolve THIS reference (cheap: re-use resolve_refs with that specific name).
        let resolutions = resolve_refs(&idx, &r.name);
        let chosen = resolutions.iter().find(|res| res.reference.byte_offset == r.byte_offset && res.reference.file == r.file);
        let (confidence, reason, def_file, def_line) = match chosen {
            Some(res) => (
                res.confidence.as_str(),
                res.reason.as_str(),
                res.definition.map(|d| d.file.clone()),
                res.definition.map(|d| d.line),
            ),
            None => ("low", "name-only", None, None),
        };
        let entry = by_callee.entry(r.name.clone()).or_insert_with(|| CalleeEntry {
            name: r.name.clone(),
            kind: "fn",
            def_file: def_file.clone(),
            def_line,
            confidence,
            reason,
            sites: Vec::new(),
        });
        entry.sites.push(CallSite {
            file: r.file.clone(),
            line: r.line,
            column: r.column,
            context: r.context.clone(),
        });
    }
    let entries: Vec<CalleeEntry> = by_callee.into_values().collect();
    emit(&entries, args.json)
}

fn emit(entries: &[CalleeEntry], json: bool) -> anyhow::Result<()> {
    if json {
        print_json(entries)?;
    } else {
        for e in entries {
            let target = match (&e.def_file, e.def_line) {
                (Some(f), Some(l)) => format!("{f}:{l}"),
                _ => "<unresolved>".to_string(),
            };
            println!("{}  -> {}  ({} {}; {} site(s))", e.name, target, e.confidence, e.reason, e.sites.len());
        }
    }
    Ok(())
}
```

- [ ] **Step 3: Run the tests**

```bash
cargo test -p codegraph --test callees_test
```

Expected: PASS.

- [ ] **Step 4: Workspace gate + commit**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

git add crates/codegraph
git commit -m "feat(codegraph): callees subcommand for rust"
```

---

### Task 11: `impact` subcommand (Rust)

**Files:**
- Modify: `crates/codegraph/src/commands/impact.rs`
- Test: `crates/codegraph/tests/impact_test.rs`

`impact <symbol>` returns:
1. Every transitive caller of `<symbol>` (BFS over `callers` relation up to a safety cap).
2. Every Reference (not just Call) to `<symbol>` — flags type-position usages too.

Output is a JSON array of `{ name, kind, file, line, distance, confidence, reason }`. `distance=0` is the symbol itself, `distance=1` is direct callers/users, etc.

Safety cap: depth 6 + visited-set so cycles don't loop forever.

- [ ] **Step 1: Write the failing test**

Create `crates/codegraph/tests/impact_test.rs`:

```rust
use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_codegraph")) }
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/multi_lang/rust_app")
}

#[test]
fn impact_of_authenticate_includes_login_and_revoke() {
    let out = Command::new(bin())
        .args(["impact", "authenticate", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let names: Vec<&str> = v["data"].as_array().unwrap().iter()
        .map(|e| e["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"authenticate"));
    assert!(names.contains(&"login"));
    assert!(names.contains(&"revoke"));
}

#[test]
fn impact_of_user_struct_includes_type_position_uses() {
    let out = Command::new(bin())
        .args(["impact", "User", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    // Every fn that takes `&User` should appear (authenticate, revoke, whoami).
    let names: Vec<&str> = v["data"].as_array().unwrap().iter()
        .map(|e| e["name"].as_str().unwrap()).collect();
    for fname in ["authenticate", "revoke", "whoami"] {
        assert!(names.contains(&fname), "impact of User missing {fname}: {names:?}");
    }
}
```

Run: `cargo test -p codegraph --test impact_test`
Expected: FAIL.

- [ ] **Step 2: Implement `commands/impact.rs`**

```rust
use crate::cli::ImpactArgs;
use crate::index::{build_index, DefKind, RefKind};
use crate::output::print_json;
use crate::resolve::resolve_refs;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet, VecDeque};

const MAX_DEPTH: usize = 6;

#[derive(Serialize, Clone)]
struct ImpactEntry {
    name: String,
    kind: &'static str,
    file: String,
    line: usize,
    distance: usize,
    confidence: &'static str,
    reason: &'static str,
}

pub fn run(args: ImpactArgs) -> anyhow::Result<()> {
    let idx = build_index(&args.path)?;

    let mut entries: BTreeMap<(String, String, usize), ImpactEntry> = BTreeMap::new();
    // Seed: the symbol itself (every matching definition).
    for d in idx.definitions.iter().filter(|d| d.name == args.name) {
        entries.insert(
            (d.name.clone(), d.file.clone(), d.line),
            ImpactEntry {
                name: d.name.clone(),
                kind: kind_label(d.kind),
                file: d.file.clone(),
                line: d.line,
                distance: 0,
                confidence: "high",
                reason: "same-file-scope",
            },
        );
    }

    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    let mut visited: HashSet<String> = HashSet::new();
    queue.push_back((args.name.clone(), 0));
    visited.insert(args.name.clone());

    while let Some((current, dist)) = queue.pop_front() {
        if dist >= MAX_DEPTH { continue; }
        for resolved in resolve_refs(&idx, &current) {
            let Some(enclosing) = idx.enclosing_definition(&resolved.reference.file, resolved.reference.byte_offset) else { continue; };
            let key = (enclosing.name.clone(), enclosing.file.clone(), enclosing.line);
            entries.entry(key).or_insert(ImpactEntry {
                name: enclosing.name.clone(),
                kind: kind_label(enclosing.kind),
                file: enclosing.file.clone(),
                line: enclosing.line,
                distance: dist + 1,
                confidence: resolved.confidence.as_str(),
                reason: resolved.reason.as_str(),
            });
            // Recurse on call-kind references — type-position uses don't propagate impact.
            if resolved.reference.kind == RefKind::Call && matches!(enclosing.kind, DefKind::Fn | DefKind::Method) && visited.insert(enclosing.name.clone()) {
                queue.push_back((enclosing.name.clone(), dist + 1));
            }
        }
    }

    let mut out: Vec<ImpactEntry> = entries.into_values().collect();
    out.sort_by(|a, b| a.distance.cmp(&b.distance).then(a.file.cmp(&b.file)).then(a.line.cmp(&b.line)));
    if args.json {
        print_json(&out)?;
    } else {
        for e in &out {
            println!("{:>2}  {}:{}  {:<10} {}  ({} {})",
                e.distance, e.file, e.line, e.kind, e.name, e.confidence, e.reason);
        }
    }
    Ok(())
}

fn kind_label(k: DefKind) -> &'static str {
    match k {
        DefKind::Fn => "fn",
        DefKind::Struct => "struct",
        DefKind::Enum => "enum",
        DefKind::Trait => "trait",
        DefKind::Class => "class",
        DefKind::Interface => "interface",
        DefKind::Type => "type",
        DefKind::Const => "const",
        DefKind::Method => "method",
    }
}
```

- [ ] **Step 3: Run the tests**

```bash
cargo test -p codegraph --test impact_test
```

Expected: PASS. If `impact_of_user_struct_includes_type_position_uses` fails because `User` references in function signatures aren't captured, double-check that `rust_refs.scm` includes `(type_identifier) @name @ref.reference` (Task 7).

- [ ] **Step 4: Workspace gate + commit**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

git add crates/codegraph
git commit -m "feat(codegraph): impact subcommand for rust (BFS callers + type users)"
```

---

## Phase 3 — TypeScript / TSX / JavaScript

Add a TS fixture, then wire defs/imports/refs queries. Lock in the same Resolver behavior — only the language-specific query files change, plus the `module_matches` heuristic for JS/TS module paths.

### Task 12: TS/JS fixture

**Files:**
- Create: `crates/codegraph/tests/fixtures/multi_lang/ts_app/src/user.ts`
- Create: `crates/codegraph/tests/fixtures/multi_lang/ts_app/src/auth.ts`
- Create: `crates/codegraph/tests/fixtures/multi_lang/ts_app/src/handlers.ts`
- Create: `crates/codegraph/tests/fixtures/multi_lang/ts_app/src/component.tsx`
- Create: `crates/codegraph/tests/fixtures/multi_lang/js_app/src/util.js`
- Create: `crates/codegraph/tests/fixtures/multi_lang/js_app/src/index.js`

- [ ] **Step 1: Create `ts_app/src/user.ts`**

```typescript
export interface User {
  id: number;
  name: string;
}

export function newUser(id: number, name: string): User {
  return { id, name };
}

export function unusedHelper(): number {
  return 42;
}
```

- [ ] **Step 2: Create `ts_app/src/auth.ts`**

```typescript
import { User } from "./user";

export function authenticate(u: User): boolean {
  return u.name.length > 0;
}

export function revoke(u: User): boolean {
  return authenticate(u);
}
```

- [ ] **Step 3: Create `ts_app/src/handlers.ts`**

```typescript
import { authenticate } from "./auth";
import { newUser, User } from "./user";

export function login(name: string): User | null {
  const u = newUser(1, name);
  return authenticate(u) ? u : null;
}

export function whoami(u: User): string {
  return u.name;
}
```

- [ ] **Step 4: Create `ts_app/src/component.tsx`**

```tsx
import { User } from "./user";

export interface HeaderProps { user: User; }

export function Header(props: HeaderProps) {
  return null as unknown as JSX.Element;
}
```

- [ ] **Step 5: Create `js_app/src/util.js`**

```javascript
export function add(a, b) {
  return a + b;
}

export function unused() {
  return null;
}
```

- [ ] **Step 6: Create `js_app/src/index.js`**

```javascript
import { add } from "./util.js";

export function main() {
  return add(1, 2);
}
```

- [ ] **Step 7: Commit**

```bash
git add crates/codegraph/tests/fixtures/multi_lang/ts_app crates/codegraph/tests/fixtures/multi_lang/js_app
git commit -m "test(codegraph): add ts/tsx/js fixtures"
```

---

### Task 13: TypeScript queries + indexer wiring

**Files:**
- Create: `crates/codegraph/src/queries/typescript_defs.scm`
- Create: `crates/codegraph/src/queries/typescript_imports.scm`
- Create: `crates/codegraph/src/queries/typescript_refs.scm`
- Modify: `crates/codegraph/src/lang.rs` (extend `query_source` for TS/Tsx)
- Modify: `crates/codegraph/src/resolve.rs` (extend `module_matches` for relative paths)
- Modify: `crates/codegraph/src/index.rs` (extend `is_exported` for TS)
- Test: `crates/codegraph/tests/find_refs_test.rs`, `callers_test.rs`, `impact_test.rs`

- [ ] **Step 1: Write the failing test**

Append to `tests/find_refs_test.rs`:

```rust
fn ts_fixture() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/multi_lang/ts_app")
}

#[test]
fn ts_cross_file_import_is_high_confidence() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_codegraph"))
        .args(["find-refs", "authenticate", "--json", "--path"])
        .arg(ts_fixture())
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let cross = v["data"].as_array().unwrap().iter().find(|h|
        h["file"].as_str().unwrap().ends_with("handlers.ts") && h["kind"] == "call"
    ).expect("handlers.ts call to authenticate");
    assert_eq!(cross["confidence"], "high");
    assert_eq!(cross["reason"], "import-resolved");
}
```

Run it — expected FAIL because TS queries are not wired.

- [ ] **Step 2: Create `crates/codegraph/src/queries/typescript_defs.scm`**

```scheme
(function_declaration name: (identifier) @name) @def.fn
(class_declaration name: (type_identifier) @name) @def.class
(interface_declaration name: (type_identifier) @name) @def.interface
(type_alias_declaration name: (type_identifier) @name) @def.type
(enum_declaration name: (identifier) @name) @def.enum
(lexical_declaration "const" (variable_declarator name: (identifier) @name)) @def.const

; Methods inside classes.
(class_body (method_definition name: (property_identifier) @name) @def.method)
```

- [ ] **Step 3: Create `crates/codegraph/src/queries/typescript_imports.scm`**

```scheme
; import { foo, bar as bb } from "./mod";
(import_statement
  (import_clause
    (named_imports
      (import_specifier
        name: (identifier) @name
        alias: (identifier)? @alias)))
  source: (string) @path) @import

; import foo from "./mod";   (default import)
(import_statement
  (import_clause
    (identifier) @name)
  source: (string) @path) @import

; import * as foo from "./mod";
(import_statement
  (import_clause
    (namespace_import (identifier) @name))
  source: (string) @path) @import
```

- [ ] **Step 4: Create `crates/codegraph/src/queries/typescript_refs.scm`**

```scheme
(call_expression function: (identifier) @name) @ref.call
(call_expression function: (member_expression property: (property_identifier) @name)) @ref.call

(type_identifier) @name @ref.reference
(identifier) @name @ref.reference
```

The `(identifier) @ref.reference` is broad — it will produce many low-confidence hits. The resolver downgrades them appropriately (Rule 5). If clippy/benchmark complains about index size on large repos, tighten by replacing it with `(type_reference (identifier) @name)` and similar specific patterns in a later iteration.

- [ ] **Step 5: Extend `lang.rs`**

```rust
    pub fn query_source(self, kind: QueryKind) -> Option<&'static str> {
        match (self, kind) {
            (Language::Rust, QueryKind::Defs) => Some(include_str!("queries/rust_defs.scm")),
            (Language::Rust, QueryKind::Imports) => Some(include_str!("queries/rust_imports.scm")),
            (Language::Rust, QueryKind::Refs) => Some(include_str!("queries/rust_refs.scm")),
            (Language::TypeScript | Language::Tsx, QueryKind::Defs) => Some(include_str!("queries/typescript_defs.scm")),
            (Language::TypeScript | Language::Tsx, QueryKind::Imports) => Some(include_str!("queries/typescript_imports.scm")),
            (Language::TypeScript | Language::Tsx, QueryKind::Refs) => Some(include_str!("queries/typescript_refs.scm")),
            _ => None,
        }
    }
```

- [ ] **Step 6: Teach the resolver about relative paths**

Update `module_matches` in `resolve.rs` to handle paths that look like `"./auth"` or `"../user"`:

```rust
fn module_matches(def: &Definition, imp: &Import) -> bool {
    if imp.module_path.is_empty() { return false; }
    let raw = imp.module_path.trim_matches('"').trim_matches('\'');
    let def_file = def.file.replace('\\', "/");

    // Rust-style first.
    if raw.starts_with("crate::") || raw == "crate" || raw.starts_with("self::") {
        let path = raw.trim_start_matches("crate::").trim_start_matches("self::").replace("::", "/");
        if path.is_empty() {
            return def_file == "src/lib.rs" || def_file == "src/main.rs" || def_file == "lib.rs";
        }
        return [
            format!("src/{path}.rs"),
            format!("src/{path}/mod.rs"),
            format!("{path}.rs"),
            format!("{path}/mod.rs"),
        ].iter().any(|s| s == &def_file);
    }

    // JS/TS-style relative paths.
    if raw.starts_with("./") || raw.starts_with("../") || raw.starts_with('/') {
        // We don't know the importing file's location here — resolve relative to def_file's parent.
        // Heuristic: strip leading "./", drop extension, check whether the result equals the def's
        // module name when chopped from the def_file basename.
        let trimmed = raw.trim_start_matches("./");
        let trimmed = trimmed.trim_end_matches(".ts").trim_end_matches(".tsx")
            .trim_end_matches(".js").trim_end_matches(".mjs").trim_end_matches(".cjs");
        // Quick win: import string's last segment matches the def's file stem.
        let import_last = trimmed.rsplit('/').next().unwrap_or(trimmed);
        let def_stem = std::path::Path::new(&def_file).file_stem().and_then(|s| s.to_str()).unwrap_or("");
        return import_last == def_stem;
    }

    // Python dotted path — handled in Task 18.
    false
}
```

- [ ] **Step 7: Teach `is_exported` about TS**

Update `is_exported` in `index.rs` so TS definitions that lack an `export` keyword are marked `exported = false`. The grammar exposes `export_statement` parents around exported declarations:

```rust
fn is_exported(node: tree_sitter::Node<'_>, bytes: &[u8], lang: Language) -> bool {
    match lang {
        Language::Rust => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "visibility_modifier" {
                    let text = child.utf8_text(bytes).unwrap_or("");
                    if text.starts_with("pub") { return true; }
                }
            }
            false
        }
        Language::TypeScript | Language::Tsx | Language::JavaScript => {
            // Walk parents looking for an export_statement.
            let mut p = node.parent();
            while let Some(node) = p {
                if node.kind() == "export_statement" || node.kind() == "export_clause" {
                    return true;
                }
                p = node.parent();
            }
            false
        }
        Language::Python => {
            // Module-level definitions are conventionally "public" — we leave this true for now
            // and refine in Task 18 if necessary.
            true
        }
    }
}
```

- [ ] **Step 8: Run all the suites**

```bash
cargo test -p codegraph
```

Expected: all PASS. If `ts_cross_file_import_is_high_confidence` fails, print the index with `cargo run -p codegraph -- find-refs authenticate --json --path crates/codegraph/tests/fixtures/multi_lang/ts_app | jq` and inspect.

- [ ] **Step 9: Workspace gate + commit**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

git add crates/codegraph
git commit -m "feat(codegraph): typescript and tsx support"
```

---

### Task 14: JavaScript queries

**Files:**
- Create: `crates/codegraph/src/queries/javascript_defs.scm`
- Create: `crates/codegraph/src/queries/javascript_imports.scm`
- Create: `crates/codegraph/src/queries/javascript_refs.scm`
- Modify: `crates/codegraph/src/lang.rs`
- Test: append JS cases to `find_refs_test.rs`

- [ ] **Step 1: Write the failing test**

```rust
fn js_fixture() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/multi_lang/js_app")
}

#[test]
fn js_cross_file_import_resolves() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_codegraph"))
        .args(["find-refs", "add", "--json", "--path"])
        .arg(js_fixture())
        .output()
        .expect("run");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let call = v["data"].as_array().unwrap().iter().find(|h|
        h["file"].as_str().unwrap().ends_with("index.js") && h["kind"] == "call"
    ).expect("index.js call to add");
    assert_eq!(call["confidence"], "high");
}
```

Expected: FAIL.

- [ ] **Step 2: Create JS query files**

`javascript_defs.scm`:

```scheme
(function_declaration name: (identifier) @name) @def.fn
(class_declaration name: (identifier) @name) @def.class
(method_definition name: (property_identifier) @name) @def.method
(lexical_declaration "const" (variable_declarator name: (identifier) @name)) @def.const
```

`javascript_imports.scm`:

```scheme
(import_statement
  (import_clause
    (named_imports
      (import_specifier
        name: (identifier) @name
        alias: (identifier)? @alias)))
  source: (string) @path) @import

(import_statement
  (import_clause (identifier) @name)
  source: (string) @path) @import

(import_statement
  (import_clause (namespace_import (identifier) @name))
  source: (string) @path) @import
```

`javascript_refs.scm`:

```scheme
(call_expression function: (identifier) @name) @ref.call
(call_expression function: (member_expression property: (property_identifier) @name)) @ref.call
(identifier) @name @ref.reference
```

- [ ] **Step 3: Wire `Language::JavaScript` in `lang.rs`**

Replace the entire `query_source` body so JS arms join the Rust and TS arms:

```rust
    pub fn query_source(self, kind: QueryKind) -> Option<&'static str> {
        match (self, kind) {
            (Language::Rust, QueryKind::Defs) => Some(include_str!("queries/rust_defs.scm")),
            (Language::Rust, QueryKind::Imports) => Some(include_str!("queries/rust_imports.scm")),
            (Language::Rust, QueryKind::Refs) => Some(include_str!("queries/rust_refs.scm")),
            (Language::TypeScript | Language::Tsx, QueryKind::Defs) => Some(include_str!("queries/typescript_defs.scm")),
            (Language::TypeScript | Language::Tsx, QueryKind::Imports) => Some(include_str!("queries/typescript_imports.scm")),
            (Language::TypeScript | Language::Tsx, QueryKind::Refs) => Some(include_str!("queries/typescript_refs.scm")),
            (Language::JavaScript, QueryKind::Defs) => Some(include_str!("queries/javascript_defs.scm")),
            (Language::JavaScript, QueryKind::Imports) => Some(include_str!("queries/javascript_imports.scm")),
            (Language::JavaScript, QueryKind::Refs) => Some(include_str!("queries/javascript_refs.scm")),
            _ => None,
        }
    }
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p codegraph
```

Expected: PASS.

- [ ] **Step 5: Workspace gate + commit**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

git add crates/codegraph
git commit -m "feat(codegraph): javascript support"
```

---

### Task 15: Python fixture + queries

**Files:**
- Create: `crates/codegraph/tests/fixtures/multi_lang/py_app/auth.py`
- Create: `crates/codegraph/tests/fixtures/multi_lang/py_app/handlers.py`
- Create: `crates/codegraph/tests/fixtures/multi_lang/py_app/user.py`
- Create: `crates/codegraph/src/queries/python_defs.scm`
- Create: `crates/codegraph/src/queries/python_imports.scm`
- Create: `crates/codegraph/src/queries/python_refs.scm`
- Modify: `crates/codegraph/src/lang.rs`
- Modify: `crates/codegraph/src/resolve.rs` (Python dotted paths)
- Test: `crates/codegraph/tests/find_refs_test.rs`

- [ ] **Step 1: Create Python fixture**

`py_app/user.py`:

```python
class User:
    def __init__(self, id: int, name: str) -> None:
        self.id = id
        self.name = name


def new_user(id: int, name: str) -> User:
    return User(id, name)


def unused_helper() -> int:
    return 42
```

`py_app/auth.py`:

```python
from .user import User


def authenticate(u: User) -> bool:
    return len(u.name) > 0


def revoke(u: User) -> bool:
    return authenticate(u)
```

`py_app/handlers.py`:

```python
from .auth import authenticate
from .user import User, new_user


def login(name: str):
    u = new_user(1, name)
    return u if authenticate(u) else None


def whoami(u: User) -> str:
    return u.name
```

- [ ] **Step 2: Write the failing test**

Append to `tests/find_refs_test.rs`:

```rust
fn py_fixture() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/multi_lang/py_app")
}

#[test]
fn python_cross_file_import_resolves() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_codegraph"))
        .args(["find-refs", "authenticate", "--json", "--path"])
        .arg(py_fixture())
        .output()
        .expect("run");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let cross = v["data"].as_array().unwrap().iter().find(|h|
        h["file"].as_str().unwrap().ends_with("handlers.py") && h["kind"] == "call"
    ).expect("handlers.py call to authenticate");
    assert_eq!(cross["confidence"], "high");
    assert_eq!(cross["reason"], "import-resolved");
}
```

Expected: FAIL.

- [ ] **Step 3: Create Python query files**

`python_defs.scm`:

```scheme
(module
  (function_definition name: (identifier) @name) @def.fn)

(module
  (class_definition name: (identifier) @name) @def.class)

(module
  (decorated_definition
    definition: (function_definition name: (identifier) @name) @def.fn))

(module
  (decorated_definition
    definition: (class_definition name: (identifier) @name) @def.class))

(class_definition
  body: (block (function_definition name: (identifier) @name) @def.method))
```

`python_imports.scm`:

```scheme
; from .user import User, new_user as nu
(import_from_statement
  module_name: (_) @path
  name: (dotted_name (identifier) @name)) @import

(import_from_statement
  module_name: (_) @path
  name: (aliased_import
    name: (dotted_name (identifier) @name)
    alias: (identifier) @alias)) @import

; import foo
(import_statement
  name: (dotted_name (identifier) @name)) @import

; import foo as bar
(import_statement
  name: (aliased_import
    name: (dotted_name (identifier) @name)
    alias: (identifier) @alias)) @import
```

`python_refs.scm`:

```scheme
(call (identifier) @name) @ref.call
(call (attribute attribute: (identifier) @name)) @ref.call
(identifier) @name @ref.reference
```

- [ ] **Step 4: Wire Python in `lang.rs`**

Replace the entire `query_source` body so Python arms complete the table — at this point every supported language has all three query files wired:

```rust
    pub fn query_source(self, kind: QueryKind) -> Option<&'static str> {
        match (self, kind) {
            (Language::Rust, QueryKind::Defs) => Some(include_str!("queries/rust_defs.scm")),
            (Language::Rust, QueryKind::Imports) => Some(include_str!("queries/rust_imports.scm")),
            (Language::Rust, QueryKind::Refs) => Some(include_str!("queries/rust_refs.scm")),
            (Language::TypeScript | Language::Tsx, QueryKind::Defs) => Some(include_str!("queries/typescript_defs.scm")),
            (Language::TypeScript | Language::Tsx, QueryKind::Imports) => Some(include_str!("queries/typescript_imports.scm")),
            (Language::TypeScript | Language::Tsx, QueryKind::Refs) => Some(include_str!("queries/typescript_refs.scm")),
            (Language::JavaScript, QueryKind::Defs) => Some(include_str!("queries/javascript_defs.scm")),
            (Language::JavaScript, QueryKind::Imports) => Some(include_str!("queries/javascript_imports.scm")),
            (Language::JavaScript, QueryKind::Refs) => Some(include_str!("queries/javascript_refs.scm")),
            (Language::Python, QueryKind::Defs) => Some(include_str!("queries/python_defs.scm")),
            (Language::Python, QueryKind::Imports) => Some(include_str!("queries/python_imports.scm")),
            (Language::Python, QueryKind::Refs) => Some(include_str!("queries/python_refs.scm")),
        }
    }
```

Note: with every variant covered the trailing `_ => None` arm is gone — exhaustiveness checking now ensures future Language variants can't silently skip query wiring.

- [ ] **Step 5: Teach `module_matches` about Python**

Append to `resolve.rs::module_matches`, just before the final `false`:

```rust
    // Python dotted paths. Examples:
    //   `.user`              → relative; match against any file with stem == "user".
    //   `package.user`       → match against `package/user.py`.
    //   `package.user.sub`   → match against `package/user/sub.py`.
    if raw.starts_with('.') || raw.chars().any(|c| c == '.') {
        let normalized = raw.trim_start_matches('.').replace('.', "/");
        let candidates = [
            format!("{normalized}.py"),
            format!("{normalized}/__init__.py"),
        ];
        let def_stem = std::path::Path::new(&def_file).file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let last = normalized.rsplit('/').next().unwrap_or(&normalized);
        return candidates.iter().any(|c| def_file.ends_with(c)) || def_stem == last;
    }
    false
```

(The order matters — keep this branch *after* the Rust and JS branches.)

- [ ] **Step 6: Run tests**

```bash
cargo test -p codegraph
```

Expected: PASS.

- [ ] **Step 7: Workspace gate + commit**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

git add crates/codegraph
git commit -m "feat(codegraph): python support"
```

---

## Phase 4 — Polish

### Task 16: `--depth N` for `callers` and `callees`

**Files:**
- Modify: `crates/codegraph/src/commands/callers.rs`
- Modify: `crates/codegraph/src/commands/callees.rs`
- Test: extend `callers_test.rs`

The two commands currently bail when `depth != 1`. Replace that with a BFS that mirrors `impact`. Output gains a `distance` field.

- [ ] **Step 1: Write the failing test**

Append to `tests/callers_test.rs`:

```rust
#[test]
fn callers_depth_2_walks_one_more_hop() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_codegraph"))
        .args(["callers", "authenticate", "--depth", "2", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let entries = v["data"].as_array().unwrap();
    // login calls authenticate (distance=1); nothing calls login in this fixture,
    // so depth=2 should match depth=1 in count — but the field must be present.
    assert!(entries.iter().all(|e| e["distance"].is_number()));
    assert!(entries.iter().any(|e| e["name"] == "revoke" && e["distance"] == 1));
}
```

Expected: FAIL — current code bails on depth!=1 and has no `distance` field.

- [ ] **Step 2: Rewrite `callers.rs` with BFS**

Replace the `if args.depth != 1 { anyhow::bail!(...) }` guard. The whole function becomes:

```rust
use crate::cli::CallersArgs;
use crate::index::{build_index, DefKind, RefKind};
use crate::output::print_json;
use crate::resolve::resolve_refs;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet, VecDeque};

const HARD_CAP: usize = 8;

#[derive(Serialize, Clone)]
struct CallerEntry {
    file: String,
    line: usize,
    column: usize,
    name: String,
    kind: &'static str,
    distance: usize,
    confidence: &'static str,
    reason: &'static str,
    sites: Vec<CallSite>,
}

#[derive(Serialize, Clone)]
struct CallSite {
    file: String,
    line: usize,
    column: usize,
    context: String,
}

pub fn run(args: CallersArgs) -> anyhow::Result<()> {
    let depth_limit = args.depth.min(HARD_CAP);
    let idx = build_index(&args.path)?;

    let mut by_caller: BTreeMap<(String, String, usize), CallerEntry> = BTreeMap::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    queue.push_back((args.name.clone(), 0));
    visited.insert(args.name.clone());

    while let Some((current, dist)) = queue.pop_front() {
        if dist >= depth_limit { continue; }
        for r in resolve_refs(&idx, &current) {
            if r.reference.kind != RefKind::Call { continue; }
            let Some(enclosing) = idx.enclosing_definition(&r.reference.file, r.reference.byte_offset) else { continue; };
            if !matches!(enclosing.kind, DefKind::Fn | DefKind::Method) { continue; }
            let key = (enclosing.name.clone(), enclosing.file.clone(), enclosing.line);
            let entry = by_caller.entry(key).or_insert_with(|| CallerEntry {
                file: enclosing.file.clone(),
                line: enclosing.line,
                column: enclosing.column,
                name: enclosing.name.clone(),
                kind: if enclosing.kind == DefKind::Method { "method" } else { "fn" },
                distance: dist + 1,
                confidence: r.confidence.as_str(),
                reason: r.reason.as_str(),
                sites: Vec::new(),
            });
            entry.sites.push(CallSite {
                file: r.reference.file.clone(),
                line: r.reference.line,
                column: r.reference.column,
                context: r.reference.context.clone(),
            });
            if visited.insert(enclosing.name.clone()) {
                queue.push_back((enclosing.name.clone(), dist + 1));
            }
        }
    }

    let mut entries: Vec<CallerEntry> = by_caller.into_values().collect();
    entries.sort_by(|a, b| a.distance.cmp(&b.distance).then(a.file.cmp(&b.file)).then(a.line.cmp(&b.line)));
    if args.json {
        print_json(&entries)?;
    } else {
        for e in &entries {
            println!("d={}  {}:{}  {}  {}  ({} call site(s), {} {})",
                e.distance, e.file, e.line, e.kind, e.name, e.sites.len(), e.confidence, e.reason);
        }
    }
    Ok(())
}
```

- [ ] **Step 3: Apply the same BFS pattern to `callees.rs`**

Adapt the existing function so that `depth > 1` recurses through resolved callees:

```rust
use crate::cli::CalleesArgs;
use crate::index::{build_index, DefKind, RefKind};
use crate::output::print_json;
use crate::resolve::resolve_refs;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet, VecDeque};

const HARD_CAP: usize = 8;

#[derive(Serialize, Clone)]
struct CalleeEntry {
    name: String,
    kind: &'static str,
    def_file: Option<String>,
    def_line: Option<usize>,
    distance: usize,
    confidence: &'static str,
    reason: &'static str,
    sites: Vec<CallSite>,
}

#[derive(Serialize, Clone)]
struct CallSite { file: String, line: usize, column: usize, context: String }

pub fn run(args: CalleesArgs) -> anyhow::Result<()> {
    let depth_limit = args.depth.min(HARD_CAP);
    let idx = build_index(&args.path)?;

    let mut entries: BTreeMap<String, CalleeEntry> = BTreeMap::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    queue.push_back((args.name.clone(), 0));
    visited.insert(args.name.clone());

    while let Some((current, dist)) = queue.pop_front() {
        if dist >= depth_limit { continue; }
        let outer: Vec<_> = idx.definitions.iter()
            .filter(|d| d.name == current && matches!(d.kind, DefKind::Fn | DefKind::Method)).collect();
        if outer.is_empty() { continue; }
        for r in &idx.references {
            if r.kind != RefKind::Call { continue; }
            if !outer.iter().any(|o| o.file == r.file && o.body_start_byte <= r.byte_offset && r.byte_offset < o.body_end_byte) { continue; }
            let resolutions = resolve_refs(&idx, &r.name);
            let chosen = resolutions.iter().find(|res| res.reference.byte_offset == r.byte_offset && res.reference.file == r.file);
            let (confidence, reason, def_file, def_line) = match chosen {
                Some(res) => (res.confidence.as_str(), res.reason.as_str(), res.definition.map(|d| d.file.clone()), res.definition.map(|d| d.line)),
                None => ("low", "name-only", None, None),
            };
            let entry = entries.entry(r.name.clone()).or_insert(CalleeEntry {
                name: r.name.clone(),
                kind: "fn",
                def_file: def_file.clone(),
                def_line,
                distance: dist + 1,
                confidence,
                reason,
                sites: Vec::new(),
            });
            entry.sites.push(CallSite { file: r.file.clone(), line: r.line, column: r.column, context: r.context.clone() });
            if visited.insert(r.name.clone()) {
                queue.push_back((r.name.clone(), dist + 1));
            }
        }
    }
    let mut out: Vec<CalleeEntry> = entries.into_values().collect();
    out.sort_by(|a, b| a.distance.cmp(&b.distance).then(a.name.cmp(&b.name)));
    if args.json {
        print_json(&out)?;
    } else {
        for e in &out {
            let target = match (&e.def_file, e.def_line) {
                (Some(f), Some(l)) => format!("{f}:{l}"),
                _ => "<unresolved>".to_string(),
            };
            println!("d={}  {}  -> {}  ({} {}; {} site(s))", e.distance, e.name, target, e.confidence, e.reason, e.sites.len());
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p codegraph
```

Expected: PASS.

- [ ] **Step 5: Workspace gate + commit**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

git add crates/codegraph
git commit -m "feat(codegraph): --depth N BFS for callers and callees"
```

---

### Task 17: Finalise SKILL.md and verify end-to-end build

**Files:**
- Modify: `skills/ny-codegraph/SKILL.md`
- Verify: `./scripts/build-skills.sh` produces `skills/ny-codegraph/scripts/codegraph` and the binary runs.

- [ ] **Step 1: Replace `skills/ny-codegraph/SKILL.md` with the final manifest**

```markdown
---
name: ny-codegraph
description: Use when you need cross-reference answers a `grep` cannot give you — "where else is X used", "who calls Y", "what does Z call", "what would break if I change W". `codegraph` parses the project with tree-sitter and reports references, callers, callees, and transitive impact, each tagged with a `confidence` (`high`/`medium`/`low`) and a `reason` so you know whether to trust a hit. Trigger on phrases like "find references to", "who calls", "what does X call", "callers of", "callees of", "impact of", "what breaks if I change", "หาที่ใช้", "ใครเรียกฟังก์ชันนี้", "ถ้าแก้ X อะไรพัง". Supports Rust, TypeScript, TSX, JavaScript, Python.
---

# codegraph

`codegraph` is a CLI in the `skills` monorepo that uses tree-sitter to build an in-memory cross-reference index of a project, then answers four questions every refactor needs:

- **Find references** — every site where a symbol is used (calls, type positions, imports, definition).
- **Callers** — functions that call a given function.
- **Callees** — functions called by a given function.
- **Impact** — transitive callers + non-call usages.

## When to use

- The user asks "where is X used", "who calls Y", "what does Z call", "what breaks if I change W".
- You are about to refactor and need to know the blast radius before editing.
- You suspect a function is dead but want to confirm before deleting.
- A memory mentions a function; you want to verify it is still called somewhere.

`codegraph` is not rust-analyzer / tsserver. It does **not** resolve types, follow re-exports, or expand macros. Every hit carries a `confidence` and `reason` so you can downrank or expand uncertain answers.

## Run

The skill ships a pre-built binary:

```bash
./scripts/codegraph <subcommand> [flags]
```

If missing, run `./scripts/install.sh` (downloads from Releases) or `./scripts/build-skills.sh` (local cargo build) from the `skills` repo root.

## Subcommands

All take `--path <DIR>` (default `.`) and `--json` (default human-readable). JSON output wraps an array in `{"schema_version": 1, "data": [...]}`.

| Subcommand | Purpose |
| --- | --- |
| `codegraph find-refs <NAME>` | Every reference to `<NAME>` across the project |
| `codegraph callers <FN> [--depth N]` | Functions that call `<FN>` (BFS up to depth N, default 1) |
| `codegraph callees <FN> [--depth N]` | Functions called by `<FN>` |
| `codegraph impact <NAME>` | Transitive callers + non-call usages of `<NAME>` |

## Output schema

Each entry in `data` has at least:

```json
{
  "file": "src/lib.rs",
  "line": 42,
  "column": 12,
  "kind": "call" | "definition" | "import" | "reference",
  "name": "User",
  "context": "    let u = User::new();",
  "confidence": "high" | "medium" | "low",
  "reason": "same-file-scope" | "import-resolved" | "name-only"
}
```

`callers` / `callees` / `impact` add a `distance` field (0 for the queried symbol, 1 for direct callers/callees, etc.) and `callers` / `callees` add a `sites` array with the individual call locations.

## Confidence rules

- **`high` + `same-file-scope`** — the reference and a matching definition live in the same file.
- **`high` + `import-resolved`** — the file imports the symbol from the file that defines it.
- **`medium` + `import-resolved`** — a glob/wildcard import (`use foo::*`, `from foo import *`) plausibly carries the symbol.
- **`medium` + `name-only`** — name matches and exactly one definition exists project-wide.
- **`low` + `name-only`** — name matches; the resolver couldn't pin a definition.

`low` hits are useful as a "look here too" hint; don't take them as truth.

## Limits

- Method dispatch is name-only — `user.save()` looks like a reference to `save`, not specifically `User::save`.
- Rust macros are opaque — `println!` is a reference to `println`, the macro body is not walked.
- No persistent cache — every invocation re-parses the project. Plenty fast for repos up to ~50k LoC.
- TS re-exports (`export * from "./mod"`) are not followed.

## Usage tips for agents

- Always pass `--json` when parsing programmatically; assert `result.schema_version === 1`.
- Prefer `find-refs <name>` for "where is this used" — it returns definitions *and* references in one call.
- Use `callers <fn> --depth 3 --json` before suggesting a non-trivial signature change.
- For dead-code questions: if `callers --json` returns an empty array AND `find-refs --json` only contains the `definition` entry, the symbol is unused (within the limits above).
- Pair with `codemap symbols <file>` when you need a list of definitions in a single file.

## Examples

```bash
./scripts/codegraph find-refs UserRepo --json --path ./my-repo
./scripts/codegraph callers handleSignup --depth 3 --json
./scripts/codegraph callees buildIndex --json
./scripts/codegraph impact ConfigLoader --json
```

## Supported languages

Rust, TypeScript, TSX, JavaScript, Python — the same set as `codemap`. Adding a new language: drop three `<lang>_{defs,imports,refs}.scm` files into `crates/codegraph/src/queries/`, wire them in `src/lang.rs`, extend `module_matches` in `src/resolve.rs`.
```

- [ ] **Step 2: Build the skill binary**

```bash
./scripts/build-skills.sh
```

Expected: prints `built skills/ny-codegraph/scripts/codegraph` among the lines, exits 0.

- [ ] **Step 3: Smoke-test the installed binary**

```bash
./skills/ny-codegraph/scripts/codegraph find-refs authenticate --json --path crates/codegraph/tests/fixtures/multi_lang/rust_app | head -c 400
```

Expected: prints a JSON envelope containing `"schema_version":1` and a `"data"` array with at least one entry.

- [ ] **Step 4: Workspace gate**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add skills/ny-codegraph/SKILL.md
git commit -m "docs(codegraph): finalise SKILL.md for agent discovery"
```

---

### Task 18: Update CLAUDE.md guidance

**Files:**
- Modify: `CLAUDE.md`

Add a paragraph to `CLAUDE.md` noting that `crates/codegraph` follows the same template as `crates/codemap` but additionally maintains three `<lang>_{defs,imports,refs}.scm` query files per supported language and a `resolve.rs` module for cross-file resolution.

- [ ] **Step 1: Append the following to `CLAUDE.md` after the "Architecture notes for `codemap`" section**

```markdown
## Architecture notes for `codegraph`

`crates/codegraph` extends the `codemap` template with semantic cross-references. Layout differences worth knowing before editing:

- Three query files per language (`<lang>_defs.scm`, `<lang>_imports.scm`, `<lang>_refs.scm`) — not one. `Language::query_source` returns `Option<&'static str>` keyed on a `QueryKind` enum.
- `src/index.rs` owns the three in-memory tables (`Definition`, `Import`, `Reference`) and the `build_index` builder. Every subcommand starts by calling `build_index(&path)`.
- `src/resolve.rs` is the confidence/reason tagger. Cross-file matching is heuristic — see `module_matches` for the per-language path rules. When you add a new language, extend that function alongside the three `.scm` files.
- Subcommands live in `src/commands/{find_refs,callers,callees,impact}.rs`. `callers`/`callees`/`impact` use BFS bounded by `HARD_CAP = 8`. `--depth` is clamped against that.
- Output schema is the same `{schema_version: 1, data: [...]}` envelope; the entry shape adds `confidence` and `reason`. Documented in `skills/ny-codegraph/SKILL.md` — keep the two in sync.
```

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: add codegraph architecture notes to CLAUDE.md"
```

---

### Task 19: Final workspace gate + manual smoke

- [ ] **Step 1: Full clean build**

```bash
cargo clean -p codegraph
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Expected: PASS.

- [ ] **Step 2: Manual smoke against this very repo**

```bash
./scripts/build-skills.sh
./skills/ny-codegraph/scripts/codegraph find-refs Language --json --path crates/codemap | jq '.data | length'
./skills/ny-codegraph/scripts/codegraph callers walk_sources --json --path crates/codemap | jq '.data[] | .name'
./skills/ny-codegraph/scripts/codegraph impact Symbol --json --path crates/codemap | jq '.data[] | .name'
```

Expected: each command exits 0 and returns plausible names (`walk_sources` should be called from at least `files`/`stats`/`find`/`symbols`, `Symbol` should appear under multiple commands' impact).

- [ ] **Step 3: Release-audit invariant check**

```bash
for crate in crates/*/; do
  name="$(basename "$crate")"
  test -d "skills/ny-$name" || { echo "missing skills/ny-$name"; exit 1; }
done
echo "audit-ok"
```

Expected: prints `audit-ok`.

- [ ] **Step 4: Nothing to commit (verification only). Move on.**

---

## Self-review checklist (perform after writing the plan, before handing off)

1. Every task that creates Rust code shows the full code body — no `// TODO`, no `// ...rest unchanged`.
2. Subcommand names in `cli.rs` (`find-refs`, `callers`, `callees`, `impact`) match the test invocations and the `SKILL.md` table.
3. `DefKind` / `RefKind` / `Confidence` / `ResolveReason` are defined exactly once and referenced consistently in resolver, commands, and tests.
4. The `confidence`/`reason` string literals (`"high"`, `"medium"`, `"low"`, `"same-file-scope"`, `"import-resolved"`, `"name-only"`) are spelled identically wherever they appear in tests and runtime code.
5. The `skills/ny-codegraph/scripts/` directory is only ever created by `build-skills.sh` — the plan never tells the engineer to `git add` anything inside it.
6. The release-audit invariant (`crates/<name>` ⇔ `skills/ny-<name>`) is satisfied from Task 1 onward.
7. Output schema documented in the SKILL.md (Task 17) matches the structs serialized by every subcommand.

If any of those fail at hand-off, fix inline before invoking the executor.
