# astedit PR 3 — Structural `rewrite` Subcommand Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the `astedit rewrite --pattern P --rewrite R [--path DIR] [--apply] [--json] [--lang LANG]` subcommand on the existing `astedit` binary. Pure ast-grep pattern→rewrite pipeline (no `codegraph_core::build_index`, no resolver). Per-file lazy compilation. AST-shape-exact matches are emitted as `applied[]` with `confidence`/`reason` omitted from every edit (spec § Output schema: rename-only fields). Compilation failure on any selected language ⇒ exit non-zero with `error_kind: "pattern-compile"`. Reuses PR 2's `apply::{write_atomic, current_len}` for atomic writes + race-window guard.

**Architecture:** astedit gains a second subcommand sharing the existing CLI dispatcher, JSON envelope helper, error enum, and apply machinery. The new `commands::rewrite::run` walks the project via `codegraph_core::walk::walk_sources`, lazily compiles `(pattern, rewrite)` into an ast-grep `Pattern` + replacement template per language, runs `find_all` on every file's tree, materialises each match into a `RewriteEdit { line, col, start_byte, end_byte, old, new }`, then either emits the dry-run JSON envelope or applies edits in reverse byte order via the same atomic-write pipeline rename uses. `RewriteData` is a parallel struct to `RenameData` (recommendation in the handoff was to keep them separate for readability — the runtime cost is zero and the wire format is determined by which one the dispatcher passes to `print_json`).

**Tech Stack:** Rust 2021, existing `astedit` bin+lib crate, `codegraph_core::{walk, lang}`, `ast-grep-core 0.38.7` + `ast-grep-language 0.38.7` (workspace-pinned in PR 2, finally consumed in PR 3), existing `clap` / `serde` / `serde_json` / `anyhow` / `thiserror` / `tempfile`.

**Branch:** `feat/astedit-rewrite` (already checked out from post-merge `main`, no commits yet). All commits in this plan land on that branch; the PR opens against `main` after Task 17 passes.

**Test baseline going in:** 66 tests pass on `main` post-PR 2:
- 13 `codemap`
- 18 `codegraph`
- 17 `codegraph-core`
- 18 `astedit` (1 error unit + 3 serialize unit + 4 apply unit + 10 rename integration tests)

PR 3 must preserve those 66 and add 11 new tests (described in Task 5 onward), for a final count of **77 tests**.

**Relationship to PR 2's plan:** This plan deliberately copies the task-and-step granularity from `docs/superpowers/plans/2026-05-21-astedit-pr2-rename.md`. The "Deviation from spec" note in PR 2's Task 2 (ast-grep deps declared in workspace but not consumed by astedit) is **reversed in this PR**: PR 3's Task 1 wires them into `astedit/Cargo.toml` after resolving the tree-sitter version conflict that forced PR 2's deferral.

---

## File Structure

**Files being created:**

```
crates/astedit/
  src/
    rewrite.rs                 — ast-grep helper: lang mapping, compile, find-and-rewrite
    commands/
      rewrite.rs               — the rewrite pipeline (top-level subcommand handler)
  tests/
    rewrite_test.rs            — 11 integration tests
    fixtures/
      rewrite_rust/            — Task 5 fixture (Rust pattern rewrite)
      rewrite_typescript/      — Task 6 fixture
      rewrite_tsx/             — Task 7 fixture
      rewrite_javascript/      — Task 8 fixture
      rewrite_python/          — Task 9 fixture
      rewrite_metavar/         — Task 10 fixture ($X)
      rewrite_multimatch/      — Task 11 fixture ($$$ARGS)
      rewrite_no_match/        — Task 12 fixture
      rewrite_lang_filter/     — Task 13 fixture (mixed languages)
      rewrite_apply/           — Task 15 fixture (apply path)
```

**Files being modified:**

- `Cargo.toml` (workspace root) — *only if Task 1 ends up bumping `tree-sitter*` versions for the conflict resolution*. The ast-grep workspace deps are already there from PR 2.
- `crates/codegraph-core/Cargo.toml` — bump `tree-sitter` + grammar deps to versions compatible with ast-grep-core 0.38.7's `tree-sitter ^0.25.4` (Task 1). **This touches `codegraph-core`. The handoff flagged this as the case where the rename pipeline reveals a missing capability that lives in core. Task 1 includes a confirmation gate before edits land.**
- `crates/codegraph-core/src/lang.rs` — only if the new grammar versions changed their `language()` exporter signature (Task 1 substep). If unchanged, no edit.
- `crates/astedit/Cargo.toml` — add three `ast-grep-* = { workspace = true }` dep lines.
- `crates/astedit/src/cli.rs` — add `Rewrite(RewriteArgs)` to `Command`, add `RewriteArgs` struct.
- `crates/astedit/src/commands/mod.rs` — add `pub mod rewrite;`.
- `crates/astedit/src/main.rs` — add `Command::Rewrite(a) => commands::rewrite::run(a)?,` dispatch arm.
- `crates/astedit/src/lib.rs` — add `pub mod rewrite;` so integration tests and `commands::rewrite` can both import the helper module.
- `crates/astedit/src/serialize.rs` — add `RewriteData`, `RewriteAppliedFile`, `RewriteEdit` structs.
- `crates/astedit/src/error.rs` — wire `PatternCompile` into use (the variant is already defined; remove the enum-level `#[allow(dead_code)]` if all variants become used).
- `skills/ny-astedit/SKILL.md` — extend the discovery hook + commands list to mention `rewrite`.

**No edits to:** `codemap`, `codegraph` (binary crate), the existing `commands/rename.rs`, the existing `tests/rename_test.rs`. PR 2's rename pipeline is locked. If a missing helper looks like it belongs in `codegraph-core` beyond the tree-sitter bump in Task 1, stop and flag.

---

## Task 1: Tree-sitter version conflict resolution + add ast-grep deps to astedit

**Files:**
- Modify: `crates/codegraph-core/Cargo.toml`
- Modify: `crates/codegraph-core/src/lang.rs` (only if grammar API changed)
- Modify: `crates/astedit/Cargo.toml`
- Modify: `Cargo.lock` (regenerated)

This task is load-bearing and has a **user confirmation gate** before any source edit lands. The handoff for PR 2 said "Do NOT modify `codegraph-core`" — that guidance is intentionally overridden here because the rename pipeline revealed a missing capability (ast-grep compatibility) that lives in core. Bump first, ask second.

### Conflict recap

- `ast-grep-core 0.38.7` (pinned exact in workspace `[workspace.dependencies]`) requires `tree-sitter ^0.25.4`.
- `codegraph-core` currently depends on `tree-sitter 0.22` via `tree-sitter-rust 0.21`, `tree-sitter-typescript 0.21`, `tree-sitter-javascript 0.21`, `tree-sitter-python 0.21`.
- Cargo's `links = "tree-sitter"` constraint forbids two packages with the same `links` value in a single dependency graph. astedit's graph would pull in both via `codegraph-core` and `ast-grep-language` ⇒ resolution error.
- `codemap` has its own copy of `tree-sitter 0.22` + grammar deps but **does not depend on `codegraph-core`** (confirmed via `crates/codemap/Cargo.toml`). It stays on 0.22; this task does not touch it.
- `codegraph` (binary) depends on `codegraph-core` (path) only, so it transitively absorbs the bump.

### Substeps

- [ ] **Step 1: Reproduce the conflict empirically (baseline)**

```bash
cd /Users/detoro/code/skills

# Snapshot the current state.
cargo tree -p codegraph-core | grep tree-sitter
```

Expected output:

```
├── tree-sitter v0.22.6
├── tree-sitter-javascript v0.21.4
│   └── tree-sitter v0.22.6 (*)
├── tree-sitter-python v0.21.0
│   └── tree-sitter v0.22.6 (*)
├── tree-sitter-rust v0.21.2
│   └── tree-sitter v0.22.6 (*)
└── tree-sitter-typescript v0.21.2
    └── tree-sitter v0.22.6 (*)
```

Confirm `cargo test --workspace --locked` shows 66 tests passing before editing anything. Anything else means the branch state has drifted; stop and investigate.

```bash
cargo test --workspace --locked
```

Expected: `66 tests passed` (sum across `codemap`, `codegraph`, `codegraph-core`, `astedit`).

- [ ] **Step 2: Probe which grammar versions support `tree-sitter ^0.25`**

We need versions of `tree-sitter-rust`, `tree-sitter-typescript`, `tree-sitter-javascript`, `tree-sitter-python` whose own `Cargo.toml` lists `tree-sitter = "^0.25"` (or wider). The crates.io API exposes this via `cargo search` and the deps endpoint, but the cheapest probe is `cargo info`:

```bash
cargo info tree-sitter-rust       2>&1 | head -30
cargo info tree-sitter-typescript 2>&1 | head -30
cargo info tree-sitter-javascript 2>&1 | head -30
cargo info tree-sitter-python     2>&1 | head -30
```

Look at the `deps:` section of each output for the listed `tree-sitter` version. From cargo's index, the latest versions are roughly:

- `tree-sitter-rust = "0.24.x"` (probably still on tree-sitter 0.22) and possibly `0.23.x` / `0.24.x` on tree-sitter 0.25.
- `tree-sitter-typescript = "0.23.x"`.
- `tree-sitter-javascript = "0.25.x"` (on tree-sitter 0.25).
- `tree-sitter-python = "0.25.x"` (on tree-sitter 0.25).

You want the **lowest version of each grammar that depends on `tree-sitter ^0.25`** — that minimises grammar-side API drift. If `cargo info` doesn't surface deps clearly, fetch the crate `Cargo.toml` from crates.io:

```bash
# Pick a specific version once you've identified a candidate (substitute the number):
curl -sL https://crates.io/api/v1/crates/tree-sitter-rust/0.24.0/download \
  | tar -tzvf - 2>/dev/null \
  | grep Cargo.toml | head
```

Or read it via the registry HTTP API:

```bash
curl -sL https://crates.io/api/v1/crates/tree-sitter-rust | jq '.versions[] | select(.num | startswith("0.24")) | .num' | head
curl -sL https://crates.io/api/v1/crates/tree-sitter-rust/0.24.0/dependencies | jq '.dependencies[] | select(.crate_id=="tree-sitter") | {version_req, kind}'
```

Document the exact versions you pick in the commit message of Step 7 below.

- [ ] **Step 3: USER CONFIRMATION GATE**

Before editing `codegraph-core/Cargo.toml`, surface the planned change set to the user:

> "PR 3 Task 1 needs to bump `codegraph-core`'s tree-sitter family to versions compatible with `ast-grep-core 0.38.7` (requires `tree-sitter ^0.25.4`). Concretely:
>
> - `tree-sitter` `0.22` → `0.25.x`
> - `tree-sitter-rust` `0.21` → `<resolved version>`
> - `tree-sitter-typescript` `0.21` → `<resolved version>`
> - `tree-sitter-javascript` `0.21` → `<resolved version>`
> - `tree-sitter-python` `0.21` → `<resolved version>`
>
> If grammar entry-point signatures changed between 0.21 and the new minor (e.g. `tree_sitter_rust::language()` was renamed to `LANGUAGE`), a follow-up edit to `crates/codegraph-core/src/lang.rs` will also be required.
>
> codemap stays on tree-sitter 0.22 (it does not depend on `codegraph-core`, so its graph is isolated). codegraph picks up the bump transitively because it depends on `codegraph-core`. All 17 codegraph-core tests + 18 codegraph tests + 18 astedit tests must still pass after the bump.
>
> Confirm to proceed?"

