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

### One-liner (no clone)

```bash
curl -fsSL https://raw.githubusercontent.com/AssetsArt/skills/main/scripts/install.sh | sh
# pin a specific version:
curl -fsSL https://raw.githubusercontent.com/AssetsArt/skills/main/scripts/install.sh | sh -s -- v0.1.2
```

The script fetches the repo's source tarball at the resolved tag (no `git` required) and stages each skill into `~/.claude/skills/ny-<name>/`. Override the destination with `CLAUDE_SKILLS_DIR=/some/path`.

### From a checkout

```bash
./scripts/install.sh           # downloads the latest release for your platform
./scripts/install.sh v0.1.1    # or pin a specific version
```

Either entry point lands the binary at `skills/ny-<name>/scripts/<name>` and also copies the skill dir into `~/.claude/skills/ny-<name>/` so Claude can discover it. If a `~/.claude/skills/ny-<name>` already exists as a symlink (manual setup), the script leaves it alone.

Supported asset slugs: `linux-gnu-x86_64`, `linux-gnu-aarch64`, `linux-musl-x86_64`, `linux-musl-aarch64`, `macos-x86_64`, `macos-aarch64`. The script auto-detects the right slug from `uname` + libc probe; override with `SKILLS_TARGET=<slug>` if you need to (e.g. installing into an Alpine container from a glibc host). If you hit GitHub's 60/hr unauthenticated API rate limit, export `GITHUB_TOKEN` before running.

## Build from source (developers)

```bash
./scripts/build-skills.sh
```

Builds every workspace crate that has a matching `skills/<name>/` directory in `--release` mode and copies each binary into `skills/<name>/scripts/<name>`. The same layout `install.sh` produces.

## Security / integrity

Release tarballs will be paired with `.sha256` companions generated in the same CI job that builds them. `install.sh` verifies the checksum and refuses tarballs containing absolute paths or `..` segments before extraction. Code signing (Apple notarisation, sigstore) is intentionally out of scope for this revision.

## Adding a new skill

The repo follows two conventions: source lives in `crates/<name>/`, and the agent-installed surface lives in `skills/ny-<name>/`. The mapping `skill_dir == "ny-" + crate_name` is load-bearing — `build-skills.sh`, `install.sh`, and `release.yml` all rely on it.

1. Create `crates/<name>/` (with `Cargo.toml`, `src/main.rs`, etc. — use `crates/codemap` as a template). Inherit shared metadata via `version.workspace = true`, `license.workspace = true`, etc.
2. Create `skills/ny-<name>/SKILL.md` with frontmatter `name: ny-<name>` and a `description` field so agents can discover it. Reference the binary as `./scripts/<name>` (the pre-built file lives at `skills/ny-<name>/scripts/<name>`; the `scripts/` directory is gitignored).
3. Add a row to the Skill Index table above: link `[ny-<name>](./skills/ny-<name>)`, crate column `` `crates/<name>` ``.
4. Run `./scripts/build-skills.sh` to verify the local build pipeline picks up the new skill. The release workflow does not require changes — its matrix is per-target, not per-skill.

## Versioning

The workspace uses `version.workspace = true`, so all skills bump together. This is the intended default — release coordination lives at the workspace level. If you need per-skill versioning later, drop `version.workspace` on the specific crate.

## License

MIT — see [LICENSE](./LICENSE).
