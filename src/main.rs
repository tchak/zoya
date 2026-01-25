use std::io::{self, BufRead, Write};

use clap::{Parser, Subcommand};

mod ast;
mod eval;
mod lexer;
mod parser;

#[derive(Parser)]
#[command(name = "zoya")]
#[command(version, about = "The Zoya programming language")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start the interactive REPL
    Run,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Run) => run_repl(),
        None => {
            println!("Zoya language - use --help for usage");
        }
    }
}

fn run_repl() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("> ");
        stdout.flush().unwrap();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error reading input: {}", e);
                break;
            }
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match eval_line(line) {
            Ok(result) => println!("{}", result),
            Err(e) => eprintln!("Error: {}", e),
        }
    }
}

fn eval_line(input: &str) -> Result<i64, String> {
    let tokens = lexer::lex(input).map_err(|e| e.message)?;
    let expr = parser::parse(tokens).map_err(|e| e.message)?;
    eval::eval(&expr).map_err(|e| e.to_string())
}
