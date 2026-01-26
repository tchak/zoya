use std::path::Path;

use crate::ast::{FunctionDef, Item};
use crate::check::{check_function, function_type_from_def, TypeEnv};
use crate::codegen::codegen_function;
use crate::eval::{self, EvalError};
use crate::lexer;
use crate::parser;

/// Run a Zoya source file
pub fn run(path: &Path) -> Result<(), EvalError> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| EvalError::RuntimeError(format!("failed to read file: {}", e)))?;

    // Lex and parse all items
    let tokens = lexer::lex(&source).map_err(|e| EvalError::RuntimeError(e.message))?;
    let items = parser::parse_items(tokens).map_err(|e| EvalError::RuntimeError(e.message))?;

    // Build type environment with all functions
    let mut type_env = TypeEnv::default();
    let mut functions: Vec<&FunctionDef> = Vec::new();

    for item in &items {
        let Item::Function(func) = item;
        let func_type =
            function_type_from_def(func).map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        type_env.functions.insert(func.name.clone(), func_type);
        functions.push(func);
    }

    // Type check all functions
    let mut typed_functions = Vec::new();
    for func in &functions {
        let typed =
            check_function(func, &type_env).map_err(|e| EvalError::RuntimeError(e.to_string()))?;
        typed_functions.push(typed);
    }

    // Find main function
    let main_func = typed_functions
        .iter()
        .find(|f| f.name == "main")
        .ok_or_else(|| EvalError::RuntimeError("no main() function found".to_string()))?;

    // Check main has no parameters
    if !main_func.params.is_empty() {
        return Err(EvalError::RuntimeError(
            "main() must not take any parameters".to_string(),
        ));
    }

    // Generate JS code
    let mut js_code = String::new();
    for typed_func in &typed_functions {
        js_code.push_str(&codegen_function(typed_func));
        js_code.push('\n');
    }
    js_code.push_str("main()");

    // Execute
    let (_runtime, context) =
        eval::create_context().map_err(|e| EvalError::RuntimeError(e.to_string()))?;

    context.with(|ctx| {
        let value = eval::eval_js_in_context(&ctx, js_code, main_func.return_type.clone())?;
        println!("{}", value);
        Ok(())
    })
}