Wait for explicit user confirmation. If the user rejects the bump, fall back to **Option 2** — find an older ast-grep release that still uses `tree-sitter 0.22`. The handoff judged this unlikely to exist, so the user is the final arbiter.

- [ ] **Step 4: Edit `crates/codegraph-core/Cargo.toml`**

Open `crates/codegraph-core/Cargo.toml` and replace the five `tree-sitter*` lines under `[dependencies]`:

```toml
tree-sitter = "0.22"
tree-sitter-rust = "0.21"
tree-sitter-typescript = "0.21"
tree-sitter-javascript = "0.21"
tree-sitter-python = "0.21"
```

with the resolved versions from Step 2 (placeholders below; substitute the exact strings):

```toml
tree-sitter = "0.25"
tree-sitter-rust = "<resolved>"
tree-sitter-typescript = "<resolved>"
tree-sitter-javascript = "<resolved>"
tree-sitter-python = "<resolved>"
```

Use caret (`"^0.25"`) ranges unless a specific patch is required for the version to actually pick up tree-sitter 0.25 — version exactness like the ast-grep deps is not warranted here.

- [ ] **Step 5: Regenerate the lockfile + audit duplicates**

```bash
cargo update -w
cargo tree -d
```

Expected `cargo tree -d` behaviour:

- Zero `tree-sitter` entries listed under "duplicate" *within any single dependency graph* (codemap's tree-sitter 0.22 is fine because nothing else in codemap's graph pulls tree-sitter; codegraph-core's tree-sitter 0.25 is fine because it's the only one in its graph; astedit will resolve through codegraph-core's 0.25 + ast-grep's 0.25, which **must** unify to the same version).
- It's OK for `tree-sitter` to appear twice overall if and only if they're in *different* graphs. The relevant check is `cargo check -p astedit --locked` succeeding (Step 6).

If `cargo tree -d` reports `tree-sitter` duplication inside `astedit`'s graph, that means `ast-grep-language` and the new `codegraph-core` ended up on different patch versions. Bump `codegraph-core`'s `tree-sitter` to the exact patch that `ast-grep-language 0.38.7` resolves to (find with `cargo tree -p ast-grep-language | grep tree-sitter`).

- [ ] **Step 6: Verify `codegraph-core` still compiles + verify grammar APIs**

```bash
cargo check -p codegraph-core --locked
```

If this fails with errors like:

```
error[E0425]: cannot find function `language` in crate `tree_sitter_rust`
```

— the grammar moved from a `language()` function to a `LANGUAGE` constant. Adjust `crates/codegraph-core/src/lang.rs::Language::ts_language` accordingly:

```rust
pub fn ts_language(self) -> TsLanguage {
    match self {
        Language::Rust       => tree_sitter_rust::LANGUAGE.into(),
        Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Language::Tsx        => tree_sitter_typescript::LANGUAGE_TSX.into(),
        Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        Language::Python     => tree_sitter_python::LANGUAGE.into(),
    }
}
```

(The `tree-sitter` 0.23+ grammars typically export a `LANGUAGE: LanguageFn` constant that implements `Into<Language>`. If yours don't, consult each crate's `lib.rs` via the `cargo info` or registry tarball from Step 2.)

If the compile succeeds without edits, leave `lang.rs` alone — the grammar entry points didn't change names.

- [ ] **Step 7: Verify `codegraph-core` tests still pass**

```bash
cargo test -p codegraph-core --locked
```

Expected: **17 tests pass** (the baseline). If any test fails because tree-sitter 0.25 parses something differently than 0.22 (e.g. node type names changed), this becomes a follow-up issue — but for the supported languages used by codegraph-core's fixtures, this is unlikely. If a test fails, **stop and flag**: that's a real semantic change.

- [ ] **Step 8: Verify `codegraph` and `codemap` still pass**

```bash
cargo test -p codegraph --locked
cargo test -p codemap --locked
```

Expected: 18 + 13 tests pass respectively. `codemap` is isolated from the bump; this is just sanity.

- [ ] **Step 9: Add `ast-grep-*` deps to `astedit/Cargo.toml`**

Edit `crates/astedit/Cargo.toml`. Under `[dependencies]`, after the `thiserror = { workspace = true }` line, add:

```toml
ast-grep-core     = { workspace = true }
ast-grep-config   = { workspace = true }
ast-grep-language = { workspace = true }
```

The exact-pinned versions live in the root `[workspace.dependencies]` from PR 2's Task 1 (`= "0.38.7"`).

- [ ] **Step 10: Verify `astedit` compiles with ast-grep linked in**

```bash
cargo check -p astedit --locked
```

Expected: success. The new deps are loaded but no astedit source consumes them yet — that lands in Task 4.

If this fails with the `links = "tree-sitter"` conflict from PR 2's Task 2 deviation, the version bump in Steps 4–6 was incomplete. Re-check `cargo tree -p astedit | grep tree-sitter` and ensure both `codegraph-core`'s tree-sitter and `ast-grep-language`'s tree-sitter resolve to the SAME version.

- [ ] **Step 11: Full-workspace test run (66 tests baseline still passing)**

```bash
cargo test --workspace --locked
```

Expected: **66 tests pass** — exactly the PR 2 baseline. No new tests added in this task. If anything regressed, the tree-sitter bump moved a parse boundary; revert and re-investigate before proceeding.

- [ ] **Step 12: `cargo tree -d` final audit**

```bash
cargo tree -d
```

Output must show no `tree-sitter` duplication inside astedit's graph. (The same audit is repeated in Task 17 Step 4 — this one is the early-warning gate.)

- [ ] **Step 13: Commit**

```bash
git add Cargo.toml Cargo.lock \
        crates/codegraph-core/Cargo.toml \
        crates/codegraph-core/src/lang.rs \
        crates/astedit/Cargo.toml
git commit -m "chore(workspace): bump codegraph-core to tree-sitter 0.25, consume ast-grep in astedit"
```

(`Cargo.toml` may be unchanged if you didn't touch workspace deps — drop it from the `git add` in that case. `lang.rs` may be unchanged if grammar entry points didn't move — drop that too. The commit message stays accurate either way.)

---

## Task 2: Add `Command::Rewrite` + `RewriteArgs` to the CLI

**Files:**
- Modify: `crates/astedit/src/cli.rs`
- Modify: `crates/astedit/src/main.rs`
- Modify: `crates/astedit/src/commands/mod.rs`
- Modify: `crates/astedit/src/lib.rs`
- Create: `crates/astedit/src/commands/rewrite.rs`

Mechanical scaffolding — the binary should accept `astedit rewrite --pattern P --rewrite R` and route to an empty handler that emits a stub JSON envelope. The real pipeline lands in Tasks 4+5.

- [ ] **Step 1: Add `Rewrite(RewriteArgs)` variant + `RewriteArgs` struct to `cli.rs`**

Open `crates/astedit/src/cli.rs`. Add the new variant to `Command`:

```rust
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Rename a symbol across the project (dry-run by default; pass --apply to write).
    Rename(RenameArgs),
    /// Structural rewrite using ast-grep pattern syntax (dry-run by default; pass --apply to write).
    Rewrite(RewriteArgs),
}
```

After the existing `RenameArgs` struct, append the `RewriteArgs` struct:

```rust
#[derive(clap::Args, Debug)]
pub struct RewriteArgs {
    /// ast-grep pattern to match against the source AST.
    /// Metavars: `$X` captures a single node, `$$$X` captures multiple.
    #[arg(long)]
    pub pattern: String,

    /// Replacement template. May reference metavars captured by `--pattern`.
    #[arg(long)]
    pub rewrite: String,

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

    /// Restrict to one language (`rust`, `typescript`, `tsx`, `javascript`,
    /// `python`). Without this, every supported extension is scanned and
    /// the pattern is compiled per language on demand.
    #[arg(long)]
    pub lang: Option<String>,
}
```

- [ ] **Step 2: Create the empty `commands/rewrite.rs` stub**

Create `crates/astedit/src/commands/rewrite.rs`:

```rust
use crate::cli::RewriteArgs;
use crate::output::print_json;
use crate::serialize::RewriteData;

pub fn run(args: RewriteArgs) -> anyhow::Result<i32> {
    // Placeholder implementation. Task 5 onward fills this out.
    let data = RewriteData {
        subcommand: "rewrite",
        dry_run: !args.apply,
        applied: Some(vec![]),
        errors: Some(vec![]),
    };
    if args.json {
        print_json(data)?;
    }
    Ok(0)
}
```

(`RewriteData` doesn't exist yet — the cargo build of this task will fail at Step 4 below, signalling to move on to Task 3 which creates the struct. We're intentionally writing tasks in an order that surfaces missing pieces in deterministic compile errors.)

- [ ] **Step 3: Register the module**

Edit `crates/astedit/src/commands/mod.rs`. Replace the file contents (currently `pub mod rename;`) with:

```rust
pub mod rename;
pub mod rewrite;
```

- [ ] **Step 4: Wire the dispatcher**

Edit `crates/astedit/src/main.rs`. Add a `Command::Rewrite` arm to the match:

```rust
use astedit::cli::{Cli, Command};
use astedit::commands;
use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Rename(a) => commands::rename::run(a)?,
        Command::Rewrite(a) => commands::rewrite::run(a)?,
    };
    std::process::exit(code);
}
```

- [ ] **Step 5: Confirm `astedit::commands::rewrite` is reachable from tests via the lib crate**

`crates/astedit/src/lib.rs` already exposes `pub mod commands;`, so `commands::rewrite::run` is automatically reachable when `commands/mod.rs` re-exports it (Step 3). No edits to `lib.rs` needed in this task; just verify by reading it:

```bash
cat crates/astedit/src/lib.rs
```

Expected to contain `pub mod commands;`. If for any reason it doesn't, add the line — but PR 2's Task 18 should have landed it.

- [ ] **Step 6: Verify the compile fails on missing `RewriteData`**

```bash
cargo build -p astedit --locked
```

Expected: failure with `error[E0432]: unresolved import `crate::serialize::RewriteData``. That's the cue to advance to Task 3, which adds the struct. Do not attempt to fix this error in-task — the type is intentionally pulled forward into the placeholder so Task 3's compile loop drives the type into existence.

- [ ] **Step 7: Do NOT commit yet**

The crate is in a broken state (the stub references a non-existent type). Task 3 will land both the struct and a green build before the first commit of this PR's source changes is created.

---

## Task 3: Add `RewriteData` + `RewriteAppliedFile` + `RewriteEdit` payload structs

**Files:**
- Modify: `crates/astedit/src/serialize.rs`

