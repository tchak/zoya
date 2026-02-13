use zoya_ast::{BinOp, UnaryOp};
use zoya_ir::{
    CheckedPackage, QualifiedPath, Type, TypedEnumConstructFields, TypedExpr, TypedFunction,
    TypedListElement, TypedMatchArm, TypedPattern,
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

/// Modulo by zero check function name used in generated JS
const MOD_CHECK_FN: &str = "$$mod";

/// BigInt modulo by zero check function name used in generated JS
const MOD_BIGINT_CHECK_FN: &str = "$$mod_bigint";

/// Power with negative exponent check function name used in generated JS
const POW_CHECK_FN: &str = "$$pow";

/// BigInt power with negative exponent check function name used in generated JS
const POW_BIGINT_CHECK_FN: &str = "$$pow_bigint";

/// List index function name used in generated JS
const LIST_IDX_FN: &str = "$$list_idx";

/// Runtime JS: prelude helpers (error, equality, arithmetic checks, etc.)
const PRELUDE_JS: &str = include_str!("js/prelude.js");

/// Runtime JS: HAMT for Dict<K, V>
const HAMT_JS: &str = include_str!("js/hamt.js");

/// Runtime JS: Int methods
const INT_JS: &str = include_str!("js/int.js");

/// Runtime JS: BigInt methods
const BIGINT_JS: &str = include_str!("js/bigint.js");

/// Runtime JS: Float methods
const FLOAT_JS: &str = include_str!("js/float.js");

/// Runtime JS: String methods
const STRING_JS: &str = include_str!("js/string.js");

/// Runtime JS: List methods
const LIST_JS: &str = include_str!("js/list.js");

/// Generate a single concatenated JS string for a package and all its dependencies.
/// Returns a `CodegenOutput` containing the prelude, dep functions, and main package functions.
pub fn codegen(package: &CheckedPackage, deps: &[&CheckedPackage]) -> CodegenOutput {
    let mut js = String::new();

    // Runtime helpers
    js.push_str(PRELUDE_JS);
    js.push('\n');
    js.push_str(HAMT_JS);
    js.push('\n');
    js.push_str(INT_JS);
    js.push('\n');
    js.push_str(BIGINT_JS);
    js.push('\n');
    js.push_str(FLOAT_JS);
    js.push('\n');
    js.push_str(STRING_JS);
    js.push('\n');
    js.push_str(LIST_JS);
    js.push('\n');

    // Dependency function definitions
    for dep in deps {
        let pkg_gen = PackageCodegen {
            pkg_name: &dep.name,
        };
        pkg_gen.append_package_body(dep, &mut js);
    }

    // Main package function definitions
    let pkg_gen = PackageCodegen {
        pkg_name: &package.name,
    };
    pkg_gen.append_package_body(package, &mut js);

    let hash = blake3::hash(js.as_bytes()).to_hex().to_string();
    CodegenOutput { code: js, hash }
}

/// Per-package codegen context. Replaces `root` in qualified paths with `pkg_name`.
struct PackageCodegen<'a> {
    pkg_name: &'a str,
}

impl<'a> PackageCodegen<'a> {
    /// Append all function definitions from the package to `js`.
    fn append_package_body(&self, pkg: &CheckedPackage, js: &mut String) {
        // Sort items by path depth (parents before children)
        let mut item_paths: Vec<_> = pkg.items.keys().collect();
        item_paths.sort_by_key(|p| p.depth());

        for path in item_paths {
            if let Some(func) = pkg.items.get(path) {
                js.push_str(&self.codegen_function(func, path));
                js.push('\n');
            }
        }
    }

    /// Format a qualified path as a JS identifier.
    /// Paths starting with `root` have it replaced by `pkg_name`.
    /// Cross-package paths (e.g., `std::option::Some`) are left as-is.
    fn format_path(&self, path: &QualifiedPath) -> String {
        let segments = path.segments();
        if segments.first().map(|s| s.as_str()) == Some("root") {
            format_export_path(path, self.pkg_name)
        } else {
            format!("${}", segments.join("$"))
        }
    }

