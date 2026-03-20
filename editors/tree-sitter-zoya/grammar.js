/// <reference types="tree-sitter-cli/dsl" />

const PREC = {
  COMPARE: 1,
  SUM: 2,
  PRODUCT: 3,
  POWER: 4,
  UNARY: 5,
  POSTFIX: 6,
  CALL: 7,
  PATH: 8,
};

module.exports = grammar({
  name: "zoya",

  externals: ($) => [
    $._interpolated_string_start,
    $._interpolated_string_content,
    $._interpolated_string_expr_start,
    $._interpolated_string_expr_end,
    $._interpolated_string_end,
  ],

  extras: ($) => [/\s/, $.line_comment],

  word: ($) => $.identifier,

  conflicts: ($) => [
    // In `match expr { ... }`, `{` after the scrutinee could start a struct_constructor
    // or the match body. prec.dynamic(-1) on struct_constructor resolves this.
    [$._expression, $.struct_constructor],
  ],

  rules: {
    source_file: ($) => repeat($._item),

    // ── Comments ──────────────────────────────────────────────────────
    line_comment: (_) => token(seq("//", /.*/)),

    // ── Identifiers ───────────────────────────────────────────────────
    identifier: (_) => /[a-zA-Z_][a-zA-Z0-9_]*/,

    // ── Items ─────────────────────────────────────────────────────────
    _item: ($) =>
      choice(
        $.function_definition,
        $.struct_definition,
        $.enum_definition,
        $.type_alias,
        $.mod_declaration,
        $.use_declaration,
        $.impl_block,
        $.trait_definition,
      ),

    visibility: (_) => "pub",

    attribute: ($) =>
      seq(
        "#",
        "[",
        field("name", $.identifier),
        optional(seq("(", commaSep1(choice($.identifier, $.string)), ")")),
        "]",
      ),

    type_parameters: ($) => seq("<", commaSep1($.identifier), ">"),

    // ── Function Definition ───────────────────────────────────────────
    function_definition: ($) =>
      seq(
        repeat($.attribute),
        optional($.visibility),
        "fn",
        field("name", $.identifier),
        optional($.type_parameters),
        "(",
        commaSep($.parameter),
        ")",
        optional(seq("->", field("return_type", $._type))),
        field("body", $._expression),
      ),

    parameter: ($) =>
      choice(
        seq(field("pattern", $._pattern), ":", field("type", $._type)),
        "self",
      ),

    // ── Struct Definition ─────────────────────────────────────────────
    struct_definition: ($) =>
      seq(
        repeat($.attribute),
        optional($.visibility),
        "struct",
        field("name", $.identifier),
        optional($.type_parameters),
        optional(
          choice($.struct_named_fields, $.struct_tuple_fields),
        ),
      ),

    struct_named_fields: ($) =>
      seq("{", commaSep($.struct_field), "}"),

    struct_field: ($) =>
      seq(field("name", $.identifier), ":", field("type", $._type)),

    struct_tuple_fields: ($) => seq("(", commaSep1($._type), ")"),

    // ── Enum Definition ───────────────────────────────────────────────
    enum_definition: ($) =>
      seq(
        repeat($.attribute),
        optional($.visibility),
        "enum",
        field("name", $.identifier),
        optional($.type_parameters),
        "{",
        commaSep1($.enum_variant),
        "}",
      ),

    enum_variant: ($) =>
      seq(
        field("name", $.identifier),
        optional(
          choice(
            seq("(", commaSep1($._type), ")"),
            seq("{", commaSep1($.struct_field), "}"),
          ),
        ),
      ),

    // ── Type Alias ────────────────────────────────────────────────────
    type_alias: ($) =>
      seq(
        repeat($.attribute),
        optional($.visibility),
        "type",
        field("name", $.identifier),
        optional($.type_parameters),
        "=",
        field("type", $._type),
      ),

    // ── Module Declaration ────────────────────────────────────────────
    mod_declaration: ($) =>
      seq(
        repeat($.attribute),
        optional($.visibility),
        "mod",
        field("name", $.identifier),
      ),

    // ── Use Declaration ───────────────────────────────────────────────
    use_declaration: ($) =>
      seq(
        repeat($.attribute),
        optional($.visibility),
        "use",
        $.use_path,
      ),

    use_path: ($) =>
      seq(
        optional(seq($._path_prefix, "::")),
        sepBy1("::", $.identifier),
        optional(
          seq(
            "::",
            choice("*", seq("{", commaSep1($.identifier), "}")),
          ),
        ),
      ),

    _path_prefix: ($) => choice("root", "self", "super"),

    // ── Trait Definition ────────────────────────────────────────────────
    trait_definition: ($) =>
      seq(
        repeat($.attribute),
        optional($.visibility),
        "trait",
        field("name", $.identifier),
        optional($.type_parameters),
        "{",
        repeat($.trait_method),
        "}",
      ),

    trait_method: ($) =>
      seq(
        "fn",
        field("name", $.identifier),
        optional($.type_parameters),
        "(",
        commaSep($.parameter),
        ")",
        optional(seq("->", field("return_type", $._type))),
        optional(field("body", $._expression)),
      ),

    // ── Impl Block ────────────────────────────────────────────────────
    impl_block: ($) =>
      seq(
        repeat($.attribute),
        "impl",
        optional($.type_parameters),
        optional(seq(field("trait", $.named_type), "for")),
        field("type", $._type),
        "{",
        repeat($.impl_method),
        "}",
      ),

    impl_method: ($) =>
      seq(
        repeat($.attribute),
        optional($.visibility),
        "fn",
        field("name", $.identifier),
        optional($.type_parameters),
        "(",
        commaSep($.parameter),
        ")",
        optional(seq("->", field("return_type", $._type))),
        field("body", $._expression),
      ),

    // ── Types ─────────────────────────────────────────────────────────
    _type: ($) =>
      choice(
        $.named_type,
        $.parameterized_type,
        $.tuple_type,
        $.function_type,
        $.self_type,
      ),

    named_type: ($) => choice($.identifier, $.path),

    self_type: (_) => "Self",

    parameterized_type: ($) =>
      prec(1, seq(choice($.identifier, $.path), "<", commaSep1($._type), ">")),

    tuple_type: ($) => seq("(", commaSep($._type), ")"),

    function_type: ($) =>
      prec.right(seq($._type, "->", $._type)),

    // ── Paths ─────────────────────────────────────────────────────────
    path: ($) =>
      prec.left(
        PREC.PATH,
        seq(
          choice($.identifier, $.path, "root", "self", "super"),
          "::",
          $.identifier,
        ),
      ),

    turbofish: ($) => seq("::", "<", commaSep1($._type), ">"),

    // Bare turbofish on identifier/path: `None::<Int>`, `Option::None::<Int>`
    turbofish_expression: ($) =>
      prec(PREC.CALL, seq(
        choice($.identifier, $.path),
        $.turbofish,
      )),

    // ── Expressions ───────────────────────────────────────────────────
    _expression: ($) =>
      choice(
        $.integer,
        $.float,
        $.bigint,
        $.string,
        $.interpolated_string,
        $.boolean,
        $.identifier,
        $.self_expression,
        $.path,
        $.turbofish_expression,
        $.parenthesized_expression,
        $.tuple_expression,
        $.list_expression,
        $.block,
        $.call_expression,
        $.struct_constructor,
        $.field_expression,
        $.index_expression,
        $.method_call,
        $.unary_expression,
        $.binary_expression,
        $.match_expression,
        $.lambda_expression,
      ),

    // ── Literals ──────────────────────────────────────────────────────
    integer: (_) => /[0-9][0-9_]*/,

    float: (_) => /[0-9][0-9_]*\.[0-9][0-9_]*/,

    bigint: (_) => /[0-9][0-9_]*n/,

    string: (_) => token(seq('"', repeat(choice(/[^"\\]/, /\\./)), '"')),

    interpolated_string: ($) =>
      seq(
        $._interpolated_string_start,
        repeat(
          choice(
            $._interpolated_string_content,
            $.interpolation,
          ),
        ),
        $._interpolated_string_end,
      ),

    interpolation: ($) =>
      seq(
        $._interpolated_string_expr_start,
        $._expression,
        $._interpolated_string_expr_end,
      ),

    boolean: (_) => choice("true", "false"),

    self_expression: (_) => "self",

    // ── Parenthesized / Tuple ─────────────────────────────────────────
    parenthesized_expression: ($) =>
      seq("(", $._expression, ")"),

    tuple_expression: ($) =>
      choice(
        seq("(", ")"),
        seq(
          "(",
          choice(
            seq($._expression, ",", commaSep($._expression)),
            seq("..", $._expression, optional(seq(",", commaSep1($._expression)))),
          ),
          ")",
        ),
      ),

    // ── List ──────────────────────────────────────────────────────────
    list_expression: ($) =>
      seq("[", commaSep($._list_element), "]"),

    _list_element: ($) =>
      choice($.spread_element, $._expression),

    spread_element: ($) => seq("..", $._expression),

    // ── Block ─────────────────────────────────────────────────────────
    block: ($) =>
      seq(
        "{",
        repeat($.let_binding),
        $._expression,
        "}",
      ),

    let_binding: ($) =>
      seq(
        "let",
        field("pattern", $._pattern),
        optional(seq(":", field("type", $._type))),
        "=",
        field("value", $._expression),
        ";",
      ),

    // ── Call Expression ───────────────────────────────────────────────
    call_expression: ($) =>
      prec(
        PREC.CALL,
        seq(
          field("function", choice($.identifier, $.path)),
          optional($.turbofish),
          "(",
          commaSep($._expression),
          ")",
        ),
      ),

    // ── Struct Constructor ────────────────────────────────────────────
    // prec.dynamic(-1): when ambiguous with match/block `{`, prefer non-struct parse
    struct_constructor: ($) =>
      prec.dynamic(-1, seq(
        field("name", choice($.identifier, $.path, $.turbofish_expression)),
        "{",
        commaSep(choice($.field_initializer, $.struct_spread)),
        "}",
      )),

    field_initializer: ($) =>
      choice(
        seq(
          field("name", $.identifier),
          ":",
          field("value", $._expression),
        ),
        field("name", $.identifier),
      ),

    struct_spread: ($) => seq("..", $._expression),

    // ── Field Access / Tuple Index ────────────────────────────────────
    field_expression: ($) =>
      prec.left(
        PREC.POSTFIX,
        seq(
          field("object", $._expression),
          ".",
          field("field", choice($.identifier, $.integer)),
        ),
      ),

    // ── Index Expression ──────────────────────────────────────────────
    index_expression: ($) =>
      prec.left(
        PREC.POSTFIX,
        seq(
          field("object", $._expression),
          "[",
          field("index", $._expression),
          "]",
        ),
      ),

    // ── Method Call ───────────────────────────────────────────────────
    method_call: ($) =>
      prec.left(
        PREC.POSTFIX,
        seq(
          field("object", $._expression),
          ".",
          field("method", $.identifier),
          "(",
          commaSep($._expression),
          ")",
        ),
      ),

    // ── Unary Expression ──────────────────────────────────────────────
    unary_expression: ($) =>
      prec(PREC.UNARY, seq("-", field("operand", $._expression))),

    // ── Binary Expression ─────────────────────────────────────────────
    binary_expression: ($) =>
      choice(
        // Comparison (lowest precedence)
        ...[
          ["==", PREC.COMPARE],
          ["!=", PREC.COMPARE],
          ["<", PREC.COMPARE],
          [">", PREC.COMPARE],
          ["<=", PREC.COMPARE],
          [">=", PREC.COMPARE],
        ].map(([op, prec_val]) =>
          prec.left(
            prec_val,
            seq(
              field("left", $._expression),
              field("operator", op),
              field("right", $._expression),
            ),
          ),
        ),
        // Sum
        ...[
          ["+", PREC.SUM],
          ["-", PREC.SUM],
        ].map(([op, prec_val]) =>
          prec.left(
            prec_val,
            seq(
              field("left", $._expression),
              field("operator", op),
              field("right", $._expression),
            ),
          ),
        ),
        // Product
        ...[
          ["*", PREC.PRODUCT],
          ["/", PREC.PRODUCT],
          ["%", PREC.PRODUCT],
        ].map(([op, prec_val]) =>
          prec.left(
            prec_val,
            seq(
              field("left", $._expression),
              field("operator", op),
              field("right", $._expression),
            ),
          ),
        ),
        // Power (right-associative)
        prec.right(
          PREC.POWER,
          seq(
            field("left", $._expression),
            field("operator", "**"),
            field("right", $._expression),
          ),
        ),
      ),

    // ── Match Expression ──────────────────────────────────────────────
    match_expression: ($) =>
      seq(
        "match",
        field("scrutinee", $._expression),
        "{",
        commaSep1($.match_arm),
        "}",
      ),

    match_arm: ($) =>
      seq(
        field("pattern", $._pattern),
        "=>",
        field("body", $._expression),
      ),

    // ── Lambda Expression ─────────────────────────────────────────────
    lambda_expression: ($) =>
      prec.right(
        seq(
          "|",
          commaSep($.lambda_parameter),
          "|",
          optional(seq("->", $._type)),
          field("body", $._expression),
        ),
      ),

    lambda_parameter: ($) =>
      seq(
        field("pattern", $._pattern),
        optional(seq(":", field("type", $._type))),
      ),

    // ── Patterns ──────────────────────────────────────────────────────
    _pattern: ($) =>
      choice(
        $.wildcard_pattern,
        $.literal_pattern,
        $.identifier,
        $.path,
        $.turbofish_expression,
        $.call_pattern,
        $.struct_pattern,
        $.list_pattern,
        $.tuple_pattern,
        $.as_pattern,
      ),

    wildcard_pattern: (_) => "_",

    literal_pattern: ($) =>
      choice(
        seq(optional("-"), $.integer),
        seq(optional("-"), $.float),
        seq(optional("-"), $.bigint),
        $.string,
        $.boolean,
      ),

    call_pattern: ($) =>
      prec(
        PREC.CALL,
        seq(
          field("name", choice($.identifier, $.path)),
          optional($.turbofish),
          "(",
          commaSep(choice($._pattern, $.rest_pattern)),
          ")",
        ),
      ),

    struct_pattern: ($) =>
      seq(
        field("name", choice($.identifier, $.path)),
        "{",
        commaSep(choice($.field_pattern, $.rest_pattern)),
        "}",
      ),

    field_pattern: ($) =>
      choice(
        seq(
          field("name", $.identifier),
          ":",
          field("pattern", $._pattern),
        ),
        field("name", $.identifier),
      ),

    list_pattern: ($) =>
      seq("[", commaSep($._list_pattern_element), "]"),

    _list_pattern_element: ($) =>
      choice($._pattern, $.rest_pattern),

    tuple_pattern: ($) =>
      choice(
        seq("(", ")"),
        seq(
          "(",
          commaSep1(choice($._pattern, $.rest_pattern)),
          ")",
        ),
      ),

    rest_pattern: ($) =>
      choice(
        "..",
        seq($.identifier, "@", ".."),
      ),

    as_pattern: ($) =>
      prec.right(
        seq(field("name", choice($.identifier, $.wildcard_pattern)), "@", field("pattern", $._pattern)),
      ),
  },
});

/**
 * Comma-separated list of one or more elements with optional trailing comma
 */
function commaSep1(rule) {
  return seq(rule, repeat(seq(",", rule)), optional(","));
}

/**
 * Comma-separated list of zero or more elements with optional trailing comma
 */
function commaSep(rule) {
  return optional(commaSep1(rule));
}

/**
 * Separated list of one or more elements
 */
function sepBy1(sep, rule) {
  return seq(rule, repeat(seq(sep, rule)));
}
