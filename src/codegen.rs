use crate::ast::{BinOp, UnaryOp};
use crate::ir::{TypedExpr, TypedFunction, TypedLetBinding, TypedMatchArm, TypedPattern};

use crate::types::Type;

/// Deep equality function name used in generated JS
const DEEP_EQ_FN: &str = "$eq";

/// Plain object check function name used in generated JS
const IS_OBJ_FN: &str = "$isObj";

/// Division by zero check function name used in generated JS
const DIV_CHECK_FN: &str = "$div";

/// BigInt absolute value function name used in generated JS
const ABS_BIGINT_FN: &str = "$absBigInt";

/// BigInt minimum function name used in generated JS
const MIN_BIGINT_FN: &str = "$minBigInt";

/// BigInt maximum function name used in generated JS
const MAX_BIGINT_FN: &str = "$maxBigInt";

/// Prelude containing helper functions for generated JS
pub fn prelude() -> &'static str {
    r#"function $isObj(x) {
  return typeof x === 'object' && x !== null && !Array.isArray(x);
}
function $div(a, b) {
  if (b === 0) throw new Error("division by zero");
  return Math.trunc(a / b);
}
function $absBigInt(x) { return x < 0n ? -x : x; }
function $minBigInt(a, b) { return a < b ? a : b; }
function $maxBigInt(a, b) { return a > b ? a : b; }
function $eq(a, b) {
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) {
      if (!$eq(a[i], b[i])) return false;
    }
    return true;
  }
  if ($isObj(a) && $isObj(b)) {
    const ka = Object.keys(a), kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    for (let k of ka) {
      if (!$eq(a[k], b[k])) return false;
    }
    return true;
  }
  return a === b;
}"#
}
/// Check if a type requires deep equality comparison
fn needs_deep_equality(ty: &Type) -> bool {
    matches!(ty, Type::List(_) | Type::Struct { .. } | Type::Enum { .. })
}

