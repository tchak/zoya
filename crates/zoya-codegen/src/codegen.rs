use zoya_ast::{BinOp, UnaryOp};
use zoya_ir::{
    CheckedPackage, QualifiedPath, Type, TypedEnumConstructFields, TypedExpr, TypedFunction,
    TypedMatchArm, TypedPattern,
};

/// Output of code generation containing JS code and content hash
#[derive(Debug, Clone)]
pub struct CodegenOutput {
    /// Generated JavaScript code
    pub code: String,
    /// Blake3 hash of the code as hex string (64 chars)
    pub hash: String,
}

/// Deep equality function name used in generated JS
const DEEP_EQ_FN: &str = "$$eq";

/// Plain object check function name used in generated JS
const IS_OBJ_FN: &str = "$$is_obj";

/// Division by zero check function name used in generated JS
const DIV_CHECK_FN: &str = "$$div";

/// BigInt division by zero check function name used in generated JS
const DIV_BIGINT_CHECK_FN: &str = "$$div_bigint";

/// BigInt absolute value function name used in generated JS
const ABS_BIGINT_FN: &str = "$$abs_bigint";

/// BigInt minimum function name used in generated JS
const MIN_BIGINT_FN: &str = "$$min_bigint";

/// BigInt maximum function name used in generated JS
const MAX_BIGINT_FN: &str = "$$max_bigint";

/// List index function name used in generated JS
const LIST_IDX_FN: &str = "$$list_idx";

/// Prelude containing helper functions for generated JS
fn prelude() -> &'static str {
    r#"function $$is_obj(x) {
  return typeof x === 'object' && x !== null && !Array.isArray(x);
}
function $$div(a, b) {
  if (b === 0) throw new Error("division by zero");
  return Math.trunc(a / b);
}
function $$div_bigint(a, b) {
  if (b === 0n) throw new Error("division by zero");
  return a / b;
}
function $$abs_bigint(x) { return x < 0n ? -x : x; }
function $$min_bigint(a, b) { return a < b ? a : b; }
function $$max_bigint(a, b) { return a > b ? a : b; }
function $$eq(a, b) {
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) {
      if (!$$eq(a[i], b[i])) return false;
    }
    return true;
  }
  if ($$is_obj(a) && $$is_obj(b)) {
    const ka = Object.keys(a), kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    for (let k of ka) {
      if (!$$eq(a[k], b[k])) return false;
    }
    return true;
  }
  return a === b;
}
function $$list_idx(arr, i) {
  const idx = i < 0 ? arr.length + i : i;
  return idx >= 0 && idx < arr.length ? { $tag: "Some", $0: arr[idx] } : { $tag: "None" };
}
function $$json_to_zoya(v) {
  if (v === null) return { $tag: "Null" };
  if (typeof v === "boolean") return { $tag: "Bool", $0: v };
  if (typeof v === "number") return Number.isInteger(v)
    ? { $tag: "Number", $0: { $tag: "Int", $0: v } }
    : { $tag: "Number", $0: { $tag: "Float", $0: v } };
  if (typeof v === "string") return { $tag: "String", $0: v };
  if (Array.isArray(v)) return { $tag: "Array", $0: v.map($$json_to_zoya) };
  return { $tag: "Object", $0: Object.entries(v).map(([k, val]) => [k, $$json_to_zoya(val)]) };
}"#
}

/// Generate JavaScript code for all items in the checked package.
/// Processes items in dependency order (parents before children).
/// Includes the prelude (runtime helper functions) at the start.
/// Returns a `CodegenOutput` containing the generated code and its Blake3 hash.
pub fn codegen(pkg: &CheckedPackage) -> CodegenOutput {
    let mut js = String::new();

    // Include prelude at the start
    js.push_str(prelude());
    js.push('\n');

    // Generate import statements for dependency functions
    let mut dep_names: Vec<&String> = pkg.imports.keys().collect();
    dep_names.sort();
    for dep_name in dep_names {
        let paths = &pkg.imports[dep_name];
        let mut import_names: Vec<String> = paths.iter().map(format_path).collect();
        import_names.sort();
        if !import_names.is_empty() {
            js.push_str(&format!(
                "import {{ {} }} from '{}';\n",
                import_names.join(", "),
                dep_name
            ));
        }
    }

    // Sort items by path depth (parents before children)
    let mut item_paths: Vec<_> = pkg.items.keys().collect();
    item_paths.sort_by_key(|p| p.depth());

    for path in item_paths {
        if let Some(func) = pkg.items.get(path) {
            js.push_str(&codegen_function(func, path));
            js.push('\n');
        }
    }

    // Build consolidated export block with only public definitions and reexports
    let mut exports: Vec<String> = Vec::new();

    // Export public definitions (functions that appear in both definitions and items)
    for path in pkg.definitions.keys() {
        if pkg.definitions[path].as_function().is_some() && pkg.items.contains_key(path) {
            push_export(
                &mut exports,
                format_path(path),
                format_export_path(path, &pkg.name),
            );
        }
    }

    // Export reexports (re-exported functions that exist in items)
    for (reexport_path, original_path) in &pkg.reexports {
        if pkg.items.contains_key(original_path) {
            push_export(
                &mut exports,
                format_path(original_path),
                format_export_path(reexport_path, &pkg.name),
            );
        }
    }

    // Sort for deterministic output
    exports.sort();

    if !exports.is_empty() {
        js.push_str(&format!("export {{ {} }};\n", exports.join(", ")));
    }

    let hash = blake3::hash(js.as_bytes()).to_hex().to_string();

    CodegenOutput { code: js, hash }
}

