use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod commands;

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
        /// Path to a .zoya file or directory with package.toml (defaults to current directory)
        path: Option<PathBuf>,
    },
    /// Start the interactive REPL
    Repl {
        /// Path to a .zoya file or directory with package.toml (defaults to current directory)
        path: Option<PathBuf>,
    },
    /// Type-check a file without executing
    Check {
        /// Path to a .zoya file or directory with package.toml (defaults to current directory)
        path: Option<PathBuf>,
    },
    /// Compile a file to JavaScript
    Build {
        /// Path to a .zoya file or directory with package.toml (defaults to current directory)
        path: Option<PathBuf>,
        /// Output file (overrides package.toml output)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Format source files
    Fmt {
        /// Path to a .zoya file or directory (defaults to current directory)
        path: Option<PathBuf>,
        /// Check if files are formatted without writing (exit 1 if not)
        #[arg(long)]
        check: bool,
    },
    /// Create a new Zoya project
    New {
        /// Path to create the project at
        path: PathBuf,
        /// Package name (defaults to directory name sanitized)
        #[arg(short, long)]
        name: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Run { path }) => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            if let Err(e) = commands::run::execute(&path) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Some(Command::Repl { path }) => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            commands::repl::execute(&path);
        }
        Some(Command::Check { path }) => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            if let Err(e) = commands::check::execute(&path) {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
        Some(Command::Build { path, output }) => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            if let Err(e) = commands::build::execute(&path, output.as_deref()) {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
        Some(Command::Fmt { path, check }) => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            if let Err(e) = commands::fmt::execute(&path, check) {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
        Some(Command::New { path, name }) => {
            if let Err(e) = commands::new::execute(&path, name.as_deref()) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        None => {
            println!("Zoya language - use --help for usage");
        }
    }
}
