; ── Scopes ────────────────────────────────────────────────────────────

(function_definition) @local.scope
(impl_method) @local.scope
(lambda_expression) @local.scope
(block) @local.scope
(match_arm) @local.scope

; ── Definitions ───────────────────────────────────────────────────────

(parameter
  pattern: (identifier) @local.definition)

(lambda_parameter
  pattern: (identifier) @local.definition)

(let_binding
  pattern: (identifier) @local.definition)

(as_pattern
  name: (identifier) @local.definition)

; ── References ────────────────────────────────────────────────────────

(identifier) @local.reference
