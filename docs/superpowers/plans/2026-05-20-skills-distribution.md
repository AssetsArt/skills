# Skills Distribution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move Rust source out of `skills/<name>/` into a parallel `crates/<name>/` tree and ship pre-built binaries through GitHub Releases so installing the skill library no longer drags along compiler-only files. Layout mirrors `obra/superpowers`; release workflow mirrors `AssetsArt/nylon-mesh` (cross-rs/cross + macos-latest for both Darwin arches).

**Architecture:** Source lives in `crates/*`. `skills/<name>/scripts/<bin>` holds a single pre-built binary (gitignored, downloaded by `scripts/install.sh`). Local dev refreshes it via `scripts/build-skills.sh`. CI releases on `v*` tag through `.github/workflows/release.yml` — 6-target matrix (gnu × musl × x86_64/aarch64 for Linux, x86_64/aarch64 for macOS) with `cross-rs/cross` for the 3 non-host Linux slugs, native cargo for everything else. Each tarball ships with a `.sha256` companion that `install.sh` verifies before extraction.

**Tech Stack:** Rust 2021 (stable), bash 4+, `cross-rs/cross` v0.2.5 (pinned), GitHub Actions with third-party actions pinned by commit SHA, `softprops/action-gh-release@v2` for asset upload, `sha256sum` (Linux) / `shasum -a 256` (macOS) for integrity, `curl` + `tar` for client install.

**Reference spec:** `docs/superpowers/specs/2026-05-20-skills-distribution-design.md`

---

## File Structure (locked before tasks)

```
repo-root/
├── Cargo.toml                          # MODIFIED: members = ["crates/*"]
├── .gitignore                          # MODIFIED: + skills/*/scripts/
├── README.md                           # MODIFIED: Install / Build / Security sections
├── crates/                             # NEW directory
│   └── codemap/                        # MOVED from skills/codemap/
│       ├── Cargo.toml                  # MOVED, unchanged
│       ├── src/                        # MOVED, unchanged
│       └── tests/                      # MOVED, unchanged
├── skills/
│   └── codemap/
│       ├── SKILL.md                    # MODIFIED: run via ./scripts/codemap; queries path -> crates/
│       ├── README.md                   # KEPT in place, unchanged
│       └── scripts/                    # NEW directory (gitignored)
│           └── codemap                 # NEW: built binary (never committed)
├── scripts/                            # NEW directory
│   ├── build-skills.sh                 # NEW: local cargo build + copy
│   └── install.sh                      # NEW: download + verify + extract
└── .github/workflows/
    ├── ci.yml                          # UNCHANGED -- `--workspace` re-discovers crates/*
    └── release.yml                     # NEW
```

**Conventions enforced by every script in this plan:**
- crate name == skill dir name == binary name
- Each task ends on a committed working tree (`git status` clean)
- Each commit message is conventional: `feat:`, `chore:`, `docs:`, `ci:`
- No emojis in code, comments, or commit messages

---

### Task 1: Move Rust source from `skills/codemap/` to `crates/codemap/`

**Files:**
- Move: `skills/codemap/Cargo.toml` -> `crates/codemap/Cargo.toml`
- Move: `skills/codemap/src/` -> `crates/codemap/src/`
- Move: `skills/codemap/tests/` -> `crates/codemap/tests/`
- Modify: `Cargo.toml` (root) line 3

- [ ] **Step 1.1: Create the `crates/` directory**

```bash
mkdir -p crates
```

- [ ] **Step 1.2: Move the three source dirs/files with `git mv` (preserves history)**

```bash
git mv skills/codemap/Cargo.toml crates/codemap/Cargo.toml
git mv skills/codemap/src        crates/codemap/src
git mv skills/codemap/tests      crates/codemap/tests
```

Note: `SKILL.md` and `README.md` MUST stay under `skills/codemap/` -- they are the install-time surface.

- [ ] **Step 1.3: Update workspace `members` in root `Cargo.toml`**

Find line 3:
```toml
members = ["skills/*"]
```
Replace with:
```toml
members = ["crates/*"]
```

