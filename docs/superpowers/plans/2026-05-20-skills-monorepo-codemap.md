# skills Monorepo + codemap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bootstrap a Cargo workspace `skills` (AssetsArt org) that hosts agent-callable CLI tools, and ship the first skill `codemap` — a tree-sitter–backed code surveyor that lets Claude understand an unfamiliar codebase without reading every file.

**Architecture:**
- Root is the Cargo workspace; every skill lives under `skills/<name>/` as an independent crate compiled to a single binary. Shared deps (`clap`, `serde`, `serde_json`, `anyhow`, `ignore`) and package metadata (license, repo, authors) live in `[workspace.dependencies]` / `[workspace.package]`. Tree-sitter grammars and `.scm` queries are bundled into the binary via `include_str!` — no runtime asset paths.
- `codemap` exposes 5 subcommands (`files`, `tree`, `symbols`, `find`, `stats`), each supporting `--json` and `--path`. Every JSON response is wrapped in a stable v1 envelope `{ "schema_version": 1, "data": ... }`; human mode is compact.
- Adding a new language = add grammar crate + new `.scm` query + register in `lang.rs`. Adding a new skill = add a new member under `skills/`.

**Tech Stack:** Rust 2021 (stable, pinned via `rust-toolchain.toml`), `clap` 4 (derive), `serde` + `serde_json`, `anyhow`, `ignore`, `tree-sitter` 0.22 + grammar crates `tree-sitter-rust = "0.21"`, `tree-sitter-typescript = "0.21"`, `tree-sitter-javascript = "0.21"`, `tree-sitter-python = "0.21"`. GitHub Actions CI on Ubuntu + macOS, all cargo invocations use `--locked`.

**Conventions:**
- TDD where tests are meaningful (logic + parsing). Bootstrap/scaffolding tasks skip the test-first step but always end on a green `cargo build` / `cargo test`.
- Commit per task with `feat:` / `chore:` / `test:` / `style:` prefixes.
- No emojis in code or commits unless asked.
- **JSON envelope** is non-negotiable for every subcommand — all JSON output goes through `crate::output::print_json(data)`. Tests assert `v["schema_version"] == 1` and read payloads via `v["data"]`.

---

## File Structure (locked before tasks)

```
skills/                                          <- repo root, Cargo workspace
├── Cargo.toml                                   <- [workspace] + shared deps/metadata
├── Cargo.lock                                   <- committed; CI uses --locked
├── README.md                                    <- collection overview + skill index
├── LICENSE                                      <- MIT, holder: AssetsArt
├── .gitignore                                   <- Rust standard (does NOT exclude Cargo.lock)
├── rust-toolchain.toml                          <- pin channel = "stable"
├── rustfmt.toml                                 <- ignore = ["**/tests/fixtures/**"]
├── .github/workflows/ci.yml                     <- fmt + clippy + test (ubuntu + macos, --locked)
├── docs/superpowers/plans/                      <- this plan lives here
└── skills/
    └── codemap/                                 <- first member
        ├── Cargo.toml
        ├── SKILL.md                             <- agent-facing manifest (frontmatter + body)
        ├── README.md                            <- human-facing usage docs
        ├── src/
        │   ├── main.rs                          <- entry + subcommand dispatch
        │   ├── cli.rs                           <- clap derive types
        │   ├── lang.rs                          <- ext → Language + query registry
        │   ├── walk.rs                          <- ignore-based walker (shared filter)
        │   ├── output.rs                        <- JSON envelope + print helper
        │   ├── symbols.rs                       <- Symbol struct + tree-sitter extractor
        │   ├── commands/
        │   │   ├── mod.rs
        │   │   ├── files.rs
        │   │   ├── tree.rs
        │   │   ├── symbols.rs
        │   │   ├── find.rs
        │   │   └── stats.rs
        │   └── queries/
        │       ├── rust.scm
        │       ├── typescript.scm               <- shared by ts + tsx
        │       ├── javascript.scm
        │       └── python.scm
        └── tests/
            ├── fixtures/
            │   └── sample_project/
            │       ├── src/lib.rs
            │       ├── src/types.ts
            │       ├── src/component.tsx
            │       ├── src/util.js
            │       └── app.py
            ├── files_test.rs
            ├── symbols_test.rs
            ├── find_test.rs
            └── stats_test.rs
```

Each file has one responsibility; commands are split by subcommand so adding/removing a command is a single-file change. The exclusion list (`target`, `node_modules`, `.git`, `dist`, `build`) lives once in `walk.rs::IGNORED_DIRS` so `commands/tree.rs` can reuse it.

---

## Phase 1 — Workspace Bootstrap

### Task 1: Initialise repo + workspace root

**Files:**
- Create: `/Users/detoro/code/skills/Cargo.toml`
- Create: `/Users/detoro/code/skills/LICENSE`
- Create: `/Users/detoro/code/skills/.gitignore`
- Create: `/Users/detoro/code/skills/README.md`
- Create: `/Users/detoro/code/skills/rust-toolchain.toml`
- Create: `/Users/detoro/code/skills/rustfmt.toml`

- [ ] **Step 1: Initialise git**

Run: `cd /Users/detoro/code/skills && git init -b main`
Expected: `Initialized empty Git repository in /Users/detoro/code/skills/.git/`

- [ ] **Step 2: Write `.gitignore`** (do NOT list `Cargo.lock` — we commit it for `--locked` CI)

```gitignore
/target
**/*.rs.bk
.DS_Store
.idea/
.vscode/
*.swp
```

- [ ] **Step 3: Write `LICENSE` (MIT, holder: AssetsArt)**

```
MIT License

Copyright (c) 2026 AssetsArt

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

- [ ] **Step 4: Write root `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = ["skills/*"]

[workspace.package]
version = "0.1.0"
edition = "2021"
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

[profile.release]
lto = "thin"
codegen-units = 1
strip = true
```

- [ ] **Step 5: Write root `README.md` skeleton**

````markdown
# skills

> AssetsArt's collection of agent-callable CLI tools, written in Rust.

`skills` is a Cargo workspace where each member is a small, focused command-line tool designed to be invoked by AI coding agents (e.g. Claude Code). Every skill ships:

- a single self-contained binary,
- a `SKILL.md` manifest the agent reads to know when and how to call it,
- a human-facing `README.md` with examples.

## Skill Index

| Skill | What it does | Crate |
| --- | --- | --- |
| [codemap](./skills/codemap) | Survey a codebase: list files, show symbols, find definitions | `skills/codemap` |

## Building

```bash
cargo build --release
# or build a single skill:
cargo build --release -p codemap
```

Binaries land in `target/release/<skill-name>`.

## Adding a new skill

1. Create `skills/<your-skill>/` with its own `Cargo.toml` (use `codemap` as a template).
2. Inherit shared metadata via `package.license.workspace = true`, etc.
3. Add a `SKILL.md` (frontmatter `name` + `description`) so agents can discover it.
4. Add a row to the table above.

## Versioning

The workspace uses `version.workspace = true`, so all skills bump together. This is the intended default — release coordination lives at the workspace level. If you need per-skill versioning later, drop `version.workspace` on the specific crate.

## License

MIT — see [LICENSE](./LICENSE).
````

- [ ] **Step 6: Write `rust-toolchain.toml`**

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 7: Write `rustfmt.toml`** (prevents `cargo fmt --all` from rewriting test fixtures and breaking line-number assertions)

```toml
ignore = ["**/tests/fixtures/**"]
```

- [ ] **Step 8: Verify workspace parses**

Run: `cd /Users/detoro/code/skills && cargo metadata --no-deps --format-version 1 >/dev/null`
Expected: exits 0, no error.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml LICENSE .gitignore README.md rust-toolchain.toml rustfmt.toml docs/
git commit -m "chore: bootstrap skills workspace"
```

