#!/usr/bin/env sh
# Download pre-built skill binaries from a GitHub Release and drop one into
# each skills/ny-<crate>/scripts/<crate>. POSIX sh on purpose -- this runs on
# bare Alpine / minimal images where bash isn't guaranteed.
#
# Usage:
#   ./scripts/install.sh                 # latest release, auto-detect target
#   ./scripts/install.sh v0.1.0          # pinned version
#   SKILLS_TARGET=macos-aarch64 ./scripts/install.sh
#   SKILLS_REPO=fork/skills GITHUB_TOKEN=... ./scripts/install.sh
#   CLAUDE_SKILLS_DIR=/path/to/skills ./scripts/install.sh   # override register dest
set -eu

# Closed allowlist. Anything outside this set is rejected -- both the auto-
# detected target and any SKILLS_TARGET override. Keep in sync with release.yml.
SUPPORTED_SLUGS="linux-gnu-x86_64 linux-gnu-aarch64 linux-musl-x86_64 linux-musl-aarch64 macos-x86_64 macos-aarch64"

print_supported() {
  echo "supported targets:" >&2
  for s in $SUPPORTED_SLUGS; do echo "  $s" >&2; done
}

# Pre-flight: refuse to start if we can't do the three things we need to do
# (fetch, unpack, verify). Fail loud so the user fixes their box, not us.
need_cmd() {
  command -v "$1" >/dev/null 2>&1 || { echo "error: required command not found: $1" >&2; exit 1; }
}
need_cmd curl
need_cmd tar
if command -v sha256sum >/dev/null 2>&1; then
  SHA_CMD="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
  SHA_CMD="shasum -a 256"
else
  echo "error: required command not found: sha256sum or shasum" >&2
  exit 1
fi

# Auto-detect slug from uname. Anything we don't recognise drops through to the
# allowlist check below, which prints the supported list and exits.
detect_slug() {
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os" in
    Darwin)
      case "$arch" in
        arm64|aarch64) echo "macos-aarch64" ;;
        x86_64|amd64)  echo "macos-x86_64" ;;
        *) echo "" ;;
      esac
      ;;
    Linux)
      # musl detection: ldd --version exits non-zero on musl, so we check stderr
      # too, plus the canonical loader paths as a belt-and-braces fallback.
      # Note: on scratch-based musl containers that ship neither `ldd` nor
      # /lib/ld-musl-*.so.1, detection falls through to "gnu". Set
      # SKILLS_TARGET=linux-musl-x86_64 (or -aarch64) explicitly in that case.
      if (ldd --version 2>&1 | grep -qi musl) \
         || [ -f /lib/ld-musl-x86_64.so.1 ] \
         || [ -f /lib/ld-musl-aarch64.so.1 ]; then
        libc="musl"
      else
        libc="gnu"
      fi
      case "$arch" in
        x86_64|amd64)  echo "linux-$libc-x86_64" ;;
        aarch64|arm64) echo "linux-$libc-aarch64" ;;
        *) echo "" ;;
      esac
      ;;
    *) echo "" ;;
  esac
}

# SECURITY: validate slug against the closed allowlist BEFORE it touches any
# URL, filename, or shell expansion. SKILLS_TARGET is user-controlled; without
# this guard a value like "..; rm -rf /" or "x/../../etc/passwd" would land in
# a curl URL further down. Do this first, then trust the value.
slug_allowed() {
  for s in $SUPPORTED_SLUGS; do
    [ "$1" = "$s" ] && return 0
  done
  return 1
}

if [ -n "${SKILLS_TARGET:-}" ]; then
  if ! slug_allowed "$SKILLS_TARGET"; then
    echo "error: SKILLS_TARGET='$SKILLS_TARGET' is not a known slug" >&2
    print_supported
    exit 1
  fi
  slug="$SKILLS_TARGET"
else
  slug="$(detect_slug)"
  if [ -z "$slug" ] || ! slug_allowed "$slug"; then
    echo "error: could not detect a supported target (uname -s=$(uname -s), uname -m=$(uname -m))" >&2
    print_supported
    echo "hint: set SKILLS_TARGET to one of the above to force a slug" >&2
    exit 1
  fi
fi

repo="${SKILLS_REPO:-AssetsArt/skills}"
echo "skills repo: https://github.com/$repo" >&2

