use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;

use crate::eval::{self, Context, Value, VirtualModules};
use zoya_ast::{Expr, FunctionDef, Item, LetBinding, Stmt, Visibility};
use zoya_check::check;
use zoya_codegen::codegen;
use zoya_ir::{CheckedItem, CheckedModuleTree, Type, TypedExpr, TypedPattern};
use zoya_module::{Module, ModulePath, ModuleTree};

/// Extract all variable bindings from a typed pattern.
/// Returns a list of (name, type) pairs for each bound variable.
fn extract_bindings(pattern: &TypedPattern) -> Vec<(String, Type)> {
    let mut bindings = Vec::new();
    extract_bindings_impl(pattern, &mut bindings);
    bindings
}

fn extract_bindings_impl(pattern: &TypedPattern, bindings: &mut Vec<(String, Type)>) {
    match pattern {
        TypedPattern::Var { name, ty } => {
            bindings.push((name.clone(), ty.clone()));
        }
        TypedPattern::As { name, ty, pattern } => {
            bindings.push((name.clone(), ty.clone()));
            extract_bindings_impl(pattern, bindings);
        }
        TypedPattern::Wildcard | TypedPattern::Literal(_) => {}
        TypedPattern::ListEmpty | TypedPattern::TupleEmpty | TypedPattern::EnumUnit { .. } => {}
        TypedPattern::ListExact { patterns, .. }
        | TypedPattern::TupleExact { patterns, .. }
        | TypedPattern::EnumTupleExact { patterns, .. } => {
            for p in patterns {
                extract_bindings_impl(p, bindings);
            }
        }
        TypedPattern::ListPrefix {
            patterns,
            rest_binding,
            ..
        }
        | TypedPattern::ListSuffix {
            patterns,
            rest_binding,
            ..
        }
        | TypedPattern::TuplePrefix {
            patterns,
            rest_binding,
            ..
        }
        | TypedPattern::TupleSuffix {
            patterns,
            rest_binding,
            ..
        }
        | TypedPattern::EnumTuplePrefix {
            patterns,
            rest_binding,
            ..
        }
        | TypedPattern::EnumTupleSuffix {
            patterns,
            rest_binding,
            ..
        } => {
            for p in patterns {
                extract_bindings_impl(p, bindings);
            }
            if let Some((name, ty)) = rest_binding {
                bindings.push((name.clone(), ty.clone()));
            }
        }
        TypedPattern::ListPrefixSuffix {
            prefix,
            suffix,
            rest_binding,
            ..
        }
        | TypedPattern::TuplePrefixSuffix {
            prefix,
            suffix,
            rest_binding,
            ..
        }
        | TypedPattern::EnumTuplePrefixSuffix {
            prefix,
            suffix,
            rest_binding,
            ..
        } => {
            for p in prefix {
                extract_bindings_impl(p, bindings);
            }
            for p in suffix {
                extract_bindings_impl(p, bindings);
            }
            if let Some((name, ty)) = rest_binding {
                bindings.push((name.clone(), ty.clone()));
            }
        }
        TypedPattern::StructExact { fields, .. }
        | TypedPattern::StructPartial { fields, .. }
        | TypedPattern::EnumStructExact { fields, .. }
        | TypedPattern::EnumStructPartial { fields, .. } => {
            for (_, p) in fields {
                extract_bindings_impl(p, bindings);
            }
        }
    }
}

/// Result of processing a single REPL statement
#[derive(Debug, Clone, PartialEq)]
pub enum ReplResult {
    /// Function was defined
    FunctionDefined(String),
    /// Struct was defined
    StructDefined(String),
    /// Enum was defined
    EnumDefined(String),
    /// Type alias was defined
    TypeAliasDefined(String),
    /// Let binding was created (may bind multiple names from pattern)
    LetBinding { bindings: Vec<(String, Type)> },
    /// Expression was evaluated
    Expression(Value),
}

/// A block to be evaluated: accumulated lets + an optional expression
struct EvalBlock {
    /// All let bindings (accumulated + new) for this block
    bindings: Vec<LetBinding>,
    /// The expression to evaluate (None if this is just for accumulating lets)
    expr: Option<Expr>,
}