/// Check if an expression doesn't need wrapping in parens when used as an operand.
/// True for simple atoms and expressions that already produce parenthesized output.
fn is_safe_operand(expr: &TypedExpr) -> bool {
    matches!(
        expr,
        TypedExpr::Int(_)
            | TypedExpr::BigInt(_)
            | TypedExpr::Float(_)
            | TypedExpr::Bool(_)
            | TypedExpr::String(_)
            | TypedExpr::Var { .. }
            | TypedExpr::Call { .. }
            | TypedExpr::List { .. }
            | TypedExpr::Tuple { .. }
            | TypedExpr::BinOp { .. }
            | TypedExpr::UnaryOp { .. }
    )
}

/// Generate JS for an expression, wrapping in parens only if needed for operator safety.
fn codegen_operand(expr: &TypedExpr) -> String {
    let code = codegen_expr(expr);
    if is_safe_operand(expr) {
        code
    } else {
        format!("({})", code)
    }
}

/// Check if a type requires deep equality comparison
fn needs_deep_equality(ty: &Type) -> bool {
    matches!(
        ty,
        Type::List(_) | Type::Tuple(_) | Type::Struct { .. } | Type::Enum { .. }
    )
}

/// Format a qualified path as a JS identifier: Option::Some -> $Option$Some
fn format_path(path: &QualifiedPath) -> String {
    format!("${}", path.segments().join("$"))
}

/// Format a qualified path as a JS export name, replacing the "root" prefix with the package name.
/// e.g., root::main with pkg_name "myapp" -> $myapp$main
fn format_export_path(path: &QualifiedPath, pkg_name: &str) -> String {
    let segments = path.segments();
    let renamed: Vec<&str> = std::iter::once(pkg_name)
        .chain(segments[1..].iter().map(|s| s.as_str()))
        .collect();
    format!("${}", renamed.join("$"))
}

/// Push an export entry, using `as` syntax only when internal and external names differ.
fn push_export(exports: &mut Vec<String>, internal: String, external: String) {
    if internal == external {
        exports.push(internal);
    } else {
        exports.push(format!("{} as {}", internal, external));
    }
}

/// Format a simple name as a JS identifier: x -> $x
fn format_name(name: &str) -> String {
    format!("${}", name)
}

/// Format an array index path: `path[idx]`
fn array_index(access_path: &str, idx: usize) -> String {
    format!("{}[{}]", access_path, idx)
}

/// Format an enum field path: `path.$idx`
fn enum_field(access_path: &str, idx: usize) -> String {
    format!("{}.${}", access_path, idx)
}

/// Generate conditions and bindings for a sequence of patterns at indexed positions.
fn codegen_indexed_patterns(
    patterns: &[TypedPattern],
    access_path: &str,
    start_idx: usize,
    make_child_path: fn(&str, usize) -> String,
    conditions: &mut Vec<String>,
    bindings: &mut Vec<String>,
) {
    for (i, pat) in patterns.iter().enumerate() {
        let child_path = make_child_path(access_path, start_idx + i);
        let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
        conditions.extend(child_conds);
        bindings.extend(child_binds);
    }
}

/// Generate conditions and bindings for suffix patterns indexed from the end of a list.
fn codegen_suffix_from_end_patterns(
    patterns: &[TypedPattern],
    access_path: &str,
    suffix_len: usize,
    conditions: &mut Vec<String>,
    bindings: &mut Vec<String>,
) {
    for (i, pat) in patterns.iter().enumerate() {
        let offset = suffix_len - i;
        let indexed_path = format!("{}[{}.length - {}]", access_path, access_path, offset);
        let (child_conds, child_binds) = codegen_pattern_at_path(pat, &indexed_path);
        conditions.extend(child_conds);
        bindings.extend(child_binds);
    }
}

/// Generate a rest binding for a tuple by enumerating known indices.
fn tuple_rest_binding(
    name: &str,
    access_path: &str,
    range: std::ops::Range<usize>,
    bindings: &mut Vec<String>,
) {
    let rest_indices: Vec<String> = range.map(|i| format!("{}[{}]", access_path, i)).collect();
    bindings.push(format!(
        "const {} = [{}];",
        format_name(name),
        rest_indices.join(", ")
    ));
}

