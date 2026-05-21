mod apply;
mod cli;
mod commands;
mod error;
mod output;
mod serialize;

use clap::Parser;
use cli::{Cli, Command};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Rename(a) => commands::rename::run(a)?,
    };
    std::process::exit(code);
}
