use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "astedit",
    version,
    about = "AST-validated rename and structural rewrite for AI coding agents."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Rename a symbol across the project (dry-run by default; pass --apply to write).
    Rename(RenameArgs),
    /// Structural rewrite using ast-grep pattern syntax (dry-run by default; pass --apply to write).
    Rewrite(RewriteArgs),
}

#[derive(clap::Args, Debug)]
pub struct RenameArgs {
    /// The symbol's current name.
    pub old: String,
    /// The new name.
    pub new: String,
    /// Project root to scan (default: current directory).
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    /// Actually write edits to disk. Without this flag, astedit reports
    /// what it would do and exits.
    #[arg(long)]
    pub apply: bool,
    /// Emit `{schema_version:1, data:...}` JSON instead of human output.
    #[arg(long)]
    pub json: bool,
    /// Optional language hint (e.g. `rust`, `python`). Without this, every
    /// supported extension is scanned and dispatched per file.
    #[arg(long)]
    pub lang: Option<String>,
    /// Disambiguator for multi-def symbols. Format: `FILE:LINE`
    /// (the `file` value must match a definition's `file` field exactly —
    /// repo-relative, forward-slash-normalized).
    #[arg(long)]
    pub anchor: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct RewriteArgs {
    /// ast-grep pattern to match against the source AST.
    /// Metavars: `$X` captures a single node, `$$$X` captures multiple.
    #[arg(long)]
    pub pattern: String,

    /// Replacement template. May reference metavars captured by `--pattern`.
    #[arg(long)]
    pub rewrite: String,

    /// Project root to scan (default: current directory).
    #[arg(long, default_value = ".")]
    pub path: PathBuf,

    /// Actually write edits to disk. Without this flag, astedit reports
    /// what it would do and exits.
    #[arg(long)]
    pub apply: bool,

    /// Emit `{schema_version:1, data:...}` JSON instead of human output.
    #[arg(long)]
    pub json: bool,

    /// Restrict to one language (`rust`, `typescript`, `tsx`, `javascript`,
    /// `python`). Without this, every supported extension is scanned and
    /// the pattern is compiled per language on demand.
    #[arg(long)]
    pub lang: Option<String>,
}
