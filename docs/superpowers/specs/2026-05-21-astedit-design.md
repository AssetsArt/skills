# astedit Design

**Status:** approved 2026-05-21 (post-brainstorm, post-2× subagent review)
**Author:** AssetsArt
**Implements:** a new `astedit` skill — the write-side counterpart to `codemap`
(survey) and `codegraph` (cross-references). Performs AST-aware code edits so
an agent can rename symbols and apply structural rewrites without composing
regex over identifier matches.

## Goal

Two operations cover ~95% of refactor work an agent needs:

- **`astedit rename <OLD> <NEW>`** — project-wide symbol rename. Reuses
  `codegraph`'s reference resolver; rewrites `high` and `medium` confidence
  hits, reports `low` hits without touching them.
- **`astedit rewrite --pattern P --rewrite R`** — structural search-replace
  using `ast-grep` pattern syntax pass-through (metavars `$X`, `$$$ARGS`,
  etc.).

Both subcommands return a `{schema_version: 1, data: …}` envelope so the same
agent contract that powers `codemap` and `codegraph` extends to writes.

## Non-goals (MVP)

- Type-aware semantic rename — `rust-analyzer` / `tsserver` territory.
- Macro expansion — `println!` is treated as a reference to `println`, body
  not walked.
- Cross-language semantic linking — Rust `User` and TypeScript `User` are not
  treated as the same identifier.
- Multi-pass rewrites — one invocation, one pass. No fixpoint iteration.
- Interactive mode — no prompts. Agent-first tool.
- Persistent cache — every invocation re-parses the project, same as
  `codegraph`.
- Refactor primitives beyond rename / structural rewrite (extract function,
  inline, change-signature). Out of scope for MVP.

### Re-exports: limited support, not a non-goal

Direct re-exports (`pub use foo::Bar` in Rust, `export { Bar } from "./foo"`
in TS, `from foo import Bar` followed by `__all__ = ["Bar"]` in Python) are
treated like any other reference: the resolver sees the re-export site and
`rename Bar Baz` will rewrite it. Downstream callers then match via the
normal import-resolution rules and are also rewritten.

**Alias re-exports** introduce a new local name whose definition the
resolver does not know:

- Rust: `pub use foo::Bar as Baz;`
- TypeScript: `export { Bar as Baz } from "./foo";`
- Python: `from foo import Bar as Baz` (re-exposed via `__all__`)

PR 1 detects these during the import-indexing pass and records them in
`Index.alias_reexports`. At rename time, if `<OLD>` is the target of an
alias re-export anywhere in the project, `astedit` surfaces every such site
in `skipped` with `skip_reason: "re-export-alias"` and a `via_alias: "Baz"`
field, so the agent knows to either re-invoke `astedit rename Baz NewBaz`
separately or inspect manually.

**Glob/wildcard re-exports** (`pub use foo::*`, `export * from "./foo"`,
`from foo import *` followed by `__all__ = ["Bar"]`) are even fuzzier — the
resolver cannot know which symbols actually cross the boundary without
parsing the target module's exports. PR 1 detects these during indexing
and records them in `Index.wildcard_reexports`. At rename time, if a
matched reference traverses a wildcard re-export, `astedit` surfaces it in
`skipped` with `skip_reason: "wildcard-reexport"` so the agent can audit
manually. Same treatment for all three languages.

Detecting both kinds of re-export is a straightforward extension to the
existing `<lang>_imports.scm` queries and lives in PR 1 alongside the core
extract.

## Engine choice: `ast-grep` crate

Embedded as a Rust library (`ast-grep-core`, `ast-grep-config`,
`ast-grep-language`). Reasons:

- Native Rust — no extra runtime to bundle, matches the workspace.
- Same language coverage as `codemap`/`codegraph` (Rust, TypeScript, TSX,
  JavaScript, Python).
- Battle-tested pattern language with metavariables, kind matchers, and
  relational matchers. Reusing it costs nothing and inherits documentation
  the agent already understands.
- `ast-grep`'s `Rewriter` does AST-validated splicing — defense in depth
  against position drift between index build and apply.