`RewriteData` is the parallel of `RenameData`. `RewriteAppliedFile` mirrors `AppliedFile`. `RewriteEdit` mirrors `AppliedEdit` **but omits `confidence` and `reason`** (spec § Output schema: "rename only, omitted for rewrite — every match is AST-shape exact ⇒ implicit `high`"). Errors reuse the existing `ErrorEntry` from PR 2.

- [ ] **Step 1: Write the failing tests**

Open `crates/astedit/src/serialize.rs`. Find the `#[cfg(test)] mod tests` block at the bottom (added by PR 2's Task 5). Append three new tests after the existing ones:

```rust
    #[test]
    fn rewrite_data_serializes_omitting_none_fields() {
        let data = RewriteData {
            subcommand: "rewrite",
            dry_run: true,
            applied: Some(vec![]),
            errors: Some(vec![]),
        };
        let v: Value = serde_json::to_value(&data).unwrap();
        assert_eq!(v["subcommand"], "rewrite");
        assert_eq!(v["dry_run"], true);
        assert!(v["applied"].is_array());
        assert!(v["errors"].is_array());
        // Rewrite never carries needs_anchor or candidates — those are rename concepts.
        assert!(v.get("needs_anchor").is_none());
        assert!(v.get("candidates").is_none());
    }

    #[test]
    fn rewrite_edit_serializes_without_confidence_or_reason() {
        let edit = RewriteEdit {
            line: 4,
            col: 7,
            start_byte: 100,
            end_byte: 108,
            old: "console.log".into(),
            new: "console.error".into(),
        };
        let v: Value = serde_json::to_value(&edit).unwrap();
        assert_eq!(v["line"], 4);
        assert_eq!(v["col"], 7);
        assert_eq!(v["start_byte"], 100);
        assert_eq!(v["end_byte"], 108);
        assert_eq!(v["old"], "console.log");
        assert_eq!(v["new"], "console.error");
        // Spec: rewrite edits omit confidence/reason (AST-shape exact ⇒ implicit "high").
        assert!(v.get("confidence").is_none(), "rewrite edit must not carry confidence: {v:?}");
        assert!(v.get("reason").is_none(),     "rewrite edit must not carry reason: {v:?}");
    }

    #[test]
    fn rewrite_applied_file_shape_matches_spec() {
        let file = RewriteAppliedFile {
            file: "src/lib.rs".into(),
            bytes_changed: 12,
            edits: vec![RewriteEdit {
                line: 1, col: 0,
                start_byte: 0, end_byte: 4,
                old: "User".into(),
                new: "Account".into(),
            }],
        };
        let v: Value = serde_json::to_value(&file).unwrap();
        assert_eq!(v["file"], "src/lib.rs");
        assert_eq!(v["bytes_changed"], 12);
        assert!(v["edits"].is_array());
        assert_eq!(v["edits"][0]["old"], "User");
        assert_eq!(v["edits"][0]["new"], "Account");
    }
```

- [ ] **Step 2: Run to confirm the tests fail to compile**

```bash
cargo test -p astedit --locked
```

Expected: compile error — `RewriteData`, `RewriteAppliedFile`, `RewriteEdit` are unknown.

- [ ] **Step 3: Implement the structs**

Open `crates/astedit/src/serialize.rs`. After the existing `ErrorEntry` struct (and its `impl From<&AstEditError>`), but before the `#[cfg(test)]` block, append:

```rust
/// The wire-format payload for `astedit rewrite`. Wrapped by `output::print_json`
/// in `{schema_version: 1, data: <RewriteData>}`. Parallel to `RenameData` —
/// kept separate (rather than a generic envelope) so the JSON shape is
/// auditable per subcommand at a glance.
#[derive(Debug, Serialize)]
pub struct RewriteData {
    pub subcommand: &'static str, // always "rewrite"
    pub dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied: Option<Vec<RewriteAppliedFile>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<ErrorEntry>>,
}

/// One file's worth of rewrite edits. Mirrors `AppliedFile` but uses `RewriteEdit`
/// (no confidence/reason fields per spec § Output schema).
#[derive(Debug, Serialize)]
pub struct RewriteAppliedFile {
    pub file: String,
    pub bytes_changed: i64,
    pub edits: Vec<RewriteEdit>,
}

/// A single structural rewrite edit. Confidence and reason are deliberately
/// omitted — structural matches are AST-shape exact (implicit "high").
#[derive(Debug, Serialize)]
pub struct RewriteEdit {
    pub line: usize,
    pub col: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub old: String,
    pub new: String,
}
```

- [ ] **Step 4: Run the tests**

```bash
cargo test -p astedit --locked
```

Expected: **69 tests pass** (66 baseline + 3 new serialize unit tests). The `commands::rewrite` placeholder from Task 2 now also compiles cleanly.

- [ ] **Step 5: Verify the CLI smoke-test still works**

```bash
cargo build -p astedit --locked
./target/debug/astedit --help
./target/debug/astedit rewrite --pattern 'foo' --rewrite 'bar' --json --path /tmp
```

Expected: `--help` shows both `rename` and `rewrite` subcommands. The `rewrite` invocation emits:

```json
{"schema_version":1,"data":{"subcommand":"rewrite","dry_run":true,"applied":[],"errors":[]}}
```

and exits 0.

- [ ] **Step 6: Clippy**

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add crates/astedit/src/cli.rs \
        crates/astedit/src/main.rs \
        crates/astedit/src/commands/mod.rs \
        crates/astedit/src/commands/rewrite.rs \
        crates/astedit/src/serialize.rs
git commit -m "feat(astedit): scaffold rewrite subcommand CLI + JSON payload structs"
```

---

## Task 4: ast-grep helper module — language mapping + pattern compile + match-to-edit

**Files:**
- Create: `crates/astedit/src/rewrite.rs`
- Modify: `crates/astedit/src/lib.rs`

Centralise the ast-grep API surface in one module so the pipeline in `commands/rewrite.rs` stays thin. The helper exposes:

1. A mapping from `codegraph_core::lang::Language` → ast-grep's per-language `Language` trait impl.
2. A function `rewrite_file(source, pattern, rewrite, lang)` returning `Result<Vec<RewriteSite>, AstEditError::PatternCompile>` where `RewriteSite` carries `(start_byte, end_byte, line, col, old, new)`.

We deliberately do NOT touch disk in this module — `commands/rewrite.rs` orchestrates IO, drift checks, and atomic writes.

### ast-grep API research substep

Before implementing, confirm the exact ast-grep API surface against the pinned 0.38.7 source. From the Context7 docs (verified during plan-writing), the relevant primitives are:

```rust
use ast_grep_core::tree_sitter::LanguageExt;
use ast_grep_core::language::{Tsx, /* etc. — one struct per supported language */};
use ast_grep_core::{Matcher, Pattern, AstGrep};

let ast = Tsx.ast_grep(source_str);
let root = ast.root();
let pattern = Pattern::try_new("console.log($A)", Tsx)
    .map_err(/* compile failure */)?;
let matches: Vec<_> = root.find_all(&pattern).collect();
// per match m:
//   m.range()              — byte range
//   m.text()               — original matched text
//   m.get_env().get_match("A") / get_multiple_matches("A")
```

For computing the *new* text per match (rewrite template applied), 0.38.7's ast-grep exposes two paths:

- **`Fixer::try_new(rewrite_template, lang)` + `fixer.generate_replacement(&match)`** — clean, single-call.
- **`ast.replace(pattern, rewrite)`** — mutates the whole tree, returns bool, doesn't expose per-match new-text directly.

Use `Fixer` if it exists in 0.38.7's public API. If it doesn't (the API moved between releases), fall back to a manual metavar substitution: walk the rewrite template, replace `$X` with `env.get_match("X").unwrap().text()` and `$$$X` with `env.get_multiple_matches("X")` joined by ", " (the canonical multimatch concatenation).

**Sanity step before writing rewrite.rs:**

```bash
cargo doc -p ast-grep-core --no-deps
# Open the generated doc and search for "Fixer" / "Pattern" / "find_all".
# Or:
find ~/.cargo/registry/src -name 'ast-grep-core-0.38.7' -type d
# Then read its src/lib.rs and src/matcher/pattern.rs.
```

Substitute the actual struct/method names below if they differ. The plan code uses `Fixer` because the Context7 docs from the same crate family advertise it; verify by reading the local checkout.

### Implementation

- [ ] **Step 1: Write the unit-test scaffold**

Create `crates/astedit/src/rewrite.rs` with this header (tests at the bottom):

```rust
//! ast-grep helper used by `commands/rewrite.rs`. Centralises pattern
//! compilation, per-language dispatch, and match-to-edit materialisation
//! so the command handler can focus on file IO + the JSON envelope.

use codegraph_core::lang::Language as CgLang;

use crate::error::AstEditError;

/// A structural match site materialised into the byte/line/col coordinates
/// the JSON envelope needs. Produced by `rewrite_file`.
#[derive(Debug, Clone)]
pub struct RewriteSite {
    pub start_byte: usize,
    pub end_byte: usize,
    pub line: usize,
    pub col: usize,
    pub old: String,
    pub new: String,
}

/// Compile (pattern, rewrite) for `lang` and return every match site as a
/// `RewriteSite` carrying both byte coordinates and the materialised
/// replacement text.
///
/// Returns `Err(PatternCompile)` if `pattern` or `rewrite` is not a valid
/// ast-grep template for `lang`. All other failure modes (file IO, drift,
/// atomic write) are the responsibility of the caller.
pub fn rewrite_file(
    source: &str,
    pattern: &str,
    rewrite: &str,
    lang: CgLang,
) -> Result<Vec<RewriteSite>, AstEditError> {
    // Implementation lands in Step 3 — driven by the failing tests in Step 2.
    let _ = (source, pattern, rewrite, lang);
    Err(AstEditError::PatternCompile {
        lang: lang.name().to_string(),
        message: "rewrite_file not yet implemented".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_file_rust_simple_pattern() {
        let source = "fn make() { println!(\"hi\"); }";
        let sites = rewrite_file(source, "println!($A)", "eprintln!($A)", CgLang::Rust)
            .expect("compile + match");
        assert_eq!(sites.len(), 1, "one match expected; got {sites:?}");
        let s = &sites[0];
        assert_eq!(s.old, "println!(\"hi\")");
        assert_eq!(s.new, "eprintln!(\"hi\")");
        assert!(s.start_byte < s.end_byte);
        assert_eq!(s.line, 1);
    }

    #[test]
    fn rewrite_file_no_match_returns_empty_vec() {
        let source = "fn main() {}";
        let sites = rewrite_file(source, "println!($A)", "eprintln!($A)", CgLang::Rust)
            .expect("compile ok, no matches");
        assert!(sites.is_empty(), "expected no matches; got {sites:?}");
    }

    #[test]
    fn rewrite_file_invalid_pattern_returns_pattern_compile() {
        // Use a syntactically broken pattern. `$` alone is not a valid metavar
        // in ast-grep, and `(((` has unbalanced delimiters — both should fail
        // to compile across all languages.
        let source = "fn main() {}";
        let err = rewrite_file(source, "(((", "fn main() {}", CgLang::Rust)
            .expect_err("expected pattern compile failure");
        assert_eq!(err.kind(), "pattern-compile", "got: {err:?}");
        if let AstEditError::PatternCompile { lang, .. } = err {
            assert_eq!(lang, "rust");
        }
    }
}
```

- [ ] **Step 2: Register the module in `lib.rs`**

Edit `crates/astedit/src/lib.rs`. Add `pub mod rewrite;` to the list (alphabetically after `pub mod output;`):

```rust
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
```

- [ ] **Step 3: Run the tests to confirm they fail (red)**

```bash
cargo test -p astedit --locked rewrite::tests
```

Expected: the first two tests fail (they call into the not-yet-implemented `rewrite_file` which returns a synthetic `PatternCompile`). The third test passes coincidentally because the stub returns `PatternCompile` for every input — that's a false positive but acceptable; Step 4's real implementation makes the third test pass for the right reason.

- [ ] **Step 4: Implement `rewrite_file`**

Replace the stub body of `rewrite_file` in `crates/astedit/src/rewrite.rs`. The exact API surface depends on the resolved ast-grep 0.38.7 source (see "API research substep" above). The reference implementation below assumes:

- `ast_grep_core::Pattern::try_new(template, lang)` — compiles patterns.
- `ast_grep_core::Fixer::try_new(template, lang)` — compiles replacement templates.
- `ast_grep_core::tree_sitter::LanguageExt::ast_grep(source)` — entry point that produces an `AstGrep<L>`.
- `ast_grep_core::language::{Rust, TypeScript, Tsx, JavaScript, Python}` — per-language unit structs.
- `m.range()` — returns a `std::ops::Range<usize>` of byte offsets.
- `m.start_pos()` — returns `(row, col)` (zero-based row + UTF-8 column). Confirm whether ast-grep numbers rows from 0 or 1; we add 1 for both row and col before emitting because the JSON envelope is 1-based.
- `fixer.generate_replacement(&match)` — returns a `String`.

If any of these names differ, adjust the call sites; the *shape* of the implementation is correct regardless.

```rust
use ast_grep_core::{Matcher, Pattern};
use ast_grep_core::language::{JavaScript, Python, Rust, Tsx, TypeScript};
use ast_grep_core::tree_sitter::LanguageExt;

pub fn rewrite_file(
    source: &str,
    pattern: &str,
    rewrite: &str,
    lang: CgLang,
) -> Result<Vec<RewriteSite>, AstEditError> {
    let lang_name = lang.name();

    // Per-language dispatch. Each branch compiles pattern + fixer for the
    // language unit struct, then runs find_all. The arms are deliberately
    // duplicated because ast-grep's `Language` type parameter is monomorphic.
    match lang {
        CgLang::Rust       => collect_sites(source, pattern, rewrite, Rust,       lang_name),
        CgLang::TypeScript => collect_sites(source, pattern, rewrite, TypeScript, lang_name),
        CgLang::Tsx        => collect_sites(source, pattern, rewrite, Tsx,        lang_name),
        CgLang::JavaScript => collect_sites(source, pattern, rewrite, JavaScript, lang_name),
        CgLang::Python     => collect_sites(source, pattern, rewrite, Python,     lang_name),
    }
}

fn collect_sites<L>(
    source: &str,
    pattern: &str,
    rewrite: &str,
    lang: L,
    lang_name: &str,
) -> Result<Vec<RewriteSite>, AstEditError>
where
    L: LanguageExt + Copy,
{
    let compiled = Pattern::try_new(pattern, lang).map_err(|e| AstEditError::PatternCompile {
        lang: lang_name.to_string(),
        message: format!("pattern: {e}"),
    })?;
    let fixer =
        ast_grep_core::Fixer::try_new(rewrite, lang).map_err(|e| AstEditError::PatternCompile {
            lang: lang_name.to_string(),
            message: format!("rewrite: {e}"),
        })?;

    let ast = lang.ast_grep(source);
    let root = ast.root();
    let mut out = Vec::new();
    for m in root.find_all(&compiled) {
        let range = m.range();
        let pos = m.start_pos();        // (row, col) — confirm 0/1-based on local docs
        let line = pos.0.saturating_add(1);
        let col  = pos.1.saturating_add(1);
        let old  = source[range.start..range.end].to_string();
        let new  = fixer.generate_replacement(&m);
        out.push(RewriteSite {
            start_byte: range.start,
            end_byte: range.end,
            line,
            col,
            old,
            new,
        });
    }
    Ok(out)
}
```

If the ast-grep 0.38.7 surface doesn't have `Fixer`, replace `fixer.generate_replacement(&m)` with a manual substitution:

```rust
fn render_rewrite(template: &str, m: &ast_grep_core::NodeMatch<'_, L>) -> String {
    let env = m.get_env();
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' { out.push(c); continue; }
        // $$$NAME multi-match
        if chars.peek() == Some(&'$') {
            chars.next();                    // consume second $
            if chars.peek() == Some(&'$') {
                chars.next();                // consume third $
                let name = take_ident(&mut chars);
                let parts = env
                    .get_multiple_matches(&name)
                    .iter()
                    .map(|n| n.text().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&parts);
                continue;
            }
            // $$NAME (rare in practice; treat as text)
            out.push('$'); out.push('$');
            continue;
        }
        // $NAME single
        let name = take_ident(&mut chars);
        if let Some(node) = env.get_match(&name) {
            out.push_str(node.text());
        } else {
            // Unknown metavar — emit verbatim to surface the bug at review.
            out.push('$');
            out.push_str(&name);
        }
    }
    out
}

fn take_ident<I: Iterator<Item = char>>(it: &mut std::iter::Peekable<I>) -> String {
    let mut name = String::new();
    while let Some(&c) = it.peek() {
        if c.is_alphanumeric() || c == '_' { name.push(c); it.next(); }
        else { break; }
    }
    name
}
```

Pick `Fixer` if it exists. The handoff explicitly named ast-grep-config — that's where the rule/config DSL lives; we don't need its `RuleConfig` machinery in PR 3, just the core pattern + fixer primitives from `ast-grep-core`.

- [ ] **Step 5: Run the rewrite unit tests**

```bash
cargo test -p astedit --locked rewrite::tests
```

Expected: **3 tests pass**.

If the third test (invalid-pattern) doesn't actually fail compilation, swap the test input for something ast-grep definitely rejects — e.g. `"$"` (bare dollar), `""` (empty), or `"fn $X("` (unbalanced).

- [ ] **Step 6: Full suite + clippy**

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: **72 tests pass** (66 baseline + 3 from Task 3's serialize tests + 3 new rewrite unit tests). Clippy clean. If clippy complains about the `let _ = (source, pattern, rewrite, lang);` discard line being removed (it should be, after Step 4's real impl), drop it.

- [ ] **Step 7: Commit**

```bash
git add crates/astedit/src/rewrite.rs crates/astedit/src/lib.rs
git commit -m "feat(astedit): add ast-grep rewrite helper (pattern compile + match → edit)"
```

---

## Task 5: TDD — Rust pattern rewrite (dry-run, single file)

**Files:**
- Create: `crates/astedit/tests/fixtures/rewrite_rust/main.rs`
- Create: `crates/astedit/tests/rewrite_test.rs`
- Modify: `crates/astedit/src/commands/rewrite.rs`

This is the load-bearing integration task that drives the per-file walk + pattern-to-envelope wiring into existence. Subsequent TDD tasks layer on edge cases (other languages, metavars, no-match, filter, apply).

- [ ] **Step 1: Create the fixture**

```bash
mkdir -p crates/astedit/tests/fixtures/rewrite_rust
```

Write `crates/astedit/tests/fixtures/rewrite_rust/main.rs`:

```rust
fn warn() {
    println!("hi");
    println!("there");
}
```

Two structural matches of `println!($A)` ⇒ two edits after `--rewrite eprintln!($A)`.

- [ ] **Step 2: Create the integration-test file with the common helper imports**

Create `crates/astedit/tests/rewrite_test.rs`:

```rust
mod common;

use common::{copy_fixture, run_astedit_json};
```

This declares the same `common` module the rename tests use (`tests/common/mod.rs` from PR 2's Task 7). The two integration-test files share it — that's the cargo convention.

- [ ] **Step 3: Write the failing test**

Append to `crates/astedit/tests/rewrite_test.rs`:

```rust
#[test]
fn rewrite_rust_two_matches_dry_run_default() {
    let tmp = copy_fixture("rewrite_rust");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rewrite",
        "--pattern", "println!($A)",
        "--rewrite", "eprintln!($A)",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0, "dry-run with matches exits 0");
    assert_eq!(data["subcommand"], "rewrite");
    assert_eq!(data["dry_run"], true);
    assert!(data["errors"].as_array().unwrap().is_empty(), "no errors expected: {:?}", data["errors"]);

    let applied = data["applied"].as_array().expect("applied array");
    assert_eq!(applied.len(), 1, "single fixture file expected; got: {applied:?}");

    let file_entry = &applied[0];
    assert!(file_entry["file"].as_str().unwrap().ends_with("main.rs"));

    let edits = file_entry["edits"].as_array().unwrap();
    assert_eq!(edits.len(), 2, "expected 2 println matches; got {}", edits.len());

    for e in edits {
        assert!(e["old"].as_str().unwrap().starts_with("println!"));
        assert!(e["new"].as_str().unwrap().starts_with("eprintln!"));
        // Spec: rewrite edits omit confidence/reason.
        assert!(e.get("confidence").is_none(), "rewrite edit must not have confidence: {e:?}");
        assert!(e.get("reason").is_none(),     "rewrite edit must not have reason: {e:?}");
        assert!(e["start_byte"].as_u64().unwrap() < e["end_byte"].as_u64().unwrap());
    }

    // Dry-run must NOT mutate the fixture copy.
    let fixture_file = tmp.path().join("main.rs");
    let after = std::fs::read_to_string(&fixture_file).unwrap();
    assert!(after.contains("println!(\"hi\")"), "dry-run modified the fixture: {after}");
    assert!(!after.contains("eprintln"),        "dry-run wrote eprintln! into the fixture");
}
```

- [ ] **Step 4: Run to confirm fail**

```bash
cargo test -p astedit --test rewrite_test --locked rewrite_rust_two_matches_dry_run_default
```

Expected: failure — the current `commands::rewrite::run` is the empty stub from Task 2.

- [ ] **Step 5: Implement the rewrite pipeline**

Replace `crates/astedit/src/commands/rewrite.rs` end-to-end. The pipeline is two-pass on purpose: pass 1 compiles `(pattern, rewrite)` for every language present in the walk (or the single `--lang` target); if ANY language fails to compile, the run is aborted before any file is read or written. Pass 2 only runs when pass 1 was clean — matching the spec's "Compilation failure on any selected language ⇒ exit non-zero" wording, which is incompatible with the alternative single-pass approach of applying some files before discovering a compile failure in a later language.

```rust
use std::collections::BTreeSet;
use std::path::Path;

use codegraph_core::lang::Language as CgLang;
use codegraph_core::walk::walk_sources;

use crate::cli::RewriteArgs;
use crate::error::AstEditError;
use crate::output::print_json;
use crate::rewrite::{rewrite_file, RewriteSite};
use crate::serialize::{ErrorEntry, RewriteAppliedFile, RewriteData, RewriteEdit};

pub fn run(args: RewriteArgs) -> anyhow::Result<i32> {
    let root = args.path.as_path();
    let lang_filter = parse_lang_filter(args.lang.as_deref())?;

    let mut applied: Vec<RewriteAppliedFile> = Vec::new();
    let mut errors: Vec<ErrorEntry> = Vec::new();

    let sources = walk_sources(root)?;

    // Pass 1: discover which languages we'd process.
    let mut langs_to_process: BTreeSet<CgLang> = BTreeSet::new();
    for src in &sources {
        if let Some(target) = lang_filter {
            if src.language != target {
                continue;
            }
        }
        langs_to_process.insert(src.language);
    }

    // Compile (pattern, rewrite) once per language. Empty source string is a
    // pure compile-only smoke test — ast-grep's `Pattern::try_new` and
    // `Fixer::try_new` run before any source is parsed, so a compile failure
    // surfaces without IO. Compile errors collect into `errors[]`; the run
    // aborts before any apply if even one language failed.
    let mut had_compile_failure = false;
    for &lang in &langs_to_process {
        if let Err(e) = rewrite_file("", &args.pattern, &args.rewrite, lang) {
            match &e {
                AstEditError::PatternCompile { .. } => {
                    had_compile_failure = true;
                    errors.push(ErrorEntry::from(&e));
                }
                _ => {
                    // Non-compile errors during a compile-only check shouldn't
                    // happen (no source ⇒ no parse/IO). Surface defensively.
                    errors.push(ErrorEntry::from(&e));
                }
            }
        }
    }

    // Pass 2: only walk + match + apply when all languages compiled cleanly.
    if !had_compile_failure {
        for src in sources {
            if let Some(target) = lang_filter {
                if src.language != target {
                    continue;
                }
            }

            let source_text = match std::fs::read_to_string(&src.path) {
                Ok(s) => s,
                Err(e) => {
                    errors.push(ErrorEntry::from(&AstEditError::WriteFailed {
                        file: relative_path(&src.path, root),
                        os_code: e.raw_os_error(),
                        message: e.to_string(),
                    }));
                    continue;
                }
            };

            let sites = match rewrite_file(
                &source_text,
                &args.pattern,
                &args.rewrite,
                src.language,
            ) {
                Ok(sites) => sites,
                Err(e) => {
                    // Pattern compile already passed in pass 1, so this is a
                    // parse / IO-shaped failure — non-fatal per sed-like rules.
                    errors.push(ErrorEntry::from(&e));
                    continue;
                }
            };

            if sites.is_empty() {
                continue;
            }

            let rel = relative_path(&src.path, root);
            match apply_or_dry_run(&src.path, &rel, &source_text, &sites, args.apply) {
                Ok(entry) => applied.push(entry),
                Err(e) => errors.push(ErrorEntry::from(&e)),
            }
        }
    }

    let data = RewriteData {
        subcommand: "rewrite",
        dry_run: !args.apply,
        applied: Some(applied),
        errors: Some(errors),
    };

    if args.json {
        print_json(data)?;
    }

    // Exit status: non-zero only when a pattern-compile failure occurred in
    // pass 1. Other error_kind entries (write-failed, concurrent-write) are
    // non-fatal per spec § Exit status.
    Ok(if had_compile_failure { 2 } else { 0 })
}

fn parse_lang_filter(lang: Option<&str>) -> anyhow::Result<Option<CgLang>> {
    match lang {
        None => Ok(None),
        Some(s) => match s {
            "rust" => Ok(Some(CgLang::Rust)),
            "typescript" => Ok(Some(CgLang::TypeScript)),
            "tsx" => Ok(Some(CgLang::Tsx)),
            "javascript" => Ok(Some(CgLang::JavaScript)),
            "python" => Ok(Some(CgLang::Python)),
            other => Err(anyhow::anyhow!(
                "--lang {other:?} not supported; valid: rust, typescript, tsx, javascript, python"
            )),
        },
    }
}

fn relative_path(abs: &Path, root: &Path) -> String {
    abs.strip_prefix(root)
        .unwrap_or(abs)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Materialise `sites` into a `RewriteAppliedFile`. When `apply` is true,
/// also splice the bytes (reverse byte order), guard the race window, and
/// atomically write via `crate::apply::write_atomic`.
fn apply_or_dry_run(
    abs: &Path,
    rel: &str,
    source: &str,
    sites: &[RewriteSite],
    apply: bool,
) -> Result<RewriteAppliedFile, AstEditError> {
    let mut edits: Vec<RewriteEdit> = sites
        .iter()
        .map(|s| RewriteEdit {
            line: s.line,
            col: s.col,
            start_byte: s.start_byte,
            end_byte: s.end_byte,
            old: s.old.clone(),
            new: s.new.clone(),
        })
        .collect();
    // Reverse byte order so splice offsets stay valid as we apply.
    edits.sort_by(|a, b| b.start_byte.cmp(&a.start_byte));

    let bytes_changed: i64 = edits
        .iter()
        .map(|e| e.new.len() as i64 - e.old.len() as i64)
        .sum();

    if apply {
        let original_len = source.len() as u64;
        let mut bytes = source.as_bytes().to_vec();
        for e in &edits {
            // Defensive bounds check — `rewrite_file` should have produced
            // ranges within `source.len()`, but the cost is one cmp per edit.
            if e.end_byte > bytes.len() || &bytes[e.start_byte..e.end_byte] != e.old.as_bytes() {
                return Err(AstEditError::NodeKindMismatch {
                    file: rel.to_string(),
                    line: e.line,
                    col: e.col,
                });
            }
            bytes.splice(e.start_byte..e.end_byte, e.new.bytes());
        }

        // Race-window guard: re-stat just before the write. If the file's
        // length changed since we read it, bail with concurrent-write.
        let current = crate::apply::current_len(abs, rel)?;
        if current != original_len {
            return Err(AstEditError::ConcurrentWrite {
                file: rel.to_string(),
            });
        }
        crate::apply::write_atomic(abs, &bytes)?;
    }

    Ok(RewriteAppliedFile {
        file: rel.to_string(),
        bytes_changed,
        edits,
    })
}
```

- [ ] **Step 6: Run the test**

```bash
cargo test -p astedit --test rewrite_test --locked rewrite_rust_two_matches_dry_run_default
```

Expected: PASS.

If `rewrite_file` returns 0 matches even though the source contains the pattern, the ast-grep Pattern compile or `find_all` invocation in `rewrite.rs` is wrong — open `cargo doc -p ast-grep-core --no-deps --open` and re-check.

- [ ] **Step 7: Confirm no regression**

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: **73 tests pass** (72 + 1 new rewrite integration test). Clippy clean.

- [ ] **Step 8: Commit**

```bash
git add crates/astedit/tests/fixtures/rewrite_rust/ \
        crates/astedit/tests/rewrite_test.rs \
        crates/astedit/src/commands/rewrite.rs
git commit -m "feat(astedit): rewrite pipeline emits dry-run edits for Rust pattern matches"
```

---

## Task 6: TDD — TypeScript pattern rewrite

**Files:**
- Create: `crates/astedit/tests/fixtures/rewrite_typescript/main.ts`
- Modify: `crates/astedit/tests/rewrite_test.rs`

The Task 5 pipeline already iterates all supported languages. This task only adds a fixture + test for TypeScript.

- [ ] **Step 1: Create the fixture**

```bash
mkdir -p crates/astedit/tests/fixtures/rewrite_typescript
```

Write `crates/astedit/tests/fixtures/rewrite_typescript/main.ts`:

```typescript
function warn(msg: string): void {
    console.log(msg);
    console.log("done");
}
```

- [ ] **Step 2: Append the failing test**

Append to `crates/astedit/tests/rewrite_test.rs`:

```rust
#[test]
fn rewrite_typescript_matches() {
    let tmp = copy_fixture("rewrite_typescript");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rewrite",
        "--pattern", "console.log($A)",
        "--rewrite", "console.error($A)",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0);
    assert!(data["errors"].as_array().unwrap().is_empty(), "errors: {:?}", data["errors"]);

    let applied = data["applied"].as_array().expect("applied array");
    assert_eq!(applied.len(), 1);
    assert!(applied[0]["file"].as_str().unwrap().ends_with("main.ts"));

    let edits = applied[0]["edits"].as_array().unwrap();
    assert_eq!(edits.len(), 2, "two console.log calls expected");
    for e in edits {
        assert!(e["old"].as_str().unwrap().starts_with("console.log"));
        assert!(e["new"].as_str().unwrap().starts_with("console.error"));
    }
}
```

- [ ] **Step 3: Run**

```bash
cargo test -p astedit --test rewrite_test --locked rewrite_typescript_matches
```

Expected: PASS — Task 5's pipeline handles every supported language; TypeScript is just another dispatch arm in `rewrite::collect_sites`.

- [ ] **Step 4: Suite + clippy**

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: **74 tests pass**. Clippy clean.

- [ ] **Step 5: Commit**

```bash
git add crates/astedit/tests/fixtures/rewrite_typescript/ \
        crates/astedit/tests/rewrite_test.rs
git commit -m "test(astedit): rewrite pattern matches in TypeScript"
```

---

## Task 7: TDD — TSX pattern rewrite

**Files:**
- Create: `crates/astedit/tests/fixtures/rewrite_tsx/main.tsx`
- Modify: `crates/astedit/tests/rewrite_test.rs`

- [ ] **Step 1: Create the fixture**

```bash
mkdir -p crates/astedit/tests/fixtures/rewrite_tsx
```

Write `crates/astedit/tests/fixtures/rewrite_tsx/main.tsx`:

```tsx
function App() {
    console.log("render");
    return <div>hello</div>;
}
```

The mix of JSX + JS expressions is the load-bearing reason TSX has its own ast-grep `Language` impl. The pattern still targets the JS expression.

- [ ] **Step 2: Append the failing test**

Append to `crates/astedit/tests/rewrite_test.rs`:

```rust
#[test]
fn rewrite_tsx_matches_jsx_file() {
    let tmp = copy_fixture("rewrite_tsx");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rewrite",
        "--pattern", "console.log($A)",
        "--rewrite", "console.error($A)",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0);
    assert!(data["errors"].as_array().unwrap().is_empty(), "errors: {:?}", data["errors"]);
    let applied = data["applied"].as_array().expect("applied array");
    assert_eq!(applied.len(), 1, "exactly one .tsx file expected");
    assert!(applied[0]["file"].as_str().unwrap().ends_with("main.tsx"));
    assert_eq!(applied[0]["edits"].as_array().unwrap().len(), 1);
}
```

- [ ] **Step 3: Run + suite + clippy**

```bash
cargo test -p astedit --test rewrite_test --locked rewrite_tsx_matches_jsx_file
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: PASS. **75 tests pass.** Clippy clean.

