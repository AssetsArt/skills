# codemap

A small, fast CLI that uses [tree-sitter](https://tree-sitter.github.io/) to answer "what's in this codebase?" without making you open every file.

Part of the [`skills`](../../README.md) monorepo.

## Install

```bash
cargo build --release -p codemap
# binary at ../../target/release/codemap (relative to this crate)
```

Or, from the workspace root:

```bash
cargo install --path skills/codemap
```

## JSON envelope

Every JSON response is wrapped in a stable v1 envelope:

```json
{ "schema_version": 1, "data": <payload> }
```

Future schema changes will bump `schema_version`. Read `data` and assert the version.

## Commands

### `codemap files [--json] [--path DIR]`

List source files grouped by language.

```
$ codemap files
python (1 files)
  app.py  10 lines  123 B
rust (1 files)
  src/lib.rs  21 lines  345 B
typescript (1 files)
  src/types.ts  15 lines  220 B
```

JSON `data` shape:

```json
[
  { "path": "src/lib.rs", "language": "rust", "lines": 21, "size_bytes": 345 }
]
```

### `codemap tree [--json] [--path DIR]`

Directory outline honouring `.gitignore` and skipping `target/`, `node_modules/`, `.git/`, `dist/`, `build/`.

### `codemap symbols <FILE | . | --all> [--kind ...] [--json] [--path DIR]`

Extract top-level symbols. Pass a file (resolved against `--path`) for single-file mode, or `.` / `--all` for whole-project.

Filter with `--kind fn,struct,...`. Supported kinds: `fn`, `struct`, `enum`, `trait`, `class`, `interface`, `type`, `const`.

JSON `data` element:

```json
{
  "file": "src/lib.rs",
  "name": "Greeter",
  "kind": "struct",
  "start_line": 1,
  "end_line": 3,
  "signature": "pub struct Greeter {"
}
```

### `codemap find <NAME> [--exact] [--json] [--path DIR]`

Find a symbol by name across the project. Substring match by default; `--exact` for equality.

### `codemap stats [--json] [--path DIR]`

JSON `data` shape:

```json
{
  "total_files": 5,
  "total_lines": 71,
  "languages": { "rust": { "files": 1, "lines": 21 }, "python": { "files": 1, "lines": 10 } },
  "symbols": { "fn": 4, "struct": 1, "class": 2, "interface": 2 }
}
```

## Supported languages

| Language | Extensions | Kinds extracted |
| --- | --- | --- |
| Rust | `.rs` | fn, struct, enum, trait, type, const, static |
| TypeScript | `.ts` | fn, class, interface, type, enum, const |
| TSX | `.tsx` | fn, class, interface, type, enum, const |
| JavaScript | `.js`, `.mjs`, `.cjs` | fn, class |
| Python | `.py` | fn (module-level), class |

## Adding a new language

1. Add the grammar crate to `Cargo.toml` (e.g. `tree-sitter-go = "0.21"`).
2. Add an extension mapping in `src/lang.rs` (`from_extension` + `ts_language`).
3. Write a `src/queries/<lang>.scm` using the `@symbol.<kind>` / `@name` convention.
4. Wire `Language::query_source` to `Some(include_str!("queries/<lang>.scm"))`.
5. Add a fixture under `tests/fixtures/sample_project/` and a test in `tests/symbols_test.rs`.

## License

MIT — see [LICENSE](../../LICENSE).
