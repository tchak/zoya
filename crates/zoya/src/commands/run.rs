use std::path::Path;

use crate::eval::EvalError;
use crate::runner;

/// Run a Zoya source file and print the result
pub fn execute(path: &Path) -> Result<(), EvalError> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| EvalError::RuntimeError(format!("failed to read file: {}", e)))?;

    let value = runner::run(&source)?;
    println!("{}", value);
    Ok(())
}
