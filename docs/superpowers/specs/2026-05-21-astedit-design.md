# astedit Design

**Status:** approved 2026-05-21 (post-brainstorm)
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

Rust `pub use foo::Bar` (direct re-export) is treated like any other
reference: the resolver sees the re-export site and `rename Bar Baz` will
rewrite it. The downstream-caller side then matches via the normal
import-resolution rules and is also rewritten.

**Alias re-exports are the failure case** — `pub use foo::Bar as Baz`
introduces a new name `Baz` whose definition the resolver does not know.
PR 1's index build detects alias re-exports during the import-indexing
pass and records them in a new `Index.alias_reexports` table. At rename
time, if `<OLD>` is the target of an alias re-export anywhere in the
project, `astedit` surfaces every such site in `skipped` with
`skip_reason: "re-export-alias"` and a `via_alias: "Baz"` field, so the
agent knows to either re-invoke `astedit rename Baz NewBaz` separately or
manually inspect.

Same treatment for TypeScript `export { Bar as Baz } from "./foo"` and
Python `from foo import Bar as Baz`. Detecting alias re-exports is a
straightforward extension to the existing `<lang>_imports.scm` queries
and lives in PR 1 alongside the core extract.

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

## Architecture: three crates after rollout

```
crates/
  codegraph-core/        ← NEW (library only)
    src/lib.rs
    src/{index,resolve,lang,walk}.rs
    src/queries/*.scm
  codegraph/             ← KEEP (binary)
    src/{main,cli,output}.rs
    src/commands/*.rs    ← `crate::index::*` → `codegraph_core::*`
  astedit/               ← NEW (binary)
    src/{main,cli,output}.rs
    src/commands/{rename,rewrite}.rs
    src/apply.rs         ← atomic write + hash check
```

`codegraph-core` exposes `Index`, `Reference`, `Confidence`, `ResolveReason`,
`build_index`, `resolve_refs`, `Language`, the language registry, and the
ignore-aware walker. No `[[bin]]` section. Inherits workspace metadata.

**One additive change to `Index` in PR 1**: alongside the existing
definition / import / reference tables, store
`file_meta: HashMap<PathBuf, FileMeta>` where
`FileMeta { len: u64, mtime: SystemTime }`. Cheap to populate during
`build_index` (already `stat`-ing files). SHA-256 is **not** stored eagerly
— `codegraph`'s read-only subcommands don't need it, and hashing every
parsed file on each invocation would tax large repos.

Instead, expose `codegraph_core::compute_file_hash(path: &Path) -> io::Result<[u8; 32]>`
as a free function. `astedit` calls it on demand for the files it is about
to touch and compares against a fresh hash at apply time. The fast path
(rename a few hits) hashes only a few files; the slow path (project-wide
rewrite) hashes all touched files but that cost is unavoidable for safety.

`file_meta` lets `astedit` do a cheap pre-flight check (mtime+len drift since
index build) before paying the SHA-256 cost. This is the only change to
`Index` in PR 1 — everything else is a pure move.

`astedit/Cargo.toml` (concrete versions pinned in the implementation plan;
the shape is):

```toml
[dependencies]
codegraph-core   = { path = "../codegraph-core" }
ast-grep-core    = { workspace = true }
ast-grep-config  = { workspace = true }
ast-grep-language = { workspace = true }
anyhow           = { workspace = true }
clap             = { workspace = true }
serde            = { workspace = true }
serde_json       = { workspace = true }
tempfile         = { workspace = true }   # also used by tests
```

The three `ast-grep-*` crates are added to `[workspace.dependencies]` in the
root `Cargo.toml` during PR 2 so every workspace member that needs them
inherits the same version. The plan pins concrete versions; this spec
intentionally leaves them open since `ast-grep` ships frequently.

## CLI surface

```
astedit rename <OLD> <NEW>             [--path DIR] [--apply] [--json]
                                       [--lang LANG] [--anchor FILE:LINE]
                                       [--max-changes N]
astedit rewrite --pattern P --rewrite R [--path DIR] [--apply] [--json]
                                        [--lang LANG] [--max-changes N]
```

Defaults:

