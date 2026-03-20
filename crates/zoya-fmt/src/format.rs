use pretty::RcDoc;
use zoya_ast::{
    Attribute, AttributeArg, BinOp, EnumDef, EnumVariant, EnumVariantKind, Expr, FunctionDef,
    ImplBlock, ImplMethod, Item, LambdaParam, LetBinding, ListElement, ListPattern, MatchArm,
    ModDecl, Param, Path, PathPrefix, Pattern, StringPart, StructDef, StructFieldDef,
    StructFieldPattern, StructKind, TraitDef, TraitMethod, TupleElement, TuplePattern,
    TypeAliasDef, TypeAnnotation, UnaryOp, UseDecl, UsePath, UseTarget, Visibility,
};

const INDENT: isize = 2;

// --- Primitives ---

pub fn fmt_attributes(attrs: &[Attribute]) -> RcDoc<'static> {
    if attrs.is_empty() {
        return RcDoc::nil();
    }
    let docs: Vec<RcDoc<'static>> = attrs
        .iter()
        .map(|a| {
            let text = match &a.args {
                None => format!("#[{}]", a.name),
                Some(args) if args.is_empty() => format!("#[{}()]", a.name),
                Some(args) => {
                    let formatted: Vec<String> = args
                        .iter()
                        .map(|arg| match arg {
                            AttributeArg::Identifier(s) => s.clone(),
                            AttributeArg::String(s) => format!("\"{}\"", s),
                        })
                        .collect();
                    format!("#[{}({})]", a.name, formatted.join(", "))
                }
            };
            RcDoc::text(text)
        })
        .collect();
    RcDoc::intersperse(docs, RcDoc::hardline()).append(RcDoc::hardline())
}

pub fn fmt_leading_comments(comments: &[String]) -> RcDoc<'static> {
    if comments.is_empty() {
        return RcDoc::nil();
    }
    let docs: Vec<RcDoc<'static>> = comments
        .iter()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                RcDoc::text("//")
            } else {
                RcDoc::text(format!("// {trimmed}"))
            }
        })
        .collect();
    RcDoc::intersperse(docs, RcDoc::hardline()).append(RcDoc::hardline())
}

pub fn fmt_vis(vis: Visibility) -> RcDoc<'static> {
    match vis {
        Visibility::Public => RcDoc::text("pub "),
        Visibility::Private => RcDoc::nil(),
    }
}

pub fn fmt_type_params(params: &[String]) -> RcDoc<'static> {
    if params.is_empty() {
        RcDoc::nil()
    } else {
        RcDoc::text("<")
            .append(RcDoc::intersperse(
                params.iter().map(|p| RcDoc::text(p.clone())),
                RcDoc::text(", "),
            ))
            .append(RcDoc::text(">"))
    }
}

pub fn fmt_path_prefix(prefix: &PathPrefix) -> RcDoc<'static> {
    match prefix {
        PathPrefix::None => RcDoc::nil(),
        PathPrefix::Root => RcDoc::text("root::"),
        PathPrefix::Self_ => RcDoc::text("self::"),
        PathPrefix::Super => RcDoc::text("super::"),
        PathPrefix::Package(name) => RcDoc::text(format!("{name}::")),
    }
}

pub fn fmt_path(path: &Path) -> RcDoc<'static> {
    let doc = fmt_path_prefix(&path.prefix).append(RcDoc::intersperse(
        path.segments.iter().map(|s| RcDoc::text(s.clone())),
        RcDoc::text("::"),
    ));
    match &path.type_args {
        Some(args) => doc
            .append(RcDoc::text("::<"))
            .append(RcDoc::intersperse(
                args.iter().map(fmt_type_annotation),
                RcDoc::text(", "),
            ))
            .append(RcDoc::text(">")),
        None => doc,
    }
}

// --- Types ---

pub fn fmt_type_annotation(ta: &TypeAnnotation) -> RcDoc<'static> {
    match ta {
        TypeAnnotation::Named(path) => fmt_path(path),
        TypeAnnotation::Parameterized(path, params) => fmt_path(path)
            .append(RcDoc::text("<"))
            .append(RcDoc::intersperse(
                params.iter().map(fmt_type_annotation),
                RcDoc::text(", "),
            ))
            .append(RcDoc::text(">")),
        TypeAnnotation::Tuple(elems) => RcDoc::text("(")
            .append(RcDoc::intersperse(
                elems.iter().map(fmt_type_annotation),
                RcDoc::text(", "),
            ))
            .append(RcDoc::text(")")),
        TypeAnnotation::Function(params, ret) => {
            if params.len() == 1 {
                fmt_type_annotation(&params[0])
                    .append(RcDoc::text(" -> "))
                    .append(fmt_type_annotation(ret))
            } else {
                RcDoc::text("(")
                    .append(RcDoc::intersperse(
                        params.iter().map(fmt_type_annotation),
                        RcDoc::text(", "),
                    ))
                    .append(RcDoc::text(") -> "))
                    .append(fmt_type_annotation(ret))
            }
        }
    }
}