/// REPL state that accumulates definitions across evaluations
pub struct State {
    /// Accumulated items (functions, structs, enums, type aliases), keyed by name
    accumulated_items: HashMap<String, Item>,
    /// Accumulated let bindings from REPL input
    accumulated_lets: Vec<LetBinding>,
    /// Counter for synthetic run function names
    run_counter: usize,
    /// QuickJS runtime (kept alive for context)
    #[allow(dead_code)]
    runtime: rquickjs::Runtime,
    /// Persistent QuickJS context with module loader
    context: Context,
    /// Virtual modules storage (shared with runtime loader)
    virtual_modules: VirtualModules,
    /// Base module tree loaded from file (if provided)
    base_tree: Option<ModuleTree>,
}

impl State {
    /// Create a new REPL state
    pub fn new(file_path: Option<&Path>) -> Result<Self, String> {
        let virtual_modules = VirtualModules::new();
        let (runtime, context) = eval::create_module_runtime(virtual_modules.clone())?;

        let base_tree = if let Some(path) = file_path {
            Some(
                zoya_loader::load_modules(path)
                    .map_err(|e| format!("Failed to load modules: {}", e))?,
            )
        } else {
            None
        };

        Ok(State {
            accumulated_items: HashMap::new(),
            accumulated_lets: Vec::new(),
            run_counter: 0,
            runtime,
            context,
            virtual_modules,
            base_tree,
        })
    }

    /// Evaluate REPL input and return results
    ///
    /// This method processes the input through the full pipeline:
    /// lexing, parsing, type-checking, and execution.
    /// Returns a result for each statement in the input.
    pub fn eval(&mut self, input: &str) -> Result<Vec<ReplResult>, String> {
        // Lex and parse
        let tokens = zoya_lexer::lex(input).map_err(|e| e.message)?;
        let (items, stmts) = zoya_parser::parse_input(tokens).map_err(|e| e.message)?;

        if items.is_empty() && stmts.is_empty() {
            return Ok(vec![]);
        }

        // Partition statements into blocks for evaluation
        let (blocks, new_lets) = partition_into_blocks(&self.accumulated_lets, &stmts);

        // Create synthetic run functions for each block
        // Use run_{n} prefix
        let mut run_function_names = Vec::new();
        let mut run_functions = Vec::new();
        for block in &blocks {
            let name = format!("run_{}", self.run_counter);
            self.run_counter += 1;
            run_functions.push(create_run_function(&name, block));
            run_function_names.push(name);
        }

        // Build module tree with accumulated items + new items + run functions
        // New items will replace accumulated items with the same name when we update state later
        let mut all_items: Vec<Item> = self.accumulated_items.values().cloned().collect();
        all_items.extend(items.clone());
        all_items.extend(run_functions);

        let tree = build_repl_tree(self.base_tree.as_ref(), all_items);

        // Type check the module tree
        let checked_tree = check(&tree).map_err(|e| e.to_string())?;

        // Generate JavaScript code (ESM with exports)
        let output = codegen(&checked_tree);

        // Generate unique module name to avoid QuickJS module caching
        let module_name = format!("root_{}", output.hash);

        // Register the module with virtual modules
        self.virtual_modules.register(&module_name, output.code);

        // Collect results
        let mut results = Vec::new();

        // First, report item definitions
        for item in &items {
            match item {
                Item::Function(f) => {
                    results.push(ReplResult::FunctionDefined(f.name.clone()));
                }
                Item::Struct(s) => {
                    results.push(ReplResult::StructDefined(s.name.clone()));
                }
                Item::Enum(e) => {
                    results.push(ReplResult::EnumDefined(e.name.clone()));
                }
                Item::TypeAlias(t) => {
                    results.push(ReplResult::TypeAliasDefined(t.name.clone()));
                }
            }
        }

        // Process each run function to get results
        for run_name in &run_function_names {
            // Find typed function in checked tree to get return type
            let typed_fn = find_typed_function(&checked_tree, run_name)
                .ok_or_else(|| format!("Internal error: run function {} not found", run_name))?;

            // Call the function via module import
            let entry_func = format!("$root$repl${}", run_name);
            let value = self.context.with(|ctx| {
                eval::eval_module(
                    &ctx,
                    &module_name,
                    &entry_func,
                    typed_fn.return_type.clone(),
                )
                .map_err(|e| e.to_string())
            })?;

            // If unit type, this was a let-only block
            if typed_fn.return_type == Type::Tuple(vec![]) {
                // Extract the LAST binding from the function body (the one just added)
                let all_bindings = extract_bindings_from_typed_expr(&typed_fn.body);
                if let Some(last_binding) = all_bindings.last() {
                    results.push(ReplResult::LetBinding {
                        bindings: last_binding.clone(),
                    });
                }
            } else {
                // We have an expression result
                results.push(ReplResult::Expression(value));
            }
        }

        // Update accumulated state (HashMap handles replacement automatically)
        for item in items {
            self.accumulated_items.insert(item.name().to_string(), item);
        }
        self.accumulated_lets = new_lets;

        // If no blocks were created, we need to report let bindings from stmts directly
        if blocks.is_empty() && !stmts.is_empty() {
            // This shouldn't happen since partition_into_blocks always creates blocks for stmts
        }

        Ok(results)
    }
}