- [ ] **Step 4: Commit**

```bash
git add crates/astedit/tests/fixtures/rewrite_tsx/ \
        crates/astedit/tests/rewrite_test.rs
git commit -m "test(astedit): rewrite pattern matches in TSX"
```

---

## Task 8: TDD — JavaScript pattern rewrite

**Files:**
- Create: `crates/astedit/tests/fixtures/rewrite_javascript/main.js`
- Modify: `crates/astedit/tests/rewrite_test.rs`

- [ ] **Step 1: Create the fixture**

```bash
mkdir -p crates/astedit/tests/fixtures/rewrite_javascript
```

Write `crates/astedit/tests/fixtures/rewrite_javascript/main.js`:

```javascript
function warn(msg) {
    console.log(msg);
}
```

- [ ] **Step 2: Append the failing test**

Append to `crates/astedit/tests/rewrite_test.rs`:

```rust
#[test]
fn rewrite_javascript_matches() {
    let tmp = copy_fixture("rewrite_javascript");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rewrite",
        "--pattern", "console.log($A)",
        "--rewrite", "console.error($A)",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0);
    let applied = data["applied"].as_array().expect("applied array");
    assert_eq!(applied.len(), 1);
    assert!(applied[0]["file"].as_str().unwrap().ends_with("main.js"));
    assert_eq!(applied[0]["edits"].as_array().unwrap().len(), 1);
}
```