## Rust API conventions (apply throughout `codegraph-core` and `astedit`)

Decisions locked before any code is written, to avoid signature-breaking
fixes after the API has callers:

- **`Index` is `#[non_exhaustive]`.** Field additions (`file_meta`,
  `alias_reexports`, `wildcard_reexports`) must not break external
  struct-literal construction. Apply the attribute in PR 1 *before* the
  field move so the lib crate ships with it from the first commit.
  `Index::default()` is the only supported constructor; all existing
  construction inside `codegraph` already goes through it
  (`crates/codegraph/src/index.rs:113`).
- **Library errors via `thiserror`, not `anyhow`.** `codegraph-core` is a
  library — callers want pattern-matchable error variants. PR 1 adds
  `thiserror = "1"` to `[workspace.dependencies]` and introduces
  `pub enum CoreError`. `astedit` defines its own
  `pub enum AstEditError` that maps 1-to-1 to the six JSON `error_kind`
  strings via `#[derive(thiserror::Error)]` + `#[serde(rename_all =
  "kebab-case")]` on the corresponding serializable companion. Binaries
  may still use `anyhow` at the top level for ergonomic error chaining.
- **Path arguments take `&Path`** (not `impl AsRef<Path>`). Matches the
  existing workspace convention (`build_index(root: &Path)` at
  `crates/codegraph/src/index.rs:112`). Diverging would be inconsistent
  for no real ergonomic win — callers in this workspace pass `&Path` or
  `&PathBuf` already.
- **Path keys in `Index` are `String`, not `PathBuf`.** `Definition.file`,
  `Reference.file`, etc. already use `String` (repo-relative,
  forward-slash-normalized via `strip_prefix` + `to_string_lossy` —
  `crates/codegraph/src/index.rs:119–124`). New fields keep the same
  representation:
  `file_meta: HashMap<String, FileMeta>`,
  `alias_reexports: HashMap<String, Vec<AliasSite>>`,
  `wildcard_reexports: HashMap<String, Vec<WildcardSite>>`.
- **File hashes are a newtype, not a raw byte array.**
  `pub struct FileHash([u8; 32]);` with `impl fmt::Display` (lowercase hex
  via the `hex` crate), `impl PartialEq + Eq + Hash`, and `impl AsRef<[u8]>`.
  Prevents scattered `hex::encode` calls and makes `Debug` output legible.
- **SHA-256 via `sha2`.** Pure Rust, MIT/Apache, no C dep, ships cleanly to
  all six release targets (including `aarch64-unknown-linux-musl`). PR 1
  adds `sha2 = "0.10"` and `hex = "0.4"` to `[workspace.dependencies]`.
  `blake3` rejected: faster but less audit-tool support, and SHA-256 is
  already fast enough for the file sizes we care about.
- **MSRV floor.** Add `rust-version = "1.74"` to `[workspace.package]` so
  the `ast-grep` crates (which require 1.74) cannot silently bump the
  effective minimum. `rust-toolchain.toml` already pins `stable`, which
  is well above 1.74.
- **`ast-grep-*` versions pinned exact** (`= "0.38.x"` style, not caret).
  The crate ships breaking changes in minor versions. PR 2 audits
  `cargo tree -d` to catch `tree-sitter` symbol-collision with
  `codegraph-core`'s grammar deps.

## Architecture: three crates after rollout

```
crates/
  codegraph-core/        ← NEW (library only)
    src/lib.rs
    src/{index,resolve,lang,walk,hash,error}.rs
    src/queries/*.scm
  codegraph/             ← KEEP (binary)
    src/{main,cli,output}.rs
    src/commands/*.rs    ← `crate::index::*` → `codegraph_core::*`
  astedit/               ← NEW (binary)
    src/{main,cli,output,error}.rs
    src/commands/{rename,rewrite}.rs
    src/apply.rs         ← atomic write + drift guard
```

`codegraph-core` exposes `Index`, `Reference`, `Confidence`, `ResolveReason`,
`build_index`, `resolve_refs`, `Language`, the language registry, the
ignore-aware walker, `FileHash`, `compute_file_hash`, and `CoreError`.
No `[[bin]]` section. Inherits workspace metadata.

