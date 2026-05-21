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
- Re-export following — `pub use foo::Bar` is rewritten at the re-export
  point but downstream callers are matched the usual way.
- Cross-language semantic linking — Rust `User` and TypeScript `User` are not
  treated as the same identifier.
- Multi-pass rewrites — one invocation, one pass. No fixpoint iteration.
- Interactive mode — no prompts. Agent-first tool.
- Persistent cache — every invocation re-parses the project, same as
  `codegraph`.
- Refactor primitives beyond rename / structural rewrite (extract function,
  inline, change-signature). Out of scope for MVP.

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
definition / import / reference tables, store a `file_hashes: HashMap<PathBuf,
[u8; 32]>` populated during `build_index` (SHA-256 of the file bytes the
parser saw). `codegraph`'s existing subcommands ignore the new field; it
exists purely so `astedit` can validate at apply time that the file on disk
still matches what the resolver was reasoning about. This is the only change
to `Index` in PR 1 — everything else is a pure move.

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
astedit rename <OLD> <NEW>             [--path DIR] [--dry-run] [--json]
                                       [--lang LANG] [--anchor FILE:LINE]
astedit rewrite --pattern P --rewrite R [--path DIR] [--dry-run] [--json]
                                        [--lang LANG]
```

Defaults:

- `--path .`
- **Apply by default** — files are written. `--dry-run` opts out and prints a
  unified diff only.
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
     a. Re-read file, compare SHA-256 with the contents that fed the index.
        Mismatch → push entry to `errors`, skip file.
     b. Parse with ast-grep, look up identifier nodes that match
        (line, col) from each reference.
     c. If the node kind at (line, col) is not an identifier matching
        <OLD>, push to `errors`, skip that edit (others in the file can
        still apply).
     d. Apply byte-level rewrites in reverse order of position.
     e. Write atomically: temp file in the same directory + rename(2).
6. Emit envelope.
```

The hash check in step 5a is the apply-time safety belt: if a file changed
between `build_index` and `apply`, the resolver's byte positions may no
longer be valid, so we refuse to touch that file.

## Structural rewrite pipeline

```
1. codegraph_core::walk(--path) → file iterator (respects .gitignore)
2. If --lang LANG → filter to extensions of LANG.
   Otherwise → per file, derive lang from extension, skip files whose lang
   doesn't compile the pattern.
3. Per file:
     a. Compile (pattern, rewrite) for the file's language via
        ast-grep-config.
     b. Re-read file, hash check (same as rename step 5a).
     c. ast-grep Matcher → list of matches.
     d. ast-grep Rewriter → byte edits in reverse order.
     e. Atomic write.
4. Emit envelope.
```

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
    "dry_run": false,
    "applied": [
      {
        "file": "src/lib.rs",
        "edits": [
          {
            "line": 42,
            "col": 12,
            "old": "User",
            "new": "Account",
            "confidence": "high",
            "reason": "same-file-scope"
          }
        ]
      }
    ],
    "skipped": [
      {
        "file": "tests/util.rs",
        "line": 7,
        "col": 4,
        "name": "User",
        "confidence": "low",
        "reason": "name-only",
        "skip_reason": "low-confidence"
      }
    ],
    "errors": [
      {
        "file": "src/broken.rs",
        "error": "parse error at line 12"
      }
    ]
  }
}
```

Notes:

- `confidence` / `reason` fields are present only for `rename`. `rewrite`
  edits omit them.
- `dry_run: true` ⇒ `applied` is populated with would-be edits but files are
  not touched.
- `skipped` is the "look here too" lane — never causes a non-zero exit, but
  the agent should treat it as candidate work.
- `errors` is non-fatal as long as at least one file applied successfully
  (sed-like behaviour). Exit non-zero only when **every** file failed or
  when the invocation itself was invalid (rename without anchor on a
  multi-def symbol, parse failure on the pattern for `rewrite`).

## Safety / apply model

- **Apply by default.** Writes files in place. The user already trusts git
  to track changes; we don't shadow that with `.bak` files (matches
  `codegraph`/`codemap` convention of "trust the user, trust git").
- **`--dry-run`** opts out of writes. Both human and JSON modes still emit
  the full `applied` list — it just describes would-be edits.
- **Atomic writes.** Temp file in the same directory, then `rename(2)`. No
  partial writes if the process is killed mid-apply.
- **Hash check.** Before applying edits to a file, re-read it and compare
  SHA-256 against the bytes that fed the index. Mismatch ⇒ skip the file,
  log an `errors` entry. Prevents corruption when the user edits a file
  between two phases.
- **No git tree check.** We don't refuse to run on a dirty tree. Agent + git
  is the accountability layer; adding the check would only annoy normal
  use.

## Build / release plumbing

Two changes in **PR 1** so that `codegraph-core` can ship as a library-only
member without breaking the existing pipeline:

- `scripts/build-skills.sh` — guard the
  `cp target/release/$name skills/ny-$name/scripts/$name` step on the
  existence of `skills/ny-$name/SKILL.md`. Lib-only crates produce no binary
  so there is nothing to copy. Skip silently.
- `.github/workflows/release.yml` "Audit crate/skill pairs" step — currently
  errors if any `crates/<name>` lacks a matching `skills/ny-<name>`.
  Change: parse the crate's `Cargo.toml`, skip the audit when the crate has
  no `[[bin]]` section. (Alternative considered: explicit allowlist file
  `scripts/lib-only-crates.txt`. Rejected — `[[bin]]` is the ground truth;
  an allowlist would drift.)

The release matrix itself is per-target, not per-skill, so no further
changes are needed. `codegraph-core` ships as a transitive dependency of the
`codegraph` and `astedit` binaries.

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

**PR 1 regression.** All 28 existing tests across `codemap` and `codegraph`
must remain green without modification. Any forced test edit during the
core extract is a signal that the extract has a behaviour change and should
be revisited.

## Rollout — three PRs

1. **PR 1: extract `codegraph-core`.** Move the four source files and the
   `queries/` tree, add `lib.rs`, swap `crate::*` imports in
   `codegraph/src/commands/*.rs` to `codegraph_core::*`. Add the
   `Index.file_hashes` field (SHA-256 per parsed file) — the only additive
   change in this PR; existing `codegraph` subcommands ignore it. Teach
   `build-skills.sh` and `release.yml` audit. **No observable behaviour
   change.** Ship when the existing 28 tests pass unchanged.
2. **PR 2: `astedit` MVP — rename only.** New `crates/astedit/` and
   `skills/ny-astedit/`, `rename` subcommand, full safety model
   (atomic + hash check), JSON envelope. SKILL.md description follows the
   `codemap`/`codegraph` "PREFER THIS over …" template so agents discover
   it.
3. **PR 3: `astedit rewrite`.** Add `rewrite` subcommand to the existing
   binary. Pass-through `ast-grep` pattern syntax. Per-language ext
   inference. New tests in `rewrite_test.rs`.

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