/// Partition statements into evaluation blocks.
/// Each new let binding gets its own let-only block for reporting purposes.
/// Each expression gets a block with all accumulated lets up to that point.
/// Returns (blocks, updated_accumulated_lets).
fn partition_into_blocks(
    accumulated_lets: &[LetBinding],
    stmts: &[Stmt],
) -> (Vec<EvalBlock>, Vec<LetBinding>) {
    let mut blocks = Vec::new();
    let mut current_lets: Vec<LetBinding> = accumulated_lets.to_vec();

    for stmt in stmts {
        match stmt {
            Stmt::Let(binding) => {
                // Add to current lets
                current_lets.push(binding.clone());
                // Create a let-only block to report this binding
                blocks.push(EvalBlock {
                    bindings: current_lets.clone(),
                    expr: None,
                });
            }
            Stmt::Expr(expr) => {
                // Create a block with all current lets + this expression
                blocks.push(EvalBlock {
                    bindings: current_lets.clone(),
                    expr: Some(expr.clone()),
                });
            }
        }
    }

    (blocks, current_lets)
}

/// Create a synthetic function for evaluating a block.
fn create_run_function(name: &str, block: &EvalBlock) -> Item {
    let body = if let Some(ref expr) = block.expr {
        // Block with expression: { let x = ...; let y = ...; expr }
        Expr::Block {
            bindings: block.bindings.clone(),
            result: Box::new(expr.clone()),
        }
    } else {
        // Let-only block: { let x = ...; () }
        Expr::Block {
            bindings: block.bindings.clone(),
            result: Box::new(Expr::Tuple(vec![])),
        }
    };

    Item::Function(FunctionDef {
        visibility: Visibility::Public,
        name: name.to_string(),
        type_params: vec![],
        params: vec![],
        return_type: None, // inferred
        body,
    })
}

/// Build a ModuleTree with REPL items in a "repl" submodule.
fn build_repl_tree(base_tree: Option<&ModuleTree>, items: Vec<Item>) -> ModuleTree {
    let repl_path = ModulePath::root().child("repl");

    let mut modules = if let Some(tree) = base_tree {
        tree.modules.clone()
    } else {
        HashMap::new()
    };

    // Create or update root module to include "repl" as child
    let root = modules.entry(ModulePath::root()).or_insert_with(|| Module {
        items: vec![],
        uses: vec![],
        path: ModulePath::root(),
        children: HashMap::new(),
    });
    root.children.insert("repl".to_string(), repl_path.clone());

    // Create the repl submodule with REPL items
    modules.insert(
        repl_path.clone(),
        Module {
            items,
            uses: vec![],
            path: repl_path,
            children: HashMap::new(),
        },
    );

    ModuleTree { modules }
}

/// Find a typed function by name in the repl submodule of the checked module tree.
fn find_typed_function<'a>(
    tree: &'a CheckedModuleTree,
    name: &str,
) -> Option<&'a zoya_ir::TypedFunction> {
    let repl_path = ModulePath::root().child("repl");
    let repl_module = tree.modules.get(&repl_path)?;
    for item in &repl_module.items {
        if let CheckedItem::Function(f) = item
            && f.name == name
        {
            return Some(f);
        }
    }
    None
}

/// Extract binding information from a typed expression (for let-only blocks).
/// Returns a list of binding groups, where each group is from a single let statement.
fn extract_bindings_from_typed_expr(expr: &TypedExpr) -> Vec<Vec<(String, Type)>> {
    let mut result = Vec::new();
    if let TypedExpr::Block { bindings, .. } = expr {
        for binding in bindings {
            let binding_info = extract_bindings(&binding.pattern);
            if !binding_info.is_empty() {
                result.push(binding_info);
            }
        }
    }
    result
}

