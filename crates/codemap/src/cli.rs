use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "codemap",
    version,
    about = "Survey a codebase: files, symbols, find, stats"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List source files grouped by language
    Files(FilesArgs),
    /// Print directory tree (respects .gitignore)
    Tree(TreeArgs),
    /// Extract symbols from a file (or whole project with `.` / `--all`)
    Symbols(SymbolsArgs),
    /// Find symbols by name across the project
    Find(FindArgs),
    /// Project statistics: files, lines, symbol counts per kind
    Stats(StatsArgs),
}

#[derive(clap::Args, Debug)]
pub struct FilesArgs {
    /// Project root
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    /// Output JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct TreeArgs {
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct SymbolsArgs {
    /// File to inspect (relative to --path), or "." for whole project (same as --all)
    pub target: Option<String>,
    /// Inspect whole project
    #[arg(long)]
    pub all: bool,
    /// Filter by kind (comma-separated: fn,struct,enum,trait,class,interface,type,const)
    #[arg(long, value_delimiter = ',')]
    pub kind: Vec<String>,
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct FindArgs {
    /// Symbol name (substring by default)
    pub name: String,
    /// Require exact match
    #[arg(long)]
    pub exact: bool,
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct StatsArgs {
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub json: bool,
}
