---
name: ny-astedit
description: PREFER THIS over manual `sed`/`grep -rl … | xargs sed`/multi-file Edit batches whenever you need to rename a symbol across files. `astedit rename <OLD> <NEW>` parses the project with tree-sitter via codegraph, resolves cross-file imports with confidence scores, and rewrites only the references the resolver vouches for. Dry-run by default; pass `--apply` to write. Atomic per-file writes, length-based drift detection with SHA-256 fallback. Trigger BEFORE running `sed -i s/X/Y/g`, `rg -l X | xargs sed`, or chains of `Edit` calls renaming the same identifier. Also use when the user asks "rename X to Y", "เปลี่ยนชื่อ symbol", "rename this struct/fn/class across the project", "เปลี่ยน X เป็น Y ทั้งโปรเจกต์". Also use `astedit rewrite --pattern P --rewrite R` for structural codemods — ast-grep pattern syntax with `$X` and `$$$ARGS` metavars, dry-run by default. Trigger BEFORE running ad-hoc `sed`, hand-coded ast-grep CLI invocations, or chained `Edit` calls that match an AST shape rather than a single identifier. Also use when the user says 'rewrite all calls to X with Y', 'apply this codemod', 'replace pattern P with R', 'เขียนใหม่ทุก call ของ X', 'แก้ pattern P ทั่วโปรเจกต์', 'apply a structural change across N files'. Returns `{schema_version:1, data:{applied,skipped,errors}}` JSON. Supports Rust, TypeScript, TSX, JavaScript, Python.
---

# astedit

`astedit` is the write-side companion to `codegraph` in the `skills` monorepo. `codegraph` answers "where is X used?" without writing anything; `astedit` answers "rewrite all those sites to Y" with a safety model designed for agents that chain tool calls without inspecting diffs between them.

## When to use

- The user says "rename X to Y", "rename this symbol across the project", "เปลี่ยนชื่อ X เป็น Y".
- You are about to run `sed -i s/Old/New/g` across multiple files. **Stop.** Run `astedit rename Old New` instead — it disambiguates definitions, respects import boundaries, and reports the per-file changes structurally.
- You are about to issue a series of `Edit` calls renaming the same identifier in N files. Use `astedit rename` and inspect the dry-run envelope first.

`astedit` is not rust-analyzer / tsserver. It does not resolve types, expand macros, or chase re-exports. References that traverse alias or wildcard re-exports show up under `skipped[]` so you can review them manually.

## Run

The skill ships a pre-built binary:

```bash
./scripts/astedit rename <OLD> <NEW> [flags]
```

### If the binary is missing — download from Releases

Releases live at <https://github.com/AssetsArt/skills/releases>. Each binary is published as `<bin>-<tag>-<slug>.tar.gz` paired with a `<bin>-<tag>-<slug>.sha256`. For this skill, `<bin>` is `astedit`.

Detect your machine first:

```bash
uname -sm
# Maps to <slug>:
#   "Darwin arm64"   → macos-aarch64       (Apple Silicon M1/M2/M3/M4)
#   "Darwin x86_64"  → macos-x86_64        (Intel Mac)
#   "Linux x86_64"   → linux-gnu-x86_64    (use linux-musl-x86_64 for static / Alpine)
#   "Linux aarch64"  → linux-gnu-aarch64   (use linux-musl-aarch64 for static / Alpine)
```

Then download, verify, and install:

```bash
BIN=astedit
SLUG=<slug from table above>
# Latest tag is resolved from the GitHub releases redirect — open
# https://github.com/AssetsArt/skills/releases/latest in a browser to inspect
# the current tag and asset list manually.
TAG=$(basename "$(curl -fsSLI -o /dev/null -w '%{url_effective}' \
        https://github.com/AssetsArt/skills/releases/latest)")
BASE="https://github.com/AssetsArt/skills/releases/download/$TAG"

curl -fsSLO "$BASE/$BIN-$TAG-$SLUG.tar.gz"
curl -fsSLO "$BASE/$BIN-$TAG-$SLUG.sha256"
# macOS: shasum -a 256 -c "$BIN-$TAG-$SLUG.sha256"
# Linux: sha256sum -c "$BIN-$TAG-$SLUG.sha256"
tar -xzf "$BIN-$TAG-$SLUG.tar.gz"
mkdir -p scripts && mv "$BIN" "scripts/$BIN" && chmod +x "scripts/$BIN"
```