- `--path .`
- **Dry-run by default** — files are NOT written. `--apply` opts in. Rationale:
  agents chain tool calls without inspecting diffs between them. A bad
  pattern that writes 40 files before the agent looks is worse than the
  extra round-trip of a preview→apply cycle. Matches `codemod` / `comby` /
  `jscodeshift -d` convention. Hash-checking at apply time does not protect
  against the agent's own previous write.
- `--max-changes N` is a circuit breaker (default `200`). If the preview
  would touch more than `N` files, `--apply` refuses and prints the file
  count. Override explicitly with a larger `--max-changes`. Dry-run is
  unaffected.
- Human-readable output by default (unified diff per file + summary).
  `--json` swaps to the `{schema_version: 1, data: …}` envelope.
- `--lang` is optional. Without it the walker scans every supported extension
  and dispatches per file.
- `--anchor FILE:LINE` is `rename`-only and disambiguates when more than one
  definition of `<OLD>` exists.
- **Exit status** (single source of truth — see also Output schema):
  - `0` when the invocation is valid, regardless of whether anything
    matched. Empty `applied` is success, matching the
    `codemap`/`codegraph` convention.
  - Non-zero only when the invocation itself was invalid:
    - `rename` called on a multi-def symbol without `--anchor`
    - `rewrite` pattern fails to compile in any selected language
    - Every targeted file ended up in `errors` (no successful apply).

## Rename pipeline

```
1. codegraph_core::build_index(--path) → Index
2. defs = index.find_defs(<OLD>)
     0  → exit 0 with empty `applied`
     1  → use as anchor
     >1 → if --anchor present, pick matching; else print candidates,
          exit non-zero
3. refs = codegraph_core::resolve_refs(index, def)
4. Partition refs by confidence:
     High | Medium → queue for edit
     Low           → skipped list (reported, not written)
5. Group queued edits by file. Per file:
     a. Pre-flight: `stat` the file, compare `(len, mtime)` against
        `index.file_meta[path]`. Cheap drift detector. Mismatch
        → SHA-256 the file and compare against
        `codegraph_core::compute_file_hash` invoked at index time; if
        still mismatched, push `errors{kind: "hash-mismatch"}` and skip
        the file.
     b. Parse with ast-grep, look up identifier nodes that match
        (line, col) from each reference.
     c. If the node kind at (line, col) is not an identifier matching
        <OLD>, push `errors{kind: "node-kind-mismatch"}` and skip that
        edit (others in the file can still apply).
     d. Apply byte-level rewrites in reverse order of position.
     e. (only when `--apply`) Race-window guard: just before the write,
        re-`stat` the file and compare against the `(len, mtime, ino)`
        observed in step 5a. Mismatch ⇒ another process touched the
        file between read and write — push `errors{kind:
        "concurrent-write"}` and skip.
     f. Atomic write: temp file in the same directory + `rename(2)`.
        On filesystem errors (read-only mount, perms) push
        `errors{kind: "write-failed", os_error: ...}` and continue with
        other files.
6. Emit envelope.
```

Steps 5b–5d run in dry-run too; only 5e–5f are gated on `--apply`. Dry-run
output therefore reflects what `--apply` *would* do, modulo step 5e's
race-window check which can only fire when racing a real write.

The hash check in step 5a is the apply-time safety belt: if a file changed
between `build_index` and `apply`, the resolver's byte positions may no
longer be valid, so we refuse to touch that file.

## Structural rewrite pipeline

