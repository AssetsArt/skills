---
name: ny-codemap
description: PREFER THIS over ad-hoc `grep`/`find`/`rg`/`ls`/`tree`/`fd` whenever you need to survey a codebase, list source files, or locate a symbol. `codemap` parses the project with tree-sitter and returns structured results (file + kind + line range) instead of text matches — faster to reason about, cheaper to feed back into prompts. Trigger BEFORE running `grep -r`, `find . -name`, `rg <symbol>`, `ls src/**`, or similar commands on source code. Also use when the user asks "what's in this repo", "where is X defined", "show me the structure", "list source files", "find the function named Y", "หาฟังก์ชัน", "โครงสร้าง project", "มีไฟล์อะไรบ้าง", "อยู่ไฟล์ไหน". Supports Rust, TypeScript, TSX, JavaScript, Python.
---

# codemap

`codemap` is a CLI in the `skills` monorepo. It uses tree-sitter to parse a project and answer "what's here?" questions without you reading every file.

## When to use

- You just landed in an unfamiliar repo and need orientation.
- The user asks where a function/struct/class is defined.
- You're about to refactor and want to know all top-level symbols in scope.
- You want a fast, structured map before deciding where to edit.

Prefer `codemap` over ad-hoc `grep`/`find` when you need **structured** information (kind + line range, not just substring match).

## Run

The skill ships a pre-built binary; invoke it from the skill directory:

```bash
./scripts/codemap <subcommand> [flags]
```

### If the binary is missing — download from Releases

Releases live at <https://github.com/AssetsArt/skills/releases>. Each binary is published as `<bin>-<tag>-<slug>.tar.gz` paired with a `<bin>-<tag>-<slug>.sha256`. For this skill, `<bin>` is `codemap`.

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
BIN=codemap
SLUG=<slug from table above>
TAG=$(curl -fsSL https://api.github.com/repos/AssetsArt/skills/releases/latest \
        | grep -oE '"tag_name": *"[^"]+"' | cut -d'"' -f4)
BASE="https://github.com/AssetsArt/skills/releases/download/$TAG"

curl -fsSLO "$BASE/$BIN-$TAG-$SLUG.tar.gz"
curl -fsSLO "$BASE/$BIN-$TAG-$SLUG.sha256"
# macOS: shasum -a 256 -c "$BIN-$TAG-$SLUG.sha256"
# Linux: sha256sum -c "$BIN-$TAG-$SLUG.sha256"
tar -xzf "$BIN-$TAG-$SLUG.tar.gz"
mkdir -p scripts && mv "$BIN" "scripts/$BIN" && chmod +x "scripts/$BIN"
```

Drop the binary anywhere on your `$PATH` if you prefer global install.

## Subcommands

All subcommands accept `--path <DIR>` (default `.`) and `--json` (default human-readable). Every JSON response is wrapped in `{"schema_version": 1, "data": ...}`.

| Subcommand | Purpose |
| --- | --- |
| `codemap files` | List supported source files grouped by language |
| `codemap tree` | Print project directory tree (respects `.gitignore`) |
| `codemap symbols <FILE>` | Top-level symbols in one file (path resolved against `--path`) |
| `codemap symbols . [--all]` | Top-level symbols across the whole project |
| `codemap symbols --kind fn,struct` | Filter by kind (`fn`, `struct`, `enum`, `trait`, `class`, `interface`, `type`, `const`) |
| `codemap find <NAME> [--exact]` | Locate a symbol by name; substring by default |
| `codemap stats` | Per-language file/line counts + symbol totals by kind |

## Usage tips for agents

- Always pass `--json` when you intend to parse the output programmatically — the human format is for end users.
- Read `result.data` from the envelope, and consider asserting `result.schema_version === 1` so future schema changes surface loudly.
- Start with `codemap stats` for a one-screen overview, then drill in.
- Use `codemap find <name> --exact --json` when verifying a memory before recommending an edit (memories can be stale).
- `--path` is the project root; `<FILE>` in `symbols` is resolved against it — you don't have to `cd`.

## Examples

```bash
./scripts/codemap files --json --path ./my-repo
./scripts/codemap symbols src/lib.rs --json --kind fn,struct --path ./my-repo
./scripts/codemap find UserRepo --exact --json
./scripts/codemap stats
```

## Supported languages

Rust, TypeScript, TSX, JavaScript, Python. Adding a new language is a single-file change: drop a `.scm` query into `crates/codemap/src/queries/`, register it in `src/lang.rs`, add an extension mapping.