> `Cargo.lock` is committed together with the codemap skeleton in Task 2.

---

## Phase 2 — codemap Member Skeleton

### Task 2: Create codemap crate (hello-world bin)

**Files:**
- Create: `skills/codemap/Cargo.toml`
- Create: `skills/codemap/src/main.rs`

- [ ] **Step 1: Write `skills/codemap/Cargo.toml`**

```toml
[package]
name = "codemap"
description = "Survey a codebase: list files, extract symbols, find definitions. Built for AI coding agents."
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true
homepage.workspace = true

[[bin]]
name = "codemap"
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

> **Note for engineer:** Versions of `tree-sitter-*` grammars must match the major version of `tree-sitter`. If `cargo build` fails with an ABI mismatch, run `cargo search tree-sitter-rust` and pin the latest grammar compatible with `tree-sitter = "0.22"`. The combo above is the latest known-good pairing.

- [ ] **Step 2: Write `skills/codemap/src/main.rs` (placeholder)**

```rust
fn main() -> anyhow::Result<()> {
    println!("codemap: not yet implemented");
    Ok(())
}
```

- [ ] **Step 3: Verify it builds (this also generates `Cargo.lock`)**

Run: `cargo build -p codemap`
Expected: compiles successfully.

- [ ] **Step 4: Commit (include the freshly generated `Cargo.lock`)**

```bash
git add skills/codemap/ Cargo.lock
git commit -m "feat(codemap): add crate skeleton"
```

---

### Task 3: Wire up clap CLI with subcommand stubs

**Files:**
- Create: `skills/codemap/src/cli.rs`
- Create: `skills/codemap/src/commands/mod.rs`
- Create: `skills/codemap/src/commands/files.rs`
- Create: `skills/codemap/src/commands/tree.rs`
- Create: `skills/codemap/src/commands/symbols.rs`
- Create: `skills/codemap/src/commands/find.rs`
- Create: `skills/codemap/src/commands/stats.rs`
- Modify: `skills/codemap/src/main.rs`

- [ ] **Step 1: Write `src/cli.rs`**

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "codemap",
    version,
    about = "Survey a codebase: files, symbols, find, stats"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List source files grouped by language
    Files(FilesArgs),
    /// Print directory tree (respects .gitignore)
    Tree(TreeArgs),
    /// Extract symbols from a file (or whole project with `.` / `--all`)
    Symbols(SymbolsArgs),
    /// Find symbols by name across the project
    Find(FindArgs),
    /// Project statistics: files, lines, symbol counts per kind
    Stats(StatsArgs),
}

#[derive(clap::Args, Debug)]
pub struct FilesArgs {
    /// Project root
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    /// Output JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct TreeArgs {
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct SymbolsArgs {
    /// File to inspect (relative to --path), or "." for whole project (same as --all)
    pub target: Option<String>,
    /// Inspect whole project
    #[arg(long)]
    pub all: bool,
    /// Filter by kind (comma-separated: fn,struct,enum,trait,class,interface,type,const)
    #[arg(long, value_delimiter = ',')]
    pub kind: Vec<String>,
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct FindArgs {
    /// Symbol name (substring by default)
    pub name: String,
    /// Require exact match
    #[arg(long)]
    pub exact: bool,
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct StatsArgs {
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub json: bool,
}
```

- [ ] **Step 2: Write `src/commands/mod.rs`**

```rust
pub mod files;
pub mod find;
pub mod stats;
pub mod symbols;
pub mod tree;
```

- [ ] **Step 3: Write stub implementations**

`src/commands/files.rs`:

```rust
use crate::cli::FilesArgs;

pub fn run(_args: FilesArgs) -> anyhow::Result<()> {
    anyhow::bail!("files: not implemented")
}
```

`src/commands/tree.rs`:

```rust
use crate::cli::TreeArgs;

pub fn run(_args: TreeArgs) -> anyhow::Result<()> {
    anyhow::bail!("tree: not implemented")
}
```

`src/commands/symbols.rs`:

```rust
use crate::cli::SymbolsArgs;

pub fn run(_args: SymbolsArgs) -> anyhow::Result<()> {
    anyhow::bail!("symbols: not implemented")
}
```

`src/commands/find.rs`:

```rust
use crate::cli::FindArgs;

pub fn run(_args: FindArgs) -> anyhow::Result<()> {
    anyhow::bail!("find: not implemented")
}
```

`src/commands/stats.rs`:

```rust
use crate::cli::StatsArgs;

pub fn run(_args: StatsArgs) -> anyhow::Result<()> {
    anyhow::bail!("stats: not implemented")
}
```

- [ ] **Step 4: Rewrite `src/main.rs` to dispatch**

```rust
mod cli;
mod commands;

use clap::Parser;
use cli::{Cli, Command};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Files(a) => commands::files::run(a),
        Command::Tree(a) => commands::tree::run(a),
        Command::Symbols(a) => commands::symbols::run(a),
        Command::Find(a) => commands::find::run(a),
        Command::Stats(a) => commands::stats::run(a),
    }
}
```

- [ ] **Step 5: Verify build + help text**

Run: `cargo build -p codemap && ./target/debug/codemap --help`
Expected: usage text listing all 5 subcommands.

- [ ] **Step 6: Commit**

```bash
git add skills/codemap/src/
git commit -m "feat(codemap): wire clap CLI with subcommand stubs"
```

---

## Phase 3 — `files` + `tree` (no tree-sitter)

### Task 4: File walker + language detection + JSON envelope helper

**Files:**
- Create: `skills/codemap/src/walk.rs`
- Create: `skills/codemap/src/lang.rs`
- Create: `skills/codemap/src/output.rs`
- Modify: `skills/codemap/src/main.rs` (add `mod walk; mod lang; mod output;`)

- [ ] **Step 1: Write `src/lang.rs`** (registry stub — extensions only; tree-sitter wiring lands in Task 7)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    TypeScript,
    Tsx,
    JavaScript,
    Python,
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
}
```

- [ ] **Step 2: Write `src/walk.rs`** (exposes one shared walker so `commands/tree.rs` can reuse the same exclusion list — single source of truth)

```rust
use crate::lang::Language;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

pub const IGNORED_DIRS: &[&str] = &["target", "node_modules", ".git", "dist", "build"];

pub struct SourceFile {
    pub path: PathBuf,
    pub language: Language,
}

/// Pre-configured walker honouring `.gitignore` and skipping `IGNORED_DIRS`.
/// Both `walk_sources` and `commands/tree.rs` build on top of this so the
/// exclusion list lives in one place.
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

/// Walk `root`, returning files whose extension maps to a recognised `Language`.
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

- [ ] **Step 3: Write `src/output.rs`** (uniform v1 JSON envelope so downstream agent prompts see a stable shape and we can evolve via `schema_version`)

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
    let env = Envelope { schema_version: 1, data };
    println!("{}", serde_json::to_string(&env)?);
    Ok(())
}
```

- [ ] **Step 4: Register modules in `src/main.rs`**

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
        Command::Files(a) => commands::files::run(a),
        Command::Tree(a) => commands::tree::run(a),
        Command::Symbols(a) => commands::symbols::run(a),
        Command::Find(a) => commands::find::run(a),
        Command::Stats(a) => commands::stats::run(a),
    }
}
```

