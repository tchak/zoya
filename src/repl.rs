use std::collections::HashMap;
use std::path::PathBuf;

use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;

use crate::ast::{EnumDef, FunctionDef, StructDef};
use crate::check::{CheckedStatement, TypeEnv, check_repl};
use crate::codegen::{codegen, codegen_function, codegen_let, prelude};
use crate::eval::{self, Context, Value};
use crate::lexer;
use crate::parser;
use crate::types::Type;

/// Result of processing a single REPL statement
#[derive(Debug, Clone, PartialEq)]
pub enum ReplResult {
    /// Function was defined
    FunctionDefined(String),
    /// Struct was defined
    StructDefined(String),
    /// Enum was defined
    EnumDefined(String),
    /// Let binding was created
    LetBinding { name: String, ty: Type },
    /// Expression was evaluated
    Expression(Value),
}

/// REPL state that accumulates function and struct definitions
pub struct State {
    /// Function definitions (AST level, for re-type-checking on redefinition)
    functions: HashMap<String, FunctionDef>,
    /// Struct definitions (AST level)
    structs: HashMap<String, StructDef>,
    /// Enum definitions (AST level)
    enums: HashMap<String, EnumDef>,
    /// Type environment
    type_env: TypeEnv,
    /// QuickJS runtime (kept alive for context)
    #[allow(dead_code)]
    runtime: rquickjs::Runtime,
    /// Persistent QuickJS context with function definitions
    context: Context,
}

impl State {
    /// Create a new REPL state
    pub fn new() -> Result<Self, String> {
        let (runtime, context) = eval::create_context()?;

        // Initialize context with prelude (helper functions)
        context.with(|ctx| {
            ctx.eval::<(), _>(prelude())
                .map_err(|e| format!("Failed to initialize prelude: {}", e))
        })?;

        Ok(State {
            functions: HashMap::new(),
            structs: HashMap::new(),
            enums: HashMap::new(),
            type_env: TypeEnv::default(),
            runtime,
            context,
        })
    }

    /// Evaluate REPL input and return results
    ///
    /// This method processes the input through the full pipeline:
    /// lexing, parsing, type-checking, and execution.
    /// Returns a result for each statement in the input.
    pub fn eval(&mut self, input: &str) -> Result<Vec<ReplResult>, String> {
        // Lex and parse
        let tokens = lexer::lex(input).map_err(|e| e.message)?;
        let statements = parser::parse_repl(tokens).map_err(|e| e.message)?;

        if statements.is_empty() {
            return Ok(vec![]);
        }

        // Type-check all statements
        let checked = check_repl(&statements, &mut self.type_env).map_err(|e| e.to_string())?;

        // Execute each checked statement and collect results
        let mut results = Vec::new();

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
                    for stmt in &statements {
                        if let crate::ast::Statement::Item(crate::ast::Item::Function(func)) = stmt
                            && func.name == name
                        {
                            self.functions.insert(name.clone(), func.clone());
                            break;
                        }
                    }

                    results.push(ReplResult::FunctionDefined(name));
                }
                CheckedStatement::Expr(typed_expr) => {
                    let js_code = codegen(&typed_expr);
                    let result_type = typed_expr.ty();

                    let value = self.context.with(|ctx| {
                        eval::eval_js_in_context(&ctx, js_code, result_type)
                            .map_err(|e| e.to_string())
                    })?;

                    results.push(ReplResult::Expression(value));
                }
                CheckedStatement::Let(typed_binding) => {
                    let name = typed_binding.name.clone();
                    let ty = typed_binding.ty.clone();
                    let js_code = codegen_let(&typed_binding);

                    // Define variable in QuickJS context
                    self.context.with(|ctx| {
                        ctx.eval::<(), _>(js_code)
                            .map_err(|e| format!("JS error: {}", e))
                    })?;

                    results.push(ReplResult::LetBinding { name, ty });
                }
                CheckedStatement::Struct(struct_def) => {
                    let name = struct_def.name.clone();
                    // Structs are type declarations - no JS code needed
                    // The struct is already registered in the type_env by check_repl
                    self.structs.insert(name.clone(), struct_def);
                    results.push(ReplResult::StructDefined(name));
                }
                CheckedStatement::Enum(enum_def) => {
                    let name = enum_def.name.clone();
                    // Enums are type declarations - no JS code needed
                    // The enum is already registered in the type_env by check_repl
                    self.enums.insert(name.clone(), enum_def);
                    results.push(ReplResult::EnumDefined(name));
                }
            }
        }

        Ok(results)
    }
}