# Auth header for the API call only -- release downloads are public.
# Token-gated so unauthenticated users still work up to 60 req/hr.
api_curl() {
  if [ -n "${GITHUB_TOKEN:-}" ]; then
    curl -fsSL --tlsv1.2 -H "Authorization: Bearer $GITHUB_TOKEN" "$@"
  else
    curl -fsSL --tlsv1.2 "$@"
  fi
}

# Resolve version: first positional arg wins; otherwise latest from the API.
if [ "${1:-}" != "" ]; then
  tag="$1"
else
  api_url="https://api.github.com/repos/$repo/releases/latest"
  release_json="$(api_curl "$api_url" 2>/dev/null)" || {
    echo "error: could not fetch $api_url" >&2
    echo "hint: if you're rate-limited, set GITHUB_TOKEN to a personal access token" >&2
    exit 1
  }
  # POSIX-grep extraction -- avoids a jq dependency.
  tag="$(printf '%s\n' "$release_json" | grep -o '"tag_name"[[:space:]]*:[[:space:]]*"[^"]*"' | head -1 | sed 's/.*"\([^"]*\)"$/\1/')"
  if [ -z "$tag" ]; then
    echo "error: could not parse tag_name from $api_url response" >&2
    exit 1
  fi
fi

echo "installing version: $tag (slug $slug)" >&2

# Only chdir into the script's containing checkout when $0 actually points at a
# file path -- under `curl | sh`, $0 is the interpreter name (e.g. "sh") with no
# directory component, and `cd "$(dirname sh)/.."` would resolve to the parent
# of the user's CWD and then iterate whatever sits there. The bootstrap branch
# below handles the no-local-checkout case explicitly.
case "$0" in
  */*)
    if [ -f "$0" ]; then
      repo_root="$(cd "$(dirname "$0")/.." && pwd)"
      cd "$repo_root"
    fi
    ;;
esac

# Bootstrap (curl | sh mode): when the current dir doesn't look like a skills
# checkout (no skills/ny-*/SKILL.md visible), pull the source tarball at the
# resolved tag and chdir into it so the per-skill walk below has skills/ny-*/
# to discover. curl + tar only -- no git dependency.
have_local_checkout=0
for skill_md in skills/ny-*/SKILL.md; do
  [ -f "$skill_md" ] || continue
  have_local_checkout=1
  break
done

