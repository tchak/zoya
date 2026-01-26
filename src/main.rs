use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod ast;
mod check;
mod codegen;
mod eval;
mod ir;
mod lexer;
mod parser;
mod repl;
mod runner;
mod types;

#[derive(Parser)]
#[command(name = "zoya")]
#[command(version, about = "The Zoya programming language")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start the interactive REPL or run a file
    Run {
        /// Optional file to execute
        file: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Run { file: Some(path) }) => {
            if let Err(e) = runner::run(&path) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Some(Command::Run { file: None }) => repl::run(),
        None => {
            println!("Zoya language - use --help for usage");
        }
    }
}