- [ ] **Step 1.4: Verify the workspace still builds**

Run:
```bash
cargo build --workspace --locked
```
Expected: builds cleanly. If a path-based `include_str!` breaks, it should fail loudly and you stop here -- the spec assumed `cargo` path-rewrites are sufficient, and the codebase was audited (architect review confirmed only `CARGO_MANIFEST_DIR`-rooted paths exist).

- [ ] **Step 1.5: Verify all 13 tests pass**

Run:
```bash
cargo test --workspace --locked
```
Expected: `test result: ok. 13 passed` across the test binaries (`files_test`, `find_test`, `stats_test`, `symbols_test`).

- [ ] **Step 1.6: Run fmt + clippy as a final sanity check**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```
Expected: both exit 0.

- [ ] **Step 1.7: Commit**

```bash
git add Cargo.toml crates skills/codemap
git commit -m "refactor: move codemap source to crates/codemap"
```

The `skills/codemap/` directory now contains only `SKILL.md` and `README.md` -- they were tracked moves of the source files, not deletions.

---

### Task 2: Add `.gitignore` entry for `skills/*/scripts/`

**Files:**
- Modify: `.gitignore`

- [ ] **Step 2.1: Append the new ignore pattern**

Open `.gitignore`. Current content (5 lines):
```
/target
**/*.rs.bk
.DS_Store
.idea/
.vscode/
*.swp
```

Append a `skills/` block:
```
# Per-skill built binaries (produced by scripts/build-skills.sh or scripts/install.sh)
skills/*/scripts/
```

- [ ] **Step 2.2: Verify the pattern works**

```bash
mkdir -p skills/codemap/scripts && touch skills/codemap/scripts/codemap
git status --short
```
Expected: `M .gitignore` only -- the new `skills/codemap/scripts/` directory is NOT listed as untracked.

- [ ] **Step 2.3: Clean up the probe file**

```bash
rm -rf skills/codemap/scripts
```

- [ ] **Step 2.4: Commit**

```bash
git add .gitignore
git commit -m "chore: gitignore built skill binaries"
```

---

### Task 3: Add `scripts/build-skills.sh`

**Files:**
- Create: `scripts/build-skills.sh`

- [ ] **Step 3.1: Create the `scripts/` directory**

```bash
mkdir -p scripts
```

- [ ] **Step 3.2: Write the build script**

Create `scripts/build-skills.sh`:
```bash
#!/usr/bin/env bash
# Build every workspace crate that has a matching skills/<name>/ dir and
# copy the release binary into skills/<name>/scripts/<name>.
#
# Run from the repo root:
#   ./scripts/build-skills.sh

set -euo pipefail

command -v cargo >/dev/null || {
  echo "error: cargo not found on PATH" >&2
  echo "hint: install Rust via https://rustup.rs/" >&2
  exit 1
}

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

cargo build --workspace --release --locked

built=0
for crate in crates/*/; do
  name="$(basename "$crate")"
  skill_dir="skills/$name"
  [ -d "$skill_dir" ] || continue          # internal helper crate; no skill surface
  mkdir -p "$skill_dir/scripts"
  cp "target/release/$name" "$skill_dir/scripts/$name"
  chmod +x "$skill_dir/scripts/$name"
  echo "built skills/$name/scripts/$name"
  built=$((built + 1))
done
echo "done: $built skill binary(ies)"
```

- [ ] **Step 3.3: Make it executable**

```bash
chmod +x scripts/build-skills.sh
```

- [ ] **Step 3.4: Run it end-to-end**

```bash
./scripts/build-skills.sh
```
Expected output (last two lines):
```
built skills/codemap/scripts/codemap
done: 1 skill binary(ies)
```

- [ ] **Step 3.5: Verify the binary works**

```bash
./skills/codemap/scripts/codemap --help | head -3
```
Expected: `Usage: codemap <COMMAND>` or similar -- confirms the binary launches.

- [ ] **Step 3.6: Verify `.gitignore` keeps the binary out of git**

```bash
git status --short
```
Expected: only `scripts/build-skills.sh` is untracked. No mention of `skills/codemap/scripts/codemap`.

- [ ] **Step 3.7: Commit**

```bash
git add scripts/build-skills.sh
git commit -m "feat: add scripts/build-skills.sh"
```

---

### Task 4: Update `skills/codemap/SKILL.md` -- agent-facing run instructions + post-migration path

**Files:**
- Modify: `skills/codemap/SKILL.md` lines 19-25 (Build section) and line 60 (queries path)

- [ ] **Step 4.1: Replace the `## Build` section (lines 19-25)**