```
1. codegraph_core::walk(--path) → file iterator (respects .gitignore)
2. If --lang LANG → filter to extensions of LANG.
   Otherwise → per file, derive lang from extension, skip files whose lang
   doesn't compile the pattern.
3. Per file (apply steps 5e/5f from rename only when --apply):
     a. Compile (pattern, rewrite) for the file's language via
        ast-grep-config. Compilation failure on first selected file ⇒
        exit non-zero with `error_kind: "pattern-compile"`.
     b. Read file. Skip drift check (no prior index — file content read
        here IS the source of truth).
     c. ast-grep Matcher → list of matches.
     d. ast-grep Rewriter → byte edits in reverse order.
     e. Race-window guard before write (same as rename 5e).
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
- **`applied[].file`** — repo-relative path.
- **`applied[].bytes_changed`** — sum of `(new.len() - old.len())` across
  edits in this file (signed). Useful for change-budget reasoning.
- **`applied[].edits[].line` / `.col`** — 1-based, human-friendly.
- **`applied[].edits[].start_byte` / `.end_byte`** — 0-based half-open byte
  range of the *original* text being replaced. Authoritative for tooling.
- **`applied[].edits[].confidence` / `.reason`** — `rename` only, omitted
  for `rewrite` (every match is AST-shape exact ⇒ implicit `high`).
- **`skipped[]`** — same shape as an edit plus `skip_reason`. Known
  `skip_reason` values: `"low-confidence"`, `"re-export-alias"`,
  `"max-changes-exceeded"`.
- **`errors[].error_kind`** — closed enum so agents can branch without
  regex-parsing prose. Known kinds:
  - `"parse-error"` — tree-sitter couldn't parse the file
  - `"hash-mismatch"` — file changed between index build and apply
  - `"concurrent-write"` — race window between pre-flight and write
  - `"node-kind-mismatch"` — node at (line,col) wasn't the identifier we
    expected (defensive — shouldn't normally happen)
  - `"write-failed"` — filesystem error during atomic rename (includes
    `os_error` field)
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
  - `rename` on multi-def symbol without `--anchor` (`needs_anchor: true`)
  - `rewrite` pattern fails to compile in any selected language
  - Every targeted file ended up in `errors` (no successful apply)
  - `--apply` was requested but `--max-changes` would be exceeded

`errors` entries are non-fatal as long as at least one file applied
successfully (sed-like).

## Safety / apply model

- **Dry-run by default.** No writes unless `--apply` is passed. Both human
  and JSON modes always emit a full `applied` list describing would-be
  edits, so the agent can inspect a structured preview before committing.
  Architect's argument that won this: agents chain tool calls without
  inspecting diffs between them; a runaway pattern that wrote 40 files
  before the agent looked at anything would be worse than the extra
  round-trip of preview→apply. `codegraph`/`codemap` are read-only so
  their "trust git" stance doesn't transfer.
- **`--apply` opts in to writes.** Combine with `--max-changes N` (default
  200) as a circuit breaker — `--apply` refuses if the preview would
  exceed `N` files and prints the count instead. Dry-run is unaffected
  by `--max-changes`.
- **Atomic per-file writes.** Temp file in the same directory, then
  `rename(2)`. No partial writes if the process is killed. Per-file
  atomicity means a fail on file 7 of 12 leaves files 1–6 written
  (correct, matching `git apply` semantics). Each failure is reported in
  `errors` so the agent can retry just the failing files.
- **Two-layer drift detection.** (a) Pre-flight `(len, mtime)` check
  against the snapshot `Index.file_meta` taken at `build_index` time —
  cheap, catches most drift. (b) On suspected drift, fall back to
  SHA-256 to disambiguate clock skew from real change. Mismatch ⇒
  `error_kind: "hash-mismatch"`, skip the file.
- **Race-window guard.** Between pre-flight (5a) and atomic write (5f),
  re-`stat` the file just before the rename. If `(len, mtime, ino)` has
  moved, emit `error_kind: "concurrent-write"` and skip. Closes the
  TOCTOU window the architect flagged.
- **No `.bak` files, no git tree check.** Agent + git remains the
  accountability layer for the writes that `--apply` does perform.
  `.bak` shadow files would just clutter the tree.

## Build / release plumbing

**Invert the iteration**, in PR 1, for both the local build script and the
release workflow: walk `skills/ny-*/` (the things we actually ship) instead
of `crates/*/` (which now includes lib-only members). This keeps the
existing invariant "skill_dir == 'ny-' + crate_name" load-bearing for
things that produce binaries, while making lib-only crates structurally
invisible to release plumbing.

Concrete changes:

- `scripts/build-skills.sh` — replace the `for crate in crates/*/` loop
  with `for skill in skills/ny-*/`, derive `name=${skill#skills/ny-}`,
  build `cargo build -p "$name" --release`, then
  `cp target/release/$name skills/ny-$name/scripts/$name`. lib-only
  crates have no matching skill dir, so nothing iterates over them.
- `.github/workflows/release.yml` — apply the same inversion to **both**
  loops the architect flagged: the "Audit crate/skill pairs" step
  (currently errors when a `crates/<name>` lacks `skills/ny-<name>`) and
  the tarball-packaging step that runs
  `tar -C "target/$triple/release" -czf … "$name"` per crate. After
  inversion, both walk `skills/ny-*/` and the missing-binary failure mode
  disappears because lib-only crates are not in the iteration set.

(Alternative considered: keep the crates iteration and gate each step on
`grep -q '^\[\[bin\]\]' "$c/Cargo.toml"`. Rejected — inverting the loop is
structurally cleaner and removes a second source of truth.)

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
  - glob/wildcard import — Rust `use foo::*` and Python `from foo import *`
    yield medium confidence
  - name-only in unrelated file (low) → ends up in `skipped`, file not
    touched
  - multi-def disambiguation — two defs of `User`, no `--anchor` ⇒
    non-zero exit + candidate list
  - `--dry-run` writes no files, but envelope shows `applied`
  - file-changed-between-index-and-apply → `errors` entry, other files
    still apply
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
during the core extract is a signal that the extract has a behaviour change
and should be revisited.

## Rollout — three PRs

1. **PR 1: extract `codegraph-core`.** Move `index.rs`, `resolve.rs`,
   `lang.rs`, `walk.rs`, and `queries/*.scm` into the new crate; add
   `lib.rs`. Swap `crate::*` imports in `codegraph/src/commands/*.rs` to
   `codegraph_core::*`. Additive changes (all in core):
   - `Index.file_meta: HashMap<PathBuf, FileMeta { len, mtime }>` — cheap
     drift snapshot, populated during the existing `stat` pass.
   - `Index.alias_reexports: HashMap<String, Vec<AliasSite>>` — populated
     from extended `<lang>_imports.scm` queries.
   - `pub fn compute_file_hash(path: &Path) -> io::Result<[u8; 32]>` —
     standalone helper, not eagerly invoked.

   Invert the iteration in `scripts/build-skills.sh` and both loops in
   `.github/workflows/release.yml` from `crates/*/` to `skills/ny-*/`.
   **No observable behaviour change** for existing `codegraph` /
   `codemap` invocations. Ship when all 31 existing tests pass unchanged
   AND new unit tests for `compute_file_hash` + alias-reexport detection
   pass.
2. **PR 2: `astedit` MVP — rename only.** New `crates/astedit/` and
   `skills/ny-astedit/`, `rename` subcommand, full safety model
   (dry-run default, atomic write, two-layer drift detection, race-window
   guard, `--max-changes` circuit breaker), JSON envelope. Adds
   `ast-grep-core`, `ast-grep-config`, `ast-grep-language` to
   `[workspace.dependencies]`. SKILL.md description follows the
   `codemap`/`codegraph` "PREFER THIS over …" template so agents
   discover it.

   **`ast-grep-*` deps are unused by `rename` directly** (rename does its
   own byte splicing on identifier nodes). They're added in PR 2 anyway
   because PR 3 follows immediately and splitting the dep addition
   adds review churn. The architect flagged this as a Medium concern;
   accepted as a deliberate trade-off. If PR 3 is delayed >2 weeks,
   re-evaluate and cfg-gate.
3. **PR 3: `astedit rewrite`.** Add `rewrite` subcommand to the existing
   binary. Pass-through `ast-grep` pattern syntax. Per-language ext
   inference. New tests in `rewrite_test.rs`. No new top-level deps —
   PR 2 already added the `ast-grep-*` family.

## Future work (not in MVP, listed for context)

- Recipe files (`astedit apply recipe.yaml`) — would let a project ship a
  codemod bundle for contributors to run.
- `--anchor` shorthand resolved via `codegraph find-defs` so the agent can
  pipe one tool into the other.
- Multi-pass mode for `rewrite` (`--max-passes N`) — useful when rewrites
  cascade.
- `extract-function`, `inline`, `change-signature` primitives — these need
  real type information; revisit only if rust-analyzer / tsserver
  embeddings become realistic.