/// Generate a rest binding for object-field-based patterns (tuple structs).
/// Similar to tuple_rest_binding but accesses `$0`, `$1` fields instead of array indices.
fn obj_field_rest_binding(
    name: &str,
    access_path: &str,
    range: std::ops::Range<usize>,
    bindings: &mut Vec<String>,
) {
    let rest_fields: Vec<String> = range.map(|i| format!("{}.${}", access_path, i)).collect();
    bindings.push(format!(
        "const {} = [{}];",
        format_name(name),
        rest_fields.join(", ")
    ));
}

/// Generate a JS condition checking an array's length.
fn array_length_condition(access_path: &str, op: &str, len: usize) -> String {
    format!(
        "Array.isArray({}) && {}.length {} {}",
        access_path, access_path, op, len
    )
}

/// Generate a JS condition checking an enum variant tag.
fn enum_tag_condition(access_path: &str, path: &QualifiedPath) -> String {
    format!(
        "{}({}) && {}.$tag === \"{}\"",
        IS_OBJ_FN,
        access_path,
        access_path,
        path.last()
    )
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
            let lit_code = codegen_expr(lit);
            if needs_deep_equality(&lit.ty()) {
                conditions.push(format!("{}({}, {})", DEEP_EQ_FN, access_path, lit_code));
            } else {
                conditions.push(format!("{} === {}", access_path, lit_code));
            }
        }

        TypedPattern::Var { name, .. } => {
            bindings.push(format!("const {} = {};", format_name(name), access_path));
        }

        TypedPattern::Wildcard => {
            // No conditions or bindings needed
        }

        TypedPattern::As { name, pattern, .. } => {
            // Bind the entire value to name
            bindings.push(format!("const {} = {};", format_name(name), access_path));
            // Recursively handle inner pattern (conditions and additional bindings)
            let (inner_conds, inner_binds) = codegen_pattern_at_path(pattern, access_path);
            conditions.extend(inner_conds);
            bindings.extend(inner_binds);
        }

        TypedPattern::ListEmpty | TypedPattern::TupleEmpty => {
            conditions.push(array_length_condition(access_path, "===", 0));
        }

        TypedPattern::ListExact { patterns, len } | TypedPattern::TupleExact { patterns, len } => {
            conditions.push(array_length_condition(access_path, "===", *len));
            codegen_indexed_patterns(
                patterns,
                access_path,
                0,
                array_index,
                &mut conditions,
                &mut bindings,
            );
        }

        TypedPattern::ListPrefix {
            patterns,
            rest_binding,
            min_len,
        } => {
            conditions.push(array_length_condition(access_path, ">=", *min_len));
            codegen_indexed_patterns(
                patterns,
                access_path,
                0,
                array_index,
                &mut conditions,
                &mut bindings,
            );
            if let Some((name, _)) = rest_binding {
                bindings.push(format!(
                    "const {} = {}.slice({});",
                    format_name(name),
                    access_path,
                    patterns.len()
                ));
            }
        }

        TypedPattern::ListSuffix {
            patterns,
            rest_binding,
            min_len,
        } => {
            conditions.push(array_length_condition(access_path, ">=", *min_len));
            codegen_suffix_from_end_patterns(
                patterns,
                access_path,
                *min_len,
                &mut conditions,
                &mut bindings,
            );
            if let Some((name, _)) = rest_binding {
                bindings.push(format!(
                    "const {} = {}.slice(0, {}.length - {});",
                    format_name(name),
                    access_path,
                    access_path,
                    patterns.len()
                ));
            }
        }

        TypedPattern::ListPrefixSuffix {
            prefix,
            suffix,
            rest_binding,
            min_len,
        } => {
            conditions.push(array_length_condition(access_path, ">=", *min_len));
            codegen_indexed_patterns(
                prefix,
                access_path,
                0,
                array_index,
                &mut conditions,
                &mut bindings,
            );
            codegen_suffix_from_end_patterns(
                suffix,
                access_path,
                suffix.len(),
                &mut conditions,
                &mut bindings,
            );
            if let Some((name, _)) = rest_binding {
                bindings.push(format!(
                    "const {} = {}.slice({}, {}.length - {});",
                    format_name(name),
                    access_path,
                    prefix.len(),
                    access_path,
                    suffix.len()
                ));
            }
        }

        TypedPattern::TuplePrefix {
            patterns,
            rest_binding,
            total_len,
        } => {
            conditions.push(array_length_condition(access_path, "===", *total_len));
            codegen_indexed_patterns(
                patterns,
                access_path,
                0,
                array_index,
                &mut conditions,
                &mut bindings,
            );
            if let Some((name, _)) = rest_binding {
                tuple_rest_binding(name, access_path, patterns.len()..*total_len, &mut bindings);
            }
        }

        TypedPattern::TupleSuffix {
            patterns,
            rest_binding,
            total_len,
        } => {
            conditions.push(array_length_condition(access_path, "===", *total_len));
            let start_idx = total_len - patterns.len();
            codegen_indexed_patterns(
                patterns,
                access_path,
                start_idx,
                array_index,
                &mut conditions,
                &mut bindings,
            );
            if let Some((name, _)) = rest_binding {
                tuple_rest_binding(name, access_path, 0..start_idx, &mut bindings);
            }
        }

        TypedPattern::TuplePrefixSuffix {
            prefix,
            suffix,
            rest_binding,
            total_len,
        } => {
            conditions.push(array_length_condition(access_path, "===", *total_len));
            let suffix_start = total_len - suffix.len();
            codegen_indexed_patterns(
                prefix,
                access_path,
                0,
                array_index,
                &mut conditions,
                &mut bindings,
            );
            codegen_indexed_patterns(
                suffix,
                access_path,
                suffix_start,
                array_index,
                &mut conditions,
                &mut bindings,
            );
            if let Some((name, _)) = rest_binding {
                tuple_rest_binding(name, access_path, prefix.len()..suffix_start, &mut bindings);
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
            conditions.push(enum_tag_condition(access_path, path));
        }

        TypedPattern::EnumTupleExact { path, patterns, .. } => {
            conditions.push(enum_tag_condition(access_path, path));
            codegen_indexed_patterns(
                patterns,
                access_path,
                0,
                enum_field,
                &mut conditions,
                &mut bindings,
            );
        }

        TypedPattern::EnumTuplePrefix {
            path,
            patterns,
            rest_binding,
            total_fields,
        } => {
            conditions.push(enum_tag_condition(access_path, path));
            codegen_indexed_patterns(
                patterns,
                access_path,
                0,
                enum_field,
                &mut conditions,
                &mut bindings,
            );
            if let Some((name, _)) = rest_binding {
                obj_field_rest_binding(
                    name,
                    access_path,
                    patterns.len()..*total_fields,
                    &mut bindings,
                );
            }
        }

        TypedPattern::EnumTupleSuffix {
            path,
            patterns,
            rest_binding,
            total_fields,
        } => {
            conditions.push(enum_tag_condition(access_path, path));
            let start_idx = total_fields - patterns.len();
            codegen_indexed_patterns(
                patterns,
                access_path,
                start_idx,
                enum_field,
                &mut conditions,
                &mut bindings,
            );
            if let Some((name, _)) = rest_binding {
                obj_field_rest_binding(name, access_path, 0..start_idx, &mut bindings);
            }
        }

        TypedPattern::EnumTuplePrefixSuffix {
            path,
            prefix,
            suffix,
            rest_binding,
            total_fields,
        } => {
            conditions.push(enum_tag_condition(access_path, path));
            codegen_indexed_patterns(
                prefix,
                access_path,
                0,
                enum_field,
                &mut conditions,
                &mut bindings,
            );
            let suffix_start = total_fields - suffix.len();
            codegen_indexed_patterns(
                suffix,
                access_path,
                suffix_start,
                enum_field,
                &mut conditions,
                &mut bindings,
            );
            if let Some((name, _)) = rest_binding {
                obj_field_rest_binding(
                    name,
                    access_path,
                    prefix.len()..suffix_start,
                    &mut bindings,
                );
            }
        }

        TypedPattern::EnumStructExact { path, fields }
        | TypedPattern::EnumStructPartial { path, fields } => {
            conditions.push(enum_tag_condition(access_path, path));
            for (field_name, pat) in fields {
                let child_path = format!("{}.{}", access_path, field_name);
                let (child_conds, child_binds) = codegen_pattern_at_path(pat, &child_path);
                conditions.extend(child_conds);
                bindings.extend(child_binds);
            }
        }

        // Tuple struct patterns
        TypedPattern::StructTupleExact { patterns, .. } => {
            conditions.push(format!("{}({})", IS_OBJ_FN, access_path));
            codegen_indexed_patterns(
                patterns,
                access_path,
                0,
                enum_field,
                &mut conditions,
                &mut bindings,
            );
        }

        TypedPattern::StructTuplePrefix {
            patterns,
            rest_binding,
            total_fields,
            ..
        } => {
            conditions.push(format!("{}({})", IS_OBJ_FN, access_path));
            codegen_indexed_patterns(
                patterns,
                access_path,
                0,
                enum_field,
                &mut conditions,
                &mut bindings,
            );
            if let Some((name, _)) = rest_binding {
                obj_field_rest_binding(
                    name,
                    access_path,
                    patterns.len()..*total_fields,
                    &mut bindings,
                );
            }
        }

        TypedPattern::StructTupleSuffix {
            patterns,
            rest_binding,
            total_fields,
            ..
        } => {
            conditions.push(format!("{}({})", IS_OBJ_FN, access_path));
            let start_idx = total_fields - patterns.len();
            codegen_indexed_patterns(
                patterns,
                access_path,
                start_idx,
                enum_field,
                &mut conditions,
                &mut bindings,
            );
            if let Some((name, _)) = rest_binding {
                obj_field_rest_binding(name, access_path, 0..start_idx, &mut bindings);
            }
        }

        TypedPattern::StructTuplePrefixSuffix {
            prefix,
            suffix,
            rest_binding,
            total_fields,
            ..
        } => {
            conditions.push(format!("{}({})", IS_OBJ_FN, access_path));
            codegen_indexed_patterns(
                prefix,
                access_path,
                0,
                enum_field,
                &mut conditions,
                &mut bindings,
            );
            let suffix_start = total_fields - suffix.len();
            codegen_indexed_patterns(
                suffix,
                access_path,
                suffix_start,
                enum_field,
                &mut conditions,
                &mut bindings,
            );
            if let Some((name, _)) = rest_binding {
                obj_field_rest_binding(
                    name,
                    access_path,
                    prefix.len()..suffix_start,
                    &mut bindings,
                );
            }
        }
    }

    (conditions, bindings)
}

