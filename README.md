# skills

> AssetsArt's collection of agent-callable skills — Rust CLI tools plus process orchestration skills.

`skills` is a Cargo workspace plus a skill registry. Most members are small, focused command-line tools written in Rust; a few are process skills with no binary at all (orchestration patterns the agent invokes by reading a `SKILL.md`). Every skill ships:

- a `SKILL.md` manifest the agent reads to know when and how to call it,
- (for CLI skills) a single self-contained binary built from a sibling `crates/<name>` workspace member,
- a human-facing `README.md` with examples.

## Skill Index

| Skill | Kind | What it does | Crate |
| --- | --- | --- | --- |
| [ny-codemap](./skills/ny-codemap) | CLI | Survey a codebase: list files, show symbols, find definitions | `crates/codemap` |
| [ny-codegraph](./skills/ny-codegraph) | CLI | Semantic cross-references: find-refs, callers, callees, impact | `crates/codegraph` |
| [ny-astedit](./skills/ny-astedit) | CLI | AST-validated rewrites: `rename` (cross-file symbol rename via codegraph) and `rewrite` (structural pattern→rewrite via ast-grep). Dry-run by default; atomic per-file writes. | `crates/astedit` |
| [ny-auto-pipeline](./skills/ny-auto-pipeline) | Process | Autonomous brainstorm → spec → spec-review → plan → subagent-driven impl → scrutinize → post-mortem. Overrides the interactive gates of `superpowers:brainstorming` / `writing-plans` when the user has granted autonomous run authority. Composes with `9arm-skills:scrutinize` / `post-mortem` / `debug-mantra` and `superpowers:verification-before-completion`. | — |

## Install (end users)

### Via Claude Code plugin (recommended — auto-updates the SKILL manifests)

In a Claude Code session:

```text
/plugin marketplace add AssetsArt/skills
/plugin install ny-skills
```

Claude Code refreshes the SKILL manifests on every restart and on `/reload-plugins`, so pushes to `main` reach agents automatically. The plugin ships `SKILL.md` + query files; the compiled CLI binary still needs to be fetched once with the installer below.

### One-liner (no clone) — also fetches the platform binary

```bash
curl -fsSL https://raw.githubusercontent.com/AssetsArt/skills/main/scripts/install.sh | sh
# pin a specific version:
curl -fsSL https://raw.githubusercontent.com/AssetsArt/skills/main/scripts/install.sh | sh -s -- v0.4.1
```

The script fetches the repo's source tarball at the resolved tag (no `git` required) and stages each skill into `~/.claude/skills/ny-<name>/`. Override the destination with `CLAUDE_SKILLS_DIR=/some/path`. Use this in addition to the plugin install above (or on its own) — the plugin handles SKILL.md updates, this handles binary updates.

### From a checkout

```bash
./scripts/install.sh           # downloads the latest release for your platform
./scripts/install.sh v0.4.1    # or pin a specific version
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

Two flavours.

### CLI skills (the common case)

Source lives in `crates/<name>/`, and the agent-installed surface lives in `skills/ny-<name>/`. The mapping `skill_dir == "ny-" + crate_name` is load-bearing — `build-skills.sh`, `install.sh`, and `release.yml` all rely on it.

1. Create `crates/<name>/` (with `Cargo.toml`, `src/main.rs`, etc. — use `crates/codemap` as a template). Inherit shared metadata via `version.workspace = true`, `license.workspace = true`, etc.
2. Create `skills/ny-<name>/SKILL.md` with frontmatter `name: ny-<name>` and a `description` field so agents can discover it. Reference the binary as `./scripts/<name>` (the pre-built file lives at `skills/ny-<name>/scripts/<name>`; the `scripts/` directory is gitignored).
3. Add a row to the Skill Index table above with Kind = `CLI`.
4. Run `./scripts/build-skills.sh` to verify the local build pipeline picks up the new skill. The release workflow does not require changes — its matrix is per-target, not per-skill.

### Process skills (no binary)

Sometimes the skill is an orchestration pattern, not a CLI. Example: `ny-auto-pipeline`. These ship a `SKILL.md` only.

1. Create `skills/ny-<name>/SKILL.md` with frontmatter `name: ny-<name>` and a `description` that lists trigger phrases (English + Thai if applicable) plus negative cases.
2. No `crates/<name>/` directory; no entry in the workspace `Cargo.toml`.
3. Add a row to the Skill Index table above with Kind = `Process` and crate column `—`.
4. `build-skills.sh` and `install.sh` skip directories without a `scripts/` subdir automatically — no other infra changes needed.

YAML frontmatter discipline: if the `description` value contains `: ` (colon + space) anywhere, **wrap the whole value in double quotes** — YAML will otherwise read the colon as a key/value separator and the manifest will fail to load.

## Versioning

The workspace uses `version.workspace = true`, so all skills bump together. This is the intended default — release coordination lives at the workspace level. If you need per-skill versioning later, drop `version.workspace` on the specific crate.

## License

MIT — see [LICENSE](./LICENSE).
