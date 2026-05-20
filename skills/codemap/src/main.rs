mod cli;
mod commands;
mod lang;
mod output;
mod walk;

use clap::Parser;
use cli::{Cli, Command};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Files(a) => commands::files::run(a),
        Command::Tree(a) => commands::tree::run(a),
        Command::Symbols(a) => commands::symbols::run(a),
        Command::Find(a) => commands::find::run(a),
        Command::Stats(a) => commands::stats::run(a),
    }
}
