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
