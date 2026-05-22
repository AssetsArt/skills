# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Codebase exploration: prefer the local skills over ad-hoc `grep`/`find`/`ls`

This repo ships three CLI skills: **`codemap`** + **`codegraph`** for read-only orientation, and **`astedit`** for write-side rewrites. The exploration table below covers the read-only pair; see "Write-side rewrites" further down for `astedit`.

**Default to the read-only pair** before reaching for `grep`/`rg`/`find`/`ls`/`tree`/`fd`. They return structured results (file + line + symbol kind + confidence), which is faster to reason about and cheaper to feed back into prompts than text matches.

| Question | Use | Not |
| --- | --- | --- |
| "What's in this repo?" / "list source files" | `./scripts/codemap files --json` or `codemap tree` | `ls -R`, `find . -name '*.rs'`, `tree` |
| "Where is `X` defined?" | `codemap find X --exact --json` | `grep -rn "fn X\|class X\|struct X"` |
| "What top-level symbols does this file have?" | `codemap symbols src/foo.rs --json` | `grep -E "^(pub )?(fn|struct|class)"` |
| "Where else is `X` used?" / "who calls Y?" | `codegraph find-refs X --json` / `codegraph callers Y --json` | `grep -rn X`, `rg X`, `find . \| xargs grep` |
| "What does `Z` call?" | `codegraph callees Z --json` | reading file by file |
| "What breaks if I change `W`?" | `codegraph impact W --json` | guessing, then grepping |

When to bypass and reach for `grep`/`rg` anyway:
- Searching for prose, comments, log lines, error messages, regex literals, config keys — anything that isn't a named code symbol.
- Searching in a language the skills don't support (current set: Rust, TypeScript, TSX, JavaScript, Python).
- Counting raw occurrences of a substring, not call/reference relationships.

All three skills accept `--json`; assert `result.schema_version === 1` and read `result.data`. The pre-built binaries live at `./skills/ny-codemap/scripts/codemap`, `./skills/ny-codegraph/scripts/codegraph`, and `./skills/ny-astedit/scripts/astedit` in this repo; from agent contexts elsewhere they install to `~/.claude/skills/ny-<name>/scripts/<name>`. If a binary is missing, run `./scripts/build-skills.sh` (local) or `./scripts/install.sh` (release tarball).

## Write-side rewrites: use `astedit` instead of `sed` / chained `Edit` calls

`astedit` is the write-side companion to `codegraph`. Default to it before running multi-file `sed -i`, hand-rolled ast-grep CLI, or chained `Edit` calls that touch the same identifier or AST shape across files. Dry-run by default; `--apply` opts into atomic per-file writes.

| Question | Use | Not |
| --- | --- | --- |
| "Rename X to Y across the project" | `astedit rename X Y --json --path .` (inspect dry-run) → re-run with `--apply` | `sed -i s/X/Y/g`, `rg -l X \| xargs sed`, chained `Edit` calls |
| "Rewrite every `print(...)` call to `log.info(...)`" (structural pattern) | `astedit rewrite --pattern 'print($A)' --rewrite 'log.info($A)' --json --path .` → `--apply` | hand-rolled `ast-grep` CLI, `sed` on AST-shaped patterns |
| "Rename collides with multiple definitions" | `astedit rename X Y --json` → reads `needs_anchor: true` → re-run with `--anchor FILE:LINE` | guessing which `X` the user means |

Safety properties (preserve if you touch the pipeline):
- **Dry-run default, `--apply` opt-in.** No file writes without `--apply`.
- **Atomic per-file writes.** Temp file in the same directory + `rename(2)`. No partial writes on crash.
- **Length-based drift detection.** Pre-flight stats compare against the index snapshot; mismatch falls back to SHA-256. Persistent mismatch ⇒ `error_kind: "hash-mismatch"`, skip the file.
- **Race-window guard.** Re-stat length immediately before each `rename(2)`. Mismatch ⇒ `error_kind: "concurrent-write"`, skip.
- **JSON envelope is spec-locked.** `error_kind` is a closed enum of six kebab-case strings; the rewrite path also requires `confidence`/`reason` to be **omitted** from `applied[].edits[]` (structural matches are AST-shape exact ⇒ implicit `high`).

## Project shape

A Cargo workspace (`resolver = "2"`, `members = ["crates/*"]`) producing a family of small CLI tools intended to be invoked by AI coding agents. The workspace deliberately couples two parallel directories:

- `crates/<name>/` — Rust source for the binary.
- `skills/ny-<name>/` — the agent-discoverable surface: a `SKILL.md` manifest plus a `scripts/<name>` binary copied in by the build/install scripts.