fn codegen_expr(expr: &TypedExpr) -> String {
    match expr {
        TypedExpr::Int(n) => n.to_string(),
        TypedExpr::BigInt(n) => format!("{}n", n), // BigInt literal
        TypedExpr::Float(n) => format_float(*n),
        TypedExpr::Bool(b) => b.to_string(),
        TypedExpr::String(s) => escape_js_string(s),
        TypedExpr::List { elements, .. } | TypedExpr::Tuple { elements, .. } => {
            let strs: Vec<String> = elements.iter().map(codegen_expr).collect();
            format!("[{}]", strs.join(", "))
        }
        TypedExpr::Var { path, .. } => format_path(path),
        TypedExpr::Call { path, args, .. } => {
            let args_str: Vec<String> = args.iter().map(codegen_expr).collect();
            format!("{}({})", format_path(path), args_str.join(", "))
        }
        TypedExpr::UnaryOp { op, expr, .. } => {
            let inner = codegen_operand(expr);
            match op {
                UnaryOp::Neg => format!("(-{})", inner),
            }
        }
        TypedExpr::BinOp {
            op,
            left,
            right,
            ty,
        } => {
            let l = codegen_expr(left);
            let r = codegen_expr(right);

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

            // Handle BigInt division with division by zero check
            if *op == BinOp::Div && *ty == Type::BigInt {
                return format!("{}({}, {})", DIV_BIGINT_CHECK_FN, l, r);
            }

            let l = if is_safe_operand(left) {
                l
            } else {
                format!("({})", l)
            };
            let r = if is_safe_operand(right) {
                r
            } else {
                format!("({})", r)
            };
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
            format!("({} {} {})", l, op_str, r)
        }
        TypedExpr::Block { bindings, result } => {
            // Generate IIFE for proper scoping
            let mut out = String::from("(function() {");

            for (i, binding) in bindings.iter().enumerate() {
                let value_code = codegen_expr(&binding.value);

                // For simple Var patterns, use direct assignment
                if let TypedPattern::Var { name, .. } = &binding.pattern {
                    out.push_str(&format!(" const {} = {};", format_name(name), value_code));
                } else {
                    // For complex patterns, store in temp and destructure
                    let temp_name = format!("$$let{}", i);
                    out.push_str(&format!(" const {} = {};", temp_name, value_code));
                    let (_, binding_stmts) = codegen_pattern_at_path(&binding.pattern, &temp_name);
                    for stmt in &binding_stmts {
                        out.push(' ');
                        out.push_str(stmt);
                    }
                }
            }

            out.push_str(&format!(" return {}; }})()", codegen_expr(result)));
            out
        }
        TypedExpr::Match {
            scrutinee, arms, ..
        } => codegen_match(scrutinee, arms),
        TypedExpr::MethodCall {
            receiver,
            method,
            args,
            ..
        } => {
            let receiver_code = codegen_expr(receiver);
            let receiver_ty = receiver.ty();
            let args_code: Vec<String> = args.iter().map(codegen_expr).collect();

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
                    Type::BigInt => {
                        format!("{}({}, {})", MIN_BIGINT_FN, receiver_code, args_code[0])
                    }
                    _ => format!("Math.min({}, {})", receiver_code, args_code[0]),
                },
                "max" => match receiver_ty {
                    Type::BigInt => {
                        format!("{}({}, {})", MAX_BIGINT_FN, receiver_code, args_code[0])
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
            }
        }

        TypedExpr::Lambda { params, body, .. } => {
            let (param_names, prologue) = codegen_params(params);
            let body_code = codegen_expr(body);

            if params.is_empty() {
                format!("(() => {})", body_code)
            } else if prologue.is_empty() {
                format!("(({}) => {})", param_names.join(", "), body_code)
            } else {
                // Need block body for destructuring
                format!(
                    "(({}) => {{ {} return {}; }})",
                    param_names.join(", "),
                    prologue.join(" "),
                    body_code
                )
            }
        }
        TypedExpr::StructConstruct { fields, spread, .. } => {
            // Generate a plain JS object, with optional spread
            let mut parts: Vec<String> = Vec::new();
            if let Some(spread_expr) = spread {
                parts.push(format!("...{}", codegen_expr(spread_expr)));
            }
            for (name, expr) in fields {
                parts.push(format!("{}: {}", name, codegen_expr(expr)));
            }
            if parts.is_empty() {
                "({})".to_string()
            } else {
                format!("({{ {} }})", parts.join(", "))
            }
        }
        TypedExpr::StructTupleConstruct { args, .. } => {
            if args.is_empty() {
                "({})".to_string()
            } else {
                let field_strs: Vec<String> = args
                    .iter()
                    .enumerate()
                    .map(|(i, e)| format!("${}: {}", i, codegen_expr(e)))
                    .collect();
                format!("({{ {} }})", field_strs.join(", "))
            }
        }
        TypedExpr::FieldAccess { expr, field, .. } => {
            format!("({}).{}", codegen_expr(expr), field)
        }
        TypedExpr::TupleIndex { expr, index, .. } => match &expr.ty() {
            Type::Tuple(_) => format!("({})[{}]", codegen_expr(expr), index),
            _ => format!("({}).${}", codegen_expr(expr), index),
        },
        TypedExpr::ListIndex { expr, index, .. } => {
            format!(
                "{}({}, {})",
                LIST_IDX_FN,
                codegen_expr(expr),
                codegen_expr(index)
            )
        }
        TypedExpr::EnumConstruct { path, fields, .. } => {
            let variant_name = path.last();
            let field_strs: Vec<String> = match fields {
                TypedEnumConstructFields::Unit => vec![],
                TypedEnumConstructFields::Tuple(exprs) => exprs
                    .iter()
                    .enumerate()
                    .map(|(i, e)| format!("${}: {}", i, codegen_expr(e)))
                    .collect(),
                TypedEnumConstructFields::Struct(fields) => fields
                    .iter()
                    .map(|(name, e)| format!("{}: {}", name, codegen_expr(e)))
                    .collect(),
            };
            if field_strs.is_empty() {
                format!("({{ $tag: \"{}\" }})", variant_name)
            } else {
                format!(
                    "({{ $tag: \"{}\", {} }})",
                    variant_name,
                    field_strs.join(", ")
                )
            }
        }
    }
}

