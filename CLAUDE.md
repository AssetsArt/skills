# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

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