**The mapping `skill_dir == "ny-" + crate_name` is load-bearing.** `scripts/build-skills.sh`, `scripts/install.sh`, and `.github/workflows/release.yml` all derive paths from it. Renaming a crate or dropping the `ny-` prefix on a skill dir will silently break installs and release packaging. The release workflow's "Audit crate/skill pairs" step explicitly errors if any `crates/<name>` lacks a matching `skills/ny-<name>`.

`skills/*/scripts/` is `.gitignored` — those binaries are build artifacts, not checked-in code.

Shared metadata (`version`, `license`, `authors`, repo URLs, common deps) lives in `[workspace.package]` / `[workspace.dependencies]` in the root `Cargo.toml`. Crates pull these in with `version.workspace = true`, `clap = { workspace = true }`, etc. All skills bump versions together by design; see README "Versioning" section before considering per-crate versioning.

## Commands

Toolchain is pinned to stable with `rustfmt` + `clippy` via `rust-toolchain.toml` — rustup will install/switch automatically.

```bash
# Build every skill binary and stage it into skills/ny-<name>/scripts/<name>
./scripts/build-skills.sh

# CI parity checks (these must pass — CI runs with -Dwarnings)
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

# Run one crate's tests
cargo test -p codemap

# Run one integration test file (matches the file stem under tests/)
cargo test -p codemap --test find_test

# Run a single test by name
cargo test -p codemap --test symbols_test -- --exact some_test_name

# Run a skill end-to-end after building
./skills/ny-codemap/scripts/codemap stats --json --path /some/repo
```

`RUSTFLAGS=-Dwarnings` is set in CI but not locally — warnings will compile fine on your machine and then fail clippy in CI. Run the clippy command above before pushing.

## Architecture notes for `codemap` (template for future skills)

The `crates/codemap` binary is the reference layout new skills should mirror:

- `src/main.rs` is a thin dispatcher — `Cli::parse()` → match on `Command` → delegate to `commands::<sub>::run(args)`.
- `src/cli.rs` defines all clap structs. Every subcommand takes `--path <DIR>` (default `.`) and `--json`.
- `src/commands/<sub>.rs` is one file per subcommand. Add a new subcommand by adding a variant to `Command`, an `Args` struct, a `commands/<sub>.rs`, and a `mod` line in `commands/mod.rs`.
- `src/output.rs` wraps every JSON response in `{"schema_version": 1, "data": ...}`. **All JSON output must route through `print_json`** — agents are documented to read `result.data` and assert `schema_version === 1`, so bypassing the envelope is a contract break.
- `src/lang.rs` is the language registry. `Language::from_extension`, `ts_language`, and `query_source` must all stay in sync — adding a language is exactly: variant + extension mapping + `tree-sitter-<lang>` dep in `Cargo.toml` + `queries/<lang>.scm` file + `include_str!` line.
- `src/queries/*.scm` are tree-sitter S-expression queries. Captures use the `symbol.<kind>` convention (`@symbol.fn`, `@symbol.struct`, ...) — `SymbolKind::from_capture_suffix` parses the suffix back into a `SymbolKind`. Adding a kind requires touching both the enum and every query file.
- `src/walk.rs` uses the `ignore` crate to respect `.gitignore` during traversal. Reuse it rather than rolling new directory walks.

Integration tests under `crates/codemap/tests/` operate on `tests/fixtures/sample_project/`. When you add a new language or symbol kind, extend the fixture and add cases to the relevant `*_test.rs` rather than creating ad-hoc temp dirs.

## Architecture notes for `astedit`

`crates/astedit` is the only **bin + lib** crate in the workspace. The `[lib]` exists so integration tests can call `astedit::apply::check_drift` directly (drift races are hard to reproduce through the CLI without a debug back-door). Layout:

- `src/main.rs` — thin dispatcher; `Cli::parse()` → match on `Command::{Rename, Rewrite}` → delegate to `commands::<sub>::run`.
- `src/cli.rs` — clap structs (`Cli`, `Command`, `RenameArgs`, `RewriteArgs`). Both subcommands share `--path`, `--apply`, `--json`, `--lang`.
- `src/commands/{rename,rewrite}.rs` — pipeline orchestration only. `rename` builds a `codegraph_core::Index`, resolves references, splices bytes. `rewrite` skips the index entirely — walks via `codegraph_core::walk::walk_sources`, compiles ast-grep `Pattern` + `TemplateFix` per language, applies edits.
- `src/rewrite.rs` — the **single import site** for `ast_grep_core::*` and `ast_grep_language::*`. `rewrite_file(source, pattern, rewrite, lang) -> Result<Vec<RewriteSite>, AstEditError>` is the only API surface; `commands/rewrite.rs` calls it and never touches ast-grep types directly.
- `src/apply.rs` — atomic writes + drift detection. `write_atomic`, `check_drift`, `current_len` are shared between `rename` and `rewrite`. **Do not bypass these helpers** with `std::fs::write` — atomicity + the race-window guard are load-bearing.
- `src/error.rs` — `AstEditError` enum with six variants serialized via `kind() -> &'static str` to spec-locked kebab-case strings: `parse-error`, `hash-mismatch`, `concurrent-write`, `node-kind-mismatch`, `write-failed`, `pattern-compile`. **The set is closed** — extending it is a spec-level change; route new failure modes into the closest existing variant.
- `src/serialize.rs` — parallel envelope structs: `RenameData`/`AppliedFile`/`AppliedEdit` (carries `confidence` + `reason`) vs. `RewriteData`/`RewriteAppliedFile`/`RewriteEdit` (omits both — structural matches are AST-shape exact). The split is deliberate; do NOT merge them into a generic envelope.
- `src/output.rs` — re-uses the same `{schema_version: 1, data: ...}` wrapper as codemap/codegraph.