- [ ] **Step 5: Verify build**

Run: `cargo build -p codemap`
Expected: compiles cleanly. `output::print_json` and `walk::*` may be flagged as dead code — that's fine; they're consumed starting Task 5.

- [ ] **Step 6: Commit**

```bash
git add skills/codemap/src/walk.rs skills/codemap/src/lang.rs skills/codemap/src/output.rs skills/codemap/src/main.rs
git commit -m "feat(codemap): add walker, language registry, json envelope"
```

---

### Task 5: `files` command + test fixtures

**Files:**
- Modify: `skills/codemap/src/commands/files.rs`
- Create: `skills/codemap/tests/fixtures/sample_project/src/lib.rs`
- Create: `skills/codemap/tests/fixtures/sample_project/src/types.ts`
- Create: `skills/codemap/tests/fixtures/sample_project/src/component.tsx`
- Create: `skills/codemap/tests/fixtures/sample_project/src/util.js`
- Create: `skills/codemap/tests/fixtures/sample_project/app.py`
- Create: `skills/codemap/tests/files_test.rs`

- [ ] **Step 1: Create fixture files**

`tests/fixtures/sample_project/src/lib.rs`:

```rust
pub struct Greeter {
    pub name: String,
}

impl Greeter {
    pub fn new(name: &str) -> Self {
        Self { name: name.into() }
    }

    pub fn greet(&self) -> String {
        format!("hello, {}", self.name)
    }
}

pub enum Mood {
    Happy,
    Sad,
}

pub trait Speak {
    fn speak(&self) -> String;
}

pub type Result<T> = std::result::Result<T, String>;

pub const VERSION: &str = "0.1.0";
```

`tests/fixtures/sample_project/src/types.ts`:

```typescript
export interface User {
  id: string;
  name: string;
}

export type Status = "active" | "inactive";

export class UserRepo {
  list(): User[] {
    return [];
  }
}

export function findUser(id: string): User | undefined {
  return undefined;
}
```

`tests/fixtures/sample_project/src/component.tsx`:

```tsx
export interface Props {
  title: string;
}

export function Header(props: Props) {
  return null as any;
}
```

`tests/fixtures/sample_project/src/util.js`:

```javascript
export function add(a, b) {
  return a + b;
}

export class Counter {
  constructor() {
    this.count = 0;
  }
}
```

`tests/fixtures/sample_project/app.py`:

```python
class Cat:
    def __init__(self, name):
        self.name = name

    def meow(self):
        return "meow"


def main():
    print(Cat("nyan").meow())
```

- [ ] **Step 2: Write failing test `tests/files_test.rs`**

```rust
use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_codemap"))
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_project")
}

#[test]
fn files_json_lists_all_supported_extensions() {
    let out = Command::new(bin())
        .args(["files", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run codemap");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid json");
    assert_eq!(v["schema_version"].as_u64().unwrap(), 1);
    let arr = v["data"].as_array().expect("data array");
    let paths: Vec<String> = arr
        .iter()
        .map(|e| e["path"].as_str().unwrap().to_string())
        .collect();
    assert!(paths.iter().any(|p| p.ends_with("src/lib.rs")));
    assert!(paths.iter().any(|p| p.ends_with("src/types.ts")));
    assert!(paths.iter().any(|p| p.ends_with("src/component.tsx")));
    assert!(paths.iter().any(|p| p.ends_with("src/util.js")));
    assert!(paths.iter().any(|p| p.ends_with("app.py")));
    let langs: std::collections::HashSet<&str> =
        arr.iter().map(|e| e["language"].as_str().unwrap()).collect();
    for expected in ["rust", "typescript", "tsx", "javascript", "python"] {
        assert!(langs.contains(expected), "missing language {expected}");
    }
}

#[test]
fn files_human_groups_by_language() {
    let out = Command::new(bin())
        .args(["files", "--path"])
        .arg(fixture())
        .output()
        .expect("run codemap");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("rust"));
    assert!(text.contains("python"));
    assert!(text.contains("typescript"));
}
```

- [ ] **Step 3: Run (expect failure)**

Run: `cargo test -p codemap --test files_test`
Expected: FAIL — `files: not implemented`.

- [ ] **Step 4: Implement `commands/files.rs`**

```rust
use crate::cli::FilesArgs;
use crate::output::print_json;
use crate::walk::walk_sources;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;

#[derive(Serialize)]
struct FileEntry {
    path: String,
    language: &'static str,
    lines: usize,
    size_bytes: u64,
}

pub fn run(args: FilesArgs) -> anyhow::Result<()> {
    let files = walk_sources(&args.path)?;
    let mut entries = Vec::with_capacity(files.len());
    for f in &files {
        let rel = f
            .path
            .strip_prefix(&args.path)
            .unwrap_or(&f.path)
            .to_string_lossy()
            .into_owned();
        let meta = fs::metadata(&f.path)?;
        let lines = fs::read_to_string(&f.path)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        entries.push(FileEntry {
            path: rel,
            language: f.language.name(),
            lines,
            size_bytes: meta.len(),
        });
    }

    if args.json {
        print_json(&entries)?;
    } else {
        let mut by_lang: BTreeMap<&'static str, Vec<&FileEntry>> = BTreeMap::new();
        for e in &entries {
            by_lang.entry(e.language).or_default().push(e);
        }
        for (lang, group) in by_lang {
            println!("{lang} ({} files)", group.len());
            for e in group {
                println!("  {}  {} lines  {} B", e.path, e.lines, e.size_bytes);
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Run (expect pass)**

Run: `cargo test -p codemap --test files_test`
Expected: 2 passed.

- [ ] **Step 6: Commit**

```bash
git add skills/codemap/src/commands/files.rs skills/codemap/tests/
git commit -m "feat(codemap): implement files subcommand + fixture project"
```

---

### Task 6: `tree` command

**Files:**
- Modify: `skills/codemap/src/commands/tree.rs`
- Modify: `skills/codemap/tests/files_test.rs` (append a test)

- [ ] **Step 1: Implement `commands/tree.rs`** (reuses `walk::default_walker` so the exclusion list lives in one place)

```rust
use crate::cli::TreeArgs;
use crate::output::print_json;
use crate::walk::default_walker;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
struct Node {
    name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<Node>,
    is_dir: bool,
}

pub fn run(args: TreeArgs) -> anyhow::Result<()> {
    let root = args.path.canonicalize().unwrap_or(args.path.clone());
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in default_walker(&root).build() {
        let entry = entry?;
        let path = entry.into_path();
        if path == root {
            continue;
        }
        paths.push(path);
    }
    paths.sort();

    if args.json {
        let tree = build_tree(&root, &paths);
        print_json(&tree)?;
    } else {
        print_human(&root, &paths);
    }
    Ok(())
}

fn build_tree(root: &Path, paths: &[PathBuf]) -> Node {
    let mut root_node = Node {
        name: root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".into()),
        children: Vec::new(),
        is_dir: true,
    };
    for path in paths {
        let rel = path.strip_prefix(root).unwrap();
        let parts: Vec<&str> = rel.iter().filter_map(|c| c.to_str()).collect();
        insert(&mut root_node, &parts, path.is_dir());
    }
    root_node
}

