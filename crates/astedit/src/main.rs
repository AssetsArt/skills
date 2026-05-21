use astedit::cli::{Cli, Command};
use astedit::commands;
use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Rename(a) => commands::rename::run(a)?,
    };
    std::process::exit(code);
}