// --- Mod & Use declarations ---

pub fn fmt_mod_decl(m: &ModDecl) -> RcDoc<'static> {
    fmt_leading_comments(&m.leading_comments)
        .append(fmt_attributes(&m.attributes))
        .append(fmt_vis(m.visibility))
        .append(RcDoc::text("mod "))
        .append(RcDoc::text(m.name.clone()))
}

pub fn fmt_use_decl(u: &UseDecl) -> RcDoc<'static> {
    fmt_leading_comments(&u.leading_comments)
        .append(fmt_attributes(&u.attributes))
        .append(fmt_vis(u.visibility))
        .append(RcDoc::text("use "))
        .append(fmt_use_path(&u.path))
}

fn fmt_use_path(path: &UsePath) -> RcDoc<'static> {
    let prefix_doc = fmt_path_prefix(&path.prefix);
    let segs_doc = if path.segments.is_empty() {
        RcDoc::nil()
    } else {
        RcDoc::intersperse(
            path.segments.iter().map(|s| RcDoc::text(s.clone())),
            RcDoc::text("::"),
        )
    };
    let target_doc = fmt_use_target(&path.target, !path.segments.is_empty());
    prefix_doc.append(segs_doc).append(target_doc)
}

fn fmt_use_target(target: &UseTarget, has_segments: bool) -> RcDoc<'static> {
    match target {
        UseTarget::Single { alias: None } => RcDoc::nil(),
        UseTarget::Single {
            alias: Some(alias), ..
        } => RcDoc::text(" as ").append(RcDoc::text(alias.clone())),
        UseTarget::Glob => {
            let sep = if has_segments { "::" } else { "" };
            RcDoc::text(format!("{sep}*"))
        }
        UseTarget::Group(items) => {
            let sep = if has_segments { "::" } else { "" };
            RcDoc::text(format!("{sep}{{"))
                .append(RcDoc::intersperse(
                    items.iter().map(|item| {
                        let doc = RcDoc::text(item.name.clone());
                        match &item.alias {
                            Some(alias) => doc
                                .append(RcDoc::text(" as "))
                                .append(RcDoc::text(alias.clone())),
                            None => doc,
                        }
                    }),
                    RcDoc::text(", "),
                ))
                .append(RcDoc::text("}"))
        }
    }
}

// --- Type alias ---

pub fn fmt_type_alias(ta: &TypeAliasDef) -> RcDoc<'static> {
    fmt_leading_comments(&ta.leading_comments)
        .append(fmt_attributes(&ta.attributes))
        .append(fmt_vis(ta.visibility))
        .append(RcDoc::text("type "))
        .append(RcDoc::text(ta.name.clone()))
        .append(fmt_type_params(&ta.type_params))
        .append(RcDoc::text(" = "))
        .append(fmt_type_annotation(&ta.typ))
}

// --- Struct ---

pub fn fmt_struct(s: &StructDef) -> RcDoc<'static> {
    let doc = fmt_leading_comments(&s.leading_comments)
        .append(fmt_attributes(&s.attributes))
        .append(fmt_vis(s.visibility))
        .append(RcDoc::text("struct "))
        .append(RcDoc::text(s.name.clone()))
        .append(fmt_type_params(&s.type_params));
    match &s.kind {
        StructKind::Unit => doc,
        StructKind::Named(fields) if !fields.is_empty() => doc
            .append(RcDoc::text(" "))
            .append(fmt_struct_fields(fields)),
        StructKind::Named(_) => doc,
        StructKind::Tuple(types) => {
            let entries: Vec<RcDoc<'static>> = types.iter().map(fmt_type_annotation).collect();
            doc.append(paren_list(entries, RcDoc::text(",")))
        }
    }
}

fn fmt_struct_fields(fields: &[StructFieldDef]) -> RcDoc<'static> {
    if fields.is_empty() {
        return RcDoc::nil();
    }
    let entries: Vec<RcDoc<'static>> = fields
        .iter()
        .map(|f| {
            RcDoc::text(f.name.clone())
                .append(RcDoc::text(": "))
                .append(fmt_type_annotation(&f.typ))
        })
        .collect();
    braced_list(entries, RcDoc::text(","))
}

// --- Enum ---

pub fn fmt_enum(e: &EnumDef) -> RcDoc<'static> {
    fmt_leading_comments(&e.leading_comments)
        .append(fmt_attributes(&e.attributes))
        .append(fmt_vis(e.visibility))
        .append(RcDoc::text("enum "))
        .append(RcDoc::text(e.name.clone()))
        .append(fmt_type_params(&e.type_params))
        .append(RcDoc::text(" "))
        .append(fmt_enum_variants(&e.variants))
}

