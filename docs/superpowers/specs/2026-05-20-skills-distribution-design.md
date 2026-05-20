# Skills Distribution Design

**Status:** approved 2026-05-20 (revised after subagent review)
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
- Windows support. The matrix targets Linux (gnu + musl, x86_64 + aarch64)
  and macOS (x86_64 + aarch64) only.
- Per-skill versioning. The workspace continues to release together
  (`version.workspace = true`).
- Auto-update / self-update inside the install script. Users re-run
  `install.sh` to upgrade.
- Code signing (Apple notarisation, sigstore). The integrity story relies on
  SHA-256 checksums verified at install time; signing can come later without
  changing the install UX.

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

Convention: **crate name == skill dir name == binary name**. The build,
install, and release scripts rely on this. A future shell-only skill (no
matching crate) is allowed; the inverse (crate without matching skill dir) is
treated as a configuration error and fails the release.

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
command -v cargo >/dev/null || { echo "cargo not found; install via https://rustup.rs/" >&2; exit 1; }
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
- Pre-flight check for `cargo`; one-line error + rustup pointer if missing.
- Crates without a matching `skills/<name>/` are skipped (allows internal
  helper crates in the future).

### `.github/workflows/release.yml`

Triggers on `push` of tags matching `v*`, plus `workflow_dispatch` so a
failed run can be retried without re-tagging. `workflow_dispatch` is gated:
the job exits early unless `github.ref_type == 'tag'`, so dispatched runs
must target an existing tag, not an arbitrary branch.

Workflow-level config (must all be explicit):

```yaml
permissions:
  contents: write              # required by softprops/action-gh-release
  id-token: none               # no OIDC use
```

Matrix (6 targets). Asset names use a short, user-friendly slug instead of
the Rust target triple; the workflow maps slug → triple internally:

| Runner          | Asset slug             | Rust target triple                | Toolchain                                       |
|-----------------|------------------------|-----------------------------------|-------------------------------------------------|
| `ubuntu-latest` | `linux-gnu-x86_64`     | `x86_64-unknown-linux-gnu`        | native (default gcc)                            |
| `ubuntu-latest` | `linux-gnu-aarch64`    | `aarch64-unknown-linux-gnu`       | apt `gcc-aarch64-linux-gnu`                     |
| `ubuntu-latest` | `linux-musl-x86_64`    | `x86_64-unknown-linux-musl`       | apt `musl-tools`                                |
| `ubuntu-latest` | `linux-musl-aarch64`   | `aarch64-unknown-linux-musl`      | `musl.cc` `aarch64-linux-musl-cross` tarball    |
| `macos-13`      | `macos-x86_64`         | `x86_64-apple-darwin`             | native                                          |
| `macos-latest`  | `macos-aarch64`        | `aarch64-apple-darwin`            | native                                          |

Per-job steps (third-party actions pinned by commit SHA, not floating tag):

1. `actions/checkout@<sha>`
2. `dtolnay/rust-toolchain@<sha>` with `toolchain: stable` and
   `targets: ${{ matrix.triple }}`.
3. **Toolchain setup, per slug:**

   - **`linux-gnu-x86_64`:** none — default `cc` works.
   - **`linux-gnu-aarch64`:**
     ```yaml
     - run: sudo apt-get update && sudo apt-get install -y gcc-aarch64-linux-gnu
     - run: |
         {
           echo "CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc"
           echo "AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar"
           echo "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc"
         } >> "$GITHUB_ENV"
     ```
   - **`linux-musl-x86_64`:**
     ```yaml
     - run: sudo apt-get update && sudo apt-get install -y musl-tools
     - run: |
         {
           echo "CC_x86_64_unknown_linux_musl=musl-gcc"
           echo "AR_x86_64_unknown_linux_musl=ar"
           echo "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc"
         } >> "$GITHUB_ENV"
     ```
   - **`linux-musl-aarch64`** (manual musl-cross tarball; pinned URL + SHA):
     ```yaml
     - run: |
         set -euo pipefail
         url="https://musl.cc/aarch64-linux-musl-cross.tgz"
         expected_sha=<PIN AT IMPLEMENT TIME>
         curl -fsSL --proto '=https' --tlsv1.2 "$url" -o /tmp/musl.tgz
         echo "$expected_sha  /tmp/musl.tgz" | sha256sum -c -
         mkdir -p "$HOME/.musl-cross"
         tar -xzf /tmp/musl.tgz -C "$HOME/.musl-cross" --strip-components=1
         echo "$HOME/.musl-cross/bin" >> "$GITHUB_PATH"
         {
           echo "CC_aarch64_unknown_linux_musl=aarch64-linux-musl-gcc"
           echo "AR_aarch64_unknown_linux_musl=aarch64-linux-musl-ar"
           echo "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=aarch64-linux-musl-gcc"
         } >> "$GITHUB_ENV"
     ```
     The `musl.cc` tarball is a third-party toolchain; the SHA-256 is pinned
     in the workflow so a host compromise cannot silently swap in a tainted
     compiler. The pin is refreshed only via a reviewed PR — never auto.
   - **`macos-x86_64`, `macos-aarch64`:** no extra toolchain step.

   Rationale for `CC_<triple>` + `AR_<triple>` everywhere: the `cc` crate
   (tree-sitter C grammars) reads these to find a compatible compiler;
   cargo reads `CARGO_TARGET_*_LINKER` only for the linker. Setting only
   the linker would leave the host `cc` compiling host-arch objects that
   the target linker rejects.

