use std::collections::HashMap;
use std::io::{self, BufRead, Write};

use crate::ast::FunctionDef;
use crate::check::{check_repl, CheckedStatement, TypeEnv};
use crate::codegen::{codegen, codegen_function};
use crate::eval::{self, Context};
use crate::lexer;
use crate::parser;

/// REPL state that accumulates function definitions
struct State {
    /// Function definitions (AST level, for re-type-checking on redefinition)
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

    /// Process REPL input: parse, type-check, and execute statements
    fn process_input(&mut self, input: &str) -> Result<(), String> {
        // Lex and parse
        let tokens = lexer::lex(input).map_err(|e| e.message)?;
        let statements = parser::parse_repl(tokens).map_err(|e| e.message)?;

        if statements.is_empty() {
            return Ok(());
        }

        // Type-check all statements
        let checked = check_repl(&statements, &mut self.type_env).map_err(|e| e.to_string())?;

        // Execute each checked statement
        for statement in checked {
            match statement {
                CheckedStatement::Function(typed_func) => {
                    let name = typed_func.name.clone();
                    let js_code = codegen_function(&typed_func);

                    // Define function in QuickJS context
                    self.context.with(|ctx| {
                        ctx.eval::<(), _>(js_code)
                            .map_err(|e| format!("JS error: {}", e))
                    })?;

                    // Store AST for potential re-checking later
                    // (extract from original statements)
                    for stmt in &statements {
                        if let crate::ast::Statement::Item(crate::ast::Item::Function(func)) = stmt {
                            if func.name == name {
                                self.functions.insert(name.clone(), func.clone());
                                break;
                            }
                        }
                    }

                    println!("defined: {}", name);
                }
                CheckedStatement::Expr(typed_expr) => {
                    let js_code = codegen(&typed_expr);
                    let result_type = typed_expr.ty();

                    let result = self.context.with(|ctx| {
                        eval::eval_js_in_context(&ctx, js_code, result_type)
                            .map_err(|e| e.to_string())
                    })?;

                    println!("{}", result);
                }
            }
        }

        Ok(())
    }
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

        if let Err(e) = state.process_input(line) {
            eprintln!("Error: {}", e);
        }
    }
}