fn fmt_enum_variants(variants: &[EnumVariant]) -> RcDoc<'static> {
    if variants.is_empty() {
        return RcDoc::text("{}");
    }
    let entries: Vec<RcDoc<'static>> = variants.iter().map(fmt_enum_variant).collect();
    braced_list(entries, RcDoc::text(","))
}

fn fmt_enum_variant(v: &EnumVariant) -> RcDoc<'static> {
    let name = RcDoc::text(v.name.clone());
    match &v.kind {
        EnumVariantKind::Unit => name,
        EnumVariantKind::Tuple(types) => name.append(RcDoc::text("(")).append(
            RcDoc::intersperse(types.iter().map(fmt_type_annotation), RcDoc::text(", "))
                .append(RcDoc::text(")")),
        ),
        EnumVariantKind::Struct(fields) => name
            .append(RcDoc::text(" "))
            .append(fmt_struct_fields(fields)),
    }
}

// --- Patterns ---

pub fn fmt_pattern(pat: &Pattern) -> RcDoc<'static> {
    match pat {
        Pattern::Literal(expr) => fmt_expr(expr),
        Pattern::Wildcard => RcDoc::text("_"),
        Pattern::List(lp) => fmt_list_pattern(lp),
        Pattern::Tuple(tp) => fmt_tuple_pattern(tp),
        Pattern::Path(path) => fmt_path(path),
        Pattern::Call { path, args } => fmt_path(path).append(fmt_call_pattern_args(args)),
        Pattern::Struct {
            path,
            fields,
            is_partial,
        } => fmt_struct_pattern(path, fields, *is_partial),
        Pattern::As { name, pattern } => RcDoc::text(name.clone())
            .append(RcDoc::text(" @ "))
            .append(fmt_pattern(pattern)),
    }
}

fn fmt_rest_binding(binding: &Option<String>) -> RcDoc<'static> {
    match binding {
        Some(name) => RcDoc::text(format!("{name} @ ..")),
        None => RcDoc::text(".."),
    }
}

fn fmt_list_pattern(lp: &ListPattern) -> RcDoc<'static> {
    match lp {
        ListPattern::Empty => RcDoc::text("[]"),
        ListPattern::Exact(pats) => RcDoc::text("[")
            .append(RcDoc::intersperse(
                pats.iter().map(fmt_pattern),
                RcDoc::text(", "),
            ))
            .append(RcDoc::text("]")),
        ListPattern::Prefix {
            patterns,
            rest_binding,
        } => {
            let mut parts: Vec<RcDoc<'static>> = patterns.iter().map(fmt_pattern).collect();
            parts.push(fmt_rest_binding(rest_binding));
            RcDoc::text("[")
                .append(RcDoc::intersperse(parts, RcDoc::text(", ")))
                .append(RcDoc::text("]"))
        }
        ListPattern::Suffix {
            patterns,
            rest_binding,
        } => {
            let mut parts: Vec<RcDoc<'static>> = vec![fmt_rest_binding(rest_binding)];
            parts.extend(patterns.iter().map(fmt_pattern));
            RcDoc::text("[")
                .append(RcDoc::intersperse(parts, RcDoc::text(", ")))
                .append(RcDoc::text("]"))
        }
        ListPattern::PrefixSuffix {
            prefix,
            suffix,
            rest_binding,
        } => {
            let mut parts: Vec<RcDoc<'static>> = prefix.iter().map(fmt_pattern).collect();
            parts.push(fmt_rest_binding(rest_binding));
            parts.extend(suffix.iter().map(fmt_pattern));
            RcDoc::text("[")
                .append(RcDoc::intersperse(parts, RcDoc::text(", ")))
                .append(RcDoc::text("]"))
        }
    }
}

fn fmt_tuple_pattern_inner(tp: &TuplePattern) -> Vec<RcDoc<'static>> {
    match tp {
        TuplePattern::Empty => vec![],
        TuplePattern::Exact(pats) => pats.iter().map(fmt_pattern).collect(),
        TuplePattern::Prefix {
            patterns,
            rest_binding,
        } => {
            let mut parts: Vec<RcDoc<'static>> = patterns.iter().map(fmt_pattern).collect();
            parts.push(fmt_rest_binding(rest_binding));
            parts
        }
        TuplePattern::Suffix {
            patterns,
            rest_binding,
        } => {
            let mut parts: Vec<RcDoc<'static>> = vec![fmt_rest_binding(rest_binding)];
            parts.extend(patterns.iter().map(fmt_pattern));
            parts
        }
        TuplePattern::PrefixSuffix {
            prefix,
            suffix,
            rest_binding,
        } => {
            let mut parts: Vec<RcDoc<'static>> = prefix.iter().map(fmt_pattern).collect();
            parts.push(fmt_rest_binding(rest_binding));
            parts.extend(suffix.iter().map(fmt_pattern));
            parts
        }
    }
}