/// Get path to history file
fn history_path() -> PathBuf {
    dirs::home_dir()
        .map(|p| p.join(".zoya_history"))
        .unwrap_or_else(|| PathBuf::from(".zoya_history"))
}

/// Run the interactive REPL
pub fn run() {
    let mut rl = match DefaultEditor::new() {
        Ok(editor) => editor,
        Err(e) => {
            eprintln!("Failed to create editor: {}", e);
            return;
        }
    };

    // Load history (ignore errors if file doesn't exist)
    let history_file = history_path();
    let _ = rl.load_history(&history_file);

    let mut state = match State::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to initialize REPL: {}", e);
            return;
        }
    };

    // Track consecutive Ctrl-C presses for exit
    let mut ctrl_c_pressed = false;

    loop {
        match rl.readline("> ") {
            Ok(line) => {
                // Reset Ctrl-C state on normal input
                ctrl_c_pressed = false;

                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                // Add to history
                let _ = rl.add_history_entry(line);

                match state.eval(line) {
                    Ok(results) => {
                        for result in results {
                            match result {
                                ReplResult::FunctionDefined(name) => {
                                    println!("defined: {}", name);
                                }
                                ReplResult::StructDefined(name) => {
                                    println!("struct: {}", name);
                                }
                                ReplResult::EnumDefined(name) => {
                                    println!("enum: {}", name);
                                }
                                ReplResult::Expression(value) => {
                                    println!("{}", value);
                                }
                                ReplResult::LetBinding { name, ty } => {
                                    println!("let {}: {}", name, ty);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl-C: exit on double press
                if ctrl_c_pressed {
                    break;
                }
                ctrl_c_pressed = true;
                println!("Press Ctrl-C again to exit, or Ctrl-D");
                continue;
            }
            Err(ReadlineError::Eof) => {
                // Ctrl-D: exit
                break;
            }
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break;
            }
        }
    }

    // Save history on exit
    let _ = rl.save_history(&history_file);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repl_simple_expression() {
        let mut state = State::new().unwrap();
        let results = state.eval("42").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int32(42))]);
    }

    #[test]
    fn test_repl_float_expression() {
        let mut state = State::new().unwrap();
        let results = state.eval("3.14").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Float(3.14))]);
    }

    #[test]
    fn test_repl_string_expression() {
        let mut state = State::new().unwrap();
        let results = state.eval(r#""hello""#).unwrap();
        assert_eq!(
            results,
            vec![ReplResult::Expression(Value::String("hello".to_string()))]
        );
    }

    #[test]
    fn test_repl_bool_expression() {
        let mut state = State::new().unwrap();
        let results = state.eval("true").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Bool(true))]);
    }

    #[test]
    fn test_repl_arithmetic() {
        let mut state = State::new().unwrap();
        let results = state.eval("1 + 2 * 3").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int32(7))]);
    }

    #[test]
    fn test_repl_function_definition() {
        let mut state = State::new().unwrap();
        let results = state
            .eval("fn add(x: Int32, y: Int32) -> Int32 { x + y }")
            .unwrap();
        assert_eq!(
            results,
            vec![ReplResult::FunctionDefined("add".to_string())]
        );
    }

    #[test]
    fn test_repl_let_binding() {
        let mut state = State::new().unwrap();
        let results = state.eval("let x = 42").unwrap();
        assert_eq!(results.len(), 1);
        assert!(matches!(
            &results[0],
            ReplResult::LetBinding { name, ty } if name == "x" && *ty == Type::Int32
        ));
    }

    #[test]
    fn test_repl_state_persistence_let() {
        let mut state = State::new().unwrap();
        state.eval("let x = 10").unwrap();
        state.eval("let y = 20").unwrap();
        let results = state.eval("x + y").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int32(30))]);
    }

    #[test]
    fn test_repl_function_call() {
        let mut state = State::new().unwrap();
        state
            .eval("fn double(n: Int32) -> Int32 { n * 2 }")
            .unwrap();
        let results = state.eval("double(21)").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int32(42))]);
    }

    #[test]
    fn test_repl_forward_reference() {
        let mut state = State::new().unwrap();
        let results = state
            .eval("fn caller() -> Int32 { callee() } fn callee() -> Int32 { 42 }")
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(matches!(&results[0], ReplResult::FunctionDefined(name) if name == "caller"));
        assert!(matches!(&results[1], ReplResult::FunctionDefined(name) if name == "callee"));

        // Call caller to verify it works
        let results = state.eval("caller()").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int32(42))]);
    }

    #[test]
    fn test_repl_mutual_recursion() {
        let mut state = State::new().unwrap();
        state
            .eval(
                r#"
            fn is_even(n: Int32) -> Bool { match n { 0 => true, _ => is_odd(n - 1) } }
            fn is_odd(n: Int32) -> Bool { match n { 0 => false, _ => is_even(n - 1) } }
        "#,
            )
            .unwrap();
        let results = state.eval("is_even(4)").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Bool(true))]);

        let results = state.eval("is_odd(3)").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Bool(true))]);
    }

    #[test]
    fn test_repl_syntax_error() {
        let mut state = State::new().unwrap();
        let result = state.eval("fn bad(");
        assert!(result.is_err());
    }

    #[test]
    fn test_repl_type_error() {
        let mut state = State::new().unwrap();
        let result = state.eval("1 + true");
        assert!(result.is_err());
    }

    #[test]
    fn test_repl_undefined_variable() {
        let mut state = State::new().unwrap();
        let result = state.eval("undefined_var");
        assert!(result.is_err());
    }

    #[test]
    fn test_repl_multiple_statements() {
        let mut state = State::new().unwrap();
        let results = state.eval("let a = 1\nlet b = 2\na + b").unwrap();
        assert_eq!(results.len(), 3);
        assert!(matches!(&results[0], ReplResult::LetBinding { name, .. } if name == "a"));
        assert!(matches!(&results[1], ReplResult::LetBinding { name, .. } if name == "b"));
        assert_eq!(results[2], ReplResult::Expression(Value::Int32(3)));
    }

    #[test]
    fn test_repl_empty_input() {
        let mut state = State::new().unwrap();
        let results = state.eval("").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_repl_whitespace_only() {
        let mut state = State::new().unwrap();
        let results = state.eval("   \n\t  ").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_repl_function_redefine() {
        let mut state = State::new().unwrap();
        state.eval("fn f() -> Int32 { 1 }").unwrap();
        let results = state.eval("f()").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int32(1))]);

        // Redefine function
        state.eval("fn f() -> Int32 { 2 }").unwrap();
        let results = state.eval("f()").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int32(2))]);
    }

    #[test]
    fn test_repl_method_call() {
        let mut state = State::new().unwrap();
        let results = state.eval(r#""hello".len()"#).unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int32(5))]);
    }

    #[test]
    fn test_repl_list() {
        let mut state = State::new().unwrap();
        let results = state.eval("[1, 2, 3]").unwrap();
        assert_eq!(
            results,
            vec![ReplResult::Expression(Value::List(vec![
                Value::Int32(1),
                Value::Int32(2),
                Value::Int32(3)
            ]))]
        );
    }

    #[test]
    fn test_repl_tuple() {
        let mut state = State::new().unwrap();
        let results = state.eval("(1, true)").unwrap();
        assert_eq!(
            results,
            vec![ReplResult::Expression(Value::Tuple(vec![
                Value::Int32(1),
                Value::Bool(true)
            ]))]
        );
    }

    #[test]
    fn test_repl_match_expression() {
        let mut state = State::new().unwrap();
        let results = state.eval("match 1 { 0 => false, _ => true }").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Bool(true))]);
    }

    #[test]
    fn test_repl_struct_definition() {
        let mut state = State::new().unwrap();
        let results = state.eval("struct Point { x: Int32, y: Int32 }").unwrap();
        assert_eq!(
            results,
            vec![ReplResult::StructDefined("Point".to_string())]
        );
    }

    #[test]
    fn test_repl_struct_construction() {
        let mut state = State::new().unwrap();
        state.eval("struct Point { x: Int32, y: Int32 }").unwrap();
        let results = state.eval("Point { x: 10, y: 20 }").unwrap();
        assert_eq!(
            results,
            vec![ReplResult::Expression(Value::Struct {
                name: "Point".to_string(),
                fields: vec![
                    ("x".to_string(), Value::Int32(10)),
                    ("y".to_string(), Value::Int32(20)),
                ],
            })]
        );
    }

    #[test]
    fn test_repl_struct_field_access() {
        let mut state = State::new().unwrap();
        state.eval("struct Point { x: Int32, y: Int32 }").unwrap();
        state.eval("let p = Point { x: 10, y: 20 }").unwrap();
        let results = state.eval("p.x").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int32(10))]);
    }

    #[test]
    fn test_repl_struct_pattern_match() {
        let mut state = State::new().unwrap();
        state.eval("struct Point { x: Int32, y: Int32 }").unwrap();
        state.eval("let p = Point { x: 10, y: 20 }").unwrap();
        let results = state.eval("match p { Point { x, y } => x + y }").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int32(30))]);
    }
}