Find:
```markdown
## Build

```bash
cargo build --release -p codemap
```

Binary lands at `target/release/codemap` (relative to the workspace root).
```

Replace with:
```markdown
## Run

The skill ships a pre-built binary; invoke it from the skill directory:

```bash
./scripts/codemap <subcommand> [flags]
```

If the binary is missing, run `./scripts/install.sh` from the repo root (downloads from GitHub Releases) or `./scripts/build-skills.sh` (builds from `crates/codemap` locally).
```

- [ ] **Step 4.2: Fix the queries path in the "Supported languages" paragraph (line 60)**

Find:
```
drop a `.scm` query into `skills/codemap/src/queries/`, register it in `src/lang.rs`, add an extension mapping.
```

Replace with:
```
drop a `.scm` query into `crates/codemap/src/queries/`, register it in `src/lang.rs`, add an extension mapping.
```

- [ ] **Step 4.3: Verify no other path references survived**

```bash
grep -n "skills/codemap/src" skills/codemap/SKILL.md
```
Expected: no output (empty result).

- [ ] **Step 4.4: Commit**

```bash
git add skills/codemap/SKILL.md
git commit -m "docs(codemap): point SKILL.md at ./scripts/codemap and crates/ path"
```

---

### Task 5: Update root `README.md` with Install / Build / Security sections

**Files:**
- Modify: `README.md` lines 18-25 (Building section)

- [ ] **Step 5.1: Replace the `## Building` section**

Find:
```markdown
## Building

```bash
cargo build --release
# or build a single skill:
cargo build --release -p codemap
```

Binaries land in `target/release/<skill-name>`.
```

Replace with:
```markdown
## Install (end users)

```bash
./scripts/install.sh           # downloads the latest release for your platform
./scripts/install.sh v0.1.1    # or pin a specific version
```

Supported asset slugs: `linux-gnu-x86_64`, `linux-gnu-aarch64`, `linux-musl-x86_64`, `linux-musl-aarch64`, `macos-x86_64`, `macos-aarch64`. The script auto-detects the right slug from `uname` + libc probe; override with `SKILLS_TARGET=<slug>` if you need to (e.g. installing into an Alpine container from a glibc host). If you hit GitHub's 60/hr unauthenticated API rate limit, export `GITHUB_TOKEN` before running.

After install, every skill exposes its binary at `skills/<name>/scripts/<name>` -- the `SKILL.md` manifest invokes it from there.

## Build from source (developers)

```bash
./scripts/build-skills.sh
```

Builds every workspace crate that has a matching `skills/<name>/` directory in `--release` mode and copies each binary into `skills/<name>/scripts/<name>`. The same layout `install.sh` produces.

## Security / integrity

Release tarballs are paired with `.sha256` files generated in the same CI job that built them. `install.sh` verifies the checksum and refuses tarballs containing absolute paths or `..` segments before extraction. Code signing (Apple notarisation, sigstore) is intentionally out of scope for this revision.
```

- [ ] **Step 5.2: Verify no `cargo build --release` instruction lingers in the README**

```bash
grep -n "cargo build" README.md
```
Expected: no output.

- [ ] **Step 5.3: Commit**

```bash
git add README.md
git commit -m "docs: README install / build / security for distribution layout"
```

---

### Task 6: Add `scripts/install.sh`

**Files:**
- Create: `scripts/install.sh`

This task is large because the script is the user-facing security boundary. Every guard the spec requires (`SKILLS_TARGET` allowlist, tar entry audit, checksum verify, atomic mv) lives here.