/// Generate conditions and bindings for a pattern at a given access path.
/// This is the core recursive function for nested pattern support.
///
/// # Arguments
/// * `pattern` - The typed pattern to generate code for
/// * `access_path` - JS expression to access the value (e.g., "$match", "$match[0]", "$match.x")
///
/// # Returns
/// Tuple of (conditions, bindings) where:
/// * conditions: Vec of JS boolean expressions that must all be true
/// * bindings: Vec of JS `const name = expr;` statements
fn codegen_pattern_at_path(
    pattern: &TypedPattern,
    access_path: &str,
) -> (Vec<String>, Vec<String>) {
    let mut conditions = Vec::new();
    let mut bindings = Vec::new();

    match pattern {
        TypedPattern::Literal(lit) => {
            let lit_code = codegen(lit);
            if needs_deep_equality(&lit.ty()) {
                conditions.push(format!("{}({}, {})", DEEP_EQ_FN, access_path, lit_code));
            } else {
                conditions.push(format!("{} === {}", access_path, lit_code));
            }
        }

        TypedPattern::Var { name, .. } => {
            bindings.push(format!("const {} = {};", name, access_path));
        }

        TypedPattern::Wildcard => {
            // No conditions or bindings needed
        }

        TypedPattern::As { name, pattern, .. } => {
            // Bind the entire value to name
            bindings.push(format!("const {} = {};", name, access_path));
            // Recursively handle inner pattern (conditions and additional bindings)
            let (inner_conds, inner_binds) = codegen_pattern_at_path(pattern, access_path);
            conditions.extend(inner_conds);
            bindings.extend(inner_binds);
        }

        TypedPattern::ListEmpty => {
            conditions.push(format!(
                "Array.isArray({}) && {}.length === 0",
                access_path, access_path
            ));
        }

        TypedPattern::ListExact { patterns, len } => {
            conditions.push(format!(
                "Array.isArray({}) && {}.length === {}",
                access_path, access_path, len
            ));
            for (i, pat) in patterns.iter().enumerate() {
                let child_path = format!("{}[{}]", access_path, i);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }

        TypedPattern::ListPrefix {
            patterns,
            rest_binding,
            min_len,
        } => {
            conditions.push(format!(
                "Array.isArray({}) && {}.length >= {}",
                access_path, access_path, min_len
            ));
            for (i, pat) in patterns.iter().enumerate() {
                let child_path = format!("{}[{}]", access_path, i);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
            // Handle rest binding: rest @ .. binds to remaining elements
            if let Some(name) = rest_binding {
                bindings.push(format!(
                    "const {} = {}.slice({});",
                    name, access_path, patterns.len()
                ));
            }
        }

        TypedPattern::ListSuffix {
            patterns,
            rest_binding,
            min_len,
        } => {
            conditions.push(format!(
                "Array.isArray({}) && {}.length >= {}",
                access_path, access_path, min_len
            ));
            for (i, pat) in patterns.iter().enumerate() {
                let offset = min_len - i;
                let child_path = format!("{}.length - {}", access_path, offset);
                let indexed_path = format!("{}[{}]", access_path, child_path);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &indexed_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
            // Handle rest binding: rest @ .. binds to leading elements
            if let Some(name) = rest_binding {
                bindings.push(format!(
                    "const {} = {}.slice(0, {}.length - {});",
                    name, access_path, access_path, patterns.len()
                ));
            }
        }

        TypedPattern::ListPrefixSuffix {
            prefix,
            suffix,
            rest_binding,
            min_len,
        } => {
            conditions.push(format!(
                "Array.isArray({}) && {}.length >= {}",
                access_path, access_path, min_len
            ));
            // Prefix patterns: indexed from start
            for (i, pat) in prefix.iter().enumerate() {
                let child_path = format!("{}[{}]", access_path, i);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
            // Suffix patterns: indexed from end
            let suffix_len = suffix.len();
            for (i, pat) in suffix.iter().enumerate() {
                let offset = suffix_len - i;
                let child_path = format!("{}.length - {}", access_path, offset);
                let indexed_path = format!("{}[{}]", access_path, child_path);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &indexed_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
            // Handle rest binding: rest @ .. binds to middle elements
            if let Some(name) = rest_binding {
                bindings.push(format!(
                    "const {} = {}.slice({}, {}.length - {});",
                    name,
                    access_path,
                    prefix.len(),
                    access_path,
                    suffix_len
                ));
            }
        }

        TypedPattern::TupleEmpty => {
            conditions.push(format!(
                "Array.isArray({}) && {}.length === 0",
                access_path, access_path
            ));
        }

        TypedPattern::TupleExact { patterns, len } => {
            conditions.push(format!(
                "Array.isArray({}) && {}.length === {}",
                access_path, access_path, len
            ));
            for (i, pat) in patterns.iter().enumerate() {
                let child_path = format!("{}[{}]", access_path, i);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }

        TypedPattern::TuplePrefix {
            patterns,
            rest_binding,
            total_len,
        } => {
            conditions.push(format!(
                "Array.isArray({}) && {}.length === {}",
                access_path, access_path, total_len
            ));
            for (i, pat) in patterns.iter().enumerate() {
                let child_path = format!("{}[{}]", access_path, i);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
            // Handle rest binding: rest @ .. binds to tuple of remaining elements
            if let Some(name) = rest_binding {
                let rest_indices: Vec<String> = (patterns.len()..*total_len)
                    .map(|i| format!("{}[{}]", access_path, i))
                    .collect();
                bindings.push(format!("const {} = [{}];", name, rest_indices.join(", ")));
            }
        }

        TypedPattern::TupleSuffix {
            patterns,
            rest_binding,
            total_len,
        } => {
            conditions.push(format!(
                "Array.isArray({}) && {}.length === {}",
                access_path, access_path, total_len
            ));
            let start_idx = total_len - patterns.len();
            for (i, pat) in patterns.iter().enumerate() {
                let idx = start_idx + i;
                let child_path = format!("{}[{}]", access_path, idx);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
            // Handle rest binding: rest @ .. binds to tuple of leading elements
            if let Some(name) = rest_binding {
                let rest_indices: Vec<String> = (0..start_idx)
                    .map(|i| format!("{}[{}]", access_path, i))
                    .collect();
                bindings.push(format!("const {} = [{}];", name, rest_indices.join(", ")));
            }
        }

        TypedPattern::TuplePrefixSuffix {
            prefix,
            suffix,
            rest_binding,
            total_len,
        } => {
            conditions.push(format!(
                "Array.isArray({}) && {}.length === {}",
                access_path, access_path, total_len
            ));
            // Prefix patterns
            for (i, pat) in prefix.iter().enumerate() {
                let child_path = format!("{}[{}]", access_path, i);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
            // Suffix patterns
            let suffix_start = total_len - suffix.len();
            for (i, pat) in suffix.iter().enumerate() {
                let idx = suffix_start + i;
                let child_path = format!("{}[{}]", access_path, idx);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
            // Handle rest binding: rest @ .. binds to tuple of middle elements
            if let Some(name) = rest_binding {
                let rest_indices: Vec<String> = (prefix.len()..suffix_start)
                    .map(|i| format!("{}[{}]", access_path, i))
                    .collect();
                bindings.push(format!("const {} = [{}];", name, rest_indices.join(", ")));
            }
        }

        TypedPattern::StructExact { fields, .. } | TypedPattern::StructPartial { fields, .. } => {
            conditions.push(format!("{}({})", IS_OBJ_FN, access_path));
            for (field_name, pat) in fields {
                let child_path = format!("{}.{}", access_path, field_name);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }

        // Enum patterns
        TypedPattern::EnumUnit { path } => {
            conditions.push(format!(
                "{}({}) && {}.$tag === \"{}\"",
                IS_OBJ_FN, access_path, access_path, path.last()
            ));
        }

        TypedPattern::EnumTupleExact {
            path,
            patterns,
            ..
        } => {
            conditions.push(format!(
                "{}({}) && {}.$tag === \"{}\"",
                IS_OBJ_FN, access_path, access_path, path.last()
            ));
            for (i, pat) in patterns.iter().enumerate() {
                let child_path = format!("{}.${}",  access_path, i);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }

        TypedPattern::EnumTuplePrefix {
            path,
            patterns,
            ..
        } => {
            conditions.push(format!(
                "{}({}) && {}.$tag === \"{}\"",
                IS_OBJ_FN, access_path, access_path, path.last()
            ));
            for (i, pat) in patterns.iter().enumerate() {
                let child_path = format!("{}.${}",  access_path, i);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }

        TypedPattern::EnumTupleSuffix {
            path,
            patterns,
            total_fields,
            ..
        } => {
            conditions.push(format!(
                "{}({}) && {}.$tag === \"{}\"",
                IS_OBJ_FN, access_path, access_path, path.last()
            ));
            let start_idx = total_fields - patterns.len();
            for (i, pat) in patterns.iter().enumerate() {
                let idx = start_idx + i;
                let child_path = format!("{}.${}",  access_path, idx);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }

        TypedPattern::EnumTuplePrefixSuffix {
            path,
            prefix,
            suffix,
            total_fields,
            ..
        } => {
            conditions.push(format!(
                "{}({}) && {}.$tag === \"{}\"",
                IS_OBJ_FN, access_path, access_path, path.last()
            ));
            // Prefix patterns
            for (i, pat) in prefix.iter().enumerate() {
                let child_path = format!("{}.${}",  access_path, i);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
            // Suffix patterns
            let suffix_start = total_fields - suffix.len();
            for (i, pat) in suffix.iter().enumerate() {
                let idx = suffix_start + i;
                let child_path = format!("{}.${}",  access_path, idx);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }

        TypedPattern::EnumStructExact { path, fields }
        | TypedPattern::EnumStructPartial { path, fields } => {
            conditions.push(format!(
                "{}({}) && {}.$tag === \"{}\"",
                IS_OBJ_FN, access_path, access_path, path.last()
            ));
            for (field_name, pat) in fields {
                let child_path = format!("{}.{}", access_path, field_name);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }
    }

    (conditions, bindings)
}

pub fn codegen(expr: &TypedExpr) -> String {
    match expr {
        TypedExpr::Int(n) => n.to_string(),
        TypedExpr::BigInt(n) => format!("{}n", n), // BigInt literal
        TypedExpr::Float(n) => format_float(*n),
        TypedExpr::Bool(b) => b.to_string(),
        TypedExpr::String(s) => escape_js_string(s),
        TypedExpr::List { elements, .. } | TypedExpr::Tuple { elements, .. } => {
            let strs: Vec<String> = elements.iter().map(codegen).collect();
            format!("[{}]", strs.join(", "))
        }
        TypedExpr::Var { path, .. } => path.last().to_string(),
        TypedExpr::Call { path, args, .. } => {
            let args_str: Vec<String> = args.iter().map(codegen).collect();
            format!("{}({})", path.last(), args_str.join(", "))
        }
        TypedExpr::UnaryOp { op, expr, .. } => {
            let inner = codegen(expr);
            match op {
                UnaryOp::Neg => format!("(-({}))", inner),
            }
        }
        TypedExpr::BinOp {
            op,
            left,
            right,
            ty,
        } => {
            let l = codegen(left);
            let r = codegen(right);

            // Handle equality operators with structural comparison for lists
            if matches!(op, BinOp::Eq | BinOp::Ne) && needs_deep_equality(&left.ty()) {
                let deep_eq = format!("{}({}, {})", DEEP_EQ_FN, l, r);
                return if *op == BinOp::Eq {
                    deep_eq
                } else {
                    format!("(!{})", deep_eq)
                };
            }

            // Handle Int division with truncation and division by zero check
            if *op == BinOp::Div && *ty == Type::Int {
                return format!("{}({}, {})", DIV_CHECK_FN, l, r);
            }

            let op_str = match op {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "/",
                BinOp::Eq => "===",
                BinOp::Ne => "!==",
                BinOp::Lt => "<",
                BinOp::Gt => ">",
                BinOp::Le => "<=",
                BinOp::Ge => ">=",
            };
            format!("(({}) {} ({}))", l, op_str, r)
        }
        TypedExpr::Block { bindings, result } => {
            // Generate IIFE for proper scoping
            let mut parts = Vec::new();
            parts.push("(function() {".to_string());

            for binding in bindings {
                let value_code = codegen(&binding.value);
                parts.push(format!("const {} = {};", binding.name, value_code));
            }

            let result_code = codegen(result);
            parts.push(format!("return {};", result_code));
            parts.push("})()".to_string());

            parts.join(" ")
        }
        TypedExpr::Match { scrutinee, arms, .. } => {
            codegen_match(scrutinee, arms)
        }
        TypedExpr::MethodCall {
            receiver,
            method,
            args,
            ..
        } => {
            let receiver_code = codegen(receiver);
            let receiver_ty = receiver.ty();
            let args_code: Vec<String> = args.iter().map(codegen).collect();

            match method.as_str() {
                // String methods
                "len" => format!("({}).length", receiver_code),
                "is_empty" => format!("(({}).length === 0)", receiver_code),
                "contains" => format!("({}).includes({})", receiver_code, args_code[0]),
                "starts_with" => format!("({}).startsWith({})", receiver_code, args_code[0]),
                "ends_with" => format!("({}).endsWith({})", receiver_code, args_code[0]),
                "to_uppercase" => format!("({}).toUpperCase()", receiver_code),
                "to_lowercase" => format!("({}).toLowerCase()", receiver_code),
                "trim" => format!("({}).trim()", receiver_code),

                // Numeric methods - BigInt needs special handling (no Math functions for BigInt)
                "abs" => match receiver_ty {
                    Type::BigInt => format!("{}({})", ABS_BIGINT_FN, receiver_code),
                    _ => format!("Math.abs({})", receiver_code),
                },
                "min" => match receiver_ty {
                    Type::BigInt => format!("{}({}, {})", MIN_BIGINT_FN, receiver_code, args_code[0]),
                    _ => format!("Math.min({}, {})", receiver_code, args_code[0]),
                },
                "max" => match receiver_ty {
                    Type::BigInt => format!("{}({}, {})", MAX_BIGINT_FN, receiver_code, args_code[0]),
                    _ => format!("Math.max({}, {})", receiver_code, args_code[0]),
                },

                // Type conversion
                "to_string" => format!("String({})", receiver_code),
                "to_float" => receiver_code, // JS numbers are already floats
                "to_int" => format!("Math.trunc({})", receiver_code),

                // Float-specific math
                "floor" => format!("Math.floor({})", receiver_code),
                "ceil" => format!("Math.ceil({})", receiver_code),
                "round" => format!("Math.round({})", receiver_code),
                "sqrt" => format!("Math.sqrt({})", receiver_code),

                // List methods
                "reverse" => format!("([...({})].reverse())", receiver_code),
                "push" => format!("([...{}, {}])", receiver_code, args_code[0]),
                "concat" => format!("([...{}, ...{}])", receiver_code, args_code[0]),

                _ => panic!("unknown method in codegen: {}", method),
            }
        }

        TypedExpr::Lambda { params, body, .. } => {
            let param_names: Vec<&str> = params.iter().map(|(name, _)| name.as_str()).collect();
            let body_code = codegen(body);
            if params.is_empty() {
                format!("(() => {})", body_code)
            } else {
                format!("(({}) => {})", param_names.join(", "), body_code)
            }
        }
        TypedExpr::StructConstruct { fields, .. } => {
            // Generate a plain JS object
            if fields.is_empty() {
                "({})".to_string()
            } else {
                let field_strs: Vec<String> = fields
                    .iter()
                    .map(|(name, expr)| format!("{}: {}", name, codegen(expr)))
                    .collect();
                format!("({{ {} }})", field_strs.join(", "))
            }
        }
        TypedExpr::FieldAccess { expr, field, .. } => {
            format!("({}).{}", codegen(expr), field)
        }
        TypedExpr::EnumConstruct { path, fields, .. } => {
            use crate::ir::TypedEnumConstructFields;
            let variant_name = path.last();
            match fields {
                TypedEnumConstructFields::Unit => {
                    format!("({{ $tag: \"{}\" }})", variant_name)
                }
                TypedEnumConstructFields::Tuple(exprs) => {
                    if exprs.is_empty() {
                        format!("({{ $tag: \"{}\" }})", variant_name)
                    } else {
                        let field_strs: Vec<String> = exprs
                            .iter()
                            .enumerate()
                            .map(|(i, e)| format!("${}: {}", i, codegen(e)))
                            .collect();
                        format!("({{ $tag: \"{}\", {} }})", variant_name, field_strs.join(", "))
                    }
                }
                TypedEnumConstructFields::Struct(fields) => {
                    if fields.is_empty() {
                        format!("({{ $tag: \"{}\" }})", variant_name)
                    } else {
                        let field_strs: Vec<String> = fields
                            .iter()
                            .map(|(name, e)| format!("{}: {}", name, codegen(e)))
                            .collect();
                        format!("({{ $tag: \"{}\", {} }})", variant_name, field_strs.join(", "))
                    }
                }
            }
        }
    }
}

/// Generate JS code for a single match arm
fn codegen_match_arm(pattern: &TypedPattern, result: &TypedExpr) -> String {
    let result_code = codegen(result);
    let (conditions, bindings) = codegen_pattern_at_path(pattern, "$match");

    let condition_str = if conditions.is_empty() {
        String::new()
    } else {
        conditions.join(" && ")
    };

    let bindings_str = bindings.join(" ");

    // Wildcard pattern - unconditional return
    if matches!(pattern, TypedPattern::Wildcard) {
        return format!("return {};", result_code);
    }

    // Variable pattern - block with binding
    if matches!(pattern, TypedPattern::Var { .. }) {
        return format!("{{ {} return {}; }}", bindings_str, result_code);
    }

    // All other patterns - conditional
    if condition_str.is_empty() {
        format!("{{ {} return {}; }}", bindings_str, result_code)
    } else if bindings_str.is_empty() {
        format!("if ({}) {{ return {}; }}", condition_str, result_code)
    } else {
        format!(
            "if ({}) {{ {} return {}; }}",
            condition_str, bindings_str, result_code
        )
    }
}

/// Generate JS code for a match expression
fn codegen_match(scrutinee: &TypedExpr, arms: &[TypedMatchArm]) -> String {
    let scrutinee_code = codegen(scrutinee);
    let mut parts = vec!["(function($match) {".to_string()];

    for arm in arms {
        parts.push(codegen_match_arm(&arm.pattern, &arm.result));
    }

    parts.push(format!("}})({})", scrutinee_code));
    parts.join(" ")
}

/// Generate JS code for a function definition
pub fn codegen_function(func: &TypedFunction) -> String {
    let params: Vec<&str> = func.params.iter().map(|(name, _)| name.as_str()).collect();
    let body = codegen(&func.body);
    format!(
        "function {}({}) {{ return {}; }}",
        func.name,
        params.join(", "),
        body
    )
}

/// Generate JS code for a REPL let binding
pub fn codegen_let(binding: &TypedLetBinding) -> String {
    let value_code = codegen(&binding.value);
    // Use var for REPL to allow redefinition and global scope
    format!("var {} = {};", binding.name, value_code)
}

fn format_float(n: f64) -> String {
    let s = n.to_string();
    // Ensure float always has decimal point for JS
    if s.contains('.') {
        s
    } else {
        format!("{}.0", s)
    }
}

fn escape_js_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 2);
    result.push('"');
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            _ => result.push(c),
        }
    }
    result.push('"');
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::QualifiedPath;

    #[test]
    fn test_codegen_int() {
        let expr = TypedExpr::Int(42);
        assert_eq!(codegen(&expr), "42");
    }

    #[test]
    fn test_codegen_negative_int() {
        let expr = TypedExpr::Int(-42);
        assert_eq!(codegen(&expr), "-42");
    }

    #[test]
    fn test_codegen_bigint() {
        let expr = TypedExpr::BigInt(42);
        assert_eq!(codegen(&expr), "42n");
    }

    #[test]
    fn test_codegen_bigint_large() {
        let expr = TypedExpr::BigInt(9_000_000_000);
        assert_eq!(codegen(&expr), "9000000000n");
    }

    #[test]
    fn test_codegen_float() {
        let expr = TypedExpr::Float(3.14);
        assert_eq!(codegen(&expr), "3.14");
    }

    #[test]
    fn test_codegen_float_whole_number() {
        let expr = TypedExpr::Float(5.0);
        assert_eq!(codegen(&expr), "5.0");
    }

    #[test]
    fn test_codegen_unary_neg_int() {
        let expr = TypedExpr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(TypedExpr::Int(42)),
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "(-(42))");
    }

    #[test]
    fn test_codegen_unary_neg_bigint() {
        let expr = TypedExpr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(TypedExpr::BigInt(42)),
            ty: Type::BigInt,
        };
        assert_eq!(codegen(&expr), "(-(42n))");
    }

    #[test]
    fn test_codegen_addition_int() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Int(1)),
            right: Box::new(TypedExpr::Int(2)),
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "((1) + (2))");
    }

    #[test]
    fn test_codegen_addition_bigint() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::BigInt(1)),
            right: Box::new(TypedExpr::BigInt(2)),
            ty: Type::BigInt,
        };
        assert_eq!(codegen(&expr), "((1n) + (2n))");
    }

    #[test]
    fn test_codegen_subtraction() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Sub,
            left: Box::new(TypedExpr::Int(5)),
            right: Box::new(TypedExpr::Int(3)),
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "((5) - (3))");
    }

    #[test]
    fn test_codegen_multiplication() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Mul,
            left: Box::new(TypedExpr::Int(3)),
            right: Box::new(TypedExpr::Int(4)),
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "((3) * (4))");
    }

    #[test]
    fn test_codegen_division() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Div,
            left: Box::new(TypedExpr::Int(10)),
            right: Box::new(TypedExpr::Int(2)),
            ty: Type::Int,
        };
        // Int division uses $div for truncation and division-by-zero checking
        assert_eq!(codegen(&expr), "$div(10, 2)");
    }

    #[test]
    fn test_codegen_complex_expression() {
        // 2 + 3 * 4
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Int(2)),
            right: Box::new(TypedExpr::BinOp {
                op: BinOp::Mul,
                left: Box::new(TypedExpr::Int(3)),
                right: Box::new(TypedExpr::Int(4)),
                ty: Type::Int,
            }),
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "((2) + (((3) * (4))))");
    }

    #[test]
    fn test_codegen_float_expression() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Float(1.5)),
            right: Box::new(TypedExpr::Float(2.5)),
            ty: Type::Float,
        };
        assert_eq!(codegen(&expr), "((1.5) + (2.5))");
    }

    #[test]
    fn test_codegen_var() {
        let expr = TypedExpr::Var {
            path: QualifiedPath::simple("x".to_string()),
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "x");
    }

    #[test]
    fn test_codegen_call_no_args() {
        let expr = TypedExpr::Call {
            path: QualifiedPath::simple("foo".to_string()),
            args: vec![],
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "foo()");
    }

    #[test]
    fn test_codegen_call_one_arg() {
        let expr = TypedExpr::Call {
            path: QualifiedPath::simple("square".to_string()),
            args: vec![TypedExpr::Int(5)],
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "square(5)");
    }

    #[test]
    fn test_codegen_call_multiple_args() {
        let expr = TypedExpr::Call {
            path: QualifiedPath::simple("add".to_string()),
            args: vec![TypedExpr::Int(1), TypedExpr::Int(2)],
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "add(1, 2)");
    }

    #[test]
    fn test_codegen_call_with_vars() {
        let expr = TypedExpr::Call {
            path: QualifiedPath::simple("add".to_string()),
            args: vec![
                TypedExpr::Var {
                    path: QualifiedPath::simple("x".to_string()),
                    ty: Type::Int,
                },
                TypedExpr::Var {
                    path: QualifiedPath::simple("y".to_string()),
                    ty: Type::Int,
                },
            ],
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "add(x, y)");
    }

    #[test]
    fn test_codegen_var_in_expression() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Var {
                path: QualifiedPath::simple("x".to_string()),
                ty: Type::Int,
            }),
            right: Box::new(TypedExpr::Int(1)),
            ty: Type::Int,
        };
        assert_eq!(codegen(&expr), "((x) + (1))");
    }

    #[test]
    fn test_codegen_function() {
        let func = TypedFunction {
            name: "square".to_string(),
            params: vec![("x".to_string(), Type::Int)],
            body: TypedExpr::BinOp {
                op: BinOp::Mul,
                left: Box::new(TypedExpr::Var {
                    path: QualifiedPath::simple("x".to_string()),
                    ty: Type::Int,
                }),
                right: Box::new(TypedExpr::Var {
                    path: QualifiedPath::simple("x".to_string()),
                    ty: Type::Int,
                }),
                ty: Type::Int,
            },
            return_type: Type::Int,
        };
        assert_eq!(
            codegen_function(&func),
            "function square(x) { return ((x) * (x)); }"
        );
    }

    #[test]
    fn test_codegen_function_multiple_params() {
        let func = TypedFunction {
            name: "add".to_string(),
            params: vec![
                ("x".to_string(), Type::Int),
                ("y".to_string(), Type::Int),
            ],
            body: TypedExpr::BinOp {
                op: BinOp::Add,
                left: Box::new(TypedExpr::Var {
                    path: QualifiedPath::simple("x".to_string()),
                    ty: Type::Int,
                }),
                right: Box::new(TypedExpr::Var {
                    path: QualifiedPath::simple("y".to_string()),
                    ty: Type::Int,
                }),
                ty: Type::Int,
            },
            return_type: Type::Int,
        };
        assert_eq!(
            codegen_function(&func),
            "function add(x, y) { return ((x) + (y)); }"
        );
    }

    #[test]
    fn test_codegen_function_no_params() {
        let func = TypedFunction {
            name: "answer".to_string(),
            params: vec![],
            body: TypedExpr::Int(42),
            return_type: Type::Int,
        };
        assert_eq!(
            codegen_function(&func),
            "function answer() { return 42; }"
        );
    }

    #[test]
    fn test_codegen_bigint_function() {
        let func = TypedFunction {
            name: "big".to_string(),
            params: vec![("x".to_string(), Type::BigInt)],
            body: TypedExpr::BinOp {
                op: BinOp::Add,
                left: Box::new(TypedExpr::Var {
                    path: QualifiedPath::simple("x".to_string()),
                    ty: Type::BigInt,
                }),
                right: Box::new(TypedExpr::BigInt(1)),
                ty: Type::BigInt,
            },
            return_type: Type::BigInt,
        };
        assert_eq!(
            codegen_function(&func),
            "function big(x) { return ((x) + (1n)); }"
        );
    }
}