### Additive changes to `Index` in PR 1

```rust
#[non_exhaustive]
pub struct Index {
    // ...existing fields...
    pub file_meta: HashMap<String, FileMeta>,
    pub alias_reexports: HashMap<String, Vec<AliasSite>>,
    pub wildcard_reexports: HashMap<String, Vec<WildcardSite>>,
}

#[non_exhaustive]
pub struct FileMeta {
    pub len: u64,
}

#[non_exhaustive]
pub struct AliasSite {
    pub file: String,           // repo-relative path of the re-export
    pub line: usize,            // 1-based
    pub alias: String,          // local name introduced (e.g. "Baz")
    pub original: String,       // symbol being aliased (e.g. "Bar")
    pub module_path: String,    // source module ("foo" in `use foo::Bar as Baz`)
}

#[non_exhaustive]
pub struct WildcardSite {
    pub file: String,
    pub line: usize,
    pub module_path: String,    // source module of the glob
}
```

`FileMeta` carries only `len` — not `mtime`. Cross-filesystem mtime
resolution (HFS+ 1s, APFS/ext4 ns, VFAT/SMB coarse) makes mtime an
unreliable drift signal; storing it would force a SHA-256 fallback storm on
any large rewrite touching files whose mtime got rounded by a recent
`tar`/`git checkout`. Length is unforgeable enough as a fast pre-flight; on
suspected drift (length differs) we fall back to `compute_file_hash`.

`FileMeta` is `#[non_exhaustive]` so we can add `mtime`, `ino`, etc. later
without breaking external construction.

### New helper

```rust
pub fn compute_file_hash(path: &Path) -> Result<FileHash, CoreError>;
```

Standalone free function. Streams the file through SHA-256 (64 KiB buffer)
to avoid allocating the full content. Not invoked eagerly during
`build_index` — only called on demand by `astedit` when length-based
drift suggests we need the slow path.

**`astedit` memoizes the result per (path, FileHash) within a single
invocation.** Step 5a's drift check and step 5e's race-window check share
the cache, so each file is hashed at most once per `astedit` run.

### `astedit/Cargo.toml`

```toml
[dependencies]
codegraph-core    = { path = "../codegraph-core" }
ast-grep-core     = { workspace = true }   # pinned exact in workspace
ast-grep-config   = { workspace = true }
ast-grep-language = { workspace = true }
anyhow            = { workspace = true }
clap              = { workspace = true }
serde             = { workspace = true }
serde_json        = { workspace = true }
thiserror         = { workspace = true }
tempfile          = { workspace = true }   # also used by tests
```

`[workspace.dependencies]` gains `ast-grep-*` (exact-pinned), `thiserror`,
`sha2`, and `hex` during PR 1 (`sha2`/`hex`) and PR 2 (`ast-grep-*`,
`thiserror`). The plan pins concrete versions.

## CLI surface

```
astedit rename <OLD> <NEW>             [--path DIR] [--apply] [--json]
                                       [--lang LANG] [--anchor FILE:LINE]
astedit rewrite --pattern P --rewrite R [--path DIR] [--apply] [--json]
                                        [--lang LANG]
```

Defaults:

- `--path .`
- **Dry-run by default** — files are NOT written. `--apply` opts in.
  Rationale: agents chain tool calls without inspecting diffs between
  them; a runaway pattern that wrote 40 files before the agent looked
  would be worse than the extra round-trip of a preview→apply cycle.
  Hash-checking at apply time does not protect against the agent's own
  previous write.
- Human-readable output by default (unified diff per file + summary).
  `--json` swaps to the `{schema_version: 1, data: …}` envelope.
- `--lang` is optional. Without it the walker scans every supported
  extension and dispatches per file.
- `--anchor FILE:LINE` is `rename`-only and disambiguates when more than
  one definition of `<OLD>` exists.
- **No built-in change-count cap.** Agents are trusted to budget their own
  preview→apply cycles. A 200-file rename is legitimate; a hard cap would
  become muscle-memory-bypassed and degrade into noise. See "Exit status"
  in the Output schema section for the full list of non-zero conditions.

