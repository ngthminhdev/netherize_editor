; Netherize Editor — Java highlight queries

; --- Comments ---
[
  (line_comment)
  (block_comment)
] @syntax.comment

; --- Strings ---
[
  (string_literal)
  (character_literal)
] @syntax.string
(escape_sequence) @syntax.escape

; --- Numbers ---
[
  (decimal_integer_literal)
  (hex_integer_literal)
  (octal_integer_literal)
  (binary_integer_literal)
  (decimal_floating_point_literal)
  (hex_floating_point_literal)
] @syntax.number

; --- Booleans & null ---
(true) @syntax.boolean
(false) @syntax.boolean
(null_literal) @syntax.constant

; --- Keywords ---
[
  "abstract" "assert" "break" "case" "catch" "class" "continue"
  "default" "do" "else" "enum" "exports" "extends" "final"
  "finally" "for" "if" "implements" "import" "instanceof"
  "interface" "module" "native" "new" "non-sealed" "open"
  "opens" "package" "permits" "private" "protected" "provides"
  "public" "record" "requires" "return" "sealed" "static"
  "strictfp" "switch" "synchronized" "throw" "throws" "to"
  "transient" "transitive" "try" "uses" "volatile" "while"
  "with" "yield"
] @syntax.keyword

; --- Types ---
(type_identifier) @syntax.type

(class_declaration     name: (identifier) @syntax.type)
(interface_declaration name: (identifier) @syntax.type)
(enum_declaration      name: (identifier) @syntax.type)
(record_declaration    name: (identifier) @syntax.type)

; Uppercase identifiers used as type/namespace (e.g. System.out)
((field_access    object: (identifier) @syntax.type) (#match? @syntax.type "^[A-Z]"))
((scoped_identifier scope: (identifier) @syntax.type) (#match? @syntax.type "^[A-Z]"))
((method_invocation object: (identifier) @syntax.type) (#match? @syntax.type "^[A-Z]"))
((method_reference . (identifier) @syntax.type)        (#match? @syntax.type "^[A-Z]"))

[
  (void_type)
  (integral_type)
  (floating_point_type)
  (boolean_type)
] @syntax.type

; --- Functions / Methods ---
(method_declaration      name: (identifier) @syntax.function)
(constructor_declaration name: (identifier) @syntax.constructor)
(method_invocation       name: (identifier) @syntax.function)
(object_creation_expression type: (type_identifier) @syntax.constructor)
(super) @syntax.function

; --- Annotations ---
(annotation        name: (identifier) @syntax.attribute)
(marker_annotation name: (identifier) @syntax.attribute)
"@" @syntax.operator

; --- Constants (UPPER_CASE) ---
((identifier) @syntax.constant (#match? @syntax.constant "^_*[A-Z][A-Z\\d_]+$"))

; --- Parameters — only the name identifier ---
(formal_parameter
  name: (identifier) @syntax.parameter)

(spread_parameter
  (variable_declarator name: (identifier) @syntax.parameter))

(lambda_expression parameters: (identifier) @syntax.parameter)
(inferred_parameters (identifier) @syntax.parameter)

; --- Local variables — only the name identifier ---
(local_variable_declaration
  declarator: (variable_declarator
    name: (identifier) @syntax.variable))

(enhanced_for_statement
  name: (identifier) @syntax.variable)

; --- Class fields — only the name identifier ---
(field_declaration
  declarator: (variable_declarator
    name: (identifier) @syntax.field))

; Field access — only the field part (obj.field → field)
(field_access field: (identifier) @syntax.field)

; --- Operators ---
[
  "+" "-" "*" "/" "%" "=" "==" "!=" "<" ">" "<=" ">="
  "&&" "||" "!" "&" "|" "^" "~" "<<" ">>" ">>>"
  "+=" "-=" "*=" "/=" "%=" "&=" "|=" "^=" "<<=" ">>=" ">>>="
  "++" "--" "->" "::"
] @syntax.operator

; --- Punctuation ---
[
  ";" "," "." ":" "?" "(" ")" "[" "]" "{" "}"
] @syntax.punctuation

; --- Fallback ---
(identifier) @syntax.identifier