    fn codegen_expr(&self, expr: &TypedExpr) -> String {
        match expr {
            TypedExpr::Int(n) => n.to_string(),
            TypedExpr::BigInt(n) => format!("{}n", n),
            TypedExpr::Float(n) => format_float(*n),
            TypedExpr::Bool(b) => b.to_string(),
            TypedExpr::String(s) => escape_js_string(s),
            TypedExpr::List { elements, .. } => {
                let strs: Vec<String> = elements
                    .iter()
                    .map(|e| match e {
                        TypedListElement::Item(expr) => self.codegen_expr(expr),
                        TypedListElement::Spread(expr) => {
                            format!("...{}", self.codegen_expr(expr))
                        }
                    })
                    .collect();
                format!("[{}]", strs.join(", "))
            }
            TypedExpr::Tuple { elements, .. } => {
                let strs: Vec<String> = elements.iter().map(|e| self.codegen_expr(e)).collect();
                format!("[{}]", strs.join(", "))
            }
            TypedExpr::Var { path, .. } => self.format_path(path),
            TypedExpr::Call { path, args, .. } => {
                let args_str: Vec<String> = args.iter().map(|a| self.codegen_expr(a)).collect();
                format!("{}({})", self.format_path(path), args_str.join(", "))
            }
            TypedExpr::UnaryOp { op, expr, .. } => {
                let inner = self.codegen_operand(expr);
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
                let l = self.codegen_expr(left);
                let r = self.codegen_expr(right);

                if matches!(op, BinOp::Eq | BinOp::Ne) && needs_deep_equality(&left.ty()) {
                    let deep_eq = format!("{}({}, {})", DEEP_EQ_FN, l, r);
                    return if *op == BinOp::Eq {
                        deep_eq
                    } else {
                        format!("(!{})", deep_eq)
                    };
                }

                if *op == BinOp::Div && *ty == Type::Int {
                    return format!("{}({}, {})", DIV_CHECK_FN, l, r);
                }

                if *op == BinOp::Div && *ty == Type::BigInt {
                    return format!("{}({}, {})", DIV_BIGINT_CHECK_FN, l, r);
                }

                if *op == BinOp::Mod && *ty == Type::Int {
                    return format!("{}({}, {})", MOD_CHECK_FN, l, r);
                }

                if *op == BinOp::Mod && *ty == Type::BigInt {
                    return format!("{}({}, {})", MOD_BIGINT_CHECK_FN, l, r);
                }

                if *op == BinOp::Pow && *ty == Type::Int {
                    return format!("{}({}, {})", POW_CHECK_FN, l, r);
                }

                if *op == BinOp::Pow && *ty == Type::BigInt {
                    return format!("{}({}, {})", POW_BIGINT_CHECK_FN, l, r);
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
                    BinOp::Mod => "%",
                    BinOp::Pow => "**",
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
                let mut out = String::from("(function() {");

                for (i, binding) in bindings.iter().enumerate() {
                    let value_code = self.codegen_expr(&binding.value);

                    if let TypedPattern::Var { name, .. } = &binding.pattern {
                        out.push_str(&format!(" const {} = {};", format_name(name), value_code));
                    } else {
                        let temp_name = format!("$$let{}", i);
                        out.push_str(&format!(" const {} = {};", temp_name, value_code));
                        let (_, binding_stmts) =
                            self.codegen_pattern_at_path(&binding.pattern, &temp_name);
                        for stmt in &binding_stmts {
                            out.push(' ');
                            out.push_str(stmt);
                        }
                    }
                }

                out.push_str(&format!(" return {}; }})()", self.codegen_expr(result)));
                out
            }
            TypedExpr::Match {
                scrutinee, arms, ..
            } => self.codegen_match(scrutinee, arms),
            TypedExpr::Lambda { params, body, .. } => {
                let (param_names, prologue) = self.codegen_params(params);
                let body_code = self.codegen_expr(body);

                if params.is_empty() {
                    format!("(() => {})", body_code)
                } else if prologue.is_empty() {
                    format!("(({}) => {})", param_names.join(", "), body_code)
                } else {
                    format!(
                        "(({}) => {{ {} return {}; }})",
                        param_names.join(", "),
                        prologue.join(" "),
                        body_code
                    )
                }
            }
            TypedExpr::StructConstruct { fields, spread, .. } => {
                let mut parts: Vec<String> = Vec::new();
                if let Some(spread_expr) = spread {
                    parts.push(format!("...{}", self.codegen_expr(spread_expr)));
                }
                for (name, expr) in fields {
                    parts.push(format!("{}: {}", name, self.codegen_expr(expr)));
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
                        .map(|(i, e)| format!("${}: {}", i, self.codegen_expr(e)))
                        .collect();
                    format!("({{ {} }})", field_strs.join(", "))
                }
            }
            TypedExpr::FieldAccess { expr, field, .. } => {
                format!("({}).{}", self.codegen_expr(expr), field)
            }
            TypedExpr::TupleIndex { expr, index, .. } => match &expr.ty() {
                Type::Tuple(_) => format!("({})[{}]", self.codegen_expr(expr), index),
                _ => format!("({}).${}", self.codegen_expr(expr), index),
            },
            TypedExpr::ListIndex { expr, index, .. } => {
                format!(
                    "{}({}, {})",
                    LIST_IDX_FN,
                    self.codegen_expr(expr),
                    self.codegen_expr(index)
                )
            }
            TypedExpr::EnumConstruct { path, fields, .. } => {
                let variant_name = path.last();
                let field_strs: Vec<String> = match fields {
                    TypedEnumConstructFields::Unit => vec![],
                    TypedEnumConstructFields::Tuple(exprs) => exprs
                        .iter()
                        .enumerate()
                        .map(|(i, e)| format!("${}: {}", i, self.codegen_expr(e)))
                        .collect(),
                    TypedEnumConstructFields::Struct(fields) => fields
                        .iter()
                        .map(|(name, e)| format!("{}: {}", name, self.codegen_expr(e)))
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

    /// Generate JS for an expression, wrapping in parens only if needed for operator safety.
    fn codegen_operand(&self, expr: &TypedExpr) -> String {
        let code = self.codegen_expr(expr);
        if is_safe_operand(expr) {
            code
        } else {
            format!("({})", code)
        }
    }

    /// Generate conditions and bindings for a pattern at a given access path.
    fn codegen_pattern_at_path(
        &self,
        pattern: &TypedPattern,
        access_path: &str,
    ) -> (Vec<String>, Vec<String>) {
        let mut conditions = Vec::new();
        let mut bindings = Vec::new();

        match pattern {
            TypedPattern::Literal(lit) => {
                let lit_code = self.codegen_expr(lit);
                if needs_deep_equality(&lit.ty()) {
                    conditions.push(format!("{}({}, {})", DEEP_EQ_FN, access_path, lit_code));
                } else {
                    conditions.push(format!("{} === {}", access_path, lit_code));
                }
            }

            TypedPattern::Var { name, .. } => {
                bindings.push(format!("const {} = {};", format_name(name), access_path));
            }

            TypedPattern::Wildcard => {}

            TypedPattern::As { name, pattern, .. } => {
                bindings.push(format!("const {} = {};", format_name(name), access_path));
                let (inner_conds, inner_binds) = self.codegen_pattern_at_path(pattern, access_path);
                conditions.extend(inner_conds);
                bindings.extend(inner_binds);
            }

            TypedPattern::ListEmpty | TypedPattern::TupleEmpty => {
                conditions.push(array_length_condition(access_path, "===", 0));
            }

            TypedPattern::ListExact { patterns, len }
            | TypedPattern::TupleExact { patterns, len } => {
                conditions.push(array_length_condition(access_path, "===", *len));
                self.codegen_indexed_patterns(
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
                self.codegen_indexed_patterns(
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
                self.codegen_suffix_from_end_patterns(
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
                self.codegen_indexed_patterns(
                    prefix,
                    access_path,
                    0,
                    array_index,
                    &mut conditions,
                    &mut bindings,
                );
                self.codegen_suffix_from_end_patterns(
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
                self.codegen_indexed_patterns(
                    patterns,
                    access_path,
                    0,
                    array_index,
                    &mut conditions,
                    &mut bindings,
                );
                if let Some((name, _)) = rest_binding {
                    tuple_rest_binding(
                        name,
                        access_path,
                        patterns.len()..*total_len,
                        &mut bindings,
                    );
                }
            }

            TypedPattern::TupleSuffix {
                patterns,
                rest_binding,
                total_len,
            } => {
                conditions.push(array_length_condition(access_path, "===", *total_len));
                let start_idx = total_len - patterns.len();
                self.codegen_indexed_patterns(
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
                self.codegen_indexed_patterns(
                    prefix,
                    access_path,
                    0,
                    array_index,
                    &mut conditions,
                    &mut bindings,
                );
                self.codegen_indexed_patterns(
                    suffix,
                    access_path,
                    suffix_start,
                    array_index,
                    &mut conditions,
                    &mut bindings,
                );
                if let Some((name, _)) = rest_binding {
                    tuple_rest_binding(
                        name,
                        access_path,
                        prefix.len()..suffix_start,
                        &mut bindings,
                    );
                }
            }

            TypedPattern::StructExact { fields, .. }
            | TypedPattern::StructPartial { fields, .. } => {
                conditions.push(format!("{}({})", IS_OBJ_FN, access_path));
                for (field_name, pat) in fields {
                    let child_path = format!("{}.{}", access_path, field_name);
                    let (child_conds, child_binds) = self.codegen_pattern_at_path(pat, &child_path);
                    conditions.extend(child_conds);
                    bindings.extend(child_binds);
                }
            }

            TypedPattern::EnumUnit { path } => {
                conditions.push(enum_tag_condition(access_path, path));
            }

            TypedPattern::EnumTupleExact { path, patterns, .. } => {
                conditions.push(enum_tag_condition(access_path, path));
                self.codegen_indexed_patterns(
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
                self.codegen_indexed_patterns(
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
                self.codegen_indexed_patterns(
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
                self.codegen_indexed_patterns(
                    prefix,
                    access_path,
                    0,
                    enum_field,
                    &mut conditions,
                    &mut bindings,
                );
                let suffix_start = total_fields - suffix.len();
                self.codegen_indexed_patterns(
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
                    let (child_conds, child_binds) = self.codegen_pattern_at_path(pat, &child_path);
                    conditions.extend(child_conds);
                    bindings.extend(child_binds);
                }
            }

            TypedPattern::StructTupleExact { patterns, .. } => {
                conditions.push(format!("{}({})", IS_OBJ_FN, access_path));
                self.codegen_indexed_patterns(
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
                self.codegen_indexed_patterns(
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
                self.codegen_indexed_patterns(
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
                self.codegen_indexed_patterns(
                    prefix,
                    access_path,
                    0,
                    enum_field,
                    &mut conditions,
                    &mut bindings,
                );
                let suffix_start = total_fields - suffix.len();
                self.codegen_indexed_patterns(
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

    /// Generate conditions and bindings for a sequence of patterns at indexed positions.
    fn codegen_indexed_patterns(
        &self,
        patterns: &[TypedPattern],
        access_path: &str,
        start_idx: usize,
        make_child_path: fn(&str, usize) -> String,
        conditions: &mut Vec<String>,
        bindings: &mut Vec<String>,
    ) {
        for (i, pat) in patterns.iter().enumerate() {
            let child_path = make_child_path(access_path, start_idx + i);
            let (child_conds, child_binds) = self.codegen_pattern_at_path(pat, &child_path);
            conditions.extend(child_conds);
            bindings.extend(child_binds);
        }
    }

    /// Generate conditions and bindings for suffix patterns indexed from the end of a list.
    fn codegen_suffix_from_end_patterns(
        &self,
        patterns: &[TypedPattern],
        access_path: &str,
        suffix_len: usize,
        conditions: &mut Vec<String>,
        bindings: &mut Vec<String>,
    ) {
        for (i, pat) in patterns.iter().enumerate() {
            let offset = suffix_len - i;
            let indexed_path = format!("{}[{}.length - {}]", access_path, access_path, offset);
            let (child_conds, child_binds) = self.codegen_pattern_at_path(pat, &indexed_path);
            conditions.extend(child_conds);
            bindings.extend(child_binds);
        }
    }

    /// Generate JS code for a single match arm
    fn codegen_match_arm(&self, pattern: &TypedPattern, result: &TypedExpr) -> String {
        let result_code = self.codegen_expr(result);
        let (conditions, bindings) = self.codegen_pattern_at_path(pattern, "$match");

        let condition_str = if conditions.is_empty() {
            String::new()
        } else {
            conditions.join(" && ")
        };

        let bindings_str = bindings.join(" ");

        if matches!(pattern, TypedPattern::Wildcard) {
            return format!("return {};", result_code);
        }

        if matches!(pattern, TypedPattern::Var { .. }) {
            return format!("{{ {} return {}; }}", bindings_str, result_code);
        }

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
    fn codegen_match(&self, scrutinee: &TypedExpr, arms: &[TypedMatchArm]) -> String {
        let scrutinee_code = self.codegen_expr(scrutinee);
        let mut out = String::from("(function($match) {");

        for arm in arms {
            out.push(' ');
            out.push_str(&self.codegen_match_arm(&arm.pattern, &arm.result));
        }

        out.push_str(&format!(" }})({})", scrutinee_code));
        out
    }

    /// Generate JS parameter names and destructuring prologue from pattern params.
    fn codegen_params(&self, params: &[(TypedPattern, Type)]) -> (Vec<String>, Vec<String>) {
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
                    let (_, bindings) = self.codegen_pattern_at_path(pattern, &synthetic_name);
                    prologue.extend(bindings);
                    param_names.push(synthetic_name);
                }
            }
        }

        (param_names, prologue)
    }

    fn codegen_function(&self, func: &TypedFunction, path: &QualifiedPath) -> String {
        if func.is_builtin {
            return self.codegen_builtin_function(func, path);
        }

        let (param_names, prologue) = self.codegen_params(&func.params);
        let body = self.codegen_expr(&func.body);

        if prologue.is_empty() {
            format!(
                "function {}({}) {{ return {}; }}",
                self.format_path(path),
                param_names.join(", "),
                body
            )
        } else {
            format!(
                "function {}({}) {{ {} return {}; }}",
                self.format_path(path),
                param_names.join(", "),
                prologue.join(" "),
                body
            )
        }
    }

    /// Generate JS code for a builtin function by looking up its implementation.
    fn codegen_builtin_function(&self, func: &TypedFunction, path: &QualifiedPath) -> String {
        let (param_names, _) = self.codegen_params(&func.params);
        let path_key = path.segments().join("::");
        let body = match path_key.as_str() {
            "root::panic" => {
                "$$throw(\"PANIC\", $message);".to_string()
            }
            "root::assert" => {
                "if (!$condition) $$throw(\"PANIC\", \"assertion failed\"); return [];"
                    .to_string()
            }
            "root::assert_eq" => {
                "if (!$$eq($left, $right)) $$throw(\"PANIC\", \"assertion failed: left != right\"); return [];"
                    .to_string()
            }
            "root::assert_ne" => {
                "if ($$eq($left, $right)) $$throw(\"PANIC\", \"assertion failed: left == right\"); return [];"
                    .to_string()
            }
            "root::io::println" => {
                "console.log($message); return [];".to_string()
            }
            "root::json::parse" => {
                "try { return { $tag: \"Ok\", $0: $$json_to_zoya(JSON.parse($value)) }; } catch(_) { return { $tag: \"Err\", $0: { $tag: \"ParseError\" } }; }".to_string()
            }
            // Int methods
            "root::int::Int::abs" => "return $$Int.abs($self);".to_string(),
            "root::int::Int::to_string" => "return $$Int.to_string($self);".to_string(),
            "root::int::Int::to_float" => "return $$Int.to_float($self);".to_string(),
            "root::int::Int::min" => "return $$Int.min($self, $other);".to_string(),
            "root::int::Int::max" => "return $$Int.max($self, $other);".to_string(),

            // BigInt methods
            "root::bigint::BigInt::abs" => "return $$BigInt.abs($self);".to_string(),
            "root::bigint::BigInt::to_string" => "return $$BigInt.to_string($self);".to_string(),
            "root::bigint::BigInt::min" => "return $$BigInt.min($self, $other);".to_string(),
            "root::bigint::BigInt::max" => "return $$BigInt.max($self, $other);".to_string(),

            // Float methods
            "root::float::Float::abs" => "return $$Float.abs($self);".to_string(),
            "root::float::Float::to_string" => "return $$Float.to_string($self);".to_string(),
            "root::float::Float::to_int" => "return $$Float.to_int($self);".to_string(),
            "root::float::Float::floor" => "return $$Float.floor($self);".to_string(),
            "root::float::Float::ceil" => "return $$Float.ceil($self);".to_string(),
            "root::float::Float::round" => "return $$Float.round($self);".to_string(),
            "root::float::Float::sqrt" => "return $$Float.sqrt($self);".to_string(),
            "root::float::Float::min" => "return $$Float.min($self, $other);".to_string(),
            "root::float::Float::max" => "return $$Float.max($self, $other);".to_string(),

            // String methods
            "root::string::String::len" => "return $$String.len($self);".to_string(),
            "root::string::String::contains" => "return $$String.contains($self, $needle);".to_string(),
            "root::string::String::starts_with" => "return $$String.starts_with($self, $prefix);".to_string(),
            "root::string::String::ends_with" => "return $$String.ends_with($self, $suffix);".to_string(),
            "root::string::String::to_uppercase" => "return $$String.to_uppercase($self);".to_string(),
            "root::string::String::to_lowercase" => "return $$String.to_lowercase($self);".to_string(),
            "root::string::String::trim" => "return $$String.trim($self);".to_string(),
            "root::string::String::trim_start" => "return $$String.trim_start($self);".to_string(),
            "root::string::String::trim_end" => "return $$String.trim_end($self);".to_string(),
            "root::string::String::replace" => "return $$String.replace($self, $from, $to);".to_string(),
            "root::string::String::repeat" => "return $$String.repeat($self, $n);".to_string(),
            "root::string::String::split" => "return $$String.split($self, $sep);".to_string(),
            "root::string::String::chars" => "return $$String.chars($self);".to_string(),
            "root::string::String::find" => "return $$String.find($self, $needle);".to_string(),
            "root::string::String::slice" => "return $$String.slice($self, $start, $end);".to_string(),
            "root::string::String::reverse" => "return $$String.reverse($self);".to_string(),
            "root::string::String::replace_first" => "return $$String.replace_first($self, $from, $to);".to_string(),
            "root::string::String::pad_start" => "return $$String.pad_start($self, $len, $fill);".to_string(),
            "root::string::String::pad_end" => "return $$String.pad_end($self, $len, $fill);".to_string(),
            "root::string::String::to_int" => "return $$String.to_int($self);".to_string(),
            "root::string::String::to_float" => "return $$String.to_float($self);".to_string(),

            // List methods
            "root::list::List::len" => "return $$List.len($self);".to_string(),
            "root::list::List::reverse" => "return $$List.reverse($self);".to_string(),
            "root::list::List::push" => "return $$List.push($self, $item);".to_string(),
            "root::list::List::map" => "return $$List.map($self, $f);".to_string(),
            "root::list::List::filter" => "return $$List.filter($self, $f);".to_string(),
            "root::list::List::fold" => "return $$List.fold($self, $init, $f);".to_string(),
            "root::list::List::filter_map" => "return $$List.filter_map($self, $f);".to_string(),
            "root::list::List::truncate" => "return $$List.truncate($self, $len);".to_string(),
            "root::list::List::insert" => "return $$List.insert($self, $index, $value);".to_string(),
            "root::list::List::remove" => "return $$List.remove($self, $index);".to_string(),

            // Dict methods
            "root::dict::Dict::new" => "return $$Dict.empty();".to_string(),
            "root::dict::Dict::get" => "return $$Dict.get($self, $key);".to_string(),
            "root::dict::Dict::insert" => "return $$Dict.insert($self, $key, $value);".to_string(),
            "root::dict::Dict::remove" => "return $$Dict.remove($self, $key);".to_string(),
            "root::dict::Dict::keys" => "return $$Dict.keys($self);".to_string(),
            "root::dict::Dict::values" => "return $$Dict.values($self);".to_string(),
            "root::dict::Dict::len" => "return $$Dict.len($self);".to_string(),

            _ => panic!(
                "no builtin JS implementation for '{}' — every #[builtin] function must have a codegen entry",
                path_key
            ),
        };
        format!(
            "function {}({}) {{ {} }}",
            self.format_path(path),
            param_names.join(", "),
            body
        )
    }
}

/// Check if an expression doesn't need wrapping in parens when used as an operand.
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

/// Check if a type requires deep equality comparison
fn needs_deep_equality(ty: &Type) -> bool {
    matches!(
        ty,
        Type::List(_) | Type::Dict(_, _) | Type::Tuple(_) | Type::Struct { .. } | Type::Enum { .. }
    )
}

/// Format a qualified path as a JS export name, replacing the "root" prefix with the package name.
/// e.g., root::main with pkg_name "myapp" -> $myapp$main
pub fn format_export_path(path: &QualifiedPath, pkg_name: &str) -> String {
    let segments = path.segments();
    let renamed: Vec<&str> = std::iter::once(pkg_name)
        .chain(segments[1..].iter().map(|s| s.as_str()))
        .collect();
    format!("${}", renamed.join("$"))
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

    /// Helper to create a PackageCodegen for tests
    fn test_gen() -> PackageCodegen<'static> {
        PackageCodegen { pkg_name: "test" }
    }

    #[test]
    fn test_codegen_int() {
        let expr = TypedExpr::Int(42);
        assert_eq!(test_gen().codegen_expr(&expr), "42");
    }

    #[test]
    fn test_codegen_negative_int() {
        let expr = TypedExpr::Int(-42);
        assert_eq!(test_gen().codegen_expr(&expr), "-42");
    }

    #[test]
    fn test_codegen_bigint() {
        let expr = TypedExpr::BigInt(42);
        assert_eq!(test_gen().codegen_expr(&expr), "42n");
    }

    #[test]
    fn test_codegen_bigint_large() {
        let expr = TypedExpr::BigInt(9_000_000_000);
        assert_eq!(test_gen().codegen_expr(&expr), "9000000000n");
    }

    #[test]
    fn test_codegen_float() {
        let expr = TypedExpr::Float(3.15);
        assert_eq!(test_gen().codegen_expr(&expr), "3.15");
    }

    #[test]
    fn test_codegen_float_whole_number() {
        let expr = TypedExpr::Float(5.0);
        assert_eq!(test_gen().codegen_expr(&expr), "5.0");
    }

    #[test]
    fn test_codegen_unary_neg_int() {
        let expr = TypedExpr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(TypedExpr::Int(42)),
            ty: Type::Int,
        };
        assert_eq!(test_gen().codegen_expr(&expr), "(-42)");
    }

    #[test]
    fn test_codegen_unary_neg_bigint() {
        let expr = TypedExpr::UnaryOp {
            op: UnaryOp::Neg,
            expr: Box::new(TypedExpr::BigInt(42)),
            ty: Type::BigInt,
        };
        assert_eq!(test_gen().codegen_expr(&expr), "(-42n)");
    }

    #[test]
    fn test_codegen_addition_int() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Int(1)),
            right: Box::new(TypedExpr::Int(2)),
            ty: Type::Int,
        };
        assert_eq!(test_gen().codegen_expr(&expr), "(1 + 2)");
    }

    #[test]
    fn test_codegen_addition_bigint() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::BigInt(1)),
            right: Box::new(TypedExpr::BigInt(2)),
            ty: Type::BigInt,
        };
        assert_eq!(test_gen().codegen_expr(&expr), "(1n + 2n)");
    }

    #[test]
    fn test_codegen_subtraction() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Sub,
            left: Box::new(TypedExpr::Int(5)),
            right: Box::new(TypedExpr::Int(3)),
            ty: Type::Int,
        };
        assert_eq!(test_gen().codegen_expr(&expr), "(5 - 3)");
    }

    #[test]
    fn test_codegen_multiplication() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Mul,
            left: Box::new(TypedExpr::Int(3)),
            right: Box::new(TypedExpr::Int(4)),
            ty: Type::Int,
        };
        assert_eq!(test_gen().codegen_expr(&expr), "(3 * 4)");
    }

    #[test]
    fn test_codegen_division() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Div,
            left: Box::new(TypedExpr::Int(10)),
            right: Box::new(TypedExpr::Int(2)),
            ty: Type::Int,
        };
        assert_eq!(test_gen().codegen_expr(&expr), "$$div(10, 2)");
    }

    #[test]
    fn test_codegen_complex_expression() {
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
        assert_eq!(test_gen().codegen_expr(&expr), "(2 + (3 * 4))");
    }

    #[test]
    fn test_codegen_float_expression() {
        let expr = TypedExpr::BinOp {
            op: BinOp::Add,
            left: Box::new(TypedExpr::Float(1.5)),
            right: Box::new(TypedExpr::Float(2.5)),
            ty: Type::Float,
        };
        assert_eq!(test_gen().codegen_expr(&expr), "(1.5 + 2.5)");
    }

    #[test]
    fn test_codegen_var() {
        let expr = TypedExpr::Var {
            path: QualifiedPath::local("x".to_string()),
            ty: Type::Int,
        };
        assert_eq!(test_gen().codegen_expr(&expr), "$x");
    }

    #[test]
    fn test_codegen_call_no_args() {
        let expr = TypedExpr::Call {
            path: QualifiedPath::local("foo".to_string()),
            args: vec![],
            ty: Type::Int,
        };
        assert_eq!(test_gen().codegen_expr(&expr), "$foo()");
    }

    #[test]
    fn test_codegen_call_one_arg() {
        let expr = TypedExpr::Call {
            path: QualifiedPath::local("square".to_string()),
            args: vec![TypedExpr::Int(5)],
            ty: Type::Int,
        };
        assert_eq!(test_gen().codegen_expr(&expr), "$square(5)");
    }

    #[test]
    fn test_codegen_call_multiple_args() {
        let expr = TypedExpr::Call {
            path: QualifiedPath::local("add".to_string()),
            args: vec![TypedExpr::Int(1), TypedExpr::Int(2)],
            ty: Type::Int,
        };
        assert_eq!(test_gen().codegen_expr(&expr), "$add(1, 2)");
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
        assert_eq!(test_gen().codegen_expr(&expr), "$add($x, $y)");
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
        assert_eq!(test_gen().codegen_expr(&expr), "($x + 1)");
    }

    #[test]
    fn test_codegen_function() {
        let pkg_gen = test_gen();
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
            is_test: false,
        };
        assert_eq!(
            pkg_gen.codegen_function(&func, &QualifiedPath::root().child(&func.name)),
            "function $test$square($x) { return ($x * $x); }"
        );
    }

    #[test]
    fn test_codegen_function_multiple_params() {
        let pkg_gen = test_gen();
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
            is_test: false,
        };
        assert_eq!(
            pkg_gen.codegen_function(&func, &QualifiedPath::root().child(&func.name)),
            "function $test$add($x, $y) { return ($x + $y); }"
        );
    }

    #[test]
    fn test_codegen_function_no_params() {
        let pkg_gen = test_gen();
        let func = TypedFunction {
            name: "answer".to_string(),
            params: vec![],
            body: TypedExpr::Int(42),
            return_type: Type::Int,
            is_builtin: false,
            is_test: false,
        };
        assert_eq!(
            pkg_gen.codegen_function(&func, &QualifiedPath::root().child(&func.name)),
            "function $test$answer() { return 42; }"
        );
    }

    #[test]
    fn test_codegen_bigint_function() {
        let pkg_gen = test_gen();
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
            is_test: false,
        };
        assert_eq!(
            pkg_gen.codegen_function(&func, &QualifiedPath::root().child(&func.name)),
            "function $test$big($x) { return ($x + 1n); }"
        );
    }
}