- [ ] **Step 6.1: Write the script header + pre-flight + slug allowlist**

Create `scripts/install.sh`:
```bash
#!/usr/bin/env sh
# Download the latest (or a pinned) release of every skill in this repo
# and drop the per-platform binary into skills/<name>/scripts/<name>.
#
# Usage:
#   ./scripts/install.sh                 # latest release, auto-detect slug
#   ./scripts/install.sh v0.1.1          # pin a version
#   SKILLS_TARGET=linux-musl-x86_64 ./scripts/install.sh   # force slug
#   SKILLS_REPO=user/fork  ./scripts/install.sh            # pull from a fork
#   GITHUB_TOKEN=... ./scripts/install.sh                  # dodge API rate limit

set -eu

# ---- known asset slugs (must match release.yml matrix) ----
SUPPORTED_SLUGS="linux-gnu-x86_64 linux-gnu-aarch64 linux-musl-x86_64 linux-musl-aarch64 macos-x86_64 macos-aarch64"

is_supported_slug() {
  for s in $SUPPORTED_SLUGS; do
    [ "$1" = "$s" ] && return 0
  done
  return 1
}

# ---- pre-flight: required tools ----
for tool in curl tar; do
  command -v "$tool" >/dev/null || { echo "error: $tool not found on PATH" >&2; exit 1; }
done
if command -v sha256sum >/dev/null; then
  SHA_CMD="sha256sum"
elif command -v shasum >/dev/null; then
  SHA_CMD="shasum -a 256"
else
  echo "error: neither sha256sum nor shasum found on PATH" >&2
  exit 1
fi
```

- [ ] **Step 6.2: Append slug detection + `SKILLS_TARGET` validation**

Append to `scripts/install.sh`:
```bash
# ---- detect target slug ----
detect_slug() {
  os=$(uname -s)
  arch=$(uname -m)
  case "$arch" in
    arm64|aarch64) arch=aarch64 ;;
    x86_64|amd64)  arch=x86_64  ;;
    *) echo "error: unsupported arch '$arch'" >&2; return 1 ;;
  esac
  case "$os" in
    Darwin) echo "macos-$arch" ;;
    Linux)
      libc=gnu
      if ldd --version 2>&1 | grep -qi musl \
         || [ -f /lib/ld-musl-x86_64.so.1 ] \
         || [ -f /lib/ld-musl-aarch64.so.1 ]; then
        libc=musl
      fi
      echo "linux-$libc-$arch" ;;
    *) echo "error: unsupported OS '$os'" >&2; return 1 ;;
  esac
}

if [ -n "${SKILLS_TARGET:-}" ]; then
  # User override. MUST validate against the allowlist before using anywhere
  # so a value like `linux-musl-x86_64/../../etc/passwd` cannot reach curl
  # or .sha256 filenames.
  if ! is_supported_slug "$SKILLS_TARGET"; then
    echo "error: SKILLS_TARGET='$SKILLS_TARGET' is not a known slug" >&2
    echo "       supported: $SUPPORTED_SLUGS" >&2
    exit 1
  fi
  slug="$SKILLS_TARGET"
else
  slug="$(detect_slug)" || exit 1
  is_supported_slug "$slug" || {
    echo "error: detected slug '$slug' is not in the supported list" >&2
    echo "       supported: $SUPPORTED_SLUGS" >&2
    exit 1
  }
fi
```

- [ ] **Step 6.3: Append repo + version resolution (with `GITHUB_TOKEN` support)**