## Rename pipeline

```
1. codegraph_core::build_index(--path) → Index
2. defs = index.find_defs(<OLD>)
     0  → exit 0 with empty `applied`
     1  → use as anchor
     >1 → if --anchor present, pick matching; else emit
          {needs_anchor: true, candidates: [...]} envelope, exit non-zero
3. refs = codegraph_core::resolve_refs(index, def)
4. Partition refs by confidence and re-export kind:
     - traverses wildcard re-export → skipped, skip_reason: wildcard-reexport
     - traverses alias re-export    → skipped, skip_reason: re-export-alias
     - High | Medium                → queue for edit
     - Low                          → skipped, skip_reason: low-confidence
5. Group queued edits by file. Per file:
     a. Pre-flight: `stat` the file, compare `len` against
        `index.file_meta[path].len`. Mismatch ⇒ `compute_file_hash` and
        compare against the on-demand hash from build_index time; if
        still mismatched (or the file is gone), push
        `errors{kind: "hash-mismatch"}` and skip the file.
     b. Parse with ast-grep, look up identifier nodes that match
        (line, col) from each reference.
     c. If the node kind at (line, col) is not an identifier matching
        <OLD>, push `errors{kind: "node-kind-mismatch"}` and skip that
        edit. Other edits in the same file can still apply.
     d. Compute byte-level rewrites in reverse order of position.
     e. (only when `--apply`) Race-window guard: just before the write,
        re-`stat` the file and compare `len` against step 5a's value.
        Mismatch ⇒ push `errors{kind: "concurrent-write"}` and skip the
        file. Same-length concurrent edits slip through this guard
        (accepted: rare, and git is the final accountability layer).
     f. Atomic write: temp file in the same directory + `rename(2)`.
        On filesystem errors push `errors{kind: "write-failed", os_code,
        message}` and continue with other files.
6. Emit envelope.
```

Steps 5b–5d run in dry-run too; only 5e–5f are gated on `--apply`. Dry-run
output therefore reflects what `--apply` *would* do, modulo step 5e's
race-window guard which can only fire when racing a real write.

## Structural rewrite pipeline

```
1. codegraph_core::walk(--path) → file iterator (respects .gitignore)
2. If --lang LANG → filter to extensions of LANG.
   Otherwise → per file, derive lang from extension, skip files whose lang
   doesn't compile the pattern.
3. Per file (steps 5e/5f from rename apply only when --apply):
     a. Compile (pattern, rewrite) for the file's language via
        ast-grep-config. Compilation failure on any selected language ⇒
        exit non-zero with `error_kind: "pattern-compile"`.
     b. Read file. No drift check — file content read here IS the source
        of truth (no prior index).
     c. ast-grep Matcher → list of matches.
     d. ast-grep Rewriter → byte edits in reverse order.
     e. Race-window guard before write (same len-only check as rename 5e).
     f. Atomic write (same as rename 5f).
4. Emit envelope.
```

`rewrite` does not build a `codegraph_core::Index`; it only needs the walker
and language registry. Faster than `rename` for large repos because the
resolver step is skipped.

Structural matches don't carry `confidence` / `reason` — they are AST-shape
exact, so every match is treated as `high`.

## Output schema

Always wrapped in `{"schema_version": 1, "data": …}`. `data` is an object
(not an array) because we need three lanes:

```json
{
  "schema_version": 1,
  "data": {
    "subcommand": "rename",
    "dry_run": true,
    "applied": [
      {
        "file": "src/lib.rs",
        "bytes_changed": 28,
        "edits": [
          {
            "line": 42, "col": 12,
            "start_byte": 1023, "end_byte": 1027,
            "old": "User", "new": "Account",
            "confidence": "high",
            "reason": "same-file-scope"
          }
        ]
      }
    ],
    "skipped": [
      {
        "file": "tests/util.rs",
        "line": 7, "col": 4,
        "start_byte": 142, "end_byte": 146,
        "name": "User",
        "confidence": "low",
        "reason": "name-only",
        "skip_reason": "low-confidence"
      }
    ],
    "errors": [
      {
        "file": "src/broken.rs",
        "error_kind": "parse-error",
        "message": "expected `;` at line 12"
      }
    ]
  }
}
```

