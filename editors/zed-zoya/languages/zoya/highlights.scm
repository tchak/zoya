; ── Keywords ───────────────────────────────────────────────────────────

[
  "fn"
  "let"
  "match"
  "struct"
  "enum"
  "type"
  "mod"
  "use"
  "impl"
] @keyword

(visibility) @keyword.modifier

; ── Literals ──────────────────────────────────────────────────────────

(integer) @number
(float) @number.float
(bigint) @number
(boolean) @boolean

(string) @string
(interpolated_string) @string

; ── Comments ──────────────────────────────────────────────────────────

(line_comment) @comment

; ── Types ─────────────────────────────────────────────────────────────

(self_type) @type.builtin

(named_type (identifier) @type)
(parameterized_type (identifier) @type)
(type_parameters (identifier) @type)

; ── Definitions ───────────────────────────────────────────────────────

(function_definition
  name: (identifier) @function)

(impl_method
  name: (identifier) @function.method)

(struct_definition
  name: (identifier) @type.definition)

(enum_definition
  name: (identifier) @type.definition)

(enum_variant
  name: (identifier) @type.enum.variant)

(type_alias
  name: (identifier) @type.definition)

(mod_declaration
  name: (identifier) @module)

; ── Attributes ────────────────────────────────────────────────────────

(attribute
  name: (identifier) @attribute)
(attribute "#" @attribute)
(attribute "[" @attribute)
(attribute "]" @attribute)

; ── Parameters ────────────────────────────────────────────────────────

(parameter
  pattern: (identifier) @variable.parameter)
(parameter "self" @variable.builtin)

(lambda_parameter
  pattern: (identifier) @variable.parameter)

; ── Function calls ────────────────────────────────────────────────────

(call_expression
  function: (identifier) @function.call)

(call_expression
  function: (path
    (identifier) @function.call .))

(method_call
  method: (identifier) @function.method.call)

; ── Field access ──────────────────────────────────────────────────────

(field_expression
  field: (identifier) @property)

(field_initializer
  name: (identifier) @property)

(struct_field
  name: (identifier) @property)

(field_pattern
  name: (identifier) @property)

; ── Patterns ──────────────────────────────────────────────────────────

(wildcard_pattern) @variable.builtin

(as_pattern
  name: (identifier) @variable)

; ── Paths ─────────────────────────────────────────────────────────────

(path (identifier) @type
  (#match? @type "^[A-Z]"))

(use_path (identifier) @module)
(use_path "*" @operator)
"root" @module
"super" @module

; ── self ──────────────────────────────────────────────────────────────

(self_expression) @variable.builtin
(use_path "self" @variable.builtin)

; ── Let bindings ──────────────────────────────────────────────────────

(let_binding
  pattern: (identifier) @variable)

; ── Operators ─────────────────────────────────────────────────────────

(binary_expression
  operator: _ @operator)

(unary_expression "-" @operator)
(spread_element ".." @operator)
(rest_pattern ".." @operator)
(struct_spread ".." @operator)

[
  "="
  "=>"
  "->"
] @operator

; ── Punctuation ───────────────────────────────────────────────────────

["(" ")" "[" "]" "{" "}"] @punctuation.bracket
["|"] @punctuation.bracket

["," ";" ":" "::" "." "@"] @punctuation.delimiter