- [ ] **Step 3: Run + suite + clippy**

```bash
cargo test -p astedit --test rewrite_test --locked rewrite_javascript_matches
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: PASS. **76 tests pass.** Clippy clean.

- [ ] **Step 4: Commit**

```bash
git add crates/astedit/tests/fixtures/rewrite_javascript/ \
        crates/astedit/tests/rewrite_test.rs
git commit -m "test(astedit): rewrite pattern matches in JavaScript"
```

---

## Task 9: TDD — Python pattern rewrite

**Files:**
- Create: `crates/astedit/tests/fixtures/rewrite_python/main.py`
- Modify: `crates/astedit/tests/rewrite_test.rs`

- [ ] **Step 1: Create the fixture**

```bash
mkdir -p crates/astedit/tests/fixtures/rewrite_python
```

Write `crates/astedit/tests/fixtures/rewrite_python/main.py`:

```python
def warn(msg):
    print(msg)
    print("done")
```

- [ ] **Step 2: Append the failing test**

Append to `crates/astedit/tests/rewrite_test.rs`:

```rust
#[test]
fn rewrite_python_matches() {
    let tmp = copy_fixture("rewrite_python");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rewrite",
        "--pattern", "print($A)",
        "--rewrite", "logging.info($A)",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0);
    let applied = data["applied"].as_array().expect("applied array");
    assert_eq!(applied.len(), 1);
    assert!(applied[0]["file"].as_str().unwrap().ends_with("main.py"));
    assert_eq!(applied[0]["edits"].as_array().unwrap().len(), 2);
    for e in applied[0]["edits"].as_array().unwrap() {
        assert!(e["new"].as_str().unwrap().starts_with("logging.info"));
    }
}
```

- [ ] **Step 3: Run + suite + clippy**

```bash
cargo test -p astedit --test rewrite_test --locked rewrite_python_matches
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: PASS. **77 tests pass.** Clippy clean.

