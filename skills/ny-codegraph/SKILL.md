---
name: ny-codegraph
description: Use when you need cross-reference answers a `grep` cannot give you — "where else is X used", "who calls Y", "what does Z call", "what would break if I change W". `codegraph` parses the project with tree-sitter and reports references, callers, callees, and transitive impact, each tagged with a `confidence` (`high`/`medium`/`low`) and a `reason` so you know whether to trust a hit. Trigger on phrases like "find references to", "who calls", "what does X call", "callers of", "callees of", "impact of", "what breaks if I change", "หาที่ใช้", "ใครเรียกฟังก์ชันนี้", "ถ้าแก้ X อะไรพัง". Supports Rust, TypeScript, TSX, JavaScript, Python.
---

# codegraph

`codegraph` is a CLI in the `skills` monorepo that uses tree-sitter to build an in-memory cross-reference index of a project, then answers four questions every refactor needs:

- **Find references** — every site where a symbol is used (calls, type positions, imports, definition).
- **Callers** — functions that call a given function.
- **Callees** — functions called by a given function.
- **Impact** — transitive callers + non-call usages.

## When to use

- The user asks "where is X used", "who calls Y", "what does Z call", "what breaks if I change W".
- You are about to refactor and need to know the blast radius before editing.
- You suspect a function is dead but want to confirm before deleting.
- A memory mentions a function; you want to verify it is still called somewhere.

`codegraph` is not rust-analyzer / tsserver. It does **not** resolve types, follow re-exports, or expand macros. Every hit carries a `confidence` and `reason` so you can downrank or expand uncertain answers.

## Run

The skill ships a pre-built binary:

```bash
./scripts/codegraph <subcommand> [flags]
```

If missing, run `./scripts/install.sh` (downloads from Releases) or `./scripts/build-skills.sh` (local cargo build) from the `skills` repo root.

## Subcommands

All take `--path <DIR>` (default `.`) and `--json` (default human-readable). JSON output wraps an array in `{"schema_version": 1, "data": [...]}`.

| Subcommand | Purpose |
| --- | --- |
| `codegraph find-refs <NAME>` | Every reference to `<NAME>` across the project |
| `codegraph callers <FN> [--depth N]` | Functions that call `<FN>` (BFS up to depth N, default 1) |
| `codegraph callees <FN> [--depth N]` | Functions called by `<FN>` |
| `codegraph impact <NAME>` | Transitive callers + non-call usages of `<NAME>` |

## Output schema

Each entry in `data` has at least:

```json
{
  "file": "src/lib.rs",
  "line": 42,
  "column": 12,
  "kind": "call" | "definition" | "import" | "reference",
  "name": "User",
  "context": "    let u = User::new();",
  "confidence": "high" | "medium" | "low",
  "reason": "same-file-scope" | "import-resolved" | "name-only"
}
```

`callers` / `callees` / `impact` add a `distance` field (0 for the queried symbol, 1 for direct callers/callees, etc.) and `callers` / `callees` add a `sites` array with the individual call locations.

## Confidence rules

- **`high` + `same-file-scope`** — the reference and a matching definition live in the same file.
- **`high` + `import-resolved`** — the file imports the symbol from the file that defines it.
- **`medium` + `import-resolved`** — a glob/wildcard import (`use foo::*`, `from foo import *`) plausibly carries the symbol.
- **`medium` + `name-only`** — name matches and exactly one definition exists project-wide.
- **`low` + `name-only`** — name matches; the resolver couldn't pin a definition.

`low` hits are useful as a "look here too" hint; don't take them as truth.

## Limits

- Method dispatch is name-only — `user.save()` looks like a reference to `save`, not specifically `User::save`.
- Rust macros are opaque — `println!` is a reference to `println`, the macro body is not walked.
- No persistent cache — every invocation re-parses the project. Plenty fast for repos up to ~50k LoC.
- TS re-exports (`export * from "./mod"`) are not followed.

## Usage tips for agents

- Always pass `--json` when parsing programmatically; assert `result.schema_version === 1`.
- Prefer `find-refs <name>` for "where is this used" — it returns definitions *and* references in one call.
- Use `callers <fn> --depth 3 --json` before suggesting a non-trivial signature change.
- For dead-code questions: if `callers --json` returns an empty array AND `find-refs --json` only contains the `definition` entry, the symbol is unused (within the limits above).
- Pair with `codemap symbols <file>` when you need a list of definitions in a single file.

## Examples

```bash
./scripts/codegraph find-refs UserRepo --json --path ./my-repo
./scripts/codegraph callers handleSignup --depth 3 --json
./scripts/codegraph callees buildIndex --json
./scripts/codegraph impact ConfigLoader --json
```

## Supported languages

Rust, TypeScript, TSX, JavaScript, Python — the same set as `codemap`. Adding a new language: drop three `<lang>_{defs,imports,refs}.scm` files into `crates/codegraph/src/queries/`, wire them in `src/lang.rs`, extend `module_matches` in `src/resolve.rs`.