Drop the binary anywhere on your `$PATH` if you prefer global install.

## Subcommand: `rename`

```
astedit rename <OLD> <NEW>  [--path DIR]  [--apply]  [--json]
                            [--lang LANG] [--anchor FILE:LINE]
```

- `--path DIR` — project root to scan; default current directory.
- `--apply` — actually write edits. Without it, astedit reports what it *would* do and exits without writing.
- `--json` — emit `{schema_version:1, data:…}` instead of the human-readable preview.
- `--lang LANG` — restrict to one language (`rust`, `typescript`, `javascript`, `python`).
- `--anchor FILE:LINE` — required when `<OLD>` has more than one definition. Pass `--anchor src/user.rs:12` to pick the definition at that location.

## Subcommand: `rewrite`

```
astedit rewrite --pattern P --rewrite R  [--path DIR]  [--apply]
                                         [--json]    [--lang LANG]
```

Structural pattern→rewrite using ast-grep syntax. Unlike `rename`, this does not consult `codegraph` — every match is an AST-shape exact match, so every edit is implicitly high-confidence. The JSON envelope's `applied[].edits[]` omits `confidence` and `reason` fields accordingly.

- `--pattern P` — ast-grep pattern. Metavars: `$X` (single node), `$$$X` (multiple).
- `--rewrite R` — replacement template; metavars from `--pattern` are substituted in.
- `--path DIR` — project root to scan (default: current directory).
- `--apply` — actually write edits. Without this flag, astedit reports what it *would* do.
- `--json` — emit `{schema_version:1, data:…}` instead of human-readable preview.
- `--lang LANG` — restrict to one language (`rust`, `typescript`, `tsx`, `javascript`, `python`). Without it, every supported file extension is scanned.

If `--pattern` or `--rewrite` fails to compile for any language scanned, astedit exits non-zero and the JSON envelope's `errors[]` carries an entry with `error_kind: "pattern-compile"` and `lang: "<language>"`. Other failure modes (concurrent writes, atomic-write errors) are reported in `errors[]` but **do not** abort the run — sed-like semantics.

## Safety model

- **Dry-run by default.** No writes unless `--apply` is passed.
- **Atomic per-file writes.** Temp file in the same directory + `rename(2)`. No partial writes on crash.
- **Length-based drift detection.** Pre-flight stats each file and compares against the index snapshot. Mismatch ⇒ SHA-256 fallback; persistent mismatch ⇒ `error_kind: "hash-mismatch"`, skip the file.
- **Race-window guard.** Just before the atomic write, re-stat length. Same-length concurrent writes slip through (accepted trade-off — git is the final accountability layer).
- **No built-in change-count cap.** A 200-file rename of a project-wide util is legitimate. Trust the dry-run preview.

## JSON envelope

`--json` emits exactly this shape:

```json
{
  "schema_version": 1,
  "data": {
    "subcommand": "rename",
    "dry_run": true,
    "applied": [{"file": "src/lib.rs", "bytes_changed": 12, "edits": [...]}],
    "skipped": [{"file": "...", "skip_reason": "low-confidence", ...}],
    "errors":  [{"error_kind": "hash-mismatch", "file": "..."}]
  }
}
```

Multi-def disambiguation: when `<OLD>` has multiple definitions and `--anchor` is absent, `data` wraps `needs_anchor: true` + a `candidates` array, and the process exits non-zero. Use one of the candidates' `file:line` as `--anchor` and retry.

Exit status:
- `0` — invocation valid; `applied` may be empty.
- non-zero — multi-def without `--anchor`, or every targeted file ended up in `errors[]` with no successful applies.

## Out of scope (today)

- Recipe files (`astedit apply recipe.yaml`) — future work.
- Type-aware rename (would need rust-analyzer / tsserver embeddings) — future work.
