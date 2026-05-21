mod cli;
mod commands;
mod index;
mod output;
mod resolve;
mod walk;

use clap::Parser;
use cli::{Cli, Command};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::FindRefs(a) => commands::find_refs::run(a),
        Command::Callers(a) => commands::callers::run(a),
        Command::Callees(a) => commands::callees::run(a),
        Command::Impact(a) => commands::impact::run(a),
    }
}