- [ ] **Step 4: Commit**

```bash
git add crates/astedit/tests/fixtures/rewrite_python/ \
        crates/astedit/tests/rewrite_test.rs
git commit -m "test(astedit): rewrite pattern matches in Python"
```

---

## Task 10: TDD — Single-metavar `$X` capture is materialised in the rewrite

**Files:**
- Create: `crates/astedit/tests/fixtures/rewrite_metavar/main.rs`
- Modify: `crates/astedit/tests/rewrite_test.rs`

Single-metavar capture is already implicit in every prior test, but this task explicitly asserts that the captured text appears verbatim in the `new` field of each edit. If `Fixer` or the manual substitution helper is wrong, this test fires first.

- [ ] **Step 1: Create the fixture**

```bash
mkdir -p crates/astedit/tests/fixtures/rewrite_metavar
```

Write `crates/astedit/tests/fixtures/rewrite_metavar/main.rs`:

```rust
fn main() {
    let user = String::from("alice");
    let user2 = String::from("bob");
}
```

Pattern: `String::from($S)`. Rewrite: `String::from($S.to_owned())`. The single capture `$S` (the string literal node) must appear verbatim in the new text.

- [ ] **Step 2: Append the failing test**

Append to `crates/astedit/tests/rewrite_test.rs`:

```rust
#[test]
fn rewrite_single_metavar_substituted_in_replacement() {
    let tmp = copy_fixture("rewrite_metavar");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rewrite",
        "--pattern", "String::from($S)",
        "--rewrite", "String::from($S.to_owned())",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0);
    let applied = data["applied"].as_array().unwrap();
    assert_eq!(applied.len(), 1);
    let edits = applied[0]["edits"].as_array().unwrap();
    assert_eq!(edits.len(), 2);

    let news: Vec<&str> = edits.iter().map(|e| e["new"].as_str().unwrap()).collect();
    assert!(
        news.iter().any(|n| n.contains("\"alice\".to_owned()")),
        "expected `alice` capture materialised in rewrite; news = {news:?}",
    );
    assert!(
        news.iter().any(|n| n.contains("\"bob\".to_owned()")),
        "expected `bob` capture materialised in rewrite; news = {news:?}",
    );
}
```

- [ ] **Step 3: Run + suite + clippy**

```bash
cargo test -p astedit --test rewrite_test --locked rewrite_single_metavar_substituted_in_replacement
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: PASS. **78 tests pass.** Clippy clean.

If the test fails because the captured text shows as `$S` literal in the `new` field, the metavar substitution in `rewrite.rs` is broken — re-check `Fixer::generate_replacement` (or the manual fallback substitution).

- [ ] **Step 4: Commit**

```bash
git add crates/astedit/tests/fixtures/rewrite_metavar/ \
        crates/astedit/tests/rewrite_test.rs
git commit -m "test(astedit): single-metavar substitution materialises captured text"
```

---

## Task 11: TDD — Multi-metavar `$$$ARGS` capture preserves multiple nodes

**Files:**
- Create: `crates/astedit/tests/fixtures/rewrite_multimatch/main.rs`
- Modify: `crates/astedit/tests/rewrite_test.rs`

`$$$NAME` captures a comma-separated list (function args, tuple elements, etc.). Asserts the join is preserved verbatim.

- [ ] **Step 1: Create the fixture**

```bash
mkdir -p crates/astedit/tests/fixtures/rewrite_multimatch
```

Write `crates/astedit/tests/fixtures/rewrite_multimatch/main.rs`:

```rust
fn main() {
    log("a", 1, true);
    log("b");
}

fn log(_args: &str) {}
```

Pattern: `log($$$ARGS)`. Rewrite: `tracing::info!($$$ARGS)`. The match should capture `"a", 1, true` and `"b"` respectively.

- [ ] **Step 2: Append the failing test**

Append to `crates/astedit/tests/rewrite_test.rs`:

```rust
#[test]
fn rewrite_multimatch_metavar_preserves_argument_list() {
    let tmp = copy_fixture("rewrite_multimatch");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rewrite",
        "--pattern", "log($$$ARGS)",
        "--rewrite", "tracing::info!($$$ARGS)",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0);
    let applied = data["applied"].as_array().unwrap();
    assert_eq!(applied.len(), 1, "single-file fixture expected; got: {applied:?}");

    // The fn declaration `fn log(_args: &str) {}` should NOT match the call-site
    // pattern `log($$$ARGS)` — it's a definition, not a call. Two call sites
    // are the only expected matches.
    let edits = applied[0]["edits"].as_array().unwrap();
    assert_eq!(edits.len(), 2, "expected 2 call-site matches; edits={edits:?}");

    let news: Vec<&str> = edits.iter().map(|e| e["new"].as_str().unwrap()).collect();
    assert!(news.iter().any(|n| n.contains("\"a\", 1, true")),
        "expected 3-arg capture preserved; news = {news:?}");
    assert!(news.iter().any(|n| n.contains("(\"b\")")),
        "expected single-arg capture preserved; news = {news:?}");
}
```

- [ ] **Step 3: Run + suite + clippy**

```bash
cargo test -p astedit --test rewrite_test --locked rewrite_multimatch_metavar_preserves_argument_list
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: PASS. **79 tests pass.** Clippy clean.

If the test fails because the call-site `log(...)` is matched but the fn-definition `fn log(_args: &str)` is ALSO matched (yielding 3 edits), the pattern is being matched against the wrong node kind — ast-grep should distinguish call expression vs fn declaration. Verify with `ast.root().find_all("log($$$ARGS)")` against just the source in a scratch test; if both match, narrow the pattern (e.g. `log($$$ARGS);` with the trailing statement terminator).

- [ ] **Step 4: Commit**

```bash
git add crates/astedit/tests/fixtures/rewrite_multimatch/ \
        crates/astedit/tests/rewrite_test.rs
git commit -m "test(astedit): multi-metavar \$\$\$ARGS preserves argument list"
```

---

## Task 12: TDD — No match → exit 0 with empty `applied`

**Files:**
- Create: `crates/astedit/tests/fixtures/rewrite_no_match/main.rs`
- Modify: `crates/astedit/tests/rewrite_test.rs`

Confirms the spec's exit-status rule: "0 when the invocation is valid, regardless of match count. Empty `applied` is success."

- [ ] **Step 1: Create the fixture**

```bash
mkdir -p crates/astedit/tests/fixtures/rewrite_no_match
```

Write `crates/astedit/tests/fixtures/rewrite_no_match/main.rs`:

```rust
fn main() {
    let x = 1;
}
```

A pattern looking for `println!($A)` against this file should produce zero matches.

- [ ] **Step 2: Append the failing test**

Append to `crates/astedit/tests/rewrite_test.rs`:

```rust
#[test]
fn rewrite_no_match_exits_zero_with_empty_applied() {
    let tmp = copy_fixture("rewrite_no_match");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rewrite",
        "--pattern", "println!($A)",
        "--rewrite", "eprintln!($A)",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0, "no-match scenario must exit 0; data was: {data}");
    assert_eq!(data["subcommand"], "rewrite");
    assert_eq!(data["dry_run"], true);
    assert!(data["applied"].as_array().unwrap().is_empty(),
        "applied must be empty when no matches; got: {:?}", data["applied"]);
    assert!(data["errors"].as_array().unwrap().is_empty(),
        "errors must be empty when no matches; got: {:?}", data["errors"]);
}
```

- [ ] **Step 3: Run + suite + clippy**

```bash
cargo test -p astedit --test rewrite_test --locked rewrite_no_match_exits_zero_with_empty_applied
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: PASS — Task 5's pipeline already skips files with zero matches (the `if sites.is_empty() { continue; }` line). **80 tests pass.** Clippy clean.

- [ ] **Step 4: Commit**

```bash
git add crates/astedit/tests/fixtures/rewrite_no_match/ \
        crates/astedit/tests/rewrite_test.rs