4. `cargo build --workspace --release --locked --target ${{ matrix.triple }}`
5. **Skill/crate pair audit** (loud assertion):
   ```sh
   shopt -s nullglob
   crates=(crates/*/)
   for c in "${crates[@]}"; do
     name=$(basename "$c")
     [ -d "skills/$name" ] || { echo "::error::crate $name has no skills/$name dir"; exit 1; }
   done
   echo "packaging ${#crates[@]} crate/skill pairs"
   ```
   The inverse (skill without crate) is a warning, not an error, because
   shell-only skills are a permitted future case.
6. **Package** each `crates/<name>` as
   `<name>-<tag>-<slug>.tar.gz` with the binary at archive root:
   ```sh
   tar -C "target/${{ matrix.triple }}/release" \
       -czf "$name-$tag-$slug.tar.gz" "$name"
   ```
7. **Checksums.** Emit `<name>-<tag>-<slug>.sha256` next to each tarball:
   ```sh
   sha256sum "$name-$tag-$slug.tar.gz" > "$name-$tag-$slug.sha256"
   ```
   (macOS jobs use `shasum -a 256`.)
8. `softprops/action-gh-release@<sha>`: upload every `.tar.gz` and `.sha256`
   to the release identified by the pushed tag, with `make_latest: true`
   only when the tag has no pre-release suffix (no `-` in the tag name).

Cross-compile choice: native toolchains over `cross-rs/cross` keep workflow
logs flat and avoid the Docker dependency. The trade-off is the explicit
envvar setup per slug plus the pinned `musl.cc` tarball for
`linux-musl-aarch64`, which the spec now enforces.

musl static-link note: the `*-unknown-linux-musl` Rust targets default to
static linking via `+crt-static`, which is what end users on Alpine,
distroless, and similar minimal images expect. No extra `RUSTFLAGS` is
required for codemap, but new crates that link to dynamic system libraries
should declare `[target.'cfg(target_env = "musl")'.dependencies]` overrides
rather than relying on host glibc.

### `scripts/install.sh`

Used by end users in a fresh clone. Designed to be idempotent (re-run to
upgrade) and atomic per skill (never leaves a half-extracted binary in
place).

Behaviour:
1. Pre-flight: `command -v curl tar sha256sum` (fall back to `shasum -a 256`
   on macOS). Exit 1 with a one-line error if any is missing.
2. Detect asset slug:
   - `uname -s` → `Darwin` or `Linux`.
   - `uname -m` → `arm64` / `aarch64` → `aarch64`; `x86_64` → `x86_64`.
   - On Linux, decide gnu vs musl by probing the dynamic loader:
     ```sh
     if ldd --version 2>&1 | grep -qi musl \
        || [ -f /lib/ld-musl-x86_64.so.1 ] \
        || [ -f /lib/ld-musl-aarch64.so.1 ]; then
       libc=musl
     else
       libc=gnu
     fi
     ```
   - Mapping:
     - `Darwin-arm64`  → `macos-aarch64`
     - `Darwin-x86_64` → `macos-x86_64`
     - `Linux-x86_64`  → `linux-$libc-x86_64`
     - `Linux-aarch64` → `linux-$libc-aarch64`
   - The slug can also be forced via `SKILLS_TARGET` env var
     (e.g. `SKILLS_TARGET=linux-musl-x86_64`) to support cross-arch installs
     on hosts where the auto-detection is wrong (containers, build hosts).
   - Anything that doesn't map → print the supported slug list, exit 1.
3. Resolve repo slug: `repo="${SKILLS_REPO:-AssetsArt/skills}"`. **Print it
   to stderr before any download** so users see what they're pulling from.
