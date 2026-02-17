use std::path::PathBuf;

use clap::{Parser, Subcommand};
use console::{Term, style};
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
        #[arg(short, long)]
        package: Option<PathBuf>,
        /// Compilation mode (dev, test, release)
        #[arg(long, default_value = "dev")]
        mode: String,
        /// Output result as JSON
        #[arg(long)]
        json: bool,
    },
    /// Start the interactive REPL
    Repl {
        /// Path to a .zy file or directory with package.toml (defaults to current directory)
        #[arg(short, long)]
        package: Option<PathBuf>,
    },
    /// Type-check a file without executing
    Check {
        /// Path to a .zy file or directory with package.toml (defaults to current directory)
        #[arg(short, long)]
        package: Option<PathBuf>,
        /// Compilation mode (dev, test, release)
        #[arg(long, default_value = "dev")]
        mode: String,
    },
    /// Compile a file to JavaScript
    Build {
        /// Path to a .zy file or directory with package.toml (defaults to current directory)
        #[arg(short, long)]
        package: Option<PathBuf>,
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
        #[arg(short, long)]
        package: Option<PathBuf>,
        /// Check if files are formatted without writing (exit 1 if not)
        #[arg(long)]
        check: bool,
    },
    /// Run tests
    Test {
        /// Path to a .zy file or directory with package.toml (defaults to current directory)
        #[arg(short, long)]
        package: Option<PathBuf>,
    },
    /// Manage task functions
    Task {
        #[command(subcommand)]
        command: TaskCommand,
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

#[derive(Subcommand)]
enum TaskCommand {
    /// List all #[task] functions in a package
    List {
        /// Path to a .zy file or directory with package.toml (defaults to current directory)
        #[arg(short, long)]
        package: Option<PathBuf>,
    },
}

fn fatal(term: &Term, msg: &str) -> ! {
    let _ = term.write_line(&format!("{}: {}", style("error").red().bold(), msg));
    std::process::exit(1);
}

fn parse_mode(term: &Term, s: &str) -> Mode {
    s.parse().unwrap_or_else(|e: String| {
        fatal(term, &e);
    })
}

fn main() {
    let cli = Cli::parse();
    let term = Term::stderr();

    match cli.command {
        Some(Command::Run {
            package,
            mode,
            json,
        }) => {
            let path = package.unwrap_or_else(|| PathBuf::from("."));
            let mode = parse_mode(&term, &mode);
            if let Err(e) = commands::run::execute(&path, mode, json) {
                fatal(&term, &e.to_string());
            }
        }
        Some(Command::Repl { package }) => {
            let path = package.unwrap_or_else(|| PathBuf::from("."));
            commands::repl::execute(&path);
        }
        Some(Command::Check { package, mode }) => {
            let path = package.unwrap_or_else(|| PathBuf::from("."));
            let mode = parse_mode(&term, &mode);
            if let Err(e) = commands::check::execute(&path, mode) {
                fatal(&term, &e.to_string());
            }
        }
        Some(Command::Build {
            package,
            output,
            mode,
        }) => {
            let path = package.unwrap_or_else(|| PathBuf::from("."));
            let mode = parse_mode(&term, &mode);
            if let Err(e) = commands::build::execute(&path, output.as_deref(), mode) {
                fatal(&term, &e.to_string());
            }
        }
        Some(Command::Fmt { package, check }) => {
            let path = package.unwrap_or_else(|| PathBuf::from("."));
            if let Err(e) = commands::fmt::execute(&path, check) {
                fatal(&term, &e.to_string());
            }
        }
        Some(Command::Test { package }) => {
            let path = package.unwrap_or_else(|| PathBuf::from("."));
            if let Err(e) = commands::test::execute(&path) {
                fatal(&term, &e.to_string());
            }
        }
        Some(Command::Task { command }) => match command {
            TaskCommand::List { package } => {
                let path = package.unwrap_or_else(|| PathBuf::from("."));
                if let Err(e) = commands::task::list::execute(&path) {
                    fatal(&term, &e.to_string());
                }
            }
        },
        Some(Command::New { path, name }) => {
            if let Err(e) = commands::new::execute(&path, name.as_deref()) {
                fatal(&term, &e.to_string());
            }
        }
        None => {
            println!("Zoya language - use --help for usage");
        }
    }
}