git commit -m "test(astedit): rewrite with zero matches exits 0 and empty applied"
```

---

## Task 13: TDD — `--lang` filter scopes the walk to one language

**Files:**
- Create: `crates/astedit/tests/fixtures/rewrite_lang_filter/main.rs`
- Create: `crates/astedit/tests/fixtures/rewrite_lang_filter/main.py`
- Modify: `crates/astedit/tests/rewrite_test.rs`

Mixed-language fixture. Pattern matches `doit()` calls in BOTH Rust and Python (the call-expression shape is identical in both grammars). With `--lang rust`, only the Rust file is touched and the Python file is never even compiled — pass 1 of the pipeline restricts to the filtered language.

A universal call pattern is used because the two-pass pipeline compiles the pattern for every language it would scan. A Rust-only pattern (like `println!($A)`) would fail Python's compile in pass 1, abort the run, and exit non-zero — which is correct spec behaviour but wouldn't test the filter.

- [ ] **Step 1: Create the fixture**

```bash
mkdir -p crates/astedit/tests/fixtures/rewrite_lang_filter
```

Write `crates/astedit/tests/fixtures/rewrite_lang_filter/main.rs`:

```rust
fn main() {
    doit();
}

fn doit() {}
```

Write `crates/astedit/tests/fixtures/rewrite_lang_filter/main.py`:

```python
def main():
    doit()

def doit():
    pass
```

Both files contain one call to `doit()` (the `fn doit() {}` declaration and `def doit():` definition are NOT call expressions and should not match).

- [ ] **Step 2: Append two failing tests**

The first test confirms the unfiltered run touches both files (proves the walk covers every language). The second confirms `--lang rust` filters at pass 1 so Python is never compiled, never read, never reported.

```rust
#[test]
fn rewrite_lang_filter_unset_walks_all_languages() {
    let tmp = copy_fixture("rewrite_lang_filter");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rewrite",
        "--pattern", "doit()",
        "--rewrite", "done()",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0, "universal pattern compiles in both langs: data={data}");
    assert!(data["errors"].as_array().unwrap().is_empty(), "errors: {:?}", data["errors"]);

    let applied = data["applied"].as_array().unwrap();
    let rs_present = applied.iter().any(|f| f["file"].as_str().unwrap().ends_with("main.rs"));
    let py_present = applied.iter().any(|f| f["file"].as_str().unwrap().ends_with("main.py"));
    assert!(rs_present, "main.rs missing from applied: {applied:?}");
    assert!(py_present, "main.py missing from applied: {applied:?}");
}

#[test]
fn rewrite_lang_rust_skips_non_rust_files_entirely() {
    let tmp = copy_fixture("rewrite_lang_filter");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rewrite",
        "--pattern", "doit()",
        "--rewrite", "done()",
        "--path", path,
        "--lang", "rust",
        "--json",
    ]);

    assert_eq!(code, 0);
    assert!(data["errors"].as_array().unwrap().is_empty(),
        "errors should be empty — python file was filtered out, not processed: {:?}",
        data["errors"]);

    let applied = data["applied"].as_array().unwrap();
    let files: Vec<&str> = applied.iter().map(|f| f["file"].as_str().unwrap()).collect();
    assert!(files.iter().any(|f| f.ends_with("main.rs")),
        "rust file should be applied: {files:?}");
    assert!(!files.iter().any(|f| f.ends_with("main.py")),
        "python file must not appear when --lang rust filters it out: {files:?}");
}
```

- [ ] **Step 3: Run + suite + clippy**

```bash
cargo test -p astedit --test rewrite_test --locked rewrite_lang
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: PASS for both. **82 tests pass.** Clippy clean.

