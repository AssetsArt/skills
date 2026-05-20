# skills

> AssetsArt's collection of agent-callable CLI tools, written in Rust.

`skills` is a Cargo workspace where each member is a small, focused command-line tool designed to be invoked by AI coding agents (e.g. Claude Code). Every skill ships:

- a single self-contained binary,
- a `SKILL.md` manifest the agent reads to know when and how to call it,
- a human-facing `README.md` with examples.

## Skill Index

| Skill | What it does | Crate |
| --- | --- | --- |
| [ny-codemap](./skills/ny-codemap) | Survey a codebase: list files, show symbols, find definitions | `crates/codemap` |

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

Release tarballs will be paired with `.sha256` companions generated in the same CI job that builds them. `install.sh` verifies the checksum and refuses tarballs containing absolute paths or `..` segments before extraction. Code signing (Apple notarisation, sigstore) is intentionally out of scope for this revision.

## Adding a new skill

1. Create `skills/<your-skill>/` with its own `Cargo.toml` (use `codemap` as a template).
2. Inherit shared metadata via `package.license.workspace = true`, etc.
3. Add a `SKILL.md` (frontmatter `name` + `description`) so agents can discover it.
4. Add a row to the table above.

## Versioning

The workspace uses `version.workspace = true`, so all skills bump together. This is the intended default — release coordination lives at the workspace level. If you need per-skill versioning later, drop `version.workspace` on the specific crate.

## License

MIT — see [LICENSE](./LICENSE).
