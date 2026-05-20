# skills

> AssetsArt's collection of agent-callable CLI tools, written in Rust.

`skills` is a Cargo workspace where each member is a small, focused command-line tool designed to be invoked by AI coding agents (e.g. Claude Code). Every skill ships:

- a single self-contained binary,
- a `SKILL.md` manifest the agent reads to know when and how to call it,
- a human-facing `README.md` with examples.

## Skill Index

| Skill | What it does | Crate |
| --- | --- | --- |
| [codemap](./skills/codemap) | Survey a codebase: list files, show symbols, find definitions | `skills/codemap` |

## Building

```bash
cargo build --release
# or build a single skill:
cargo build --release -p codemap
```

Binaries land in `target/release/<skill-name>`.

## Adding a new skill

1. Create `skills/<your-skill>/` with its own `Cargo.toml` (use `codemap` as a template).
2. Inherit shared metadata via `package.license.workspace = true`, etc.
3. Add a `SKILL.md` (frontmatter `name` + `description`) so agents can discover it.
4. Add a row to the table above.

## Versioning

The workspace uses `version.workspace = true`, so all skills bump together. This is the intended default — release coordination lives at the workspace level. If you need per-skill versioning later, drop `version.workspace` on the specific crate.

## License

MIT — see [LICENSE](./LICENSE).