4. Resolve version: first positional arg, else `latest` via
   `https://api.github.com/repos/$repo/releases/latest`. If
   `GITHUB_TOKEN` is set in the environment, send it via
   `Authorization: Bearer` to bypass the 60/hr unauthenticated rate limit
   (relevant when running in CI or behind shared NAT).
5. For each `skills/<name>/`:
   a. Stage to `$(mktemp -d)/<name>`.
   b. Download `<name>-<tag>-<slug>.tar.gz` and `<name>-<tag>-<slug>.sha256`
      with `curl --fail --show-error --location --proto '=https' --tlsv1.2`.
      On 404, print a warning and skip (lets shell-only skills coexist).
   c. **Verify checksum** before touching the destination.
   d. **Tar entry audit** before extraction:
      ```sh
      if tar -tzf "$archive" | grep -E '(^/|(^|/)\.\./)' ; then
        echo "refusing tarball with absolute or parent-relative entries" >&2
        exit 1
      fi
      ```
   e. Extract into the stage dir with `tar -xzf "$archive" -C "$stage"`.
      Confirm the resulting file is `$stage/<name>` and nothing else.
   f. `chmod +x` then `mv -f "$stage/<name>" "skills/<name>/scripts/<name>"`.
      On Darwin: `xattr -d com.apple.quarantine "skills/<name>/scripts/<name>"`
      (ignore failure if the attribute is absent).
   g. Clean the stage dir.
6. Final log line: `installed N skills (<slug>) at version <tag>`.

Atomicity model: staging + final `mv` means a failed download or checksum
mismatch never corrupts the previously installed binary. Per-skill atomicity
is enough; cross-skill atomicity (all-or-nothing across multiple skills) is
out of scope because skills are independent.

### `SKILL.md` (codemap)

Two changes:
- Replace install / run instructions with: `./scripts/codemap <subcommand>`.
  The agent-facing manifest must always invoke the relative path so installs
  through `install.sh` work without modifying `PATH`.
- Update the "Adding a new language" paragraph that currently points to
  `skills/codemap/src/queries/` to point to `crates/codemap/src/queries/`
  post-migration.

### `README.md` (workspace root)

Replaces the existing "Building" section with:

- **Install (end users):** `./scripts/install.sh [version]`, with a note
  listing supported asset slugs (`linux-gnu-x86_64`, `linux-gnu-aarch64`,
  `linux-musl-x86_64`, `linux-musl-aarch64`, `macos-x86_64`, `macos-aarch64`)
  and a one-liner about `GITHUB_TOKEN=...` to bypass the API rate limit if
  the user hits it. Also document `SKILLS_TARGET=<slug>` for forcing a slug
  when auto-detection picks the wrong libc (e.g. installing into an Alpine
  container from a glibc host).
- **Build from source (developers):** `./scripts/build-skills.sh`.
- **Security / integrity:** one paragraph stating that binaries are
  SHA-256-checksummed at release time and verified by `install.sh`; signing
  is a non-goal for this revision.

### `.gitignore`

Adds:

```
skills/*/scripts/
```

The pattern is anchored at `skills/` so the top-level `scripts/` directory
(`build-skills.sh`, `install.sh`) is unaffected.

## Data flow

**End-user install.** `git clone` → `./scripts/install.sh` → uname detect →
GitHub Releases API for tag → per-skill: stage temp → download tarball +
`.sha256` → verify checksum → audit tar entries → extract → `mv` into
`skills/<name>/scripts/` → chmod +x → strip `com.apple.quarantine` on
darwin → agent runs `./scripts/<name>` directly.

**Developer iteration.** Edit `crates/<name>/src/...` → `cargo test`
(workspace) → `./scripts/build-skills.sh` to refresh local
`skills/<name>/scripts/<bin>` → exercise the binary via SKILL.md.

**Release.** Tag `v0.1.1` → push → `release.yml` matrix builds → per crate
+ per target: tarball + sha256 attached to the release → users re-run
`install.sh` (or `install.sh v0.1.1`) to upgrade.

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
6. Update `skills/codemap/SKILL.md`:
   - run command → `./scripts/codemap`
   - "Adding a new language" path → `crates/codemap/src/queries/`.
7. Update root `README.md` (Install / Build / Security sections).
8. Add `scripts/install.sh` with the staging + checksum + entry-audit flow.
9. Add `.github/workflows/release.yml` with pinned action SHAs, explicit
   `permissions:`, the 6-target matrix (gnu/musl × x86_64/aarch64 + macos
   x86_64/aarch64), per-slug toolchain setup including the pinned
   `musl.cc` aarch64 tarball, pair audit, and checksum emission.
