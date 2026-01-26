use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use rquickjs::{Context, Runtime};

mod ast;
mod check;
mod codegen;
mod eval;
mod ir;
mod lexer;
mod parser;
mod types;

use ast::{FunctionDef, Item};
use check::{check_function, check_with_env, function_type_from_def, TypeEnv};
use codegen::{codegen, codegen_function};
use types::Type;

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
            if let Err(e) = run_file(&path) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Some(Command::Run { file: None }) => run_repl(),
        None => {
            println!("Zoya language - use --help for usage");
        }
    }
}

/// REPL state that accumulates function definitions
struct ReplState {
    /// Function definitions (AST level, for re-type-checking)
    functions: HashMap<String, FunctionDef>,
    /// Type environment
    type_env: TypeEnv,
    /// QuickJS runtime (kept alive for context)
    runtime: Runtime,
    /// Persistent QuickJS context with function definitions
    context: Context,
}

impl ReplState {
    fn new() -> Result<Self, String> {
        let runtime = Runtime::new().map_err(|e| e.to_string())?;
        let context = Context::full(&runtime).map_err(|e| e.to_string())?;

        Ok(ReplState {
            functions: HashMap::new(),
            type_env: TypeEnv::default(),
            runtime,
            context,
        })
    }

    /// Add or update a function definition
    fn add_function(&mut self, func: FunctionDef) -> Result<(), String> {
        // Get function type from definition
        let func_type = function_type_from_def(&func).map_err(|e| e.to_string())?;

        // Save old state for rollback
        let old_func = self.functions.get(&func.name).cloned();
        let old_type = self.type_env.functions.get(&func.name).cloned();

        // Add to environment (allow redefinition)
        self.functions.insert(func.name.clone(), func.clone());
        self.type_env.functions.insert(func.name.clone(), func_type);

        // Re-type-check all functions to catch breakage from redefinition
        if let Err(e) = self.recheck_all_functions() {
            // Rollback on error
            if let Some(old_f) = old_func {
                self.functions.insert(func.name.clone(), old_f);
            } else {
                self.functions.remove(&func.name);
            }
            if let Some(old_t) = old_type {
                self.type_env.functions.insert(func.name.clone(), old_t);
            } else {
                self.type_env.functions.remove(&func.name);
            }
            return Err(e);
        }

        // Type check and generate JS for the function
        let typed_func = check_function(&func, &self.type_env).map_err(|e| e.to_string())?;
        let js_code = codegen_function(&typed_func);

        // Define function in QuickJS context
        self.context.with(|ctx| {
            ctx.eval::<(), _>(js_code)
                .map_err(|e| format!("JS error: {}", e))
        })?;

        Ok(())
    }

    /// Re-type-check all functions (catches breakage from redefinition)
    fn recheck_all_functions(&self) -> Result<(), String> {
        for (name, func) in &self.functions {
            check_function(func, &self.type_env)
                .map_err(|e| format!("error in function '{}': {}", name, e))?;
        }
        Ok(())
    }

    /// Evaluate an expression in the context of defined functions
    fn eval_expr(&self, input: &str) -> Result<eval::Value, String> {
        let tokens = lexer::lex(input).map_err(|e| e.message)?;
        let expr = parser::parse(tokens).map_err(|e| e.message)?;
        let typed_expr = check_with_env(&expr, &self.type_env).map_err(|e| e.to_string())?;

        let js_code = codegen(&typed_expr);
        let result_type = typed_expr.ty();

        self.context.with(|ctx| {
            let result: f64 = ctx
                .eval(js_code)
                .map_err(|e| format!("runtime error: {}", e))?;

            // Check for division by zero
            if result.is_infinite() || result.is_nan() {
                return Err("division by zero".to_string());
            }

            match result_type {
                Type::Int => Ok(eval::Value::Int(result as i64)),
                Type::Float => Ok(eval::Value::Float(result)),
                Type::Var(name) => Err(format!("unresolved type variable: {}", name)),
            }
        })
    }
}

/// Check if input starts with a keyword that indicates a declaration
fn is_declaration(input: &str) -> bool {
    let trimmed = input.trim_start();
    trimmed.starts_with("fn ") || trimmed.starts_with("fn\t") || trimmed == "fn"
}

fn run_repl() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    let mut state = match ReplState::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to initialize REPL: {}", e);
            return;
        }
    };

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

        if is_declaration(line) {
            // Parse and add function definition
            let tokens = match lexer::lex(line) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Error: {}", e.message);
                    continue;
                }
            };

            let item = match parser::parse_item(tokens) {
                Ok(i) => i,
                Err(e) => {
                    eprintln!("Error: {}", e.message);
                    continue;
                }
            };

            let Item::Function(func) = item;

            match state.add_function(func.clone()) {
                Ok(()) => println!("defined: {}", func.name),
                Err(e) => eprintln!("Error: {}", e),
            }
        } else {
            // Evaluate expression
            match state.eval_expr(line) {
                Ok(result) => println!("{}", result),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
    }
}

/// Run a Zoya source file
fn run_file(path: &PathBuf) -> Result<(), String> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read file: {}", e))?;

    // Lex and parse all items
    let tokens = lexer::lex(&source).map_err(|e| e.message)?;
    let items = parser::parse_items(tokens).map_err(|e| e.message)?;

    // Build type environment with all functions
    let mut type_env = TypeEnv::default();
    let mut functions: Vec<&FunctionDef> = Vec::new();

    for item in &items {
        let Item::Function(func) = item;
        let func_type = function_type_from_def(func).map_err(|e| e.to_string())?;
        type_env.functions.insert(func.name.clone(), func_type);
        functions.push(func);
    }

    // Type check all functions
    let mut typed_functions = Vec::new();
    for func in &functions {
        let typed = check_function(func, &type_env).map_err(|e| e.to_string())?;
        typed_functions.push(typed);
    }

    // Find main function
    let main_func = typed_functions
        .iter()
        .find(|f| f.name == "main")
        .ok_or_else(|| "no main() function found".to_string())?;

    // Check main has no parameters
    if !main_func.params.is_empty() {
        return Err("main() must not take any parameters".to_string());
    }

    // Generate JS code
    let mut js_code = String::new();
    for typed_func in &typed_functions {
        js_code.push_str(&codegen_function(typed_func));
        js_code.push('\n');
    }
    js_code.push_str("main()");

    // Execute
    let runtime = Runtime::new().map_err(|e| e.to_string())?;
    let context = Context::full(&runtime).map_err(|e| e.to_string())?;

    context.with(|ctx| {
        let result: f64 = ctx
            .eval(js_code.clone())
            .map_err(|e| format!("runtime error: {}", e))?;

        // Check for division by zero
        if result.is_infinite() || result.is_nan() {
            return Err("division by zero".to_string());
        }

        let value = match &main_func.return_type {
            Type::Int => eval::Value::Int(result as i64),
            Type::Float => eval::Value::Float(result),
            Type::Var(name) => return Err(format!("unresolved type variable: {}", name)),
        };

        println!("{}", value);
        Ok(())
    })
}
