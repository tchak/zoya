use crate::ast::{BinOp, UnaryOp};
use crate::ir::{TypedExpr, TypedFunction, TypedLetBinding, TypedMatchArm, TypedPattern};

use crate::types::Type;

/// Deep equality function name used in generated JS
const DEEP_EQ_FN: &str = "$eq";

/// Plain object check function name used in generated JS
const IS_OBJ_FN: &str = "$isObj";

/// Prelude containing helper functions for generated JS
pub fn prelude() -> &'static str {
    r#"function $isObj(x) {
  return typeof x === 'object' && x !== null && !Array.isArray(x);
}
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

        TypedPattern::ListPrefix { patterns, min_len } => {
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
        }

        TypedPattern::ListSuffix { patterns, min_len } => {
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
        }

        TypedPattern::ListPrefixSuffix {
            prefix,
            suffix,
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

        TypedPattern::TuplePrefix { patterns, total_len } => {
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
        }

        TypedPattern::TupleSuffix { patterns, total_len } => {
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
        }

        TypedPattern::TuplePrefixSuffix {
            prefix,
            suffix,
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
        TypedPattern::EnumUnit { variant_name, .. } => {
            conditions.push(format!(
                "{}({}) && {}.$tag === \"{}\"",
                IS_OBJ_FN, access_path, access_path, variant_name
            ));
        }

        TypedPattern::EnumTupleExact {
            variant_name,
            patterns,
            ..
        } => {
            conditions.push(format!(
                "{}({}) && {}.$tag === \"{}\"",
                IS_OBJ_FN, access_path, access_path, variant_name
            ));
            for (i, pat) in patterns.iter().enumerate() {
                let child_path = format!("{}.${}",  access_path, i);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }

        TypedPattern::EnumTuplePrefix {
            variant_name,
            patterns,
            ..
        } => {
            conditions.push(format!(
                "{}({}) && {}.$tag === \"{}\"",
                IS_OBJ_FN, access_path, access_path, variant_name
            ));
            for (i, pat) in patterns.iter().enumerate() {
                let child_path = format!("{}.${}",  access_path, i);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }

        TypedPattern::EnumTupleSuffix {
            variant_name,
            patterns,
            total_fields,
            ..
        } => {
            conditions.push(format!(
                "{}({}) && {}.$tag === \"{}\"",
                IS_OBJ_FN, access_path, access_path, variant_name
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
            variant_name,
            prefix,
            suffix,
            total_fields,
            ..
        } => {
            conditions.push(format!(
                "{}({}) && {}.$tag === \"{}\"",
                IS_OBJ_FN, access_path, access_path, variant_name
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

        TypedPattern::EnumStructExact {
            variant_name,
            fields,
            ..
        }
        | TypedPattern::EnumStructPartial {
            variant_name,
            fields,
            ..
        } => {
            conditions.push(format!(
                "{}({}) && {}.$tag === \"{}\"",
                IS_OBJ_FN, access_path, access_path, variant_name
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
        TypedExpr::Int32(n) => n.to_string(),
        TypedExpr::Int64(n) => format!("{}n", n), // BigInt literal
        TypedExpr::Float(n) => format_float(*n),
        TypedExpr::Bool(b) => b.to_string(),
        TypedExpr::String(s) => escape_js_string(s),
        TypedExpr::List { elements, .. } => {
            let element_strs: Vec<String> = elements.iter().map(codegen).collect();
            format!("[{}]", element_strs.join(", "))
        }
        TypedExpr::Tuple { elements, .. } => {
            // Tuples are represented as JS arrays
            let element_strs: Vec<String> = elements.iter().map(codegen).collect();
            format!("[{}]", element_strs.join(", "))
        }
        TypedExpr::Var { name, .. } => name.clone(),
        TypedExpr::Call { func, args, ty } => {
            let args_str: Vec<String> = args.iter().map(codegen).collect();
            let call = format!("{}({})", func, args_str.join(", "));
            // Wrap Int32 function calls with overflow check
            if *ty == Type::Int32 {
                wrap_int32_overflow(&call)
            } else {
                call
            }
        }
        TypedExpr::UnaryOp { op, expr, ty } => {
            let inner = codegen(expr);
            let result = match op {
                UnaryOp::Neg => format!("(-({}))", inner),
            };
            // Wrap Int32 unary ops with overflow check
            if *ty == Type::Int32 {
                wrap_int32_overflow(&result)
            } else {
                result
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
            let result = format!("(({}) {} ({}))", l, op_str, r);
            // Wrap Int32 operations with overflow check
            if *ty == Type::Int32 {
                wrap_int32_overflow(&result)
            } else {
                result
            }
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
            ty,
        } => {
            let receiver_code = codegen(receiver);
            let receiver_ty = receiver.ty();
            let args_code: Vec<String> = args.iter().map(codegen).collect();

            let result = match method.as_str() {
                // String methods
                "len" => format!("({}).length", receiver_code),
                "is_empty" => format!("(({}).length === 0)", receiver_code),
                "contains" => format!("({}).includes({})", receiver_code, args_code[0]),
                "starts_with" => format!("({}).startsWith({})", receiver_code, args_code[0]),
                "ends_with" => format!("({}).endsWith({})", receiver_code, args_code[0]),
                "to_uppercase" => format!("({}).toUpperCase()", receiver_code),
                "to_lowercase" => format!("({}).toLowerCase()", receiver_code),
                "trim" => format!("({}).trim()", receiver_code),

                // Numeric methods - Int64 needs special handling (no Math functions for BigInt)
                "abs" => match receiver_ty {
                    Type::Int64 => format!("((x) => x < 0n ? -x : x)({})", receiver_code),
                    _ => format!("Math.abs({})", receiver_code),
                },
                "min" => match receiver_ty {
                    Type::Int64 => {
                        format!("((a, b) => a < b ? a : b)({}, {})", receiver_code, args_code[0])
                    }
                    _ => format!("Math.min({}, {})", receiver_code, args_code[0]),
                },
                "max" => match receiver_ty {
                    Type::Int64 => {
                        format!("((a, b) => a > b ? a : b)({}, {})", receiver_code, args_code[0])
                    }
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
            };

            // Wrap Int32 results with overflow check
            if *ty == Type::Int32 {
                wrap_int32_overflow(&result)
            } else {
                result
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
        TypedExpr::EnumConstruct {
            variant_name,
            fields,
            ..
        } => {
            use crate::ir::TypedEnumConstructFields;
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

/// Generate JS code for a match expression
fn codegen_match(scrutinee: &TypedExpr, arms: &[TypedMatchArm]) -> String {
    let scrutinee_code = codegen(scrutinee);
    let mut parts = Vec::new();

    parts.push("(function($match) {".to_string());

    for arm in arms {
        match &arm.pattern {
            TypedPattern::Literal(lit) => {
                let lit_code = codegen(lit);
                let result_code = codegen(&arm.result);
                parts.push(format!(
                    "if ($match === {}) {{ return {}; }}",
                    lit_code, result_code
                ));
            }
            TypedPattern::Var { name, .. } => {
                let result_code = codegen(&arm.result);
                parts.push(format!(
                    "{{ const {} = $match; return {}; }}",
                    name, result_code
                ));
            }
            TypedPattern::Wildcard => {
                let result_code = codegen(&arm.result);
                parts.push(format!("return {};", result_code));
            }
            TypedPattern::ListEmpty => {
                let result_code = codegen(&arm.result);
                parts.push(format!(
                    "if (Array.isArray($match) && $match.length === 0) {{ return {}; }}",
                    result_code
                ));
            }
            TypedPattern::ListExact { patterns, len } => {
                let result_code = codegen(&arm.result);
                let (condition, bindings) = codegen_list_pattern_bindings(patterns, *len, true);
                parts.push(format!(
                    "if (Array.isArray($match) && $match.length === {} && {}) {{ {} return {}; }}",
                    len, condition, bindings, result_code
                ));
            }
            TypedPattern::ListPrefix { patterns, min_len } => {
                let result_code = codegen(&arm.result);
                let (condition, bindings) = codegen_list_pattern_bindings(patterns, *min_len, false);
                if condition == "true" {
                    parts.push(format!(
                        "if (Array.isArray($match) && $match.length >= {}) {{ {} return {}; }}",
                        min_len, bindings, result_code
                    ));
                } else {
                    parts.push(format!(
                        "if (Array.isArray($match) && $match.length >= {} && {}) {{ {} return {}; }}",
                        min_len, condition, bindings, result_code
                    ));
                }
            }
            TypedPattern::ListSuffix { patterns, min_len } => {
                let result_code = codegen(&arm.result);
                let (condition, bindings) = codegen_suffix_pattern_bindings(patterns, *min_len);
                if condition == "true" {
                    parts.push(format!(
                        "if (Array.isArray($match) && $match.length >= {}) {{ {} return {}; }}",
                        min_len, bindings, result_code
                    ));
                } else {
                    parts.push(format!(
                        "if (Array.isArray($match) && $match.length >= {} && {}) {{ {} return {}; }}",
                        min_len, condition, bindings, result_code
                    ));
                }
            }
            TypedPattern::ListPrefixSuffix { prefix, suffix, min_len } => {
                let result_code = codegen(&arm.result);
                let (condition, bindings) = codegen_prefix_suffix_pattern_bindings(prefix, suffix);
                if condition == "true" {
                    parts.push(format!(
                        "if (Array.isArray($match) && $match.length >= {}) {{ {} return {}; }}",
                        min_len, bindings, result_code
                    ));
                } else {
                    parts.push(format!(
                        "if (Array.isArray($match) && $match.length >= {} && {}) {{ {} return {}; }}",
                        min_len, condition, bindings, result_code
                    ));
                }
            }
            // Tuple patterns (tuples have fixed size, so always use === for length)
            TypedPattern::TupleEmpty => {
                let result_code = codegen(&arm.result);
                parts.push(format!(
                    "if (Array.isArray($match) && $match.length === 0) {{ return {}; }}",
                    result_code
                ));
            }
            TypedPattern::TupleExact { patterns, len } => {
                let result_code = codegen(&arm.result);
                let (condition, bindings) = codegen_tuple_pattern_bindings(patterns, *len);
                parts.push(format!(
                    "if (Array.isArray($match) && $match.length === {} && {}) {{ {} return {}; }}",
                    len, condition, bindings, result_code
                ));
            }
            TypedPattern::TuplePrefix { patterns, total_len } => {
                let result_code = codegen(&arm.result);
                let (condition, bindings) = codegen_tuple_pattern_bindings(patterns, *total_len);
                parts.push(format!(
                    "if (Array.isArray($match) && $match.length === {} && {}) {{ {} return {}; }}",
                    total_len, condition, bindings, result_code
                ));
            }
            TypedPattern::TupleSuffix { patterns, total_len } => {
                let result_code = codegen(&arm.result);
                let (condition, bindings) = codegen_tuple_suffix_bindings(patterns, *total_len);
                parts.push(format!(
                    "if (Array.isArray($match) && $match.length === {} && {}) {{ {} return {}; }}",
                    total_len, condition, bindings, result_code
                ));
            }
            TypedPattern::TuplePrefixSuffix { prefix, suffix, total_len } => {
                let result_code = codegen(&arm.result);
                let (condition, bindings) = codegen_tuple_prefix_suffix_bindings(prefix, suffix, *total_len);
                parts.push(format!(
                    "if (Array.isArray($match) && $match.length === {} && {}) {{ {} return {}; }}",
                    total_len, condition, bindings, result_code
                ));
            }
            TypedPattern::StructExact { fields, .. } | TypedPattern::StructPartial { fields, .. } => {
                let result_code = codegen(&arm.result);
                let (condition, bindings) = codegen_struct_pattern_bindings(fields);
                if condition == "true" {
                    parts.push(format!(
                        "if ({}($match)) {{ {} return {}; }}",
                        IS_OBJ_FN, bindings, result_code
                    ));
                } else {
                    parts.push(format!(
                        "if ({}($match) && {}) {{ {} return {}; }}",
                        IS_OBJ_FN, condition, bindings, result_code
                    ));
                }
            }
            // Enum patterns
            TypedPattern::EnumUnit { variant_name, .. } => {
                let result_code = codegen(&arm.result);
                parts.push(format!(
                    "if ({}($match) && $match.$tag === \"{}\") {{ return {}; }}",
                    IS_OBJ_FN, variant_name, result_code
                ));
            }
            TypedPattern::EnumTupleExact { variant_name, patterns, .. } => {
                let result_code = codegen(&arm.result);
                let (condition, bindings) = codegen_enum_tuple_pattern_bindings(patterns);
                if condition == "true" {
                    parts.push(format!(
                        "if ({}($match) && $match.$tag === \"{}\") {{ {} return {}; }}",
                        IS_OBJ_FN, variant_name, bindings, result_code
                    ));
                } else {
                    parts.push(format!(
                        "if ({}($match) && $match.$tag === \"{}\" && {}) {{ {} return {}; }}",
                        IS_OBJ_FN, variant_name, condition, bindings, result_code
                    ));
                }
            }
            TypedPattern::EnumTuplePrefix { variant_name, patterns, .. } => {
                let result_code = codegen(&arm.result);
                let (condition, bindings) = codegen_enum_tuple_pattern_bindings(patterns);
                if condition == "true" {
                    parts.push(format!(
                        "if ({}($match) && $match.$tag === \"{}\") {{ {} return {}; }}",
                        IS_OBJ_FN, variant_name, bindings, result_code
                    ));
                } else {
                    parts.push(format!(
                        "if ({}($match) && $match.$tag === \"{}\" && {}) {{ {} return {}; }}",
                        IS_OBJ_FN, variant_name, condition, bindings, result_code
                    ));
                }
            }
            TypedPattern::EnumTupleSuffix { variant_name, patterns, total_fields, .. } => {
                let result_code = codegen(&arm.result);
                let (condition, bindings) = codegen_enum_tuple_suffix_bindings(patterns, *total_fields);
                if condition == "true" {
                    parts.push(format!(
                        "if ({}($match) && $match.$tag === \"{}\") {{ {} return {}; }}",
                        IS_OBJ_FN, variant_name, bindings, result_code
                    ));
                } else {
                    parts.push(format!(
                        "if ({}($match) && $match.$tag === \"{}\" && {}) {{ {} return {}; }}",
                        IS_OBJ_FN, variant_name, condition, bindings, result_code
                    ));
                }
            }
            TypedPattern::EnumTuplePrefixSuffix { variant_name, prefix, suffix, total_fields, .. } => {
                let result_code = codegen(&arm.result);
                let (condition, bindings) = codegen_enum_tuple_prefix_suffix_bindings(prefix, suffix, *total_fields);
                if condition == "true" {
                    parts.push(format!(
                        "if ({}($match) && $match.$tag === \"{}\") {{ {} return {}; }}",
                        IS_OBJ_FN, variant_name, bindings, result_code
                    ));
                } else {
                    parts.push(format!(
                        "if ({}($match) && $match.$tag === \"{}\" && {}) {{ {} return {}; }}",
                        IS_OBJ_FN, variant_name, condition, bindings, result_code
                    ));
                }
            }
            TypedPattern::EnumStructExact { variant_name, fields, .. }
            | TypedPattern::EnumStructPartial { variant_name, fields, .. } => {
                let result_code = codegen(&arm.result);
                let (condition, bindings) = codegen_struct_pattern_bindings(fields);
                if condition == "true" {
                    parts.push(format!(
                        "if ({}($match) && $match.$tag === \"{}\") {{ {} return {}; }}",
                        IS_OBJ_FN, variant_name, bindings, result_code
                    ));
                } else {
                    parts.push(format!(
                        "if ({}($match) && $match.$tag === \"{}\" && {}) {{ {} return {}; }}",
                        IS_OBJ_FN, variant_name, condition, bindings, result_code
                    ));
                }
            }
        }
    }

    parts.push(format!("}})({})", scrutinee_code));
    parts.join(" ")
}

/// Generate condition checks and bindings for list patterns
/// Returns (condition_expr, bindings_code)
fn codegen_list_pattern_bindings(
    patterns: &[TypedPattern],
    _len: usize,
    _exact: bool,
) -> (String, String) {
    let mut conditions = Vec::new();
    let mut bindings = Vec::new();

    for (i, pat) in patterns.iter().enumerate() {
        match pat {
            TypedPattern::Literal(lit) => {
                let lit_code = codegen(lit);
                // For list literals, use deep equality
                if needs_deep_equality(&lit.ty()) {
                    conditions.push(format!("{}($match[{}], {})", DEEP_EQ_FN, i, lit_code));
                } else {
                    conditions.push(format!("$match[{}] === {}", i, lit_code));
                }
            }
            TypedPattern::Var { name, .. } => {
                bindings.push(format!("const {} = $match[{}];", name, i));
            }
            TypedPattern::Wildcard => {
                // No binding or condition needed
            }
            // Nested patterns - use recursive helper
            _ => {
                let child_path = format!("$match[{}]", i);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }
    }

    let condition = if conditions.is_empty() {
        "true".to_string()
    } else {
        conditions.join(" && ")
    };

    let bindings_code = bindings.join(" ");

    (condition, bindings_code)
}

/// Generate condition checks and bindings for suffix patterns [.., x, y]
/// Returns (condition_expr, bindings_code)
fn codegen_suffix_pattern_bindings(
    patterns: &[TypedPattern],
    min_len: usize,
) -> (String, String) {
    let mut conditions = Vec::new();
    let mut bindings = Vec::new();

    for (i, pat) in patterns.iter().enumerate() {
        // Index from end: patterns[i] is at $match.length - (min_len - i)
        // For [.., x, y] with min_len=2: x is at length-2, y is at length-1
        let offset = min_len - i;
        let index_expr = format!("$match.length - {}", offset);

        match pat {
            TypedPattern::Literal(lit) => {
                let lit_code = codegen(lit);
                if needs_deep_equality(&lit.ty()) {
                    conditions.push(format!("{}($match[{}], {})", DEEP_EQ_FN, index_expr, lit_code));
                } else {
                    conditions.push(format!("$match[{}] === {}", index_expr, lit_code));
                }
            }
            TypedPattern::Var { name, .. } => {
                bindings.push(format!("const {} = $match[{}];", name, index_expr));
            }
            TypedPattern::Wildcard => {
                // No binding or condition needed
            }
            // Nested patterns - use recursive helper
            _ => {
                let child_path = format!("$match[{}]", index_expr);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }
    }

    let condition = if conditions.is_empty() {
        "true".to_string()
    } else {
        conditions.join(" && ")
    };

    (condition, bindings.join(" "))
}

/// Generate condition checks and bindings for prefix+suffix patterns [a, .., z]
/// Returns (condition_expr, bindings_code)
fn codegen_prefix_suffix_pattern_bindings(
    prefix: &[TypedPattern],
    suffix: &[TypedPattern],
) -> (String, String) {
    let mut conditions = Vec::new();
    let mut bindings = Vec::new();

    // Prefix patterns: indexed from start
    for (i, pat) in prefix.iter().enumerate() {
        match pat {
            TypedPattern::Literal(lit) => {
                let lit_code = codegen(lit);
                if needs_deep_equality(&lit.ty()) {
                    conditions.push(format!("{}($match[{}], {})", DEEP_EQ_FN, i, lit_code));
                } else {
                    conditions.push(format!("$match[{}] === {}", i, lit_code));
                }
            }
            TypedPattern::Var { name, .. } => {
                bindings.push(format!("const {} = $match[{}];", name, i));
            }
            TypedPattern::Wildcard => {}
            // Nested patterns - use recursive helper
            _ => {
                let child_path = format!("$match[{}]", i);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }
    }

    // Suffix patterns: indexed from end
    let suffix_len = suffix.len();
    for (i, pat) in suffix.iter().enumerate() {
        let offset = suffix_len - i;
        let index_expr = format!("$match.length - {}", offset);

        match pat {
            TypedPattern::Literal(lit) => {
                let lit_code = codegen(lit);
                if needs_deep_equality(&lit.ty()) {
                    conditions.push(format!("{}($match[{}], {})", DEEP_EQ_FN, index_expr, lit_code));
                } else {
                    conditions.push(format!("$match[{}] === {}", index_expr, lit_code));
                }
            }
            TypedPattern::Var { name, .. } => {
                bindings.push(format!("const {} = $match[{}];", name, index_expr));
            }
            TypedPattern::Wildcard => {}
            // Nested patterns - use recursive helper
            _ => {
                let child_path = format!("$match[{}]", index_expr);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }
    }

    let condition = if conditions.is_empty() {
        "true".to_string()
    } else {
        conditions.join(" && ")
    };

    (condition, bindings.join(" "))
}

/// Generate condition checks and bindings for tuple patterns (a, b, c)
/// Returns (condition_expr, bindings_code)
fn codegen_tuple_pattern_bindings(
    patterns: &[TypedPattern],
    _total_len: usize,
) -> (String, String) {
    let mut conditions = Vec::new();
    let mut bindings = Vec::new();

    for (i, pat) in patterns.iter().enumerate() {
        match pat {
            TypedPattern::Literal(lit) => {
                let lit_code = codegen(lit);
                if needs_deep_equality(&lit.ty()) {
                    conditions.push(format!("{}($match[{}], {})", DEEP_EQ_FN, i, lit_code));
                } else {
                    conditions.push(format!("$match[{}] === {}", i, lit_code));
                }
            }
            TypedPattern::Var { name, .. } => {
                bindings.push(format!("const {} = $match[{}];", name, i));
            }
            TypedPattern::Wildcard => {}
            // Nested patterns - use recursive helper
            _ => {
                let child_path = format!("$match[{}]", i);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }
    }

    let condition = if conditions.is_empty() {
        "true".to_string()
    } else {
        conditions.join(" && ")
    };

    (condition, bindings.join(" "))
}

/// Generate condition checks and bindings for tuple suffix patterns (.., y, z)
/// Returns (condition_expr, bindings_code)
fn codegen_tuple_suffix_bindings(
    patterns: &[TypedPattern],
    total_len: usize,
) -> (String, String) {
    let mut conditions = Vec::new();
    let mut bindings = Vec::new();

    // For tuples, we know the exact size, so we can compute indices directly
    let start_idx = total_len - patterns.len();
    for (i, pat) in patterns.iter().enumerate() {
        let idx = start_idx + i;
        match pat {
            TypedPattern::Literal(lit) => {
                let lit_code = codegen(lit);
                if needs_deep_equality(&lit.ty()) {
                    conditions.push(format!("{}($match[{}], {})", DEEP_EQ_FN, idx, lit_code));
                } else {
                    conditions.push(format!("$match[{}] === {}", idx, lit_code));
                }
            }
            TypedPattern::Var { name, .. } => {
                bindings.push(format!("const {} = $match[{}];", name, idx));
            }
            TypedPattern::Wildcard => {}
            // Nested patterns - use recursive helper
            _ => {
                let child_path = format!("$match[{}]", idx);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }
    }

    let condition = if conditions.is_empty() {
        "true".to_string()
    } else {
        conditions.join(" && ")
    };

    (condition, bindings.join(" "))
}

/// Generate condition checks and bindings for tuple prefix+suffix patterns (a, .., z)
/// Returns (condition_expr, bindings_code)
fn codegen_tuple_prefix_suffix_bindings(
    prefix: &[TypedPattern],
    suffix: &[TypedPattern],
    total_len: usize,
) -> (String, String) {
    let mut conditions = Vec::new();
    let mut bindings = Vec::new();

    // Prefix patterns: indexed from start
    for (i, pat) in prefix.iter().enumerate() {
        match pat {
            TypedPattern::Literal(lit) => {
                let lit_code = codegen(lit);
                if needs_deep_equality(&lit.ty()) {
                    conditions.push(format!("{}($match[{}], {})", DEEP_EQ_FN, i, lit_code));
                } else {
                    conditions.push(format!("$match[{}] === {}", i, lit_code));
                }
            }
            TypedPattern::Var { name, .. } => {
                bindings.push(format!("const {} = $match[{}];", name, i));
            }
            TypedPattern::Wildcard => {}
            // Nested patterns - use recursive helper
            _ => {
                let child_path = format!("$match[{}]", i);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }
    }

    // Suffix patterns: indexed from end (known position since tuple has fixed size)
    let suffix_start = total_len - suffix.len();
    for (i, pat) in suffix.iter().enumerate() {
        let idx = suffix_start + i;
        match pat {
            TypedPattern::Literal(lit) => {
                let lit_code = codegen(lit);
                if needs_deep_equality(&lit.ty()) {
                    conditions.push(format!("{}($match[{}], {})", DEEP_EQ_FN, idx, lit_code));
                } else {
                    conditions.push(format!("$match[{}] === {}", idx, lit_code));
                }
            }
            TypedPattern::Var { name, .. } => {
                bindings.push(format!("const {} = $match[{}];", name, idx));
            }
            TypedPattern::Wildcard => {}
            // Nested patterns - use recursive helper
            _ => {
                let child_path = format!("$match[{}]", idx);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }
    }

    let condition = if conditions.is_empty() {
        "true".to_string()
    } else {
        conditions.join(" && ")
    };

    (condition, bindings.join(" "))
}

/// Generate condition checks and bindings for struct patterns
/// Returns (condition_expr, bindings_code)
fn codegen_struct_pattern_bindings(
    fields: &[(String, TypedPattern)],
) -> (String, String) {
    let mut conditions = Vec::new();
    let mut bindings = Vec::new();

    for (field_name, pat) in fields {
        match pat {
            TypedPattern::Literal(lit) => {
                let lit_code = codegen(lit);
                if needs_deep_equality(&lit.ty()) {
                    conditions.push(format!(
                        "{}($match.{}, {})",
                        DEEP_EQ_FN, field_name, lit_code
                    ));
                } else {
                    conditions.push(format!("$match.{} === {}", field_name, lit_code));
                }
            }
            TypedPattern::Var { name, .. } => {
                bindings.push(format!("const {} = $match.{};", name, field_name));
            }
            TypedPattern::Wildcard => {}
            // Nested patterns - use recursive helper
            _ => {
                let child_path = format!("$match.{}", field_name);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }
    }

    let condition = if conditions.is_empty() {
        "true".to_string()
    } else {
        conditions.join(" && ")
    };

    (condition, bindings.join(" "))
}

/// Generate condition checks and bindings for enum tuple patterns (prefix, including exact)
/// Returns (condition_expr, bindings_code)
fn codegen_enum_tuple_pattern_bindings(
    patterns: &[TypedPattern],
) -> (String, String) {
    let mut conditions = Vec::new();
    let mut bindings = Vec::new();

    for (i, pat) in patterns.iter().enumerate() {
        match pat {
            TypedPattern::Literal(lit) => {
                let lit_code = codegen(lit);
                if needs_deep_equality(&lit.ty()) {
                    conditions.push(format!("{}($match.${}, {})", DEEP_EQ_FN, i, lit_code));
                } else {
                    conditions.push(format!("$match.${} === {}", i, lit_code));
                }
            }
            TypedPattern::Var { name, .. } => {
                bindings.push(format!("const {} = $match.${};", name, i));
            }
            TypedPattern::Wildcard => {}
            // Nested patterns - use recursive helper
            _ => {
                let child_path = format!("$match.${}", i);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }
    }

    let condition = if conditions.is_empty() {
        "true".to_string()
    } else {
        conditions.join(" && ")
    };

    (condition, bindings.join(" "))
}

/// Generate condition checks and bindings for enum tuple suffix patterns (.., y, z)
/// Returns (condition_expr, bindings_code)
fn codegen_enum_tuple_suffix_bindings(
    patterns: &[TypedPattern],
    total_fields: usize,
) -> (String, String) {
    let mut conditions = Vec::new();
    let mut bindings = Vec::new();

    // For enum tuples, we know the exact field count, so compute indices directly
    let start_idx = total_fields - patterns.len();
    for (i, pat) in patterns.iter().enumerate() {
        let idx = start_idx + i;
        match pat {
            TypedPattern::Literal(lit) => {
                let lit_code = codegen(lit);
                if needs_deep_equality(&lit.ty()) {
                    conditions.push(format!("{}($match.${}, {})", DEEP_EQ_FN, idx, lit_code));
                } else {
                    conditions.push(format!("$match.${} === {}", idx, lit_code));
                }
            }
            TypedPattern::Var { name, .. } => {
                bindings.push(format!("const {} = $match.${};", name, idx));
            }
            TypedPattern::Wildcard => {}
            // Nested patterns - use recursive helper
            _ => {
                let child_path = format!("$match.${}", idx);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }
    }

    let condition = if conditions.is_empty() {
        "true".to_string()
    } else {
        conditions.join(" && ")
    };

    (condition, bindings.join(" "))
}

/// Generate condition checks and bindings for enum tuple prefix+suffix patterns (a, .., z)
/// Returns (condition_expr, bindings_code)
fn codegen_enum_tuple_prefix_suffix_bindings(
    prefix: &[TypedPattern],
    suffix: &[TypedPattern],
    total_fields: usize,
) -> (String, String) {
    let mut conditions = Vec::new();
    let mut bindings = Vec::new();

    // Prefix patterns: indexed from start
    for (i, pat) in prefix.iter().enumerate() {
        match pat {
            TypedPattern::Literal(lit) => {
                let lit_code = codegen(lit);
                if needs_deep_equality(&lit.ty()) {
                    conditions.push(format!("{}($match.${}, {})", DEEP_EQ_FN, i, lit_code));
                } else {
                    conditions.push(format!("$match.${} === {}", i, lit_code));
                }
            }
            TypedPattern::Var { name, .. } => {
                bindings.push(format!("const {} = $match.${};", name, i));
            }
            TypedPattern::Wildcard => {}
            // Nested patterns - use recursive helper
            _ => {
                let child_path = format!("$match.${}", i);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }
    }

    // Suffix patterns: indexed from end (known position since variant has fixed fields)
    let suffix_start = total_fields - suffix.len();
    for (i, pat) in suffix.iter().enumerate() {
        let idx = suffix_start + i;
        match pat {
            TypedPattern::Literal(lit) => {
                let lit_code = codegen(lit);
                if needs_deep_equality(&lit.ty()) {
                    conditions.push(format!("{}($match.${}, {})", DEEP_EQ_FN, idx, lit_code));
                } else {
                    conditions.push(format!("$match.${} === {}", idx, lit_code));
                }
            }
            TypedPattern::Var { name, .. } => {
                bindings.push(format!("const {} = $match.${};", name, idx));
            }
            TypedPattern::Wildcard => {}
            // Nested patterns - use recursive helper
            _ => {
                let child_path = format!("$match.${}", idx);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }
    }

    let condition = if conditions.is_empty() {
        "true".to_string()
    } else {
        conditions.join(" && ")
    };

    (condition, bindings.join(" "))
}

/// Wrap an Int32 expression with overflow checking
fn wrap_int32_overflow(expr: &str) -> String {
    // Check for non-finite (Infinity/NaN from division by zero) first,
    // then check for overflow
    format!(
        "(function(r){{if(!Number.isFinite(r))throw new Error(\"division by zero\");if(r>2147483647||r<-2147483648)throw new Error(\"Int32 overflow\");return r;}})({})",
        expr
    )
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

    fn int32_wrap(expr: &str) -> String {
        format!(
            "(function(r){{if(!Number.isFinite(r))throw new Error(\"division by zero\");if(r>2147483647||r<-2147483648)throw new Error(\"Int32 overflow\");return r;}})({})",
            expr
        )
    }

    #[test]
    fn test_codegen_int32() {
        let expr = TypedExpr::Int32(42);
        assert_eq!(codegen(&expr), "42");
    }

    #[test]
    fn test_codegen_negative_int32() {
        let expr = TypedExpr::Int32(-42);
        assert_eq!(codegen(&expr), "-42");
    }

    #[test]
    fn test_codegen_int64() {
        let expr = TypedExpr::Int64(42);
        assert_eq!(codegen(&expr), "42n");
    }

    #[test]
    fn test_codegen_int64_large() {
        let expr = TypedExpr::Int64(9_000_000_000);
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
    fn test_codegen_unary_neg_int32() {
        let expr = TypedExpr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(TypedExpr::Int32(42)),
            ty: Type::Int32,
        };
        // Int32 gets overflow wrapped
        assert_eq!(codegen(&expr), int32_wrap("(-(42))"));
    }

    #[test]
    fn test_codegen_unary_neg_int64() {
        let expr = TypedExpr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(TypedExpr::Int64(42)),
            ty: Type::Int64,
        };
        // Int64 does not get overflow wrapped
        assert_eq!(codegen(&expr), "(-(42n))");
    }

    #[test]
    fn test_codegen_addition_int32() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Int32(1)),
            right: Box::new(TypedExpr::Int32(2)),
            ty: Type::Int32,
        };
        assert_eq!(codegen(&expr), int32_wrap("((1) + (2))"));
    }

    #[test]
    fn test_codegen_addition_int64() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Int64(1)),
            right: Box::new(TypedExpr::Int64(2)),
            ty: Type::Int64,
        };
        // Int64 (BigInt) does not get overflow wrapped
        assert_eq!(codegen(&expr), "((1n) + (2n))");
    }

    #[test]
    fn test_codegen_subtraction() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Sub,
            left: Box::new(TypedExpr::Int32(5)),
            right: Box::new(TypedExpr::Int32(3)),
            ty: Type::Int32,
        };
        assert_eq!(codegen(&expr), int32_wrap("((5) - (3))"));
    }

    #[test]
    fn test_codegen_multiplication() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Mul,
            left: Box::new(TypedExpr::Int32(3)),
            right: Box::new(TypedExpr::Int32(4)),
            ty: Type::Int32,
        };
        assert_eq!(codegen(&expr), int32_wrap("((3) * (4))"));
    }

    #[test]
    fn test_codegen_division() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Div,
            left: Box::new(TypedExpr::Int32(10)),
            right: Box::new(TypedExpr::Int32(2)),
            ty: Type::Int32,
        };
        assert_eq!(codegen(&expr), int32_wrap("((10) / (2))"));
    }

    #[test]
    fn test_codegen_complex_expression() {
        // 2 + 3 * 4 - nested Int32 ops each get wrapped
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Int32(2)),
            right: Box::new(TypedExpr::BinOp {
                op: BinOp::Mul,
                left: Box::new(TypedExpr::Int32(3)),
                right: Box::new(TypedExpr::Int32(4)),
                ty: Type::Int32,
            }),
            ty: Type::Int32,
        };
        let inner = int32_wrap("((3) * (4))");
        let expected = int32_wrap(&format!("((2) + ({}))", inner));
        assert_eq!(codegen(&expr), expected);
    }

    #[test]
    fn test_codegen_float_expression() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Float(1.5)),
            right: Box::new(TypedExpr::Float(2.5)),
            ty: Type::Float,
        };
        // Float does not get overflow wrapped
        assert_eq!(codegen(&expr), "((1.5) + (2.5))");
    }

    #[test]
    fn test_codegen_var() {
        let expr = TypedExpr::Var {
            name: "x".to_string(),
            ty: Type::Int32,
        };
        assert_eq!(codegen(&expr), "x");
    }

    #[test]
    fn test_codegen_call_no_args() {
        let expr = TypedExpr::Call {
            func: "foo".to_string(),
            args: vec![],
            ty: Type::Int32,
        };
        // Int32 function calls get overflow wrapped
        assert_eq!(codegen(&expr), int32_wrap("foo()"));
    }

    #[test]
    fn test_codegen_call_one_arg() {
        let expr = TypedExpr::Call {
            func: "square".to_string(),
            args: vec![TypedExpr::Int32(5)],
            ty: Type::Int32,
        };
        assert_eq!(codegen(&expr), int32_wrap("square(5)"));
    }

    #[test]
    fn test_codegen_call_multiple_args() {
        let expr = TypedExpr::Call {
            func: "add".to_string(),
            args: vec![TypedExpr::Int32(1), TypedExpr::Int32(2)],
            ty: Type::Int32,
        };
        assert_eq!(codegen(&expr), int32_wrap("add(1, 2)"));
    }

    #[test]
    fn test_codegen_call_with_vars() {
        let expr = TypedExpr::Call {
            func: "add".to_string(),
            args: vec![
                TypedExpr::Var {
                    name: "x".to_string(),
                    ty: Type::Int32,
                },
                TypedExpr::Var {
                    name: "y".to_string(),
                    ty: Type::Int32,
                },
            ],
            ty: Type::Int32,
        };
        assert_eq!(codegen(&expr), int32_wrap("add(x, y)"));
    }

    #[test]
    fn test_codegen_var_in_expression() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Var {
                name: "x".to_string(),
                ty: Type::Int32,
            }),
            right: Box::new(TypedExpr::Int32(1)),
            ty: Type::Int32,
        };
        assert_eq!(codegen(&expr), int32_wrap("((x) + (1))"));
    }

    #[test]
    fn test_codegen_function() {
        let func = TypedFunction {
            name: "square".to_string(),
            params: vec![("x".to_string(), Type::Int32)],
            body: TypedExpr::BinOp {
                op: BinOp::Mul,
                left: Box::new(TypedExpr::Var {
                    name: "x".to_string(),
                    ty: Type::Int32,
                }),
                right: Box::new(TypedExpr::Var {
                    name: "x".to_string(),
                    ty: Type::Int32,
                }),
                ty: Type::Int32,
            },
            return_type: Type::Int32,
        };
        let body = int32_wrap("((x) * (x))");
        assert_eq!(
            codegen_function(&func),
            format!("function square(x) {{ return {}; }}", body)
        );
    }

    #[test]
    fn test_codegen_function_multiple_params() {
        let func = TypedFunction {
            name: "add".to_string(),
            params: vec![
                ("x".to_string(), Type::Int32),
                ("y".to_string(), Type::Int32),
            ],
            body: TypedExpr::BinOp {
                op: BinOp::Add,
                left: Box::new(TypedExpr::Var {
                    name: "x".to_string(),
                    ty: Type::Int32,
                }),
                right: Box::new(TypedExpr::Var {
                    name: "y".to_string(),
                    ty: Type::Int32,
                }),
                ty: Type::Int32,
            },
            return_type: Type::Int32,
        };
        let body = int32_wrap("((x) + (y))");
        assert_eq!(
            codegen_function(&func),
            format!("function add(x, y) {{ return {}; }}", body)
        );
    }

    #[test]
    fn test_codegen_function_no_params() {
        let func = TypedFunction {
            name: "answer".to_string(),
            params: vec![],
            body: TypedExpr::Int32(42),
            return_type: Type::Int32,
        };
        assert_eq!(
            codegen_function(&func),
            "function answer() { return 42; }"
        );
    }

    #[test]
    fn test_codegen_int64_function() {
        let func = TypedFunction {
            name: "big".to_string(),
            params: vec![("x".to_string(), Type::Int64)],
            body: TypedExpr::BinOp {
                op: BinOp::Add,
                left: Box::new(TypedExpr::Var {
                    name: "x".to_string(),
                    ty: Type::Int64,
                }),
                right: Box::new(TypedExpr::Int64(1)),
                ty: Type::Int64,
            },
            return_type: Type::Int64,
        };
        // Int64 does not get overflow wrapped
        assert_eq!(
            codegen_function(&func),
            "function big(x) { return ((x) + (1n)); }"
        );
    }
}