Append:
```bash
# ---- repo + version ----
repo="${SKILLS_REPO:-AssetsArt/skills}"
echo "skills repo: https://github.com/$repo" >&2

curl_auth_args() {
  # Prints curl auth args (or nothing) for GitHub API calls.
  if [ -n "${GITHUB_TOKEN:-}" ]; then
    printf -- '-H Authorization:Bearer %s' "$GITHUB_TOKEN"
  fi
}

if [ "$#" -ge 1 ] && [ -n "$1" ]; then
  tag="$1"
else
  echo "resolving latest tag for $repo" >&2
  api="https://api.github.com/repos/$repo/releases/latest"
  # shellcheck disable=SC2046
  resp="$(curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
            $(curl_auth_args) "$api" || true)"
  if [ -z "$resp" ]; then
    echo "error: could not reach $api" >&2
    echo "       if you hit the 60/hr unauthenticated rate limit, set GITHUB_TOKEN" >&2
    exit 1
  fi
  tag="$(printf '%s' "$resp" | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
  if [ -z "$tag" ]; then
    echo "error: could not parse tag_name from latest-release response" >&2
    exit 1
  fi
fi
echo "installing version: $tag (slug $slug)" >&2
```

- [ ] **Step 6.4: Append the per-skill download + verify + extract loop**

Append:
```bash
# ---- per-skill install ----
installed=0
for skill_dir in skills/*/; do
  name="$(basename "$skill_dir")"
  asset="$name-$tag-$slug.tar.gz"
  sha_file="$name-$tag-$slug.sha256"
  base="https://github.com/$repo/releases/download/$tag"

  stage="$(mktemp -d)"
  trap 'rm -rf "$stage"' EXIT

  # 1) download tarball + checksum companion (404 == skill has no release asset; skip)
  if ! curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
        -o "$stage/$asset" "$base/$asset"; then
    echo "skip: no asset for $name ($asset)" >&2
    rm -rf "$stage"; trap - EXIT
    continue
  fi
  if ! curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
        -o "$stage/$sha_file" "$base/$sha_file"; then
    echo "error: checksum file missing for $name ($sha_file)" >&2
    exit 1
  fi

  # 2) verify checksum BEFORE touching the destination
  expected="$(awk '{print $1}' "$stage/$sha_file")"
  actual="$(cd "$stage" && $SHA_CMD "$asset" | awk '{print $1}')"
  if [ "$expected" != "$actual" ]; then
    echo "error: sha256 mismatch for $asset" >&2
    echo "       expected $expected, got $actual" >&2
    exit 1
  fi

  # 3) tar entry audit -- reject absolute or parent-relative paths
  if tar -tzf "$stage/$asset" | grep -E '(^/|(^|/)\.\./)' >/dev/null; then
    echo "error: refusing tarball with absolute or '..' entries: $asset" >&2
    exit 1
  fi

  # 4) extract, sanity check, move atomically
  tar -xzf "$stage/$asset" -C "$stage"
  if [ ! -f "$stage/$name" ]; then
    echo "error: tarball $asset did not contain '$name' at archive root" >&2
    exit 1
  fi
  mkdir -p "$skill_dir/scripts"
  chmod +x "$stage/$name"
  mv -f "$stage/$name" "$skill_dir/scripts/$name"

  # 5) strip macOS quarantine xattr (so the binary runs immediately)
  if [ "$(uname -s)" = "Darwin" ]; then
    xattr -d com.apple.quarantine "$skill_dir/scripts/$name" 2>/dev/null || true
  fi

  rm -rf "$stage"; trap - EXIT
  echo "installed: $skill_dir/scripts/$name" >&2
  installed=$((installed + 1))
done

echo "installed $installed skill(s) ($slug) at version $tag" >&2
```

- [ ] **Step 6.5: Make it executable**

```bash
chmod +x scripts/install.sh
```

- [ ] **Step 6.6: Run shellcheck if available, otherwise `sh -n`**

```bash
if command -v shellcheck >/dev/null; then
  shellcheck scripts/install.sh
else
  sh -n scripts/install.sh && echo "syntax ok"
fi
```
Expected: no errors. The `SC2046` warning around `curl_auth_args` is intentionally silenced inline.

- [ ] **Step 6.7: Exercise the slug validation locally**

```bash
SKILLS_TARGET="linux-musl-x86_64/../../etc/passwd" ./scripts/install.sh 2>&1 | head -3
```
Expected: prints `error: SKILLS_TARGET='linux-musl-x86_64/../../etc/passwd' is not a known slug` and exits non-zero -- proves the allowlist runs before any URL interpolation.