fn fmt_tuple_pattern(tp: &TuplePattern) -> RcDoc<'static> {
    let parts = fmt_tuple_pattern_inner(tp);
    if parts.is_empty() {
        return RcDoc::text("()");
    }
    // Single-element exact tuple needs trailing comma
    let needs_trailing_comma = matches!(tp, TuplePattern::Exact(pats) if pats.len() == 1);
    let inner = RcDoc::intersperse(parts, RcDoc::text(", "));
    let inner = if needs_trailing_comma {
        inner.append(RcDoc::text(","))
    } else {
        inner
    };
    RcDoc::text("(").append(inner).append(RcDoc::text(")"))
}

fn fmt_call_pattern_args(tp: &TuplePattern) -> RcDoc<'static> {
    let parts = fmt_tuple_pattern_inner(tp);
    if parts.is_empty() {
        return RcDoc::text("()");
    }
    RcDoc::text("(")
        .append(RcDoc::intersperse(parts, RcDoc::text(", ")))
        .append(RcDoc::text(")"))
}

fn fmt_struct_pattern(
    path: &Path,
    fields: &[StructFieldPattern],
    is_partial: bool,
) -> RcDoc<'static> {
    let name_doc = fmt_path(path).append(RcDoc::text(" "));

    if fields.is_empty() && is_partial {
        return name_doc.append(RcDoc::text("{ .. }"));
    }
    if fields.is_empty() && !is_partial {
        return name_doc.append(RcDoc::text("{}"));
    }

    let mut entries: Vec<RcDoc<'static>> = fields.iter().map(fmt_struct_field_pattern).collect();
    if is_partial {
        entries.push(RcDoc::text(".."));
    }
    name_doc.append(braced_list(entries, RcDoc::text(",")))
}

fn fmt_struct_field_pattern(f: &StructFieldPattern) -> RcDoc<'static> {
    // Check for shorthand: `field_name` is shorthand for `field_name: field_name`
    if is_shorthand_field_pattern(f) {
        RcDoc::text(f.field_name.clone())
    } else {
        RcDoc::text(f.field_name.clone())
            .append(RcDoc::text(": "))
            .append(fmt_pattern(&f.pattern))
    }
}

fn is_shorthand_field_pattern(f: &StructFieldPattern) -> bool {
    matches!(&*f.pattern, Pattern::Path(p) if p.is_simple() && p.prefix == PathPrefix::None && p.type_args.is_none() && p.segments[0] == f.field_name)
}

// --- Expressions ---

pub fn fmt_expr(expr: &Expr) -> RcDoc<'static> {
    match expr {
        Expr::Int(n) => RcDoc::text(n.to_string()),
        Expr::BigInt(n) => RcDoc::text(format!("{n}n")),
        Expr::Float(f) => fmt_float(*f),
        Expr::Bool(b) => RcDoc::text(if *b { "true" } else { "false" }),
        Expr::String(s) => fmt_string(s),
        Expr::List(elems) => fmt_list_expr(elems),
        Expr::Tuple(elems) => fmt_tuple_expr(elems),
        Expr::Path(path) => fmt_path(path),
        Expr::Call { path, args } => fmt_call_expr(path, args),
        Expr::UnaryOp { op, expr } => fmt_unary_op(*op, expr),
        Expr::BinOp { op, left, right } => fmt_bin_op(*op, left, right, 0),
        Expr::Block { bindings, result } => fmt_block_expr(bindings, result),
        Expr::Match { scrutinee, arms } => fmt_match_expr(scrutinee, arms),
        Expr::MethodCall {
            receiver,
            method,
            args,
        } => fmt_method_call(receiver, method, args),
        Expr::Lambda {
            params,
            return_type,
            body,
        } => fmt_lambda(params, return_type.as_ref(), body),
        Expr::Struct {
            path,
            fields,
            spread,
        } => fmt_struct_expr(path, fields, spread.as_deref()),
        Expr::FieldAccess { expr, field } => fmt_field_access(expr, field),
        Expr::TupleIndex { expr, index } => fmt_tuple_index(expr, *index),
        Expr::ListIndex { expr, index } => fmt_list_index(expr, index),
        Expr::InterpolatedString(parts) => {
            let mut doc = RcDoc::text("$\"");
            for part in parts {
                match part {
                    StringPart::Literal(s) => {
                        // Escape special chars back to source form
                        let escaped = s
                            .replace('\\', "\\\\")
                            .replace('"', "\\\"")
                            .replace('{', "\\{")
                            .replace('}', "\\}")
                            .replace('\n', "\\n")
                            .replace('\t', "\\t")
                            .replace('\r', "\\r");
                        doc = doc.append(RcDoc::text(escaped));
                    }
                    StringPart::Expr(expr) => {
                        doc = doc
                            .append(RcDoc::text("{"))
                            .append(fmt_expr(expr))
                            .append(RcDoc::text("}"));
                    }
                }
            }
            doc.append(RcDoc::text("\""))
        }
    }
}

