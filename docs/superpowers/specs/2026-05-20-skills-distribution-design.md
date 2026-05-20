# Skills Distribution Design

**Status:** approved 2026-05-20
**Author:** AssetsArt
**Implements:** moving Rust source out of `skills/` and shipping pre-built
binaries through GitHub Releases, mirroring the layout of
[`obra/superpowers`](https://github.com/obra/superpowers).

## Goal

`skills/<name>/` is the *distributable* surface that agents install. Today it
contains the Rust source for each skill, which means a user installing the
skill library also pulls down compiler-only files they will never run. Move
the Rust source to a separate `crates/` tree and have `skills/<name>/scripts/`
hold a single pre-built binary per skill. Ship those binaries through GitHub
Releases so end users do not need a Rust toolchain to run a skill.

## Non-goals

- Multi-skill orchestration, plugins, or runtime discovery. Each skill is a
  single self-contained binary launched by its `SKILL.md`.
- Windows support. The matrix targets Linux x86_64/aarch64 and macOS
  x86_64/aarch64 only.
- Per-skill versioning. The workspace continues to release together
  (`version.workspace = true`).
- Auto-update / self-update inside the install script. Users re-run
  `install.sh` to upgrade.

## File layout (target state)

```
repo-root/
├── Cargo.toml                       # [workspace] members = ["crates/*"]
├── Cargo.lock
├── README.md
├── LICENSE
├── rust-toolchain.toml
├── .gitignore                       # adds skills/*/scripts/
├── crates/
│   └── codemap/
│       ├── Cargo.toml
│       ├── src/
│       └── tests/
├── skills/
│   └── codemap/
│       ├── SKILL.md                 # exec: ./scripts/codemap
│       ├── README.md
│       └── scripts/                 # gitignored
│           └── codemap              # pre-built binary
├── scripts/
│   ├── build-skills.sh              # local dev: cargo build + copy bins
│   └── install.sh                   # end user: download from GH Releases
└── .github/workflows/
    ├── ci.yml                       # existing
    └── release.yml                  # NEW
```

Convention: **crate name == skill dir name == binary name**. The build and
install scripts rely on this; an exception (e.g. a future shell-only skill
with no Rust crate) is handled by graceful skipping, not by extra config.

## Components

### `Cargo.toml` (workspace)

```toml
[workspace]
resolver = "2"
members = ["crates/*"]
```

All other workspace-level metadata (`[workspace.package]`,
`[workspace.dependencies]`, `[profile.release]`) is unchanged.

### `scripts/build-skills.sh`

Used during local development to refresh `skills/*/scripts/<bin>` from the
current source tree.

```sh
#!/usr/bin/env bash
set -euo pipefail
cargo build --workspace --release --locked
for crate in crates/*/; do
  name=$(basename "$crate")
  skill_dir="skills/$name"
  [ -d "$skill_dir" ] || continue
  mkdir -p "$skill_dir/scripts"
  cp "target/release/$name" "$skill_dir/scripts/$name"
  chmod +x "$skill_dir/scripts/$name"
done
```

Behaviour:
- Fails fast (`set -euo pipefail`).
- Crates without a matching `skills/<name>/` are skipped.
- Pre-flight checks for `cargo` on PATH; prints a one-line pointer to
  `https://rustup.rs/` and exits 1 if missing.

### `.github/workflows/release.yml`

Triggers on `push` of tags matching `v*`, plus `workflow_dispatch` so a
failed run can be retried without re-tagging.

Matrix:

| Runner          | Target triple                  |
|-----------------|--------------------------------|
| `ubuntu-latest` | `x86_64-unknown-linux-gnu`     |
| `ubuntu-latest` | `aarch64-unknown-linux-gnu`    |
| `macos-13`      | `x86_64-apple-darwin`          |
| `macos-latest`  | `aarch64-apple-darwin`         |

Per-job steps:

1. `actions/checkout@v4`
2. `dtolnay/rust-toolchain@stable` with `targets: ${{ matrix.target }}`
3. For `aarch64-unknown-linux-gnu`: `apt-get install -y gcc-aarch64-linux-gnu`,
   export `CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc`.
4. `cargo build --workspace --release --locked --target ${{ matrix.target }}`
5. For each `crates/<name>/`: package
   `target/<target>/release/<name>` into
   `<name>-<tag>-<target>.tar.gz` (tar with the binary at archive root).
6. `softprops/action-gh-release@v2` upload all archives to the release
   identified by the pushed tag, with the release marked as latest only when
   the tag does not contain `-` (i.e. not a pre-release).

The cross-compile path uses the native GCC cross-linker rather than
`cross-rs/cross` to keep workflow logs flat and avoid the Docker dependency.
`tree-sitter` C grammars build through the `cc` crate, which honours
`CARGO_TARGET_*_LINKER` and the `CC_<triple>` envvar.

### `scripts/install.sh`

Used by end users in a fresh clone.

Behaviour:
- Detects platform via `uname -s` / `uname -m`, maps to a target triple.
- First positional arg is an optional version; default is `latest`, resolved
  via `https://api.github.com/repos/<repo>/releases/latest`.
- For each `skills/<name>/`, downloads
  `https://github.com/<repo>/releases/download/<tag>/<name>-<tag>-<triple>.tar.gz`
  and extracts to `skills/<name>/scripts/`.
- Pre-flight checks for `curl` and `tar`. Unknown platforms print the
  supported list and exit 1.
- If an asset is missing for a skill (HTTP 404), prints a warning and
  continues — `skills/` may contain shell-only skills in the future that have
  no release asset.

The repo slug is parameterised through the `SKILLS_REPO` environment variable,
defaulting to `AssetsArt/skills`, so a fork can run its own releases.

### `SKILL.md` (codemap)

The frontmatter description stays the same. The body changes the install/run
instructions from `cargo build` / `cargo install` to:

```
Run the bundled binary:
  ./scripts/codemap <subcommand> [flags]
```

The agent-facing manifest must always invoke `./scripts/codemap` (relative to
the skill directory) so installs from `install.sh` work without modifying the
user's `PATH`.

### `README.md` (workspace root)

Replaces the existing "Building" section with two subsections:

- **Install (end users):** `./scripts/install.sh [version]`
- **Build from source (developers):** `./scripts/build-skills.sh`

Also adds a one-line note that release assets cover Linux x86_64/aarch64 and
macOS x86_64/aarch64.

### `.gitignore`

Adds:

```
skills/*/scripts/
```

This means `scripts/` directories under each skill are never tracked. The
top-level `scripts/` (containing `build-skills.sh` and `install.sh`) is
unaffected because the pattern is anchored at `skills/`.

## Data flow

**End-user install.** `git clone` → `./scripts/install.sh` → uname detect →
GitHub Releases API for tag → per-skill download + extract into
`skills/<name>/scripts/` → chmod +x → agent runs `./scripts/<name>` directly.

**Developer iteration.** Edit `crates/<name>/src/...` → `cargo test`
(workspace) → `./scripts/build-skills.sh` to refresh local
`skills/<name>/scripts/<bin>` → exercise the binary via SKILL.md.

**Release.** Tag `v0.1.1` → push → `release.yml` matrix builds → 4 tarballs
attached to the GitHub release for that tag → users re-run `install.sh` (or
`install.sh v0.1.1`) to upgrade.

## Migration (one-shot, in order)

1. `git mv skills/codemap/Cargo.toml crates/codemap/Cargo.toml`,
   `git mv skills/codemap/src crates/codemap/src`,
   `git mv skills/codemap/tests crates/codemap/tests`. `SKILL.md` and
   `README.md` stay in `skills/codemap/`.
2. Update root `Cargo.toml` `members = ["crates/*"]`.
3. Verify `cargo build --workspace --locked` and
   `cargo test --workspace --locked` still pass with the existing 13 tests.
4. Add `.gitignore` entry for `skills/*/scripts/`.
5. Add `scripts/build-skills.sh`, run it, confirm
   `skills/codemap/scripts/codemap` is produced and runs.
6. Update `skills/codemap/SKILL.md` body to reference `./scripts/codemap`.
7. Update root `README.md` (Install + Build sections).
8. Add `scripts/install.sh`.
9. Add `.github/workflows/release.yml`.
10. Single commit per logical step (migration / build script / docs /
    install script / release workflow) so history stays bisectable.

## Error handling summary

| Surface           | Failure                                   | Behaviour                                              |
|-------------------|-------------------------------------------|--------------------------------------------------------|
| `build-skills.sh` | `cargo` missing                           | one-line error + rustup pointer, exit 1                |
| `build-skills.sh` | crate has no matching `skills/<name>/`    | skip silently                                          |
| `install.sh`      | `curl` / `tar` missing                    | print which tool is missing, exit 1                    |
| `install.sh`      | unknown platform                          | list supported triples, exit 1                         |
| `install.sh`      | asset 404 for a given skill               | print warning, continue to next skill                  |
| `release.yml`     | aarch64 cross-link failure                | surfaced through `set -e`; rerun via workflow_dispatch |

## Testing

- `cargo test --workspace --locked` continues to pass post-migration
  (same 13 tests).
- Existing `ci.yml` is untouched and still discovers crates via
  `--workspace`.
- `release.yml` cannot be dry-run locally; first verification is the next
  pushed tag. `workflow_dispatch` is wired so retries do not require a new
  tag.
- `install.sh` is exercised manually after the first successful release: in
  a fresh clone, run `./scripts/install.sh` and confirm
  `skills/codemap/scripts/codemap files --path .` works.

## Open questions

None at design time. Windows support, signed binaries, and homebrew /
shell-completion packaging are explicit non-goals for this revision.
