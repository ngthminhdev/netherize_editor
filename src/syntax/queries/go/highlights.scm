; Netherize Editor — Go highlight queries

; --- Comments ---
(comment) @syntax.comment

; --- Strings / escapes ---
[
  (interpreted_string_literal)
  (raw_string_literal)
  (rune_literal)
] @syntax.string

(escape_sequence) @syntax.escape

; --- Numbers / booleans / nil ---
[
  (int_literal)
  (float_literal)
  (imaginary_literal)
] @syntax.number

[
  (true)
  (false)
] @syntax.boolean

(nil) @syntax.constant

; --- Keywords ---
[
  "break" "case" "chan" "const" "continue" "default" "defer" "else"
  "fallthrough" "for" "func" "go" "goto" "if" "import" "interface"
  "map" "package" "range" "return" "select" "struct" "switch" "type"
  "var"
] @syntax.keyword

; --- Package names as namespace ---
(package_clause (package_identifier) @syntax.namespace)

; --- Types ---
(type_identifier) @syntax.type
(type_spec name: (type_identifier) @syntax.type)

; Uppercase identifiers treated as types
((identifier) @syntax.type
 (#match? @syntax.type "^[A-Z]"))

; Composite literal type as constructor
(composite_literal
  type: (type_identifier) @syntax.constructor)

; --- Functions / methods ---
(function_declaration
  name: (identifier) @syntax.function)

(method_declaration
  name: (field_identifier) @syntax.function)

(call_expression
  function: (identifier) @syntax.function)

(call_expression
  function: (selector_expression
    field: (field_identifier) @syntax.function))

; --- Parameters ---
(parameter_declaration
  name: (identifier) @syntax.parameter)

(variadic_parameter_declaration
  name: (identifier) @syntax.parameter)

; --- Variables ---
(var_spec
  name: (identifier) @syntax.variable)

(short_var_declaration
  left: (expression_list
    (identifier) @syntax.variable))

(range_clause
  left: (expression_list
    (identifier) @syntax.variable))

; --- Fields / properties ---
(field_declaration
  name: (field_identifier) @syntax.field)

(selector_expression
  field: (field_identifier) @syntax.field)

(keyed_element
  key: (literal_element
    (identifier) @syntax.property))

; --- Constants ---
(const_spec
  name: (identifier) @syntax.constant)

((identifier) @syntax.constant
 (#match? @syntax.constant "^_*[A-Z][A-Z\\d_]+$"))

; --- Operators ---
[
  "+" "-" "*" "/" "%" "=" "==" "!=" "<" ">" "<=" ">="
  ":=" "&&" "||" "!" "&" "|" "^" "&^" "<<" ">>" "+=" "-="
  "*=" "/=" "%=" "&=" "|=" "^=" "<<=" ">>=" "&^=" "++" "--"
  "<-" ":"
] @syntax.operator

; --- Punctuation ---
[
  ";" "," "." "(" ")" "[" "]" "{" "}"
] @syntax.punctuation

; --- Fallback ---
(identifier) @syntax.identifier
(field_identifier) @syntax.identifier