fn fmt_float(f: f64) -> RcDoc<'static> {
    let s = f.to_string();
    if s.contains('.') {
        RcDoc::text(s)
    } else {
        RcDoc::text(format!("{s}.0"))
    }
}

fn fmt_string(s: &str) -> RcDoc<'static> {
    let mut escaped = String::with_capacity(s.len() + 2);
    escaped.push('"');
    for c in s.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\t' => escaped.push_str("\\t"),
            '\r' => escaped.push_str("\\r"),
            _ => escaped.push(c),
        }
    }
    escaped.push('"');
    RcDoc::text(escaped)
}

fn fmt_list_expr(elems: &[ListElement]) -> RcDoc<'static> {
    if elems.is_empty() {
        return RcDoc::text("[]");
    }
    let entries: Vec<RcDoc<'static>> = elems
        .iter()
        .map(|e| match e {
            ListElement::Item(expr) => fmt_expr(expr),
            ListElement::Spread(expr) => RcDoc::text("..").append(fmt_expr(expr)),
        })
        .collect();
    bracketed_list(entries, RcDoc::text(","))
}

fn fmt_tuple_element(e: &TupleElement) -> RcDoc<'static> {
    match e {
        TupleElement::Item(expr) => fmt_expr(expr),
        TupleElement::Spread(expr) => RcDoc::text("..").append(fmt_expr(expr)),
    }
}

fn fmt_tuple_expr(elems: &[TupleElement]) -> RcDoc<'static> {
    if elems.is_empty() {
        return RcDoc::text("()");
    }
    // Single-element tuple needs trailing comma (only for non-spread items)
    if elems.len() == 1 && matches!(&elems[0], TupleElement::Item(_)) {
        return RcDoc::text("(")
            .append(fmt_tuple_element(&elems[0]))
            .append(RcDoc::text(",)"));
    }
    let entries: Vec<RcDoc<'static>> = elems.iter().map(fmt_tuple_element).collect();
    paren_list(entries, RcDoc::text(","))
}

fn fmt_call_expr(path: &Path, args: &[Expr]) -> RcDoc<'static> {
    let entries: Vec<RcDoc<'static>> = args.iter().map(fmt_expr).collect();
    fmt_path(path).append(paren_list(entries, RcDoc::text(",")))
}

fn fmt_unary_op(op: UnaryOp, expr: &Expr) -> RcDoc<'static> {
    let op_str = match op {
        UnaryOp::Neg => "-",
    };
    // Wrap in parens if the inner expression is a binop
    let inner = match expr {
        Expr::BinOp { .. } => RcDoc::text("(")
            .append(fmt_expr(expr))
            .append(RcDoc::text(")")),
        _ => fmt_expr(expr),
    };
    RcDoc::text(op_str).append(inner)
}

fn binop_precedence(op: BinOp) -> u8 {
    match op {
        BinOp::Pow => 4,
        BinOp::Mul | BinOp::Div | BinOp::Mod => 3,
        BinOp::Add | BinOp::Sub => 2,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => 1,
    }
}

fn binop_str(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Pow => "**",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "<=",
        BinOp::Ge => ">=",
    }
}

fn is_right_associative(op: BinOp) -> bool {
    matches!(op, BinOp::Pow)
}

fn fmt_bin_op(op: BinOp, left: &Expr, right: &Expr, parent_prec: u8) -> RcDoc<'static> {
    let prec = binop_precedence(op);
    let right_assoc = is_right_associative(op);
    let left_doc = match left {
        Expr::BinOp {
            op: child_op,
            left: cl,
            right: cr,
        } => {
            let child_prec = binop_precedence(*child_op);
            // For right-associative ops, left child at same precedence needs parens
            if child_prec < prec || (right_assoc && child_prec == prec) {
                RcDoc::text("(")
                    .append(fmt_bin_op(*child_op, cl, cr, 0))
                    .append(RcDoc::text(")"))
            } else {
                fmt_bin_op(*child_op, cl, cr, prec)
            }
        }
        _ => fmt_expr(left),
    };
    let right_doc = match right {
        Expr::BinOp {
            op: child_op,
            left: cl,
            right: cr,
        } => {
            let child_prec = binop_precedence(*child_op);
            // For right-associative ops, right child at same precedence does NOT need parens
            if child_prec < prec || (!right_assoc && child_prec == prec) {
                RcDoc::text("(")
                    .append(fmt_bin_op(*child_op, cl, cr, 0))
                    .append(RcDoc::text(")"))
            } else {
                fmt_bin_op(*child_op, cl, cr, prec)
            }
        }
        _ => fmt_expr(right),
    };
    let doc = left_doc
        .append(RcDoc::text(format!(" {} ", binop_str(op))))
        .append(right_doc);
    if prec < parent_prec {
        RcDoc::text("(").append(doc).append(RcDoc::text(")"))
    } else {
        doc
    }
}