```bash
SKILLS_TARGET="linux-gnu-x86_64" ./scripts/install.sh v0.0.0-does-not-exist 2>&1 | head -5
```
Expected: prints `skills repo: https://github.com/AssetsArt/skills`, `installing version: v0.0.0-does-not-exist (slug linux-gnu-x86_64)`, then `skip: no asset for codemap (codemap-v0.0.0-does-not-exist-linux-gnu-x86_64.tar.gz)`. Confirms the per-skill 404 path is graceful.

- [ ] **Step 6.8: Commit**

```bash
git add scripts/install.sh
git commit -m "feat: add scripts/install.sh with checksum + slug allowlist"
```

---

### Task 7: Add `.github/workflows/release.yml`

**Files:**
- Create: `.github/workflows/release.yml`

- [ ] **Step 7.1: Resolve commit SHAs for the three third-party actions**

The spec requires pinning by commit SHA, not floating tag. Resolve once at implement time:

```bash
gh api repos/actions/checkout/git/refs/tags/v4              -q .object.sha
gh api repos/dtolnay/rust-toolchain/git/refs/heads/stable   -q .object.sha
gh api repos/softprops/action-gh-release/git/refs/tags/v2   -q .object.sha
```

If `gh` is unauthenticated, fall back to:
```bash
curl -fsSL https://api.github.com/repos/actions/checkout/git/refs/tags/v4 \
  | sed -n 's/.*"sha":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1
```

Record the three SHAs and substitute them into the YAML in Step 7.2 (replace `<sha>` placeholders).

- [ ] **Step 7.2: Create the workflow file**

Create `.github/workflows/release.yml`:
```yaml
name: release

on:
  push:
    tags: ["v*"]
  workflow_dispatch:

permissions:
  contents: write    # softprops/action-gh-release upload
  id-token: none     # no OIDC

env:
  CARGO_TERM_COLOR: always
  CROSS_PINNED_TAG: v0.2.5

jobs:
  build:
    # workflow_dispatch must target a tag, never an arbitrary branch.
    if: github.event_name == 'push' || github.ref_type == 'tag'
    strategy:
      fail-fast: false
      matrix:
        include:
          - runner: ubuntu-latest
            slug:   linux-gnu-x86_64
            triple: x86_64-unknown-linux-gnu
            builder: cargo
          - runner: ubuntu-latest
            slug:   linux-gnu-aarch64
            triple: aarch64-unknown-linux-gnu
            builder: cross
          - runner: ubuntu-latest
            slug:   linux-musl-x86_64
            triple: x86_64-unknown-linux-musl
            builder: cross
            rustflags: "-C target-feature=+crt-static"
          - runner: ubuntu-latest
            slug:   linux-musl-aarch64
            triple: aarch64-unknown-linux-musl
            builder: cross
            rustflags: "-C target-feature=+crt-static"
          - runner: macos-latest
            slug:   macos-x86_64
            triple: x86_64-apple-darwin
            builder: cargo
          - runner: macos-latest
            slug:   macos-aarch64
            triple: aarch64-apple-darwin
            builder: cargo

    runs-on: ${{ matrix.runner }}
    env:
      RUSTFLAGS: ${{ matrix.rustflags }}
    steps:
      - uses: actions/checkout@<sha>          # v4 -- pinned

      - uses: dtolnay/rust-toolchain@<sha>    # stable -- pinned
        with:
          toolchain: stable
          targets: ${{ matrix.triple }}

      - name: Install cross (cross builder only)
        if: matrix.builder == 'cross'
        run: cargo install cross --git https://github.com/cross-rs/cross --tag ${{ env.CROSS_PINNED_TAG }} --locked

      - name: Audit crate/skill pairs
        shell: bash
        run: |
          set -euo pipefail
          shopt -s nullglob
          crates=(crates/*/)
          for c in "${crates[@]}"; do
            name=$(basename "$c")
            if [ ! -d "skills/$name" ]; then
              echo "::error::crate $name has no skills/$name dir"
              exit 1
            fi
          done
          echo "packaging ${#crates[@]} crate/skill pair(s) for ${{ matrix.slug }}"

      - name: Build (cargo)
        if: matrix.builder == 'cargo'
        run: cargo build --workspace --release --locked --target ${{ matrix.triple }}

      - name: Build (cross)
        if: matrix.builder == 'cross'
        run: cross build --workspace --release --locked --target ${{ matrix.triple }}

      - name: Package + checksum every crate/skill pair
        shell: bash
        run: |
          set -euo pipefail
          tag="${GITHUB_REF_NAME}"
          slug="${{ matrix.slug }}"
          triple="${{ matrix.triple }}"
          out="release-assets"
          mkdir -p "$out"
          if command -v sha256sum >/dev/null; then SHA="sha256sum"; else SHA="shasum -a 256"; fi
          for crate in crates/*/; do
            name=$(basename "$crate")
            tar -C "target/$triple/release" -czf "$out/$name-$tag-$slug.tar.gz" "$name"
            ( cd "$out" && $SHA "$name-$tag-$slug.tar.gz" > "$name-$tag-$slug.sha256" )
            echo "packaged $out/$name-$tag-$slug.tar.gz"
          done

      - name: Upload release assets
        uses: softprops/action-gh-release@<sha>   # v2 -- pinned
        with:
          tag_name: ${{ github.ref_name }}
          files: |
            release-assets/*.tar.gz
            release-assets/*.sha256
          make_latest: ${{ !contains(github.ref_name, '-') }}
          fail_on_unmatched_files: true
```