bootstrap_dir=""
if [ "$have_local_checkout" = "0" ]; then
  echo "no skills/ny-* checkout at $(pwd); bootstrapping from $repo @ $tag" >&2
  bootstrap_dir="$(mktemp -d 2>/dev/null || mktemp -d -t skills-bootstrap)"
  if ! api_curl -o "$bootstrap_dir/repo.tar.gz" "https://api.github.com/repos/$repo/tarball/$tag" 2>/dev/null; then
    rm -rf "$bootstrap_dir"
    echo "error: failed to fetch tarball https://api.github.com/repos/$repo/tarball/$tag" >&2
    exit 1
  fi
  tar -xzf "$bootstrap_dir/repo.tar.gz" -C "$bootstrap_dir"
  # GitHub tarballs expand to <owner>-<repo>-<sha7>/. Pick the first dir.
  bootstrap_root=""
  for d in "$bootstrap_dir"/*/; do
    [ -d "$d" ] || continue
    bootstrap_root="${d%/}"
    break
  done
  if [ -z "$bootstrap_root" ] || [ ! -d "$bootstrap_root/skills" ]; then
    rm -rf "$bootstrap_dir"
    echo "error: source tarball for $tag did not contain a skills/ dir" >&2
    exit 1
  fi
  cd "$bootstrap_root"
fi

installed=0
for skill_dir in skills/*/; do
  [ -d "$skill_dir" ] || continue
  skill="$(basename "$skill_dir")"
  skill_src="${skill_dir%/}"

  # Convention: skill dir is "ny-<crate>"; the release asset uses the bare
  # crate name. Strip the prefix here so the URL is "codemap-<tag>-<slug>",
  # not "ny-codemap-<tag>-<slug>".
  crate="${skill#ny-}"
  if [ "$crate" = "$skill" ]; then
    echo "warn: skill '$skill' is not a 'ny-' skill; cannot map to a crate asset; skipping" >&2
    continue
  fi

  asset="$crate-$tag-$slug.tar.gz"
  sum="$crate-$tag-$slug.sha256"
  asset_url="https://github.com/$repo/releases/download/$tag/$asset"
  sum_url="https://github.com/$repo/releases/download/$tag/$sum"

  stage="$(mktemp -d 2>/dev/null || mktemp -d -t skills-install)"
  # Cleanup on every path out of this iteration. trap is per-iteration because
  # we want per-skill atomicity (a failed download must not poison the next).
  trap 'rm -rf "$stage"' EXIT INT TERM

  # Asset 404 is a soft skip: shell-only skills won't publish a binary, and a
  # release that ships some-but-not-all binaries is still useful.
  if ! curl -fsSL --tlsv1.2 -o "$stage/$asset" "$asset_url" 2>/dev/null; then
    echo "skip: no asset for $skill ($asset)" >&2
    rm -rf "$stage"
    trap - EXIT INT TERM
    continue
  fi

  # Checksum 404 with the tarball present is a partial release -- treat as
  # hard error so we never install an unverified binary.
  if ! curl -fsSL --tlsv1.2 -o "$stage/$sum" "$sum_url" 2>/dev/null; then
    echo "error: tarball downloaded but checksum missing: $sum_url" >&2
    rm -rf "$stage"
    exit 1
  fi

  # SECURITY: verify the checksum BEFORE we let tar touch the bytes. If the
  # archive is tampered with, we never reach extraction.
  expected="$(awk '{print $1}' "$stage/$sum")"
  actual="$(cd "$stage" && $SHA_CMD "$asset" | awk '{print $1}')"
  if [ "$expected" != "$actual" ]; then
    echo "error: checksum mismatch for $asset (expected $expected, got $actual)" >&2
    rm -rf "$stage"
    exit 1
  fi

  # SECURITY: audit tar entries for absolute paths and ../ traversal BEFORE
  # extracting. A malicious tarball could otherwise overwrite arbitrary files
  # outside the skill dir.
  # Audit covers POSIX entries: absolute (/...) and any segment containing
  # `..`. Windows drive prefixes (C:/...) are not caught -- we don't target
  # Windows in the release matrix.
  # Audit tar entries BEFORE extraction. Capture in two steps so a corrupt
  # archive (tar -tzf fails) is surfaced with a diagnostic rather than being
  # silently masked by the grep that follows it.
  tar_listing="$(tar -tzf "$stage/$asset")" || {
    echo "error: could not list $asset (corrupt or unreadable archive)" >&2
    rm -rf "$stage"
    exit 1
  }
  if printf '%s\n' "$tar_listing" | grep -E '(^/|(^|/)\.\.(/|$))' >/dev/null; then
    echo "error: refusing tarball with absolute or '..' entries: $asset" >&2
    rm -rf "$stage"
    exit 1
  fi

  tar -xzf "$stage/$asset" -C "$stage"
  if [ ! -f "$stage/$crate" ]; then
    echo "error: $asset did not contain expected binary '$crate' at archive root" >&2
    rm -rf "$stage"
    exit 1
  fi

  mkdir -p "$skill_src/scripts"
  chmod +x "$stage/$crate"
  mv -f "$stage/$crate" "$skill_src/scripts/$crate"

  # macOS quarantines downloaded binaries; strip the xattr so the user doesn't
  # get a Gatekeeper prompt on first run. Failure is fine (linux, no xattr).
  if [ "$(uname -s)" = "Darwin" ]; then
    xattr -d com.apple.quarantine "$skill_src/scripts/$crate" 2>/dev/null || true
  fi

  echo "installed: $skill_src/scripts/$crate" >&2

  # Register the skill in the user's Claude skills dir so it actually becomes
  # discoverable. Defaults to ~/.claude/skills; override CLAUDE_SKILLS_DIR for
  # custom installs. A pre-existing symlink is left alone -- it represents a
  # deliberate manual setup we shouldn't blow away.
  reg_root="${CLAUDE_SKILLS_DIR:-$HOME/.claude/skills}"
  mkdir -p "$reg_root"
  reg_dest="$reg_root/$skill"
  if [ -L "$reg_dest" ]; then
    echo "skip register: $reg_dest is a symlink (manual install); leaving alone" >&2
  else
    reg_tmp="$reg_dest.tmp.$$"
    rm -rf "$reg_tmp"
    cp -R "$skill_src" "$reg_tmp"
    rm -rf "$reg_dest"
    mv "$reg_tmp" "$reg_dest"
    echo "registered: $reg_dest" >&2
  fi

  installed=$((installed + 1))

  rm -rf "$stage"
  trap - EXIT INT TERM
done

if [ -n "$bootstrap_dir" ]; then
  rm -rf "$bootstrap_dir"
fi

echo "installed $installed skill(s) ($slug) at version $tag" >&2