fn fmt_block_expr(bindings: &[LetBinding], result: &Expr) -> RcDoc<'static> {
    let mut parts = Vec::new();
    for b in bindings {
        parts.push(fmt_let_binding(b).append(RcDoc::text(";")));
    }
    parts.push(fmt_expr(result));

    RcDoc::text("{")
        .append(
            RcDoc::hardline()
                .append(RcDoc::intersperse(parts, RcDoc::hardline()))
                .nest(INDENT),
        )
        .append(RcDoc::hardline())
        .append(RcDoc::text("}"))
}

fn fmt_let_binding(b: &LetBinding) -> RcDoc<'static> {
    let doc = RcDoc::text("let ").append(fmt_pattern(&b.pattern));
    let doc = match &b.type_annotation {
        Some(ta) => doc
            .append(RcDoc::text(": "))
            .append(fmt_type_annotation(ta)),
        None => doc,
    };
    doc.append(RcDoc::text(" = ")).append(fmt_expr(&b.value))
}

fn fmt_match_expr(scrutinee: &Expr, arms: &[MatchArm]) -> RcDoc<'static> {
    let scrutinee_doc = fmt_expr(scrutinee);
    let arms_doc: Vec<RcDoc<'static>> = arms
        .iter()
        .map(|arm| fmt_match_arm(arm).append(RcDoc::text(",")))
        .collect();

    RcDoc::text("match ")
        .append(scrutinee_doc)
        .append(RcDoc::text(" {"))
        .append(
            RcDoc::hardline()
                .append(RcDoc::intersperse(arms_doc, RcDoc::hardline()))
                .nest(INDENT),
        )
        .append(RcDoc::hardline())
        .append(RcDoc::text("}"))
}

fn fmt_match_arm(arm: &MatchArm) -> RcDoc<'static> {
    let pat_doc = fmt_pattern(&arm.pattern);
    let body_doc = fmt_body(&arm.result);
    pat_doc.append(RcDoc::text(" => ")).append(body_doc)
}

fn fmt_method_call(receiver: &Expr, method: &str, args: &[Expr]) -> RcDoc<'static> {
    let receiver_doc = fmt_expr_needs_parens_for_postfix(receiver);
    let entries: Vec<RcDoc<'static>> = args.iter().map(fmt_expr).collect();
    receiver_doc
        .append(RcDoc::text(format!(".{method}")))
        .append(paren_list(entries, RcDoc::text(",")))
}

fn fmt_lambda(
    params: &[LambdaParam],
    return_type: Option<&TypeAnnotation>,
    body: &Expr,
) -> RcDoc<'static> {
    let params_doc = RcDoc::text("|")
        .append(RcDoc::intersperse(
            params.iter().map(|p| {
                let pat = fmt_pattern(&p.pattern);
                match &p.typ {
                    Some(ta) => pat
                        .append(RcDoc::text(": "))
                        .append(fmt_type_annotation(ta)),
                    None => pat,
                }
            }),
            RcDoc::text(", "),
        ))
        .append(RcDoc::text("|"));
    let ret_doc = match return_type {
        Some(ta) => RcDoc::text(" -> ").append(fmt_type_annotation(ta)),
        None => RcDoc::nil(),
    };
    let body_doc = RcDoc::text(" ").append(fmt_body(body));
    params_doc.append(ret_doc).append(body_doc)
}

fn fmt_struct_expr(
    path: &Path,
    fields: &[(String, Expr)],
    spread: Option<&Expr>,
) -> RcDoc<'static> {
    let name_doc = fmt_path(path).append(RcDoc::text(" "));
    if fields.is_empty() && spread.is_none() {
        return name_doc.append(RcDoc::text("{}"));
    }
    let mut entries: Vec<RcDoc<'static>> = fields
        .iter()
        .map(|(name, value)| {
            // Check for shorthand: `x` is shorthand for `x: x`
            if is_shorthand_field_expr(name, value) {
                RcDoc::text(name.clone())
            } else {
                RcDoc::text(name.clone())
                    .append(RcDoc::text(": "))
                    .append(fmt_expr(value))
            }
        })
        .collect();
    if let Some(spread_expr) = spread {
        entries.push(RcDoc::text("..").append(fmt_expr(spread_expr)));
    }
    name_doc.append(braced_list(entries, RcDoc::text(",")))
}

