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
        /// Optional file to load modules from
        file: Option<PathBuf>,
    },
    /// Type-check a file without executing
    Check {
        /// Path to a .zoya file or directory with package.toml (defaults to current directory)
        path: Option<PathBuf>,
    },
    /// Compile a file to JavaScript
    Build {
        /// File to compile
        file: PathBuf,
        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,
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
            let entry_point = match commands::resolve::resolve_entry_point(path.as_deref()) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };
            if let Err(e) = commands::run::execute(&entry_point) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Some(Command::Repl { file }) => commands::repl::execute(file.as_deref()),
        Some(Command::Check { path }) => {
            let entry_point = match commands::resolve::resolve_entry_point(path.as_deref()) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };
            if let Err(e) = commands::check::execute(&entry_point) {
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