10. Single commit per logical step (migration / build script / docs /
    install script / release workflow) so history stays bisectable.

## Error handling summary

| Surface           | Failure                                   | Behaviour                                                                                  |
|-------------------|-------------------------------------------|--------------------------------------------------------------------------------------------|
| `build-skills.sh` | `cargo` missing                           | one-line error + rustup pointer, exit 1                                                    |
| `build-skills.sh` | crate has no matching `skills/<name>/`    | skip silently (internal helper crate)                                                       |
| `install.sh`      | `curl` / `tar` / `sha256sum` missing      | name the missing tool, exit 1                                                              |
| `install.sh`      | unknown platform / unmappable slug        | list the 6 supported slugs, exit 1                                                         |
| `install.sh`      | `SKILLS_TARGET` set to an unknown slug    | name the offending value + supported list, exit 1                                          |
| `install.sh`      | GitHub API rate-limited (HTTP 403)        | print hint about `GITHUB_TOKEN` env var, exit 1                                            |
| `install.sh`      | asset 404 for a given skill               | warn, continue (shell-only skill coexistence)                                              |
| `install.sh`      | checksum mismatch                         | delete stage, exit 1; previously installed binary untouched                                |
| `install.sh`      | tar entry audit fails                     | exit 1 before extraction; previously installed binary untouched                            |
| `release.yml`     | crate without matching `skills/<name>/`   | `::error::` annotation, fail the job (caught by pair-audit step)                           |
| `release.yml`     | linux-musl-aarch64 toolchain SHA mismatch | `sha256sum -c` exits non-zero; job fails before any cargo build                            |
| `release.yml`     | cross-compile / linker failure            | `set -e` exits; retry via `workflow_dispatch` against the same tag                         |
| `release.yml`     | `workflow_dispatch` against a branch      | early-exit guard on `github.ref_type == 'tag'`                                             |

## Security & integrity model

- **Source integrity:** Rust source is unchanged; the migration is `git mv`
  plus a workspace-members edit. CI keeps running fmt / clippy / test on
  every push.
- **Build integrity:** release runs on GitHub-hosted runners. All
  third-party actions (`actions/checkout`, `dtolnay/rust-toolchain`,
  `softprops/action-gh-release`) are pinned to commit SHAs to prevent
  upstream tag-rewrite attacks. The `linux-musl-aarch64` toolchain is
  downloaded from `musl.cc` once per job and its SHA-256 is pinned in the
  workflow; a tarball whose hash doesn't match fails the job before cargo
  runs. Refreshing that pin requires a reviewed PR — it is never auto-bumped.
- **Distribution integrity:** every tarball is paired with a `.sha256`
  generated in the same job that built it. `install.sh` verifies the
  checksum before extraction and refuses tarballs containing absolute paths
  or `..` segments.
- **Trust boundary:** `GITHUB_TOKEN` is scoped to `contents: write` at the
  workflow level only; `id-token` is explicitly disabled. The
  `workflow_dispatch` trigger is gated on `github.ref_type == 'tag'` so a
  re-run cannot ship binaries from an arbitrary branch.
- **macOS quarantine:** `install.sh` strips `com.apple.quarantine` after
  install so the binary runs without manual user intervention. (Signing /
  notarisation remain non-goals.)
- **Out of scope:** TOCTOU between download and verify (acceptable given
  per-skill atomic `mv`); arbitrary `SKILLS_REPO` substitution by a
  tampered shell profile (mitigated by printing the resolved repo).

## Testing

- `cargo test --workspace --locked` continues to pass post-migration
  (same 13 tests).
- Existing `ci.yml` is untouched and still discovers crates via
  `--workspace`.
- `release.yml` cannot be dry-run locally; first verification is the next
  pushed tag. `workflow_dispatch` (gated on a tag ref) is wired so retries
  do not require a new tag.
- `install.sh` is exercised manually after the first successful release: in
  a fresh clone, run `./scripts/install.sh` and confirm
  `skills/codemap/scripts/codemap files --path .` works on both Linux and
  macOS hosts. A second run is expected to be a no-op upgrade with no
  ownership change to the previously installed binary.

## Follow-ups (next iteration)

- Apple notarisation / sigstore signing for binaries.
- Windows targets.
- Optional `SKILLS_REPO` allowlist so users in regulated environments can
  pin to a known publisher.
- Cosign-style transparency log for release assets.