/// Generate JS code for a single match arm
fn codegen_match_arm(pattern: &TypedPattern, result: &TypedExpr) -> String {
    let result_code = codegen_expr(result);
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
    let scrutinee_code = codegen_expr(scrutinee);
    let mut out = String::from("(function($match) {");

    for arm in arms {
        out.push(' ');
        out.push_str(&codegen_match_arm(&arm.pattern, &arm.result));
    }

    out.push_str(&format!(" }})({})", scrutinee_code));
    out
}

/// Generate JS code for a function definition
/// Generate JS parameter names and destructuring prologue from pattern params.
fn codegen_params(params: &[(TypedPattern, Type)]) -> (Vec<String>, Vec<String>) {
    let mut param_names = Vec::new();
    let mut prologue = Vec::new();
    let mut param_counter = 0;

    for (pattern, _) in params {
        match pattern {
            TypedPattern::Var { name, .. } => {
                param_names.push(format_name(name));
            }
            _ => {
                let synthetic_name = format!("$$param{}", param_counter);
                param_counter += 1;
                let (_, bindings) = codegen_pattern_at_path(pattern, &synthetic_name);
                prologue.extend(bindings);
                param_names.push(synthetic_name);
            }
        }
    }

    (param_names, prologue)
}

fn codegen_function(func: &TypedFunction, path: &QualifiedPath) -> String {
    if func.is_builtin {
        return codegen_builtin_function(func, path);
    }

    let (param_names, prologue) = codegen_params(&func.params);
    let body = codegen_expr(&func.body);

    if prologue.is_empty() {
        format!(
            "function {}({}) {{ return {}; }}",
            format_path(path),
            param_names.join(", "),
            body
        )
    } else {
        format!(
            "function {}({}) {{ {} return {}; }}",
            format_path(path),
            param_names.join(", "),
            prologue.join(" "),
            body
        )
    }
}