/// Get path to history file
fn history_path() -> PathBuf {
    dirs::home_dir()
        .map(|p| p.join(".zoya_history"))
        .unwrap_or_else(|| PathBuf::from(".zoya_history"))
}

/// Run the interactive REPL
pub fn execute(file_path: Option<&Path>) {
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

    let mut state = match State::new(file_path) {
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
                                ReplResult::TypeAliasDefined(name) => {
                                    println!("type: {}", name);
                                }
                                ReplResult::Expression(value) => {
                                    println!("{}", value);
                                }
                                ReplResult::LetBinding { bindings } => {
                                    for (name, ty) in bindings {
                                        println!("let {}: {}", name, ty);
                                    }
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
        let mut state = State::new(None).unwrap();
        let results = state.eval("42").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int(42))]);
    }

    #[test]
    fn test_repl_float_expression() {
        let mut state = State::new(None).unwrap();
        let results = state.eval("3.14").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Float(3.14))]);
    }

    #[test]
    fn test_repl_string_expression() {
        let mut state = State::new(None).unwrap();
        let results = state.eval(r#""hello""#).unwrap();
        assert_eq!(
            results,
            vec![ReplResult::Expression(Value::String("hello".to_string()))]
        );
    }

    #[test]
    fn test_repl_bool_expression() {
        let mut state = State::new(None).unwrap();
        let results = state.eval("true").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Bool(true))]);
    }

    #[test]
    fn test_repl_arithmetic() {
        let mut state = State::new(None).unwrap();
        let results = state.eval("1 + 2 * 3").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int(7))]);
    }

    #[test]
    fn test_repl_function_definition() {
        let mut state = State::new(None).unwrap();
        let results = state
            .eval("fn add(x: Int, y: Int) -> Int { x + y }")
            .unwrap();
        assert_eq!(
            results,
            vec![ReplResult::FunctionDefined("add".to_string())]
        );
    }

    #[test]
    fn test_repl_let_binding() {
        let mut state = State::new(None).unwrap();
        let results = state.eval("let x = 42").unwrap();
        assert_eq!(results.len(), 1);
        assert!(matches!(
            &results[0],
            ReplResult::LetBinding { bindings } if bindings.len() == 1 && bindings[0].0 == "x" && bindings[0].1 == Type::Int
        ));
    }

    #[test]
    fn test_repl_state_persistence_let() {
        let mut state = State::new(None).unwrap();
        state.eval("let x = 10").unwrap();
        state.eval("let y = 20").unwrap();
        let results = state.eval("x + y").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int(30))]);
    }

    #[test]
    fn test_repl_function_call() {
        let mut state = State::new(None).unwrap();
        state.eval("fn double(n: Int) -> Int { n * 2 }").unwrap();
        let results = state.eval("double(21)").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int(42))]);
    }

    #[test]
    fn test_repl_forward_reference() {
        let mut state = State::new(None).unwrap();
        let results = state
            .eval("fn caller() -> Int { callee() } fn callee() -> Int { 42 }")
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(matches!(&results[0], ReplResult::FunctionDefined(name) if name == "caller"));
        assert!(matches!(&results[1], ReplResult::FunctionDefined(name) if name == "callee"));

        // Call caller to verify it works
        let results = state.eval("caller()").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int(42))]);
    }

    #[test]
    fn test_repl_mutual_recursion() {
        let mut state = State::new(None).unwrap();
        state
            .eval(
                r#"
            fn is_even(n: Int) -> Bool { match n { 0 => true, _ => is_odd(n - 1) } }
            fn is_odd(n: Int) -> Bool { match n { 0 => false, _ => is_even(n - 1) } }
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
        let mut state = State::new(None).unwrap();
        let result = state.eval("fn bad(");
        assert!(result.is_err());
    }

    #[test]
    fn test_repl_type_error() {
        let mut state = State::new(None).unwrap();
        let result = state.eval("1 + true");
        assert!(result.is_err());
    }

    #[test]
    fn test_repl_undefined_variable() {
        let mut state = State::new(None).unwrap();
        let result = state.eval("undefined_var");
        assert!(result.is_err());
    }

    #[test]
    fn test_repl_multiple_statements() {
        let mut state = State::new(None).unwrap();
        let results = state.eval("let a = 1\nlet b = 2\na + b").unwrap();
        assert_eq!(results.len(), 3);
        assert!(
            matches!(&results[0], ReplResult::LetBinding { bindings } if bindings.len() == 1 && bindings[0].0 == "a")
        );
        assert!(
            matches!(&results[1], ReplResult::LetBinding { bindings } if bindings.len() == 1 && bindings[0].0 == "b")
        );
        assert_eq!(results[2], ReplResult::Expression(Value::Int(3)));
    }

    #[test]
    fn test_repl_empty_input() {
        let mut state = State::new(None).unwrap();
        let results = state.eval("").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_repl_whitespace_only() {
        let mut state = State::new(None).unwrap();
        let results = state.eval("   \n\t  ").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_repl_function_redefine() {
        let mut state = State::new(None).unwrap();
        state.eval("fn f() -> Int { 1 }").unwrap();
        let results = state.eval("f()").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int(1))]);

        // Redefine function
        state.eval("fn f() -> Int { 2 }").unwrap();
        let results = state.eval("f()").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int(2))]);
    }

    #[test]
    fn test_repl_method_call() {
        let mut state = State::new(None).unwrap();
        let results = state.eval(r#""hello".len()"#).unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int(5))]);
    }

    #[test]
    fn test_repl_list() {
        let mut state = State::new(None).unwrap();
        let results = state.eval("[1, 2, 3]").unwrap();
        assert_eq!(
            results,
            vec![ReplResult::Expression(Value::List(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(3)
            ]))]
        );
    }

    #[test]
    fn test_repl_tuple() {
        let mut state = State::new(None).unwrap();
        let results = state.eval("(1, true)").unwrap();
        assert_eq!(
            results,
            vec![ReplResult::Expression(Value::Tuple(vec![
                Value::Int(1),
                Value::Bool(true)
            ]))]
        );
    }

    #[test]
    fn test_repl_match_expression() {
        let mut state = State::new(None).unwrap();
        let results = state.eval("match 1 { 0 => false, _ => true }").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Bool(true))]);
    }

    #[test]
    fn test_repl_struct_definition() {
        let mut state = State::new(None).unwrap();
        let results = state.eval("struct Point { x: Int, y: Int }").unwrap();
        assert_eq!(
            results,
            vec![ReplResult::StructDefined("Point".to_string())]
        );
    }

    #[test]
    fn test_repl_struct_construction() {
        let mut state = State::new(None).unwrap();
        state.eval("struct Point { x: Int, y: Int }").unwrap();
        let results = state.eval("Point { x: 10, y: 20 }").unwrap();
        assert_eq!(
            results,
            vec![ReplResult::Expression(Value::Struct {
                name: "Point".to_string(),
                fields: vec![
                    ("x".to_string(), Value::Int(10)),
                    ("y".to_string(), Value::Int(20)),
                ],
            })]
        );
    }

    #[test]
    fn test_repl_struct_field_access() {
        let mut state = State::new(None).unwrap();
        state.eval("struct Point { x: Int, y: Int }").unwrap();
        state.eval("let p = Point { x: 10, y: 20 }").unwrap();
        let results = state.eval("p.x").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int(10))]);
    }

    #[test]
    fn test_repl_struct_pattern_match() {
        let mut state = State::new(None).unwrap();
        state.eval("struct Point { x: Int, y: Int }").unwrap();
        state.eval("let p = Point { x: 10, y: 20 }").unwrap();
        let results = state.eval("match p { Point { x, y } => x + y }").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int(30))]);
    }

    #[test]
    fn test_repl_let_tuple_destructure() {
        let mut state = State::new(None).unwrap();
        let results = state.eval("let (a, b) = (1, 2)").unwrap();
        assert_eq!(results.len(), 1);
        assert!(matches!(
            &results[0],
            ReplResult::LetBinding { bindings } if bindings.len() == 2
                && bindings[0].0 == "a" && bindings[0].1 == Type::Int
                && bindings[1].0 == "b" && bindings[1].1 == Type::Int
        ));
        // Verify bindings work
        let results = state.eval("a + b").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int(3))]);
    }

    #[test]
    fn test_repl_list_pattern_in_match() {
        // List patterns are refutable, so we test them in match expressions
        let mut state = State::new(None).unwrap();
        let results = state
            .eval("match [1, 2] { [x, y] => x + y, _ => 0 }")
            .unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int(3))]);
    }

    #[test]
    fn test_repl_list_pattern_with_rest_in_match() {
        // List prefix patterns with rest binding in match
        let mut state = State::new(None).unwrap();
        let results = state
            .eval("match [1, 2, 3] { [h, ..] => h, _ => 0 }")
            .unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int(1))]);
    }

    #[test]
    fn test_repl_let_struct_destructure() {
        let mut state = State::new(None).unwrap();
        state.eval("struct Point { x: Int, y: Int }").unwrap();
        state.eval("let p = Point { x: 10, y: 20 }").unwrap();
        let results = state.eval("let Point { x, y } = p").unwrap();
        assert_eq!(results.len(), 1);
        assert!(matches!(
            &results[0],
            ReplResult::LetBinding { bindings } if bindings.len() == 2
        ));
        // Verify bindings work
        let results = state.eval("x + y").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int(30))]);
    }

    #[test]
    fn test_repl_let_struct_partial() {
        let mut state = State::new(None).unwrap();
        state.eval("struct Point { x: Int, y: Int }").unwrap();
        state.eval("let p = Point { x: 10, y: 20 }").unwrap();
        let results = state.eval("let Point { x, .. } = p").unwrap();
        assert_eq!(results.len(), 1);
        assert!(matches!(
            &results[0],
            ReplResult::LetBinding { bindings } if bindings.len() == 1 && bindings[0].0 == "x"
        ));
        // Verify binding works
        let results = state.eval("x").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int(10))]);
    }

    #[test]
    fn test_repl_let_as_pattern() {
        let mut state = State::new(None).unwrap();
        let results = state.eval("let p @ (a, b) = (1, 2)").unwrap();
        assert_eq!(results.len(), 1);
        assert!(matches!(
            &results[0],
            ReplResult::LetBinding { bindings } if bindings.len() == 3
                && bindings[0].0 == "p"
                && bindings[1].0 == "a"
                && bindings[2].0 == "b"
        ));
        // Verify all bindings work
        let results = state.eval("p").unwrap();
        assert_eq!(
            results,
            vec![ReplResult::Expression(Value::Tuple(vec![
                Value::Int(1),
                Value::Int(2)
            ]))]
        );
        let results = state.eval("a + b").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int(3))]);
    }

    #[test]
    fn test_repl_enum_definition() {
        let mut state = State::new(None).unwrap();
        let results = state.eval("enum Color { Red, Blue }").unwrap();
        assert_eq!(
            results,
            vec![ReplResult::EnumDefined("Color".to_string())]
        );
    }

    #[test]
    fn test_repl_enum_variant() {
        use crate::eval::EnumValueFields;
        let mut state = State::new(None).unwrap();
        state.eval("enum Color { Red, Blue }").unwrap();
        let results = state.eval("Color::Red").unwrap();
        assert_eq!(
            results,
            vec![ReplResult::Expression(Value::Enum {
                enum_name: "Color".to_string(),
                variant_name: "Red".to_string(),
                fields: EnumValueFields::Unit,
            })]
        );
    }

    #[test]
    fn test_repl_enum_with_data() {
        use crate::eval::EnumValueFields;
        let mut state = State::new(None).unwrap();
        state.eval("enum Option<T> { Some(T), None }").unwrap();
        let results = state.eval("Option::Some(42)").unwrap();
        assert_eq!(
            results,
            vec![ReplResult::Expression(Value::Enum {
                enum_name: "Option".to_string(),
                variant_name: "Some".to_string(),
                fields: EnumValueFields::Tuple(vec![Value::Int(42)]),
            })]
        );
        // Test pattern matching on enum
        let results = state
            .eval("match Option::Some(10) { Option::Some(x) => x, Option::None => 0 }")
            .unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int(10))]);
    }

    #[test]
    fn test_repl_type_alias() {
        let mut state = State::new(None).unwrap();
        let results = state.eval("type Id = Int").unwrap();
        assert_eq!(
            results,
            vec![ReplResult::TypeAliasDefined("Id".to_string())]
        );
        // Verify type alias works in function signature
        state.eval("fn get_id(x: Id) -> Id { x }").unwrap();
        let results = state.eval("get_id(42)").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int(42))]);
    }

    #[test]
    fn test_repl_state_with_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("lib.zoya");
        std::fs::write(
            &file,
            r#"
            fn helper() -> Int { 100 }
            "#,
        )
        .unwrap();

        let mut state = State::new(Some(&file)).unwrap();
        // Call function from the loaded file using super:: since REPL is in a submodule
        let results = state.eval("super::helper()").unwrap();
        assert_eq!(results, vec![ReplResult::Expression(Value::Int(100))]);
    }
}
