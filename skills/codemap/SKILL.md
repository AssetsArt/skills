---
name: codemap
description: Use when exploring an unfamiliar codebase, surveying project structure, or locating definitions before editing — `codemap` lists source files, extracts top-level symbols (fn/struct/class/interface/type/enum/trait/const), finds a symbol by name across the project, and reports per-language stats. Trigger on phrases like "what's in this repo", "where is X defined", "show me the structure", "หาฟังก์ชัน", "โครงสร้าง project", "มีไฟล์อะไรบ้าง", "อยู่ไฟล์ไหน". Supports Rust, TypeScript, TSX, JavaScript, Python.
---

# codemap

`codemap` is a CLI in the `skills` monorepo. It uses tree-sitter to parse a project and answer "what's here?" questions without you reading every file.

## When to use

- You just landed in an unfamiliar repo and need orientation.
- The user asks where a function/struct/class is defined.
- You're about to refactor and want to know all top-level symbols in scope.
- You want a fast, structured map before deciding where to edit.

Prefer `codemap` over ad-hoc `grep`/`find` when you need **structured** information (kind + line range, not just substring match).

## Build

```bash
cargo build --release -p codemap
```

Binary lands at `target/release/codemap` (relative to the workspace root).

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
codemap files --json --path ./my-repo
codemap symbols src/lib.rs --json --kind fn,struct --path ./my-repo
codemap find UserRepo --exact --json
codemap stats
```

## Supported languages

Rust, TypeScript, TSX, JavaScript, Python. Adding a new language is a single-file change: drop a `.scm` query into `skills/codemap/src/queries/`, register it in `src/lang.rs`, add an extension mapping.
