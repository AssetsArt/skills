#!/usr/bin/env bash
# Build every skill (skills/ny-<name>/) by compiling the crate of the same
# name and copying the release binary into skills/ny-<name>/scripts/<name>.
#
# Iteration walks `skills/ny-*/`, NOT `crates/*/`. Lib-only crates (e.g.
# codegraph-core) have no matching skill dir and are not iterated. The
# CI release.yml runs a separate audit that fails if any crate with a
# [[bin]] section lacks a skill dir.
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
shopt -s nullglob
for skill_dir in skills/ny-*/; do
  skill_dir="${skill_dir%/}"               # strip trailing /
  name="${skill_dir#skills/ny-}"           # ny-codemap -> codemap
  binary="target/release/$name"
  if [ ! -f "$binary" ]; then
    echo "error: $binary missing; the crate may not declare [[bin]]" >&2
    exit 1
  fi
  mkdir -p "$skill_dir/scripts"
  cp "$binary" "$skill_dir/scripts/$name"
  chmod +x "$skill_dir/scripts/$name"
  echo "built $skill_dir/scripts/$name"
  built=$((built + 1))
done
echo "done: $built skill binary(ies)"
