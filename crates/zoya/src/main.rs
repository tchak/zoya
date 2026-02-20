use std::path::PathBuf;

use clap::{Parser, Subcommand};
use console::{Term, style};
use zoya_loader::Mode;

mod commands;
mod diagnostic;

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
    #[command(trailing_var_arg = true)]
    Run {
        /// Function name to run (defaults to "main")
        name: Option<String>,
        /// Arguments to pass to the function
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
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
    /// Start a development server
    Dev {
        /// Path to a .zy file or directory with package.toml (defaults to current directory)
        #[arg(short, long)]
        package: Option<PathBuf>,
        /// Port to listen on
        #[arg(long, default_value_t = 3000)]
        port: u16,
    },
    /// Create a new Zoya project
    Init {
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
        /// Compilation mode (dev, test, release)
        #[arg(long, default_value = "dev")]
        mode: String,
    },
    /// Run a #[task] function
    #[command(trailing_var_arg = true)]
    Run {
        /// Task name (e.g., "deploy" or "utils::migrate")
        name: String,
        /// Arguments to pass to the task function
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
        /// Path to a .zy file or directory with package.toml
        #[arg(short, long)]
        package: Option<PathBuf>,
        /// Compilation mode (dev, test, release)
        #[arg(long, default_value = "dev")]
        mode: String,
        /// Output result as JSON
        #[arg(long)]
        json: bool,
    },
}

fn fatal(term: &Term, msg: &str) -> ! {
    let _ = term.write_line(&format!("{}: {}", style("error").red().bold(), msg));
    std::process::exit(1);
}

fn handle_error(term: &Term, e: impl Into<anyhow::Error>) -> ! {
    let e = e.into();
    if !diagnostic::try_render_diagnostic(&e) {
        fatal(term, &e.to_string());
    }
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
            name,
            args,
            package,
            mode,
            json,
        }) => {
            let path = package.unwrap_or_else(|| PathBuf::from("."));
            let mode = parse_mode(&term, &mode);
            if let Err(e) = commands::run::execute(&path, mode, name.as_deref(), &args, json) {
                handle_error(&term, e);
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
                handle_error(&term, e);
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
                handle_error(&term, e);
            }
        }
        Some(Command::Fmt { package, check }) => {
            let path = package.unwrap_or_else(|| PathBuf::from("."));
            if let Err(e) = commands::fmt::execute(&path, check) {
                handle_error(&term, e);
            }
        }
        Some(Command::Test { package }) => {
            let path = package.unwrap_or_else(|| PathBuf::from("."));
            if let Err(e) = commands::test::execute(&path) {
                handle_error(&term, e);
            }
        }
        Some(Command::Dev { package, port }) => {
            let path = package.unwrap_or_else(|| PathBuf::from("."));
            if let Err(e) = commands::dev::execute(&path, port) {
                handle_error(&term, e);
            }
        }
        Some(Command::Task { command }) => match command {
            TaskCommand::List { package, mode } => {
                let path = package.unwrap_or_else(|| PathBuf::from("."));
                let mode = parse_mode(&term, &mode);
                if let Err(e) = commands::task::list::execute(&path, mode) {
                    handle_error(&term, e);
                }
            }
            TaskCommand::Run {
                name,
                args,
                package,
                mode,
                json,
            } => {
                let path = package.unwrap_or_else(|| PathBuf::from("."));
                let mode = parse_mode(&term, &mode);
                if let Err(e) = commands::task::run::execute(&path, &name, &args, json, mode) {
                    handle_error(&term, e);
                }
            }
        },
        Some(Command::Init { path, name }) => {
            if let Err(e) = commands::init::execute(&path, name.as_deref()) {
                handle_error(&term, e);
            }
        }
        None => {
            println!("Zoya language - use --help for usage");
        }
    }
}