fn insert(node: &mut Node, parts: &[&str], is_dir: bool) {
    if parts.is_empty() {
        return;
    }
    let head = parts[0];
    let existing = node.children.iter_mut().position(|c| c.name == head);
    let idx = match existing {
        Some(i) => i,
        None => {
            node.children.push(Node {
                name: head.to_string(),
                children: Vec::new(),
                is_dir: parts.len() > 1 || is_dir,
            });
            node.children.len() - 1
        }
    };
    insert(&mut node.children[idx], &parts[1..], is_dir);
}

fn print_human(root: &Path, paths: &[PathBuf]) {
    let display_root = root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".".into());
    println!("{}", display_root);
    let mut last_parts: Vec<String> = Vec::new();
    for path in paths {
        let rel = path.strip_prefix(root).unwrap();
        let parts: Vec<String> = rel
            .iter()
            .map(|c| c.to_string_lossy().into_owned())
            .collect();
        for (i, part) in parts.iter().enumerate() {
            if last_parts.get(i) == Some(part) {
                continue;
            }
            let indent = "  ".repeat(i + 1);
            println!("{indent}{part}");
        }
        last_parts = parts;
    }
}
```

- [ ] **Step 2: Append a test to `tests/files_test.rs`**

```rust
#[test]
fn tree_json_returns_nested_structure() {
    let out = Command::new(bin())
        .args(["tree", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run codemap");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid json");
    assert_eq!(v["schema_version"].as_u64().unwrap(), 1);
    let tree = &v["data"];
    assert!(tree["is_dir"].as_bool().unwrap_or(false));
    let children = tree["children"].as_array().expect("children array");
    let names: Vec<&str> = children.iter().filter_map(|c| c["name"].as_str()).collect();
    assert!(names.contains(&"src"));
    assert!(names.contains(&"app.py"));
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p codemap`
Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add skills/codemap/src/commands/tree.rs skills/codemap/tests/files_test.rs
git commit -m "feat(codemap): implement tree subcommand"
```

---

## Phase 4 — Symbols Engine (Rust first)

### Task 7: Language registry with tree-sitter + Symbol model

**Files:**
- Modify: `skills/codemap/src/lang.rs`
- Create: `skills/codemap/src/symbols.rs`
- Create: `skills/codemap/src/queries/rust.scm`
- Modify: `skills/codemap/src/main.rs` (add `mod symbols;`)

- [ ] **Step 1: Write `src/queries/rust.scm`**

```scheme
(function_item
  name: (identifier) @name) @symbol.fn

(struct_item
  name: (type_identifier) @name) @symbol.struct

(enum_item
  name: (type_identifier) @name) @symbol.enum

(trait_item
  name: (type_identifier) @name) @symbol.trait

(type_item
  name: (type_identifier) @name) @symbol.type

(const_item
  name: (identifier) @name) @symbol.const

(static_item
  name: (identifier) @name) @symbol.const
```

> **Convention:** the parent node uses `@symbol.<kind>` and the identifier uses `@name`. The extractor reads the capture name suffix to derive `SymbolKind`.

- [ ] **Step 2: Extend `src/lang.rs`** — `query_source` returns `Option<&'static str>` so "language not yet supported" is distinguishable from "no symbols matched" at the type level.

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

    /// Returns `None` for languages whose symbol query hasn't been wired up yet.
    /// `Some("")` is intentionally not a valid state.
    pub fn query_source(self) -> Option<&'static str> {
        match self {
            Language::Rust => Some(include_str!("queries/rust.scm")),
            // The remaining branches are wired up in Tasks 9–11.
            Language::TypeScript | Language::Tsx => None,
            Language::JavaScript => None,
            Language::Python => None,
        }
    }
}
```

- [ ] **Step 3: Write `src/symbols.rs`**

```rust
use crate::lang::Language;
use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::path::Path;
use tree_sitter::{Parser, Query, QueryCursor};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    Fn,
    Struct,
    Enum,
    Trait,
    Class,
    Interface,
    Type,
    Const,
}

impl SymbolKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "fn" | "function" => Some(SymbolKind::Fn),
            "struct" => Some(SymbolKind::Struct),
            "enum" => Some(SymbolKind::Enum),
            "trait" => Some(SymbolKind::Trait),
            "class" => Some(SymbolKind::Class),
            "interface" => Some(SymbolKind::Interface),
            "type" => Some(SymbolKind::Type),
            "const" => Some(SymbolKind::Const),
            _ => None,
        }
    }

    fn from_capture_suffix(s: &str) -> Option<Self> {
        let suffix = s.strip_prefix("symbol.")?;
        Self::parse(suffix)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Symbol {
    pub file: String,
    pub name: String,
    pub kind: SymbolKind,
    pub start_line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Extract symbols from one file. `rel_path` is stored on each `Symbol.file`.
/// Returns an empty vec when the language has no wired-up query (= unsupported here, not just no matches).
pub fn extract_file(path: &Path, rel_path: &str, language: Language) -> Result<Vec<Symbol>> {
    let Some(query_src) = language.query_source() else {
        return Ok(Vec::new());
    };
    let source = fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;
    let mut parser = Parser::new();
    parser
        .set_language(&language.ts_language())
        .with_context(|| format!("set language {}", language.name()))?;
    let tree = parser
        .parse(&source, None)
        .with_context(|| format!("parse {}", path.display()))?;
    let query = Query::new(&language.ts_language(), query_src)
        .with_context(|| format!("compile query for {}", language.name()))?;
    let capture_names = query.capture_names();

    let mut cursor = QueryCursor::new();
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    for m in cursor.matches(&query, tree.root_node(), bytes) {
        let mut symbol_node = None;
        let mut symbol_kind = None;
        let mut name = None;
        for cap in m.captures {
            let cname = capture_names[cap.index as usize];
            if cname == "name" {
                name = Some(cap.node.utf8_text(bytes).unwrap_or("").to_string());
            } else if let Some(k) = SymbolKind::from_capture_suffix(cname) {
                symbol_node = Some(cap.node);
                symbol_kind = Some(k);
            }
        }
        let (Some(n), Some(node), Some(kind)) = (name, symbol_node, symbol_kind) else {
            continue;
        };
        let start_line = node.start_position().row + 1;
        let end_line = node.end_position().row + 1;
        let signature = first_line_of(node, bytes).map(|s| truncate(s, 120));
        out.push(Symbol {
            file: rel_path.to_string(),
            name: n,
            kind,
            start_line,
            end_line,
            signature,
        });
    }
    out.sort_by(|a, b| a.start_line.cmp(&b.start_line));
    Ok(out)
}

fn first_line_of(node: tree_sitter::Node<'_>, src: &[u8]) -> Option<String> {
    let text = node.utf8_text(src).ok()?;
    Some(text.lines().next()?.trim().to_string())
}

fn truncate(mut s: String, max: usize) -> String {
    if s.chars().count() > max {
        s = s.chars().take(max).collect::<String>() + "…";
    }
    s
}
```

- [ ] **Step 4: Register `mod symbols;` in `src/main.rs`**

```rust
mod cli;
mod commands;
mod lang;
mod output;
mod symbols;
mod walk;

use clap::Parser;
use cli::{Cli, Command};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Files(a) => commands::files::run(a),
        Command::Tree(a) => commands::tree::run(a),
        Command::Symbols(a) => commands::symbols::run(a),
        Command::Find(a) => commands::find::run(a),
        Command::Stats(a) => commands::stats::run(a),
    }
}
```

- [ ] **Step 5: Verify build**

Run: `cargo build -p codemap`
Expected: compiles cleanly.

- [ ] **Step 6: Commit**

```bash
git add skills/codemap/src/lang.rs skills/codemap/src/symbols.rs skills/codemap/src/queries/rust.scm skills/codemap/src/main.rs
git commit -m "feat(codemap): add tree-sitter symbol extractor + Rust query"
```

---

### Task 8: `symbols <FILE>` (single-file mode, Rust)

**Files:**
- Modify: `skills/codemap/src/commands/symbols.rs`
- Create: `skills/codemap/tests/symbols_test.rs`

- [ ] **Step 1: Write failing test `tests/symbols_test.rs`**

```rust
use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_codemap"))
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_project")
}