- [ ] **Step 7.3: Substitute the three pinned SHAs**

Replace each `<sha>` placeholder with the commit SHA collected in Step 7.1. Sanity check:
```bash
grep -nE '@<sha>' .github/workflows/release.yml
```
Expected: no output (all three placeholders gone).

- [ ] **Step 7.4: Validate YAML syntax**

```bash
python3 -c 'import yaml,sys; yaml.safe_load(open(".github/workflows/release.yml"))' && echo "yaml ok"
```
Expected: `yaml ok`. If `python3` is unavailable, install `actionlint` and run `actionlint .github/workflows/release.yml`; either tool catches structural mistakes.

- [ ] **Step 7.5: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add release workflow (6-target matrix, cross-rs, sha256)"
```

- [ ] **Step 7.6: Final state check**

```bash
git log --oneline -8
git status
cargo test --workspace --locked   # one more time, end-to-end
./scripts/build-skills.sh         # confirm local pipeline still works
```
Expected: 7 new commits since the start of the plan; working tree clean; 13 tests pass; binary rebuilds into `skills/codemap/scripts/codemap`.

---

## Verification matrix (after Task 7)

| Layer                | Verification                                                                                       |
|----------------------|-----------------------------------------------------------------------------------------------------|
| Source migration     | `cargo test --workspace --locked` -> 13 passed                                                      |
| Local dev workflow   | `./scripts/build-skills.sh` -> `skills/codemap/scripts/codemap --help` works                        |
| Install validation   | `SKILLS_TARGET="evil/../../etc/passwd" ./scripts/install.sh` exits 1 with allowlist error           |
| Install graceful 404 | `./scripts/install.sh v0.0.0-noexist` skips per-skill 404s, exits 0 (or "installed 0 skill(s)")     |
| YAML syntax          | `python3 -c 'import yaml; yaml.safe_load(open(".github/workflows/release.yml"))'` -> ok             |
| Release (real)       | Out of band: push `v0.1.1` tag, watch all 6 jobs go green, confirm assets + `.sha256` on the release |

The real-release verification cannot be dry-run; it is the first thing to do after merging this plan and before announcing the new install path.

## Follow-ups (out of scope here -- track separately if/when needed)

- Apple notarisation / sigstore signing.
- Windows targets.
- Universal2 macOS binary (if download size becomes a concern).
- Cosign-style transparency log for release assets.
- `SKILLS_REPO` allowlist for regulated-environment users.
