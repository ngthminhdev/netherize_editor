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
(escape_sequence) @syntax.string.escape

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
  "assert" "break" "case" "catch" "continue" "default" "do" "else"
  "finally" "for" "if" "return" "switch" "throw" "throws" "try" "while" "yield"
] @syntax.keyword.control

[
  "abstract" "class" "enum" "final" "interface" "module" "native" "non-sealed"
  "open" "private" "protected" "public" "record" "sealed" "static" "strictfp"
  "synchronized" "transient" "volatile"
] @syntax.keyword.storage

[
  "exports" "extends" "implements" "import" "instanceof" "new" "opens"
  "package" "permits" "provides" "requires" "to" "transitive" "uses" "with"
] @syntax.keyword

; --- Types ---
(type_identifier) @syntax.type

(class_declaration     name: (identifier) @syntax.type)
(interface_declaration name: (identifier) @syntax.type)
(enum_declaration      name: (identifier) @syntax.type)
(record_declaration    name: (identifier) @syntax.type)

; Package / import paths
(package_declaration
  (scoped_identifier
    scope: (identifier) @syntax.namespace
    name: (identifier) @syntax.namespace))

(import_declaration
  (scoped_identifier
    scope: (identifier) @syntax.namespace
    name: (identifier) @syntax.namespace))

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
] @syntax.type.builtin

; --- Functions / Methods ---
(method_declaration      name: (identifier) @syntax.function)
(constructor_declaration name: (identifier) @syntax.constructor)
(method_invocation       name: (identifier) @syntax.function)
(object_creation_expression type: (type_identifier) @syntax.constructor)
(super) @syntax.variable.builtin
(this) @syntax.variable.builtin

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

; Field access — distinguish class-qualified access from object properties
((field_access
  object: (identifier) @syntax.type
  field: (identifier) @syntax.field)
 (#match? @syntax.type "^[A-Z]"))

(field_access field: (identifier) @syntax.property)

; Scoped identifiers outside imports/packages often represent namespace/type chains
((scoped_identifier
  scope: (identifier) @syntax.namespace
  name: (identifier) @syntax.type)
 (#match? @syntax.type "^[A-Z]"))

((scoped_identifier
  scope: (identifier) @syntax.namespace
  name: (identifier) @syntax.namespace)
 (#not-match? @syntax.namespace "^[A-Z]"))

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