### Field reference

- **`subcommand`** — `"rename"` or `"rewrite"`.
- **`dry_run`** — `true` unless `--apply` was passed.
- **`applied[].file`** — repo-relative, forward-slash-normalized path.
- **`applied[].bytes_changed`** — sum of `(new.len() - old.len())` across
  edits in this file (signed). Useful for change-budget reasoning.
- **`applied[].edits[].line` / `.col`** — 1-based, human-friendly.
- **`applied[].edits[].start_byte` / `.end_byte`** — 0-based half-open byte
  range of the *original* text being replaced. Authoritative for tooling.
- **`applied[].edits[].confidence` / `.reason`** — `rename` only, omitted
  for `rewrite` (every match is AST-shape exact ⇒ implicit `high`).
- **`skipped[]`** — same shape as an edit plus `skip_reason`. Known
  `skip_reason` values:
  - `"low-confidence"` — resolver `Low` tier; agent should inspect
  - `"re-export-alias"` — reference traverses `pub use X as Y` style
    alias; carries an extra `via_alias` field
  - `"wildcard-reexport"` — reference traverses `pub use foo::*` etc.;
    carries an extra `via_module` field
- **`errors[].error_kind`** — closed enum (Rust `AstEditError` variants
  serialize to these via `#[serde(rename_all = "kebab-case")]`):
  - `"parse-error"` — tree-sitter couldn't parse the file
  - `"hash-mismatch"` — file changed between index build and apply
  - `"concurrent-write"` — race window between pre-flight and write
  - `"node-kind-mismatch"` — node at (line,col) wasn't the identifier we
    expected (defensive; should not normally happen)
  - `"write-failed"` — filesystem error during atomic rename. Carries
    additional `os_code: Option<i32>` (from `io::Error::raw_os_error`) and
    `message: String` (from `io::Error::to_string`) fields. The
    `io::Error` itself is not embedded because it's not serializable.
  - `"pattern-compile"` — `rewrite` only; pattern failed to compile

### Multi-def disambiguation (rename exit non-zero)

When `rename` is invoked on a symbol with multiple definitions and no
`--anchor`, the envelope wraps a `candidates` array instead of
`applied`/`skipped`:

```json
{
  "schema_version": 1,
  "data": {
    "subcommand": "rename",
    "needs_anchor": true,
    "candidates": [
      { "file": "src/user.rs",    "line": 12, "kind": "struct" },
      { "file": "src/account.rs", "line":  8, "kind": "fn"     }
    ]
  }
}
```

Agent uses one of those `file:line` values for `--anchor` on the retry.

### Exit status — single source of truth

- `0` when the invocation is valid, regardless of match count. Empty
  `applied` is success, matching `codemap`/`codegraph` convention.
- Non-zero only when:
  - `rename` on multi-def symbol without `--anchor` (`needs_anchor: true`).
  - `rewrite` pattern fails to compile in any selected language.
  - Every targeted file ended up in `errors` (no successful apply).

`errors` entries are non-fatal as long as at least one file applied
successfully (sed-like).

## Safety / apply model

- **Dry-run by default.** No writes unless `--apply` is passed. Both human
  and JSON modes always emit a full `applied` list describing would-be
  edits, so the agent can inspect a structured preview before committing.
  `codegraph`/`codemap` are read-only, so their "trust git" stance doesn't
  transfer to a tool that writes.
- **Atomic per-file writes.** Temp file in the same directory, then
  `rename(2)`. No partial writes if the process is killed. Per-file
  atomicity means a failure on file 7 of 12 leaves files 1–6 written
  (matches `git apply` semantics). Each failure is reported in `errors`
  so the agent can retry just the failing files.
- **Length-based drift detection.** Pre-flight (step 5a) compares the
  current file length against `Index.file_meta[path].len`. Mismatch ⇒
  fall back to `compute_file_hash` and compare against the on-demand
  hash from index time. Mismatch persists ⇒ `error_kind:
  "hash-mismatch"`, skip the file. Length is unforgeable enough as a
  fast first cut; storing mtime was rejected because cross-filesystem
  resolution differences would cause a hash-fallback storm on benign
  `tar`/`git checkout` activity.
