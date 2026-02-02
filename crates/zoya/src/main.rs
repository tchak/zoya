use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod commands;
mod eval;
mod repl;
mod runner;

#[derive(Parser)]
#[command(name = "zoya")]
#[command(version, about = "The Zoya programming language")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run a file
    Run {
        /// File to execute
        file: PathBuf,
    },
    /// Start the interactive REPL
    Repl,
    /// Type-check a file without executing
    Check {
        /// File to type-check
        file: PathBuf,
    },
    /// Compile a file to JavaScript
    Build {
        /// File to compile
        file: PathBuf,
        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Run { file }) => {
            if let Err(e) = commands::run::execute(&file) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Some(Command::Repl) => repl::run(),
        Some(Command::Check { file }) => {
            if let Err(e) = commands::check::execute(&file) {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
        Some(Command::Build { file, output }) => {
            if let Err(e) = commands::build::execute(&file, output.as_deref()) {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
        None => {
            println!("Zoya language - use --help for usage");
        }
    }
}