/// Generate JS code for a builtin function by looking up its implementation.
fn codegen_builtin_function(func: &TypedFunction, path: &QualifiedPath) -> String {
    let (param_names, _) = codegen_params(&func.params);
    let path_key = path.segments().join("::");
    let body = match path_key.as_str() {
        "root::panic" => {
            "throw new Error(\"$$panic:\" + $message);".to_string()
        }
        "root::json::parse" => {
            "try { return { $tag: \"Ok\", $0: $$json_to_zoya(JSON.parse($value)) }; } catch(_) { return { $tag: \"Err\", $0: { $tag: \"ParseError\" } }; }".to_string()
        }
        _ => panic!(
            "no builtin JS implementation for '{}' — every #[builtin] function must have a codegen entry",
            path_key
        ),
    };
    format!(
        "function {}({}) {{ {} }}",
        format_path(path),
        param_names.join(", "),
        body
    )
}

fn format_float(n: f64) -> String {
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n.is_infinite() {
        return if n.is_sign_positive() {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        };
    }
    let s = n.to_string();
    // Ensure float always has decimal point for JS
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{}.0", s)
    }
}

fn escape_js_string(s: &str) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(s.len() + 2);
    result.push('"');
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\0' => result.push_str("\\0"),
            // Other control characters as \uXXXX
            c if c < '\x20' => {
                let _ = write!(result, "\\u{:04x}", c as u32);
            }
            _ => result.push(c),
        }
    }
    result.push('"');
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoya_ir::QualifiedPath;

    #[test]
    fn test_codegen_int() {
        let expr = TypedExpr::Int(42);
        assert_eq!(codegen_expr(&expr), "42");
    }

    #[test]
    fn test_codegen_negative_int() {
        let expr = TypedExpr::Int(-42);
        assert_eq!(codegen_expr(&expr), "-42");
    }

    #[test]
    fn test_codegen_bigint() {
        let expr = TypedExpr::BigInt(42);
        assert_eq!(codegen_expr(&expr), "42n");
    }

    #[test]
    fn test_codegen_bigint_large() {
        let expr = TypedExpr::BigInt(9_000_000_000);
        assert_eq!(codegen_expr(&expr), "9000000000n");
    }

    #[test]
    fn test_codegen_float() {
        let expr = TypedExpr::Float(3.15);
        assert_eq!(codegen_expr(&expr), "3.15");
    }

    #[test]
    fn test_codegen_float_whole_number() {
        let expr = TypedExpr::Float(5.0);
        assert_eq!(codegen_expr(&expr), "5.0");
    }

    #[test]
    fn test_codegen_unary_neg_int() {
        let expr = TypedExpr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(TypedExpr::Int(42)),
            ty: Type::Int,
        };
        assert_eq!(codegen_expr(&expr), "(-42)");
    }

    #[test]
    fn test_codegen_unary_neg_bigint() {
        let expr = TypedExpr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(TypedExpr::BigInt(42)),
            ty: Type::BigInt,
        };
        assert_eq!(codegen_expr(&expr), "(-42n)");
    }

    #[test]
    fn test_codegen_addition_int() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Int(1)),
            right: Box::new(TypedExpr::Int(2)),
            ty: Type::Int,
        };
        assert_eq!(codegen_expr(&expr), "(1 + 2)");
    }

    #[test]
    fn test_codegen_addition_bigint() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::BigInt(1)),
            right: Box::new(TypedExpr::BigInt(2)),
            ty: Type::BigInt,
        };
        assert_eq!(codegen_expr(&expr), "(1n + 2n)");
    }

    #[test]
    fn test_codegen_subtraction() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Sub,
            left: Box::new(TypedExpr::Int(5)),
            right: Box::new(TypedExpr::Int(3)),
            ty: Type::Int,
        };
        assert_eq!(codegen_expr(&expr), "(5 - 3)");
    }

    #[test]
    fn test_codegen_multiplication() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Mul,
            left: Box::new(TypedExpr::Int(3)),
            right: Box::new(TypedExpr::Int(4)),
            ty: Type::Int,
        };
        assert_eq!(codegen_expr(&expr), "(3 * 4)");
    }

    #[test]
    fn test_codegen_division() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Div,
            left: Box::new(TypedExpr::Int(10)),
            right: Box::new(TypedExpr::Int(2)),
            ty: Type::Int,
        };
        // Int division uses $$div for truncation and division-by-zero checking
        assert_eq!(codegen_expr(&expr), "$$div(10, 2)");
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
        assert_eq!(codegen_expr(&expr), "(2 + (3 * 4))");
    }

    #[test]
    fn test_codegen_float_expression() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Float(1.5)),
            right: Box::new(TypedExpr::Float(2.5)),
            ty: Type::Float,
        };
        assert_eq!(codegen_expr(&expr), "(1.5 + 2.5)");
    }

    #[test]
    fn test_codegen_var() {
        let expr = TypedExpr::Var {
            path: QualifiedPath::local("x".to_string()),
            ty: Type::Int,
        };
        assert_eq!(codegen_expr(&expr), "$x");
    }

    #[test]
    fn test_codegen_call_no_args() {
        let expr = TypedExpr::Call {
            path: QualifiedPath::local("foo".to_string()),
            args: vec![],
            ty: Type::Int,
        };
        assert_eq!(codegen_expr(&expr), "$foo()");
    }

    #[test]
    fn test_codegen_call_one_arg() {
        let expr = TypedExpr::Call {
            path: QualifiedPath::local("square".to_string()),
            args: vec![TypedExpr::Int(5)],
            ty: Type::Int,
        };
        assert_eq!(codegen_expr(&expr), "$square(5)");
    }

    #[test]
    fn test_codegen_call_multiple_args() {
        let expr = TypedExpr::Call {
            path: QualifiedPath::local("add".to_string()),
            args: vec![TypedExpr::Int(1), TypedExpr::Int(2)],
            ty: Type::Int,
        };
        assert_eq!(codegen_expr(&expr), "$add(1, 2)");
    }

    #[test]
    fn test_codegen_call_with_vars() {
        let expr = TypedExpr::Call {
            path: QualifiedPath::local("add".to_string()),
            args: vec![
                TypedExpr::Var {
                    path: QualifiedPath::local("x".to_string()),
                    ty: Type::Int,
                },
                TypedExpr::Var {
                    path: QualifiedPath::local("y".to_string()),
                    ty: Type::Int,
                },
            ],
            ty: Type::Int,
        };
        assert_eq!(codegen_expr(&expr), "$add($x, $y)");
    }

    #[test]
    fn test_codegen_var_in_expression() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Var {
                path: QualifiedPath::local("x".to_string()),
                ty: Type::Int,
            }),
            right: Box::new(TypedExpr::Int(1)),
            ty: Type::Int,
        };
        assert_eq!(codegen_expr(&expr), "($x + 1)");
    }

    #[test]
    fn test_codegen_function() {
        let func = TypedFunction {
            name: "square".to_string(),
            params: vec![(
                TypedPattern::Var {
                    name: "x".to_string(),
                    ty: Type::Int,
                },
                Type::Int,
            )],
            body: TypedExpr::BinOp {
                op: BinOp::Mul,
                left: Box::new(TypedExpr::Var {
                    path: QualifiedPath::local("x".to_string()),
                    ty: Type::Int,
                }),
                right: Box::new(TypedExpr::Var {
                    path: QualifiedPath::local("x".to_string()),
                    ty: Type::Int,
                }),
                ty: Type::Int,
            },
            return_type: Type::Int,
            is_builtin: false,
        };
        assert_eq!(
            codegen_function(&func, &QualifiedPath::root().child(&func.name)),
            "function $root$square($x) { return ($x * $x); }"
        );
    }

    #[test]
    fn test_codegen_function_multiple_params() {
        let func = TypedFunction {
            name: "add".to_string(),
            params: vec![
                (
                    TypedPattern::Var {
                        name: "x".to_string(),
                        ty: Type::Int,
                    },
                    Type::Int,
                ),
                (
                    TypedPattern::Var {
                        name: "y".to_string(),
                        ty: Type::Int,
                    },
                    Type::Int,
                ),
            ],
            body: TypedExpr::BinOp {
                op: BinOp::Add,
                left: Box::new(TypedExpr::Var {
                    path: QualifiedPath::local("x".to_string()),
                    ty: Type::Int,
                }),
                right: Box::new(TypedExpr::Var {
                    path: QualifiedPath::local("y".to_string()),
                    ty: Type::Int,
                }),
                ty: Type::Int,
            },
            return_type: Type::Int,
            is_builtin: false,
        };
        assert_eq!(
            codegen_function(&func, &QualifiedPath::root().child(&func.name)),
            "function $root$add($x, $y) { return ($x + $y); }"
        );
    }

    #[test]
    fn test_codegen_function_no_params() {
        let func = TypedFunction {
            name: "answer".to_string(),
            params: vec![],
            body: TypedExpr::Int(42),
            return_type: Type::Int,
            is_builtin: false,
        };
        assert_eq!(
            codegen_function(&func, &QualifiedPath::root().child(&func.name)),
            "function $root$answer() { return 42; }"
        );
    }

    #[test]
    fn test_codegen_bigint_function() {
        let func = TypedFunction {
            name: "big".to_string(),
            params: vec![(
                TypedPattern::Var {
                    name: "x".to_string(),
                    ty: Type::BigInt,
                },
                Type::BigInt,
            )],
            body: TypedExpr::BinOp {
                op: BinOp::Add,
                left: Box::new(TypedExpr::Var {
                    path: QualifiedPath::local("x".to_string()),
                    ty: Type::BigInt,
                }),
                right: Box::new(TypedExpr::BigInt(1)),
                ty: Type::BigInt,
            },
            return_type: Type::BigInt,
            is_builtin: false,
        };
        assert_eq!(
            codegen_function(&func, &QualifiedPath::root().child(&func.name)),
            "function $root$big($x) { return ($x + 1n); }"
        );
    }
}