If the first test fails because `println!($A)` *does* match against the Python file (i.e. ast-grep reports a match where it shouldn't), the per-language dispatch in `rewrite::rewrite_file` is wrong — confirm the language argument is plumbed correctly.

If `--lang rust` produces a `pattern-compile` error for the Python file, the filter check in `commands/rewrite.rs::run` ran AFTER compile rather than BEFORE — the test failure surfaces that ordering bug.

- [ ] **Step 4: Commit**

```bash
git add crates/astedit/tests/fixtures/rewrite_lang_filter/ \
        crates/astedit/tests/rewrite_test.rs
git commit -m "test(astedit): --lang filter scopes walk to one language"
```

---

## Task 14: TDD — Pattern-compile failure → exit non-zero with `error_kind: "pattern-compile"`

**Files:**
- Modify: `crates/astedit/tests/rewrite_test.rs`
- Modify: `crates/astedit/src/error.rs` (only if `#[allow(dead_code)]` removal is needed)

Wires the only previously-unused `AstEditError` variant. Reuses the `rewrite_rust` fixture from Task 5 with an intentionally broken pattern.

- [ ] **Step 1: Append the failing test**

Append to `crates/astedit/tests/rewrite_test.rs`:

```rust
#[test]
fn rewrite_pattern_compile_failure_exits_nonzero_with_error_kind() {
    let tmp = copy_fixture("rewrite_rust");
    let path = tmp.path().to_str().unwrap();

    // `(((` is unbalanced — ast-grep's Pattern::try_new should reject it for
    // every supported language. If a future ast-grep release accepts it,
    // swap for another definitely-invalid pattern (e.g. lone `$` or empty
    // string).
    let (code, data) = run_astedit_json(&[
        "rewrite",
        "--pattern", "(((",
        "--rewrite", "eprintln!($A)",
        "--path", path,
        "--lang", "rust",
        "--json",
    ]);

    assert_ne!(code, 0, "pattern-compile failure must exit non-zero; data was: {data}");
    assert_eq!(data["subcommand"], "rewrite");

    let errors = data["errors"].as_array().expect("errors array");
    let pattern_errs: Vec<&serde_json::Value> = errors
        .iter()
        .filter(|e| e["error_kind"] == "pattern-compile")
        .collect();
    assert!(!pattern_errs.is_empty(),
        "expected at least one pattern-compile error; errors = {errors:?}");

    let err = pattern_errs[0];
    assert_eq!(err["lang"], "rust");
    assert!(err["message"].as_str().unwrap().contains("pattern")
            || err["message"].as_str().unwrap().contains("parse")
            || !err["message"].as_str().unwrap().is_empty(),
        "expected a non-empty pattern-related message: {err:?}");
}
```

- [ ] **Step 2: Run to confirm the test passes (likely)**

```bash
cargo test -p astedit --test rewrite_test --locked rewrite_pattern_compile_failure_exits_nonzero_with_error_kind
```

Expected: PASS. The exit-status logic in Task 5's `run` already returns `2` when `had_pattern_compile_failure` is set, and `rewrite_file` returns `Err(PatternCompile { ... })` for `Pattern::try_new` failures.

If the test fails:
- Exit was 0: the `had_pattern_compile_failure` flag wasn't tripped — re-check the match arm in `commands/rewrite.rs::run`.
- Exit was non-zero but no `pattern-compile` error in `errors[]`: the `From<&AstEditError>` impl for `ErrorEntry` is wrong for the `PatternCompile` variant.
- Test panicked before the assertion: most likely a JSON-parse failure because the stub printed something unexpected to stderr; capture stdout/stderr and inspect.

- [ ] **Step 3: Remove `#[allow(dead_code)]` from `AstEditError` if every variant is now used**

Check whether `ParseError` is still unused:

```bash
grep -rEn "AstEditError::ParseError|ParseError \{" crates/astedit/src/
```

If `ParseError` is still only referenced in the `error::tests` block, leave the `#[allow(dead_code)]` annotation in place on the enum. If every variant is reachable from production code, remove the annotation (line 10 of `error.rs`). PR 3 wires `PatternCompile` but does NOT wire `ParseError` — ast-grep surfaces parse failures as either empty matches or compile failures, not as a separate parse-error code path. So expect to leave the annotation.

If you do remove the annotation and clippy now complains about specific unused variants, re-add a narrow `#[allow(dead_code)]` on just those variants (don't blanket-allow the enum).

- [ ] **Step 4: Run the suite + clippy**

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: **83 tests pass.** Clippy clean.

- [ ] **Step 5: Commit**

```bash
git add crates/astedit/tests/rewrite_test.rs \
        crates/astedit/src/error.rs
git commit -m "feat(astedit): emit pattern-compile error and non-zero exit on invalid pattern"
```

---

## Task 15: TDD — `--apply` writes correctly with race-window guard + atomic write

**Files:**
- Create: `crates/astedit/tests/fixtures/rewrite_apply/main.rs`
- Modify: `crates/astedit/tests/rewrite_test.rs`

End-to-end test that `--apply` actually mutates the file on disk, dry-run leaves it untouched, and `bytes_changed` is the signed total. The race-window guard + atomic write are already exercised by the implementation; this test pins the contract.

- [ ] **Step 1: Create the fixture**

```bash
mkdir -p crates/astedit/tests/fixtures/rewrite_apply
```

Write `crates/astedit/tests/fixtures/rewrite_apply/main.rs`:

```rust
fn main() {
    println!("a");
    println!("bb");
}
```

Pattern `println!($A)` → rewrite `eprintln!($A)` produces +1 byte per edit (`eprintln` is 1 byte longer than `println`). With two matches: bytes_changed = +2.

- [ ] **Step 2: Append the failing test**

Append to `crates/astedit/tests/rewrite_test.rs`:

```rust
#[test]
fn rewrite_apply_writes_changes_to_disk() {
    let tmp = copy_fixture("rewrite_apply");
    let path = tmp.path().to_str().unwrap();
    let target = tmp.path().join("main.rs");
    let before = std::fs::read_to_string(&target).unwrap();
    assert!(before.contains("println!"));

    let (code, data) = run_astedit_json(&[
        "rewrite",
        "--pattern", "println!($A)",
        "--rewrite", "eprintln!($A)",
        "--path", path,
        "--apply",
        "--json",
    ]);

    assert_eq!(code, 0);
    assert_eq!(data["dry_run"], false);
    assert!(data["errors"].as_array().unwrap().is_empty(),
        "no errors expected; got: {:?}", data["errors"]);

    let after = std::fs::read_to_string(&target).unwrap();
    assert!(!after.contains("println!"),  "println! should be gone after apply: {after}");
    assert!(after.contains("eprintln!(\"a\")"),  "first eprintln should be present: {after}");
    assert!(after.contains("eprintln!(\"bb\")"), "second eprintln should be present: {after}");

    // bytes_changed = +1 per edit × 2 edits.
    let applied = data["applied"].as_array().unwrap();
    let entry = applied.iter()
        .find(|f| f["file"].as_str().unwrap().ends_with("main.rs"))
        .unwrap();
    let bytes_changed = entry["bytes_changed"].as_i64().unwrap();
    assert_eq!(bytes_changed, 2, "expected +2 bytes total; got {bytes_changed}");
}
```

- [ ] **Step 3: Run**

```bash
cargo test -p astedit --test rewrite_test --locked rewrite_apply_writes_changes_to_disk
```

Expected: PASS — Task 5's `apply_or_dry_run` handles the `--apply` branch via `crate::apply::write_atomic` (the same helper rename uses).

If the test fails because the file is unchanged after `--apply`, the `if apply { ... }` block in `apply_or_dry_run` isn't being entered — re-check the `args.apply` flag plumbing.

If the test fails because the file is corrupted (e.g. partial write), the byte splicing in reverse order is wrong — confirm `edits.sort_by(|a, b| b.start_byte.cmp(&a.start_byte))` runs before the splice loop.

- [ ] **Step 4: Suite + clippy**

```bash
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: **84 tests pass.** Clippy clean.

Wait — earlier task counts predicted 83 after Task 14. Adding Task 15's test brings the total to 84. Reconcile this against the goal of "11 new tests = 77 total" by recounting:

- 3 serialize unit tests (Task 3)
- 3 rewrite unit tests (Task 4)
- 1 Rust integration (Task 5)
- 1 TypeScript (Task 6)
- 1 TSX (Task 7)
- 1 JavaScript (Task 8)
- 1 Python (Task 9)
- 1 metavar (Task 10)
- 1 multimatch (Task 11)
- 1 no-match (Task 12)
- 2 lang-filter (Task 13)
- 1 pattern-compile (Task 14)
- 1 apply (Task 15)

= 18 new tests, **84 total** (66 baseline + 18 new). That's the corrected baseline going into Task 17. The handoff's "~10 rewrite integration tests" target maps to the 12 integration tests added in Tasks 5–15 (plus the 6 supporting unit tests).

- [ ] **Step 5: Commit**

```bash
git add crates/astedit/tests/fixtures/rewrite_apply/ \
        crates/astedit/tests/rewrite_test.rs
git commit -m "feat(astedit): --apply performs atomic byte splicing for structural rewrites"
```

---

## Task 16: Update `SKILL.md` to document the `rewrite` subcommand

**Files:**
- Modify: `skills/ny-astedit/SKILL.md`

PR 2 landed the SKILL.md with rename-only content. PR 3 extends the discovery hook (so agents surface `astedit rewrite` for "structural codemod" requests) and adds a `rewrite` section under the commands list.

- [ ] **Step 1: Read the current SKILL.md**

```bash
cat skills/ny-astedit/SKILL.md
```

Familiarise with the structure (frontmatter description, when-to-use, run, subcommand, safety, JSON envelope, out-of-scope).

- [ ] **Step 2: Edit the frontmatter `description:` field**

Append a sentence to the `description:` field so the skill discovery hook covers rewrite. The new field should end with a final period after the additions; the exact prepended/appended phrases (English + Thai) below should be merged into the existing sentence stream rather than replacing it:

Add these trigger phrases (in addition to whatever's already there):

> "Also use `astedit rewrite --pattern P --rewrite R` for structural codemods — ast-grep pattern syntax with `$X` and `$$$ARGS` metavars, dry-run by default. Trigger BEFORE running ad-hoc `sed`, hand-coded ast-grep CLI invocations, or chained `Edit` calls that match an AST shape rather than a single identifier. Also use when the user says 'rewrite all calls to X with Y', 'apply this codemod', 'replace pattern P with R', 'เขียนใหม่ทุก call ของ X', 'แก้ pattern P ทั่วโปรเจกต์', 'apply a structural change across N files'."

Keep the result as one long single-line `description:` value (no embedded newlines — YAML's plain scalars don't tolerate them inside a single mapping value the way our PR 2 SKILL.md is shaped).

- [ ] **Step 3: Add a `Subcommand: rewrite` section**

Below the existing `## Subcommand: rename` section (and before the `## Safety model` section), insert:

```markdown
## Subcommand: `rewrite`

```
astedit rewrite --pattern P --rewrite R  [--path DIR]  [--apply]
                                         [--json]    [--lang LANG]
```

Structural pattern→rewrite using ast-grep syntax. Unlike `rename`, this does not consult `codegraph` — every match is an AST-shape exact match, so every edit is implicitly high-confidence. The JSON envelope's `applied[].edits[]` omits `confidence` and `reason` fields accordingly.

- `--pattern P` — ast-grep pattern. Metavars: `$X` (single node), `$$$X` (multiple).
- `--rewrite R` — replacement template; metavars from `--pattern` are substituted in.
- `--path DIR` — project root to scan (default: current directory).
- `--apply` — actually write edits. Without this flag, astedit reports what it *would* do.
- `--json` — emit `{schema_version:1, data:…}` instead of human-readable preview.
- `--lang LANG` — restrict to one language (`rust`, `typescript`, `tsx`, `javascript`, `python`). Without it, every supported file extension is scanned.

If `--pattern` or `--rewrite` fails to compile for any language scanned, astedit exits non-zero and the JSON envelope's `errors[]` carries an entry with `error_kind: "pattern-compile"` and `lang: "<language>"`. Other failure modes (concurrent writes, atomic-write errors) are reported in `errors[]` but **do not** abort the run — sed-like semantics.
```

- [ ] **Step 4: Update the "Out of scope" section**

Replace the line:

```markdown
- `astedit rewrite --pattern P --rewrite R` — coming in PR 3.
```

with:

```markdown
- `astedit rewrite --pattern P --rewrite R` — landed in PR 3 (this skill).
```

If you want to be more conservative, just delete the line entirely (it's a stale "future work" pointer once shipped).

- [ ] **Step 5: Rebuild the skill binary**

```bash
./scripts/build-skills.sh
```

Expected: `done: 3 skill binary(ies)` with a line for `skills/ny-astedit/scripts/astedit`.

- [ ] **Step 6: Smoke-test the installed binary**

```bash
./skills/ny-astedit/scripts/astedit --help
./skills/ny-astedit/scripts/astedit rewrite --pattern 'foo' --rewrite 'bar' --json --path /tmp
```

Expected: `--help` lists both `rename` and `rewrite`. The `rewrite` invocation on `/tmp` (no source code) emits an empty envelope and exits 0.

- [ ] **Step 7: Commit**

```bash
git add skills/ny-astedit/SKILL.md
git commit -m "docs(skills): document astedit rewrite subcommand"
```

---

## Task 17: Final regression — fmt + clippy + test + cargo tree -d + open PR

**Files:** none modified (verification gate + PR).

- [ ] **Step 1: Format check**

```bash
cargo fmt --all --check
```

If anything is unformatted, run `cargo fmt --all` and either amend the most recent commit or land a separate `style: cargo fmt --all` commit (PR 2 used the latter — match it).

- [ ] **Step 2: Clippy with the CI gate**

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: clean. Zero new `#[allow(...)]` annotations unless justified inline.

- [ ] **Step 3: Full test suite**

```bash
cargo test --workspace --locked
```

Expected count: **84 tests pass** (66 baseline from PR 2 + 18 new in PR 3). The 18 new tests break down as:

- 3 serialize unit tests (Task 3)
- 3 rewrite-helper unit tests (Task 4)
- 12 rewrite integration tests (Tasks 5–15)

If the codemap / codegraph / codegraph-core counts changed without a matching test edit, the tree-sitter bump from Task 1 moved a parse boundary — revert and investigate before pushing.

- [ ] **Step 4: Audit for `tree-sitter` duplication**

```bash
cargo tree -d
```

Expected: no `tree-sitter` line listed as a duplicate inside any single dep graph. The same audit ran at Task 1 Step 12; this is the final pre-PR gate. If duplicates re-emerged (e.g. because `cargo update -w` was re-run between tasks), redo the version reconciliation from Task 1 Step 5.

- [ ] **Step 5: Build the release artifacts**

```bash
./scripts/build-skills.sh
```

Expected: `done: 3 skill binary(ies)` with `skills/ny-astedit/scripts/astedit` present and executable.

- [ ] **Step 6: Smoke test the rewrite binary against this repo's own source**

```bash
# Run rewrite in dry-run mode against this repo. Use a pattern that's unlikely
# to match much — confirms the binary doesn't panic on real-world Rust source.
./skills/ny-astedit/scripts/astedit rewrite \
  --pattern 'todo!()' \
  --rewrite 'unimplemented!()' \
  --json \
  --path crates/
```

Expected: exit 0, valid JSON envelope, `applied` may be empty (this codebase likely has zero `todo!()` calls). No panics, no parse errors flooding `errors[]`.

- [ ] **Step 7: Push the branch**

```bash
git push -u origin feat/astedit-rewrite
```

- [ ] **Step 8: Open the PR**

```bash
gh pr create --base main --head feat/astedit-rewrite \
  --title "feat(astedit): structural rewrite subcommand (astedit PR 3/3)" \
  --body "$(cat <<'EOF'
## Summary

Ships the `astedit rewrite --pattern P --rewrite R` subcommand — the third and final PR of the astedit rollout. PR 1 landed `codegraph-core`; PR 2 landed `astedit rename`; this PR completes the trio.

- New `Rewrite(RewriteArgs)` variant on the existing CLI. Uses `codegraph_core::walk::walk_sources` for file iteration (no `build_index`, no resolver) and ast-grep's `Pattern` + `Fixer` machinery for matching and replacement template expansion.
- Reuses PR 2's atomic-write + race-window-guard machinery (`astedit::apply::{write_atomic, current_len}`); same dry-run-by-default safety model, same `error_kind` JSON envelope contract.
- Wires `AstEditError::PatternCompile` — the variant PR 2 declared but never produced. Compilation failure on any scanned language ⇒ exit non-zero with `error_kind: "pattern-compile"` per spec.
- `RewriteEdit` deliberately omits `confidence` / `reason` (spec § Output schema: rename-only fields; structural matches are AST-shape exact ⇒ implicit `high`).
- `--lang` filter scopes the walk to one of `rust | typescript | tsx | javascript | python` before any per-language compile attempt.
- **Bumps `codegraph-core`'s `tree-sitter` family to 0.25-compatible versions** to coexist with `ast-grep-core 0.38.7`'s `tree-sitter ^0.25.4` requirement. PR 2 deferred this; the bump finally lands here. `codegraph-core`'s 17 tests continue to pass; `codegraph`'s 18 tests pass transitively. `codemap` is untouched (it does not depend on `codegraph-core`, so its own tree-sitter 0.22 graph stays isolated).

No `codegraph` (binary) source changes. No `codemap` source changes. No edits to PR 2's `commands/rename.rs` or `tests/rename_test.rs`.

## Test plan
- [x] `cargo fmt --all --check`
- [x] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [x] `cargo test --workspace --locked` — **84 tests pass** (66 baseline + 18 new)
- [x] `cargo tree -d` — no `tree-sitter` duplication inside astedit's dep graph
- [x] `./scripts/build-skills.sh` produces 3 skill binaries (codemap, codegraph, astedit)
- [x] `./skills/ny-astedit/scripts/astedit rewrite --pattern 'todo!()' --rewrite 'unimplemented!()' --json --path crates/` returns a v1 envelope and exits 0

Spec: `docs/superpowers/specs/2026-05-21-astedit-design.md`
Plan: `docs/superpowers/plans/2026-05-22-astedit-pr3-rewrite.md`
PR 1: #2  ·  PR 2: #3
EOF
)"
```

Expected: PR URL printed. CI runs the same fmt + clippy + test gates; should be green on the first run unless `cargo tree -d` resolves differently under `--locked` than locally.

- [ ] **Step 9: Mark this plan complete**

After merge, leave the plan file in place. The 3-PR astedit rollout is shipped; the spec's "Future work" list (recipe files, `--max-passes`, parallel apply, type-aware rename) becomes the candidate backlog for any follow-up work.