#[test]
fn symbols_rust_single_file() {
    let file = fixture().join("src/lib.rs");
    let out = Command::new(bin())
        .args(["symbols", "--json"])
        .arg(&file)
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["schema_version"].as_u64().unwrap(), 1);
    let arr = v["data"].as_array().expect("data array");
    let mut names: Vec<(String, String)> = arr
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap().to_string(),
                s["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    names.sort();
    assert!(names.contains(&("Greeter".into(), "struct".into())));
    assert!(names.contains(&("Mood".into(), "enum".into())));
    assert!(names.contains(&("Speak".into(), "trait".into())));
    assert!(names.contains(&("Result".into(), "type".into())));
    assert!(names.contains(&("VERSION".into(), "const".into())));
    assert!(names.iter().any(|(n, k)| n == "greet" && k == "fn"));
}

#[test]
fn symbols_kind_filter_keeps_only_requested() {
    let file = fixture().join("src/lib.rs");
    let out = Command::new(bin())
        .args(["symbols", "--json", "--kind", "struct,enum"])
        .arg(&file)
        .output()
        .expect("run");
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    for s in v["data"].as_array().unwrap() {
        let k = s["kind"].as_str().unwrap();
        assert!(matches!(k, "struct" | "enum"), "unexpected kind {k}");
    }
}
```

- [ ] **Step 2: Run (expect failure)**

Run: `cargo test -p codemap --test symbols_test`
Expected: FAIL — `symbols: not implemented`.

- [ ] **Step 3: Implement `commands/symbols.rs`** (single-file mode now resolves `target` against `--path`, eliminating the ambiguity flagged in review)

```rust
use crate::cli::SymbolsArgs;
use crate::lang::Language;
use crate::output::print_json;
use crate::symbols::{extract_file, Symbol, SymbolKind};
use crate::walk::walk_sources;
use anyhow::{anyhow, Result};
use std::path::Path;

pub fn run(args: SymbolsArgs) -> Result<()> {
    let filter = parse_kind_filter(&args.kind)?;
    let target = args.target.as_deref().unwrap_or(".");
    let whole_project = args.all || target == ".";

    let symbols = if whole_project {
        let root = &args.path;
        let mut all = Vec::new();
        for f in walk_sources(root)? {
            let rel = relative_to(root, &f.path);
            all.extend(extract_file(&f.path, &rel, f.language)?);
        }
        all
    } else {
        // Resolve `target` against `--path` so `codemap symbols src/lib.rs --path /repo`
        // looks at /repo/src/lib.rs, not CWD/src/lib.rs. Absolute targets pass through.
        let path = args.path.join(target);
        let language = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(Language::from_extension)
            .ok_or_else(|| anyhow!("unsupported file: {}", path.display()))?;
        let rel = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        extract_file(&path, &rel, language)?
    };

    let symbols: Vec<Symbol> = symbols
        .into_iter()
        .filter(|s| filter.is_empty() || filter.contains(&s.kind))
        .collect();

    if args.json {
        print_json(&symbols)?;
    } else {
        for s in &symbols {
            println!(
                "{:>4}-{:<4} {:<9} {}{}",
                s.start_line,
                s.end_line,
                format!("{:?}", s.kind).to_ascii_lowercase(),
                s.name,
                s.signature
                    .as_ref()
                    .map(|sig| format!("    {sig}"))
                    .unwrap_or_default()
            );
        }
    }
    Ok(())
}

fn parse_kind_filter(raw: &[String]) -> Result<Vec<SymbolKind>> {
    let mut out = Vec::new();
    for k in raw {
        out.push(
            SymbolKind::parse(k).ok_or_else(|| anyhow!("unknown --kind value: {k}"))?,
        );
    }
    Ok(out)
}

fn relative_to(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}
```

- [ ] **Step 4: Run (expect pass)**

Run: `cargo test -p codemap --test symbols_test`
Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add skills/codemap/src/commands/symbols.rs skills/codemap/tests/symbols_test.rs
git commit -m "feat(codemap): implement symbols subcommand (Rust)"
```

---

## Phase 5 — Expand Languages

### Task 9: TypeScript + TSX queries

**Files:**
- Create: `skills/codemap/src/queries/typescript.scm`
- Modify: `skills/codemap/src/lang.rs`
- Modify: `skills/codemap/tests/symbols_test.rs`

- [ ] **Step 1: Append failing tests**

```rust
#[test]
fn symbols_typescript_extracts_class_interface_type_fn() {
    let file = fixture().join("src/types.ts");
    let out = Command::new(bin())
        .args(["symbols", "--json"])
        .arg(&file)
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let pairs: Vec<(String, String)> = v["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap().to_string(),
                s["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(pairs.contains(&("User".into(), "interface".into())));
    assert!(pairs.contains(&("Status".into(), "type".into())));
    assert!(pairs.contains(&("UserRepo".into(), "class".into())));
    assert!(pairs.contains(&("findUser".into(), "fn".into())));
}

#[test]
fn symbols_tsx_extracts_function_and_interface() {
    let file = fixture().join("src/component.tsx");
    let out = Command::new(bin())
        .args(["symbols", "--json"])
        .arg(&file)
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let pairs: Vec<(String, String)> = v["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap().to_string(),
                s["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(pairs.contains(&("Props".into(), "interface".into())));
    assert!(pairs.contains(&("Header".into(), "fn".into())));
}
```

- [ ] **Step 2: Run (expect failure)** — unsupported language returns empty.

Run: `cargo test -p codemap --test symbols_test symbols_typescript`
Expected: FAIL.

- [ ] **Step 3: Write `src/queries/typescript.scm`**

```scheme
(function_declaration
  name: (identifier) @name) @symbol.fn

(class_declaration
  name: (type_identifier) @name) @symbol.class

(interface_declaration
  name: (type_identifier) @name) @symbol.interface

(type_alias_declaration
  name: (type_identifier) @name) @symbol.type

(enum_declaration
  name: (identifier) @name) @symbol.enum

(lexical_declaration
  "const"
  (variable_declarator
    name: (identifier) @name)) @symbol.const
```

- [ ] **Step 4: Wire in `lang.rs`** — replace the `None` branch:

```rust
            Language::TypeScript | Language::Tsx => Some(include_str!("queries/typescript.scm")),
```

- [ ] **Step 5: Re-run tests**

Run: `cargo test -p codemap --test symbols_test`
Expected: all TS/TSX tests pass alongside Rust.

- [ ] **Step 6: Commit**

```bash
git add skills/codemap/src/queries/typescript.scm skills/codemap/src/lang.rs skills/codemap/tests/symbols_test.rs
git commit -m "feat(codemap): add TypeScript + TSX symbol queries"
```

---

### Task 10: JavaScript query

**Files:**
- Create: `skills/codemap/src/queries/javascript.scm`
- Modify: `skills/codemap/src/lang.rs`
- Modify: `skills/codemap/tests/symbols_test.rs`

