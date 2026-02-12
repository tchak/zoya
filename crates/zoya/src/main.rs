use std::path::PathBuf;

use clap::{Parser, Subcommand};
use zoya_loader::Mode;

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
        /// Path to a .zy file or directory with package.toml (defaults to current directory)
        path: Option<PathBuf>,
        /// Compilation mode (dev, test, release)
        #[arg(long, default_value = "dev")]
        mode: String,
    },
    /// Start the interactive REPL
    Repl {
        /// Path to a .zy file or directory with package.toml (defaults to current directory)
        path: Option<PathBuf>,
    },
    /// Type-check a file without executing
    Check {
        /// Path to a .zy file or directory with package.toml (defaults to current directory)
        path: Option<PathBuf>,
        /// Compilation mode (dev, test, release)
        #[arg(long, default_value = "dev")]
        mode: String,
    },
    /// Compile a file to JavaScript
    Build {
        /// Path to a .zy file or directory with package.toml (defaults to current directory)
        path: Option<PathBuf>,
        /// Output file (overrides package.toml output)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Compilation mode (dev, test, release)
        #[arg(long, default_value = "dev")]
        mode: String,
    },
    /// Format source files
    Fmt {
        /// Path to a .zy file or directory (defaults to current directory)
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

fn parse_mode(s: &str) -> Mode {
    s.parse().unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    })
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Run { path, mode }) => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            let mode = parse_mode(&mode);
            if let Err(e) = commands::run::execute(&path, mode) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Some(Command::Repl { path }) => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            commands::repl::execute(&path);
        }
        Some(Command::Check { path, mode }) => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            let mode = parse_mode(&mode);
            if let Err(e) = commands::check::execute(&path, mode) {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
        Some(Command::Build { path, output, mode }) => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            let mode = parse_mode(&mode);
            if let Err(e) = commands::build::execute(&path, output.as_deref(), mode) {
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