fn is_shorthand_field_expr(name: &str, value: &Expr) -> bool {
    matches!(value, Expr::Path(p) if p.is_simple() && p.prefix == PathPrefix::None && p.type_args.is_none() && p.segments[0] == name)
}

fn fmt_list_index(expr: &Expr, index: &Expr) -> RcDoc<'static> {
    let expr_doc = fmt_expr_needs_parens_for_postfix(expr);
    expr_doc
        .append(RcDoc::text("["))
        .append(fmt_expr(index))
        .append(RcDoc::text("]"))
}

fn fmt_field_access(expr: &Expr, field: &str) -> RcDoc<'static> {
    let expr_doc = fmt_expr_needs_parens_for_postfix(expr);
    expr_doc.append(RcDoc::text(format!(".{field}")))
}

fn fmt_tuple_index(expr: &Expr, index: u64) -> RcDoc<'static> {
    let expr_doc = fmt_expr_needs_parens_for_postfix(expr);
    expr_doc.append(RcDoc::text(format!(".{index}")))
}

/// Wrap expressions that need parentheses when used as receivers for `.method()` or `.field`
fn fmt_expr_needs_parens_for_postfix(expr: &Expr) -> RcDoc<'static> {
    match expr {
        Expr::BinOp { .. } | Expr::UnaryOp { .. } | Expr::Lambda { .. } => RcDoc::text("(")
            .append(fmt_expr(expr))
            .append(RcDoc::text(")")),
        _ => fmt_expr(expr),
    }
}

// --- Function ---

pub fn fmt_function(f: &FunctionDef) -> RcDoc<'static> {
    let sig = fmt_leading_comments(&f.leading_comments)
        .append(fmt_attributes(&f.attributes))
        .append(fmt_vis(f.visibility))
        .append(RcDoc::text("fn "))
        .append(RcDoc::text(f.name.clone()))
        .append(fmt_type_params(&f.type_params))
        .append(fmt_params(&f.params))
        .append(fmt_return_type(&f.return_type));
    let body_doc = RcDoc::text(" ").append(fmt_body(&f.body));
    sig.append(body_doc)
}

fn fmt_params(params: &[Param]) -> RcDoc<'static> {
    let entries: Vec<RcDoc<'static>> = params
        .iter()
        .map(|p| {
            fmt_pattern(&p.pattern)
                .append(RcDoc::text(": "))
                .append(fmt_type_annotation(&p.typ))
        })
        .collect();
    paren_list(entries, RcDoc::text(","))
}

fn fmt_return_type(rt: &Option<TypeAnnotation>) -> RcDoc<'static> {
    match rt {
        Some(ta) => RcDoc::text(" -> ").append(fmt_type_annotation(ta)),
        None => RcDoc::nil(),
    }
}

/// Format a body expression with the no-braces heuristic.
/// - Block: always braces (mandatory, has let bindings)
/// - Match: always braces (multi-line, reads better in braces)
/// - Other: no braces
fn fmt_body(expr: &Expr) -> RcDoc<'static> {
    match expr {
        Expr::Block { bindings, result } => fmt_block_expr(bindings, result),
        Expr::Match { scrutinee, arms } => RcDoc::text("{")
            .append(
                RcDoc::hardline()
                    .append(fmt_match_expr(scrutinee, arms))
                    .nest(INDENT),
            )
            .append(RcDoc::hardline())
            .append(RcDoc::text("}")),
        _ => fmt_expr(expr),
    }
}

// --- Item dispatch ---

pub fn fmt_item(item: &Item) -> RcDoc<'static> {
    match item {
        Item::Function(f) => fmt_function(f),
        Item::Struct(s) => fmt_struct(s),
        Item::Enum(e) => fmt_enum(e),
        Item::TypeAlias(ta) => fmt_type_alias(ta),
        Item::Use(u) => fmt_use_decl(u),
        Item::Impl(i) => fmt_impl_block(i),
        Item::Trait(t) => fmt_trait_def(t),
        Item::ModDecl(m) => fmt_mod_decl(m),
    }
}

fn fmt_impl_block(i: &ImplBlock) -> RcDoc<'static> {
    let mut header = fmt_leading_comments(&i.leading_comments)
        .append(fmt_attributes(&i.attributes))
        .append(RcDoc::text("impl"))
        .append(fmt_type_params(&i.type_params));

    if let Some(ref trait_path) = i.trait_path {
        header = header
            .append(RcDoc::text(" "))
            .append(RcDoc::text(trait_path.to_string()))
            .append(RcDoc::text(" for"));
    }

    header = header
        .append(RcDoc::text(" "))
        .append(fmt_type_annotation(&i.target_type));

    if i.methods.is_empty() {
        return header.append(RcDoc::text(" {}"));
    }

    let methods = RcDoc::intersperse(
        i.methods.iter().map(fmt_impl_method),
        RcDoc::hardline().append(RcDoc::hardline()),
    );

    header
        .append(RcDoc::text(" {"))
        .append(RcDoc::hardline().append(methods).nest(INDENT))
        .append(RcDoc::hardline())
        .append(RcDoc::text("}"))
}