- **Race-window guard (step 5e).** Re-`stat` len just before the
  `rename(2)`. Mismatch ⇒ `error_kind: "concurrent-write"`, skip.
  Same-length concurrent writes slip through (accepted as a rare
  trade-off; git is the final accountability layer).
- **No `.bak` files, no git tree check.** Agent + git remains the
  accountability layer for the writes that `--apply` does perform. `.bak`
  shadow files would clutter the tree.
- **No built-in change-count cap.** A 200-file rename of a project-wide
  util is legitimate. A hard cap would become muscle-memory-bypassed.

## Build / release plumbing

**Invert the iteration**, in PR 1, for both the local build script and the
release workflow: walk `skills/ny-*/` (the things we actually ship) instead
of `crates/*/` (which now includes lib-only members). This keeps the
existing invariant "skill_dir == 'ny-' + crate_name" load-bearing for
binaries while making lib-only crates structurally invisible to release
plumbing.

Concrete changes:

- `scripts/build-skills.sh` — replace `for crate in crates/*/` with
  `for skill in skills/ny-*/`, derive `name=${skill#skills/ny-}`, build
  `cargo build -p "$name" --release`, then
  `cp target/release/$name skills/ny-$name/scripts/$name`. Lib-only
  crates have no matching skill dir, so the loop never visits them.
- `.github/workflows/release.yml` — apply the same inversion to **both**
  loops: the "Audit crate/skill pairs" step (currently errors when
  `crates/<name>` lacks `skills/ny-<name>`) and the tarball-packaging
  step that runs `tar -C "target/$triple/release" -czf … "$name"` per
  crate.
- **New separate audit step**, added in PR 1, that walks `crates/*/` and
  fails if any crate has a `[[bin]]` section but no matching
  `skills/ny-<name>/SKILL.md`. This preserves the old "you forgot a
  skill dir" guarantee that the inverted loops alone would lose.
  `codegraph-core` has no `[[bin]]`, so it's exempt naturally — no
  explicit allowlist needed.

The release matrix itself is per-target, not per-skill, so no further
changes are needed. `codegraph-core` ships as a transitive dependency of
the `codegraph` and `astedit` binaries.

## Testing strategy

`crates/astedit/tests/` mirrors `codemap`/`codegraph`. Two integration test
files:

- `rename_test.rs`
  - same-file scope (high confidence) — fixture defines and uses `User`
    in the same Rust file
  - cross-file via explicit import — Python `from foo import User`
  - glob/wildcard import — Rust `use foo::*` and Python `from foo import
    *` yield medium confidence
  - name-only in unrelated file (low) → ends up in `skipped`, file not
    touched
  - multi-def disambiguation — two defs of `User`, no `--anchor` ⇒
    non-zero exit + candidate list
  - `--dry-run` (default) writes no files, but envelope shows `applied`
  - file-changed-between-index-and-apply → `errors{hash-mismatch}` entry,
    other files still apply
  - alias re-export → `skipped{re-export-alias}`
  - wildcard re-export → `skipped{wildcard-reexport}`
- `rewrite_test.rs`
  - one pattern per supported language (Rust, TS, TSX, JS, Python)
  - metavar capture (`$X` and `$$$ARGS`)
  - no-match → exit 0, empty `applied`
  - `--lang` filter scopes to one language

**Fixture reuse with care.** Tests share
`crates/codegraph/tests/fixtures/sample_project/` but `astedit` writes to
disk, so every test copies the fixture into a `tempfile::TempDir` and
operates on the copy. A small helper `tests/common/copy_fixture(name) ->
TempDir` lives in `crates/astedit/tests/common/mod.rs`.

**Snapshot assertions.** Use `assert_eq!` on parsed JSON values rather than
adding `insta` to the workspace. Lower dep, less ceremony, and the spec
already pins the schema.

**PR 1 regression.** All 31 existing tests (13 in `codemap`, 18 in
`codegraph`) must remain green without modification. Any forced test edit
during the core extract is a signal that the extract has a behaviour
change and should be revisited.