The **rewrite pipeline is two-pass on purpose** (`commands/rewrite.rs::run`): pass 1 compiles `(pattern, rewrite)` for every language present in the walk; if ANY language fails to compile, the run aborts before any file is read or written. Pass 2 only runs when pass 1 was clean. This matches the spec's "Compilation failure on any selected language ⇒ exit non-zero" wording strictly. Don't collapse into a single pass — files processed before a compile failure would otherwise be applied.

Integration tests under `crates/astedit/tests/` use the shared `tests/common/mod.rs` helper (`copy_fixture`, `run_astedit_json`). Each fixture lives in `tests/fixtures/<name>/`; reuse fixtures across tests when the source content is the same (Task 14's pattern-compile test reuses Task 5's `rewrite_rust/` fixture).

Workspace constraint: `ast-grep-core 0.38.7` requires `tree-sitter ^0.25.4`. `codegraph-core` and `codemap` both bumped to the `tree-sitter 0.25` family for this — Cargo's `links = "tree-sitter"` is workspace-wide, not per-graph, so sibling crates can't straddle versions even when their dependency graphs don't overlap directly.

## Architecture notes for `codegraph`

`crates/codegraph` extends the `codemap` template with semantic cross-references. Layout differences worth knowing before editing:

- Three query files per language (`<lang>_defs.scm`, `<lang>_imports.scm`, `<lang>_refs.scm`) — not one. `Language::query_source` returns `Option<&'static str>` keyed on a `QueryKind` enum.
- `src/index.rs` owns the three in-memory tables (`Definition`, `Import`, `Reference`) and the `build_index` builder. Every subcommand starts by calling `build_index(&path)`.
- `src/resolve.rs` is the confidence/reason tagger. Cross-file matching is heuristic — see `module_matches` for the per-language path rules. When you add a new language, extend that function alongside the three `.scm` files.
- Subcommands live in `src/commands/{find_refs,callers,callees,impact}.rs`. `callers`/`callees`/`impact` use BFS bounded by `HARD_CAP = 8`. `--depth` is clamped against that.
- Output schema is the same `{schema_version: 1, data: [...]}` envelope; the entry shape adds `confidence` and `reason`. Documented in `skills/ny-codegraph/SKILL.md` — keep the two in sync.

## Adding a new skill (checklist beyond the README)

The README covers the high-level steps. Things that will bite if missed:

1. The new crate's `Cargo.toml` should inherit shared metadata (`version.workspace = true`, `license.workspace = true`, ...) — don't hard-code versions, the workspace bumps them together.
2. Declare a `[[bin]]` with `name = "<name>"` matching the crate name. `build-skills.sh` does `cp target/release/$name skills/ny-$name/scripts/$name` — anything else breaks it.
3. The `SKILL.md` frontmatter `name:` must be `ny-<name>`, and the `description:` is the agent-discovery hook — write trigger phrases (including non-English ones if relevant) so agents actually surface it.
4. Reference the binary in `SKILL.md` as `./scripts/<name>` — it's the same path whether the user got it from a release tarball or `build-skills.sh`.
5. No release workflow changes needed — the matrix is per-target, not per-skill, and the audit step will catch a missing `skills/ny-<name>` dir.

## Release pipeline

Triggered by `v*` tags. The matrix builds 6 targets (linux gnu/musl × x86_64/aarch64, macOS x86_64/aarch64); linux-aarch64 and both musl targets go through `cross` (pinned to `CROSS_PINNED_TAG`), the others through plain `cargo`. Each crate/skill pair is tarred separately as `<crate>-<tag>-<slug>.tar.gz` with a paired `.sha256`. `install.sh` verifies the checksum and rejects tarballs containing absolute paths or `..` segments — preserve those guards if you touch the install or release scripts.