fn fmt_impl_method(m: &ImplMethod) -> RcDoc<'static> {
    let sig = fmt_leading_comments(&m.leading_comments)
        .append(fmt_attributes(&m.attributes))
        .append(fmt_vis(m.visibility))
        .append(RcDoc::text("fn "))
        .append(RcDoc::text(m.name.clone()))
        .append(fmt_type_params(&m.type_params))
        .append(fmt_impl_method_params(m.has_self, &m.params))
        .append(fmt_return_type(&m.return_type));
    let body_doc = RcDoc::text(" ").append(fmt_body(&m.body));
    sig.append(body_doc)
}

fn fmt_trait_def(t: &TraitDef) -> RcDoc<'static> {
    let header = fmt_leading_comments(&t.leading_comments)
        .append(fmt_attributes(&t.attributes))
        .append(fmt_vis(t.visibility))
        .append(RcDoc::text("trait "))
        .append(RcDoc::text(t.name.clone()))
        .append(fmt_type_params(&t.type_params));

    if t.methods.is_empty() {
        return header.append(RcDoc::text(" {}"));
    }

    let methods = RcDoc::intersperse(
        t.methods.iter().map(fmt_trait_method),
        RcDoc::hardline().append(RcDoc::hardline()),
    );

    header
        .append(RcDoc::text(" {"))
        .append(RcDoc::hardline().append(methods).nest(INDENT))
        .append(RcDoc::hardline())
        .append(RcDoc::text("}"))
}

fn fmt_trait_method(m: &TraitMethod) -> RcDoc<'static> {
    let sig = fmt_leading_comments(&m.leading_comments)
        .append(RcDoc::text("fn "))
        .append(RcDoc::text(m.name.clone()))
        .append(fmt_type_params(&m.type_params))
        .append(fmt_impl_method_params(m.has_self, &m.params))
        .append(fmt_return_type(&m.return_type));
    match &m.body {
        Some(body) => sig.append(RcDoc::text(" ")).append(fmt_body(body)),
        None => sig,
    }
}

fn fmt_impl_method_params(has_self: bool, params: &[Param]) -> RcDoc<'static> {
    let mut entries: Vec<RcDoc<'static>> = Vec::new();
    if has_self {
        entries.push(RcDoc::text("self"));
    }
    for p in params {
        entries.push(
            fmt_pattern(&p.pattern)
                .append(RcDoc::text(": "))
                .append(fmt_type_annotation(&p.typ)),
        );
    }
    paren_list(entries, RcDoc::text(","))
}

// --- Helpers for grouped layout ---

/// `{ entry, entry, ... }` with group-based line breaking
fn braced_list(entries: Vec<RcDoc<'static>>, sep: RcDoc<'static>) -> RcDoc<'static> {
    let len = entries.len();
    let body = RcDoc::intersperse(
        entries.into_iter().enumerate().map(|(i, e)| {
            if i < len - 1 {
                e.append(sep.clone())
            } else {
                e
            }
        }),
        RcDoc::line(),
    );
    RcDoc::text("{")
        .append(
            RcDoc::line()
                .append(body)
                .nest(INDENT)
                .append(RcDoc::line()),
        )
        .append(RcDoc::text("}"))
        .group()
}

/// `[ entry, entry, ... ]` with group-based line breaking
fn bracketed_list(entries: Vec<RcDoc<'static>>, sep: RcDoc<'static>) -> RcDoc<'static> {
    let len = entries.len();
    let body = RcDoc::intersperse(
        entries.into_iter().enumerate().map(|(i, e)| {
            if i < len - 1 {
                e.append(sep.clone())
            } else {
                e
            }
        }),
        RcDoc::line(),
    );
    RcDoc::text("[")
        .append(
            RcDoc::line_()
                .append(body)
                .nest(INDENT)
                .append(RcDoc::line_()),
        )
        .append(RcDoc::text("]"))
        .group()
}

/// `( entry, entry, ... )` with group-based line breaking
fn paren_list(entries: Vec<RcDoc<'static>>, sep: RcDoc<'static>) -> RcDoc<'static> {
    if entries.is_empty() {
        return RcDoc::text("()");
    }
    let len = entries.len();
    let body = RcDoc::intersperse(
        entries.into_iter().enumerate().map(|(i, e)| {
            if i < len - 1 {
                e.append(sep.clone())
            } else {
                e
            }
        }),
        RcDoc::line(),
    );
    RcDoc::text("(")
        .append(
            RcDoc::line_()
                .append(body)
                .nest(INDENT)
                .append(RcDoc::line_()),
        )
        .append(RcDoc::text(")"))
        .group()
}