## Rollout — three PRs

1. **PR 1: extract `codegraph-core`.** Move `index.rs`, `resolve.rs`,
   `lang.rs`, `walk.rs`, and `queries/*.scm` into the new crate; add
   `lib.rs`, `hash.rs`, `error.rs`. Swap `crate::*` imports in
   `codegraph/src/commands/*.rs` to `codegraph_core::*`. Mark
   `Index`, `FileMeta`, `AliasSite`, `WildcardSite` `#[non_exhaustive]`
   from the first commit.

   Additive surfaces in core (all part of the same PR, all guarded by
   new unit tests):
   - `Index.file_meta: HashMap<String, FileMeta { len: u64 }>` — populated
     during the existing `stat` pass; no separate IO.
   - `Index.alias_reexports: HashMap<String, Vec<AliasSite>>` and
     `Index.wildcard_reexports: HashMap<String, Vec<WildcardSite>>` —
     populated from extended `<lang>_imports.scm` queries.
   - `pub struct FileHash([u8; 32])` newtype with `Display` (hex) +
     `AsRef<[u8]>`.
   - `pub fn compute_file_hash(path: &Path) -> Result<FileHash, CoreError>`
     — standalone helper, streams via SHA-256 (64 KiB buffer).
   - `pub enum CoreError` via `thiserror`.

   New workspace deps in PR 1: `sha2 = "0.10"`, `hex = "0.4"`,
   `thiserror = "1"`. New `rust-version = "1.74"` floor in
   `[workspace.package]`.

   Build/release plumbing changes: invert iteration in
   `scripts/build-skills.sh` and both loops in
   `.github/workflows/release.yml`; add the new `[[bin]]`-without-skill
   audit step.

   **No observable behaviour change** for existing `codegraph` / `codemap`
   invocations. Ship when all 31 existing tests pass unchanged AND new
   unit tests for `compute_file_hash` + alias/wildcard re-export detection
   pass.

2. **PR 2: `astedit` MVP — rename only.** New `crates/astedit/` and
   `skills/ny-astedit/`, `rename` subcommand, full safety model (dry-run
   default, atomic write, length-based drift detection with hash
   fallback, race-window guard), JSON envelope, `AstEditError` enum
   serializing to the six `error_kind` strings. Adds `ast-grep-core`,
   `ast-grep-config`, `ast-grep-language` to `[workspace.dependencies]`
   pinned exact. SKILL.md description follows the `codemap`/`codegraph`
   "PREFER THIS over …" template so agents discover it. `cargo tree -d`
   audit confirms no `tree-sitter` symbol collision before merge.

   **`ast-grep-*` deps are unused by `rename` directly** (rename does its
   own byte splicing on identifier nodes located via the codegraph
   resolver). They're added in PR 2 anyway because PR 3 follows
   immediately; splitting the dep addition adds review churn for no
   safety win. If PR 3 slips >2 weeks, re-evaluate and cfg-gate.

3. **PR 3: `astedit rewrite`.** Add `rewrite` subcommand to the existing
   binary. Pass-through `ast-grep` pattern syntax. Per-language ext
   inference. New tests in `rewrite_test.rs`. No new top-level deps —
   PR 2 already added the `ast-grep-*` family.

## Future work (not in MVP, listed for context)

- Recipe files (`astedit apply recipe.yaml`) — would let a project ship a
  codemod bundle for contributors to run.
- `--anchor` shorthand resolved via `codegraph find-defs` so the agent
  can pipe one tool into the other.
- Multi-pass mode for `rewrite` (`--max-passes N`) — useful when rewrites
  cascade.
- Owned `Resolved` (replacing `&'a Reference` borrowing) — current
  `Resolved<'a>` (`crates/codegraph/src/resolve.rs:38`) ties future
  parallelization to the `Index` lifetime. Not blocking the sequential
  MVP, but worth revisiting if `astedit` ever wants per-file parallel
  apply.
- `extract-function`, `inline`, `change-signature` primitives — need real
  type information; revisit only if rust-analyzer / tsserver embeddings
  become realistic.