- [ ] **Step 1: Append failing test**

```rust
#[test]
fn symbols_javascript_extracts_function_and_class() {
    let file = fixture().join("src/util.js");
    let out = Command::new(bin())
        .args(["symbols", "--json"])
        .arg(&file)
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let pairs: Vec<(String, String)> = v["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap().to_string(),
                s["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(pairs.contains(&("add".into(), "fn".into())));
    assert!(pairs.contains(&("Counter".into(), "class".into())));
}
```

- [ ] **Step 2: Run (expect failure)**

Run: `cargo test -p codemap --test symbols_test symbols_javascript`
Expected: FAIL.

- [ ] **Step 3: Write `src/queries/javascript.scm`**

```scheme
(function_declaration
  name: (identifier) @name) @symbol.fn

(class_declaration
  name: (identifier) @name) @symbol.class
```

- [ ] **Step 4: Wire in `lang.rs`**

```rust
            Language::JavaScript => Some(include_str!("queries/javascript.scm")),
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p codemap --test symbols_test`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add skills/codemap/src/queries/javascript.scm skills/codemap/src/lang.rs skills/codemap/tests/symbols_test.rs
git commit -m "feat(codemap): add JavaScript symbol query"
```

---

### Task 11: Python query

**Files:**
- Create: `skills/codemap/src/queries/python.scm`
- Modify: `skills/codemap/src/lang.rs`
- Modify: `skills/codemap/tests/symbols_test.rs`

- [ ] **Step 1: Append failing test**

```rust
#[test]
fn symbols_python_extracts_class_and_top_level_fn() {
    let file = fixture().join("app.py");
    let out = Command::new(bin())
        .args(["symbols", "--json"])
        .arg(&file)
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let pairs: Vec<(String, String)> = v["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap().to_string(),
                s["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(pairs.contains(&("Cat".into(), "class".into())));
    assert!(pairs.contains(&("main".into(), "fn".into())));
}
```

- [ ] **Step 2: Run (expect failure)**

Run: `cargo test -p codemap --test symbols_test symbols_python`
Expected: FAIL.

- [ ] **Step 3: Write `src/queries/python.scm`** — `@symbol.fn` / `@symbol.class` sit on the inner definition (not the `decorated_definition` wrapper), so `signature` shows the `def`/`class` line, not the `@decorator` line.

```scheme
(module
  (function_definition
    name: (identifier) @name) @symbol.fn)

(module
  (class_definition
    name: (identifier) @name) @symbol.class)

(module
  (decorated_definition
    definition: (function_definition
      name: (identifier) @name) @symbol.fn))

(module
  (decorated_definition
    definition: (class_definition
      name: (identifier) @name) @symbol.class))
```

- [ ] **Step 4: Wire in `lang.rs`**

```rust
            Language::Python => Some(include_str!("queries/python.scm")),
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p codemap --test symbols_test`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add skills/codemap/src/queries/python.scm skills/codemap/src/lang.rs skills/codemap/tests/symbols_test.rs
git commit -m "feat(codemap): add Python symbol query"
```

---

### Task 12: `symbols .` / `--all` whole-project verification

Implementation was completed in Task 8; this task adds an end-to-end test across all languages.

**Files:**
- Modify: `skills/codemap/tests/symbols_test.rs`

- [ ] **Step 1: Append test**

```rust
#[test]
fn symbols_whole_project_aggregates_all_languages() {
    let out = Command::new(bin())
        .args(["symbols", ".", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let files: std::collections::HashSet<String> = v["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["file"].as_str().unwrap().to_string())
        .collect();
    assert!(files.iter().any(|f| f.ends_with("lib.rs")));
    assert!(files.iter().any(|f| f.ends_with("types.ts")));
    assert!(files.iter().any(|f| f.ends_with("component.tsx")));
    assert!(files.iter().any(|f| f.ends_with("util.js")));
    assert!(files.iter().any(|f| f.ends_with("app.py")));
}
```

- [ ] **Step 2: Run**

Run: `cargo test -p codemap --test symbols_test symbols_whole_project`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add skills/codemap/tests/symbols_test.rs
git commit -m "test(codemap): cover whole-project symbols aggregation"
```

---

## Phase 6 — `find` + `stats`

### Task 13: `find` command

**Files:**
- Modify: `skills/codemap/src/commands/find.rs`
- Create: `skills/codemap/tests/find_test.rs`

- [ ] **Step 1: Write failing test `tests/find_test.rs`**

```rust
use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_codemap")) }
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_project")
}

#[test]
fn find_substring_matches_across_languages() {
    let out = Command::new(bin())
        .args(["find", "User", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["schema_version"].as_u64().unwrap(), 1);
    let names: Vec<&str> = v["data"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"User"));
    assert!(names.contains(&"UserRepo"));
    assert!(names.iter().any(|n| *n == "findUser"));
}

#[test]
fn find_exact_only_returns_exact_name() {
    let out = Command::new(bin())
        .args(["find", "User", "--exact", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let arr = v["data"].as_array().unwrap();
    for e in arr {
        assert_eq!(e["name"].as_str().unwrap(), "User");
    }
    assert!(!arr.is_empty());
}
```

- [ ] **Step 2: Run (expect failure)**

Run: `cargo test -p codemap --test find_test`
Expected: FAIL — `find: not implemented`.

- [ ] **Step 3: Implement `commands/find.rs`**

```rust
use crate::cli::FindArgs;
use crate::output::print_json;
use crate::symbols::{extract_file, Symbol};
use crate::walk::walk_sources;
use anyhow::Result;

pub fn run(args: FindArgs) -> Result<()> {
    let mut hits: Vec<Symbol> = Vec::new();
    for f in walk_sources(&args.path)? {
        let rel = f
            .path
            .strip_prefix(&args.path)
            .unwrap_or(&f.path)
            .to_string_lossy()
            .into_owned();
        for s in extract_file(&f.path, &rel, f.language)? {
            let matches = if args.exact {
                s.name == args.name
            } else {
                s.name.contains(&args.name)
            };
            if matches {
                hits.push(s);
            }
        }
    }
    hits.sort_by(|a, b| a.file.cmp(&b.file).then(a.start_line.cmp(&b.start_line)));
    if args.json {
        print_json(&hits)?;
    } else {
        for s in &hits {
            println!(
                "{}:{}  {}  {}",
                s.file,
                s.start_line,
                format!("{:?}", s.kind).to_ascii_lowercase(),
                s.name
            );
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p codemap --test find_test`
Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add skills/codemap/src/commands/find.rs skills/codemap/tests/find_test.rs
git commit -m "feat(codemap): implement find subcommand"
```

---

### Task 14: `stats` command

**Files:**
- Modify: `skills/codemap/src/commands/stats.rs`
- Create: `skills/codemap/tests/stats_test.rs`

- [ ] **Step 1: Write failing test `tests/stats_test.rs`**

```rust
use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_codemap")) }
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_project")
}

#[test]
fn stats_json_returns_per_language_and_per_kind() {
    let out = Command::new(bin())
        .args(["stats", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["schema_version"].as_u64().unwrap(), 1);
    let d = &v["data"];
    assert!(d["total_files"].as_u64().unwrap() >= 5);
    assert!(d["total_lines"].as_u64().unwrap() > 0);
    assert!(d["languages"]["rust"]["files"].as_u64().unwrap() >= 1);
    assert!(d["languages"]["python"]["files"].as_u64().unwrap() >= 1);
    assert!(d["symbols"]["fn"].as_u64().unwrap() >= 1);
    assert!(d["symbols"]["struct"].as_u64().unwrap() >= 1);
    assert!(d["symbols"]["class"].as_u64().unwrap() >= 1);
    assert!(d["symbols"]["interface"].as_u64().unwrap() >= 1);
}
```

- [ ] **Step 2: Run (expect failure)**

Run: `cargo test -p codemap --test stats_test`
Expected: FAIL — `stats: not implemented`.

- [ ] **Step 3: Implement `commands/stats.rs`**

```rust
use crate::cli::StatsArgs;
use crate::output::print_json;
use crate::symbols::{extract_file, SymbolKind};
use crate::walk::walk_sources;
use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;

#[derive(Serialize, Default)]
struct LangStats {
    files: usize,
    lines: usize,
}

#[derive(Serialize)]
struct Report {
    total_files: usize,
    total_lines: usize,
    languages: BTreeMap<&'static str, LangStats>,
    symbols: BTreeMap<&'static str, usize>,
}

pub fn run(args: StatsArgs) -> Result<()> {
    let files = walk_sources(&args.path)?;
    let mut total_files = 0usize;
    let mut total_lines = 0usize;
    let mut languages: BTreeMap<&'static str, LangStats> = BTreeMap::new();
    let mut symbols: BTreeMap<&'static str, usize> = BTreeMap::new();

    for f in &files {
        total_files += 1;
        let lines = fs::read_to_string(&f.path).map(|s| s.lines().count()).unwrap_or(0);
        total_lines += lines;
        let entry = languages.entry(f.language.name()).or_default();
        entry.files += 1;
        entry.lines += lines;

        let rel = f
            .path
            .strip_prefix(&args.path)
            .unwrap_or(&f.path)
            .to_string_lossy()
            .into_owned();
        for s in extract_file(&f.path, &rel, f.language)? {
            let key: &'static str = match s.kind {
                SymbolKind::Fn => "fn",
                SymbolKind::Struct => "struct",
                SymbolKind::Enum => "enum",
                SymbolKind::Trait => "trait",
                SymbolKind::Class => "class",
                SymbolKind::Interface => "interface",
                SymbolKind::Type => "type",
                SymbolKind::Const => "const",
            };
            *symbols.entry(key).or_insert(0) += 1;
        }
    }

    let report = Report { total_files, total_lines, languages, symbols };
    if args.json {
        print_json(&report)?;
    } else {
        println!("files: {}", report.total_files);
        println!("lines: {}", report.total_lines);
        println!("languages:");
        for (lang, ls) in &report.languages {
            println!("  {lang:<12} {} files  {} lines", ls.files, ls.lines);
        }
        println!("symbols:");
        for (k, n) in &report.symbols {
            println!("  {k:<10} {n}");
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p codemap --test stats_test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add skills/codemap/src/commands/stats.rs skills/codemap/tests/stats_test.rs
git commit -m "feat(codemap): implement stats subcommand"
```

---

## Phase 7 — Skill Manifest + Docs + CI

### Task 15: Write `SKILL.md`

**Files:**
- Create: `skills/codemap/SKILL.md`

- [ ] **Step 1: Write the manifest**

````markdown
---
name: codemap
description: Use when exploring an unfamiliar codebase, surveying project structure, or locating definitions before editing — `codemap` lists source files, extracts top-level symbols (fn/struct/class/interface/type/enum/trait/const), finds a symbol by name across the project, and reports per-language stats. Trigger on phrases like "what's in this repo", "where is X defined", "show me the structure", "หาฟังก์ชัน", "โครงสร้าง project", "มีไฟล์อะไรบ้าง", "อยู่ไฟล์ไหน". Supports Rust, TypeScript, TSX, JavaScript, Python.
---

# codemap

`codemap` is a CLI in the `skills` monorepo. It uses tree-sitter to parse a project and answer "what's here?" questions without you reading every file.

## When to use

- You just landed in an unfamiliar repo and need orientation.
- The user asks where a function/struct/class is defined.
- You're about to refactor and want to know all top-level symbols in scope.
- You want a fast, structured map before deciding where to edit.

Prefer `codemap` over ad-hoc `grep`/`find` when you need **structured** information (kind + line range, not just substring match).

## Build

```bash
cargo build --release -p codemap
```

Binary lands at `target/release/codemap` (relative to the workspace root).

## Subcommands

All subcommands accept `--path <DIR>` (default `.`) and `--json` (default human-readable). Every JSON response is wrapped in `{"schema_version": 1, "data": ...}`.

| Subcommand | Purpose |
| --- | --- |
| `codemap files` | List supported source files grouped by language |
| `codemap tree` | Print project directory tree (respects `.gitignore`) |
| `codemap symbols <FILE>` | Top-level symbols in one file (path resolved against `--path`) |
| `codemap symbols . [--all]` | Top-level symbols across the whole project |
| `codemap symbols --kind fn,struct` | Filter by kind (`fn`, `struct`, `enum`, `trait`, `class`, `interface`, `type`, `const`) |
| `codemap find <NAME> [--exact]` | Locate a symbol by name; substring by default |
| `codemap stats` | Per-language file/line counts + symbol totals by kind |

## Usage tips for agents

- Always pass `--json` when you intend to parse the output programmatically — the human format is for end users.
- Read `result.data` from the envelope, and consider asserting `result.schema_version === 1` so future schema changes surface loudly.
- Start with `codemap stats` for a one-screen overview, then drill in.
- Use `codemap find <name> --exact --json` when verifying a memory before recommending an edit (memories can be stale).
- `--path` is the project root; `<FILE>` in `symbols` is resolved against it — you don't have to `cd`.

## Examples

```bash
codemap files --json --path ./my-repo
codemap symbols src/lib.rs --json --kind fn,struct --path ./my-repo
codemap find UserRepo --exact --json
codemap stats
```

## Supported languages

Rust, TypeScript, TSX, JavaScript, Python. Adding a new language is a single-file change: drop a `.scm` query into `skills/codemap/src/queries/`, register it in `src/lang.rs`, add an extension mapping.
````

- [ ] **Step 2: Commit**

```bash
git add skills/codemap/SKILL.md
git commit -m "docs(codemap): add SKILL.md manifest"
```

---

### Task 16: codemap README + confirm root index

**Files:**
- Create: `skills/codemap/README.md`

- [ ] **Step 1: Write `skills/codemap/README.md`**

````markdown
# codemap

A small, fast CLI that uses [tree-sitter](https://tree-sitter.github.io/) to answer "what's in this codebase?" without making you open every file.

Part of the [`skills`](../../README.md) monorepo.

## Install

```bash
cargo build --release -p codemap
# binary at ../../target/release/codemap (relative to this crate)
```

Or, from the workspace root:

```bash
cargo install --path skills/codemap
```

## JSON envelope

Every JSON response is wrapped in a stable v1 envelope:

```json
{ "schema_version": 1, "data": <payload> }
```

Future schema changes will bump `schema_version`. Read `data` and assert the version.

## Commands

### `codemap files [--json] [--path DIR]`

List source files grouped by language.

```
$ codemap files
python (1 files)
  app.py  10 lines  123 B
rust (1 files)
  src/lib.rs  21 lines  345 B
typescript (1 files)
  src/types.ts  15 lines  220 B
```

JSON `data` shape:

```json
[
  { "path": "src/lib.rs", "language": "rust", "lines": 21, "size_bytes": 345 }
]
```

### `codemap tree [--json] [--path DIR]`

Directory outline honouring `.gitignore` and skipping `target/`, `node_modules/`, `.git/`, `dist/`, `build/`.

### `codemap symbols <FILE | . | --all> [--kind ...] [--json] [--path DIR]`

Extract top-level symbols. Pass a file (resolved against `--path`) for single-file mode, or `.` / `--all` for whole-project.

Filter with `--kind fn,struct,...`. Supported kinds: `fn`, `struct`, `enum`, `trait`, `class`, `interface`, `type`, `const`.

JSON `data` element:

```json
{
  "file": "src/lib.rs",
  "name": "Greeter",
  "kind": "struct",
  "start_line": 1,
  "end_line": 3,
  "signature": "pub struct Greeter {"
}
```

### `codemap find <NAME> [--exact] [--json] [--path DIR]`

Find a symbol by name across the project. Substring match by default; `--exact` for equality.

### `codemap stats [--json] [--path DIR]`

JSON `data` shape:

```json
{
  "total_files": 5,
  "total_lines": 71,
  "languages": { "rust": { "files": 1, "lines": 21 }, "python": { "files": 1, "lines": 10 } },
  "symbols": { "fn": 4, "struct": 1, "class": 2, "interface": 2 }
}
```

## Supported languages

| Language | Extensions | Kinds extracted |
| --- | --- | --- |
| Rust | `.rs` | fn, struct, enum, trait, type, const, static |
| TypeScript | `.ts` | fn, class, interface, type, enum, const |
| TSX | `.tsx` | fn, class, interface, type, enum, const |
| JavaScript | `.js`, `.mjs`, `.cjs` | fn, class |
| Python | `.py` | fn (module-level), class |

## Adding a new language

1. Add the grammar crate to `Cargo.toml` (e.g. `tree-sitter-go = "0.21"`).
2. Add an extension mapping in `src/lang.rs` (`from_extension` + `ts_language`).
3. Write a `src/queries/<lang>.scm` using the `@symbol.<kind>` / `@name` convention.
4. Wire `Language::query_source` to `Some(include_str!("queries/<lang>.scm"))`.
5. Add a fixture under `tests/fixtures/sample_project/` and a test in `tests/symbols_test.rs`.

## License

MIT — see [LICENSE](../../LICENSE).
````

- [ ] **Step 2: Confirm root README index row**

Run: `grep -F 'skills/codemap' /Users/detoro/code/skills/README.md`
Expected: a hit — the row added in Task 1.

- [ ] **Step 3: Commit**

```bash
git add skills/codemap/README.md
git commit -m "docs(codemap): add crate README"
```

---

### Task 17: CI workflow

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Write `.github/workflows/ci.yml`** — all cargo invocations use `--locked` so CI fails fast if `Cargo.lock` drifts.

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  test:
    name: ${{ matrix.os }} / stable
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest]
    steps:
      - uses: actions/checkout@v4
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: Swatinem/rust-cache@v2
      - name: Format
        run: cargo fmt --all -- --check
      - name: Clippy
        run: cargo clippy --workspace --all-targets --locked -- -D warnings
      - name: Test
        run: cargo test --workspace --all-targets --locked
```

- [ ] **Step 2: Verify YAML is syntactically valid (best effort)**

Run: `python3 -c "import yaml; yaml.safe_load(open('/Users/detoro/code/skills/.github/workflows/ci.yml'))" && echo OK`
Expected: `OK`. If `pyyaml` is unavailable, skip and trust CI.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add fmt + clippy + test workflow (ubuntu + macos, --locked)"
```

---

## Phase 8 — Quality Gate

### Task 18: Full workspace lint + test sweep

**Files:** none (verification only)

- [ ] **Step 1: Format check**

Run: `cd /Users/detoro/code/skills && cargo fmt --all -- --check`
Expected: no output, exit 0. (`rustfmt.toml` excludes `tests/fixtures/**`.) If it fails, run `cargo fmt --all` and commit as `style:`.

- [ ] **Step 2: Clippy with `-D warnings` and `--locked`**

Run: `cargo clippy --workspace --all-targets --locked -- -D warnings`
Expected: exit 0. Use `#[allow(...)]` only for genuine false positives with a one-line justification.

- [ ] **Step 3: Test sweep**

Run: `cargo test --workspace --all-targets --locked`
Expected: all tests pass.

- [ ] **Step 4: Smoke-test the release binary**

```bash
cd /Users/detoro/code/skills
cargo build --release -p codemap --locked
./target/release/codemap files --path skills/codemap/tests/fixtures/sample_project
./target/release/codemap stats --json --path skills/codemap/tests/fixtures/sample_project
./target/release/codemap find Greeter --exact --json --path skills/codemap/tests/fixtures/sample_project
```

Expected: every JSON output starts with `{"schema_version":1,"data":` — confirms envelope. No panics, exit 0.

- [ ] **Step 5: If any of the above produced fixes, commit**

```bash
git add -A
git commit -m "chore: lint sweep + final touch-ups"
```

- [ ] **Step 6: Final summary to the user**

Print:
- Workspace builds clean on stable Rust (pinned via `rust-toolchain.toml`).
- All 5 subcommands implemented + tested across Rust, TS, TSX, JS, Python.
- JSON output uses stable `schema_version:1` envelope across the board.
- CI configured for ubuntu + macos with `--locked`.
- `SKILL.md` + READMEs in place.
- Quick start: `cargo build --release -p codemap && ./target/release/codemap --help`.

---

## Self-Review Notes

- **Spec coverage:** every spec bullet maps to a task — workspace `[workspace.dependencies]` / `[workspace.package]` (Task 1), `skills/codemap/` member (Task 2), CLI (Task 3), walker + envelope (Task 4), all 5 subcommands (Tasks 5, 6, 8, 13, 14), 5 languages via tree-sitter (Tasks 7, 9–11), JSON + human modes throughout, bilingual SKILL.md (Task 15), READMEs + tests + CI (Tasks 16–17), final fmt/clippy/test gate (Task 18).
- **Reviewer feedback applied:**
  - BLOCKER (rustfmt fixture clobber) — fixed by `rustfmt.toml` in Task 1.
  - BLOCKER (`symbols <FILE> --path` ambiguity) — fixed by joining `target` against `args.path` in Task 8.
  - BLOCKER (Python decorator signature) — fixed by moving `@symbol.fn`/`@symbol.class` to the inner definition in Task 11.
  - STRONG (JSON envelope versioning) — `output::print_json` wraps every response in `{schema_version:1, data:…}`; tests assert it (Tasks 4, 5, 6, 8, 12, 13, 14).
  - STRONG (`rust-toolchain.toml`) — added in Task 1.
  - STRONG (dedupe ignore-dir filter) — lifted to `walk::default_walker` + `IGNORED_DIRS`, reused by tree (Tasks 4, 6).
  - STRONG (`query_source → Option`) — Task 7 + Tasks 9–11 wire `Some(include_str!(…))`.
  - STRONG (`--locked` CI + `Cargo.lock` committed) — Task 17 + Task 2 commit.
- **Tree-sitter API:** reviewers disagreed; the rust-reviewer verified the pinned grammar 0.21.x + tree-sitter 0.22 combo uses `language()` functions and is correct. The Task 2 engineer note covers the fallback if a future ABI change occurs.
- **Type consistency:** `Symbol`, `SymbolKind`, `Language` defined in Task 7 and used unchanged in Tasks 8, 12, 13, 14.
- **No placeholders:** every code step contains actual code; every command step has expected output described.
