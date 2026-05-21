use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "codegraph",
    version,
    about = "Semantic cross-references: find-refs, callers, callees, impact"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Find references to <name> across the project
    FindRefs(FindRefsArgs),
    /// List functions that call <fn>
    Callers(CallersArgs),
    /// List functions called by <fn>
    Callees(CalleesArgs),
    /// Transitive callers + type users of <symbol>
    Impact(ImpactArgs),
}

#[derive(clap::Args, Debug)]
pub struct FindRefsArgs {
    pub name: String,
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct CallersArgs {
    pub name: String,
    /// Recursive depth (1 = direct callers only)
    #[arg(long, default_value_t = 1)]
    pub depth: usize,
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct CalleesArgs {
    pub name: String,
    #[arg(long, default_value_t = 1)]
    pub depth: usize,
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct ImpactArgs {
    pub name: String,
    #[arg(long, default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub json: bool,
}
