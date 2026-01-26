use std::collections::HashMap;
use std::io::{self, BufRead, Write};

use crate::ast::{FunctionDef, Item};
use crate::check::{check_function, check_with_env, function_type_from_def, TypeEnv};
use crate::codegen::{codegen, codegen_function};
use crate::eval::{self, Context};
use crate::lexer;
use crate::parser;

/// REPL state that accumulates function definitions
struct State {
    /// Function definitions (AST level, for re-type-checking)
    functions: HashMap<String, FunctionDef>,
    /// Type environment
    type_env: TypeEnv,
    /// QuickJS runtime (kept alive for context)
    #[allow(dead_code)]
    runtime: rquickjs::Runtime,
    /// Persistent QuickJS context with function definitions
    context: Context,
}

impl State {
    fn new() -> Result<Self, String> {
        let (runtime, context) = eval::create_context()?;

        Ok(State {
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

        self.context
            .with(|ctx| eval::eval_js_in_context(&ctx, js_code, result_type).map_err(|e| e.to_string()))
    }
}

/// Check if input starts with a keyword that indicates a declaration
fn is_declaration(input: &str) -> bool {
    let trimmed = input.trim_start();
    trimmed.starts_with("fn ") || trimmed.starts_with("fn\t") || trimmed == "fn"
}

/// Run the interactive REPL
pub fn run() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    let mut state = match State::new() {
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
